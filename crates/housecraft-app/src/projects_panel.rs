//! The Projects part of the Project Browser: the plans in the project folder
//! (the open plan's folder unless you choose another), in folders of your
//! own. Double-click a plan to open it; drag plans onto a folder to file
//! them; right-click for New Folder, Rename and Move To.

use std::path::{Path, PathBuf};

use egui::{Sense, Ui};
use housecraft_io_native::projects::{self, Folder};

use crate::app::{App, Pending};

/// How deep sub-folders are listed.
const DEPTH: usize = 6;
/// Seconds between re-reading the folder (files can change outside the app).
const RESCAN: f64 = 2.0;

#[derive(Default)]
pub struct ProjectsPanel {
    /// The listing, the folder it is of, and when it was read.
    tree: Option<(PathBuf, Folder, f64)>,
    /// Selected plans and folders.
    selected: Vec<PathBuf>,
    /// Where a Shift+click range starts.
    anchor: Option<PathBuf>,
    /// A name being typed: a new folder in a parent, or a rename.
    prompt: Option<Prompt>,
}

enum Prompt {
    NewFolder { parent: PathBuf, name: String },
    Rename { item: PathBuf, name: String },
}

/// What a click in the listing asks for (applied after drawing it).
enum Action {
    Click(PathBuf, egui::Modifiers),
    Open(PathBuf),
    Move(Vec<PathBuf>, PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    Reveal(PathBuf),
}

impl App {
    /// The project folder: the one chosen, or the open plan's.
    pub fn project_root(&self) -> Option<PathBuf> {
        if let Some(r) = self.prefs.project_root.as_ref().map(PathBuf::from).filter(|p| p.is_dir()) {
            return Some(r);
        }
        self.path.as_deref().and_then(Path::parent).filter(|d| !d.as_os_str().is_empty() && d.is_dir()).map(Path::to_path_buf)
    }

