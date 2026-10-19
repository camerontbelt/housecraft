//! The user's library on disk: CAD blocks saved as JSON files
//! (`<library>/blocks/<name>.json`) to reuse in any plan.

use std::path::{Path, PathBuf};

use housecraft_model::DetailShape;
use serde::{Deserialize, Serialize};

use crate::Error;

/// A block saved in the library: shapes relative to its insertion point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryBlock {
    pub name: String,
    pub shapes: Vec<DetailShape>,
}

/// The library folder: `$HOUSECRAFT_LIBRARY`, or `HouseCraft/Library` under
/// the user's application-data folder (`%APPDATA%`, `$XDG_DATA_HOME` or
/// `~/.local/share`; `~/Library/Application Support` on macOS).
pub fn library_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("HOUSECRAFT_LIBRARY") {
        return Some(PathBuf::from(p));
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }?;
    Some(base.join("HouseCraft").join("Library"))
}

/// A file name for `name` with characters that are unsafe in paths replaced.
fn file_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' })
        .collect();
    let stem = stem.trim().trim_start_matches('.').to_string();
    if stem.is_empty() { "block".into() } else { stem }
}

/// Save `block` into `dir`'s blocks folder, replacing one of the same name.
pub fn save_block(dir: &Path, block: &LibraryBlock) -> Result<PathBuf, Error> {
    let folder = dir.join("blocks");
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(format!("{}.json", file_stem(&block.name)));
    let json = serde_json::to_vec_pretty(block).map_err(|e| Error::Corrupt(e.to_string()))?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// The blocks saved in `dir`, by name; unreadable files are skipped.
pub fn list_blocks(dir: &Path) -> Vec<(PathBuf, LibraryBlock)> {
    let Ok(entries) = std::fs::read_dir(dir.join("blocks")) else {
        return Vec::new();
    };
    let mut blocks: Vec<(PathBuf, LibraryBlock)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| {
            let block = load_block(&p).ok()?;
            Some((p, block))
        })
        .collect();
    blocks.sort_by_key(|b| b.1.name.to_lowercase());
    blocks
}

pub fn load_block(path: &Path) -> Result<LibraryBlock, Error> {
    serde_json::from_slice(&std::fs::read(path)?).map_err(|e| Error::Corrupt(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_model::Shape;
    use housecraft_units::{Length, Point2};

    #[test]
    fn save_and_list() {
        let dir = std::env::temp_dir().join(format!("hc-lib-{}", std::process::id()));
        let block = LibraryBlock {
            name: "Toilet / WC".into(),
            shapes: vec![DetailShape {
                shape: Shape::Circle {
                    center: Point2(Length::ZERO, Length::ZERO),
                    radius: Length::from_inches(8.0),
                },
                color: None,
                fill: None,
            }],
        };
        let path = save_block(&dir, &block).unwrap();
        assert_eq!(path.file_name().unwrap(), "Toilet _ WC.json");
        let listed = list_blocks(&dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].1, block);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
