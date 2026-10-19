//! Dialog windows: print, transforms, CAD operations, preferences, defaults,
//! wall types, layers, styles and help.

use egui::{Context, Ui};
use housecraft_api::geom::{DVec2, v2};
use housecraft_api::model::*;
use housecraft_api::units::{DisplayUnit, Length};
use housecraft_api::{Command, TransformOp};
use housecraft_draft::pdf::{Paper, Scale};
use serde_json::json;

use crate::app::{App, Theme};
use crate::util::{fmt, parse};

pub struct PrintDialog {
    pub open: bool,
    pub paper: Paper,
    pub landscape: bool,
    /// 0 = fit, otherwise model units per paper unit.
    pub ratio: f64,
}

impl Default for PrintDialog {
    fn default() -> Self {
        PrintDialog {
            open: false,
            paper: Paper::Letter,
            landscape: true,
            ratio: 0.0,
        }
    }
}

impl PrintDialog {
    pub fn scale(&self) -> Scale {
        if self.ratio > 0.0 {
            Scale::Ratio(self.ratio)
        } else {
            Scale::Fit
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformTab {
    Move,
    Rotate,
    Reflect,
    MultipleCopy,
}

pub struct TransformDialog {
    pub open: bool,
    pub tab: TransformTab,
    dx: String,
    dy: String,
    angle: String,
    axis_vertical: bool,
    copies: u32,
    copy: bool,
}

impl Default for TransformDialog {
    fn default() -> Self {
        TransformDialog {
            open: false,
            tab: TransformTab::Move,
            dx: "0".into(),
            dy: "0".into(),
            angle: "90".into(),
            axis_vertical: true,
            copies: 3,
            copy: false,
        }
    }
}

impl TransformDialog {
    pub fn open_tab(&mut self, tab: TransformTab) {
        self.open = true;
        self.tab = tab;
    }
    pub fn open_multiple(&mut self) {
        self.open_tab(TransformTab::MultipleCopy);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CadOp {
    Offset,
    Fillet,
    Chamfer,
}

#[derive(Default)]
pub struct CadOpDialog {
    op: Option<CadOp>,
    value: String,
}

impl CadOpDialog {
    pub fn open(&mut self, op: CadOp) {
        self.op = Some(op);
        if self.value.is_empty() {
            self.value = "6\"".into();
        }
    }
}

type LayerRow = (String, String, String);

#[derive(Default)]
pub struct WallTypesDialog {
    pub open: bool,
    selected: Option<Id>,
    /// Working copy of the selected type's layers: (name, thickness, material).
    edit: Option<(Id, Vec<LayerRow>)>,
}

pub struct FoundationDialog {
    pub open: bool,
    kind: FoundationKind,
    wall_height: String,
    footing_width: String,
    footing_depth: String,
    auto_rebuild: bool,
}

impl Default for FoundationDialog {
    fn default() -> Self {
        FoundationDialog {
            open: false,
            kind: FoundationKind::Crawlspace,
            wall_height: String::new(),
            footing_width: String::new(),
            footing_depth: String::new(),
            auto_rebuild: true,
        }
    }
}

/// 3D > Sun Angle: sun position (direct or from place and time) and north.
pub struct SunDialog {
    pub open: bool,
    latitude: f64,
    month: usize,
    day: u32,
    hour: f64,
}

impl Default for SunDialog {
    fn default() -> Self {
        SunDialog {
            open: false,
            latitude: 40.0,
            month: 6,
            day: 21,
            hour: 15.0,
        }
    }
}

const MONTHS: [(&str, u32); 12] = [
    ("January", 31),
    ("February", 28),
    ("March", 31),
    ("April", 30),
    ("May", 31),
    ("June", 30),
    ("July", 31),
    ("August", 31),
    ("September", 30),
    ("October", 31),
    ("November", 30),
    ("December", 31),
];

#[derive(Default)]
pub struct Dialogs {
    /// Keys and tool name being entered in Keyboard Shortcuts.
    pub new_hotkey: (String, String),
    pub foundation: FoundationDialog,
    pub sun: SunDialog,
    pub print: PrintDialog,
    pub transform: TransformDialog,
    pub cad_op: CadOpDialog,
    pub wall_types: WallTypesDialog,
    pub prefs: bool,
    pub defaults: bool,
    pub layers: bool,
    pub text_styles: bool,
    pub line_styles: bool,
    /// Name and pattern being typed for a new line style.
    pub new_line_type: (String, String),
    pub room_types: bool,
    pub about: bool,
    pub shortcuts: bool,
    pub save_view: Option<String>,
}

/// Centre of the selection's characteristic points.
fn selection_center(doc: &Document, ids: &[Id]) -> DVec2 {
    let mut pts = Vec::new();
    for id in ids {
        match doc.object(*id) {
            Some(Object::Wall(w)) => pts.extend([v2(w.start), v2(w.end)]),
            Some(Object::Cad(c)) => pts.extend(
                housecraft_api::geom::math::shape_snap_points(&c.shape)
                    .into_iter()
                    .map(|p| p.0),
            ),
            Some(Object::Text(t)) => pts.push(v2(t.position)),
            Some(Object::Dimension(d)) => match &d.geometry {
                DimensionGeometry::Linear { points, .. } => {
                    pts.extend(points.iter().map(|p| v2(*p)))
                }
                DimensionGeometry::Angular { center, .. } => pts.push(v2(*center)),
            },
            _ => {}
        }
    }
    housecraft_api::geom::math::bounds(pts).map_or(DVec2::ZERO, |(lo, hi)| (lo + hi) / 2.0)
}

pub fn show(app: &mut App, ctx: &Context) {
    foundation_dialog(app, ctx);
    sun_dialog(app, ctx);
    print_dialog(app, ctx);
    transform_dialog(app, ctx);
    cad_op_dialog(app, ctx);
    prefs_dialog(app, ctx);
    defaults_dialog(app, ctx);
    wall_types_dialog(app, ctx);
    layers_dialog(app, ctx);
    list_dialog::<TextStyle>(
        app,
        ctx,
        "Text Styles",
        |a| &mut a.dialogs.text_styles,
        || Command::AddTextStyle {
            name: "New Style".into(),
            height: Length::from_inches(6.0),
            font: None,
            bold: false,
            italic: false,
            color: None,
        },
    );
    list_dialog::<RoomType>(
        app,
        ctx,
        "Room Types",
        |a| &mut a.dialogs.room_types,
        || Command::AddRoomType {
            name: "New Room Type".into(),
            ceiling_height: None,
            floor_finish: String::new(),
            living_area: true,
        },
    );
    save_view_dialog(app, ctx);
    line_styles_dialog(app, ctx);
    shortcuts_dialog(app, ctx);
    about_dialog(app, ctx);
}

fn print_dialog(app: &mut App, ctx: &Context) {
    let mut open = app.dialogs.print.open;
    let mut export = false;
    egui::Window::new("Print / Export PDF")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let d = &mut app.dialogs.print;
            egui::Grid::new("print").num_columns(2).show(ui, |ui| {
                ui.label("Paper");
                egui::ComboBox::from_id_salt("paper")
                    .selected_text(format!("{:?}", d.paper))
                    .show_ui(ui, |ui| {
                        for p in [Paper::Letter, Paper::Legal, Paper::Tabloid, Paper::ArchC, Paper::ArchD, Paper::A4, Paper::A3, Paper::A2, Paper::A1] {
                            ui.selectable_value(&mut d.paper, p, format!("{p:?}"));
                        }
                    });
                ui.end_row();
                ui.label("Orientation");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut d.landscape, true, "Landscape");
                    ui.radio_value(&mut d.landscape, false, "Portrait");
                });
                ui.end_row();
                ui.label("Drawing Scale");
                let label = |r: f64| {
                    if r == 0.0 {
                        "Fit to page".to_string()
                    } else {
                        housecraft_draft::pdf::scale_label(r, true)
                    }
                };
                egui::ComboBox::from_id_salt("scale")
                    .selected_text(label(d.ratio))
                    .show_ui(ui, |ui| {
                        for r in [0.0, 12.0, 16.0, 24.0, 48.0, 96.0, 192.0] {
                            ui.selectable_value(&mut d.ratio, r, label(r));
                        }
                    });
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.weak("Prints the current floor with the active layer set. Open the PDF to send it to a printer.");
            if ui.button("Export PDF…").clicked() {
                export = true;
            }
        });
    app.dialogs.print.open = open && !export;
    if export {
        app.export("pdf");
    }
}

fn transform_dialog(app: &mut App, ctx: &Context) {
    let mut open = app.dialogs.transform.open;
    let mut command = None;
    egui::Window::new("Transform / Replicate")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let doc = app.session.document();
            let ids = app.selection.clone();
            let t = &mut app.dialogs.transform;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut t.tab, TransformTab::Move, "Move");
                ui.selectable_value(&mut t.tab, TransformTab::Rotate, "Rotate");
                ui.selectable_value(&mut t.tab, TransformTab::Reflect, "Reflect");
                ui.selectable_value(&mut t.tab, TransformTab::MultipleCopy, "Multiple Copy");
            });
            ui.separator();
            if ids.is_empty() {
                ui.weak("Select objects first.");
                return;
            }
            let center = selection_center(doc, &ids);
            let op = match t.tab {
                TransformTab::Move | TransformTab::MultipleCopy => {
                    egui::Grid::new("move").num_columns(2).show(ui, |ui| {
                        ui.label("X distance");
                        ui.text_edit_singleline(&mut t.dx);
                        ui.end_row();
                        ui.label("Y distance");
                        ui.text_edit_singleline(&mut t.dy);
                        ui.end_row();
                        if t.tab == TransformTab::MultipleCopy {
                            ui.label("Number of copies");
                            ui.add(egui::DragValue::new(&mut t.copies).range(1..=500));
                            ui.end_row();
                        }
                    });
                    ui.weak("Arithmetic is allowed, e.g. 12'6\" + 3\".");
                    match (parse(doc, &t.dx), parse(doc, &t.dy)) {
                        (Some(dx), Some(dy)) => Some(TransformOp::Translate { dx, dy }),
                        _ => None,
                    }
                }
                TransformTab::Rotate => {
                    ui.horizontal(|ui| {
                        ui.label("Angle (° counter-clockwise)");
                        ui.text_edit_singleline(&mut t.angle);
                    });
                    ui.weak("Rotates about the centre of the selection.");
                    t.angle
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .map(|angle| TransformOp::Rotate {
                            center: housecraft_api::geom::pt(center),
                            angle,
                        })
                }
                TransformTab::Reflect => {
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut t.axis_vertical, true, "About vertical axis");
                        ui.radio_value(&mut t.axis_vertical, false, "About horizontal axis");
                    });
                    let d = if t.axis_vertical { DVec2::Y } else { DVec2::X };
                    Some(TransformOp::Mirror {
                        a: housecraft_api::geom::pt(center),
                        b: housecraft_api::geom::pt(center + d * 1000.0),
                    })
                }
            };
            if t.tab != TransformTab::MultipleCopy {
                ui.checkbox(&mut t.copy, "Make a copy (keep the original)");
            }
            ui.add_space(4.0);
            if ui
                .add_enabled(op.is_some(), egui::Button::new("Apply"))
                .clicked()
                && let Some(op) = op
            {
                let copies = match t.tab {
                    TransformTab::MultipleCopy => t.copies,
                    _ if t.copy => 1,
                    _ => 0,
                };
                command = Some(Command::Transform {
                    ids,
                    transform: op,
                    copies,
                });
            }
        });
    app.dialogs.transform.open = open;
    if let Some(c) = command {
        app.run1(c);
    }
}

