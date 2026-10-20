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
use housecraft_api::geom::DVec2;
use housecraft_api::units::{Length, Point2};
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
    /// Plans on white paper whatever the theme (as Chief draws them).
    pub paper_canvas: bool,
    /// The red X / blue Y origin marker and axis indicator.
    pub show_axes: bool,
    /// Your own tool shortcuts: one key or a two-key sequence.
    pub hotkeys: Vec<UserHotkey>,
    /// Recent files kept at the top of the list.
    pub pinned: Vec<String>,
    /// The template New Plan starts from (`None`: the built-in plan).
    pub default_template: Option<String>,
    /// Minutes between automatic archive copies of a changed plan (0: off).
    pub archive_minutes: u32,
    /// How many archive copies to keep per plan.
    pub archive_keep: u32,
    /// The folder the Plan Portfolio lists.
    pub portfolio_folder: Option<String>,
}

/// A user-assigned shortcut: `keys` is one stroke ("G", "Ctrl+Shift+L") or
/// two separated by a space ("G W": press G, then W).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserHotkey {
    pub keys: String,
    /// The tool's name as shown in the menus.
    pub tool: String,
}

/// Normalise "ctrl+g  w" to "CTRL+G W"; `None` unless one or two strokes.
pub fn parse_hotkey(text: &str) -> Option<String> {
    let strokes: Vec<String> = text
        .split_whitespace()
        .map(|s| s.to_uppercase())
        .collect();
    let ok = |s: &String| {
        let key = s.rsplit('+').next().unwrap_or("");
        egui::Key::ALL.iter().any(|k| k.name().eq_ignore_ascii_case(key))
    };
    (matches!(strokes.len(), 1 | 2) && strokes.iter().all(ok)).then(|| strokes.join(" "))
}

/// Every drawing tool, once each, in toolbar order.
pub fn all_tools() -> Vec<Tool> {
    let mut out: Vec<Tool> = Vec::new();
    for t in Family::TOOLBAR
        .iter()
        .flat_map(|f| f.tools().iter().copied())
        .chain(Tool::TERRAIN)
        .chain(Tool::SITE)
    {
        if !out.iter().any(|o| o.name() == t.name()) {
            out.push(t);
        }
    }
    out
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: Theme::System,
            snap: SnapSettings::default(),
            show_grid: true,
            recent: Vec::new(),
            paper_canvas: true,
            show_axes: true,
            hotkeys: Vec::new(),
            pinned: Vec::new(),
            default_template: None,
            archive_minutes: 15,
            archive_keep: 5,
            portfolio_folder: None,
        }
    }
}

