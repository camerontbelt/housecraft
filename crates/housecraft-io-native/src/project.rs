//! Plans with their files: the images and textures a plan refers to,
//! gathered next to it (a managed project) or packed with it into one
//! `.hcproj` archive that opens anywhere.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use housecraft_model::Document;
use serde_json::Value;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::Error;

/// Extension of a single-file project archive.
pub const ARCHIVE_EXTENSION: &str = "hcproj";

/// Keys whose string values name files.
const FILE_KEYS: [&str; 2] = ["path", "image"];

fn walk(v: &mut Value, f: &mut dyn FnMut(&mut String)) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                match x {
                    Value::String(s) if FILE_KEYS.contains(&k.as_str()) && !s.is_empty() => f(s),
                    _ => walk(x, f),
                }
            }
        }
        Value::Array(a) => {
            for x in a {
                walk(x, f);
            }
        }
        _ => {}
    }
}

/// Every file path the plan refers to (pictures, texture maps, plant
/// images, panoramas), each once.
pub fn asset_paths(doc: &Document) -> Vec<String> {
    let mut v = serde_json::to_value(doc).expect("documents serialize");
    let mut out = Vec::new();
    walk(&mut v, &mut |s| {
        if !out.contains(s) {
            out.push(s.clone());
        }
    });
    out
}

/// The plan with its file paths changed by `map` (others unchanged).
pub fn rewrite_paths(doc: &Document, map: &BTreeMap<String, String>) -> Result<Document, Error> {
    let mut v = serde_json::to_value(doc).expect("documents serialize");
    walk(&mut v, &mut |s| {
        if let Some(n) = map.get(s.as_str()) {
            *s = n.clone();
        }
    });
    serde_json::from_value(v).map_err(|e| Error::Corrupt(e.to_string()))
}

/// Where a referenced file is: absolute, or relative to the plan's folder.
pub fn resolve(path: &str, plan_dir: Option<&Path>) -> PathBuf {
    let p = PathBuf::from(path);
    match plan_dir {
        Some(d) if p.is_relative() => d.join(p),
        _ => p,
    }
}

/// A file name for `src` unused in `taken`.
fn unique_name(src: &Path, taken: &mut Vec<String>) -> String {
    let stem = src.file_stem().map_or("file".into(), |s| s.to_string_lossy().into_owned());
    let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut name = format!("{stem}{ext}");
    let mut k = 2;
    while taken.iter().any(|t| t.eq_ignore_ascii_case(&name)) {
        name = format!("{stem} ({k}){ext}");
        k += 1;
    }
    taken.push(name.clone());
    name
}

/// What gathering or packing did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gathered {
    /// Files copied in.
    pub copied: usize,
    /// Referenced files that could not be found.
    pub missing: Vec<String>,
}

/// Managed project: copy every file the plan uses into `<plan> files/`
/// beside `plan_path`, and point the plan at those copies (relative paths,
/// so the folder can move as a whole). Files already there are left.
pub fn gather(doc: &Document, plan_path: &Path) -> Result<(Document, Gathered), Error> {
    let dir = plan_path.parent().unwrap_or(Path::new("."));
    let stem = plan_path.file_stem().map_or("plan".into(), |s| s.to_string_lossy().into_owned());
    let folder_name = format!("{stem} files");
    let folder = dir.join(&folder_name);
    let mut map = BTreeMap::new();
    let mut report = Gathered::default();
    let mut taken: Vec<String> = std::fs::read_dir(&folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    for p in asset_paths(doc) {
        if p.starts_with(&format!("{folder_name}/")) || p.starts_with(&format!("{folder_name}\\")) {
            continue;
        }
        let src = resolve(&p, Some(dir));
        if !src.is_file() {
            report.missing.push(p);
            continue;
        }
        std::fs::create_dir_all(&folder)?;
        let name = unique_name(&src, &mut taken);
        std::fs::copy(&src, folder.join(&name))?;
        map.insert(p, format!("{folder_name}/{name}"));
        report.copied += 1;
    }
    Ok((rewrite_paths(doc, &map)?, report))
}

/// Pack the plan and every file it uses into one `.hcproj` archive.
/// `plan_dir` is where relative paths are found.
pub fn pack(doc: &Document, plan_dir: Option<&Path>, out: &Path) -> Result<Gathered, Error> {
    let mut map = BTreeMap::new();
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let mut taken = Vec::new();
    let mut report = Gathered::default();
    for p in asset_paths(doc) {
        let src = resolve(&p, plan_dir);
        if !src.is_file() {
            report.missing.push(p);
            continue;
        }
        let name = unique_name(&src, &mut taken);
        map.insert(p, format!("assets/{name}"));
        files.push((name, src));
    }
    let packed = rewrite_paths(doc, &map)?;
    let mut plan = std::io::Cursor::new(Vec::new());
    crate::save(&packed, &mut plan)?;
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(std::fs::File::create(out)?);
    zip.start_file("plan.housecraft", options)?;
    zip.write_all(plan.get_ref())?;
    for (name, src) in &files {
        zip.start_file(format!("assets/{name}"), options)?;
        zip.write_all(&std::fs::read(src)?)?;
        report.copied += 1;
    }
    zip.finish()?;
    Ok(report)
}

/// Unpack a `.hcproj` archive into a new folder in `dest` (named after the
/// archive) and return the plan file's path there.
pub fn unpack(archive: &Path, dest: &Path) -> Result<PathBuf, Error> {
    let stem = archive.file_stem().map_or("project".into(), |s| s.to_string_lossy().into_owned());
    let mut folder = dest.join(&stem);
    let mut k = 2;
    while folder.exists() {
        folder = dest.join(format!("{stem} ({k})"));
        k += 1;
    }
    std::fs::create_dir_all(folder.join("assets"))?;
    let mut zip = ZipArchive::new(std::fs::File::open(archive)?)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name()?.to_string();
        // Only the plan and flat asset names: nothing can escape the folder.
        let target = if name == "plan.housecraft" {
            folder.join(format!("{stem}.housecraft"))
        } else if let Some(file) = name.strip_prefix("assets/").filter(|f| !f.is_empty() && !f.contains(['/', '\\']) && !f.contains("..")) {
            folder.join("assets").join(file)
        } else {
            continue;
        };
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        std::fs::write(target, bytes)?;
    }
    let plan = folder.join(format!("{stem}.housecraft"));
    if !plan.is_file() {
        return Err(Error::NotHouseCraft("the archive has no plan".into()));
    }
    Ok(plan)
}

