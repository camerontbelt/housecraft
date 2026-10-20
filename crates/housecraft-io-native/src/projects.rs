//! Projects: a folder on disk holding a building's plans (revisions, options)
//! in sub-folders of your own, as listed in the Project Browser. The folder is
//! the project; nothing else records it, so files moved in Explorer or
//! Finder show up where they land.

use std::path::{Path, PathBuf};

use crate::{EXTENSION, Error};

/// The automatic archive copies of a plan go in a folder named after it
/// with this ending; the Project Browser leaves those folders out.
pub const ARCHIVE_SUFFIX: &str = " Archive";

/// A folder in a project: its documents and sub-folders, sorted by name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Folder {
    pub path: PathBuf,
    pub name: String,
    pub folders: Vec<Folder>,
    pub documents: Vec<PathBuf>,
}

impl Folder {
    /// Every document in this folder and below, in listing order.
    pub fn all_documents(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for f in &self.folders {
            out.extend(f.all_documents());
        }
        out.extend(self.documents.iter().cloned());
        out
    }
}

/// Whether `path` is a plan the Project Browser lists.
pub fn is_document(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION) || e.eq_ignore_ascii_case("hcraft"))
}

/// List `root` and its sub-folders (at most `depth` levels down). Hidden
/// folders, archive folders and folders holding no plans at any depth are
/// left out, except empty folders made for organising (no files at all).
pub fn scan(root: &Path, depth: usize) -> Result<Folder, Error> {
    let name = root.file_name().and_then(|n| n.to_str()).unwrap_or("Project").to_string();
    let mut folder = Folder { path: root.to_path_buf(), name, ..Folder::default() };
    let mut entries: Vec<_> = std::fs::read_dir(root)?.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for e in entries {
        let path = e.path();
        let hidden = e.file_name().to_string_lossy().starts_with('.');
        if path.is_dir() {
            let archive = e.file_name().to_string_lossy().ends_with(ARCHIVE_SUFFIX);
            if hidden || archive || depth == 0 {
                continue;
            }
            let sub = scan(&path, depth - 1)?;
            let empty = std::fs::read_dir(&path).map(|mut d| d.next().is_none()).unwrap_or(false);
            if empty || !sub.all_documents().is_empty() {
                folder.folders.push(sub);
            }
        } else if is_document(&path) && !hidden {
            folder.documents.push(path);
        }
    }
    Ok(folder)
}

/// Make a new folder named `name` inside `parent` ("New Folder", then
/// "New Folder 2"… when the name is taken). Returns its path.
pub fn new_folder(parent: &Path, name: &str) -> Result<PathBuf, Error> {
    let name = clean_name(name)?;
    let mut n = 1;
    loop {
        let candidate = if n == 1 { parent.join(&name) } else { parent.join(format!("{name} {n}")) };
        if !candidate.exists() {
            std::fs::create_dir(&candidate)?;
            return Ok(candidate);
        }
        n += 1;
    }
}

/// Move documents (or folders) into `folder`, keeping their names. Nothing
/// is overwritten: a clash fails before anything moves. Returns the new paths
/// in the same order.
pub fn move_into(items: &[PathBuf], folder: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut targets = Vec::new();
    for item in items {
        let name = item.file_name().ok_or_else(|| Error::Invalid(format!("{} has no name", item.display())))?;
        let target = folder.join(name);
        if item.is_dir() && folder.starts_with(item) {
            return Err(Error::Invalid(format!("{} can't go inside itself", item.display())));
        }
        if target != *item && target.exists() {
            return Err(Error::Invalid(format!("{} is already in {}", name.to_string_lossy(), folder.display())));
        }
        targets.push(target);
    }
    for (item, target) in items.iter().zip(&targets) {
        if item != target {
            std::fs::rename(item, target)?;
        }
    }
    Ok(targets)
}

/// Rename a document or folder in place (a document keeps its extension).
pub fn rename(item: &Path, name: &str) -> Result<PathBuf, Error> {
    let name = clean_name(name)?;
    let file = match item.extension().and_then(|e| e.to_str()).filter(|_| item.is_file()) {
        Some(ext) => format!("{name}.{ext}"),
        None => name,
    };
    let target = item.with_file_name(file);
    if target != item && target.exists() {
        return Err(Error::Invalid(format!("{} already exists", target.display())));
    }
    std::fs::rename(item, &target)?;
    Ok(target)
}

fn clean_name(name: &str) -> Result<String, Error> {
    let name = name.trim();
    if name.is_empty() || name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')) || name == "." || name == ".." {
        return Err(Error::Invalid(format!("{name:?} can't be used as a name")));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_model::Document;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("housecraft-projects-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn plan(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(format!("{name}.{EXTENSION}"));
        crate::save_path(&Document::new(name), &p).unwrap();
        p
    }

    #[test]
    fn organises_plans_into_folders() {
        let root = temp("organise");
        let current = plan(&root, "CHIC COTTAGE-CURRENT");
        let shell = plan(&root, "Chic Cottage-Shell");
        let rooms = plan(&root, "Chic Cottage-Rooms");
        std::fs::write(root.join("notes.txt"), "not a plan").unwrap();
        std::fs::create_dir(root.join("pictures")).unwrap();
        std::fs::write(root.join("pictures").join("logo.png"), "png").unwrap();
        let archive = root.join(format!("CHIC COTTAGE-CURRENT{ARCHIVE_SUFFIX}"));
        std::fs::create_dir(&archive).unwrap();
        plan(&archive, "CHIC COTTAGE-CURRENT 20261020 101500");

        let tree = scan(&root, 4).unwrap();
        assert_eq!(tree.documents, vec![current.clone(), rooms.clone(), shell.clone()]);
        assert!(tree.folders.is_empty(), "folders of pictures and archive copies aren't listed: {:?}", tree.folders);

        let folder = new_folder(&root, "Exterior Walls Tutorials").unwrap();
        assert_eq!(new_folder(&root, "Exterior Walls Tutorials").unwrap(), root.join("Exterior Walls Tutorials 2"));
        assert_eq!(scan(&root, 4).unwrap().folders.len(), 2, "new empty folders show so you can fill them");

        let moved = move_into(&[shell.clone(), rooms.clone()], &folder).unwrap();
        assert!(moved.iter().all(|p| p.exists() && p.parent() == Some(folder.as_path())));
        let tree = scan(&root, 4).unwrap();
        assert_eq!(tree.documents, vec![current.clone()]);
        let sub = tree.folders.iter().find(|f| f.name == "Exterior Walls Tutorials").unwrap();
        assert_eq!(sub.documents.len(), 2);
        assert_eq!(tree.all_documents().len(), 3);

        // No overwriting, no moving a folder into itself, no bad names.
        let again = plan(&root, "Chic Cottage-Shell");
        assert!(move_into(&[again], &folder).is_err());
        assert!(move_into(std::slice::from_ref(&folder), &folder.join("x")).is_err());
        assert!(new_folder(&root, "a/b").is_err());

        let renamed = rename(&moved[0], "Chic Cottage-Walls").unwrap();
        assert_eq!(renamed, folder.join(format!("Chic Cottage-Walls.{EXTENSION}")));
        let _ = std::fs::remove_dir_all(&root);
    }
}