fn cad_op_dialog(app: &mut App, ctx: &Context) {
    let Some(op) = app.dialogs.cad_op.op else {
        return;
    };
    let mut open = true;
    let mut command = None;
    let (title, label) = match op {
        CadOp::Offset => ("Offset", "Distance (positive = left / outward)"),
        CadOp::Fillet => ("Fillet", "Radius (0 joins the corner)"),
        CadOp::Chamfer => ("Chamfer", "Distance"),
    };
    egui::Window::new(title)
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let doc = app.session.document();
            ui.horizontal(|ui| {
                ui.label(label);
                ui.text_edit_singleline(&mut app.dialogs.cad_op.value);
            });
            let value = parse(doc, &app.dialogs.cad_op.value);
            let sel = &app.selection;
            let ready = value.is_some()
                && if op == CadOp::Offset {
                    sel.len() == 1
                } else {
                    sel.len() == 2
                };
            if !ready {
                ui.weak(if op == CadOp::Offset {
                    "Select one line, polyline, circle, arc or wall."
                } else {
                    "Select two lines."
                });
            }
            if ui.add_enabled(ready, egui::Button::new("Apply")).clicked()
                && let Some(v) = value
            {
                command = Some(match op {
                    CadOp::Offset => Command::Offset {
                        id: sel[0],
                        distance: v,
                    },
                    CadOp::Fillet => Command::Fillet {
                        a: sel[0],
                        b: sel[1],
                        radius: v,
                    },
                    CadOp::Chamfer => Command::Chamfer {
                        a: sel[0],
                        b: sel[1],
                        distance: v,
                    },
                });
            }
        });
    if command.is_some() || !open {
        app.dialogs.cad_op.op = None;
    }
    if let Some(c) = command {
        app.run1(c);
    }
}

