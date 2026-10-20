//! The native `.housecraft` format (see `docs/formats/housecraft.md`): a zip archive holding
//!
//! - `manifest.json`: format marker, format version, generator
//! - `document.json`: the [`Document`]
//!
//! Assets (textures, imported symbols) will live under `assets/` later.
//! Older format versions are upgraded in [`migrate`] before deserializing.
//! Files from early builds used the `.hcraft` extension; they are identical inside.

pub mod dxf;
pub mod library;
pub mod pdf;
pub mod project;
pub mod symbols;

use std::io::{Read, Seek, Write};
use std::path::Path;

use housecraft_model::Document;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const EXTENSION: &str = "housecraft";
/// Extension used by early builds; still opened.
pub const LEGACY_EXTENSION: &str = "hcraft";
/// MIME type registered with desktop environments.
pub const MIME_TYPE: &str = "application/x-housecraft";
pub const FORMAT: &str = "housecraft";
pub const FORMAT_VERSION: u32 = 1;

const MANIFEST: &str = "manifest.json";
const DOCUMENT: &str = "document.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub generator: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a HouseCraft file: {0}")]
    NotHouseCraft(String),
    #[error("file format version {found} is newer than this build supports ({supported})")]
    TooNew { found: u32, supported: u32 },
    #[error("corrupt file: {0}")]
    Corrupt(String),
    #[error("document is inconsistent: {0}")]
    Invalid(String),
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        match e {
            zip::result::ZipError::Io(io) => Error::Io(io),
            other => Error::NotHouseCraft(other.to_string()),
        }
    }
}

pub fn save<W: Write + Seek>(doc: &Document, writer: W) -> Result<(), Error> {
    let manifest = Manifest {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        generator: concat!("housecraft ", env!("CARGO_PKG_VERSION")).into(),
    };
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(writer);
    zip.start_file(MANIFEST, options)?;
    serde_json::to_writer_pretty(&mut zip, &manifest).map_err(std::io::Error::from)?;
    zip.start_file(DOCUMENT, options)?;
    serde_json::to_writer_pretty(&mut zip, doc).map_err(std::io::Error::from)?;
    zip.finish()?;
    Ok(())
}

pub fn load<R: Read + Seek>(reader: R) -> Result<Document, Error> {
    let mut zip = ZipArchive::new(reader)?;
    let manifest: Manifest = read_json(&mut zip, MANIFEST)?;
    if manifest.format != FORMAT {
        return Err(Error::NotHouseCraft(format!(
            "format is {:?}",
            manifest.format
        )));
    }
    if manifest.format_version > FORMAT_VERSION {
        return Err(Error::TooNew {
            found: manifest.format_version,
            supported: FORMAT_VERSION,
        });
    }
    let raw: serde_json::Value = read_json(&mut zip, DOCUMENT)?;
    let doc: Document = serde_json::from_value(migrate(raw, manifest.format_version)?)
        .map_err(|e| Error::Corrupt(format!("{DOCUMENT}: {e}")))?;
    let issues = doc.validate();
    if !issues.is_empty() {
        let list: Vec<String> = issues.iter().map(ToString::to_string).collect();
        return Err(Error::Invalid(list.join("; ")));
    }
    Ok(doc)
}

/// Save via a temporary file and rename, so a crash never leaves a half-written plan.
pub fn save_path(doc: &Document, path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    let tmp = path.with_extension(format!("{EXTENSION}.tmp"));
    let result = std::fs::File::create(&tmp)
        .map_err(Error::from)
        .and_then(|f| save(doc, std::io::BufWriter::new(f)));
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Make a Copy: save the plan to `path` named after the file (a revision
/// such as "Chic Cottage-Roof"), leaving `doc` itself untouched.
pub fn save_copy(doc: &Document, path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    let mut copy = doc.clone();
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        copy.name = stem.to_string();
    }
    save_path(&copy, path)
}

/// Where Make a Copy suggests saving: beside `original` (or in the current
/// folder), as "{name}-Copy", numbered so it doesn't overwrite a file.
pub fn copy_path(original: Option<&Path>, name: &str) -> std::path::PathBuf {
    let dir = original.and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default();
    let stem = original.and_then(Path::file_stem).and_then(|s| s.to_str()).unwrap_or(name);
    let mut n = 1;
    loop {
        let suffix = if n == 1 { "-Copy".to_string() } else { format!("-Copy {n}") };
        let candidate = dir.join(format!("{stem}{suffix}.{EXTENSION}"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

pub fn load_path(path: impl AsRef<Path>) -> Result<Document, Error> {
    load(std::io::BufReader::new(std::fs::File::open(path)?))
}

/// Upgrade a document written by format version `from` to the current version.
fn migrate(doc: serde_json::Value, from: u32) -> Result<serde_json::Value, Error> {
    match from {
        FORMAT_VERSION => Ok(doc),
        other => Err(Error::Corrupt(format!("unknown format version {other}"))),
    }
}

fn read_json<R: Read + Seek, T: serde::de::DeserializeOwned>(
    zip: &mut ZipArchive<R>,
    name: &str,
) -> Result<T, Error> {
    let entry = zip
        .by_name(name)
        .map_err(|_| Error::NotHouseCraft(format!("missing {name}")))?;
    serde_json::from_reader(entry).map_err(|e| Error::Corrupt(format!("{name}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trips() {
        let doc = Document::new("Round trip");
        let mut buf = Cursor::new(Vec::new());
        save(&doc, &mut buf).unwrap();
        buf.set_position(0);
        assert_eq!(load(buf).unwrap(), doc);
    }

    #[test]
    fn make_a_copy_leaves_the_original_and_numbers_names() {
        let dir = std::env::temp_dir().join(format!("housecraft-copy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let original = dir.join(format!("Cottage.{EXTENSION}"));
        let doc = Document::new("Cottage");
        save_path(&doc, &original).unwrap();
        let first = copy_path(Some(&original), &doc.name);
        assert_eq!(first, dir.join(format!("Cottage-Copy.{EXTENSION}")));
        save_copy(&doc, &first).unwrap();
        assert_eq!(doc.name, "Cottage", "the open plan keeps its name");
        assert_eq!(load_path(&first).unwrap().name, "Cottage-Copy");
        assert_eq!(copy_path(Some(&original), &doc.name), dir.join(format!("Cottage-Copy 2.{EXTENSION}")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_non_zip() {
        let err = load(Cursor::new(b"hello".to_vec())).unwrap_err();
        assert!(matches!(err, Error::NotHouseCraft(_)), "{err}");
    }

    #[test]
    fn rejects_newer_versions() {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut buf);
            zip.start_file(MANIFEST, SimpleFileOptions::default())
                .unwrap();
            zip.write_all(br#"{"format":"housecraft","format_version":999,"generator":"x"}"#)
                .unwrap();
            zip.finish().unwrap();
        }
        buf.set_position(0);
        assert!(matches!(
            load(buf).unwrap_err(),
            Error::TooNew { found: 999, .. }
        ));
    }

    #[test]
    fn save_path_writes_atomically() {
        let dir = std::env::temp_dir().join(format!("housecraft-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.housecraft");
        let doc = Document::new("On disk");
        save_path(&doc, &path).unwrap();
        assert_eq!(load_path(&path).unwrap(), doc);
        assert!(!path.with_extension("housecraft.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