    pub fn projects_section(&mut self, ui: &mut Ui) {
        let now = ui.input(|i| i.time);
        let root = self.project_root();
        let stale = match (&self.projects.tree, &root) {
            (Some((r, _, at)), Some(root)) => r != root || now - at > RESCAN,
            (None, Some(_)) => true,
            (_, None) => false,
        };
        if stale && let Some(root) = &root {
            match projects::scan(root, DEPTH) {
                Ok(tree) => self.projects.tree = Some((root.clone(), tree, now)),
                Err(e) => {
                    self.projects.tree = None;
                    self.error(format!("Can't read {}: {e}", root.display()));
                }
            }
        }
        if root.is_none() {
            self.projects.tree = None;
        }
        egui::CollapsingHeader::new("Projects").id_salt("projects-section").default_open(true).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.small_button("Choose Folder…").on_hover_text("Use another folder as the project").clicked()
                    && let Some(dir) = pick_folder()
                {
                    self.prefs.project_root = Some(dir.display().to_string());
                    self.projects.tree = None;
                }
                if self.prefs.project_root.is_some()
                    && ui.small_button("This Plan's Folder").on_hover_text("List the open plan's folder again").clicked()
                {
                    self.prefs.project_root = None;
                    self.projects.tree = None;
                }
            });
            let Some((_, tree, _)) = self.projects.tree.clone() else {
                ui.weak("Save the plan to see its project folder here, or choose a folder.");
                return;
            };
            let order = flat_order(&tree);
            let mut actions = Vec::new();
            self.folder_ui(ui, &tree, true, &mut actions);
            for a in actions {
                self.project_action(a, &order);
            }
        });
        self.project_prompt(ui.ctx());
    }

    fn folder_ui(&self, ui: &mut Ui, folder: &Folder, root: bool, actions: &mut Vec<Action>) {
        let selected = self.projects.selected.contains(&folder.path);
        let title = if root { format!("{} (project)", folder.name) } else { folder.name.clone() };
        let header = egui::CollapsingHeader::new(egui::RichText::new(title).strong().color(if selected {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().text_color()
        }))
        .id_salt(("project-folder", &folder.path))
        .default_open(true)
        .show(ui, |ui| {
            for sub in &folder.folders {
                self.folder_ui(ui, sub, false, actions);
            }
            for doc in &folder.documents {
                self.document_ui(ui, doc, actions);
            }
            if folder.folders.is_empty() && folder.documents.is_empty() {
                ui.weak("Empty: drag plans here");
            }
        });
        let h = header.header_response.interact(Sense::click_and_drag());
        if let Some(items) = h.dnd_release_payload::<Vec<PathBuf>>() {
            actions.push(Action::Move(items.to_vec(), folder.path.clone()));
        } else if h.dnd_hover_payload::<Vec<PathBuf>>().is_some() {
            ui.painter().rect_stroke(h.rect, 2.0, ui.visuals().selection.stroke, egui::StrokeKind::Outside);
        }
        if !root && h.dragged() {
            h.dnd_set_drag_payload(vec![folder.path.clone()]);
        }
        h.context_menu(|ui| {
            if ui.button("New Folder…").clicked() {
                actions.push(Action::NewFolder(folder.path.clone()));
                ui.close();
            }
            if !root && ui.button("Rename…").clicked() {
                actions.push(Action::Rename(folder.path.clone()));
                ui.close();
            }
            if ui.button("Show in File Manager").clicked() {
                actions.push(Action::Reveal(folder.path.clone()));
                ui.close();
            }
        });
    }

    fn document_ui(&self, ui: &mut Ui, doc: &Path, actions: &mut Vec<Action>) {
        let name = doc.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        let open = self.path.as_deref() == Some(doc);
        let label = if open { format!("{name}  (open)") } else { name.to_string() };
        let selected = self.projects.selected.iter().any(|p| p == doc);
        let r = ui.selectable_label(selected, label).interact(Sense::click_and_drag());
        let r = r.on_hover_text("Double-click to open; drag onto a folder to move");
        if r.double_clicked() && !open {
            actions.push(Action::Open(doc.to_path_buf()));
        } else if r.clicked() {
            actions.push(Action::Click(doc.to_path_buf(), ui.input(|i| i.modifiers)));
        }
        if r.drag_started() && !selected {
            actions.push(Action::Click(doc.to_path_buf(), egui::Modifiers::NONE));
        }
        if r.dragged() {
            let items = if selected { self.projects.selected.clone() } else { vec![doc.to_path_buf()] };
            r.dnd_set_drag_payload(items);
        }
        r.context_menu(|ui| {
            if !open && ui.button("Open").clicked() {
                actions.push(Action::Open(doc.to_path_buf()));
                ui.close();
            }
            if ui.button("Rename…").clicked() {
                actions.push(Action::Rename(doc.to_path_buf()));
                ui.close();
            }
            if let Some((_, tree, _)) = &self.projects.tree {
                ui.menu_button("Move To", |ui| {
                    let mut folders = Vec::new();
                    all_folders(tree, 0, &mut folders);
                    for (f, depth) in folders {
                        if doc.parent() == Some(f.as_path()) {
                            continue;
                        }
                        let name = f.file_name().and_then(|s| s.to_str()).unwrap_or("?");
                        if ui.button(format!("{}{name}", "   ".repeat(depth))).clicked() {
                            let items = if selected { self.projects.selected.clone() } else { vec![doc.to_path_buf()] };
                            actions.push(Action::Move(items, f));
                            ui.close();
                        }
                    }
                });
            }
        });
    }

    fn project_action(&mut self, action: Action, order: &[PathBuf]) {
        match action {
            Action::Click(p, m) => {
                let sel = &mut self.projects.selected;
                if m.shift
                    && let Some(a) = &self.projects.anchor
                    && let (Some(i), Some(j)) = (order.iter().position(|x| x == a), order.iter().position(|x| *x == p))
                {
                    let (lo, hi) = (i.min(j), i.max(j));
                    *sel = order[lo..=hi].to_vec();
                } else if m.command || m.ctrl {
                    if let Some(i) = sel.iter().position(|x| *x == p) {
                        sel.remove(i);
                    } else {
                        sel.push(p.clone());
                    }
                    self.projects.anchor = Some(p);
                } else {
                    *sel = vec![p.clone()];
                    self.projects.anchor = Some(p);
                }
            }
            Action::Open(p) => self.guarded(Pending::OpenPath(p)),
            Action::Move(items, folder) => {
                let items: Vec<PathBuf> = items.into_iter().filter(|i| i.parent() != Some(folder.as_path())).collect();
                if items.is_empty() {
                    return;
                }
                match projects::move_into(&items, &folder) {
                    Ok(moved) => {
                        for (from, to) in items.iter().zip(&moved) {
                            self.moved_on_disk(from, to);
                        }
                        self.projects.selected = moved;
                        self.projects.tree = None;
                        self.info(format!("Moved {} item(s) to {}", items.len(), folder.display()));
                    }
                    Err(e) => self.error(format!("Couldn't move: {e}")),
                }
            }
            Action::NewFolder(parent) => {
                self.projects.prompt = Some(Prompt::NewFolder { parent, name: "New Folder".into() });
            }
            Action::Rename(item) => {
                let name = item.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
                self.projects.prompt = Some(Prompt::Rename { item, name });
            }
            Action::Reveal(p) => crate::app::reveal(&p),
        }
    }

    /// Keep the open plan's path and the recent list pointing at a file (or
    /// a folder holding it) that moved.
    fn moved_on_disk(&mut self, from: &Path, to: &Path) {
        let remap = |p: &Path| p.strip_prefix(from).ok().map(|rest| if rest.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest) });
        if let Some(new) = self.path.as_deref().and_then(remap) {
            self.path = Some(new);
        }
        for r in &mut self.prefs.recent {
            if let Some(new) = remap(Path::new(r.as_str())) {
                *r = new.display().to_string();
            }
        }
    }

    fn project_prompt(&mut self, ctx: &egui::Context) {
        let Some(prompt) = &mut self.projects.prompt else {
            return;
        };
        let title = match prompt {
            Prompt::NewFolder { .. } => "New Folder",
            Prompt::Rename { .. } => "Rename",
        };
        let mut done = None;
        let mut open = true;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                let name = match prompt {
                    Prompt::NewFolder { name, .. } | Prompt::Rename { name, .. } => name,
                };
                ui.text_edit_singleline(name).request_focus();
                let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
                if escape {
                    done = Some(false);
                }
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || enter {
                        done = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        done = Some(false);
                    }
                });
            });
        if !open {
            done = Some(false);
        }
        let Some(ok) = done else {
            return;
        };
        let prompt = self.projects.prompt.take().expect("open prompt");
        if !ok {
            return;
        }
        match prompt {
            Prompt::NewFolder { parent, name } => match projects::new_folder(&parent, &name) {
                Ok(p) => {
                    self.projects.selected = vec![p];
                    self.projects.tree = None;
                }
                Err(e) => self.error(format!("Couldn't make the folder: {e}")),
            },
            Prompt::Rename { item, name } => match projects::rename(&item, &name) {
                Ok(p) => {
                    self.moved_on_disk(&item, &p);
                    self.projects.selected = vec![p];
                    self.projects.tree = None;
                }
                Err(e) => self.error(format!("Couldn't rename: {e}")),
            },
        }
    }
}

/// Plans and folders in the order they're listed (for Shift+click ranges).
fn flat_order(f: &Folder) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for sub in &f.folders {
        out.push(sub.path.clone());
        out.extend(flat_order(sub));
    }
    out.extend(f.documents.iter().cloned());
    out
}

fn all_folders(f: &Folder, depth: usize, out: &mut Vec<(PathBuf, usize)>) {
    out.push((f.path.clone(), depth));
    for sub in &f.folders {
        all_folders(sub, depth + 1, out);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().set_title("Choose Project Folder").pick_folder()
}

#[cfg(target_arch = "wasm32")]
fn pick_folder() -> Option<PathBuf> {
    None
}