fn prefs_dialog(app: &mut App, ctx: &Context) {
    let mut open = app.dialogs.prefs;
    let mut unit_cmd = None;
    let mut theme_changed = false;
    let mut file_type_result: Option<std::io::Result<String>> = None;
    egui::Window::new("Preferences")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let doc = app.session.document();
            ui.heading("Appearance");
            ui.horizontal(|ui| {
                for (t, name) in [
                    (Theme::System, "Follow system"),
                    (Theme::Light, "Light"),
                    (Theme::Dark, "Dark"),
                ] {
                    if ui.radio(app.prefs.theme == t, name).clicked() {
                        app.prefs.theme = t;
                        theme_changed = true;
                    }
                }
            });
            ui.checkbox(&mut app.prefs.show_grid, "Show reference grid (graph paper)");
            ui.checkbox(&mut app.prefs.paper_canvas, "Plans on white paper in every theme");
            ui.checkbox(&mut app.prefs.show_axes, "Show origin marker and X/Y/Z axes");
            ui.separator();
            ui.heading("Auto-archive");
            ui.horizontal(|ui| {
                ui.label("Save a copy of a changed plan every");
                ui.add(egui::DragValue::new(&mut app.prefs.archive_minutes).range(0..=240).suffix(" min"));
            });
            ui.horizontal(|ui| {
                ui.label("Keep the newest");
                ui.add(egui::DragValue::new(&mut app.prefs.archive_keep).range(1..=100));
                ui.label("copies (0 minutes turns archiving off)");
            });
            ui.weak("Copies go in a \"<plan> Archive\" folder beside the plan.");
            ui.separator();
            ui.heading("Snapping");
            let s = &mut app.prefs.snap;
            ui.checkbox(&mut s.objects, "Object snaps (Shift+F11)");
            ui.checkbox(&mut s.angles, "Angle snaps (F10)");
            ui.checkbox(&mut s.grid, "Grid snaps (Ctrl+F9)");
            egui::Grid::new("snap_grid").num_columns(2).show(ui, |ui| {
                ui.label("Angle increment (°)");
                ui.add(egui::DragValue::new(&mut s.angle_step).range(1.0..=90.0));
                ui.end_row();
                ui.label("Grid spacing");
                let mut g = fmt(doc, Length::from_mm(s.grid_mm));
                if ui.text_edit_singleline(&mut g).lost_focus()
                    && let Some(l) = parse(doc, &g)
                    && l.mm() > 0.0
                {
                    s.grid_mm = l.mm();
                }
                ui.end_row();
                ui.label("Length snap increment");
                let mut l = fmt(doc, Length::from_mm(s.length_step_mm));
                if ui.text_edit_singleline(&mut l).lost_focus()
                    && let Some(v) = parse(doc, &l)
                    && v.mm() >= 0.0
                {
                    s.length_step_mm = v.mm();
                }
                ui.end_row();
            });
            ui.separator();
            ui.heading("File Types");
            let ext = crate::associate::EXTENSION;
            match crate::associate::status() {
                crate::associate::Status::Registered => {
                    ui.label(format!(".{ext} files open in this copy of HouseCraft."));
                    if ui.button("Remove association").clicked() {
                        file_type_result = Some(
                            crate::associate::unregister()
                                .map(|()| "File association removed".to_string()),
                        );
                    }
                }
                crate::associate::Status::ByPackage => {
                    ui.label(format!(
                        ".{ext} files are associated by the installed HouseCraft app."
                    ));
                }
                status => {
                    ui.label(if status == crate::associate::Status::OtherExecutable {
                        format!(".{ext} files open in a different HouseCraft build.")
                    } else {
                        format!(".{ext} files are not associated with HouseCraft yet.")
                    });
                    if ui
                        .button(format!("Open .{ext} files with HouseCraft"))
                        .clicked()
                    {
                        file_type_result = Some(crate::associate::register().map(|()| {
                            format!("Double-clicking a .{ext} file now opens HouseCraft")
                        }));
                    }
                }
            }
            ui.separator();
            ui.heading("Units (saved with the plan)");
            let current = doc.settings.display_unit;
            let options = [
                (
                    DisplayUnit::FeetInches { denominator: 16 },
                    "Feet and inches, 1/16\"",
                ),
                (
                    DisplayUnit::FeetInches { denominator: 32 },
                    "Feet and inches, 1/32\"",
                ),
                (
                    DisplayUnit::FeetInches { denominator: 128 },
                    "Feet and inches, 1/128\"",
                ),
                (DisplayUnit::Inches { denominator: 16 }, "Inches, 1/16\""),
                (DisplayUnit::Millimeters, "Millimeters"),
                (DisplayUnit::Meters { decimals: 3 }, "Meters"),
            ];
            for (u, name) in options {
                if ui.radio(current == u, name).clicked() && current != u {
                    unit_cmd = Some(Command::SetDisplayUnit { unit: u });
                }
            }
        });
    app.dialogs.prefs = open;
    if theme_changed {
        app.apply_theme(ctx);
    }
    match file_type_result {
        Some(Ok(msg)) => app.info(msg),
        Some(Err(e)) => app.error(format!("File association failed: {e}")),
        None => {}
    }
    if let Some(c) = unit_cmd {
        app.run1(c);
    }
}

