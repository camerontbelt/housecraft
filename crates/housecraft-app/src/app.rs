//! Application state, menus, toolbars, side windows and file handling.
//!
//! The layout follows the arrangement residential CAD users know: menus, a
//! two-row toolbar (file/view row, then build tools), a Tool Palette of child
//! tools on the left, side windows on the right with a vertical view toolbar,
//! an Edit toolbar under the drawing for the selection, and a status bar.

use std::path::PathBuf;

use egui::{RichText, Ui};
use housecraft_api::model::*;
use housecraft_api::{Command, Outcome, Session};
use serde::{Deserialize, Serialize};

use crate::canvas::PlanState;
use crate::dialogs::Dialogs;
use crate::icons::{self, Icon};
use crate::snap::SnapSettings;
use crate::tools::{CameraAction, Family, Tool};
use crate::util::EditBuffers;
use crate::view3d::View3d;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: Theme,
    pub snap: SnapSettings,
    pub show_grid: bool,
    pub recent: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: Theme::System,
            snap: SnapSettings::default(),
            show_grid: true,
            recent: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Plan,
    ThreeD,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideTab {
    ProjectBrowser,
    LayerDisplay,
    Library,
}

/// Something waiting on "save unsaved changes?".
#[derive(Debug, Clone)]
pub enum Pending {
    New,
    Open,
    OpenPath(PathBuf),
    Quit,
}