/// Copy a library, templates and the master price list from another
/// HouseCraft data folder (an older install) into this one; returns how
/// many files were copied. Files already here are kept.
pub fn migrate_data_folder(from: &Path, to: &Path) -> Result<usize, Error> {
    fn copy_tree(src: &Path, dst: &Path, n: &mut usize) -> std::io::Result<()> {
        for e in std::fs::read_dir(src)?.flatten() {
            let p = e.path();
            let target = dst.join(e.file_name());
            if p.is_dir() {
                std::fs::create_dir_all(&target)?;
                copy_tree(&p, &target, n)?;
            } else if !target.exists() {
                std::fs::copy(&p, &target)?;
                *n += 1;
            }
        }
        Ok(())
    }
    let mut n = 0;
    for sub in ["Library", "Templates"] {
        let src = from.join(sub);
        if src.is_dir() {
            let dst = to.join(sub);
            std::fs::create_dir_all(&dst)?;
            copy_tree(&src, &dst, &mut n)?;
        }
    }
    let prices = from.join("master-prices.json");
    if prices.is_file() && !to.join("master-prices.json").exists() {
        std::fs::create_dir_all(to)?;
        std::fs::copy(prices, to.join("master-prices.json"))?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_model::*;

    fn plan_with_picture(dir: &Path) -> Document {
        let mut doc = Document::new("p");
        let img = dir.join("logo.png");
        std::fs::write(&img, b"not really a png").unwrap();
        let floor = doc.floors_by_elevation()[0].id;
        let layer = doc.active_defaults().layers.cad;
        let v = serde_json::json!({"kind": "picture", "id": 0, "floor": floor.0, "layer": layer.0, "path": img.display().to_string(),
            "position": [0, 0], "width": 1000.0, "height": 500.0});
        doc.insert(serde_json::from_value::<Object>(v).unwrap());
        doc
    }

    #[test]
    fn gather_pack_and_unpack() {
        let root = std::env::temp_dir().join(format!("hc-project-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let doc = plan_with_picture(&root);
        assert_eq!(asset_paths(&doc).len(), 1);
        // Managed project: the picture is copied beside the plan.
        let plan_path = root.join("House.housecraft");
        let (managed, report) = gather(&doc, &plan_path).unwrap();
        assert_eq!(report.copied, 1);
        let p = asset_paths(&managed)[0].clone();
        assert_eq!(p, "House files/logo.png");
        assert!(root.join("House files").join("logo.png").is_file());
        // Archive and unpack elsewhere: the picture comes along.
        crate::save_path(&managed, &plan_path).unwrap();
        let archive = root.join("House.hcproj");
        let r = pack(&managed, Some(&root), &archive).unwrap();
        assert_eq!((r.copied, r.missing.len()), (1, 0));
        let out = root.join("unpacked");
        std::fs::create_dir_all(&out).unwrap();
        let plan = unpack(&archive, &out).unwrap();
        let back = crate::load_path(&plan).unwrap();
        let ap = asset_paths(&back)[0].clone();
        assert_eq!(ap, "assets/logo.png");
        assert!(resolve(&ap, plan.parent()).is_file());
        let _ = std::fs::remove_dir_all(&root);
    }
}