fn defaults_dialog(app: &mut App, ctx: &Context) {
    let mut open = app.dialogs.defaults;
    let mut commands = Vec::new();
    egui::Window::new("Default Settings")
        .open(&mut open)
        .resizable(true)
        .default_width(420.0)
        .show(ctx, |ui| {
            let doc = app.session.document();
            let d = doc.active_defaults().clone();
            let id = d.id;
            let p = |v: serde_json::Value| Command::Update {
                id,
                set: v.as_object().cloned().unwrap_or_default(),
            };
            ui.horizontal(|ui| {
                ui.label("Default set:");
                egui::ComboBox::from_id_salt("default_set")
                    .selected_text(d.name.clone())
                    .show_ui(ui, |ui| {
                        for s in doc.iter::<DefaultSet>() {
                            if ui.selectable_label(s.id == id, &s.name).clicked() && s.id != id {
                                commands.push(Command::SetActiveDefaultSet { id: s.id });
                            }
                        }
                    });
                if ui.button("Copy as new set").clicked() {
                    commands.push(Command::AddDefaultSet {
                        name: format!("{} copy", d.name),
                        copy_from: Some(id),
                    });
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Camera Default Sets")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.weak(
                            "The default set each kind of view uses when you draw dimensions,                              text and lines in it.",
                        );
                        let vd = doc.settings.view_defaults;
                        egui::Grid::new("view_defaults").num_columns(2).show(ui, |ui| {
                            for (kind, label) in [
                                (ViewKind::Elevation, "Elevations"),
                                (ViewKind::Section, "Cross sections"),
                                (ViewKind::Detail, "CAD details"),
                            ] {
                                ui.label(label);
                                let current = vd.get(kind);
                                let name = current
                                    .and_then(|c| doc.get::<DefaultSet>(c))
                                    .map_or("Active default set".to_string(), |s| s.name.clone());
                                egui::ComboBox::from_id_salt(("view_defaults", label))
                                    .selected_text(name)
                                    .show_ui(ui, |ui| {
                                        if ui
                                            .selectable_label(current.is_none(), "Active default set")
                                            .clicked()
                                            && current.is_some()
                                        {
                                            commands.push(Command::SetViewDefaults {
                                                view_kind: kind,
                                                default_set: None,
                                            });
                                        }
                                        for s in doc.iter::<DefaultSet>() {
                                            if ui.selectable_label(current == Some(s.id), &s.name).clicked()
                                                && current != Some(s.id)
                                            {
                                                commands.push(Command::SetViewDefaults {
                                                    view_kind: kind,
                                                    default_set: Some(s.id),
                                                });
                                            }
                                        }
                                    });
                                ui.end_row();
                            }
                        });
                    });
                egui::CollapsingHeader::new("Walls")
                    .default_open(true)
                    .show(ui, |ui| {
                        pick_row::<WallType>(
                            ui,
                            doc,
                            "Exterior wall type",
                            d.exterior_wall_type,
                            |v| commands.push(p(json!({"exterior_wall_type": v}))),
                        );
                        pick_row::<WallType>(
                            ui,
                            doc,
                            "Interior wall type",
                            d.interior_wall_type,
                            |v| commands.push(p(json!({"interior_wall_type": v}))),
                        );
                        ui.horizontal(|ui| {
                            ui.label("Draw walls along");
                            let mut r = d.wall_reference;
                            egui::ComboBox::from_id_salt("wall_reference")
                                .selected_text(r.name())
                                .show_ui(ui, |ui| {
                                    for v in WallReference::ALL {
                                        ui.selectable_value(&mut r, v, v.name());
                                    }
                                });
                            if r != d.wall_reference {
                                commands.push(p(json!({"wall_reference": r})));
                            }
                        });
                    });
                egui::CollapsingHeader::new("Doors")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("door_defaults")
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("Style");
                                let mut s = d.door.style;
                                egui::ComboBox::from_id_salt("door_style")
                                    .selected_text(s.name())
                                    .show_ui(ui, |ui| {
                                        for v in DoorStyle::ALL {
                                            ui.selectable_value(&mut s, v, v.name());
                                        }
                                    });
                                if s != d.door.style {
                                    commands.push(p(json!({"door": {"style": s}})));
                                }
                                ui.end_row();
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Width",
                                    ("door", "width"),
                                    d.door.width,
                                    |v| commands.push(p(json!({"door": {"width": v}}))),
                                );
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Height",
                                    ("door", "height"),
                                    d.door.height,
                                    |v| commands.push(p(json!({"door": {"height": v}}))),
                                );
                            });
                        egui::CollapsingHeader::new("Sizes by door type")
                            .id_salt("door_sizes")
                            .show(ui, |ui| {
                                egui::Grid::new("door_size_grid")
                                    .num_columns(3)
                                    .show(ui, |ui| {
                                        ui.weak("Type");
                                        ui.weak("Width");
                                        ui.weak("Height");
                                        ui.end_row();
                                        for style in DoorStyle::ALL {
                                            let (w, h) = d.door.size_for(style);
                                            ui.label(style.name());
                                            let mut set = |w: Length, h: Length| {
                                                let mut sizes = d.door.sizes.clone();
                                                sizes.retain(|s| s.style != style);
                                                sizes.push(DoorSize {
                                                    style,
                                                    width: w,
                                                    height: h,
                                                });
                                                commands.push(p(json!({"door": {"sizes": sizes}})));
                                            };
                                            let kw =
                                                egui::Id::unique(("door_size_w", style.name()));
                                            if let Some(v) = app.bufs.length(ui, doc, kw, w)
                                                && v.mm() > 0.0
                                            {
                                                set(v, h);
                                            }
                                            let kh =
                                                egui::Id::unique(("door_size_h", style.name()));
                                            if let Some(v) = app.bufs.length(ui, doc, kh, h)
                                                && v.mm() > 0.0
                                            {
                                                set(w, v);
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });
                    });
                egui::CollapsingHeader::new("Windows")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("window_defaults")
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("Style");
                                let mut s = d.window.style;
                                egui::ComboBox::from_id_salt("window_style")
                                    .selected_text(s.name())
                                    .show_ui(ui, |ui| {
                                        for v in WindowStyle::ALL {
                                            ui.selectable_value(&mut s, v, v.name());
                                        }
                                    });
                                if s != d.window.style {
                                    commands.push(p(json!({"window": {"style": s}})));
                                }
                                ui.end_row();
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Width",
                                    ("window", "width"),
                                    d.window.width,
                                    |v| commands.push(p(json!({"window": {"width": v}}))),
                                );
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Height",
                                    ("window", "height"),
                                    d.window.height,
                                    |v| commands.push(p(json!({"window": {"height": v}}))),
                                );
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Top height",
                                    ("window", "head"),
                                    d.window.head_height,
                                    |v| commands.push(p(json!({"window": {"head_height": v}}))),
                                );
                            });
                    });
                egui::CollapsingHeader::new("Roofs")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("roof_defaults")
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("Pitch");
                                let mut text = crate::util::fmt_pitch(d.roof.pitch);
                                if ui.text_edit_singleline(&mut text).lost_focus()
                                    && let Some(deg) = crate::util::parse_pitch(&text)
                                    && (deg - d.roof.pitch).abs() > 1e-9
                                {
                                    commands.push(p(json!({"roof": {"pitch": deg}})));
                                }
                                ui.end_row();
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Eave overhang",
                                    ("roof", "overhang"),
                                    d.roof.overhang,
                                    |v| commands.push(p(json!({"roof": {"overhang": v}}))),
                                );
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Rake overhang",
                                    ("roof", "rake"),
                                    d.roof.rake_overhang,
                                    |v| commands.push(p(json!({"roof": {"rake_overhang": v}}))),
                                );
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Roof thickness",
                                    ("roof", "thick"),
                                    d.roof.thickness,
                                    |v| commands.push(p(json!({"roof": {"thickness": v}}))),
                                );
                                ui.label("Raised plate (heel)");
                                if let Some(v) = app.bufs.length(
                                    ui,
                                    doc,
                                    egui::Id::unique(("defaults", "roof", "heel")),
                                    d.roof.heel,
                                ) && v.mm() >= 0.0
                                {
                                    commands.push(p(json!({"roof": {"heel": v}})));
                                }
                                ui.end_row();
                            });
                    });
                for (title, key, sd) in [
                    ("Stairs", "stair", &d.stair),
                    ("Exterior Stairs", "exterior_stair", &d.exterior_stair),
                ] {
                    egui::CollapsingHeader::new(title)
                        .default_open(key == "stair")
                        .show(ui, |ui| {
                            if key != "stair" {
                                ui.weak("Used for stairs that start outside the building.");
                            }
                            egui::Grid::new(("stair_defaults", key))
                                .num_columns(2)
                                .show(ui, |ui| {
                                    for (label, field, value) in [
                                        ("Width", "width", sd.width),
                                        ("Maximum riser", "max_riser", sd.max_riser),
                                        ("Tread depth", "tread_depth", sd.tread_depth),
                                        ("Railing height", "railing_height", sd.railing_height),
                                    ] {
                                        len_row(
                                            ui,
                                            doc,
                                            &mut app.bufs,
                                            label,
                                            (key, field),
                                            value,
                                            |v| commands.push(p(json!({ key: { field: v } }))),
                                        );
                                    }
                                    ui.label("Nosing overhang");
                                    if let Some(v) = app.bufs.length(
                                        ui,
                                        doc,
                                        egui::Id::unique(("defaults", key, "overhang")),
                                        sd.tread_overhang,
                                    ) && v.mm() >= 0.0
                                    {
                                        commands.push(p(json!({ key: { "tread_overhang": v } })));
                                    }
                                    ui.end_row();
                                    ui.label("Railings");
                                    ui.horizontal(|ui| {
                                        let (mut l, mut r) = (sd.railing_left, sd.railing_right);
                                        if ui.checkbox(&mut l, "Left").changed() {
                                            commands.push(p(json!({ key: { "railing_left": l } })));
                                        }
                                        if ui.checkbox(&mut r, "Right").changed() {
                                            commands
                                                .push(p(json!({ key: { "railing_right": r } })));
                                        }
                                        let mut st = sd.stringers;
                                        if ui.checkbox(&mut st, "Stringers").changed() {
                                            commands.push(p(json!({ key: { "stringers": st } })));
                                        }
                                    });
                                    ui.end_row();
                                });
                        });
                }
                egui::CollapsingHeader::new("Rooms")
                    .default_open(true)
                    .show(ui, |ui| {
                        pick_row::<RoomType>(ui, doc, "New room type", d.room_type, |v| {
                            commands.push(p(json!({"room_type": v})))
                        });
                        let rl = d.room_labels;
                        let mut area = rl.show_area;
                        if ui.checkbox(&mut area, "Labels show the area").changed() {
                            commands.push(p(json!({"room_labels": {"show_area": area}})));
                        }
                        ui.horizontal(|ui| {
                            ui.label("Labels show the size");
                            let mut s = rl.size;
                            for (v, name) in [
                                (RoomSizeFormat::Off, "No"),
                                (RoomSizeFormat::Rounded, "Rounded (12' x 14')"),
                                (RoomSizeFormat::Exact, "Exact (12'-0\" x 14'-6\")"),
                            ] {
                                ui.radio_value(&mut s, v, name);
                            }
                            if s != rl.size {
                                commands.push(p(json!({"room_labels": {"size": s}})));
                            }
                        });
                        let mut clg = rl.show_ceiling_height;
                        if ui.checkbox(&mut clg, "Labels show the ceiling height").changed() {
                            commands.push(p(json!({"room_labels": {"show_ceiling_height": clg}})));
                        }
                    });
                egui::CollapsingHeader::new("Text and Dimensions")
                    .default_open(true)
                    .show(ui, |ui| {
                        pick_row::<TextStyle>(ui, doc, "Text style", d.text_style, |v| {
                            commands.push(p(json!({"text_style": v})))
                        });
                        pick_row::<TextStyle>(
                            ui,
                            doc,
                            "Dimension text",
                            d.dimensions.text_style,
                            |v| commands.push(p(json!({"dimensions": {"text_style": v}}))),
                        );
                        ui.horizontal(|ui| {
                            ui.label("Arrows");
                            let mut a = d.dimensions.arrow;
                            for (v, n) in [
                                (ArrowStyle::Tick, "Tick"),
                                (ArrowStyle::Arrow, "Arrow"),
                                (ArrowStyle::Dot, "Dot"),
                                (ArrowStyle::None, "None"),
                            ] {
                                ui.radio_value(&mut a, v, n);
                            }
                            if a != d.dimensions.arrow {
                                commands.push(p(json!({"dimensions": {"arrow": a}})));
                            }
                        });
                        egui::Grid::new("dim_defaults")
                            .num_columns(2)
                            .show(ui, |ui| {
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Extension gap",
                                    ("dim", "gap"),
                                    d.dimensions.extension_gap,
                                    |v| {
                                        commands
                                            .push(p(json!({"dimensions": {"extension_gap": v}})))
                                    },
                                );
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Extension overshoot",
                                    ("dim", "over"),
                                    d.dimensions.extension_overshoot,
                                    |v| {
                                        commands.push(p(
                                            json!({"dimensions": {"extension_overshoot": v}}),
                                        ))
                                    },
                                );
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Exterior offset",
                                    ("dim", "ext"),
                                    d.dimensions.exterior_offset,
                                    |v| {
                                        commands
                                            .push(p(json!({"dimensions": {"exterior_offset": v}})))
                                    },
                                );
                                len_row(
                                    ui,
                                    doc,
                                    &mut app.bufs,
                                    "Show inches below",
                                    ("dim", "inch"),
                                    d.dimensions.inches_below,
                                    |v| {
                                        commands.push(p(json!({"dimensions": {"inches_below": v}})))
                                    },
                                );
                            });
                    });
                egui::CollapsingHeader::new("Object Labels").show(ui, |ui| {
                    let l = &d.labels;
                    egui::Grid::new("label_defaults").num_columns(2).show(ui, |ui| {
                        for (label, key, v) in [
                            ("Doors", "doors", &l.doors),
                            ("Windows", "windows", &l.windows),
                            ("Cabinets", "cabinets", &l.cabinets),
                            ("Fixtures and furniture", "fixtures", &l.fixtures),
                        ] {
                            ui.label(label);
                            let id = egui::Id::unique(("label_default", key));
                            if let Some(t) = app.bufs.text(ui, id, v, 180.0) {
                                commands.push(p(json!({"labels": {key: t}})));
                            }
                            ui.end_row();
                        }
                        ui.label("Border");
                        let mut b = l.border;
                        if ui.checkbox(&mut b, "Box around each label").changed() {
                            commands.push(p(json!({"labels": {"border": b}})));
                        }
                        ui.end_row();
                    });
                    ui.weak("Empty shows no label. Macros:");
                    for (m, what) in housecraft_draft::macros::OBJECT_MACROS {
                        ui.weak(format!("{{{m}}}  {what}"));
                    }
                });
                egui::CollapsingHeader::new("Framing").show(ui, |ui| {
                    let f = &d.framing;
                    egui::Grid::new("framing_defaults").num_columns(2).show(ui, |ui| {
                        for (label, key, v) in [
                            ("Stud spacing", "stud_spacing", f.stud_spacing),
                            ("Floor joist spacing", "joist_spacing", f.joist_spacing),
                            ("Ceiling joist spacing", "ceiling_joist_spacing", f.ceiling_joist_spacing),
                            ("Rafter spacing", "rafter_spacing", f.rafter_spacing),
                            ("Lumber thickness", "lumber_width", f.lumber_width),
                            ("Floor joist depth", "floor_joist_depth", f.floor_joist_depth),
                            ("Ceiling joist depth", "ceiling_joist_depth", f.ceiling_joist_depth),
                            ("Rafter depth", "rafter_depth", f.rafter_depth),
                        ] {
                            len_row(ui, doc, &mut app.bufs, label, ("framing", key), v, |v| {
                                commands.push(p(json!({"framing": {key: v}})))
                            });
                        }
                        ui.label("Top plates");
                        let mut n = f.top_plates;
                        if ui.add(egui::DragValue::new(&mut n).range(1..=3)).changed() {
                            commands.push(p(json!({"framing": {"top_plates": n}})));
                        }
                        ui.end_row();
                        ui.label("Corners");
                        ui.horizontal(|ui| {
                            for c in CornerFraming::ALL {
                                if ui.selectable_label(f.corners == c, c.name()).clicked() {
                                    commands.push(p(json!({"framing": {"corners": c}})));
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("Roof framing");
                        ui.horizontal(|ui| {
                            for (label, on) in [("Rafters", false), ("Trusses", true)] {
                                if ui.selectable_label(f.trusses == on, label).clicked() {
                                    commands.push(p(json!({"framing": {"trusses": on}})));
                                }
                            }
                        });
                        ui.end_row();
                        len_row(ui, doc, &mut app.bufs, "Truss spacing", ("framing", "truss_spacing"), f.truss_spacing, |v| {
                            commands.push(p(json!({"framing": {"truss_spacing": v}})))
                        });
                        ui.label("Mid-height blocking");
                        let mut b = f.wall_blocking;
                        if ui.checkbox(&mut b, "").changed() {
                            commands.push(p(json!({"framing": {"wall_blocking": b}})));
                        }
                        ui.end_row();
                    });
                });
                egui::CollapsingHeader::new("3D Solids").show(ui, |ui| {
                    let s = &d.solids;
                    ui.horizontal(|ui| {
                        ui.label("Shape");
                        for preset in SolidPreset::ALL {
                            if ui.selectable_label(s.preset == preset, preset.name()).clicked() {
                                commands.push(p(json!({"solids": {"preset": preset}})));
                            }
                        }
                    });
                    egui::Grid::new("solid_defaults").num_columns(2).show(ui, |ui| {
                        len_row(ui, doc, &mut app.bufs, "Height", ("solid", "height"), s.height, |v| {
                            commands.push(p(json!({"solids": {"height": v}})))
                        });
                        ui.label("Colour");
                        let [r, g, b, a] = s.color.0;
                        let mut c32 = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
                        if ui.color_edit_button_srgba(&mut c32).changed() {
                            let [r, g, b, _] = c32.to_srgba_unmultiplied();
                            commands.push(p(json!({"solids": {"color": Color::rgb(r, g, b)}})));
                        }
                        ui.end_row();
                    });
                });
                egui::CollapsingHeader::new("Layers for new objects").show(ui, |ui| {
                    let l = &d.layers;
                    for (label, field, value) in [
                        ("Walls", "walls", l.walls),
                        ("Doors", "doors", l.doors),
                        ("Windows", "windows", l.windows),
                        ("Room labels", "rooms", l.rooms),
                        ("CAD", "cad", l.cad),
                        ("Dimensions", "dimensions", l.dimensions),
                        ("Text", "text", l.text),
                    ] {
                        pick_row::<Layer>(ui, doc, label, value, |v| {
                            commands.push(p(json!({"layers": {field: v}})))
                        });
                    }
                });
            });
        });
    app.dialogs.defaults = open;
    if !commands.is_empty() {
        app.run(commands);
    }
}

