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
        match housecraft_io_native::library::export_settings(self.doc(), &path) {
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
        let commands = settings_commands(self.doc(), &objects);
        let n = objects.len();
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
        let existing = self
            .doc()
            .iter::<Block>()
            .find(|b| b.name == block.name)
            .map(|b| b.id);
        let id = match existing {
            Some(id) => Some(id),
            None => self
                .run1(Command::AddBlock {
                    name: block.name.clone(),
                    ids: Vec::new(),
                    base: None,
                    shapes: block.shapes.clone(),
                })
                .and_then(|o| o.created.first().copied()),
        };
        if let Some(id) = id {
            self.plan.set_tool(Tool::PlaceBlock(id));
            self.palette = Family::Cad;
            self.mode = Mode::Plan;
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
        let drawing = self.current_drawing().0;
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
                            .add_filter("3D models", &["obj", "stl"])
                            .pick_file()
                    {
                        self.symbol_import = Some((path, Default::default()));
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
                        .filter(|id| matches!(self.doc().kind_of(*id), Some(ObjectKind::Wall | ObjectKind::RoofPlane)))
                        .collect();
                    if item(ui, "Rebuild Framing of Selected Walls/Roof Planes", "", !hosts.is_empty()) {
                        cmd = Some(Command::BuildFraming { floor: None, parts: parts(true, false, false, true), hosts: hosts.clone() });
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
                        let framing_only = vec![C::Wall, C::Opening, C::Floor, C::Ceiling, C::Roof, C::Foundation, C::Fixture, C::Cabinet, C::Terrain];
                        self.view3d.hidden = if self.view3d.hidden.contains(&C::Wall) { vec![C::Ceiling] } else { framing_only };
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
                });
                if tool_item(ui, Tool::Text) {
                    self.set_tool(Tool::Text);
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
                });
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
            SideTab::Library => crate::library_view::show(self, ui),
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
                "3D View"
            } else if matches!(self.mode, Mode::Drawing(_)) {
                "Elevation / Section"
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
            &[housecraft_io_native::EXTENSION, "hcraft"],
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