pub struct App {
    pub session: Session,
    pub path: Option<PathBuf>,
    /// Bumped on every document change; caches compare against it.
    pub rev: u64,
    /// Selected objects, in the order they were picked.
    pub selection: Vec<Id>,
    pub mode: Mode,
    pub palette: Family,
    pub side_tab: SideTab,
    pub plan: PlanState,
    pub view3d: View3d,
    pub prefs: Prefs,
    pub dialogs: Dialogs,
    pub bufs: EditBuffers,
    /// Objects whose specification windows are open.
    pub spec: Vec<Id>,
    /// Rooms being edited together (multi-room specification).
    pub room_multi: Option<Vec<Id>>,
    pub clipboard: Vec<serde_json::Value>,
    pub status: Option<(String, bool)>,
    pub pending: Option<Pending>,
    quit_confirmed: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        let prefs: Prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, "prefs"))
            .unwrap_or_default();
        if let Some(rs) = cc.wgpu_render_state.as_ref() {
            crate::view3d::init_renderer(rs);
        }
        let doc = Document::new("Untitled");
        let mut app = App {
            plan: PlanState::new(&doc),
            session: Session::new(doc),
            path: None,
            rev: 1,
            selection: Vec::new(),
            mode: Mode::Plan,
            palette: Family::Walls,
            side_tab: SideTab::ProjectBrowser,
            view3d: View3d::default(),
            prefs,
            dialogs: Dialogs::default(),
            bufs: EditBuffers::default(),
            spec: Vec::new(),
            room_multi: None,
            clipboard: Vec::new(),
            status: None,
            pending: None,
            quit_confirmed: false,
        };
        app.apply_theme(&cc.egui_ctx);
        if let Some(p) = path {
            app.open_path(p);
        }
        app
    }

    pub fn doc(&self) -> &Document {
        self.session.document()
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
        ctx.set_theme(match self.prefs.theme {
            Theme::System => egui::ThemePreference::System,
            Theme::Light => egui::ThemePreference::Light,
            Theme::Dark => egui::ThemePreference::Dark,
        });
    }

    pub fn info(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), false));
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), true));
    }

    /// Apply commands as one undoable transaction. Returns the outcome, or
    /// shows the error in the status bar and returns `None`.
    pub fn run(&mut self, commands: Vec<Command>) -> Option<Outcome> {
        if commands.is_empty() {
            return None;
        }
        match self.session.apply(&commands) {
            Ok(out) => {
                self.changed();
                self.status = None;
                Some(out)
            }
            Err(e) => {
                self.error(e.to_string());
                None
            }
        }
    }

    pub fn run1(&mut self, command: Command) -> Option<Outcome> {
        self.run(vec![command])
    }

    /// Note a document change: invalidate caches and drop stale references.
    pub fn changed(&mut self) {
        self.rev += 1;
        let doc = self.session.document();
        self.selection.retain(|id| doc.object(*id).is_some());
        self.spec.retain(|id| doc.object(*id).is_some());
        if doc.get::<Floor>(self.plan.floor).is_none() {
            self.plan.floor = doc.floors_by_elevation()[0].id;
        }
        if doc.get::<LayerSet>(self.plan.layer_set).is_none() {
            self.plan.layer_set = doc.iter::<LayerSet>().next().map_or(Id(0), |s| s.id);
        }
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.plan.set_tool(tool);
        self.palette = tool.family();
        self.mode = Mode::Plan;
    }

    pub fn undo(&mut self) {
        if self.session.undo() {
            self.changed();
            self.info("Undone");
        }
    }

    pub fn redo(&mut self) {
        if self.session.redo() {
            self.changed();
            self.info("Redone");
        }
    }

    pub fn delete_selection(&mut self) {
        let ids: Vec<Id> = self.selection.clone();
        if !ids.is_empty() && self.run1(Command::Delete { ids }).is_some() {
            self.selection.clear();
        }
    }

    /// Open specification windows for the selection (Open Object).
    pub fn open_object(&mut self) {
        let doc = self.doc();
        if self.selection.len() > 1
            && self
                .selection
                .iter()
                .all(|id| doc.kind_of(*id) == Some(ObjectKind::Room))
        {
            self.room_multi = Some(self.selection.clone());
            return;
        }
        for id in self.selection.clone() {
            if !self.spec.contains(&id) {
                self.spec.push(id);
            }
        }
    }

    /// Cycle the selection among objects under the last click (Select Next Object).
    pub fn select_next(&mut self) {
        if let Some(next) = self.plan.next_candidate(self.selection.first().copied()) {
            self.selection = vec![next];
        }
    }

    pub fn select_same_type(&mut self) {
        let Some(kind) = self
            .selection
            .first()
            .and_then(|id| self.doc().kind_of(*id))
        else {
            return;
        };
        let floor = self.plan.floor;
        let doc = self.doc();
        self.selection = doc
            .objects
            .values()
            .filter(|o| o.kind() == kind && doc.floor_of(o.id()).is_none_or(|f| f == floor))
            .map(Object::id)
            .collect();
    }

    pub fn copy(&mut self) {
        let doc = self.doc();
        let mut ids: Vec<Id> = self.selection.clone();
        // Bring doors and windows along with their walls.
        for id in self.selection.iter() {
            if doc.kind_of(*id) == Some(ObjectKind::Wall) {
                ids.extend(doc.openings_in(*id).map(|o| o.id));
            }
        }
        ids.sort();
        ids.dedup();
        self.clipboard = ids
            .iter()
            .filter(|id| doc.kind_of(**id).is_some_and(|k| k.is_placed()))
            .filter_map(|id| housecraft_api::query::get_object(doc, *id))
            .collect();
        let n = self.clipboard.len();
        self.info(format!(
            "Copied {n} object{}",
            if n == 1 { "" } else { "s" }
        ));
    }

    pub fn cut(&mut self) {
        self.copy();
        self.delete_selection();
    }

    /// Paste the clipboard. In place keeps positions; otherwise it lands under
    /// the cursor (or offset a little if the cursor is not over the plan).
    pub fn paste(&mut self, in_place: bool) {
        if self.clipboard.is_empty() {
            return;
        }
        let (dx, dy) = if in_place {
            (0.0, 0.0)
        } else {
            let pts: Vec<housecraft_api::geom::DVec2> = self
                .clipboard
                .iter()
                .filter_map(|v| serde_json::from_value::<Object>(v.clone()).ok())
                .filter_map(|o| match o {
                    Object::Wall(w) => Some(housecraft_api::geom::v2(w.start)),
                    Object::Text(t) => Some(housecraft_api::geom::v2(t.position)),
                    Object::Cad(c) => housecraft_api::geom::math::shape_snap_points(&c.shape)
                        .first()
                        .map(|p| p.0),
                    _ => None,
                })
                .collect();
            match (self.plan.cursor, housecraft_api::geom::math::bounds(pts)) {
                (Some(c), Some((lo, hi))) => {
                    let center = (lo + hi) / 2.0;
                    (c.x - center.x, c.y - center.y)
                }
                _ => (304.8, -304.8),
            }
        };
        if let Some(out) = self.run1(Command::InsertObjects {
            objects: self.clipboard.clone(),
            floor: Some(self.plan.floor),
            dx: housecraft_api::units::Length::from_mm(dx),
            dy: housecraft_api::units::Length::from_mm(dy),
        }) {
            let doc = self.doc();
            self.selection = out
                .created
                .into_iter()
                .filter(|id| doc.kind_of(*id).is_some_and(|k| k != ObjectKind::Opening))
                .collect();
        }
    }

    pub fn select_all(&mut self) {
        let floor = self.plan.floor;
        let doc = self.doc();
        self.selection = doc
            .objects
            .keys()
            .copied()
            .filter(|id| {
                doc.floor_of(*id) == Some(floor)
                    && doc
                        .kind_of(*id)
                        .is_some_and(|k| k.is_placed() && k != ObjectKind::Opening)
            })
            .collect();
    }

    pub fn change_floor(&mut self, up: bool) {
        let floors: Vec<Id> = self
            .doc()
            .floors_by_elevation()
            .iter()
            .map(|f| f.id)
            .collect();
        let i = floors
            .iter()
            .position(|f| *f == self.plan.floor)
            .unwrap_or(0);
        let j = if up {
            (i + 1).min(floors.len() - 1)
        } else {
            i.saturating_sub(1)
        };
        if up && j == i {
            self.info("Already on the top floor (Build > Floor > Build New Floor adds one)");
        }
        self.plan.floor = floors[j];
        self.selection.clear();
    }

    pub fn camera_action(&mut self, action: CameraAction) {
        self.mode = Mode::ThreeD;
        self.view3d.preset_for(action);
    }

    fn replace_document(&mut self, doc: Document, path: Option<PathBuf>) {
        self.plan = PlanState::new(&doc);
        self.session = Session::new(doc);
        self.path = path;
        self.selection.clear();
        self.spec.clear();
        self.view3d = View3d::default();
        self.changed();
    }

    pub fn new_document(&mut self) {
        self.replace_document(Document::new("Untitled"), None);
        self.info("New plan");
    }

    pub fn open_path(&mut self, path: PathBuf) {
        match housecraft_io_native::load_path(&path) {
            Ok(doc) => {
                let shown = path.display().to_string();
                self.prefs.recent.retain(|p| p != &shown);
                self.prefs.recent.insert(0, shown.clone());
                self.prefs.recent.truncate(8);
                self.replace_document(doc, Some(path));
                self.plan.fit_requested = true;
                self.info(format!("Opened {shown}"));
            }
            Err(e) => self.error(format!("Could not open {}: {e}", path.display())),
        }
    }

    pub fn save(&mut self, save_as: bool) -> bool {
        let path = match (&self.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => pick_save_path(&self.doc().name, housecraft_io_native::EXTENSION),
        };
        let Some(path) = path else {
            return false;
        };
        match housecraft_io_native::save_path(self.doc(), &path) {
            Ok(()) => {
                self.session.mark_saved();
                self.info(format!("Saved {}", path.display()));
                self.path = Some(path);
                true
            }
            Err(e) => {
                self.error(format!("Save failed: {e}"));
                false
            }
        }
    }

    /// Do `action`, first asking about unsaved changes if there are any.
    pub fn guarded(&mut self, action: Pending) {
        if self.session.is_dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action);
        }
    }

    fn perform(&mut self, action: Pending) {
        match action {
            Pending::New => self.new_document(),
            Pending::Open => {
                if let Some(p) = pick_open_path() {
                    self.open_path(p);
                }
            }
            Pending::OpenPath(p) => self.open_path(p),
            Pending::Quit => self.quit_confirmed = true,
        }
    }

    pub fn export(&mut self, ext: &str) {
        let doc = self.doc();
        let Some(path) = pick_save_path(&doc.name, ext) else {
            return;
        };
        let opts = housecraft_draft::PlanOptions {
            floor: self.plan.floor,
            layer_set: self.plan.layer_set,
            reference_floor: false,
            default_set: None,
        };
        let drawing = housecraft_draft::plan(doc, &opts);
        let imperial = crate::util::input_unit(doc) == housecraft_api::units::LengthUnit::Inches;
        let bytes = if ext == "pdf" {
            let floor = doc
                .get::<Floor>(self.plan.floor)
                .map_or("", |f| f.name.as_str());
            housecraft_draft::pdf::to_pdf(
                &drawing,
                &housecraft_draft::pdf::PdfOptions {
                    paper: self.dialogs.print.paper,
                    landscape: self.dialogs.print.landscape,
                    scale: self.dialogs.print.scale(),
                    title: format!("{} - {floor}", doc.name),
                    imperial,
                },
            )
            .0
        } else {
            housecraft_draft::svg::to_svg(
                &drawing,
                &housecraft_draft::svg::SvgOptions {
                    scale: if imperial { 48.0 } else { 50.0 },
                    ..Default::default()
                },
            )
            .into_bytes()
        };
        match std::fs::write(&path, bytes) {
            Ok(()) => self.info(format!("Exported {}", path.display())),
            Err(e) => self.error(format!("Export failed: {e}")),
        }
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        let has_sel = !self.selection.is_empty();
        let two = self.selection.len() == 2;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if item(ui, "New Plan", "Ctrl+N", true) {
                    self.guarded(Pending::New);
                }
                if item(ui, "Open Plan…", "Ctrl+O", true) {
                    self.guarded(Pending::Open);
                }
                ui.menu_button("Recent Files", |ui| {
                    if self.prefs.recent.is_empty() {
                        ui.weak("(none)");
                    }
                    for p in self.prefs.recent.clone() {
                        if ui.button(&p).clicked() {
                            self.guarded(Pending::OpenPath(PathBuf::from(&p)));
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if item(ui, "Save", "Ctrl+S", true) {
                    self.save(false);
                }
                if item(ui, "Save As…", "", true) {
                    self.save(true);
                }
                ui.separator();
                ui.menu_button("Export", |ui| {
                    if item(ui, "Export Floor Plan as PDF…", "", true) {
                        self.dialogs.print.open = true;
                    }
                    if item(ui, "Export Floor Plan as SVG…", "", true) {
                        self.export("svg");
                    }
                });
                if item(ui, "Print…", "Ctrl+P", true) {
                    self.dialogs.print.open = true;
                }
                ui.separator();
                if item(ui, "Exit", "", true) {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                if item(ui, "Undo", "Ctrl+Z", self.session.can_undo()) {
                    self.undo();
                }
                if item(ui, "Redo", "Ctrl+Y", self.session.can_redo()) {
                    self.redo();
                }
                ui.separator();
                if item(ui, "Cut", "Ctrl+X", has_sel) {
                    self.cut();
                }
                if item(ui, "Copy", "Ctrl+C", has_sel) {
                    self.copy();
                }
                if item(ui, "Paste", "Ctrl+V", !self.clipboard.is_empty()) {
                    self.paste(false);
                }
                if item(
                    ui,
                    "Paste Hold Position",
                    "Ctrl+Alt+V",
                    !self.clipboard.is_empty(),
                ) {
                    self.paste(true);
                }
                if item(ui, "Delete", "Del", has_sel) {
                    self.delete_selection();
                }
                ui.separator();
                if item(ui, "Select All", "Ctrl+A", true) {
                    self.select_all();
                }
                if item(ui, "Select Same Type", "Ctrl+J", has_sel) {
                    self.select_same_type();
                }
                if item(ui, "Open Object", "Ctrl+E", has_sel) {
                    self.open_object();
                }
                ui.separator();
                ui.menu_button("Edit Objects", |ui| {
                    if item(ui, "Move / Copy / Rotate / Reflect…", "", has_sel) {
                        self.dialogs.transform.open = true;
                    }
                    if item(ui, "Multiple Copy…", "", has_sel) {
                        self.dialogs.transform.open_multiple();
                    }
                    if item(ui, "Offset…", "", has_sel) {
                        self.dialogs.cad_op.open(crate::dialogs::CadOp::Offset);
                    }
                    if item(ui, "Fillet…", "", two) {
                        self.dialogs.cad_op.open(crate::dialogs::CadOp::Fillet);
                    }
                    if item(ui, "Chamfer…", "", two) {
                        self.dialogs.cad_op.open(crate::dialogs::CadOp::Chamfer);
                    }
                    if item(ui, "Trim / Extend to Second Object", "", two) {
                        let (id, boundary) = (self.selection[0], self.selection[1]);
                        self.run1(Command::ExtendTo { id, boundary });
                    }
                });
                ui.separator();
                if item(ui, "Default Settings…", "", true) {
                    self.dialogs.defaults = true;
                }
                if item(ui, "Preferences…", "~", true) {
                    self.dialogs.prefs = true;
                }
            });
            ui.menu_button("Build", |ui| {
                ui.menu_button("Wall", |ui| {
                    for tool in Family::Walls.tools() {
                        if tool_item(ui, *tool) {
                            self.set_tool(*tool);
                        }
                    }
                    ui.separator();
                    if item(ui, "Wall Types…", "", true) {
                        self.dialogs.wall_types.open = true;
                    }
                });
                ui.menu_button("Door", |ui| {
                    for tool in Family::Doors.tools() {
                        if tool_item(ui, *tool) {
                            self.set_tool(*tool);
                        }
                    }
                });
                ui.menu_button("Window", |ui| {
                    for tool in Family::Windows.tools() {
                        if tool_item(ui, *tool) {
                            self.set_tool(*tool);
                        }
                    }
                });
                if item(ui, "Build Foundation...", "Ctrl+F", true) {
                    self.dialogs.foundation.open = true;
                }
                ui.menu_button("Floor", |ui| {
                    if item(ui, "Build New Floor", "Shift+X", true)
                        && let Some(out) = self.run1(Command::AddFloor {
                            name: None,
                            elevation: None,
                            ceiling_height: None,
                            floor_structure: None,
                            below: None,
                        })
                    {
                        self.plan.floor = out.created[0];
                    }
                    if item(ui, "Insert Floor Below Current", "", true)
                        && let Some(out) = self.run1(Command::AddFloor {
                            name: None,
                            elevation: None,
                            ceiling_height: None,
                            floor_structure: None,
                            below: Some(self.plan.floor),
                        })
                    {
                        self.plan.floor = out.created[0];
                    }
                    let above = {
                        let floors = self.doc().floors_by_elevation();
                        let i = floors.iter().position(|f| f.id == self.plan.floor);
                        i.and_then(|i| floors.get(i + 1)).map(|f| f.id)
                    };
                    if item(ui, "Exchange with Floor Above", "", above.is_some())
                        && let Some(b) = above
                    {
                        self.run1(Command::SwapFloors {
                            a: self.plan.floor,
                            b,
                        });
                    }
                    if item(ui, "Up One Floor", "Shift+N", true) {
                        self.change_floor(true);
                    }
                    if item(ui, "Down One Floor", "Shift+M", true) {
                        self.change_floor(false);
                    }
                    if item(ui, "Reference Floor Display", "F9", true) {
                        self.plan.reference_floor = !self.plan.reference_floor;
                    }
                });
                ui.separator();
                for f in [
                    Family::Cabinets,
                    Family::Stairs,
                    Family::Roofs,
                    Family::Electrical,
                ] {
                    item(ui, f.name(), "coming soon", false);
                }
            });
            ui.menu_button("Terrain", |ui| {
                ui.weak("Terrain tools are planned for a later release (backlog M6).");
            });
            ui.menu_button("Library", |ui| {
                if item(ui, "Library Browser", "", true) {
                    self.side_tab = SideTab::Library;
                }
            });
            ui.menu_button("3D", |ui| {
                ui.menu_button("Create Camera View", |ui| {
                    for a in CameraAction::ALL {
                        if item(ui, a.name(), a.hotkey(), true) {
                            self.camera_action(*a);
                        }
                    }
                });
                if item(ui, "Save Current 3D View as Camera", "", true) {
                    let n = self.doc().count::<Camera>() + 1;
                    let cmd = self
                        .view3d
                        .save_command(format!("Camera {n}"), self.plan.layer_set);
                    self.run1(cmd);
                }
                ui.separator();
                if item(ui, "Back to Plan View", "F2", self.mode == Mode::ThreeD) {
                    self.mode = Mode::Plan;
                }
            });
            ui.menu_button("CAD", |ui| {
                for tool in Family::Cad.tools() {
                    if tool_item(ui, *tool) {
                        self.set_tool(*tool);
                    }
                }
                ui.separator();
                ui.menu_button("Dimensions", |ui| {
                    for tool in Family::Dimensions.tools() {
                        if tool_item(ui, *tool) {
                            self.set_tool(*tool);
                        }
                    }
                    if item(ui, "Auto Exterior Dimensions", "Shift+A", true) {
                        self.auto_exterior_dimensions();
                    }
                });
                if tool_item(ui, Tool::Text) {
                    self.set_tool(Tool::Text);
                }
            });
            ui.menu_button("Tools", |ui| {
                ui.menu_button("Snap Settings", |ui| {
                    ui.checkbox(&mut self.prefs.snap.objects, "Object Snaps (Shift+F11)");
                    ui.checkbox(&mut self.prefs.snap.angles, "Angle Snaps (F10)");
                    ui.checkbox(&mut self.prefs.snap.grid, "Grid Snaps (Ctrl+F9)");
                });
                ui.menu_button("Layer", |ui| {
                    if item(ui, "Layer Display Options", "", true) {
                        self.side_tab = SideTab::LayerDisplay;
                    }
                    if item(ui, "Layers and Layer Sets…", "", true) {
                        self.dialogs.layers = true;
                    }
                });
                if item(ui, "Wall Types…", "", true) {
                    self.dialogs.wall_types.open = true;
                }
                if item(ui, "Text Styles…", "", true) {
                    self.dialogs.text_styles = true;
                }
                if item(ui, "Room Types…", "", true) {
                    self.dialogs.room_types = true;
                }
            });
            ui.menu_button("View", |ui| {
                if item(ui, "Fill Window", "F6", true) {
                    self.plan.fit_requested = true;
                    self.view3d.fit_requested = true;
                }
                if item(ui, "Zoom to Selection", "", has_sel) {
                    self.plan.zoom_to_selection = true;
                }
                ui.separator();
                ui.checkbox(&mut self.prefs.show_grid, "Reference Grid");
                ui.checkbox(
                    &mut self.plan.reference_floor,
                    "Reference Floor Display (F9)",
                );
                ui.separator();
                ui.menu_button("Color Scheme", |ui| {
                    for (t, name) in [
                        (Theme::System, "Follow System"),
                        (Theme::Light, "Light"),
                        (Theme::Dark, "Dark"),
                    ] {
                        if ui.radio(self.prefs.theme == t, name).clicked() {
                            self.prefs.theme = t;
                            self.apply_theme(ui.ctx());
                            ui.close();
                        }
                    }
                });
            });
            ui.menu_button("Window", |ui| {
                if item(ui, "Plan View", "F2", true) {
                    self.mode = Mode::Plan;
                }
                if item(ui, "3D View", "F3", true) {
                    self.mode = Mode::ThreeD;
                }
                ui.separator();
                if item(ui, "Project Browser", "", true) {
                    self.side_tab = SideTab::ProjectBrowser;
                }
                if item(ui, "Active Layer Display Options", "", true) {
                    self.side_tab = SideTab::LayerDisplay;
                }
                if item(ui, "Library Browser", "", true) {
                    self.side_tab = SideTab::Library;
                }
            });
            ui.menu_button("Help", |ui| {
                if item(ui, "Keyboard Shortcuts", "", true) {
                    self.dialogs.shortcuts = true;
                }
                if item(ui, "About HouseCraft", "", true) {
                    self.dialogs.about = true;
                }
            });
        });
    }

    pub fn auto_exterior_dimensions(&mut self) {
        self.run1(Command::AddExteriorDimensions {
            floor: Some(self.plan.floor),
        });
    }

    /// Row 1: file, edit, view and settings buttons, plus the plan view picker.
    fn toolbar_main(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if icons::button(ui, Icon::New, false, true, "New Plan (Ctrl+N)").clicked() {
                self.guarded(Pending::New);
            }
            if icons::button(ui, Icon::Open, false, true, "Open Plan (Ctrl+O)").clicked() {
                self.guarded(Pending::Open);
            }
            if icons::button(ui, Icon::Save, false, true, "Save (Ctrl+S)").clicked() {
                self.save(false);
            }
            if icons::button(ui, Icon::Print, false, true, "Print / Export PDF (Ctrl+P)").clicked()
            {
                self.dialogs.print.open = true;
            }
            ui.separator();
            if icons::button(
                ui,
                Icon::Undo,
                false,
                self.session.can_undo(),
                "Undo (Ctrl+Z)",
            )
            .clicked()
            {
                self.undo();
            }
            if icons::button(
                ui,
                Icon::Redo,
                false,
                self.session.can_redo(),
                "Redo (Ctrl+Y)",
            )
            .clicked()
            {
                self.redo();
            }
            ui.separator();
            if icons::button(ui, Icon::Help, false, true, "Keyboard shortcuts").clicked() {
                self.dialogs.shortcuts = true;
            }
            ui.separator();
            // Plan view picker.
            let views: Vec<PlanView> = self.doc().iter::<PlanView>().cloned().collect();
            let current = self
                .plan
                .view
                .and_then(|v| views.iter().find(|x| x.id == v))
                .map_or("Plan View".to_string(), |v| v.name.clone());
            egui::ComboBox::from_id_salt("plan_view_picker")
                .selected_text(current)
                .width(150.0)
                .show_ui(ui, |ui| {
                    for v in &views {
                        if ui
                            .selectable_label(self.plan.view == Some(v.id), &v.name)
                            .clicked()
                        {
                            self.apply_plan_view(v);
                        }
                    }
                    ui.separator();
                    if ui.button("Save Current View…").clicked() {
                        self.dialogs.save_view = Some("New View".into());
                    }
                });
            ui.separator();
            if icons::button(ui, Icon::Defaults, false, true, "Default Settings").clicked() {
                self.dialogs.defaults = true;
            }
            if icons::button(ui, Icon::Preferences, false, true, "Preferences (~)").clicked() {
                self.dialogs.prefs = true;
            }
            ui.separator();
            if icons::button(
                ui,
                Icon::Plan,
                self.mode == Mode::Plan,
                true,
                "Plan View (F2)",
            )
            .clicked()
            {
                self.mode = Mode::Plan;
            }
            if icons::button(
                ui,
                Icon::Overview,
                self.mode == Mode::ThreeD,
                true,
                "3D View (F3)",
            )
            .clicked()
            {
                self.mode = Mode::ThreeD;
            }
        });
    }

    /// Row 2: build tool families (parent buttons), then cameras and floors.
    fn toolbar_build(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            for family in Family::TOOLBAR {
                if *family == Family::Cameras {
                    ui.separator();
                    for a in CameraAction::ALL {
                        let tip = format!("{} ({})", a.name(), a.hotkey());
                        if icons::button(ui, a.icon(), false, true, &tip).clicked() {
                            self.camera_action(*a);
                        }
                    }
                    continue;
                }
                let active = self.plan.tool.family() == *family && self.mode == Mode::Plan;
                let tip = if family.available() {
                    family.name().to_string()
                } else {
                    format!("{} (coming soon)", family.name())
                };
                let resp = icons::button(ui, family.icon(), active, family.available(), &tip);
                if resp.clicked() {
                    self.palette = *family;
                    if let Some(t) = family.default_tool() {
                        self.set_tool(t);
                    }
                }
                if matches!(family, Family::Select | Family::Text) {
                    ui.separator();
                }
            }
            ui.separator();
            if icons::button(ui, Icon::FloorUp, false, true, "Up One Floor (Shift+N)").clicked() {
                self.change_floor(true);
            }
            if icons::button(ui, Icon::FloorDown, false, true, "Down One Floor (Shift+M)").clicked()
            {
                self.change_floor(false);
            }
            if icons::button(
                ui,
                Icon::Reference,
                self.plan.reference_floor,
                true,
                "Reference Floor Display (F9)",
            )
            .clicked()
            {
                self.plan.reference_floor = !self.plan.reference_floor;
            }
        });
    }

    /// The Tool Palette: child tools of the active family.
    fn tool_palette(&mut self, ui: &mut Ui) {
        ui.strong(self.palette.name());
        ui.separator();
        if self.palette == Family::Cameras {
            for a in CameraAction::ALL {
                if palette_row(ui, a.icon(), a.name(), false).clicked() {
                    self.camera_action(*a);
                }
            }
            return;
        }
        for tool in self.palette.tools() {
            let resp = palette_row(ui, tool.icon(), tool.name(), self.plan.tool == *tool);
            if resp.on_hover_text(tool.tooltip()).clicked() {
                self.set_tool(*tool);
            }
        }
        if self.palette == Family::Dimensions
            && palette_row(ui, Icon::AutoDimension, "Auto Exterior Dimensions", false)
                .on_hover_text("Auto Exterior Dimensions (Shift+A)")
                .clicked()
        {
            self.auto_exterior_dimensions();
        }
        if self.palette == Family::Walls
            && palette_row(ui, Icon::Defaults, "Wall Types…", false).clicked()
        {
            self.dialogs.wall_types.open = true;
        }
        if !self.palette.available() {
            ui.weak("These tools are planned for a later milestone.");
        }
    }

    /// The vertical view toolbar beside the side windows.
    fn view_toolbar(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            if icons::button(ui, Icon::ZoomIn, false, true, "Zoom In").clicked() {
                self.plan.zoom_by(0.7);
                self.view3d.zoom_by(0.8);
            }
            if icons::button(ui, Icon::ZoomOut, false, true, "Zoom Out").clicked() {
                self.plan.zoom_by(1.0 / 0.7);
                self.view3d.zoom_by(1.25);
            }
            if icons::button(ui, Icon::FillWindow, false, true, "Fill Window (F6)").clicked() {
                self.plan.fit_requested = true;
                self.view3d.fit_requested = true;
            }
            if icons::button(
                ui,
                Icon::ZoomSelection,
                false,
                !self.selection.is_empty(),
                "Zoom to Selection",
            )
            .clicked()
            {
                self.plan.zoom_to_selection = true;
            }
            ui.separator();
            if icons::button(ui, Icon::FloorUp, false, true, "Up One Floor (Shift+N)").clicked() {
                self.change_floor(true);
            }
            if icons::button(ui, Icon::FloorDown, false, true, "Down One Floor (Shift+M)").clicked()
            {
                self.change_floor(false);
            }
            ui.separator();
            if icons::button(
                ui,
                Icon::Layers,
                self.side_tab == SideTab::LayerDisplay,
                true,
                "Active Layer Display Options",
            )
            .clicked()
            {
                self.side_tab = SideTab::LayerDisplay;
            }
            if icons::button(
                ui,
                Icon::Library,
                self.side_tab == SideTab::Library,
                true,
                "Library Browser",
            )
            .clicked()
            {
                self.side_tab = SideTab::Library;
            }
        });
    }

    /// The Edit toolbar for the current selection, under the drawing.
    fn edit_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let n = self.selection.len();
            if n == 0 {
                ui.weak("Select an object to edit it.");
                return;
            }
            let kind = self.doc().kind_of(self.selection[0]);
            let label = if n == 1 {
                crate::props::title_case(kind.map_or("", |k| k.name()))
            } else {
                format!("{n} objects")
            };
            ui.label(RichText::new(label).strong());
            ui.separator();
            if icons::button(ui, Icon::OpenObject, false, true, "Open Object (Ctrl+E)").clicked() {
                self.open_object();
            }
            if icons::button(
                ui,
                Icon::SelectNext,
                false,
                true,
                "Select Next Object (Tab)",
            )
            .clicked()
            {
                self.select_next();
            }
            if icons::button(ui, Icon::Delete, false, true, "Delete (Del)").clicked() {
                self.delete_selection();
            }
            if icons::button(ui, Icon::Copy, false, true, "Copy (Ctrl+C)").clicked() {
                self.copy();
            }
            ui.separator();
            let t = &mut self.dialogs.transform;
            if icons::button(ui, Icon::Move, false, true, "Move…").clicked() {
                t.open_tab(crate::dialogs::TransformTab::Move);
            }
            if icons::button(ui, Icon::Rotate, false, true, "Rotate…").clicked() {
                t.open_tab(crate::dialogs::TransformTab::Rotate);
            }
            if icons::button(ui, Icon::Reflect, false, true, "Reflect About Object…").clicked() {
                t.open_tab(crate::dialogs::TransformTab::Reflect);
            }
            if icons::button(ui, Icon::MultiCopy, false, true, "Multiple Copy…").clicked() {
                t.open_multiple();
            }
            let line_like = kind.is_some_and(|k| matches!(k, ObjectKind::Cad | ObjectKind::Wall));
            if line_like {
                ui.separator();
                if icons::button(ui, Icon::Offset, false, n == 1, "Offset…").clicked() {
                    self.dialogs.cad_op.open(crate::dialogs::CadOp::Offset);
                }
                if icons::button(ui, Icon::Fillet, false, n == 2, "Fillet (select two lines)")
                    .clicked()
                {
                    self.dialogs.cad_op.open(crate::dialogs::CadOp::Fillet);
                }
                if icons::button(
                    ui,
                    Icon::Chamfer,
                    false,
                    n == 2,
                    "Chamfer (select two lines)",
                )
                .clicked()
                {
                    self.dialogs.cad_op.open(crate::dialogs::CadOp::Chamfer);
                }
                if icons::button(
                    ui,
                    Icon::Extend,
                    false,
                    n == 2,
                    "Trim / Extend first to second",
                )
                .clicked()
                {
                    let (id, boundary) = (self.selection[0], self.selection[1]);
                    self.run1(Command::ExtendTo { id, boundary });
                }
            }
        });
    }

    fn side_windows(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.side_tab,
                SideTab::ProjectBrowser,
                "Project Browser",
            );
            ui.selectable_value(&mut self.side_tab, SideTab::LayerDisplay, "Layer Display");
            ui.selectable_value(&mut self.side_tab, SideTab::Library, "Library");
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| match self.side_tab {
            SideTab::ProjectBrowser => self.project_browser(ui),
            SideTab::LayerDisplay => self.layer_display(ui),
            SideTab::Library => {
                ui.weak(
                    "The Library Browser will hold wall types, symbols, materials and \
                     manufacturer catalogs (backlog LIB-01). Wall types and room types are \
                     managed from the Tools menu for now.",
                );
            }
        });
    }

    fn project_browser(&mut self, ui: &mut Ui) {
        ui.strong(self.doc().name.clone());
        egui::CollapsingHeader::new("Floors")
            .default_open(true)
            .show(ui, |ui| {
                let floors: Vec<(Id, String)> = self
                    .doc()
                    .floors_by_elevation()
                    .iter()
                    .rev()
                    .map(|f| (f.id, f.name.clone()))
                    .collect();
                for (id, name) in floors {
                    let resp = ui.selectable_label(self.plan.floor == id, &name);
                    if resp.clicked() {
                        self.plan.floor = id;
                        self.selection.clear();
                    }
                    if resp.double_clicked() {
                        self.spec.push(id);
                    }
                }
            });
        egui::CollapsingHeader::new("Plan Views")
            .default_open(true)
            .show(ui, |ui| {
                let views: Vec<PlanView> = self.doc().iter::<PlanView>().cloned().collect();
                for v in views {
                    let resp = ui.selectable_label(self.plan.view == Some(v.id), &v.name);
                    if resp.clicked() {
                        self.apply_plan_view(&v);
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Open Object").clicked() {
                            self.spec.push(v.id);
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            self.run1(Command::Delete { ids: vec![v.id] });
                            ui.close();
                        }
                    });
                }
            });
        egui::CollapsingHeader::new("Cameras")
            .default_open(true)
            .show(ui, |ui| {
                let cams: Vec<Camera> = self.doc().iter::<Camera>().cloned().collect();
                for cam in cams {
                    let resp = ui.selectable_label(false, &cam.name);
                    if resp.clicked() {
                        self.view3d.apply_camera(&cam);
                        self.mode = Mode::ThreeD;
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Open Object").clicked() {
                            self.spec.push(cam.id);
                            ui.close();
                        }
                        if ui.button("Copy").clicked() {
                            self.run1(Command::AddCamera {
                                name: format!("{} copy", cam.name),
                                projection: cam.projection.clone(),
                                eye: cam.eye,
                                target: cam.target,
                                layer_set: Some(cam.layer_set),
                            });
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            self.run1(Command::Delete { ids: vec![cam.id] });
                            ui.close();
                        }
                    });
                }
            });
        egui::CollapsingHeader::new("Rooms")
            .default_open(true)
            .show(ui, |ui| {
                let rooms: Vec<(Id, String)> = self
                    .doc()
                    .rooms_on(self.plan.floor)
                    .map(|r| (r.id, self.doc().room_name(r)))
                    .collect();
                if rooms.is_empty() {
                    ui.weak("Enclose an area with walls to make a room.");
                }
                for (id, name) in rooms {
                    let resp = ui.selectable_label(self.selection.contains(&id), name);
                    if resp.clicked() {
                        self.selection = vec![id];
                    }
                    if resp.double_clicked() {
                        self.spec.push(id);
                    }
                }
                let summary = housecraft_api::query::summary(self.doc());
                ui.weak(format!("Living area: {}", summary.living_area));
            });
    }

    fn layer_display(&mut self, ui: &mut Ui) {
        let sets: Vec<(Id, String)> = self
            .doc()
            .iter::<LayerSet>()
            .map(|s| (s.id, s.name.clone()))
            .collect();
        ui.horizontal(|ui| {
            ui.label("Layer Set:");
            let current = sets
                .iter()
                .find(|(id, _)| *id == self.plan.layer_set)
                .map_or(String::new(), |s| s.1.clone());
            egui::ComboBox::from_id_salt("layer_set")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (id, name) in &sets {
                        ui.selectable_value(&mut self.plan.layer_set, *id, name);
                    }
                });
        });
        let set = self.doc().get::<LayerSet>(self.plan.layer_set).cloned();
        let layers: Vec<Layer> = self.doc().iter::<Layer>().cloned().collect();
        egui::Grid::new("layers").striped(true).show(ui, |ui| {
            ui.strong("Disp");
            ui.strong("Name");
            ui.strong("Color");
            ui.end_row();
            for layer in layers {
                let mut visible = set.as_ref().is_none_or(|s| s.is_visible(layer.id));
                if ui.checkbox(&mut visible, "").changed() {
                    self.run1(Command::SetLayerDisplay {
                        layer_set: self.plan.layer_set,
                        layer: layer.id,
                        visible: Some(visible),
                        color: None,
                        line_weight: None,
                        line_style: None,
                    });
                }
                if ui.selectable_label(false, &layer.name).double_clicked() {
                    self.spec.push(layer.id);
                }
                let swatch = layer
                    .color
                    .map_or(ui.visuals().text_color(), crate::util::to_color32);
                ui.label(RichText::new("■■").color(swatch));
                ui.end_row();
            }
        });
        if ui.button("Layers and Layer Sets…").clicked() {
            self.dialogs.layers = true;
        }
    }

    pub fn apply_plan_view(&mut self, v: &PlanView) {
        self.plan.view = Some(v.id);
        if let Some(f) = v.floor {
            self.plan.floor = f;
        }
        self.plan.layer_set = v.layer_set;
        self.plan.reference_floor = v.reference_floor;
        if let Some(z) = &v.zoom {
            self.plan.center = housecraft_api::geom::v2(z.center);
            self.plan.mm_per_pt = z.mm_per_point;
        }
        if self.doc().settings.active_default_set != v.default_set {
            self.run1(Command::SetActiveDefaultSet { id: v.default_set });
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        use egui::{Key, KeyboardShortcut, Modifiers};
        let consume = |m: Modifiers, k: Key| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)))
        };
        let ctrl = Modifiers::COMMAND;
        let shift = Modifiers::SHIFT;
        let none = Modifiers::NONE;
        if consume(ctrl, Key::Z) {
            self.undo();
        }
        if consume(ctrl, Key::Y) || consume(ctrl | shift, Key::Z) {
            self.redo();
        }
        if consume(ctrl, Key::S) {
            self.save(false);
        }
        if consume(ctrl, Key::O) {
            self.guarded(Pending::Open);
        }
        if consume(ctrl, Key::N) {
            self.guarded(Pending::New);
        }
        if consume(ctrl, Key::P) {
            self.dialogs.print.open = true;
        }
        if consume(ctrl | Modifiers::ALT, Key::V) {
            self.paste(true);
        }
        if consume(ctrl, Key::C) && !self.selection.is_empty() {
            self.copy();
        }
        if consume(ctrl, Key::X) && !self.selection.is_empty() {
            self.cut();
        }
        if consume(ctrl, Key::V) {
            self.paste(false);
        }
        if consume(ctrl, Key::A) {
            self.select_all();
        }
        if consume(ctrl, Key::F) {
            self.dialogs.foundation.open = true;
        }
        if consume(ctrl, Key::J) {
            self.select_same_type();
        }
        if consume(ctrl, Key::E) {
            self.open_object();
        }
        if consume(ctrl, Key::F9) {
            self.prefs.snap.grid = !self.prefs.snap.grid;
        }
        if consume(shift, Key::F11) {
            self.prefs.snap.objects = !self.prefs.snap.objects;
        }
        // Single-key tool and view shortcuts.
        if consume(shift, Key::Q) {
            self.set_tool(Tool::ExteriorWall);
        }
        if consume(shift, Key::W) {
            self.set_tool(Tool::Window(WindowStyle::DoubleHung));
        }
        if consume(shift, Key::E) {
            self.set_tool(Tool::Door(DoorStyle::Hinged));
        }
        if consume(shift, Key::P) {
            self.set_tool(Tool::Box);
        }
        if consume(shift, Key::A) {
            self.auto_exterior_dimensions();
        }
        if consume(shift, Key::N) {
            self.change_floor(true);
        }
        if consume(shift, Key::M) {
            self.change_floor(false);
        }
        if consume(shift, Key::X)
            && let Some(out) = self.run1(Command::AddFloor {
                name: None,
                elevation: None,
                ceiling_height: None,
                floor_structure: None,
                below: None,
            })
        {
            self.plan.floor = out.created[0];
        }
        if consume(shift, Key::J) {
            self.camera_action(CameraAction::FullCamera);
        }
        if consume(shift, Key::K) {
            self.camera_action(CameraAction::PerspectiveOverview);
        }
        if self.mode == Mode::Plan
            && self.plan.pending.is_empty()
            && self.plan.length_input.is_none()
        {
            if consume(none, Key::Space) {
                self.set_tool(Tool::Select);
            }
            if consume(none, Key::W) {
                self.set_tool(Tool::Line);
            }
            if consume(none, Key::E) {
                self.set_tool(Tool::Arc);
            }
            if consume(none, Key::K) {
                self.set_tool(Tool::Circle);
            }
            if consume(none, Key::Y) {
                self.set_tool(Tool::Text);
            }
            if consume(none, Key::H) {
                self.camera_action(CameraAction::OrthographicOverview);
            }
        }
        if consume(none, Key::Tab) {
            self.select_next();
        }
        if consume(none, Key::Delete) || consume(none, Key::Backspace) {
            self.delete_selection();
        }
        if consume(none, Key::F2) {
            self.mode = Mode::Plan;
        }
        if consume(none, Key::F3) {
            self.mode = Mode::ThreeD;
        }
        if consume(none, Key::F6) {
            self.plan.fit_requested = true;
            self.view3d.fit_requested = true;
        }
        if consume(none, Key::F9) {
            self.plan.reference_floor = !self.plan.reference_floor;
        }
        if consume(none, Key::F10) {
            self.prefs.snap.angles = !self.prefs.snap.angles;
        }
        let tilde = ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Text(t) if t == "~"))
        });
        if tilde {
            self.dialogs.prefs = true;
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let tool_name = if self.mode == Mode::ThreeD {
                "3D View"
            } else {
                self.plan.tool.name()
            };
            ui.label(tool_name);
            ui.separator();
            // Foundation is floor 0; storeys count from 1.
            let floor = self
                .doc()
                .floors_by_elevation()
                .iter()
                .filter(|f| f.foundation.is_none())
                .position(|f| f.id == self.plan.floor)
                .map_or(0, |i| i + 1);
            ui.label(format!("Floor: {floor}"));
            ui.separator();
            let layer = self
                .selection
                .first()
                .and_then(|id| self.doc().layer_of(*id))
                .and_then(|l| self.doc().get::<Layer>(l))
                .map_or_else(|| "-".to_string(), |l| l.name.clone());
            ui.label(format!("Object Layer: {layer}"));
            ui.separator();
            if let Some((msg, is_err)) = &self.status {
                let color = if *is_err {
                    ui.visuals().error_fg_color
                } else {
                    ui.visuals().text_color()
                };
                ui.label(RichText::new(msg).color(color));
            } else if self.mode == Mode::Plan {
                ui.weak(self.plan.tool.hint());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(p) = self.plan.cursor
                    && self.mode == Mode::Plan
                {
                    let doc = self.doc();
                    ui.monospace(format!(
                        "X: {}  Y: {}",
                        crate::util::fmt(doc, housecraft_api::units::Length::from_mm(p.x)),
                        crate::util::fmt(doc, housecraft_api::units::Length::from_mm(p.y)),
                    ));
                    if let Some(s) = self.plan.snap
                        && !s.kind.label().is_empty()
                    {
                        ui.weak(s.kind.label());
                    }
                }
                if self.session.is_dirty() {
                    ui.weak("Modified");
                }
            });
        });
    }

    fn confirm_discard(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending.clone() else {
            return;
        };
        let modal = egui::Modal::new(egui::Id::unique("discard")).show(ctx, |ui| {
            ui.heading("Save changes?");
            ui.label(format!("\"{}\" has unsaved changes.", self.doc().name));
            ui.add_space(8.0);
            let mut choice = None;
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    choice = Some(true);
                }
                if ui.button("Don't Save").clicked() {
                    choice = Some(false);
                }
                if ui.button("Cancel").clicked() {
                    self.pending = None;
                }
            });
            choice
        });
        if let Some(save) = modal.inner {
            self.pending = None;
            if !save || self.save(false) {
                self.session.mark_saved();
                self.perform(action);
                if self.quit_confirmed {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}

/// A menu entry with a right-aligned shortcut. Returns true (and closes the
/// menu) when clicked.
fn item(ui: &mut Ui, label: &str, shortcut: &str, enabled: bool) -> bool {
    let button = egui::Button::new(label).shortcut_text(shortcut);
    let clicked = ui.add_enabled(enabled, button).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

fn tool_item(ui: &mut Ui, tool: Tool) -> bool {
    item(ui, tool.name(), tool.hotkey().unwrap_or(""), true)
}

fn palette_row(ui: &mut Ui, icon: Icon, name: &str, selected: bool) -> egui::Response {
    ui.horizontal(|ui| {
        let r = icons::button(ui, icon, selected, true, name);
        let l = ui.selectable_label(selected, name);
        r.union(l)
    })
    .inner
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Intercept window close to offer saving.
        if ctx.input(|i| i.viewport().close_requested())
            && !self.quit_confirmed
            && self.session.is_dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }
        let title = format!(
            "{}{} - HouseCraft",
            if self.session.is_dirty() { "* " } else { "" },
            self.path.as_ref().and_then(|p| p.file_name()).map_or_else(
                || self.doc().name.clone(),
                |n| n.to_string_lossy().into_owned()
            )
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        self.shortcuts(&ctx);
        // Open plans dropped onto the window.
        let dropped: Option<PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter().find_map(|f| {
                Some(f.path().to_path_buf()).filter(|p| {
                    p.extension().is_some_and(|e| {
                        e.eq_ignore_ascii_case(housecraft_io_native::EXTENSION)
                            || e.eq_ignore_ascii_case(housecraft_io_native::LEGACY_EXTENSION)
                    })
                })
            })
        });
        if let Some(p) = dropped {
            self.guarded(Pending::OpenPath(p));
        }
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar_main").show(ui, |ui| self.toolbar_main(ui));
        egui::Panel::top("toolbar_build").show(ui, |ui| self.toolbar_build(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("tool_palette")
            .resizable(true)
            .default_size(190.0)
            .show(ui, |ui| self.tool_palette(ui));
        egui::Panel::right("view_toolbar")
            .resizable(false)
            .exact_size(40.0)
            .show(ui, |ui| self.view_toolbar(ui));
        egui::Panel::right("side_windows")
            .resizable(true)
            .default_size(260.0)
            .show(ui, |ui| self.side_windows(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                egui::Panel::bottom("edit_toolbar").show(ui, |ui| self.edit_toolbar(ui));
                match self.mode {
                    Mode::Plan => crate::canvas::show(self, ui),
                    Mode::ThreeD => crate::view3d::show(self, ui),
                }
            });
        crate::props::spec_windows(self, &ctx);
        crate::props::room_multi_window(self, &ctx);
        crate::dialogs::show(self, &ctx);
        self.confirm_discard(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "prefs", &self.prefs);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_open_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "HouseCraft plan",
            &[housecraft_io_native::EXTENSION, "hcraft"],
        )
        .pick_file()
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_save_path(name: &str, ext: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_file_name(format!("{name}.{ext}"))
        .add_filter(ext.to_uppercase(), &[ext])
        .save_file()
}

#[cfg(target_arch = "wasm32")]
fn pick_open_path() -> Option<PathBuf> {
    None
}

#[cfg(target_arch = "wasm32")]
fn pick_save_path(_name: &str, _ext: &str) -> Option<PathBuf> {
    None
}