fn pick_row<T: ObjectData + Named>(
    ui: &mut Ui,
    doc: &Document,
    label: &str,
    current: Id,
    mut on_change: impl FnMut(Id),
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let name = doc.get::<T>(current).map_or("?", |t| t.name());
        egui::ComboBox::from_id_salt((label, T::KIND.name()))
            .selected_text(name)
            .show_ui(ui, |ui| {
                for t in doc.iter::<T>() {
                    if ui.selectable_label(t.id() == current, t.name()).clicked()
                        && t.id() != current
                    {
                        on_change(t.id());
                    }
                }
            });
    });
}

fn len_row(
    ui: &mut Ui,
    doc: &Document,
    bufs: &mut crate::util::EditBuffers,
    label: &str,
    key: (&str, &str),
    value: Length,
    mut on_change: impl FnMut(Length),
) {
    ui.label(label);
    if let Some(v) = bufs.length(ui, doc, egui::Id::unique(("defaults", key)), value)
        && v.mm() > 0.0
    {
        on_change(v);
    }
    ui.end_row();
}

fn wall_types_dialog(app: &mut App, ctx: &Context) {
    let mut open = app.dialogs.wall_types.open;
    let mut commands = Vec::new();
    egui::Window::new("Wall Type Definitions")
        .open(&mut open)
        .resizable(true)
        .default_width(560.0)
        .show(ctx, |ui| {
            let doc = app.session.document();
            let dlg = &mut app.dialogs.wall_types;
            let types: Vec<WallType> = doc.iter::<WallType>().cloned().collect();
            if dlg
                .selected
                .is_none_or(|s| doc.get::<WallType>(s).is_none())
            {
                dlg.selected = types.first().map(|t| t.id);
                dlg.edit = None;
            }
            ui.horizontal(|ui| {
                ui.label("Wall type:");
                let current = dlg
                    .selected
                    .and_then(|s| doc.get::<WallType>(s))
                    .map_or("", |t| t.name.as_str());
                egui::ComboBox::from_id_salt("wall_type_pick")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for t in &types {
                            if ui
                                .selectable_label(dlg.selected == Some(t.id), &t.name)
                                .clicked()
                            {
                                dlg.selected = Some(t.id);
                                dlg.edit = None;
                            }
                        }
                    });
                if let Some(sel) = dlg.selected.and_then(|s| doc.get::<WallType>(s)) {
                    if ui.button("Copy").clicked() {
                        commands.push(Command::AddWallType {
                            name: format!("{} copy", sel.name),
                            layers: sel.layers.clone(),
                        });
                    }
                    if ui.button("Rename / Edit").clicked() {
                        app.spec.push(sel.id);
                    }
                    if ui.button("Delete").clicked() {
                        commands.push(Command::Delete { ids: vec![sel.id] });
                    }
                }
            });
            let Some(sel) = dlg.selected.and_then(|s| doc.get::<WallType>(s)).cloned() else {
                return;
            };
            let rows = &mut dlg
                .edit
                .get_or_insert_with(|| {
                    (
                        sel.id,
                        sel.layers
                            .iter()
                            .map(|l| (l.name.clone(), fmt(doc, l.thickness), l.material.clone()))
                            .collect(),
                    )
                })
                .1;
            ui.separator();
            ui.label("Layers, exterior (top) to interior (bottom):");
            let mut remove = None;
            let mut swap = None;
            egui::Grid::new("wall_layers")
                .num_columns(5)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("#");
                    ui.strong("Name");
                    ui.strong("Thickness");
                    ui.strong("Material");
                    ui.end_row();
                    let n = rows.len();
                    for (i, (name, thick, mat)) in rows.iter_mut().enumerate() {
                        ui.label((i + 1).to_string());
                        ui.add(egui::TextEdit::singleline(name).desired_width(120.0));
                        ui.add(egui::TextEdit::singleline(thick).desired_width(80.0));
                        ui.add(egui::TextEdit::singleline(mat).desired_width(120.0));
                        ui.horizontal(|ui| {
                            if ui.add_enabled(i > 0, egui::Button::new("↑")).clicked() {
                                swap = Some((i, i - 1));
                            }
                            if ui.add_enabled(i + 1 < n, egui::Button::new("↓")).clicked() {
                                swap = Some((i, i + 1));
                            }
                            if ui.add_enabled(n > 1, egui::Button::new("✕")).clicked() {
                                remove = Some(i);
                            }
                        });
                        ui.end_row();
                    }
                });
            if let Some((a, b)) = swap {
                rows.swap(a, b);
            }
            if let Some(i) = remove {
                rows.remove(i);
            }
            let parsed: Option<Vec<WallLayer>> = rows
                .iter()
                .map(|(name, thick, mat)| {
                    parse(doc, thick)
                        .filter(|l| l.mm() > 0.0)
                        .map(|thickness| WallLayer {
                            name: name.clone(),
                            thickness,
                            material: mat.clone(),
                        })
                })
                .collect();
            let total: Option<Length> =
                parsed.as_ref().map(|v| v.iter().map(|l| l.thickness).sum());
            ui.horizontal(|ui| {
                if ui.button("Add layer").clicked() {
                    rows.push(("New layer".into(), "1/2\"".into(), String::new()));
                }
                ui.separator();
                match total {
                    Some(t) => ui.label(format!("Total thickness: {}", fmt(doc, t))),
                    None => ui.colored_label(
                        ui.visuals().error_fg_color,
                        "Every thickness must be a positive length",
                    ),
                };
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(parsed.is_some(), egui::Button::new("Apply"))
                    .clicked()
                    && let Some(layers) = parsed
                {
                    commands.push(Command::Update {
                        id: sel.id,
                        set: json!({ "layers": layers })
                            .as_object()
                            .cloned()
                            .unwrap_or_default(),
                    });
                    dlg.edit = None;
                }
                if ui.button("Revert").clicked() {
                    dlg.edit = None;
                }
            });
        });
    app.dialogs.wall_types.open = open;
    if !commands.is_empty() {
        app.run(commands);
    }
}