/// Where a requested screenshot lands when the window has drawn it.
pub type CaptureSlot = std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<egui::ColorImage>>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Plan,
    ThreeD,
    /// A 2D elevation, cross section or layout page.
    Drawing(crate::drawing_view::DrawingSource),
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
    /// Another plan file shown for reference.
    pub reference: Option<crate::project_tools::RefPlan>,
    pub time: crate::project_tools::TimeTracker,
    pub prefs: Prefs,
    pub dialogs: Dialogs,
    pub materials: crate::materials::MaterialsDialog,
    pub library: crate::library_view::LibraryState,
    pub object_painter: Option<crate::object_painter::ObjectPainter>,
    /// The GPU, for rendering images offscreen.
    pub render_state: Option<egui_wgpu::RenderState>,
    /// The 3D image export dialog: width, height, transparent background.
    pub image_export: Option<(u32, u32, bool)>,
    /// Terrain > Plot Plan from Bearings: (sides, setback) being typed.
    plot: Option<(String, String)>,
    /// A special render being set up (3D > Export).
    render_export: Option<RenderExport>,
    /// A 3D note being written: where, its text, and its camera.
    pub note3d: Option<(housecraft_api::units::Point3, String, Option<Id>)>,
    /// A 3D model file waiting for its import options.
    pub symbol_import: Option<(std::path::PathBuf, housecraft_io_native::symbols::ImportOptions)>,
    pub bufs: EditBuffers,
    /// Objects whose specification windows are open.
    pub spec: Vec<Id>,
    /// Rooms being edited together (multi-room specification).
    pub room_multi: Option<Vec<Id>>,
    /// The schedule or materials list shown in the table window, if open.
    pub table_view: Option<crate::schedules::TableSource>,
    pub table_this_floor: bool,
    pub drawing_view: crate::drawing_view::DrawingViewState,
    pub clipboard: Vec<serde_json::Value>,
    /// The clipboard holds layout boxes (copied from a layout page), and
    /// the file and plan they came from.
    pub clipboard_layout: Option<(Option<PathBuf>, String)>,
    /// The layer set the materials list counts (the plan view's when `None`).
    pub materials_layer_set: Option<Id>,
    /// A screen capture on its way (3D > Material from View): the image
    /// slot, and the 3D view's rect and scale when it was asked for.
    pub capture: Option<(CaptureSlot, egui::Rect, f32)>,
    /// The eyedropper: the material to colour and, once clicked, where.
    pub eyedrop: Option<(Id, Option<egui::Pos2>)>,
    /// A capture for the eyedropper on its way.
    pub eyedrop_capture: Option<(CaptureSlot, egui::Pos2, f32, Id)>,
    pub status: Option<(String, bool)>,
    pub pending: Option<Pending>,
    quit_confirmed: bool,
    /// Views open as document tabs, in the order opened.
    pub tabs: Vec<Mode>,
    /// The Tool Search box's text.
    tool_search: String,
    /// The first key of a two-key shortcut, and when it was pressed.
    chord: Option<(String, f64)>,
    /// Size of the drawing area in points.
    pub view_px: egui::Vec2,
    /// Active Layer Display Options shown in its own floating window.
    layer_window: bool,
    /// The Project Browser's filter text, and the item whose details show.
    browser_filter: String,
    browser_detail: Option<Id>,
    /// When the plan was last archived (app time, s) and its revision then.
    last_archive: (f64, u64),
    /// The name being typed in Save as Template.
    template_name: Option<String>,
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
        let render_state = cc.wgpu_render_state.clone();
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
            reference: None,
            time: Default::default(),
            prefs,
            dialogs: Dialogs::default(),
            materials: Default::default(),
            library: Default::default(),
            symbol_import: None,
            render_state,
            image_export: None,
            plot: None,
            render_export: None,
            note3d: None,
            object_painter: None,
            bufs: EditBuffers::default(),
            spec: Vec::new(),
            room_multi: None,
            table_view: None,
            table_this_floor: false,
            drawing_view: Default::default(),
            clipboard: Vec::new(),
            clipboard_layout: None,
            materials_layer_set: None,
            capture: None,
            eyedrop: None,
            eyedrop_capture: None,
            status: None,
            pending: None,
            quit_confirmed: false,
            tabs: vec![Mode::Plan],
            tool_search: String::new(),
            chord: None,
            view_px: egui::Vec2::ZERO,
            last_archive: (0.0, 0),
            layer_window: false,
            browser_filter: String::new(),
            browser_detail: None,
            template_name: None,
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
        if self
            .plan
            .sheet
            .is_some_and(|s| {
                let block = doc.kind_of(s) == Some(ObjectKind::Block);
                (!doc.is_sheet(s) && !block) || doc.kind_of(s) == Some(ObjectKind::Floor)
            })
        {
            self.plan.sheet = None;
            self.plan.fit_requested = true;
        }
        if doc.get::<LayerSet>(self.plan.layer_set).is_none() {
            self.plan.layer_set = doc.iter::<LayerSet>().next().map_or(Id(0), |s| s.id);
        }
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.library.opening = None;
        self.plan.set_tool(tool);
        self.palette = tool.family();
        self.mode = Mode::Plan;
        if !tool.is_drafting() {
            self.close_sheet();
        }
    }

    /// Show an elevation/section view or CAD detail in the plan window, where
    /// the CAD, text and dimension tools draw on it.
    pub fn open_sheet(&mut self, id: Id) {
        self.mode = Mode::Plan;
        if self.plan.sheet != Some(id) {
            self.plan.sheet = Some(id);
            self.plan.fit_requested = true;
            self.selection.clear();
        }
        if !self.plan.tool.is_drafting() {
            self.plan.set_tool(Tool::Select);
        }
    }

    /// Back to the floor plan from a view or CAD detail.
    fn close_sheet(&mut self) {
        if self.plan.sheet.take().is_some() {
            self.plan.fit_requested = true;
            self.selection.clear();
        }
    }

    /// The plan window: back to the floor plan.
    pub fn show_plan(&mut self) {
        self.mode = Mode::Plan;
        self.close_sheet();
    }

    /// What the plan window shows (the floor plan, or the open view or CAD
    /// detail), and a name for it.
    pub fn current_drawing(&self) -> (housecraft_draft::Drawing, String) {
        let doc = self.doc();
        match self.plan.sheet.and_then(|s| doc.object(s)) {
            Some(Object::SectionView(v)) => {
                (housecraft_draft::elevation::section_view(doc, v), v.name.clone())
            }
            Some(Object::CadDetail(d)) => (
                housecraft_draft::detail::cad_detail(doc, d.id, Some(self.plan.layer_set)),
                d.name.clone(),
            ),
            _ => {
                let opts = housecraft_draft::PlanOptions {
                    floor: self.plan.floor,
                    layer_set: self.plan.layer_set,
                    reference_floor: self.plan.reference_floor,
                    references: self.plan.references.clone(),
                    phase: self.plan.phase,
                    sun_shadows: self.plan.sun_shadows,
                    default_set: None,
                };
                let name = doc
                    .get::<Floor>(self.plan.floor)
                    .map_or("Plan".into(), |f| format!("{} Plan", f.name));
                (housecraft_draft::plan(doc, &opts), name)
            }
        }
    }

    /// CAD > CAD Detail from View: copy what the plan window shows into a
    /// new CAD detail of plain, editable lines and text, and open it.
    pub fn detail_from_view(&mut self) {
        let (drawing, name) = self.current_drawing();
        let mut n = 1;
        let mut detail_name = format!("{name} Detail");
        while self.doc().iter::<CadDetail>().any(|d| d.name == detail_name) {
            n += 1;
            detail_name = format!("{name} Detail {n}");
        }
        let command = housecraft_draft::detail::detail_from_drawing(&drawing, &detail_name);
        if let Some(out) = self.run1(command)
            && let Some(id) = out.created.first()
        {
            self.open_sheet(*id);
            self.info(format!("Made CAD detail {detail_name:?}"));
        }
    }

    /// File > Import > Picture: place an image in the middle of the view,
    /// 20' wide (10' on a view or detail), keeping its proportions.
    pub fn import_picture(&mut self) {
        let Some(path) = crate::props::pick_image() else {
            return;
        };
        let pixels = crate::images::pixel_size(&path);
        let width = housecraft_api::units::Length::from_feet(if self.plan.sheet.is_some() { 10.0 } else { 20.0 });
        let height = pixels.map_or(width, |[w, h]| width * (h as f64 / w.max(1) as f64));
        let c = self.plan.center;
        let position = housecraft_api::units::Point2(
            housecraft_api::units::Length::from_mm(c.x - width.mm() / 2.0),
            housecraft_api::units::Length::from_mm(c.y - height.mm() / 2.0),
        );
        self.mode = Mode::Plan;
        if let Some(out) = self.run1(Command::AddPicture {
            floor: Some(self.plan.target()),
            path,
            position,
            width,
            height: Some(height),
            pixels,
            transparent: None,
        }) {
            self.selection = out.created;
        }
    }

    /// The 3D image export window.
    pub fn image_export_dialog(&mut self, ctx: &egui::Context) {
        let Some((mut w, mut h, mut transparent)) = self.image_export else {
            return;
        };
        let mut open = true;
        let mut go = false;
        egui::Window::new("Export 3D View as Image")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Size:");
                    ui.add(egui::DragValue::new(&mut w).range(16..=16384).suffix(" px"));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut h).range(16..=16384).suffix(" px"));
                });
                ui.horizontal(|ui| {
                    for (name, pw, ph) in [("HD", 1920, 1080), ("4K", 3840, 2160), ("8K", 7680, 4320), ("Square", 4096, 4096)] {
                        if ui.small_button(name).clicked() {
                            (w, h) = (pw, ph);
                        }
                    }
                });
                ui.checkbox(&mut transparent, "Transparent background (no sky)");
                ui.weak("Uses the current 3D camera; very large sizes depend on your graphics card.");
                go = ui.button("Export…").clicked();
            });
        if go {
            self.image_export = None;
            if let Some(path) = pick_save_path(&self.doc().name, "png") {
                self.mode = Mode::ThreeD;
                match crate::view3d::export_image(self, w, h, transparent, &path) {
                    Ok(()) => self.info(format!("Saved {}", path.display())),
                    Err(e) => self.error(format!("Export failed: {e}")),
                }
            }
        } else {
            self.image_export = open.then_some((w, h, transparent));
        }
    }

    /// The 3D model import options window.
    pub fn symbol_import_dialog(&mut self, ctx: &egui::Context) {
        use housecraft_io_native::symbols::{ModelUnit, read_model};
        let Some((path, mut opts)) = self.symbol_import.clone() else {
            return;
        };
        let mut open = true;
        let mut go = false;
        egui::Window::new("Import 3D Symbol")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(path.file_name().map_or(String::new(), |f| f.to_string_lossy().into_owned()));
                egui::Grid::new("symbol_import").num_columns(2).show(ui, |ui| {
                    ui.label("Drawn in");
                    egui::ComboBox::from_id_salt("symbol_unit")
                        .selected_text(opts.unit.name())
                        .show_ui(ui, |ui| {
                            for u in ModelUnit::ALL {
                                ui.selectable_value(&mut opts.unit, u, u.name());
                            }
                        });
                    ui.end_row();
                    ui.label("Up is");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut opts.y_up, true, "Y (most OBJ files)");
                        ui.radio_value(&mut opts.y_up, false, "Z");
                    });
                    ui.end_row();
                });
                ui.weak("If it comes in lying down or the wrong size, delete it and import again with other choices.");
                go = ui.button("Import").clicked();
            });
        if go {
            self.symbol_import = None;
            match read_model(&path, opts) {
                Ok(def) => {
                    if let Some(out) = self.run1(Command::AddSymbolDef {
                        name: def.name,
                        positions: def.positions,
                        triangles: def.triangles,
                        groups: def.groups,
                        behavior: def.behavior,
                    }) && let Some(id) = out.created.first()
                    {
                        self.set_tool(Tool::PlaceSymbol(*id));
                        self.info("Click in the plan to place the symbol");
                    }
                }
                Err(e) => self.error(format!("Could not import: {e}")),
            }
        } else if !open {
            self.symbol_import = None;
        } else {
            self.symbol_import = Some((path, opts));
        }
    }

    /// CAD > CAD Details > New CAD Detail.
    /// Copy a CAD detail with everything drawn on it (paste it in this or
    /// another plan with CAD > CAD Details > Paste CAD Detail).
    pub fn copy_cad_detail(&mut self, detail: Id) {
        let doc = self.doc();
        let mut ids = vec![detail];
        ids.extend(doc.objects.keys().copied().filter(|id| doc.floor_of(*id) == Some(detail)));
        let clip: Vec<serde_json::Value> = ids.iter().filter_map(|id| housecraft_api::query::get_object(doc, *id)).collect();
        let n = clip.len() - 1;
        self.clipboard = clip;
        self.clipboard_layout = None;
        self.info(format!("Copied the CAD detail and its {n} object(s)"));
    }

    pub fn paste_cad_detail(&mut self) {
        let objects: Vec<serde_json::Value> = self.clipboard.clone();
        if let Some(out) = self.run1(Command::InsertObjects {
            objects,
            floor: None,
            dx: housecraft_api::units::Length::ZERO,
            dy: housecraft_api::units::Length::ZERO,
        }) && let Some(d) = out.created.iter().copied().find(|id| self.doc().get::<CadDetail>(*id).is_some())
        {
            self.open_sheet(d);
            self.plan.fit_requested = true;
        }
    }

    pub fn new_cad_detail(&mut self) {
        let n = self.doc().count::<CadDetail>() + 1;
        if let Some(out) = self.run1(Command::AddCadDetail {
            name: format!("CAD Detail {n}"),
            shapes: Vec::new(),
            texts: Vec::new(),
        }) && let Some(id) = out.created.first()
        {
            self.open_sheet(*id);
        }
    }

    /// CAD > Blocks > Make Block: the selected CAD shapes become a block.
    pub fn make_block(&mut self) {
        let doc = self.doc();
        let ids: Vec<Id> = self
            .selection
            .iter()
            .copied()
            .filter(|id| doc.kind_of(*id) == Some(ObjectKind::Cad))
            .collect();
        if ids.is_empty() {
            self.error("Select the lines and shapes to make into a block first.");
            return;
        }
        let mut n = doc.count::<Block>() + 1;
        while doc.iter::<Block>().any(|b| b.name == format!("Block {n}")) {
            n += 1;
        }
        if let Some(out) = self.run1(Command::AddBlock {
            name: format!("Block {n}"),
            ids,
            base: None,
            shapes: Vec::new(),
        }) {
            self.selection = out
                .created
                .iter()
                .copied()
                .filter(|id| self.doc().kind_of(*id) == Some(ObjectKind::BlockRef))
                .collect();
            self.info(format!("Made Block {n}; rename it in its specification"));
        }
    }

    /// The selected placed block, if exactly one is selected.
    fn selected_block_ref(&self) -> Option<BlockRef> {
        match self.selection.as_slice() {
            [id] => self.doc().get::<BlockRef>(*id).cloned(),
            _ => None,
        }
    }

    /// The Plot Plan from Bearings window: one side per line as
    /// "bearing, distance", drawn from the plan's origin.
    fn plot_dialog(&mut self, ctx: &egui::Context) {
        let Some((mut sides, mut setback)) = self.plot.take() else {
            return;
        };
        let mut open = true;
        let mut go = false;
        egui::Window::new("Plot Plan from Bearings")
            .open(&mut open)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.label("One side per line: bearing, distance (e.g. N 45°30' E, 120').");
                ui.weak("The lot starts at the plan's origin; end where you started to close it.");
                ui.add(egui::TextEdit::multiline(&mut sides).desired_rows(6).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    ui.label("Building setback");
                    ui.text_edit_singleline(&mut setback);
                });
                go = ui.button("Draw Lot").clicked();
            });
        if go {
            let unit = crate::util::input_unit(self.doc());
            let mut legs = Vec::new();
            for line in sides.lines().filter(|l| !l.trim().is_empty()) {
                let Some((b, d)) = line.rsplit_once(',') else {
                    self.error(format!("Expected \"bearing, distance\": {line}"));
                    self.plot = Some((sides, setback));
                    return;
                };
                match housecraft_api::units::Length::parse(d.trim(), unit) {
                    Ok(distance) => legs.push(housecraft_api::PlotLeg { bearing: b.trim().into(), distance }),
                    Err(_) => {
                        self.error(format!("Can't read the distance in: {line}"));
                        self.plot = Some((sides, setback));
                        return;
                    }
                }
            }
            let setback = housecraft_api::units::Length::parse(setback.trim(), unit).ok().filter(|l| l.mm() > 0.0);
            if self
                .run1(Command::AddPlotLines {
                    floor: Some(self.plan.floor),
                    start: Default::default(),
                    legs,
                    setback,
                })
                .is_some()
            {
                return;
            }
        }
        if open {
            self.plot = Some((sides, setback));
        }
    }

    /// Whether the framing layer shows in the current plan view.
    fn framing_visible(&self) -> bool {
        let doc = self.doc();
        match (doc.active_defaults().layers.framing, doc.get::<LayerSet>(self.plan.layer_set)) {
            (Some(l), Some(set)) => set.display(l).visible,
            _ => false,
        }
    }

    fn set_framing_visible(&mut self, visible: bool) {
        let Some(layer) = self.doc().active_defaults().layers.framing else {
            return;
        };
        let set = self.plan.layer_set;
        self.run1(Command::SetLayerDisplay {
            layer_set: set,
            layer,
            visible: Some(visible),
            color: None,
            line_weight: None,
            line_style: None,
        });
    }

    /// Writing a 3D note's text after clicking where it goes.
    fn note3d_dialog(&mut self, ctx: &egui::Context) {
        let Some((at, mut text, camera)) = self.note3d.take() else {
            return;
        };
        let mut open = true;
        let mut ok = false;
        egui::Window::new("3D Note").open(&mut open).resizable(false).show(ctx, |ui| {
            let r = ui.text_edit_singleline(&mut text);
            r.request_focus();
            ok = ui.button("Add").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        });
        if ok && !text.trim().is_empty() {
            self.run1(Command::AddAnnotation3d {
                camera,
                mark: housecraft_api::model::Mark3d::Note { at, text },
            });
        } else if open {
            self.note3d = Some((at, text, camera));
        }
    }

    /// The settings window for a special render, then the render itself.
    fn render_export_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut e) = self.render_export.take() else {
            return;
        };
        let mut open = true;
        let mut go = false;
        egui::Window::new(e.kind.title()).open(&mut open).resizable(false).show(ctx, |ui| {
            egui::Grid::new("render_export").num_columns(2).show(ui, |ui| {
                if e.kind == RenderKind::Panorama {
                    ui.label("Width");
                    ui.add(egui::DragValue::new(&mut e.w).range(512..=8192).suffix(" px"));
                    ui.end_row();
                    ui.label("");
                    ui.weak("The height is half the width (up to 8192 × 4096).");
                    ui.end_row();
                } else {
                    ui.label("Size");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut e.w).range(16..=8192).suffix(" px"));
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut e.h).range(16..=8192).suffix(" px"));
                    });
                    ui.end_row();
                }
                if e.kind == RenderKind::Quality {
                    ui.label("Passes");
                    ui.add(egui::Slider::new(&mut e.samples, 1..=128)).on_hover_text("More passes: smoother edges and softer shadows, but slower");
                    ui.end_row();
                    ui.label("Depth of field");
                    ui.horizontal(|ui| {
                        let r = &mut self.view3d.render;
                        let mut on = r.depth_of_field.is_some();
                        if ui.checkbox(&mut on, "").changed() {
                            r.depth_of_field = on.then(|| housecraft_api::model::DepthOfField {
                                focus: housecraft_api::units::Length::from_m(self.view3d.distance as f64),
                                f_stop: 4.0,
                            });
                        }
                        if let Some(d) = &mut r.depth_of_field {
                            let mut m = d.focus.mm() / 1000.0;
                            if ui.add(egui::DragValue::new(&mut m).range(0.2..=500.0).speed(0.1).prefix("focus ").suffix(" m")).changed() {
                                d.focus = housecraft_api::units::Length::from_m(m);
                            }
                            ui.add(egui::DragValue::new(&mut d.f_stop).range(1.0..=32.0).speed(0.1).prefix("f/"));
                        }
                    });
                    ui.end_row();
                }
                if e.kind == RenderKind::TimeLapse {
                    ui.label("Latitude");
                    ui.add(egui::DragValue::new(&mut e.latitude).range(-90.0..=90.0).speed(0.1).suffix("°"));
                    ui.end_row();
                    ui.label("Day of year");
                    ui.add(egui::DragValue::new(&mut e.day).range(1..=365));
                    ui.end_row();
                    ui.label("From / to (hour)");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut e.start).range(0.0..=24.0).speed(0.25));
                        ui.add(egui::DragValue::new(&mut e.end).range(0.0..=24.0).speed(0.25));
                    });
                    ui.end_row();
                    ui.label("Every");
                    ui.add(egui::DragValue::new(&mut e.step).range(1.0..=240.0).suffix(" min"));
                    ui.end_row();
                }
            });
            ui.weak(e.kind.note());
            go = ui.button("Render…").clicked();
        });
        if !go {
            if open {
                self.render_export = Some(e);
            }
            return;
        }
        self.mode = Mode::ThreeD;
        let name = self.doc().name.clone();
        let result = match e.kind {
            RenderKind::Quality | RenderKind::Panorama | RenderKind::Hdr => {
                let ext = if e.kind == RenderKind::Hdr { "hdr" } else { "png" };
                let Some(path) = pick_save_path(&name, ext) else {
                    self.render_export = Some(e);
                    return;
                };
                match e.kind {
                    RenderKind::Quality => crate::view3d::export_quality(self, e.w, e.h, e.samples, &path),
                    RenderKind::Panorama => crate::view3d::export_panorama(self, e.w, &path),
                    _ => crate::view3d::export_hdr(self, e.w, e.h, &path),
                }
                .map(|()| format!("Saved {}", path.display()))
            }
            RenderKind::TimeLapse | RenderKind::Walkthrough(_) => {
                let Some(folder) = rfd::FileDialog::new().set_title("Folder for the frames").pick_folder() else {
                    self.render_export = Some(e);
                    return;
                };
                let n = match e.kind {
                    RenderKind::Walkthrough(id) => match self.doc().get::<Walkthrough>(id).cloned() {
                        Some(w) => crate::view3d::export_walkthrough(self, &w, e.w, e.h, &folder),
                        None => Err("the walkthrough is gone".into()),
                    },
                    _ => crate::view3d::export_time_lapse(self, e.w, e.h, e.latitude, e.day, (e.start, e.end), e.step, &folder),
                };
                n.map(|n| format!("Saved {n} frames in {}", folder.display()))
            }
        };
        match result {
            Ok(msg) => self.info(msg),
            Err(err) => self.error(format!("Render failed: {err}")),
        }
    }

    /// Terrain > Terrain Around Building: a lot 30' bigger than the plan
    /// on every side.
    fn terrain_around_building(&mut self) {
        let doc = self.doc();
        let floor = doc.floors_by_elevation()[0].id;
        let pts = housecraft_api::geom::walls::wall_geoms(doc, floor)
            .into_iter()
            .flat_map(|g| [g.start, g.end]);
        let (lo, hi) = housecraft_api::geom::math::bounds(pts)
            .unwrap_or((glam::DVec2::splat(-6000.0), glam::DVec2::splat(6000.0)));
        use housecraft_api::units::{Length, Point2};
        let pad = Length::from_feet(30.0).mm();
        let p = |x: f64, y: f64| Point2(Length::from_mm(x), Length::from_mm(y));
        self.run1(Command::AddTerrain {
            floor: Some(floor),
            points: vec![
                p(lo.x - pad, lo.y - pad),
                p(hi.x + pad, lo.y - pad),
                p(hi.x + pad, hi.y + pad),
                p(lo.x - pad, hi.y + pad),
            ],
        });
    }

    /// Terrain > Import Survey Points: a text file of x, y, elevation (in
    /// the plan's units) or a GPX file.
    fn import_survey_points(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Survey points", &["txt", "csv", "xyz", "gpx"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.error(format!("Could not read {}: {e}", path.display()));
                return;
            }
        };
        let gpx = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gpx"));
        if let Some(out) = self.run1(Command::ImportTerrainPoints {
            floor: Some(self.plan.floor),
            text,
            gpx,
            unit: None,
        }) {
            self.info(format!("Imported {} points", out.created.len()));
        }
    }

    /// File > Export Settings: wall types, room types, text styles, layers,
    /// materials and molding profiles to share with other plans.
    fn export_settings(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("HouseCraft settings", &["hcsettings"])
            .set_file_name("Settings.hcsettings")
            .save_file()
        else {
            return;
        };
        let hotkeys = serde_json::to_value(&self.prefs.hotkeys).unwrap_or_default();
        match housecraft_io_native::library::export_settings_with(self.doc(), &path, hotkeys) {
            Ok(n) => self.info(format!("Exported {n} settings")),
            Err(e) => self.error(format!("Export failed: {e}")),
        }
    }

    /// File > Import Settings: settings with a name already in the plan
    /// replace it; the rest are added.
    fn import_settings(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("HouseCraft settings", &["hcsettings"])
            .pick_file()
        else {
            return;
        };
        let objects = match housecraft_io_native::library::read_settings(&path) {
            Ok(o) => o,
            Err(e) => {
                self.error(format!("Import failed: {e}"));
                return;
            }
        };
        let mut commands = settings_commands(self.doc(), &objects);
        let mut n = objects.len();
        if let Ok((plan, hotkeys)) = housecraft_io_native::library::read_plan_settings(&path) {
            let doc = self.doc();
            for (name, text) in plan.macros {
                commands.push(Command::SetMacro { name, text: Some(text) });
                n += 1;
            }
            for lt in plan.line_types.into_iter().filter(|l| !doc.settings.line_types.iter().any(|x| x.name == l.name)) {
                commands.push(Command::SetLineType { index: None, line_type: lt });
                n += 1;
            }
            for hp in plan.hatch_patterns.into_iter().filter(|h| !doc.settings.hatch_patterns.iter().any(|x| x.name == h.name)) {
                commands.push(Command::SetHatchPattern { index: None, pattern: hp });
                n += 1;
            }
            // Named dimension styles use this plan's dimension text style.
            let text_style = doc.active_defaults().dimensions.text_style;
            for mut ds in plan.dimension_styles {
                ds.defaults.text_style = text_style;
                commands.push(Command::SetDimensionStyle { name: ds.name, defaults: Some(ds.defaults) });
                n += 1;
            }
            for p in plan.auto_dim_presets {
                commands.push(Command::SetAutoDimPreset { name: p.name, dims: Some(p.dims) });
                n += 1;
            }
            // Layer sets new to this plan, their layers found by name.
            let sets: Vec<serde_json::Value> = plan
                .layer_sets
                .iter()
                .filter(|ls| !doc.iter::<LayerSet>().any(|x| x.name == ls.name))
                .map(|ls| {
                    let layers: serde_json::Map<String, serde_json::Value> = ls
                        .layers
                        .iter()
                        .filter_map(|(name, d)| doc.iter::<Layer>().find(|l| l.name == *name).map(|l| (l.id.0.to_string(), serde_json::to_value(d).unwrap_or_default())))
                        .collect();
                    serde_json::json!({"kind": "layer_set", "id": 0, "name": ls.name, "layers": layers})
                })
                .collect();
            if !sets.is_empty() {
                n += sets.len();
                commands.push(Command::InsertObjects { objects: sets, floor: None, dx: Length::ZERO, dy: Length::ZERO });
            }
            if let Ok(keys) = serde_json::from_value::<Vec<UserHotkey>>(hotkeys) {
                for k in keys {
                    self.prefs.hotkeys.retain(|h| h.keys != k.keys);
                    self.prefs.hotkeys.push(k);
                    n += 1;
                }
            }
        }
        if self.run(commands).is_some() {
            self.info(format!("Imported {n} settings"));
        }
    }

    /// CAD > Blocks > Explode Block.
    pub fn explode_block(&mut self) {
        if let Some(b) = self.selected_block_ref()
            && let Some(out) = self.run1(Command::ExplodeBlock { id: b.id })
        {
            self.selection = out.created.clone();
        }
    }

    /// CAD > Blocks > Save Block to Library.
    pub fn save_block_to_library(&mut self) {
        use housecraft_io_native::library;
        let Some(b) = self.selected_block_ref() else {
            return;
        };
        let doc = self.doc();
        let Some(block) = doc.get::<Block>(b.block) else {
            return;
        };
        let Some(dir) = library::library_dir() else {
            self.error("No library folder (set HOUSECRAFT_LIBRARY)");
            return;
        };
        let saved = library::save_block(
            &dir,
            &library::LibraryBlock {
                name: block.name.clone(),
                shapes: housecraft_api::blocks::block_shapes(doc, block.id),
            },
        );
        match saved {
            Ok(path) => self.info(format!("Saved to {}", path.display())),
            Err(e) => self.error(format!("Could not save the block: {e}")),
        }
    }

    /// Place a library block: add it to the plan (once) and pick the
    /// Place Block tool.
    pub fn place_library_block(&mut self, block: &housecraft_io_native::library::LibraryBlock) {
        if let Some(id) = self.ensure_library_block(block) {
            self.plan.set_tool(Tool::PlaceBlock(id));
            self.palette = Family::Cad;
            self.mode = Mode::Plan;
        }
    }

    /// The plan's block of that name, made from the library's when missing.
    pub fn ensure_library_block(&mut self, block: &housecraft_io_native::library::LibraryBlock) -> Option<Id> {
        let existing = self
            .doc()
            .iter::<Block>()
            .find(|b| b.name == block.name)
            .map(|b| b.id);
        match existing {
            Some(id) => Some(id),
            None => self
                .run1(Command::AddBlock {
                    name: block.name.clone(),
                    ids: Vec::new(),
                    base: None,
                    shapes: block.shapes.clone(),
                })
                .and_then(|o| o.created.first().copied()),
        }
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
        if let Mode::Drawing(_) = self.mode {
            self.copy_layout_boxes();
            return;
        }
        self.clipboard_layout = None;
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
        let mut clip: Vec<serde_json::Value> = ids
            .iter()
            .filter(|id| doc.kind_of(**id).is_some_and(|k| k.is_placed()))
            .filter_map(|id| housecraft_api::query::get_object(doc, *id))
            .collect();
        let n = clip.len();
        // The layers and types they use come along, so pasting into another
        // plan finds (by name) or adds them.
        let mut support: Vec<Id> = Vec::new();
        for v in &clip {
            for key in ["layer", "wall_type", "style", "room_type", "text_style"] {
                if let Some(id) = v.get(key).and_then(|n| n.as_u64()).map(Id)
                    && matches!(doc.kind_of(id), Some(ObjectKind::Layer | ObjectKind::WallType | ObjectKind::TextStyle | ObjectKind::RoomType))
                    && !support.contains(&id)
                {
                    support.push(id);
                }
            }
        }
        let extra: Vec<serde_json::Value> = support.iter().filter_map(|id| housecraft_api::query::get_object(doc, *id)).collect();
        clip.extend(extra);
        self.clipboard = clip;
        self.info(format!(
            "Copied {n} object{}",
            if n == 1 { "" } else { "s" }
        ));
    }

    /// Copy the selected layout boxes with the CAD details and views they
    /// show (used when pasting into another plan).
    fn copy_layout_boxes(&mut self) {
        let doc = self.doc();
        let boxes: Vec<&LayoutBox> = self.selection.iter().filter_map(|id| doc.get::<LayoutBox>(*id)).collect();
        if boxes.is_empty() {
            return;
        }
        let mut ids: Vec<Id> = boxes.iter().map(|b| b.id).collect();
        for b in &boxes {
            let sheet = match b.source {
                LayoutSource::Detail { detail } => Some(detail),
                LayoutSource::View { view } => Some(view),
                _ => None,
            };
            if let Some(sheet) = sheet {
                ids.push(sheet);
                ids.extend(doc.objects.keys().filter(|id| doc.floor_of(**id) == Some(sheet)));
            }
        }
        ids.sort();
        ids.dedup();
        let n = boxes.len();
        let clip: Vec<serde_json::Value> = ids.iter().filter_map(|id| housecraft_api::query::get_object(doc, *id)).collect();
        let origin = (self.path.clone(), doc.name.clone());
        self.clipboard = clip;
        self.clipboard_layout = Some(origin);
        self.info(format!("Copied {n} layout box{}", if n == 1 { "" } else { "es" }));
    }

    /// Paste copied layout boxes onto layout page `page`. From this same
    /// plan they show the same views; from another plan, the CAD details
    /// and views come along and plans show this plan's floors.
    fn paste_layout_boxes(&mut self, layout: Id, page: usize, in_place: bool) {
        let doc = self.doc();
        let same = self.clipboard_layout.as_ref().is_some_and(|(p, name)| *p == self.path && *name == doc.name);
        let first_floor = doc.floors_by_elevation().iter().find(|f| f.foundation.is_none()).map(|f| f.id);
        let offset = if in_place { 0.0 } else { 10.0 };
        let objects: Vec<serde_json::Value> = self
            .clipboard
            .iter()
            .filter_map(|v| {
                let mut v = v.clone();
                if v.get("kind").and_then(|k| k.as_str()) != Some("layout_box") {
                    return (!same).then_some(v);
                }
                v["layout"] = serde_json::json!(layout.0);
                v["page"] = serde_json::json!(page);
                for key in ["x", "y"] {
                    let at = v[key].as_f64().unwrap_or(0.0);
                    v[key] = serde_json::json!(at + offset);
                }
                // A plan from another file shows this one's matching floor.
                if !same
                    && v["source"]["type"] == "plan"
                    && let Some(f) = first_floor
                {
                    let floor = v["source"]["floor"].as_u64().map(Id);
                    if floor.is_none_or(|f| doc.get::<Floor>(f).is_none()) {
                        v["source"]["floor"] = serde_json::json!(f.0);
                        v["source"]["layer_set"] = serde_json::Value::Null;
                    }
                }
                Some(v)
            })
            .collect();
        if let Some(out) = self.run1(Command::InsertObjects {
            objects,
            floor: None,
            dx: housecraft_api::units::Length::ZERO,
            dy: housecraft_api::units::Length::ZERO,
        }) {
            let doc = self.doc();
            self.selection = out.created.into_iter().filter(|id| doc.get::<LayoutBox>(*id).is_some()).collect();
        }
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
        match (self.mode, self.clipboard_layout.is_some()) {
            (Mode::Drawing(crate::drawing_view::DrawingSource::Layout(layout, page)), true) => {
                self.paste_layout_boxes(layout, page, in_place);
                return;
            }
            (_, true) => {
                self.info("Layout boxes paste onto a layout page");
                return;
            }
            (Mode::Drawing(_), false) => {
                self.info("Paste plan objects in a plan or drafting view");
                return;
            }
            _ => {}
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

    /// Wait for a plan click for an edit button (see [`crate::canvas::Pick`]).
    pub fn start_pick(&mut self, pick: crate::canvas::Pick) {
        self.mode = Mode::Plan;
        self.plan.pick = Some(pick);
    }

    /// Select everything on this floor on the layers of the selected objects.
    pub fn select_on_layer(&mut self) {
        let floor = self.plan.floor;
        let doc = self.doc();
        let layers: Vec<Id> = self.selection.iter().filter_map(|id| doc.layer_of(*id)).collect();
        let picked: Vec<Id> = doc
            .objects
            .keys()
            .copied()
            .filter(|id| {
                doc.floor_of(*id) == Some(floor)
                    && doc.kind_of(*id).is_some_and(|k| k.is_placed())
                    && doc.layer_of(*id).is_some_and(|l| layers.contains(&l))
            })
            .collect();
        let n = picked.len();
        self.selection = picked;
        self.info(format!("{n} objects selected"));
    }

    /// Line up or space out the selection by their drawn extents; aligning
    /// goes to the first object selected, distributing keeps the outer two.
    pub fn align(&mut self, how: Align) {
        let boxes: Vec<(Id, DVec2, DVec2)> = self
            .selection
            .iter()
            .filter_map(|id| self.plan.bounds_of(&[*id]).map(|(lo, hi)| (*id, lo, hi)))
            .collect();
        let Some(&(_, rlo, rhi)) = boxes.first() else {
            return;
        };
        let mut moves: Vec<(Id, DVec2)> = Vec::new();
        if how.is_distribute() {
            let x = how == Align::DistributeHorizontally;
            let axis = |v: DVec2| if x { v.x } else { v.y };
            let mut sorted = boxes.clone();
            sorted.sort_by(|a, b| axis((a.1 + a.2) / 2.0).total_cmp(&axis((b.1 + b.2) / 2.0)));
            let (first, last) = (sorted[0], sorted[sorted.len() - 1]);
            let total: f64 = sorted.iter().map(|b| axis(b.2 - b.1)).sum();
            let gap = (axis(last.2) - axis(first.1) - total) / (sorted.len() - 1) as f64;
            let mut at = axis(first.2) + gap;
            for b in &sorted[1..sorted.len() - 1] {
                let d = at - axis(b.1);
                moves.push((b.0, if x { DVec2::new(d, 0.0) } else { DVec2::new(0.0, d) }));
                at += axis(b.2 - b.1) + gap;
            }
        } else {
            for &(id, lo, hi) in &boxes[1..] {
                let d = match how {
                    Align::Left => DVec2::new(rlo.x - lo.x, 0.0),
                    Align::Right => DVec2::new(rhi.x - hi.x, 0.0),
                    Align::CentersVertically => DVec2::new((rlo.x + rhi.x - lo.x - hi.x) / 2.0, 0.0),
                    Align::Top => DVec2::new(0.0, rhi.y - hi.y),
                    Align::Bottom => DVec2::new(0.0, rlo.y - lo.y),
                    Align::CentersHorizontally => DVec2::new(0.0, (rlo.y + rhi.y - lo.y - hi.y) / 2.0),
                    _ => DVec2::ZERO,
                };
                moves.push((id, d));
            }
        }
        let commands: Vec<Command> = moves
            .into_iter()
            .filter(|(_, d)| d.length() > 0.5)
            .map(|(id, d)| Command::Transform {
                ids: vec![id],
                transform: housecraft_api::TransformOp::Translate {
                    dx: Length::from_mm(d.x),
                    dy: Length::from_mm(d.y),
                },
                copies: 0,
            })
            .collect();
        if !commands.is_empty() {
            self.run(commands);
        }
    }

    /// Mirror the whole plan left to right, or turn it by `angle` degrees,
    /// about the middle of the current floor's drawing (every floor moves).
    pub fn transform_plan(&mut self, mirror: bool, angle: f64) {
        let doc = self.doc();
        let ids: Vec<Id> = doc
            .objects
            .keys()
            .copied()
            .filter(|id| {
                doc.kind_of(*id).is_some_and(|k| k.is_placed() && k != ObjectKind::Opening && k != ObjectKind::Room)
            })
            .collect();
        let center = self
            .plan
            .bounds_of(&ids)
            .map_or(DVec2::ZERO, |(lo, hi)| (lo + hi) / 2.0);
        let c = Point2::from_mm(center.x, center.y);
        let transform = if mirror {
            housecraft_api::TransformOp::Mirror { a: c, b: Point2::from_mm(center.x, center.y + 1000.0) }
        } else {
            housecraft_api::TransformOp::Rotate { center: c, angle }
        };
        self.run1(Command::Transform { ids, transform, copies: 0 });
        self.plan.fit_requested = true;
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
        self.close_sheet();
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
        self.time.reset();
        self.tabs = vec![Mode::Plan];
        self.mode = Mode::Plan;
        self.changed();
    }

    /// The current floor's number: the foundation is 0, storeys from 1.
    pub fn floor_number(&self) -> usize {
        self.doc()
            .floors_by_elevation()
            .iter()
            .filter(|f| f.foundation.is_none())
            .position(|f| f.id == self.plan.floor)
            .map_or(0, |i| i + 1)
    }

    /// Remember the current view as a tab; drop tabs whose layout is gone.
    fn keep_tabs(&mut self) {
        let doc = self.session.document();
        self.tabs.retain(|m| match m {
            Mode::Drawing(crate::drawing_view::DrawingSource::Layout(id, _)) => doc.object(*id).is_some(),
            _ => true,
        });
        if !self.tabs.contains(&self.mode) {
            self.tabs.push(self.mode);
        }
    }

    fn tab_name(&self, mode: Mode) -> String {
        let doc = self.doc();
        match mode {
            Mode::Plan => {
                let floor = doc.get::<Floor>(self.plan.floor).map_or("Floor", |f| f.name.as_str());
                let view = self
                    .plan
                    .view
                    .and_then(|v| doc.get::<PlanView>(v))
                    .map_or("Floor Plan", |v| v.name.as_str());
                format!("{view}: {floor}")
            }
            Mode::ThreeD => self
                .view3d
                .camera
                .and_then(|c| doc.get::<Camera>(c))
                .map_or("3D View".to_string(), |c| format!("Camera: {}", c.name)),
            Mode::Drawing(crate::drawing_view::DrawingSource::Layout(id, page)) => {
                let name = doc.get::<Layout>(id).map(|l| l.name.clone()).unwrap_or_default();
                format!("{name}: page {}", page + 1)
            }
        }
    }

    /// The tab strip under the toolbars: one tab per open view.
    fn doc_tabs(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let mut close = None;
            for &tab in &self.tabs.clone() {
                let active = tab == self.mode;
                let name = self.tab_name(tab);
                egui::Frame::new()
                    .fill(if active { ui.visuals().extreme_bg_color } else { ui.visuals().panel_fill })
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .corner_radius(egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 })
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let label = if active { RichText::new(&name).strong() } else { RichText::new(&name) };
                            if ui.add(egui::Label::new(label).sense(egui::Sense::click())).clicked() {
                                match tab {
                                    Mode::Plan => self.show_plan(),
                                    other => self.mode = other,
                                }
                            }
                            if tab != Mode::Plan && ui.small_button("×").on_hover_text("Close").clicked() {
                                close = Some(tab);
                            }
                        });
                    });
            }
            if let Some(tab) = close {
                self.tabs.retain(|t| *t != tab);
                if self.mode == tab {
                    self.show_plan();
                }
            }
        });
    }

    /// The Tool Search box: type part of a tool's name, Enter or click to use it.
    fn tool_search_box(&mut self, ui: &mut Ui) {
        let r = ui.add(
            egui::TextEdit::singleline(&mut self.tool_search)
                .hint_text("Tool Search")
                .desired_width(150.0),
        );
        let query = self.tool_search.trim().to_lowercase();
        if query.is_empty() {
            return;
        }
        let found: Vec<Tool> = all_tools()
            .into_iter()
            .filter(|t| t.name().to_lowercase().contains(&query))
            .take(12)
            .collect();
        let mut chosen = None;
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            chosen = found.first().copied();
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.tool_search.clear();
            return;
        }
        egui::Popup::from_response(&r)
            .open(true)
            .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
            .show(|ui| {
                if found.is_empty() {
                    ui.weak("No tool matches");
                }
                for t in &found {
                    let text = match t.hotkey() {
                        Some(k) => format!("{}    {k}", t.name()),
                        None => t.name().to_string(),
                    };
                    if ui.selectable_label(false, text).clicked() {
                        chosen = Some(*t);
                    }
                }
            });
        if let Some(t) = chosen {
            self.tool_search.clear();
            self.mode = Mode::Plan;
            self.set_tool(t);
        }
    }

    /// Your own shortcuts (Help > Keyboard Shortcuts), checked before the
    /// built-in ones. A two-key shortcut waits up to two seconds for its
    /// second key.
    fn user_hotkeys(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self.chord.as_ref().is_some_and(|(_, t)| now - t > 2.0) {
            self.chord = None;
        }
        if self.prefs.hotkeys.is_empty() {
            return;
        }
        let bindings = self.prefs.hotkeys.clone();
        let mut chord = self.chord.clone();
        let mut fire: Option<String> = None;
        ctx.input_mut(|i| {
            i.events.retain(|e| {
                let egui::Event::Key { key, pressed: true, modifiers, repeat: false, .. } = e else {
                    return true;
                };
                if matches!(key, egui::Key::Escape) {
                    let had = chord.take().is_some();
                    return !had;
                }
                let mut stroke = String::new();
                if modifiers.command {
                    stroke += "CTRL+";
                }
                if modifiers.alt {
                    stroke += "ALT+";
                }
                if modifiers.shift {
                    stroke += "SHIFT+";
                }
                stroke += &key.name().to_uppercase();
                if let Some((first, _)) = chord.take() {
                    let seq = format!("{first} {stroke}");
                    if let Some(b) = bindings.iter().find(|b| b.keys == seq) {
                        fire = Some(b.tool.clone());
                    }
                    return false;
                }
                if bindings.iter().any(|b| b.keys.starts_with(&format!("{stroke} "))) {
                    chord = Some((stroke, now));
                    return false;
                }
                if let Some(b) = bindings.iter().find(|b| b.keys == stroke) {
                    fire = Some(b.tool.clone());
                    return false;
                }
                true
            });
        });
        self.chord = chord;
        if let Some(name) = fire
            && let Some(t) = all_tools().into_iter().find(|t| t.name() == name)
        {
            self.mode = Mode::Plan;
            self.set_tool(t);
        }
    }

    fn template_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut name) = self.template_name.take() else {
            return;
        };
        let mut keep = true;
        egui::Window::new("Save as Template")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("Template name");
                ui.text_edit_singleline(&mut name);
                ui.weak("New plans can start from it: File > New from Template.");
                ui.horizontal(|ui| {
                    if ui.add_enabled(!name.trim().is_empty(), egui::Button::new("Save")).clicked() {
                        let n = name.trim().to_string();
                        self.save_template(&n);
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
        if keep {
            self.template_name = Some(name);
        }
    }

    pub fn new_document(&mut self) {
        if let Some(t) = self.prefs.default_template.clone()
            && let Ok(mut doc) = housecraft_io_native::load_path(&t)
        {
            doc.name = "Untitled".into();
            self.replace_document(doc, None);
            self.plan.fit_requested = true;
            self.info(format!("New plan from template {}", template_label(&t)));
            return;
        }
        self.replace_document(Document::new("Untitled"), None);
        self.info("New plan");
    }

    /// A new, unsaved plan copied from a template file.
    fn new_from_template(&mut self, path: &str) {
        match housecraft_io_native::load_path(path) {
            Ok(mut doc) => {
                doc.name = "Untitled".into();
                self.replace_document(doc, None);
                self.plan.fit_requested = true;
                self.info(format!("New plan from template {}", template_label(path)));
            }
            Err(e) => self.error(format!("Could not open template: {e}")),
        }
    }

    /// Save the plan into the templates folder as `name`.
    fn save_template(&mut self, name: &str) {
        let Some(dir) = housecraft_io_native::library::templates_dir() else {
            self.error("No application-data folder for templates");
            return;
        };
        let stem: String = name.chars().map(|c| if c.is_alphanumeric() || " -_()".contains(c) { c } else { '_' }).collect();
        let path = dir.join(format!("{}.{}", stem.trim(), housecraft_io_native::EXTENSION));
        let mut doc = self.doc().clone();
        doc.name = name.to_string();
        let saved = std::fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|_| housecraft_io_native::save_path(&doc, &path).map_err(|e| e.to_string()));
        match saved {
            Ok(()) => self.info(format!("Saved template {name}")),
            Err(e) => self.error(format!("Could not save template: {e}")),
        }
    }

    /// Every template file, by name.
    fn templates(&self) -> Vec<(String, String)> {
        let Some(dir) = housecraft_io_native::library::templates_dir() else {
            return Vec::new();
        };
        let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(housecraft_io_native::EXTENSION)))
            .map(|p| {
                let s = p.display().to_string();
                (template_label(&s), s)
            })
            .collect();
        out.sort();
        out
    }

    /// Where archive copies of this plan go: a folder beside the plan, or
    /// HouseCraft/Archive for plans not saved yet.
    fn archive_dir(&self) -> Option<PathBuf> {
        match &self.path {
            Some(p) => {
                let stem = p.file_stem()?.to_string_lossy().into_owned();
                Some(p.parent()?.join(format!("{stem} Archive")))
            }
            None => Some(housecraft_io_native::library::app_data_dir()?.join("Archive")),
        }
    }

    /// Every few minutes, copy a changed plan into its archive folder and
    /// keep only the newest few copies.
    fn auto_archive(&mut self, now: f64) {
        let minutes = self.prefs.archive_minutes;
        if minutes == 0 || !self.session.is_dirty() {
            return;
        }
        let (last, rev) = self.last_archive;
        if last == 0.0 {
            self.last_archive = (now, self.rev);
            return;
        }
        if now - last < minutes as f64 * 60.0 || rev == self.rev {
            return;
        }
        self.last_archive = (now, self.rev);
        let Some(dir) = self.archive_dir() else {
            return;
        };
        let stem = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or_else(|| self.doc().name.clone(), |s| s.to_string_lossy().into_owned());
        let path = dir.join(format!("{stem} {}.{}", timestamp(), housecraft_io_native::EXTENSION));
        let saved = std::fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|_| housecraft_io_native::save_path(self.doc(), &path).map_err(|e| e.to_string()));
        match saved {
            Ok(()) => {
                // Keep the newest few (names sort by time).
                let mut old: Vec<PathBuf> = std::fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&format!("{stem} "))))
                    .collect();
                old.sort();
                let keep = self.prefs.archive_keep.max(1) as usize;
                if old.len() > keep {
                    for p in &old[..old.len() - keep] {
                        let _ = std::fs::remove_file(p);
                    }
                }
                self.info(format!("Archived a copy to {}", dir.display()));
            }
            Err(e) => self.error(format!("Auto-archive failed: {e}")),
        }
    }

    /// Bring a DXF drawing's lines, arcs, circles and text in on this
    /// floor (or the sheet being drafted on). PDFs come in the same way,
    /// first page, at 1/4" = 1'-0" (1:50 metric) or `scale`.
    fn import_dxf(&mut self, path: &std::path::Path) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.error(format!("Could not read {}: {e}", path.display()));
                return;
            }
        };
        let is_pdf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        let read = if is_pdf {
            let scale = if crate::util::input_unit(self.doc()) == housecraft_api::units::LengthUnit::Inches { 48.0 } else { 50.0 };
            housecraft_io_native::pdf::read(&bytes, 0, scale)
        } else {
            housecraft_io_native::dxf::read(&String::from_utf8_lossy(&bytes), None)
        };
        let d = match read {
            Ok(d) => d,
            Err(e) => {
                self.error(format!("Could not read {}: {e}", path.display()));
                return;
            }
        };
        let command = Command::ImportDrawing {
            floor: Some(self.plan.target()),
            shapes: d
                .shapes
                .iter()
                .map(|(layer, shape)| housecraft_api::NamedShape { layer: layer.clone(), shape: shape.clone() })
                .collect(),
            texts: d
                .texts
                .iter()
                .map(|t| housecraft_api::NamedText {
                    layer: t.layer.clone(),
                    position: t.at,
                    text: t.text.clone(),
                    height: t.height,
                    rotation: t.rotation,
                })
                .collect(),
            offset: None,
        };
        if self.run1(command).is_some() {
            self.plan.fit_requested = true;
            let skipped = if d.skipped.is_empty() {
                String::new()
            } else {
                format!(" (skipped {})", d.skipped.iter().map(|(n, c)| format!("{c} {n}")).collect::<Vec<_>>().join(", "))
            };
            self.info(format!("Imported {} shapes and {} notes{skipped}", d.shapes.len(), d.texts.len()));
        }
    }

    pub fn open_path(&mut self, path: PathBuf) {
        // A project archive unpacks into a folder beside it, then opens.
        let path = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case(housecraft_io_native::project::ARCHIVE_EXTENSION)) {
            let dest = path.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
            match housecraft_io_native::project::unpack(&path, &dest) {
                Ok(plan) => plan,
                Err(e) => {
                    self.error(format!("Could not unpack {}: {e}", path.display()));
                    return;
                }
            }
        } else {
            path
        };
        // Pictures and textures named relative to the plan are found from its folder.
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::env::set_current_dir(dir);
        }
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
        // Time counted so far goes into the plan's log.
        if let Some(c) = self.time.flush() {
            self.run1(c);
        }
        match housecraft_io_native::save_path(self.doc(), &path) {
            Ok(()) => {
                self.session.mark_saved();
                self.info(format!("Saved {}", path.display()));
                if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                    let _ = std::env::set_current_dir(dir);
                }
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
        let drawing = self.current_drawing().0;
        let imperial = crate::util::input_unit(doc) == housecraft_api::units::LengthUnit::Inches;
        let bytes = if ext == "dxf" {
            housecraft_draft::dxf::to_dxf(doc, &drawing, &housecraft_draft::dxf::DxfOptions { imperial }).into_bytes()
        } else if ext == "pdf" {
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
                ui.menu_button("New from Template", |ui| {
                    let templates = self.templates();
                    if templates.is_empty() {
                        ui.weak("No templates yet: File > Save as Template…");
                    }
                    for (name, path) in &templates {
                        ui.horizontal(|ui| {
                            if ui.button(name).clicked() {
                                self.new_from_template(path);
                                ui.close();
                            }
                            let is_default = self.prefs.default_template.as_deref() == Some(path.as_str());
                            if ui.selectable_label(is_default, "Default").on_hover_text("New Plan starts from this template").clicked() {
                                self.prefs.default_template = (!is_default).then(|| path.clone());
                            }
                        });
                    }
                    if self.prefs.default_template.is_some() {
                        ui.separator();
                        if ui.button("New Plan uses the built-in plan").clicked() {
                            self.prefs.default_template = None;
                        }
                    }
                });
                ui.menu_button("Recent Files", |ui| {
                    if self.prefs.recent.is_empty() && self.prefs.pinned.is_empty() {
                        ui.weak("(none)");
                    }
                    let pinned = self.prefs.pinned.clone();
                    let rest: Vec<String> = self.prefs.recent.iter().filter(|p| !pinned.contains(p)).cloned().collect();
                    for (p, is_pinned) in pinned.iter().map(|p| (p, true)).chain(rest.iter().map(|p| (p, false))) {
                        ui.horizontal(|ui| {
                            let pin = if is_pinned { "Unpin" } else { "Pin" };
                            if ui.small_button(pin).on_hover_text("Keep this file at the top of the list").clicked() {
                                if is_pinned {
                                    self.prefs.pinned.retain(|x| x != p);
                                } else {
                                    self.prefs.pinned.push(p.clone());
                                }
                            }
                            let label = if is_pinned { RichText::new(p).strong() } else { RichText::new(p) };
                            if ui.button(label).clicked() {
                                self.guarded(Pending::OpenPath(PathBuf::from(p)));
                                ui.close();
                            }
                        });
                        if is_pinned && Some(p) == pinned.last() && !rest.is_empty() {
                            ui.separator();
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
                if item(ui, "Save as Template…", "", true) {
                    self.template_name = Some(self.doc().name.clone());
                }
                if item(ui, "Project Information…", "", true) {
                    self.dialogs.project = Some(self.doc().settings.project.clone());
                }
                ui.menu_button("Project Files", |ui| {
                    if item(ui, "Gather Files into Project Folder", "", self.path.is_some()) {
                        self.gather_files();
                    }
                    if item(ui, "Save as Project Archive (.hcproj)…", "", true) {
                        self.save_archive();
                    }
                    if item(ui, "Plan Portfolio…", "", true) {
                        self.dialogs.portfolio.open = true;
                    }
                    if item(ui, "Import From Another HouseCraft Folder…", "", true) {
                        self.migrate_folder();
                    }
                });
                if item(ui, "Show Auto-Archive Folder", "", true)
                    && let Some(dir) = self.archive_dir()
                {
                    let _ = std::fs::create_dir_all(&dir);
                    reveal(&dir);
                }
                ui.separator();
                ui.menu_button("Layout", |ui| {
                    if item(ui, "New Layout", "", true) {
                        self.new_layout();
                    }
                    let layouts: Vec<(Id, String, usize)> = self
                        .doc()
                        .iter::<Layout>()
                        .map(|l| (l.id, l.name.clone(), l.pages.len()))
                        .collect();
                    ui.separator();
                    ui.menu_button("Send to Layout", |ui| {
                        let what = self.current_layout_source();
                        if what.is_none() {
                            ui.weak("Open a floor plan or an elevation first.");
                        }
                        if let Some(source) = what {
                            if layouts.is_empty() && item(ui, "New Layout", "", true) {
                                let id = self.new_layout();
                                self.send_to_layout(id, 0, source.clone());
                            }
                            for (id, name, pages) in &layouts {
                                for p in 0..*pages {
                                    if item(ui, &format!("{name}, page {}", p + 1), "", true) {
                                        self.send_to_layout(*id, p as u32, source.clone());
                                    }
                                }
                            }
                        }
                    });
                    ui.menu_button("Send Schedule to Layout", |ui| {
                        for kind in ScheduleKind::ALL {
                            ui.menu_button(kind.name(), |ui| {
                                for (id, name, pages) in &layouts {
                                    for p in 0..*pages {
                                        if item(ui, &format!("{name}, page {}", p + 1), "", true) {
                                            self.send_to_layout(
                                                *id,
                                                p as u32,
                                                LayoutSource::Schedule {
                                                    schedule_kind: kind,
                                                    floor: None,
                                                },
                                            );
                                        }
                                    }
                                }
                                if layouts.is_empty() {
                                    ui.weak("Create a layout first.");
                                }
                            });
                        }
                    });
                    if let Mode::Drawing(crate::drawing_view::DrawingSource::Layout(id, page)) = self.mode {
                        if item(ui, "Add Revision Table", "", true) {
                            self.send_to_layout(id, page as u32, LayoutSource::Revisions);
                        }
                        if item(ui, "Add Sheet Index", "", true) {
                            self.send_to_layout(id, page as u32, LayoutSource::SheetIndex { layout: id });
                        }
                        if item(ui, "Number Pages A-1, A-2…", "", true) {
                            self.run1(Command::NumberPages { layout: id, prefix: "A-".into(), start: 1 });
                        }
                        ui.menu_button("Delete Boxes", |ui| {
                            let kinds = [
                                ("All", None),
                                ("Plans", Some("plan")),
                                ("Elevations and Sections", Some("view")),
                                ("CAD Details", Some("detail")),
                                ("Schedules", Some("schedule")),
                                ("Notes", Some("text")),
                            ];
                            for (pages, whole) in [("On This Page", false), ("On Every Page", true)] {
                                ui.menu_button(pages, |ui| {
                                    for (label, kind) in kinds {
                                        if ui.button(label).clicked() {
                                            if let Some(out) = self.run1(Command::DeleteLayoutBoxes {
                                                layout: id,
                                                page: (!whole).then_some(page as u32),
                                                kind: kind.map(String::from),
                                            }) {
                                                self.info(format!("{} box(es) deleted", out.deleted.len()));
                                            }
                                            ui.close();
                                        }
                                    }
                                });
                            }
                        });
                    }
                    if let Mode::Drawing(crate::drawing_view::DrawingSource::Layout(id, page)) =
                        self.mode
                        && item(ui, "Add Notes Box", "", true)
                    {
                        self.send_to_layout(
                            id,
                            page as u32,
                            LayoutSource::Text {
                                text: "{project}\n{date}\nLiving area: {living_area}".into(),
                            },
                        );
                    }
                    if let Mode::Drawing(crate::drawing_view::DrawingSource::Layout(id, _)) =
                        self.mode
                        && item(ui, "Export Layout as PDF…", "", true)
                    {
                        self.export_layout_pdf(id);
                    }
                });
                ui.menu_button("Import", |ui| {
                    if item(ui, "Picture…", "", true) {
                        self.import_picture();
                    }
                    if item(ui, "3D Symbol (OBJ, STL)…", "", true)
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("3D models", &["obj", "stl", "gltf", "glb"])
                            .pick_file()
                    {
                        self.symbol_import = Some((path, Default::default()));
                    }
                    if item(ui, "PDF Drawing (vector)…", "", true)
                        && let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file()
                    {
                        self.import_dxf(&path);
                    }
                    if item(ui, "DXF Drawing (2D)…", "", true)
                        && let Some(path) = rfd::FileDialog::new().add_filter("DXF drawing", &["dxf"]).pick_file()
                    {
                        self.import_dxf(&path);
                    }
                });
                ui.menu_button("Export", |ui| {
                    if item(ui, "Export 3D View as Image…", "", true) {
                        self.image_export = Some((3840, 2160, false));
                    }
                    if item(ui, "Export Floor Plan as PDF…", "", true) {
                        self.dialogs.print.open = true;
                    }
                    if item(ui, "Export Floor Plan as SVG…", "", true) {
                        self.export("svg");
                    }
                    if item(ui, "Export Floor Plan as DXF…", "", true) {
                        self.export("dxf");
                    }
                    ui.separator();
                    if item(ui, "Export Settings…", "", true) {
                        self.export_settings();
                    }
                });
                if item(ui, "Import Settings…", "", true) {
                    self.import_settings();
                }
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
                ui.add_enabled_ui(!self.clipboard.is_empty(), |ui| {
                    ui.menu_button("Paste Hold Position to Layer", |ui| {
                        let layers: Vec<(Id, String)> = self.doc().iter::<Layer>().map(|l| (l.id, l.name.clone())).collect();
                        for (id, name) in layers {
                            if ui.button(name).clicked() {
                                self.paste(true);
                                let doc = self.doc();
                                let moves: Vec<Command> = self
                                    .selection
                                    .iter()
                                    .filter(|o| serde_json::to_value(doc.object(**o)).is_ok_and(|v| v.get("layer").is_some()))
                                    .map(|o| Command::Update {
                                        id: *o,
                                        set: serde_json::json!({ "layer": id }).as_object().cloned().unwrap_or_default(),
                                    })
                                    .collect();
                                if !moves.is_empty() {
                                    self.run(moves);
                                }
                                ui.close();
                            }
                        }
                    });
                });
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
                if item(ui, "Select All on Layer", "", has_sel) {
                    self.select_on_layer();
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
                    ui.separator();
                    if item(ui, "Pin Selected in Project Browser", "", !self.selection.is_empty()) {
                        let ids = self.selection.clone();
                        self.run1(Command::PinObjects { ids, pinned: true });
                    }
                    if item(ui, "Scatter Copies…", "", true) {
                        self.dialogs.scatter = Some((20, 0.25, 1));
                    }
                    if item(ui, "Find and Replace Text…", "", true) {
                        self.dialogs.find_replace.open = true;
                    }
                    if item(ui, "Match Properties", "", self.selection.len() == 1) {
                        self.start_pick(crate::canvas::Pick::Match(self.selection[0]));
                    }
                    if item(ui, "Center Object", "", has_sel) {
                        self.start_pick(crate::canvas::Pick::Center(self.selection.clone()));
                    }
                    let view = self.plan.sheet.and_then(|s| self.doc().get::<SectionView>(s).cloned());
                    if item(ui, "Center in View", "", has_sel && view.is_some())
                        && let Some(v) = view
                        && let Some((l, r)) = housecraft_draft::elevation::model_span(self.doc(), &v)
                        && let Some((lo, hi)) = self.plan.bounds_of(&self.selection)
                    {
                        let dx = (l + r) / 2.0 - (lo.x + hi.x) / 2.0;
                        self.run1(Command::Transform {
                            ids: self.selection.clone(),
                            transform: housecraft_api::TransformOp::Translate { dx: Length::from_mm(dx), dy: Length::ZERO },
                            copies: 0,
                        });
                    }
                    ui.menu_button("Align and Distribute", |ui| {
                        for a in Align::ALL {
                            let need = if a.is_distribute() { 3 } else { 2 };
                            if item(ui, a.name(), "", self.selection.len() >= need) {
                                self.align(a);
                            }
                        }
                    });
                });
                ui.menu_button("Entire Plan", |ui| {
                    if item(ui, "Reverse Plan (mirror left to right)", "", true) {
                        self.transform_plan(true, 0.0);
                    }
                    if item(ui, "Rotate Plan 90° Clockwise", "", true) {
                        self.transform_plan(false, -90.0);
                    }
                    if item(ui, "Rotate Plan 90° Counter-clockwise", "", true) {
                        self.transform_plan(false, 90.0);
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
                    let boxes: Vec<Id> = self.selection.iter().copied().filter(|id| self.doc().kind_of(*id) == Some(ObjectKind::Cad)).collect();
                    if item(ui, "Convert Space Plan to Rooms", "", !boxes.is_empty())
                        && self.run1(Command::ConvertSpacePlan { ids: boxes }).is_some()
                    {
                        self.selection.clear();
                        self.info("Walls built round the space-planning boxes");
                    }
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
                ui.menu_button("Roof", |ui| {
                    if item(ui, "Build Roof", "Ctrl+R", true) {
                        self.build_roof();
                    }
                    ui.menu_button("Roof Style", |ui| {
                        for style in RoofStyle::ALL {
                            if item(ui, style.name(), "", true) {
                                self.run1(Command::SetRoofStyle {
                                    floor: None,
                                    style,
                                    pitch: None,
                                    lower_pitch: None,
                                    break_height: None,
                                });
                            }
                        }
                    });
                    if item(ui, "Remove Roof", "", true) {
                        self.run1(Command::RemoveRoof { floor: None });
                    }
                    if tool_item(ui, Tool::RoofPlane) {
                        self.set_tool(Tool::RoofPlane);
                    }
                });
                ui.menu_button("Electrical", |ui| {
                    for tool in Family::Electrical.tools() {
                        if tool_item(ui, *tool) {
                            self.set_tool(*tool);
                        }
                    }
                    ui.separator();
                    if item(ui, "Auto Place Outlets", "", true)
                        && let Some(out) = self.run1(Command::AutoOutlets { floor: Some(self.plan.floor) })
                    {
                        self.info(format!("{} outlets placed", out.created.len()));
                    }
                });
                ui.menu_button("Renovation", |ui| {
                    let has_sel = !self.selection.is_empty();
                    for status in Renovation::ALL {
                        if item(
                            ui,
                            &format!("Mark Selected as {}", status.name()),
                            "",
                            has_sel,
                        ) {
                            let ids = self.renovatable(&self.selection.clone());
                            self.run1(Command::SetRenovation { ids, status });
                        }
                    }
                    ui.separator();
                    if item(ui, "Mark Everything Existing", "", true) {
                        let all: Vec<Id> = self.doc().objects.keys().copied().collect();
                        let ids = self.renovatable(&all);
                        if self
                            .run1(Command::SetRenovation {
                                ids,
                                status: Renovation::Existing,
                            })
                            .is_some()
                        {
                            self.info("Walls, doors, windows and electrical marked existing");
                        }
                    }
                });
                ui.menu_button("Stairs", |ui| {
                    for tool in Family::Stairs.tools() {
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
                ui.menu_button("Framing", |ui| {
                    let floor = Some(self.plan.floor);
                    let parts = |walls, floors, ceilings, roofs| housecraft_api::framing::FramingParts { walls, floors, ceilings, roofs };
                    let mut cmd = None;
                    if item(ui, "Build All Framing", "", true) {
                        cmd = Some(Command::BuildFraming { floor: None, parts: parts(true, true, true, true), hosts: Vec::new() });
                    }
                    if item(ui, "Build Wall Framing (this floor)", "", true) {
                        cmd = Some(Command::BuildFraming { floor, parts: parts(true, false, false, false), hosts: Vec::new() });
                    }
                    if item(ui, "Build Floor and Ceiling Framing (this floor)", "", true) {
                        cmd = Some(Command::BuildFraming { floor, parts: parts(false, true, true, false), hosts: Vec::new() });
                    }
                    if item(ui, "Build Roof Framing", "", true) {
                        cmd = Some(Command::BuildFraming { floor: None, parts: parts(false, false, false, true), hosts: Vec::new() });
                    }
                    let hosts: Vec<Id> = self
                        .selection
                        .iter()
                        .copied()
                        .filter(|id| matches!(self.doc().kind_of(*id), Some(ObjectKind::Wall | ObjectKind::RoofPlane | ObjectKind::Room)))
                        .collect();
                    if item(ui, "Rebuild Framing of Selected Walls, Rooms or Roof Planes", "", !hosts.is_empty()) {
                        cmd = Some(Command::BuildFraming { floor: None, parts: parts(true, false, false, true), hosts: hosts.clone() });
                    }
                    let room = self.selection.iter().copied().find(|id| self.doc().kind_of(*id) == Some(ObjectKind::Room));
                    if item(ui, "Post and Beam Under Selected Room", "", room.is_some())
                        && let Some(room) = room
                    {
                        cmd = Some(Command::AutoPostAndBeam { room, max_span: None, post_spacing: None });
                    }
                    ui.separator();
                    for tool in [Tool::JoistDirection, Tool::Post, Tool::Beam] {
                        if tool_item(ui, tool) {
                            self.set_tool(tool);
                        }
                    }
                    let truss = self.selection.iter().copied().find(|id| {
                        self.doc()
                            .get::<FramingMember>(*id)
                            .is_some_and(|m| matches!(m.role, FramingRole::Chord | FramingRole::Web))
                    });
                    if item(ui, "Truss Detail", "", truss.is_some())
                        && let Some(member) = truss
                        && let Some(out) = self.run1(Command::TrussDetail { member })
                        && let Some(id) = out.created.first()
                    {
                        self.open_sheet(*id);
                    }
                    let wall = (hosts.len() == 1 && self.doc().kind_of(hosts[0]) == Some(ObjectKind::Wall)).then(|| hosts[0]);
                    if item(ui, "Wall Framing Detail", "", wall.is_some())
                        && let Some(wall) = wall
                        && let Some(out) = self.run1(Command::WallFramingDetail { wall })
                        && let Some(id) = out.created.first()
                    {
                        self.open_sheet(*id);
                    }
                    let show = self.framing_visible();
                    if item(ui, if show { "Hide Framing in Plan" } else { "Show Framing in Plan" }, "", self.doc().active_defaults().layers.framing.is_some()) {
                        self.set_framing_visible(!show);
                    }
                    if item(ui, "Framing Overview (3D)", "", true) {
                        use housecraft_api::geom::mesh::Category as C;
                        // Framing in front of a faint shell of walls and roofs.
                        let v = &mut self.view3d;
                        if v.ghosted.contains(&C::Wall) || v.hidden.contains(&C::Wall) {
                            v.hidden = vec![C::Ceiling];
                            v.ghosted.clear();
                        } else {
                            v.hidden = vec![C::Opening, C::Floor, C::Ceiling, C::Foundation, C::Fixture, C::Cabinet, C::Terrain];
                            v.ghosted = vec![C::Wall, C::Roof];
                        }
                        self.mode = Mode::ThreeD;
                    }
                    ui.separator();
                    if item(ui, "Clear Automatic Framing", "", true) {
                        cmd = Some(Command::ClearFraming { floor: None, manual_too: false });
                    }
                    if let Some(c) = cmd
                        && let Some(out) = self.run1(c)
                    {
                        let n = out.created.iter().filter(|id| self.doc().kind_of(**id) == Some(ObjectKind::FramingMember)).count();
                        self.info(format!("{n} framing members built"));
                    }
                });
                ui.menu_button("Cabinets", |ui| {
                    for tool in Family::Cabinets.tools().iter().take(5) {
                        if tool_item(ui, *tool) {
                            self.set_tool(*tool);
                        }
                    }
                    if tool_item(ui, Tool::Backsplash) {
                        self.set_tool(Tool::Backsplash);
                    }
                    ui.separator();
                    if item(ui, "Auto Cabinet Dimensions", "", true) {
                        let floor = Some(self.plan.floor);
                        self.run1(Command::AutoCabinetDimensions { floor });
                    }
                    let d = self.doc().active_defaults();
                    let (set, shown) = (d.id, d.cabinets.show_codes);
                    if item(ui, if shown { "Hide Cabinet Codes" } else { "Show Cabinet Codes (NKBA)" }, "", true) {
                        self.run1(Command::Update {
                            id: set,
                            set: serde_json::json!({ "cabinets": { "show_codes": !shown } })
                                .as_object()
                                .expect("object")
                                .clone(),
                        });
                    }
                    if item(ui, "Auto Fillers", "", true) {
                        let floor = Some(self.plan.floor);
                        self.run1(Command::AutoFillers { floor });
                    }
                    if item(ui, "Auto End Panels", "", true)
                        && let Some(out) = self.run1(Command::AutoEndPanels { floor: Some(self.plan.floor) })
                    {
                        self.info(format!("End panels set on {} cabinet(s)", out.modified.len()));
                    }
                });
                ui.menu_button("Appliances & Fixtures", |ui| {
                    for tool in Family::Cabinets.tools().iter().skip(5) {
                        if tool_item(ui, *tool) {
                            self.set_tool(*tool);
                        }
                    }
                });
            });
            ui.menu_button("Terrain", |ui| {
                for tool in Tool::TERRAIN {
                    if tool_item(ui, tool) {
                        self.set_tool(tool);
                    }
                }
                ui.separator();
                if item(ui, "Import Survey Drawing (DXF)…", "", true)
                    && let Some(path) = rfd::FileDialog::new().add_filter("DXF drawing", &["dxf"]).pick_file()
                {
                    self.import_survey_dxf(&path);
                }
                let cads: Vec<Id> = self.selection.iter().copied().filter(|id| self.doc().kind_of(*id) == Some(ObjectKind::Cad)).collect();
                if item(ui, "Selected Lines to Break Lines", "", !cads.is_empty()) {
                    self.cad_to_break_lines(&cads);
                }
                if item(ui, "Add Subfloor Marker", "", true) {
                    self.add_subfloor_marker();
                }
                let lot = self.selection.iter().copied().find(|id| {
                    self.doc().get::<Cad>(*id).is_some_and(|c| matches!(c.shape, Shape::Polyline { closed: true, .. }))
                });
                ui.add_enabled_ui(lot.is_some(), |ui| {
                    ui.menu_button("Subdivide Selected Lot", |ui| {
                        for n in 2..=8u32 {
                            if ui.button(format!("{n} lots of equal area")).clicked()
                                && let Some(lot) = lot
                            {
                                self.run1(Command::SubdivideLot { lot, count: n, angle: None });
                                ui.close();
                            }
                        }
                    });
                });
                ui.menu_button("Site", |ui| {
                    for tool in Tool::SITE {
                        if tool_item(ui, tool) {
                            self.set_tool(tool);
                        }
                    }
                });
                ui.separator();
                let terrain = self.doc().iter::<Terrain>().next().map(|t| t.id);
                if item(ui, "Terrain Specification", "", terrain.is_some())
                    && let Some(t) = terrain
                {
                    self.selection = vec![t];
                    self.spec.push(t);
                }
                if item(ui, "Terrain Around Building", "", terrain.is_none()) {
                    self.terrain_around_building();
                }
                if item(ui, "Import Survey Points…", "", true) {
                    self.import_survey_points();
                }
                ui.separator();
                if item(ui, "Plot Plan from Bearings…", "", true) {
                    self.plot = Some(("N 0°00'00\" E, 100'
S 90°00'00\" E, 80'
S 0°00'00\" W, 100'
N 90°00'00\" W, 80'".into(), "10'".into()));
                }
                if item(ui, "Site Plan Detail", "", true) {
                    let n = self.doc().count::<CadDetail>() + 1;
                    if let Some(out) = self.run1(Command::SitePlanDetail { name: format!("Site Plan {n}") })
                        && let Some(id) = out.created.first()
                    {
                        self.open_sheet(*id);
                    }
                }
                ui.separator();
                if item(ui, "Clear Terrain", "", terrain.is_some()) {
                    self.run1(Command::ClearTerrain);
                }
            });
            ui.menu_button("Library", |ui| {
                if item(ui, "Library Browser", "", true) {
                    self.side_tab = SideTab::Library;
                }
                if item(ui, "Convert Selection to Symbol", "", !self.selection.is_empty()) {
                    let n = self.doc().count::<SymbolDef>() + 1;
                    if let Some(out) = self.run1(Command::ConvertToSymbol {
                        ids: self.selection.clone(),
                        name: format!("Symbol {n}"),
                    }) {
                        self.selection = out.created.iter().copied().skip(1).collect();
                        self.info("Made a symbol; rename it or add it to the library from its specification");
                    }
                }
            });
            ui.menu_button("3D", |ui| {
                if item(ui, "Material from View", "", self.mode == Mode::ThreeD && self.view3d.last_rect.is_some())
                    && let Some(rect) = self.view3d.last_rect
                {
                    let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
                    let s2 = slot.clone();
                    let c2 = ui.ctx().clone();
                    ui.ctx().request_screenshot(move |img| {
                        if let Ok(mut g) = s2.lock() {
                            *g = Some(img);
                        }
                        c2.request_repaint();
                    });
                    self.capture = Some((slot, rect, ui.ctx().pixels_per_point()));
                }
                if item(ui, "Material Painter", "", true) {
                    self.mode = Mode::ThreeD;
                    self.view3d.painter.get_or_insert_with(|| {
                        crate::view3d::Painter::new(self.session.document())
                    });
                }
                ui.menu_button("Create Camera View", |ui| {
                    for a in CameraAction::ALL {
                        if item(ui, a.name(), a.hotkey(), true) {
                            self.camera_action(*a);
                        }
                    }
                });
                if item(ui, "Sun Angle and North...", "", true) {
                    self.dialogs.sun.open = true;
                }
                ui.menu_button("Walkthroughs", |ui| {
                    let recording = self.view3d.recording.is_some();
                    if item(ui, if recording { "Stop Recording and Save" } else { "Record Walkthrough" }, "", true) {
                        if let Some((keys, _)) = self.view3d.recording.take() {
                            if keys.len() >= 2 {
                                let n = self.doc().count::<Walkthrough>() + 1;
                                self.run1(Command::AddWalkthrough { name: format!("Walkthrough {n}"), keyframes: keys, fps: None });
                            } else {
                                self.error("Move the camera while recording (a keyframe is kept every second)");
                            }
                        } else {
                            self.mode = Mode::ThreeD;
                            self.view3d.recording = Some((Vec::new(), 0.0));
                            self.info("Recording: walk or orbit; choose Stop Recording and Save when done");
                        }
                    }
                    if item(ui, "Create Orbit Walkthrough", "", true) {
                        let n = self.doc().count::<Walkthrough>() + 1;
                        self.run1(Command::OrbitWalkthrough { name: format!("Orbit {n}"), seconds: 20.0 });
                    }
                    let walks: Vec<(Id, String)> = self.doc().iter::<Walkthrough>().map(|w| (w.id, w.name.clone())).collect();
                    if !walks.is_empty() {
                        ui.separator();
                    }
                    for (id, name) in walks {
                        ui.menu_button(name, |ui| {
                            if item(ui, "Play", "", true) {
                                self.mode = Mode::ThreeD;
                                self.view3d.playing = Some((id, ui.input(|i| i.time)));
                            }
                            if item(ui, "Edit", "", true) {
                                self.selection = vec![id];
                                self.spec.push(id);
                            }
                            if item(ui, "Export Frames…", "", true) {
                                self.render_export = Some(RenderExport::new(RenderKind::Walkthrough(id)));
                            }
                        });
                    }
                });
                if item(ui, "Lighting Manager…", "", true) {
                    self.view3d.lighting_window = true;
                }
                ui.menu_button("Notes and Dimensions", |ui| {
                    if item(ui, "3D Note", "", true) {
                        self.mode = Mode::ThreeD;
                        self.view3d.annotate = Some(crate::view3d::Annotate::Note);
                        self.info("Click a surface to pin a note");
                    }
                    if item(ui, "3D Dimension", "", true) {
                        self.mode = Mode::ThreeD;
                        self.view3d.annotate = Some(crate::view3d::Annotate::Dimension(None));
                        self.info("Click two points on surfaces to dimension between them");
                    }
                });
                ui.menu_button("Special Renders", |ui| {
                    for (label, kind) in [
                        ("High-Quality Image…", RenderKind::Quality),
                        ("360° Panorama…", RenderKind::Panorama),
                        ("HDR Image…", RenderKind::Hdr),
                        ("Sun Time-Lapse…", RenderKind::TimeLapse),
                    ] {
                        if item(ui, label, "", true) {
                            self.render_export = Some(RenderExport::new(kind));
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
                ui.menu_button("Elevations and Sections", |ui| {
                    if item(ui, "Cross Section / Elevation Tool", "", true) {
                        self.set_tool(Tool::SectionLine(SectionKind::Section));
                        self.mode = Mode::Plan;
                    }
                    if item(ui, "Elevation Tool", "", true) {
                        self.set_tool(Tool::SectionLine(SectionKind::Elevation));
                        self.mode = Mode::Plan;
                    }
                    if item(ui, "Auto Exterior Elevations", "", true)
                        && let Some(out) = self.run1(Command::AutoElevations {
                            floor: Some(self.plan.floor),
                        })
                    {
                        self.info(format!(
                            "{} elevations added (Project Browser)",
                            out.created.len()
                        ));
                    }
                    let room = self
                        .selection
                        .iter()
                        .copied()
                        .find(|id| self.doc().kind_of(*id) == Some(ObjectKind::Room));
                    if item(ui, "Room Elevations (selected room)", "", room.is_some())
                        && let Some(room) = room
                        && let Some(out) = self.run1(Command::RoomElevations { room })
                        && let Some(first) = out.created.first()
                    {
                        self.open_sheet(*first);
                    }
                });
                if item(
                    ui,
                    "Back to Plan View",
                    "F2",
                    self.mode != Mode::Plan || self.plan.sheet.is_some(),
                ) {
                    self.show_plan();
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
                    let presets = self.doc().settings.auto_dim_presets.clone();
                    if !presets.is_empty() {
                        ui.menu_button("Auto Exterior Dimensions With", |ui| {
                            for p in presets {
                                if ui.button(&p.name).clicked() {
                                    self.run1(Command::AddExteriorDimensions { floor: Some(self.plan.floor), options: Some(p.dims) });
                                    ui.close();
                                }
                            }
                        });
                    }
                    if item(ui, "Auto Dimension Tiers…", "", true) {
                        let d = self.doc().active_defaults().dimensions.auto;
                        self.dialogs.auto_dims = Some((self.doc().settings.auto_dims.get(&self.plan.floor).copied().unwrap_or(d), String::new()));
                    }
                    let view = self.plan.sheet.filter(|s| self.doc().get::<SectionView>(*s).is_some());
                    ui.separator();
                    if item(ui, "Elevation Dimensions", "", view.is_some())
                        && let Some(v) = view
                    {
                        self.run1(Command::ElevationDimensions { view: v });
                    }
                    if item(ui, "Story Pole", "", view.is_some())
                        && let Some(v) = view
                    {
                        self.run1(Command::StoryPole { view: v });
                    }
                    if item(ui, "Framing Reference Markers", "", view.is_some())
                        && let Some(v) = view
                    {
                        self.run1(Command::FramingMarkers { view: v });
                    }
                });
                if tool_item(ui, Tool::Text) {
                    self.set_tool(Tool::Text);
                }
                if tool_item(ui, Tool::TextArrow) {
                    self.set_tool(Tool::TextArrow);
                }
                ui.separator();
                ui.menu_button("CAD Details", |ui| {
                    if item(ui, "New CAD Detail", "", true) {
                        self.new_cad_detail();
                    }
                    if item(ui, "CAD Detail from View", "", true) {
                        self.detail_from_view();
                    }
                    let details: Vec<(Id, String)> = self
                        .doc()
                        .iter::<CadDetail>()
                        .map(|d| (d.id, d.name.clone()))
                        .collect();
                    if !details.is_empty() {
                        ui.separator();
                    }
                    for (id, name) in details {
                        if item(ui, &name, "", true) {
                            self.open_sheet(id);
                        }
                    }
                    ui.separator();
                    let open_detail = self.plan.sheet.filter(|s| self.doc().get::<CadDetail>(*s).is_some());
                    if item(ui, "Copy This CAD Detail", "", open_detail.is_some())
                        && let Some(d) = open_detail
                    {
                        self.copy_cad_detail(d);
                    }
                    let has_detail = self.clipboard.iter().any(|v| v.get("kind").and_then(|k| k.as_str()) == Some("cad_detail"));
                    if item(ui, "Paste CAD Detail", "", has_detail) {
                        self.paste_cad_detail();
                    }
                });
                ui.menu_button("Convert CAD To", |ui| {
                    let cads: Vec<Id> = self.selection.iter().copied().filter(|id| self.doc().kind_of(*id) == Some(ObjectKind::Cad)).collect();
                    let targets: Vec<(&str, housecraft_api::CadTarget)> = vec![
                        ("Exterior Walls", housecraft_api::CadTarget::Walls { interior: false }),
                        ("Interior Walls", housecraft_api::CadTarget::Walls { interior: true }),
                        ("Walls from Double Lines", housecraft_api::CadTarget::DoubleLineWalls),
                        ("Countertop", housecraft_api::CadTarget::Countertop),
                        ("Deck", housecraft_api::CadTarget::Deck),
                        ("Molding", housecraft_api::CadTarget::Molding),
                        ("Sidewalk", housecraft_api::CadTarget::SiteFeature {
                            feature: SiteKind::Road { style: RoadStyle::Sidewalk, width: Length::from_feet(4.0), thickness: Length::from_inches(4.0), curbs: false, median: Length::ZERO, cul_de_sac: Length::ZERO },
                        }),
                        ("Driveway", housecraft_api::CadTarget::SiteFeature {
                            feature: SiteKind::Road { style: RoadStyle::Driveway, width: Length::from_feet(12.0), thickness: Length::from_inches(4.0), curbs: false, median: Length::ZERO, cul_de_sac: Length::ZERO },
                        }),
                        ("Solid (8' tall)", housecraft_api::CadTarget::Solid { height: Some(Length::from_feet(8.0)) }),
                    ];
                    if cads.is_empty() {
                        ui.weak("Select CAD lines or shapes first");
                    }
                    for (label, to) in targets {
                        if item(ui, label, "", !cads.is_empty()) {
                            if let Some(out) = self.run1(Command::ConvertCad { ids: cads.clone(), to }) {
                                self.selection = out.created;
                            }
                            ui.close();
                        }
                    }
                    ui.separator();
                    if item(ui, "Roof Planes to Polylines", "", self.doc().iter::<RoofPlane>().next().is_some()) {
                        let roofs: Vec<Id> = self.selection.iter().copied().filter(|id| self.doc().kind_of(*id) == Some(ObjectKind::RoofPlane)).collect();
                        if let Some(out) = self.run1(Command::RoofPlanesToPolylines { ids: roofs }) {
                            self.info(format!("{} outline(s) drawn on the CAD layer", out.created.len()));
                        }
                    }
                });
                if item(ui, "Hatch Patterns…", "", true) {
                    self.dialogs.hatch.open = true;
                }
                ui.menu_button("Shapes and Solids", |ui| {
                    let cads: Vec<Id> = self
                        .selection
                        .iter()
                        .copied()
                        .filter(|id| self.doc().kind_of(*id) == Some(ObjectKind::Cad))
                        .collect();
                    let two = cads.len() >= 2;
                    let mut cmd = None;
                    for (label, op) in [
                        ("Join Shapes", housecraft_api::ShapeOp::Union),
                        ("Intersect Shapes", housecraft_api::ShapeOp::Intersect),
                        ("Subtract from First Shape", housecraft_api::ShapeOp::Subtract),
                    ] {
                        if item(ui, label, "", two) {
                            cmd = Some(Command::CadBoolean { ids: cads.clone(), operation: op });
                        }
                    }
                    ui.separator();
                    if item(ui, "Extrude to 3D Solid", "", !cads.is_empty()) {
                        cmd = Some(Command::ExtrudeCad { ids: cads.clone(), height: None });
                    }
                    if item(ui, "Revolve to 3D Solid", "", cads.len() == 1) {
                        cmd = Some(Command::RevolveCad { id: cads[0], sweep: None });
                    }
                    if let Some(c) = cmd
                        && let Some(out) = self.run1(c)
                    {
                        self.selection = out.created.clone();
                    }
                });
                {
                    let doc = self.doc();
                    let window = self.selection.iter().copied().find(|id| {
                        doc.get::<Opening>(*id).is_some_and(|o| !o.opening_kind.is_door())
                    });
                    let cads: Vec<Id> = self
                        .selection
                        .iter()
                        .copied()
                        .filter(|id| doc.kind_of(*id) == Some(ObjectKind::Cad))
                        .collect();
                    if item(ui, "Make Window Grille from Selection", "", window.is_some() && !cads.is_empty())
                        && let Some(opening) = window
                    {
                        self.run1(Command::MuntinsFromCad { opening, ids: cads });
                    }
                }
                if item(
                    ui,
                    "Make Molding Profile from Selection",
                    "",
                    self.selection.len() == 1
                        && self.doc().kind_of(self.selection[0]) == Some(ObjectKind::Cad),
                ) {
                    let n = self.doc().count::<MoldingProfile>() + 1;
                    let cad = self.selection[0];
                    if let Some(out) = self.run1(Command::AddMoldingProfile {
                        name: format!("Profile {n}"),
                        points: Vec::new(),
                        from_cad: Some(cad),
                    }) && let Some(id) = out.created.first()
                    {
                        self.spec.push(*id);
                    }
                }
                ui.menu_button("Blocks", |ui| {
                    let cad_selected = self
                        .selection
                        .iter()
                        .any(|id| self.doc().kind_of(*id) == Some(ObjectKind::Cad));
                    if item(ui, "Make Block from Selection", "", cad_selected) {
                        self.make_block();
                    }
                    let one = self.selected_block_ref().is_some();
                    if item(ui, "Explode Block", "", one) {
                        self.explode_block();
                    }
                    if item(ui, "Edit Block", "", one)
                        && let Some(b) = self.selected_block_ref()
                    {
                        let block = b.block;
                        self.open_sheet(block);
                        self.info("Editing the block: changes show in every copy. F2 goes back to the plan.");
                    }
                    if item(ui, "Save Block to Library", "", one) {
                        self.save_block_to_library();
                    }
                    let blocks: Vec<(Id, String)> = self
                        .doc()
                        .iter::<Block>()
                        .map(|b| (b.id, b.name.clone()))
                        .collect();
                    if !blocks.is_empty() {
                        ui.separator();
                        ui.weak("Place:");
                    }
                    for (id, name) in blocks {
                        if item(ui, &name, "", true) {
                            self.set_tool(Tool::PlaceBlock(id));
                        }
                    }
                });
            });
            ui.menu_button("Tools", |ui| {
                if item(ui, "Plan Materials…", "", true) {
                    self.materials.open = true;
                }
                ui.menu_button("Schedules", |ui| {
                    for kind in ScheduleKind::ALL {
                        if item(ui, &format!("{}…", kind.name()), "", true) {
                            self.table_view = Some(crate::schedules::TableSource::Schedule(kind));
                        }
                    }
                    if item(ui, "Materials List…", "", true) {
                        self.table_view = Some(crate::schedules::TableSource::Materials);
                    }
                    ui.separator();
                    ui.menu_button("Place in Plan", |ui| {
                        for kind in ScheduleKind::ALL {
                            if item(ui, kind.name(), "", true) {
                                crate::schedules::place_schedule(self, kind);
                            }
                        }
                    });
                });
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
                if item(ui, "Plan Check…", "", true) {
                    self.dialogs.plan_check = true;
                }
                if item(ui, "Time Tracker…", "", true) {
                    self.time.open = true;
                }
                if item(ui, "Text Macros…", "", true) {
                    self.dialogs.macros.open = true;
                }
                if item(ui, "Line Styles…", "", true) {
                    self.dialogs.line_styles = true;
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
                if item(ui, "Add Standard Plan Views", "", true)
                    && let Some(out) = self.run1(Command::AddStandardPlanViews)
                {
                    let n = out.created.iter().filter(|id| self.doc().kind_of(**id) == Some(ObjectKind::PlanView)).count();
                    self.info(format!("{n} plan views added: pick them in the plan view list"));
                }
                ui.separator();
                ui.checkbox(&mut self.prefs.show_grid, "Reference Grid");
                ui.checkbox(
                    &mut self.plan.reference_floor,
                    "Reference Floor Display (F9)",
                );
                ui.menu_button("Reference Floors", |ui| {
                    if ui
                        .radio(self.plan.references.is_empty(), "Floor Below")
                        .clicked()
                    {
                        self.plan.references.clear();
                        self.plan.reference_floor = true;
                    }
                    let floors: Vec<(Id, String)> = self
                        .doc()
                        .floors_by_elevation()
                        .iter()
                        .filter(|f| f.id != self.plan.floor)
                        .map(|f| (f.id, f.name.clone()))
                        .collect();
                    for (id, name) in floors {
                        let mut on = self.plan.references.contains(&id);
                        if ui.checkbox(&mut on, name).changed() {
                            if on {
                                self.plan.references.push(id);
                            } else {
                                self.plan.references.retain(|f| *f != id);
                            }
                            self.plan.reference_floor = true;
                        }
                    }
                });
                ui.menu_button("Reference Plan File", |ui| crate::project_tools::reference_menu(self, ui));
                ui.checkbox(&mut self.plan.sun_shadows, "Sun Shadows in Plan");
                ui.menu_button("Renovation Phase", |ui| {
                    for p in Phase::ALL {
                        if ui.radio(self.plan.phase == p, p.name()).clicked() {
                            self.plan.phase = p;
                            ui.close();
                        }
                    }
                });
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
                    self.show_plan();
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

    pub fn build_roof(&mut self) {
        if self
            .run1(Command::BuildRoof {
                floor: None,
                auto_rebuild: true,
            })
            .is_some()
        {
            self.info("Roof built; it rebuilds as walls change. Set walls to Full Gable in their specification.");
        }
    }

    pub fn auto_exterior_dimensions(&mut self) {
        self.run1(Command::AddExteriorDimensions {
            floor: Some(self.plan.floor),
            options: None,
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
            // Floor stepper: down, the floor's number, up.
            if icons::button(ui, Icon::FloorDown, false, true, "Down One Floor (Shift+M)").clicked() {
                self.change_floor(false);
            }
            ui.label(RichText::new(self.floor_number().to_string()).strong())
                .on_hover_text("Current floor (0 is the foundation)");
            if icons::button(ui, Icon::FloorUp, false, true, "Up One Floor (Shift+N)").clicked() {
                self.change_floor(true);
            }
            ui.separator();
            self.tool_search_box(ui);
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
                self.mode == Mode::Plan && self.plan.sheet.is_none(),
                true,
                "Plan View (F2)",
            )
            .clicked()
            {
                self.show_plan();
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
            for tool in [
                Tool::SectionLine(SectionKind::Section),
                Tool::SectionLine(SectionKind::Elevation),
            ] {
                let resp = palette_row(ui, tool.icon(), tool.name(), self.plan.tool == tool);
                if resp.on_hover_text(tool.hint()).clicked() {
                    self.set_tool(tool);
                    self.mode = Mode::Plan;
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
        if self.plan.tool == Tool::SpaceBox {
            ui.horizontal(|ui| {
                ui.label("Room type");
                let types: Vec<String> = self.doc().iter::<RoomType>().map(|t| t.name.clone()).collect();
                egui::ComboBox::from_id_salt("space_type")
                    .selected_text(self.plan.space_type.clone())
                    .show_ui(ui, |ui| {
                        for t in types {
                            ui.selectable_value(&mut self.plan.space_type, t.clone(), t);
                        }
                    });
            });
        }
        if self.plan.tool == Tool::RegularPolygon {
            ui.horizontal(|ui| {
                ui.label("Sides");
                ui.add(egui::DragValue::new(&mut self.plan.polygon_sides).range(3..=64));
            });
        }
        if self.palette == Family::Cad
            && palette_row(ui, Icon::Layers, "Layer Painter", matches!(self.plan.pick, Some(crate::canvas::Pick::PaintLayer(_))))
                .on_hover_text("Pick up an object's layer, then click objects to move them onto it")
                .clicked()
        {
            self.set_tool(Tool::Select);
            self.start_pick(crate::canvas::Pick::PaintLayer(None));
        }
        if self.palette == Family::Cad
            && palette_row(ui, Icon::LayerHider, "Layer Hider", self.plan.pick == Some(crate::canvas::Pick::HideLayer))
                .on_hover_text("Click objects in the plan to hide their layers")
                .clicked()
        {
            self.set_tool(Tool::Select);
            self.start_pick(crate::canvas::Pick::HideLayer);
        }
        if self.palette == Family::Dimensions
            && palette_row(ui, Icon::AutoDimension, "Auto Exterior Dimensions", false)
                .on_hover_text("Auto Exterior Dimensions (Shift+A)")
                .clicked()
        {
            self.auto_exterior_dimensions();
        }
        if self.palette == Family::Roofs {
            if palette_row(ui, Icon::Roof, "Build Roof", false)
                .on_hover_text("Build Roof (Ctrl+R)")
                .clicked()
            {
                self.build_roof();
            }
            if palette_row(ui, Icon::Delete, "Remove Roof", false).clicked() {
                self.run1(Command::RemoveRoof { floor: None });
            }
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
            ui.separator();
            if icons::button(ui, Icon::MatchProperties, false, n == 1, "Match Properties: click others to copy onto")
                .clicked()
            {
                self.start_pick(crate::canvas::Pick::Match(self.selection[0]));
            }
            if icons::button(ui, Icon::CenterObject, false, true, "Center Object: then click a wall, line or object")
                .clicked()
            {
                self.start_pick(crate::canvas::Pick::Center(self.selection.clone()));
            }
            if n >= 2 {
                let r = icons::button(ui, Icon::AlignLeft, false, true, "Align and Distribute");
                egui::Popup::menu(&r).show(|ui| {
                    for a in Align::ALL {
                        let need = if a.is_distribute() { 3 } else { 2 };
                        if item(ui, a.name(), "", n >= need) {
                            self.align(a);
                        }
                    }
                });
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
            ui.selectable_value(&mut self.side_tab, SideTab::Library, "Library");
            ui.selectable_value(
                &mut self.side_tab,
                SideTab::ProjectBrowser,
                "Project Browser",
            );
            ui.selectable_value(&mut self.side_tab, SideTab::LayerDisplay, "Layer Display");
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| match self.side_tab {
            SideTab::ProjectBrowser => {
                ui.add(egui::TextEdit::singleline(&mut self.browser_filter).hint_text("Filter by name").desired_width(f32::INFINITY));
                if self.browser_filter.trim().is_empty() {
                    self.project_browser(ui);
                } else {
                    self.browser_matches(ui);
                }
                self.browser_details(ui);
            }
            SideTab::LayerDisplay if self.layer_window => {
                ui.weak("Layer Display is in its own window.");
                if ui.button("Dock It Here").clicked() {
                    self.layer_window = false;
                }
            }
            SideTab::LayerDisplay => {
                if ui.button("Pop Out").on_hover_text("Show the layer options in a window you can move").clicked() {
                    self.layer_window = true;
                }
                self.layer_display(ui);
            }
            SideTab::Library => crate::library_view::show(self, ui),
        });
    }

    /// Everything in the plan whose name or description matches the filter.
    fn browser_matches(&mut self, ui: &mut Ui) {
        let q = self.browser_filter.trim().to_lowercase();
        let found: Vec<housecraft_api::query::ObjectInfo> = housecraft_api::query::list_objects(self.doc(), None)
            .into_iter()
            .filter(|o| o.label.to_lowercase().contains(&q) || o.kind.name().contains(&q))
            .take(200)
            .collect();
        if found.is_empty() {
            ui.weak("Nothing matches.");
        }
        for o in found {
            let resp = ui.selectable_label(self.browser_detail == Some(o.id), format!("{}  {}", o.kind.name(), o.label));
            if resp.clicked() {
                self.browser_detail = Some(o.id);
                if o.kind.is_placed() {
                    if let Some(f) = self.doc().floor_of(o.id) {
                        self.plan.floor = f;
                    }
                    self.selection = vec![o.id];
                    self.mode = Mode::Plan;
                    self.plan.zoom_to_selection = true;
                }
            }
            if resp.double_clicked() {
                self.spec.push(o.id);
            }
        }
    }

    /// The Project Browser's Details pane for the item last clicked there.
    fn browser_details(&mut self, ui: &mut Ui) {
        let Some(id) = self.browser_detail.or_else(|| self.selection.first().copied()) else {
            return;
        };
        let doc = self.doc();
        let Some(object) = doc.object(id) else {
            return;
        };
        ui.separator();
        let mut open_it = false;
        egui::CollapsingHeader::new("Details").default_open(true).show(ui, |ui| {
            let info = housecraft_api::query::describe(doc, object);
            egui::Grid::new("browser_details").num_columns(2).show(ui, |ui| {
                ui.weak("Kind");
                ui.label(info.kind.name());
                ui.end_row();
                ui.weak("Name");
                ui.label(&info.label);
                ui.end_row();
                ui.weak("Id");
                ui.label(format!("#{}", id.0));
                ui.end_row();
                if let Some(f) = doc.floor_of(id).and_then(|f| doc.get::<Floor>(f)) {
                    ui.weak("Floor");
                    ui.label(&f.name);
                    ui.end_row();
                }
                if let Some(l) = doc.layer_of(id).and_then(|l| doc.get::<Layer>(l)) {
                    ui.weak("Layer");
                    ui.label(&l.name);
                    ui.end_row();
                }
                if let Some(i) = doc.settings.object_info.get(&id) {
                    for (k, v) in [("Code", &i.code), ("Description", &i.description), ("Manufacturer", &i.manufacturer)] {
                        if !v.is_empty() {
                            ui.weak(k);
                            ui.label(v);
                            ui.end_row();
                        }
                    }
                }
            });
            open_it = ui.button("Open Object").clicked();
        });
        if open_it {
            self.spec.push(id);
        }
    }

    /// Active Layer Display Options as a floating window (after Pop Out).
    fn layer_window(&mut self, ctx: &egui::Context) {
        if !self.layer_window {
            return;
        }
        let mut open = true;
        egui::Window::new("Active Layer Display Options")
            .open(&mut open)
            .resizable(true)
            .default_width(300.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.layer_display(ui));
            });
        if !open {
            self.layer_window = false;
        }
    }

    fn project_browser(&mut self, ui: &mut Ui) {
        ui.strong(self.doc().name.clone());
        // Pinned objects first.
        let pins: Vec<(Id, String)> = self
            .doc()
            .settings
            .pins
            .iter()
            .filter_map(|id| self.doc().object(*id).map(|o| (*id, housecraft_api::query::describe(self.doc(), o).label)))
            .collect();
        if !pins.is_empty() {
            egui::CollapsingHeader::new("Pinned").default_open(true).show(ui, |ui| {
                for (id, name) in pins {
                    let r = ui.selectable_label(self.selection.contains(&id), name);
                    if r.clicked() {
                        crate::schedules::select(self, &[id]);
                    }
                    r.context_menu(|ui| {
                        if ui.button("Unpin").clicked() {
                            self.run1(Command::PinObjects { ids: vec![id], pinned: false });
                            ui.close();
                        }
                    });
                }
            });
        }
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
                                render: Some(cam.render.clone()),
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
        egui::CollapsingHeader::new("Layouts")
            .default_open(true)
            .show(ui, |ui| {
                let layouts: Vec<Layout> = self.doc().iter::<Layout>().cloned().collect();
                if layouts.is_empty() {
                    ui.weak("File > Layout > New Layout");
                }
                for l in layouts {
                    let header = egui::CollapsingHeader::new(&l.name)
                        .id_salt(("layout", l.id))
                        .default_open(true)
                        .show(ui, |ui| {
                            for (i, page) in l.pages.iter().enumerate() {
                                let src = crate::drawing_view::DrawingSource::Layout(l.id, i);
                                if ui
                                    .selectable_label(self.mode == Mode::Drawing(src), page)
                                    .clicked()
                                {
                                    self.mode = Mode::Drawing(src);
                                }
                            }
                        });
                    header.header_response.context_menu(|ui| {
                        if ui.button("Open Object").clicked() {
                            self.spec.push(l.id);
                            ui.close();
                        }
                        if ui.button("Add Page").clicked() {
                            self.run1(Command::AddLayoutPage {
                                layout: l.id,
                                name: None,
                            });
                            ui.close();
                        }
                        if ui.button("Export PDF…").clicked() {
                            self.export_layout_pdf(l.id);
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            self.run1(Command::Delete { ids: vec![l.id] });
                            ui.close();
                        }
                    });
                }
            });
        egui::CollapsingHeader::new("Elevations and Sections")
            .default_open(true)
            .show(ui, |ui| {
                let views: Vec<(Id, String)> = self
                    .doc()
                    .iter::<SectionView>()
                    .map(|v| (v.id, v.name.clone()))
                    .collect();
                if views.is_empty() {
                    ui.weak("3D > Elevations and Sections");
                }
                for (id, name) in views {
                    let resp = ui.selectable_label(
                        self.mode == Mode::Plan && self.plan.sheet == Some(id),
                        &name,
                    );
                    if resp.clicked() {
                        self.open_sheet(id);
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Open Object").clicked() {
                            self.spec.push(id);
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            self.run1(Command::Delete { ids: vec![id] });
                            ui.close();
                        }
                    });
                }
            });
        egui::CollapsingHeader::new("CAD Details")
            .default_open(true)
            .show(ui, |ui| {
                let details: Vec<(Id, String)> = self
                    .doc()
                    .iter::<CadDetail>()
                    .map(|d| (d.id, d.name.clone()))
                    .collect();
                if details.is_empty() {
                    ui.weak("CAD > CAD Details");
                }
                for (id, name) in details {
                    let resp = ui.selectable_label(
                        self.mode == Mode::Plan && self.plan.sheet == Some(id),
                        &name,
                    );
                    if resp.clicked() {
                        self.open_sheet(id);
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Rename…").clicked() {
                            self.spec.push(id);
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            self.run1(Command::Delete { ids: vec![id] });
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

    /// The ids among `ids` that carry a renovation status.
    fn renovatable(&self, ids: &[Id]) -> Vec<Id> {
        let doc = self.doc();
        ids.iter()
            .copied()
            .filter(|id| {
                matches!(
                    doc.kind_of(*id),
                    Some(ObjectKind::Wall | ObjectKind::Opening | ObjectKind::Electrical)
                )
            })
            .collect()
    }

    /// Create a layout and open its first page.
    pub fn new_layout(&mut self) -> Id {
        let n = self.doc().count::<Layout>() + 1;
        let Some(out) = self.run1(Command::AddLayout {
            name: format!("Layout {n}"),
            paper_width: None,
            paper_height: None,
            pages: 1,
        }) else {
            return Id(0);
        };
        let id = out.created[0];
        self.mode = Mode::Drawing(crate::drawing_view::DrawingSource::Layout(id, 0));
        id
    }

    /// What "Send to Layout" sends from the current view.
    fn current_layout_source(&self) -> Option<LayoutSource> {
        match self.mode {
            Mode::Plan => match self.plan.sheet.and_then(|s| self.doc().kind_of(s)) {
                Some(ObjectKind::SectionView) => Some(LayoutSource::View {
                    view: self.plan.sheet?,
                }),
                Some(ObjectKind::CadDetail) => Some(LayoutSource::Detail {
                    detail: self.plan.sheet?,
                }),
                _ => Some(LayoutSource::Plan {
                    floor: self.plan.floor,
                    layer_set: Some(self.plan.layer_set),
                    phase: self.plan.phase,
                }),
            },
            _ => None,
        }
    }

    pub fn send_to_layout(&mut self, layout: Id, page: u32, source: LayoutSource) {
        if self
            .run1(Command::SendToLayout {
                layout,
                page,
                source,
                x: None,
                y: None,
                width: None,
                height: None,
                scale: None,
                label: None,
            })
            .is_some()
        {
            self.mode = Mode::Drawing(crate::drawing_view::DrawingSource::Layout(
                layout,
                page as usize,
            ));
            self.info("Sent to layout");
        }
    }

    pub fn export_layout_pdf(&mut self, layout: Id) {
        let doc = self.doc();
        let Some(l) = doc.get::<Layout>(layout).cloned() else {
            return;
        };
        let Some(path) = pick_save_path(&l.name, "pdf") else {
            return;
        };
        let pages = housecraft_draft::layout::layout_pages(doc, &l);
        let bytes =
            housecraft_draft::pdf::pages_to_pdf(&pages, l.paper_width.mm(), l.paper_height.mm());
        match std::fs::write(&path, bytes) {
            Ok(()) => self.info(format!("Exported {}", path.display())),
            Err(e) => self.status = Some((format!("Export failed: {e}"), true)),
        }
    }

    /// A requested screen capture has arrived: crop it to the 3D view, save
    /// it as an image and make a material that uses it.
    fn finish_capture(&mut self, ctx: &egui::Context) {
        self.finish_eyedrop(ctx);
        let Some((slot, rect, ppp)) = &self.capture else { return };
        let img = match slot.lock() {
            Ok(mut g) => g.take(),
            Err(_) => None,
        };
        let Some(img) = img else { return };
        let (rect, ppp) = (*rect, *ppp);
        self.capture = None;
        let [w, h] = img.size;
        let x0 = ((rect.left() * ppp) as usize).min(w);
        let y0 = ((rect.top() * ppp) as usize).min(h);
        let x1 = ((rect.right() * ppp) as usize).min(w);
        let y1 = ((rect.bottom() * ppp) as usize).min(h);
        if x1 <= x0 + 8 || y1 <= y0 + 8 {
            self.error("The capture came back empty");
            return;
        }
        let mut bytes = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
        let mut sum = [0u64; 3];
        for y in y0..y1 {
            for x in x0..x1 {
                let c = img.pixels[y * w + x];
                bytes.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
                sum[0] += c.r() as u64;
                sum[1] += c.g() as u64;
                sum[2] += c.b() as u64;
            }
        }
        let n = ((x1 - x0) * (y1 - y0)) as u64;
        let Some(dir) = housecraft_io_native::library::app_data_dir().map(|d| d.join("Captures")) else {
            self.error("No folder to keep the capture in");
            return;
        };
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let path = dir.join(format!("capture-{stamp}.png"));
        let saved = std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| {
            image::RgbaImage::from_raw((x1 - x0) as u32, (y1 - y0) as u32, bytes)
                .ok_or_else(|| "bad image".to_string())?
                .save(&path)
                .map_err(|e| e.to_string())
        });
        if let Err(e) = saved {
            self.error(format!("Could not save the capture: {e}"));
            return;
        }
        let color = housecraft_api::model::Color::rgb((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8);
        let Some(out) = self.run1(Command::AddMaterial { name: format!("Capture {stamp}"), category: Some("Captures".into()), class: None, color: Some(color), copy_from: None }) else {
            return;
        };
        let Some(id) = out.created.first().copied() else { return };
        let aspect = (y1 - y0) as f64 / (x1 - x0) as f64;
        self.run1(Command::Update {
            id,
            set: serde_json::json!({
                "maps": {"color": {"path": path.display().to_string(), "strength": 1.0}},
                "texture_width": housecraft_api::units::Length::from_feet(4.0),
                "texture_height": housecraft_api::units::Length::from_feet(4.0 * aspect),
            })
            .as_object()
            .cloned()
            .unwrap_or_default(),
        });
        self.spec.push(id);
        self.info(format!("Made a material from the view: {}", path.display()));
    }

    /// Contours, break lines and spot heights from a surveyor's DXF.
    fn import_survey_dxf(&mut self, path: &std::path::Path) {
        let text = match std::fs::read(path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => {
                self.error(format!("Could not read {}: {e}", path.display()));
                return;
            }
        };
        let t = match housecraft_io_native::dxf::read_terrain(&text, None) {
            Ok(t) => t,
            Err(e) => {
                self.error(format!("Could not read the DXF: {e}"));
                return;
            }
        };
        let (c, b, p) = (t.contours.len(), t.breaks.len(), t.points.len());
        if c + b + p == 0 {
            self.error("The drawing has no lines or points with heights");
            return;
        }
        let mut shapes: Vec<ElevationShape> = t.contours.into_iter().map(|(points, elevation)| ElevationShape::Line { points, elevation, smooth: false }).collect();
        shapes.extend(t.breaks.into_iter().map(|(points, elevations)| ElevationShape::BreakLine { points, elevations }));
        shapes.extend(t.points.into_iter().map(|(at, elevation)| ElevationShape::Point { at, elevation }));
        if self.run1(Command::AddElevationShapes { floor: Some(self.plan.floor), shapes }).is_some() {
            self.plan.fit_requested = true;
            self.info(format!("Imported {c} contours, {b} break lines and {p} spot heights"));
        }
    }

    /// CAD lines become terrain break lines at the ground's present heights.
    fn cad_to_break_lines(&mut self, cads: &[Id]) {
        let doc = self.doc();
        let surface = doc.iter::<Terrain>().next().and_then(|t| housecraft_api::geom::terrain::surface(doc, t));
        let mut shapes = Vec::new();
        for id in cads {
            let Some(c) = doc.get::<Cad>(*id) else { continue };
            for (pts, _) in housecraft_api::geom::math::shape_polylines(&c.shape) {
                if pts.len() < 2 {
                    continue;
                }
                let z = |p: housecraft_api::geom::DVec2| Length::from_mm(surface.as_ref().map_or(0.0, |s| s.height_at(p)));
                shapes.push(ElevationShape::BreakLine {
                    points: pts.iter().map(|p| Point2::from_mm(p.x, p.y)).collect(),
                    elevations: pts.iter().map(|p| z(*p)).collect(),
                });
            }
        }
        let n = shapes.len();
        let mut cmds = vec![Command::AddElevationShapes { floor: Some(self.plan.floor), shapes }];
        cmds.push(Command::Delete { ids: cads.to_vec() });
        if self.run(cmds).is_some() {
            self.info(format!("{n} break line(s): set each point's height in its specification"));
        }
    }

    /// A subfloor marker just outside the building's first corner.
    fn add_subfloor_marker(&mut self) {
        let doc = self.doc();
        let floor = doc.floors_by_elevation().iter().find(|f| f.foundation.is_none()).map(|f| f.id);
        let corner = floor
            .and_then(|f| housecraft_api::geom::rooms::detect(doc, f).footprints.into_iter().next())
            .and_then(housecraft_api::geom::math::bounds)
            .map_or(housecraft_api::geom::DVec2::ZERO, |(lo, _)| lo - housecraft_api::geom::DVec2::splat(600.0));
        let shape = ElevationShape::Subfloor { at: Point2::from_mm(corner.x, corner.y), below: Length::from_inches(8.0) };
        if let Some(out) = self.run1(Command::AddElevationShapes { floor: Some(self.plan.floor), shapes: vec![shape] }) {
            self.selection = out.created;
            self.info("Subfloor marker added: the ground there sits 8\" under the subfloor (change it in its specification)");
        }
    }

    /// The eyedropper: arm it from a material's specification, take a
    /// capture when the 3D view is clicked, then read that pixel.
    fn finish_eyedrop(&mut self, ctx: &egui::Context) {
        let armed: Option<Id> = ctx.data_mut(|d| d.remove_temp::<u64>(egui::Id::unique("eyedrop_material"))).map(Id);
        if let Some(m) = armed {
            self.eyedrop = Some((m, None));
            self.mode = Mode::ThreeD;
            self.info("Click in the 3D view on the colour to pick");
        }
        if let Some((m, Some(pos))) = self.eyedrop {
            let slot: CaptureSlot = std::sync::Arc::new(std::sync::Mutex::new(None));
            let s2 = slot.clone();
            let c2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                if let Ok(mut g) = s2.lock() {
                    *g = Some(img);
                }
                c2.request_repaint();
            });
            self.eyedrop_capture = Some((slot, pos, ctx.pixels_per_point(), m));
            self.eyedrop = None;
        }
        let Some((slot, pos, ppp, m)) = &self.eyedrop_capture else { return };
        let img = match slot.lock() {
            Ok(mut g) => g.take(),
            Err(_) => None,
        };
        let Some(img) = img else { return };
        let (pos, ppp, m) = (*pos, *ppp, *m);
        self.eyedrop_capture = None;
        let [w, h] = img.size;
        let (x, y) = (((pos.x * ppp) as usize).min(w.saturating_sub(1)), ((pos.y * ppp) as usize).min(h.saturating_sub(1)));
        let c = img.pixels[y * w + x];
        let color = housecraft_api::model::Color::rgb(c.r(), c.g(), c.b());
        if self.run1(Command::Update { id: m, set: serde_json::json!({ "color": color }).as_object().cloned().unwrap_or_default() }).is_some() {
            self.info(format!("Picked {}", serde_json::to_value(color).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()));
        }
    }

    /// Managed project: copy the pictures and textures the plan uses into a
    /// folder beside it and point the plan at the copies, then save.
    fn gather_files(&mut self) {
        let Some(path) = self.path.clone() else { return };
        match housecraft_io_native::project::gather(self.doc(), &path) {
            Ok((doc, report)) => {
                let name = self.doc().name.clone();
                if report.copied > 0 {
                    // Through the session so it can be undone.
                    let objects: Vec<serde_json::Value> = doc.objects.values().filter_map(|o| {
                        let v = serde_json::to_value(o).ok()?;
                        let old = serde_json::to_value(self.doc().object(o.id())?).ok()?;
                        (v != old).then_some(v)
                    }).collect();
                    let cmds: Vec<Command> = objects
                        .into_iter()
                        .filter_map(|v| {
                            let id = Id(v.get("id")?.as_u64()?);
                            let mut set = v.as_object()?.clone();
                            set.remove("id");
                            set.remove("kind");
                            Some(Command::Update { id, set })
                        })
                        .collect();
                    self.run(cmds);
                    self.save(false);
                }
                let missing = if report.missing.is_empty() { String::new() } else { format!("; {} file(s) not found", report.missing.len()) };
                self.info(format!("{name}: {} file(s) gathered into the project folder{missing}", report.copied));
            }
            Err(e) => self.error(format!("Could not gather files: {e}")),
        }
    }

    /// The plan and every file it uses in one archive.
    fn save_archive(&mut self) {
        let Some(out) = pick_save_path(&self.doc().name, housecraft_io_native::project::ARCHIVE_EXTENSION) else { return };
        let dir = self.path.as_ref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf);
        match housecraft_io_native::project::pack(self.doc(), dir.as_deref(), &out) {
            Ok(r) => {
                let missing = if r.missing.is_empty() { String::new() } else { format!("; {} file(s) not found", r.missing.len()) };
                self.info(format!("Saved {} with {} file(s){missing}", out.display(), r.copied));
            }
            Err(e) => self.error(format!("Could not save the archive: {e}")),
        }
    }

    /// Bring the library, templates and prices over from an older install.
    fn migrate_folder(&mut self) {
        let Some(from) = rfd::FileDialog::new().set_title("The other HouseCraft data folder").pick_folder() else { return };
        let Some(to) = housecraft_io_native::library::app_data_dir() else {
            self.error("No HouseCraft data folder here");
            return;
        };
        match housecraft_io_native::project::migrate_data_folder(&from, &to) {
            Ok(n) => {
                self.library.refresh();
                self.info(format!("Copied {n} library, template and price file(s)"));
            }
            Err(e) => self.error(format!("Import failed: {e}")),
        }
    }

    /// Open what a callout links to: a view or detail, a plan view or a layout.
    pub fn open_link(&mut self, id: Id) {
        match self.doc().object(id).cloned() {
            Some(Object::SectionView(_) | Object::CadDetail(_)) => self.open_sheet(id),
            Some(Object::PlanView(v)) => {
                self.show_plan();
                self.apply_plan_view(&v);
            }
            Some(Object::Layout(_)) => self.mode = Mode::Drawing(crate::drawing_view::DrawingSource::Layout(id, 0)),
            // A grid bubble's line, say: select it.
            Some(_) => self.selection = vec![id],
            None => self.info("The callout's link is gone"),
        }
    }

    pub fn apply_plan_view(&mut self, v: &PlanView) {
        self.plan.view = Some(v.id);
        if let Some(f) = v.floor {
            self.plan.floor = f;
        }
        self.plan.layer_set = v.layer_set;
        self.plan.reference_floor = v.reference_floor;
        self.plan.references = v.references.clone();
        self.plan.phase = v.phase;
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
        self.user_hotkeys(ctx);
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
        if consume(ctrl, Key::R) {
            self.build_roof();
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
            self.show_plan();
        }
        if consume(none, Key::F3) {
            self.mode = Mode::ThreeD;
        }
        if consume(none, Key::F6) {
            self.plan.fit_requested = true;
            self.view3d.fit_requested = true;
            self.drawing_view.fit_requested = true;
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
                "3D View".to_string()
            } else if matches!(self.mode, Mode::Drawing(_)) {
                "Layout".to_string()
            } else {
                self.plan.tool.tooltip().replace(" (", " [").replace(')', "]")
            };
            ui.label(tool_name);
            ui.separator();
            ui.label(format!("Floor: {}", self.floor_number()));
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
            } else if let Some((first, _)) = &self.chord {
                ui.strong(format!("{first} … press the second key"));
            } else if self.mode == Mode::Plan
                && let Some(pick) = &self.plan.pick
            {
                ui.strong(pick.hint());
            } else if self.mode == Mode::Plan {
                ui.weak(self.plan.tool.hint());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(p) = self.plan.cursor
                    && self.mode == Mode::Plan
                {
                    let doc = self.doc();
                    ui.monospace(format!("{:.0} x {:.0}", self.view_px.x, self.view_px.y))
                        .on_hover_text("Drawing area size in pixels");
                    ui.separator();
                    let z = doc.get::<Floor>(self.plan.floor).map_or(Length::ZERO, |f| f.elevation);
                    ui.monospace(format!(
                        "X: {}  Y: {}  Z: {}",
                        crate::util::fmt(doc, Length::from_mm(p.x)),
                        crate::util::fmt(doc, Length::from_mm(p.y)),
                        crate::util::fmt(doc, z),
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
        self.finish_capture(&ctx);
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
        self.auto_archive(ctx.input(|i| i.time));
        if ctx.input(|i| i.time) > 0.0 {
            ctx.request_repaint_after(std::time::Duration::from_secs(60));
        }
        self.template_dialog(&ctx);
        self.layer_window(&ctx);
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
        self.keep_tabs();
        egui::Panel::top("doc_tabs").show(ui, |ui| self.doc_tabs(ui));
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
                self.view_px = ui.available_size();
                match self.mode {
                    Mode::Plan => crate::canvas::show(self, ui),
                    Mode::ThreeD => crate::view3d::show(self, ui),
                    Mode::Drawing(source) => crate::drawing_view::show(self, ui, source),
                }
            });
        crate::props::spec_windows(self, &ctx);
        crate::props::room_multi_window(self, &ctx);
        crate::schedules::table_window(self, &ctx);
        crate::dialogs::show(self, &ctx);
        crate::materials::materials_dialog(self, &ctx);
        self.symbol_import_dialog(&ctx);
        self.image_export_dialog(&ctx);
        self.plot_dialog(&ctx);
        self.render_export_dialog(&ctx);
        crate::view3d::lighting_window(self, &ctx);
        self.note3d_dialog(&ctx);
        crate::view3d::render_window(self, &ctx);
        crate::object_painter::window(self, &ctx);
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
            &[housecraft_io_native::EXTENSION, "hcraft", housecraft_io_native::project::ARCHIVE_EXTENSION],
        )
        .pick_file()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn pick_save_path(name: &str, ext: &str) -> Option<PathBuf> {
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
pub fn pick_save_path(_name: &str, _ext: &str) -> Option<PathBuf> {
    None
}

/// Commands bringing settings from another plan in: one with the same kind
/// and name is updated, others are added.
fn settings_commands(doc: &Document, objects: &[Object]) -> Vec<Command> {
    let mut commands = Vec::new();
    let mut new = Vec::new();
    for o in objects {
        let Ok(mut v) = serde_json::to_value(o) else {
            continue;
        };
        let name = v.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string();
        let same = doc
            .objects
            .values()
            .find(|e| e.kind() == o.kind() && serde_json::to_value(e).ok().and_then(|x| x.get("name").cloned()) == Some(serde_json::json!(name)));
        match same {
            Some(e) => {
                if let Some(m) = v.as_object_mut() {
                    m.remove("id");
                    m.remove("kind");
                    // Keep this plan's text style for its layers.
                    m.remove("text_style");
                    commands.push(Command::Update { id: e.id(), set: m.clone() });
                }
            }
            None => new.push(v),
        }
    }
    if !new.is_empty() {
        commands.push(Command::InsertObjects {
            objects: new,
            floor: None,
            dx: Default::default(),
            dy: Default::default(),
        });
    }
    commands
}

/// The kinds of special render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderKind {
    Quality,
    Panorama,
    Hdr,
    TimeLapse,
    Walkthrough(Id),
}

impl RenderKind {
    fn title(self) -> &'static str {
        match self {
            RenderKind::Quality => "High-Quality Image",
            RenderKind::Panorama => "360° Panorama",
            RenderKind::Hdr => "HDR Image",
            RenderKind::TimeLapse => "Sun Time-Lapse",
            RenderKind::Walkthrough(_) => "Walkthrough Frames",
        }
    }

    fn note(self) -> &'static str {
        match self {
            RenderKind::Quality => "Renders the current view many times with tiny changes and averages them.",
            RenderKind::Panorama => "A 360° image from where the camera stands, for panorama viewers.",
            RenderKind::Hdr => "A Radiance .hdr image merged from three exposures.",
            RenderKind::TimeLapse => "One stamped image for each step of the sun across the day, from this view.",
            RenderKind::Walkthrough(_) => "One image per frame; join them into a video with any video editor.",
        }
    }
}

/// A special render's settings.
struct RenderExport {
    kind: RenderKind,
    w: u32,
    h: u32,
    samples: u32,
    latitude: f64,
    day: u32,
    start: f64,
    end: f64,
    step: f64,
}

impl RenderExport {
    fn new(kind: RenderKind) -> Self {
        let (w, h) = match kind {
            RenderKind::Panorama => (4096, 2048),
            RenderKind::TimeLapse | RenderKind::Walkthrough(_) => (1280, 720),
            _ => (1920, 1080),
        };
        RenderExport { kind, w, h, samples: 24, latitude: 40.0, day: 172, start: 6.0, end: 20.0, step: 30.0 }
    }
}

/// Edit > Edit Objects > Align and Distribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    CentersVertically,
    Right,
    Top,
    CentersHorizontally,
    Bottom,
    DistributeHorizontally,
    DistributeVertically,
}

impl Align {
    pub const ALL: [Align; 8] = [
        Align::Left,
        Align::CentersVertically,
        Align::Right,
        Align::Top,
        Align::CentersHorizontally,
        Align::Bottom,
        Align::DistributeHorizontally,
        Align::DistributeVertically,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Align::Left => "Align Left Edges",
            Align::CentersVertically => "Align Centers on a Vertical Line",
            Align::Right => "Align Right Edges",
            Align::Top => "Align Top Edges",
            Align::CentersHorizontally => "Align Centers on a Horizontal Line",
            Align::Bottom => "Align Bottom Edges",
            Align::DistributeHorizontally => "Distribute Evenly Left to Right",
            Align::DistributeVertically => "Distribute Evenly Bottom to Top",
        }
    }

    pub fn is_distribute(self) -> bool {
        matches!(self, Align::DistributeHorizontally | Align::DistributeVertically)
    }
}

/// A template's name: its file name without the extension.
fn template_label(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map_or_else(|| path.to_string(), |s| s.to_string_lossy().into_owned())
}

/// Local time as YYYY-MM-DD HHMMSS (UTC when the offset is unknown).
fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let day = secs % 86_400;
    format!(
        "{} {:02}{:02}{:02}",
        housecraft_draft::macros::today(),
        day / 3600,
        day % 3600 / 60,
        day % 60
    )
}

/// Open a folder in the system file manager.
fn reveal(dir: &std::path::Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
}

#[cfg(test)]
mod hotkey_tests {
    use super::parse_hotkey;

    #[test]
    fn hotkeys_parse_one_or_two_strokes() {
        assert_eq!(parse_hotkey("g").as_deref(), Some("G"));
        assert_eq!(parse_hotkey(" ctrl+shift+l ").as_deref(), Some("CTRL+SHIFT+L"));
        assert_eq!(parse_hotkey("g  w").as_deref(), Some("G W"));
        assert_eq!(parse_hotkey("F5").as_deref(), Some("F5"));
        assert_eq!(parse_hotkey(""), None);
        assert_eq!(parse_hotkey("a b c"), None);
        assert_eq!(parse_hotkey("ctrl+nothing"), None);
    }
}