fn layers_dialog(app: &mut App, ctx: &Context) {
    let mut open = app.dialogs.layers;
    let mut commands = Vec::new();
    let mut spec = Vec::new();
    egui::Window::new("Layers and Layer Sets")
        .open(&mut open)
        .resizable(true)
        .default_width(420.0)
        .show(ctx, |ui| {
            let doc = app.session.document();
            ui.heading("Layers");
            egui::Grid::new("layer_list")
                .num_columns(4)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Name");
                    ui.strong("Weight");
                    ui.strong("Style");
                    ui.end_row();
                    for l in doc.iter::<Layer>() {
                        if ui.link(&l.name).clicked() {
                            spec.push(l.id);
                        }
                        ui.label(format!("{:.2} mm", l.line_weight));
                        ui.label(format!("{:?}", l.line_style));
                        if ui.small_button("Delete").clicked() {
                            commands.push(Command::Delete { ids: vec![l.id] });
                        }
                        ui.end_row();
                    }
                });
            if ui.button("New Layer").clicked() {
                commands.push(Command::AddLayer {
                    name: format!("Layer {}", doc.count::<Layer>() + 1),
                    color: None,
                    line_weight: None,
                    line_style: LineStyle::Solid,
                });
            }
            ui.separator();
            ui.heading("Layer Sets");
            for s in doc.iter::<LayerSet>() {
                ui.horizontal(|ui| {
                    if ui.link(&s.name).clicked() {
                        spec.push(s.id);
                    }
                    if ui.small_button("Copy").clicked() {
                        commands.push(Command::AddLayerSet {
                            name: format!("{} copy", s.name),
                            copy_from: Some(s.id),
                        });
                    }
                    if ui.small_button("Delete").clicked() {
                        commands.push(Command::Delete { ids: vec![s.id] });
                    }
                });
            }
            ui.weak("Visibility per set is edited in the Layer Display side window.");
        });
    app.dialogs.layers = open;
    app.spec.extend(spec);
    if !commands.is_empty() {
        app.run(commands);
    }
}

/// A simple list of named objects with New, Edit (spec window) and Delete.
fn list_dialog<T: ObjectData + Named>(
    app: &mut App,
    ctx: &Context,
    title: &str,
    flag: fn(&mut App) -> &mut bool,
    new: impl Fn() -> Command,
) {
    let mut open = *flag(app);
    if !open {
        return;
    }
    let mut commands = Vec::new();
    let mut spec = Vec::new();
    egui::Window::new(title)
        .open(&mut open)
        .resizable(true)
        .show(ctx, |ui| {
            let doc = app.session.document();
            for t in doc.iter::<T>() {
                ui.horizontal(|ui| {
                    if ui.link(t.name()).clicked() {
                        spec.push(t.id());
                    }
                    if ui.small_button("Delete").clicked() {
                        commands.push(Command::Delete { ids: vec![t.id()] });
                    }
                });
            }
            if ui.button("New").clicked() {
                commands.push(new());
            }
        });
    *flag(app) = open;
    app.spec.extend(spec);
    if let Some(out) = (!commands.is_empty()).then(|| app.run(commands)).flatten() {
        app.spec.extend(out.created);
    }
}

fn save_view_dialog(app: &mut App, ctx: &Context) {
    let Some(mut name) = app.dialogs.save_view.take() else {
        return;
    };
    let mut keep = true;
    let mut save = false;
    egui::Window::new("Save Plan View")
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut name);
            });
            ui.weak("Saves the floor, layer set, default set and zoom.");
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    save = true;
                }
                if ui.button("Cancel").clicked() {
                    keep = false;
                }
            });
        });
    if save {
        keep = false;
        let cmd = Command::AddPlanView {
            name: name.clone(),
            floor: Some(app.plan.floor),
            layer_set: Some(app.plan.layer_set),
            default_set: Some(app.doc().settings.active_default_set),
            zoom: Some(ViewRect {
                center: housecraft_api::geom::pt(app.plan.center),
                mm_per_point: app.plan.mm_per_pt,
            }),
            reference_floor: app.plan.reference_floor,
            references: app.plan.references.clone(),
            phase: app.plan.phase,
        };
        if let Some(out) = app.run1(cmd) {
            app.plan.view = out.created.first().copied();
        }
    }
    if keep {
        app.dialogs.save_view = Some(name);
    }
}

/// Parse "3 1.5 0.4 1.5" into a dash pattern (pairs of on/off, paper mm).
fn parse_pattern(text: &str) -> Option<Vec<f64>> {
    let v: Vec<f64> = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<f64>().ok().filter(|x| x.is_finite() && *x >= 0.0))
        .collect::<Option<_>>()?;
    v.len().is_multiple_of(2).then_some(v)
}

/// Tools > Line Styles…: the plan's own dash patterns, how often each is
/// used, and deleting one into another.
fn line_styles_dialog(app: &mut App, ctx: &Context) {
    let mut open = app.dialogs.line_styles;
    let mut commands = Vec::new();
    egui::Window::new("Line Styles").open(&mut open).resizable(true).show(ctx, |ui| {
        let doc = app.session.document();
        ui.label("Built in: Solid, Dashed, Dotted, Dash-dot. Patterns are dash and gap lengths on paper (mm).");
        ui.separator();
        egui::Grid::new("line_types").striped(true).num_columns(5).show(ui, |ui| {
            ui.strong("Name");
            ui.strong("Pattern (mm)");
            ui.strong("Used by");
            ui.strong("Delete, using instead");
            ui.end_row();
            for (i, t) in doc.settings.line_types.iter().enumerate() {
                let i = i as u32;
                let mut name = t.name.clone();
                if ui.text_edit_singleline(&mut name).lost_focus() && name != t.name {
                    commands.push(Command::SetLineType { index: Some(i), line_type: LineType { name, pattern: t.pattern.clone() } });
                }
                let text: String = t.pattern.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
                let mut edit = text.clone();
                if ui.text_edit_singleline(&mut edit).lost_focus()
                    && edit != text
                    && let Some(pattern) = parse_pattern(&edit)
                {
                    commands.push(Command::SetLineType { index: Some(i), line_type: LineType { name: t.name.clone(), pattern } });
                }
                let uses = housecraft_api::ops_line_type_uses(doc, i);
                ui.label(format!("{uses} object{}", if uses == 1 { "" } else { "s" }));
                ui.horizontal(|ui| {
                    let others = LineStyle::BUILT_IN
                        .into_iter()
                        .chain((0..doc.settings.line_types.len() as u32).map(LineStyle::Custom))
                        .filter(|s| *s != LineStyle::Custom(i));
                    ui.menu_button("Delete…", |ui| {
                        for s in others {
                            if ui.button(s.name(&doc.settings)).clicked() {
                                commands.push(Command::DeleteLineType { index: i, merge_into: s });
                                ui.close();
                            }
                        }
                    });
                });
                ui.end_row();
            }
        });
        ui.separator();
        let (name, pattern) = &mut app.dialogs.new_line_type;
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(name).hint_text("Name").desired_width(120.0));
            ui.add(egui::TextEdit::singleline(pattern).hint_text("e.g. 6 1.5 1.5 1.5").desired_width(140.0));
            let parsed = parse_pattern(pattern).filter(|p| !p.is_empty());
            if ui.add_enabled(parsed.is_some() && !name.trim().is_empty(), egui::Button::new("Add")).clicked()
                && let Some(p) = parsed
            {
                commands.push(Command::SetLineType { index: None, line_type: LineType { name: name.trim().to_string(), pattern: p } });
                name.clear();
                pattern.clear();
            }
        });
    });
    app.dialogs.line_styles = open;
    if !commands.is_empty() {
        app.run(commands);
    }
}

fn shortcuts_dialog(app: &mut App, ctx: &Context) {
    let mut open = app.dialogs.shortcuts;
    egui::Window::new("Keyboard Shortcuts")
        .open(&mut open)
        .resizable(true)
        .show(ctx, |ui| {
            ui.heading("Your shortcuts");
            ui.weak("Give any tool a key, or two keys pressed one after the other (\"G W\").");
            let mut remove = None;
            egui::Grid::new("user_keys").striped(true).show(ui, |ui| {
                for (i, h) in app.prefs.hotkeys.iter().enumerate() {
                    ui.monospace(&h.keys);
                    ui.label(&h.tool);
                    if ui.small_button("Remove").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = remove {
                app.prefs.hotkeys.remove(i);
            }
            let (keys, tool) = &mut app.dialogs.new_hotkey;
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(keys).hint_text("Keys, e.g. G W").desired_width(90.0));
                egui::ComboBox::from_id_salt("hotkey_tool")
                    .selected_text(if tool.is_empty() { "Choose a tool" } else { tool.as_str() })
                    .height(400.0)
                    .show_ui(ui, |ui| {
                        for t in crate::app::all_tools() {
                            ui.selectable_value(tool, t.name().to_string(), t.name());
                        }
                    });
                let parsed = crate::app::parse_hotkey(keys);
                let ok = parsed.is_some() && !tool.is_empty();
                if ui.add_enabled(ok, egui::Button::new("Add")).clicked()
                    && let Some(k) = parsed
                {
                    app.prefs.hotkeys.retain(|h| h.keys != k);
                    app.prefs.hotkeys.push(crate::app::UserHotkey { keys: k, tool: tool.clone() });
                    keys.clear();
                }
            });
            ui.separator();
            ui.heading("Built in");
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("keys").striped(true).show(ui, |ui| {
                    for (k, what) in [
                        ("Space", "Select Objects"),
                        ("Tab", "Select Next Object"),
                        ("Ctrl+E / double-click", "Open Object"),
                        ("Esc", "Cancel / end drawing"),
                        ("Del", "Delete"),
                        ("Ctrl+Z / Ctrl+Y", "Undo / Redo"),
                        ("Ctrl+C / Ctrl+X / Ctrl+V", "Copy / Cut / Paste"),
                        ("Ctrl+Alt+V", "Paste Hold Position"),
                        ("Ctrl+A / Ctrl+J", "Select All / Select Same Type"),
                        ("Shift+Q", "Straight Exterior Wall"),
                        ("Shift+E", "Hinged Door"),
                        ("Shift+W", "Window"),
                        ("W / E / K", "Draw Line / Arc / Circle"),
                        ("Shift+P", "Rectangular Polyline"),
                        ("Y", "Text"),
                        ("Shift+A", "Auto Exterior Dimensions"),
                        ("Shift+X", "Build New Floor"),
                        ("Shift+N / Shift+M", "Up / Down One Floor"),
                        ("F9", "Reference Floor Display"),
                        ("F6", "Fill Window"),
                        ("F10 / Ctrl+F9 / Shift+F11", "Angle / Grid / Object snaps"),
                        (
                            "Shift+J / Shift+K / H",
                            "Full Camera / Perspective / Ortho Overview",
                        ),
                        ("F2 / F3", "Plan view / 3D view"),
                        ("~", "Preferences"),
                        (
                            "Typing while drawing",
                            "Exact length, then Enter (arithmetic allowed)",
                        ),
                        ("Middle / right drag, wheel", "Pan, zoom"),
                        (
                            "3D: drag / Shift+drag / wheel",
                            "Orbit / pan / zoom; WASD walks",
                        ),
                    ] {
                        ui.monospace(k);
                        ui.label(what);
                        ui.end_row();
                    }
                });
            });
        });
    app.dialogs.shortcuts = open;
}

fn about_dialog(app: &mut App, ctx: &Context) {
    egui::Window::new("About HouseCraft")
        .open(&mut app.dialogs.about)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading(format!("HouseCraft {}", env!("CARGO_PKG_VERSION")));
            ui.label("Residential design and construction documents.");
            ui.label("Built in Rust with egui and wgpu. Also usable headless (housecraft CLI) and by AI assistants (housecraft mcp).");
        });
}

/// True once per edit of a drag value: when a drag ends, or on a typed or
/// clicked change (so undo gets one step, not one per frame).
fn committed(r: &egui::Response) -> bool {
    r.drag_stopped() || (r.changed() && !r.dragged())
}

fn sun_dialog(app: &mut App, ctx: &Context) {
    let mut open = app.dialogs.sun.open;
    if !open {
        return;
    }
    let mut commands = Vec::new();
    egui::Window::new("Sun Angle and North")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let settings = app.session.document().settings.clone();
            let d = &mut app.dialogs.sun;
            let sun = settings.sun;
            ui.heading("Sun");
            egui::Grid::new("sun_direct").num_columns(2).show(ui, |ui| {
                ui.label("Bearing (° from north, clockwise)");
                let mut az = sun.azimuth;
                if committed(&ui.add(egui::DragValue::new(&mut az).fixed_decimals(1).range(0.0..=360.0).speed(1.0).suffix("°"))) {
                    commands.push(Command::SetSun { azimuth: Some(az), altitude: None, latitude: None, day_of_year: None, hour: None, shadows: None });
                }
                ui.end_row();
                ui.label("Altitude above horizon");
                let mut alt = sun.altitude;
                if committed(&ui.add(egui::DragValue::new(&mut alt).fixed_decimals(1).range(-10.0..=90.0).speed(0.5).suffix("°"))) {
                    commands.push(Command::SetSun { azimuth: None, altitude: Some(alt), latitude: None, day_of_year: None, hour: None, shadows: None });
                }
                ui.end_row();
                ui.label("Shadows");
                let mut sh = sun.shadows;
                if ui.checkbox(&mut sh, "Cast shadows in 3D").changed() {
                    commands.push(Command::SetSun { azimuth: None, altitude: None, latitude: None, day_of_year: None, hour: None, shadows: Some(sh) });
                }
                ui.end_row();
            });
            ui.separator();
            ui.label("Or from location and time:");
            egui::Grid::new("sun_place").num_columns(2).show(ui, |ui| {
                ui.label("Latitude");
                ui.add(egui::DragValue::new(&mut d.latitude).range(-90.0..=90.0).speed(0.1).suffix("°"));
                ui.end_row();
                ui.label("Date");
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("sun_month")
                        .selected_text(MONTHS[d.month - 1].0)
                        .show_ui(ui, |ui| {
                            for (i, (name, _)) in MONTHS.iter().enumerate() {
                                ui.selectable_value(&mut d.month, i + 1, *name);
                            }
                        });
                    let days = MONTHS[d.month - 1].1;
                    d.day = d.day.min(days);
                    ui.add(egui::DragValue::new(&mut d.day).range(1..=days));
                });
                ui.end_row();
                ui.label("Solar time");
                ui.add(egui::Slider::new(&mut d.hour, 4.0..=20.0).step_by(0.25).custom_formatter(|h, _| {
                    format!("{:02}:{:02}", h.floor() as u32, ((h.fract() * 60.0).round()) as u32)
                }));
                ui.end_row();
            });
            if ui.button("Set sun from location and time").clicked() {
                let day: u32 = MONTHS[..d.month - 1].iter().map(|m| m.1).sum::<u32>() + d.day;
                commands.push(Command::SetSun {
                    azimuth: None,
                    altitude: None,
                    latitude: Some(d.latitude),
                    day_of_year: Some(day),
                    hour: Some(d.hour),
                    shadows: None,
                });
            }
            ui.separator();
            ui.heading("North");
            egui::Grid::new("north").num_columns(2).show(ui, |ui| {
                ui.label("North points (° clockwise from up)");
                let mut cw = (90.0 - settings.north).rem_euclid(360.0);
                if committed(&ui.add(egui::DragValue::new(&mut cw).fixed_decimals(1).range(0.0..=360.0).speed(1.0).suffix("°"))) {
                    commands.push(Command::SetNorth {
                        angle: (90.0 - cw).rem_euclid(360.0),
                        pointer: settings.north_pointer,
                    });
                }
                ui.end_row();
                ui.label("North pointer");
                let mut show = settings.north_pointer.is_some();
                if ui.checkbox(&mut show, "Show in plan views").changed() {
                    commands.push(Command::SetNorth {
                        angle: settings.north,
                        pointer: show.then(|| housecraft_api::geom::pt(app.plan.center)),
                    });
                }
                ui.end_row();
            });
            ui.weak("The pointer is placed at the centre of the current plan view. Turn it off and on again to move it.");
        });
    app.dialogs.sun.open = open;
    for c in commands {
        app.run1(c);
    }
}

fn foundation_dialog(app: &mut App, ctx: &Context) {
    let mut open = app.dialogs.foundation.open;
    if !open {
        return;
    }
    let mut command = None;
    egui::Window::new("Build Foundation")
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let doc = app.session.document();
            let d = &mut app.dialogs.foundation;
            ui.horizontal(|ui| {
                ui.radio_value(&mut d.kind, FoundationKind::Slab, "Slab");
                ui.radio_value(&mut d.kind, FoundationKind::Crawlspace, "Crawlspace / stem wall");
                ui.radio_value(&mut d.kind, FoundationKind::Basement, "Basement");
                ui.radio_value(&mut d.kind, FoundationKind::Piers, "Pier and beam");
            });
            let hint = match d.kind {
                FoundationKind::Slab => "4\"",
                FoundationKind::Crawlspace => "30\"",
                FoundationKind::Basement => "8'",
                FoundationKind::Piers => "24\"",
            };
            egui::Grid::new("foundation_grid").num_columns(2).show(ui, |ui| {
                ui.label(if d.kind == FoundationKind::Slab { "Slab thickness" } else { "Wall height" });
                ui.add(egui::TextEdit::singleline(&mut d.wall_height).hint_text(hint));
                ui.end_row();
                ui.label("Footing width");
                ui.add(egui::TextEdit::singleline(&mut d.footing_width).hint_text(if d.kind == FoundationKind::Slab { "12\"" } else { "16\"" }));
                ui.end_row();
                ui.label("Footing depth");
                ui.add(egui::TextEdit::singleline(&mut d.footing_depth).hint_text(if d.kind == FoundationKind::Slab { "12\"" } else { "8\"" }));
                ui.end_row();
            });
            ui.checkbox(&mut d.auto_rebuild, "Rebuild automatically when the floor above changes");
            ui.weak("Built under the lowest floor, following its exterior walls. Running it again replaces the foundation.");
            let opt = |s: &str| if s.trim().is_empty() { Some(None) } else { parse(doc, s).filter(|l| l.mm() > 0.0).map(Some) };
            let parsed = (opt(&d.wall_height), opt(&d.footing_width), opt(&d.footing_depth));
            let ok = parsed.0.is_some() && parsed.1.is_some() && parsed.2.is_some();
            if ui.add_enabled(ok, egui::Button::new("Build")).clicked()
                && let (Some(h), Some(w), Some(dp)) = parsed
            {
                command = Some(Command::BuildFoundation {
                    kind: d.kind,
                    wall_height: h,
                    wall_thickness: None,
                    footing_width: w,
                    footing_depth: dp,
                    slab_thickness: None,
                    auto_rebuild: d.auto_rebuild,
                    pier_spacing: None,
                    pier_diameter: None,
                });
            }
        });
    app.dialogs.foundation.open = open && command.is_none();
    if let Some(c) = command
        && app.run1(c).is_some()
    {
        app.info("Foundation built");
    }
}
