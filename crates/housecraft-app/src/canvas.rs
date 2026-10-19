//! The plan view: drawing, navigation, tools, snapping, selection and editing.

use egui::{
    Color32, Painter, Pos2, Rect, Response, Sense, Shape as EShape, Stroke, Ui, pos2, vec2,
};
use housecraft_api::geom::math::{distance_to_polyline, point_in_polygon};
use housecraft_api::geom::rooms::FloorRegions;
use housecraft_api::geom::triangulate::triangulate as triangulate_poly;
use housecraft_api::geom::walls::wall_geoms;
use housecraft_api::geom::{DVec2, pt, v2};
use housecraft_api::model::*;
use housecraft_api::units::{Length, Point2};
use housecraft_api::{Command, LengthAnchor, TransformOp};
use housecraft_draft::{Drawing, Fill, Ink, Prim, approx_text_width};

use crate::app::App;
use crate::snap::{Snap, SnapKind, SnapTargets, snap};
use crate::tools::{StairKind, Tool};

/// Pick radius in screen points.
const PICK_PX: f64 = 7.0;

struct Cache {
    key: (u64, Id, Id, bool, Vec<Id>, Phase, bool),
    drawing: Drawing,
    fills: Vec<(Vec<DVec2>, Vec<[usize; 3]>, Fill)>,
    regions: FloorRegions,
    targets: SnapTargets,
    /// What the dimension tool snaps to (Default Settings > dimension locate).
    dim_targets: SnapTargets,
}

#[derive(Debug, Clone)]
enum Drag {
    Pan,
    Move { ids: Vec<Id>, grab: DVec2 },
    Handle { id: Id, handle: Handle },
    Box { start: Pos2 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Handle {
    WallStart,
    WallEnd,
    Vertex(usize),
    Radius,
    TextPosition,
    DimOffset,
}

pub struct PlanState {
    pub floor: Id,
    /// An elevation/section view or CAD detail shown in place of the floor
    /// plan; drafting tools then draw on it.
    pub sheet: Option<Id>,
    pub layer_set: Id,
    pub view: Option<Id>,
    pub reference_floor: bool,
    /// Chosen reference floors; empty = the floor below.
    pub references: Vec<Id>,
    pub phase: Phase,
    pub sun_shadows: bool,
    pub center: DVec2,
    pub mm_per_pt: f64,
    pub fit_requested: bool,
    pub zoom_to_selection: bool,
    pub tool: Tool,
    /// Points clicked so far by the active tool.
    pub pending: Vec<DVec2>,
    /// Typed length while drawing walls or lines.
    pub length_input: Option<String>,
    pub cursor: Option<DVec2>,
    pub snap: Option<Snap>,
    hover: Option<Id>,
    drag: Option<Drag>,
    cache: Option<Cache>,
    /// Objects under the last click, for Select Next Object.
    candidates: Vec<Id>,
    /// What the next click picks for an edit button (instead of selecting).
    pub pick: Option<Pick>,
    /// Sides for the Regular Polygon tool.
    pub polygon_sides: u32,
    /// Room type the Space Planning Box labels its boxes with.
    pub space_type: String,
    text_prompt: Option<(DVec2, String)>,
    /// Arrow tips for the text being typed (Text with Arrow).
    prompt_leaders: Vec<DVec2>,
    /// The wall drawn last in the current run (its end is mitred to the next).
    pub last_wall: Option<Id>,
    /// The first wall of the current run (its start is mitred on closing).
    pub first_wall: Option<Id>,
    temp_dim_edit: Option<(Id, String, bool)>,
    /// The switch being wired by the Connect Electrical tool.
    pub wiring: Option<Id>,
}

impl PlanState {
    pub fn new(doc: &Document) -> Self {
        let view = doc.iter::<PlanView>().next();
        PlanState {
            floor: view
                .and_then(|v| v.floor)
                .unwrap_or_else(|| doc.floors_by_elevation()[0].id),
            layer_set: view
                .map(|v| v.layer_set)
                .or_else(|| doc.iter::<LayerSet>().next().map(|s| s.id))
                .unwrap_or(Id(0)),
            view: view.map(|v| v.id),
            sheet: None,
            reference_floor: false,
            references: Vec::new(),
            phase: Phase::All,
            sun_shadows: false,
            center: DVec2::new(6000.0, 4500.0),
            mm_per_pt: 20.0,
            fit_requested: true,
            zoom_to_selection: false,
            tool: Tool::Select,
            pending: Vec::new(),
            length_input: None,
            cursor: None,
            snap: None,
            hover: None,
            drag: None,
            cache: None,
            candidates: Vec::new(),
            pick: None,
            polygon_sides: 6,
            space_type: "Living".into(),
            text_prompt: None,
            prompt_leaders: Vec::new(),
            last_wall: None,
            first_wall: None,
            temp_dim_edit: None,
            wiring: None,
        }
    }

    /// Where drafting goes: the open view or CAD detail, else the floor.
    pub fn target(&self) -> Id {
        self.sheet.unwrap_or(self.floor)
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.wiring = None;
        self.pending.clear();
        self.length_input = None;
        self.drag = None;
    }

    pub fn zoom_by(&mut self, factor: f64) {
        self.mm_per_pt = (self.mm_per_pt * factor).clamp(0.05, 2000.0);
    }

    pub fn next_candidate(&mut self, current: Option<Id>) -> Option<Id> {
        if self.candidates.is_empty() {
            return None;
        }
        let i = current
            .and_then(|c| self.candidates.iter().position(|x| *x == c))
            .map_or(0, |i| (i + 1) % self.candidates.len());
        Some(self.candidates[i])
    }

    fn ensure_cache(&mut self, doc: &Document, rev: u64) {
        let key = (
            rev,
            self.target(),
            self.layer_set,
            self.reference_floor,
            self.references.clone(),
            self.phase,
            self.sun_shadows,
        );
        if self.cache.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        let opts = housecraft_draft::PlanOptions {
            floor: self.floor,
            layer_set: self.layer_set,
            reference_floor: self.reference_floor,
            references: self.references.clone(),
            phase: self.phase,
            sun_shadows: self.sun_shadows,
            default_set: None,
        };
        let drawing = match self.sheet.and_then(|s| doc.object(s)) {
            Some(Object::SectionView(v)) => housecraft_draft::elevation::section_view(doc, v),
            Some(Object::CadDetail(d)) => {
                housecraft_draft::detail::cad_detail(doc, d.id, Some(self.layer_set))
            }
            Some(Object::Block(b)) => housecraft_draft::detail::cad_detail(doc, b.id, Some(self.layer_set)),
            _ => housecraft_draft::plan(doc, &opts),
        };
        let fills = drawing
            .items
            .iter()
            .filter_map(|i| match &i.prim {
                Prim::Polygon { points, fill } => {
                    Some((points.clone(), triangulate_poly(points), *fill))
                }
                _ => None,
            })
            .collect();
        let mut floors = vec![self.target()];
        let mut targets;
        if self.sheet.is_some() {
            targets = SnapTargets::build(doc, &floors);
            targets.add_drawing(&drawing);
        } else {
            floors.extend(opts.reference_floor_ids(doc));
            targets = SnapTargets::build(doc, &floors);
        }
        let dim_targets = if self.sheet.is_some() {
            let mut t = SnapTargets::build(doc, &floors);
            t.add_drawing(&drawing);
            t
        } else {
            SnapTargets::build_dims(doc, &floors)
        };
        self.cache = Some(Cache {
            key,
            fills,
            regions: housecraft_api::geom::rooms::detect(doc, self.target()),
            targets,
            dim_targets,
            drawing,
        });
    }
}

#[derive(Clone, Copy)]
pub(crate) struct View {
    pub rect: Rect,
    pub center: DVec2,
    pub mpp: f64,
}

impl View {
    pub(crate) fn to_screen(self, p: DVec2) -> Pos2 {
        let c = self.rect.center();
        pos2(
            c.x + ((p.x - self.center.x) / self.mpp) as f32,
            c.y - ((p.y - self.center.y) / self.mpp) as f32,
        )
    }
    pub(crate) fn to_model(self, s: Pos2) -> DVec2 {
        let c = self.rect.center();
        DVec2::new(
            self.center.x + (s.x - c.x) as f64 * self.mpp,
            self.center.y - (s.y - c.y) as f64 * self.mpp,
        )
    }
    fn len_px(self, mm: f64) -> f32 {
        (mm / self.mpp) as f32
    }
}

fn ink_color(ui: &Ui, ink: Ink) -> Color32 {
    let dark = ui.visuals().dark_mode;
    match ink {
        Ink::Default if dark => Color32::from_gray(225),
        Ink::Default => Color32::BLACK,
        Ink::Faded if dark => Color32::from_gray(110),
        Ink::Faded => Color32::from_gray(170),
        Ink::Color(c) if dark => {
            // Lift dark layer colours so they stay readable on the dark canvas.
            let col = crate::util::to_color32(c);
            let luma = 0.299 * col.r() as f32 + 0.587 * col.g() as f32 + 0.114 * col.b() as f32;
            if luma < 110.0 {
                col.lerp_to_gamma(Color32::WHITE, 0.45)
            } else {
                col
            }
        }
        Ink::Color(c) => crate::util::to_color32(c),
    }
}

pub(crate) fn fill_color(ui: &Ui, fill: Fill) -> Color32 {
    match fill {
        Fill::Poche if ui.visuals().dark_mode => Color32::from_gray(90),
        Fill::Poche => Color32::from_gray(195),
        Fill::Shadow if ui.visuals().dark_mode => Color32::from_rgb(44, 48, 56),
        Fill::Shadow => Color32::from_rgb(226, 228, 232),
        Fill::Paper if ui.visuals().dark_mode => Color32::from_gray(28),
        Fill::Paper => Color32::WHITE,
        Fill::Ink(i) => ink_color(ui, i),
    }
}

fn pen_px(paper_mm: f64) -> f32 {
    (paper_mm * 3.2).clamp(0.7, 3.5) as f32
}

pub fn show(app: &mut App, ui: &mut Ui) {
    if app.prefs.paper_canvas {
        // Plans are drawn on white paper, as on a printed sheet.
        *ui.visuals_mut() = egui::Visuals::light();
    }
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = resp.rect;
    app.plan.ensure_cache(app.session.document(), app.rev);

    if std::mem::take(&mut app.plan.fit_requested) {
        let bounds = app
            .plan
            .cache
            .as_ref()
            .and_then(|c| c.drawing.bounds())
            .unwrap_or((DVec2::ZERO, DVec2::new(12192.0, 9144.0)));
        fit(&mut app.plan, rect, bounds);
    }
    if std::mem::take(&mut app.plan.zoom_to_selection) {
        let pts: Vec<DVec2> = app
            .plan
            .cache
            .as_ref()
            .map(|c| {
                c.drawing
                    .items
                    .iter()
                    .filter(|i| i.object.is_some_and(|o| app.selection.contains(&o)))
                    .flat_map(|i| prim_points(&i.prim))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(b) = housecraft_api::geom::math::bounds(pts) {
            fit(&mut app.plan, rect, b);
        }
    }

    navigate(app, ui, &resp);
    let view = View {
        rect,
        center: app.plan.center,
        mpp: app.plan.mm_per_pt,
    };

    // Cursor and snapping.
    let tol = PICK_PX * view.mpp;
    app.plan.cursor = resp.hover_pos().map(|p| view.to_model(p));
    app.plan.snap = app.plan.cursor.map(|c| {
        let from = app.plan.pending.last().copied().or(match &app.plan.drag {
            Some(Drag::Handle { id, handle }) => {
                handle_anchor(app.session.document(), *id, *handle)
            }
            _ => None,
        });
        let cache = app.plan.cache.as_ref().expect("cache built");
        let targets = if matches!(app.plan.tool, Tool::Dimension | Tool::AngularDimension) { &cache.dim_targets } else { &cache.targets };
        // Holding Alt places freely, ignoring every snap.
        let free = ui.input(|i| i.modifiers.alt);
        if (app.plan.tool == Tool::Select && app.plan.drag.is_none()) || free {
            Snap {
                point: c,
                kind: SnapKind::Free,
            }
        } else {
            snap(c, tol, targets, &app.prefs.snap, from)
        }
    });

    let mut commands: Vec<Command> = Vec::new();
    match app.plan.tool {
        Tool::Select => select_tool(app, ui, &resp, &view, &mut commands),
        _ => drawing_tool(app, ui, &resp, &view, &mut commands),
    }

    paint(app, ui, &painter, &view);
    if app.prefs.show_axes {
        paint_origin(&painter, &view);
    }
    if let Some(sheet) = app.plan.sheet {
        let doc = app.session.document();
        let name = match doc.object(sheet) {
            Some(Object::SectionView(v)) => v.name.clone(),
            Some(Object::CadDetail(d)) => format!("CAD Detail: {}", d.name),
            Some(Object::Block(b)) => format!("Block: {} (changes show in every copy)", b.name),
            _ => String::new(),
        };
        let text = format!("{name}  ·  draw with the CAD, text and dimension tools  ·  F2 for plan");
        let at = rect.left_top() + egui::vec2(10.0, 8.0);
        let galley = painter.layout_no_wrap(
            text,
            egui::FontId::proportional(13.0),
            ui.visuals().strong_text_color(),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(at, galley.size()).expand(4.0),
            3.0,
            ui.visuals().extreme_bg_color,
        );
        painter.galley(at, galley, ui.visuals().strong_text_color());
    }
    overlays(app, ui, &view, &mut commands);

    if !commands.is_empty() {
        let tool = app.plan.tool;
        if let Some(out) = app.run(commands) {
            if tool == Tool::Select && !out.created.is_empty() {
                app.selection = out.created.clone();
            }
            if matches!(tool, Tool::ExteriorWall | Tool::InteriorWall) {
                let doc = app.session.document();
                if let Some(w) = out.created.iter().find(|id| doc.kind_of(**id) == Some(ObjectKind::Wall)) {
                    app.plan.last_wall = Some(*w);
                    if app.plan.first_wall.is_none() {
                        app.plan.first_wall = Some(*w);
                    }
                }
            }
            // A new elevation or section opens straight away.
            if matches!(tool, Tool::SectionLine(_))
                && let Some(id) = out
                    .created
                    .iter()
                    .find(|id| app.doc().kind_of(**id) == Some(ObjectKind::SectionView))
            {
                app.open_sheet(*id);
            }
        }
    }
}

fn fit(plan: &mut PlanState, rect: Rect, (lo, hi): (DVec2, DVec2)) {
    let size = (hi - lo).max(DVec2::splat(1000.0));
    plan.center = (lo + hi) / 2.0;
    plan.mm_per_pt =
        (size.x / (rect.width() as f64 * 0.9)).max(size.y / (rect.height() as f64 * 0.9));
}

fn prim_points(p: &Prim) -> Vec<DVec2> {
    match p {
        Prim::Polyline { points, .. } | Prim::Polygon { points, .. } => points.clone(),
        Prim::Text { at, .. } => vec![*at],
        Prim::Image { corners, .. } => corners.to_vec(),
    }
}

/// Wheel zoom about the cursor; middle- or right-drag pans.
fn navigate(app: &mut App, ui: &Ui, resp: &Response) {
    let plan = &mut app.plan;
    if let Some(hover) = resp.hover_pos() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta().y, i.zoom_delta()));
        let factor = (-scroll as f64 * 0.0015).exp() / zoom as f64;
        if (factor - 1.0).abs() > 1e-6 {
            let view = View {
                rect: resp.rect,
                center: plan.center,
                mpp: plan.mm_per_pt,
            };
            let before = view.to_model(hover);
            plan.zoom_by(factor);
            let after = View {
                mpp: plan.mm_per_pt,
                ..view
            }
            .to_model(hover);
            plan.center += before - after;
        }
    }
    let panning = resp.dragged_by(egui::PointerButton::Middle)
        || resp.dragged_by(egui::PointerButton::Secondary);
    if panning {
        let d = resp.drag_delta();
        plan.center -= DVec2::new(d.x as f64, -d.y as f64) * plan.mm_per_pt;
        plan.drag = Some(Drag::Pan);
    } else if matches!(plan.drag, Some(Drag::Pan)) {
        plan.drag = None;
    }
}

/// Objects under a model point, most specific first.
fn hit_test(doc: &Document, cache: &Cache, p: DVec2, tol: f64) -> Vec<Id> {
    let mut hits: Vec<Id> = Vec::new();
    let sheet = cache.key.1;
    let on_sheet = doc.kind_of(sheet) != Some(ObjectKind::Floor);
    for item in cache.drawing.items.iter().rev() {
        let Some(id) = item.object else {
            continue;
        };
        if hits.contains(&id) || (on_sheet && doc.floor_of(id) != Some(sheet)) {
            continue;
        }
        let hit = match &item.prim {
            Prim::Image { corners, .. } => point_in_polygon(p, corners),
            Prim::Polyline { points, closed, .. } => {
                distance_to_polyline(p, points, *closed) <= tol
            }
            Prim::Polygon { points, .. } => point_in_polygon(p, points),
            Prim::Text {
                at,
                text,
                height,
                rotation,
                justify,
                ..
            } => {
                let local = (p - *at).rotate(DVec2::from_angle(-rotation.to_radians()));
                let w = approx_text_width(text, *height);
                let x0 = match justify {
                    Justify::Left => 0.0,
                    Justify::Center => -w / 2.0,
                    Justify::Right => -w,
                };
                local.x >= x0 - tol
                    && local.x <= x0 + w + tol
                    && local.y >= -height * 0.3 - tol
                    && local.y <= height * 1.2 + tol
            }
        };
        if hit {
            hits.push(id);
        }
    }
    for room in doc.rooms_on(cache.key.1) {
        if let Some(i) = cache.regions.room_at(v2(room.anchor))
            && point_in_polygon(p, &cache.regions.rooms[i].polygon)
            && !hits.contains(&room.id)
        {
            hits.push(room.id);
        }
    }
    let rank = |id: &Id| match doc.kind_of(*id) {
        Some(ObjectKind::Opening) => 0,
        Some(ObjectKind::Text) => 1,
        Some(ObjectKind::Dimension) => 2,
        Some(ObjectKind::Cad) => 3,
        Some(ObjectKind::Wall) => 4,
        Some(ObjectKind::Room) => 6,
        _ => 5,
    };
    hits.sort_by_key(rank);
    hits
}

/// Editing handles of a single selected object: (handle, model position).
fn handles(doc: &Document, id: Id) -> Vec<(Handle, DVec2)> {
    match doc.object(id) {
        Some(Object::Wall(w)) => vec![
            (Handle::WallStart, v2(w.start)),
            (Handle::WallEnd, v2(w.end)),
        ],
        Some(Object::Cad(c)) => match &c.shape {
            Shape::Line { a, b, .. } => {
                vec![(Handle::Vertex(0), v2(*a)), (Handle::Vertex(1), v2(*b))]
            }
            Shape::Polyline { points, .. } | Shape::Spline { points, .. } => points
                .iter()
                .enumerate()
                .map(|(i, p)| (Handle::Vertex(i), v2(*p)))
                .collect(),
            Shape::Circle { center, radius } | Shape::Arc { center, radius, .. } => {
                vec![(Handle::Radius, v2(*center) + DVec2::X * radius.mm())]
            }
            _ => Vec::new(),
        },
        Some(Object::Text(t)) => vec![(Handle::TextPosition, v2(t.position))],
        Some(Object::Dimension(d)) => match &d.geometry {
            DimensionGeometry::Linear {
                points,
                orientation,
                offset,
            } if points.len() >= 2 => {
                vec![(
                    Handle::DimOffset,
                    dim_line_mid(points, *orientation, offset.mm()),
                )]
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The fixed point a handle drags from (for angle snapping).
fn handle_anchor(doc: &Document, id: Id, handle: Handle) -> Option<DVec2> {
    match (doc.object(id)?, handle) {
        (Object::Wall(w), Handle::WallStart) => Some(v2(w.end)),
        (Object::Wall(w), Handle::WallEnd) => Some(v2(w.start)),
        (Object::Cad(c), Handle::Vertex(i)) => match &c.shape {
            Shape::Line { a, b, .. } => Some(v2(if i == 0 { *b } else { *a })),
            Shape::Polyline { points, .. } if i > 0 => Some(v2(points[i - 1])),
            _ => None,
        },
        (Object::Cad(c), Handle::Radius) => match &c.shape {
            Shape::Circle { center, .. } | Shape::Arc { center, .. } => Some(v2(*center)),
            _ => None,
        },
        _ => None,
    }
}

fn dim_line_mid(points: &[Point2], orientation: DimOrientation, offset: f64) -> DVec2 {
    let a = v2(points[0]);
    let b = v2(points[points.len() - 1]);
    match orientation {
        DimOrientation::Aligned => {
            let u = (b - a).normalize_or(DVec2::X);
            (a + b) / 2.0 + DVec2::new(-u.y, u.x) * offset
        }
        DimOrientation::Horizontal => DVec2::new((a.x + b.x) / 2.0, a.y + offset),
        DimOrientation::Vertical => DVec2::new(a.x + offset, (a.y + b.y) / 2.0),
    }
}

/// Command for dragging `handle` of `id` to `p`.
fn handle_command(doc: &Document, id: Id, handle: Handle, p: DVec2) -> Option<Command> {
    let patch = |v: serde_json::Value| Command::Update {
        id,
        set: v.as_object().cloned().unwrap_or_default(),
    };
    match (doc.object(id)?, handle) {
        (Object::Wall(_), Handle::WallStart) => Some(Command::MoveWall {
            id,
            start: Some(pt(p)),
            end: None,
            detach: false,
        }),
        (Object::Wall(_), Handle::WallEnd) => Some(Command::MoveWall {
            id,
            start: None,
            end: Some(pt(p)),
            detach: false,
        }),
        (Object::Cad(c), Handle::Vertex(i)) => {
            let mut shape = c.shape.clone();
            match &mut shape {
                Shape::Line { a, b, .. } => *(if i == 0 { a } else { b }) = pt(p),
                Shape::Polyline { points, .. } | Shape::Spline { points, .. } => points[i] = pt(p),
                _ => return None,
            }
            Some(patch(serde_json::json!({ "shape": shape })))
        }
        (Object::Cad(c), Handle::Radius) => {
            let mut shape = c.shape.clone();
            match &mut shape {
                Shape::Circle { center, radius } | Shape::Arc { center, radius, .. } => {
                    *radius = Length::from_mm(v2(*center).distance(p).max(1.0));
                }
                _ => return None,
            }
            Some(patch(serde_json::json!({ "shape": shape })))
        }
        (Object::Text(_), Handle::TextPosition) => {
            Some(patch(serde_json::json!({ "position": pt(p) })))
        }
        (Object::Dimension(d), Handle::DimOffset) => {
            let DimensionGeometry::Linear {
                points,
                orientation,
                ..
            } = &d.geometry
            else {
                return None;
            };
            let a = v2(points[0]);
            let b = v2(points[points.len() - 1]);
            let offset = match orientation {
                DimOrientation::Aligned => {
                    let u = (b - a).normalize_or(DVec2::X);
                    (p - a).dot(DVec2::new(-u.y, u.x))
                }
                DimOrientation::Horizontal => p.y - a.y,
                DimOrientation::Vertical => p.x - a.x,
            };
            let mut g = d.geometry.clone();
            if let DimensionGeometry::Linear { offset: o, .. } = &mut g {
                *o = Length::from_mm(offset);
            }
            Some(patch(serde_json::json!({ "geometry": g })))
        }
        _ => None,
    }
}

fn select_tool(app: &mut App, ui: &Ui, resp: &Response, view: &View, commands: &mut Vec<Command>) {
    let doc = app.session.document();
    let cache = app.plan.cache.as_ref().expect("cache built");
    let tol = PICK_PX * view.mpp;
    let ctrl = ui.input(|i| i.modifiers.command || i.modifiers.shift);
    app.plan.hover = app
        .plan
        .cursor
        .and_then(|c| hit_test(doc, cache, c, tol).into_iter().next());

    if resp.drag_started_by(egui::PointerButton::Primary)
        && let Some(press) = resp.interact_pointer_pos()
    {
        let m = view.to_model(press);
        // Handles of a single selection take priority.
        let single = if app.selection.len() == 1 {
            handles(doc, app.selection[0])
        } else {
            Vec::new()
        };
        let handle = single
            .into_iter()
            .find(|(_, hp)| hp.distance(m) <= tol * 1.3);
        if let Some((h, _)) = handle {
            app.plan.drag = Some(Drag::Handle {
                id: app.selection[0],
                handle: h,
            });
        } else if let Some(hit) = hit_test(doc, cache, m, tol)
            .into_iter()
            .find(|id| doc.kind_of(*id) != Some(ObjectKind::Room))
        {
            if !app.selection.contains(&hit) {
                app.selection = vec![hit];
            }
            // Grab from the nearest snap point on the object for point-to-point moves.
            let grab = snap(m, tol, &cache.targets, &app.prefs.snap, None).point;
            app.plan.drag = Some(Drag::Move {
                ids: app.selection.clone(),
                grab,
            });
        } else {
            app.plan.drag = Some(Drag::Box { start: press });
        }
    }

    if resp.drag_stopped_by(egui::PointerButton::Primary) {
        let cursor = app.plan.cursor;
        match app.plan.drag.take() {
            Some(Drag::Move { ids, grab }) => {
                if let Some(c) = cursor {
                    let dest = snap(c, tol, &cache.targets, &app.prefs.snap, Some(grab)).point;
                    let d = dest - grab;
                    if d.length() > 0.5 {
                        commands.push(move_command(doc, &ids, d));
                    }
                }
            }
            Some(Drag::Handle { id, handle }) => {
                if let Some(s) = app.plan.snap
                    && let Some(cmd) = handle_command(doc, id, handle, s.point)
                {
                    commands.push(cmd);
                }
            }
            Some(Drag::Box { start }) => {
                if let Some(end) = resp.interact_pointer_pos().or(resp.hover_pos()) {
                    let (a, b) = (view.to_model(start), view.to_model(end));
                    let (lo, hi) = (a.min(b), a.max(b));
                    let mut picked: Vec<Id> = Vec::new();
                    for item in &cache.drawing.items {
                        if let Some(id) = item.object
                            && !picked.contains(&id)
                            && prim_points(&item.prim)
                                .iter()
                                .any(|p| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y)
                            && doc.kind_of(id) != Some(ObjectKind::Room)
                        {
                            picked.push(id);
                        }
                    }
                    if ctrl {
                        for id in picked {
                            if !app.selection.contains(&id) {
                                app.selection.push(id);
                            }
                        }
                    } else {
                        app.selection = picked;
                    }
                }
            }
            other => app.plan.drag = other,
        }
    }

    // Alt+click a linked callout to open what it points at.
    if resp.clicked()
        && ui.input(|i| i.modifiers.alt)
        && let Some(c) = app.plan.cursor
        && let Some(Object::Callout(co)) = hit_test(doc, cache, c, tol).first().and_then(|id| doc.object(*id))
        && let Some(link) = co.link
    {
        app.open_link(link);
        return;
    }
    // Alt+click a note's link to open it.
    if resp.clicked()
        && ui.input(|i| i.modifiers.alt)
        && let Some(c) = app.plan.cursor
        && let Some(Object::Text(t)) = hit_test(doc, cache, c, tol).first().and_then(|id| doc.object(*id))
        && let Some(url) = housecraft_draft::rich::links(&t.text).into_iter().next()
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        return;
    }
    if resp.clicked()
        && let Some(c) = app.plan.cursor
        && let Some(pick) = app.plan.pick.clone()
    {
        let hit = hit_test(doc, cache, c, tol).into_iter().next();
        if let Some(target) = hit {
            match pick {
                Pick::Match(from) => {
                    if doc.kind_of(target) == doc.kind_of(from) {
                        commands.push(Command::MatchProperties { from, to: vec![target] });
                    }
                }
                Pick::Center(ids) => {
                    let center = |set: &[Id]| object_bounds(cache, set).map(|(lo, hi)| (lo + hi) / 2.0);
                    if let Some(at) = center(&ids) {
                        // Along a wall or line: slide to its middle; on anything
                        // else, to the middle of what was clicked.
                        let edge = doc.get::<Wall>(target).map(|w| (v2(w.start), v2(w.end))).or_else(|| {
                            doc.get::<housecraft_api::model::Cad>(target).and_then(|cad| match cad.shape {
                                Shape::Line { a, b, .. } => Some((v2(a), v2(b))),
                                _ => None,
                            })
                        });
                        let d = match edge {
                            Some((a, b)) => {
                                let dir = (b - a).normalize_or_zero();
                                dir * ((a + b) / 2.0 - at).dot(dir)
                            }
                            None => center(&[target]).map_or(DVec2::ZERO, |m| m - at),
                        };
                        if d.length() > 0.5 {
                            commands.push(Command::Transform {
                                ids,
                                transform: TransformOp::Translate {
                                    dx: Length::from_mm(d.x),
                                    dy: Length::from_mm(d.y),
                                },
                                copies: 0,
                            });
                        }
                    }
                    app.plan.pick = None;
                }
                Pick::PaintLayer(layer) => {
                    let alt = ui.input(|i| i.modifiers.alt);
                    match layer {
                        Some(l) if !alt => {
                            let has_layer = serde_json::to_value(doc.object(target)).is_ok_and(|v| v.get("layer").is_some());
                            if has_layer && doc.layer_of(target) != Some(l) {
                                commands.push(Command::Update {
                                    id: target,
                                    set: serde_json::json!({ "layer": l }).as_object().cloned().unwrap_or_default(),
                                });
                            }
                        }
                        _ => app.plan.pick = Some(Pick::PaintLayer(doc.layer_of(target))),
                    }
                }
                Pick::HideLayer => {
                    if let Some(layer) = doc.layer_of(target) {
                        app.selection.retain(|id| doc.layer_of(*id) != Some(layer));
                        commands.push(Command::SetLayerDisplay {
                            layer_set: app.plan.layer_set,
                            layer,
                            visible: Some(false),
                            color: None,
                            line_weight: None,
                            line_style: None,
                        });
                    }
                }
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.plan.pick = None;
        }
        return;
    }
    if resp.clicked()
        && let Some(c) = app.plan.cursor
    {
        let hits = hit_test(doc, cache, c, tol);
        app.plan.candidates = hits.clone();
        match hits.first() {
            Some(id) if ctrl => {
                if let Some(i) = app.selection.iter().position(|x| x == id) {
                    app.selection.remove(i);
                } else {
                    app.selection.push(*id);
                }
            }
            Some(id) => app.selection = vec![*id],
            None if !ctrl => app.selection.clear(),
            None => {}
        }
    }
    if resp.double_clicked() {
        app.open_object();
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        if app.plan.pick.take().is_none() {
            app.selection.clear();
        }
        app.plan.drag = None;
    }
}

/// The arc from `s` through `m` to `e`: centre, radius, start angle and
/// counter-clockwise sweep (degrees). `None` when the points are in a line.
fn arc_through(s: DVec2, m: DVec2, e: DVec2) -> Option<(DVec2, f64, f64, f64)> {
    let d = 2.0 * (s.x * (m.y - e.y) + m.x * (e.y - s.y) + e.x * (s.y - m.y));
    if d.abs() < 1e-6 {
        return None;
    }
    let sq = |p: DVec2| p.length_squared();
    let c = DVec2::new(
        (sq(s) * (m.y - e.y) + sq(m) * (e.y - s.y) + sq(e) * (s.y - m.y)) / d,
        (sq(s) * (e.x - m.x) + sq(m) * (s.x - e.x) + sq(e) * (m.x - s.x)) / d,
    );
    let ang = |p: DVec2| (p - c).to_angle().to_degrees();
    let (a_s, a_m, a_e) = (ang(s), ang(m), ang(e));
    let to_end = (a_e - a_s).rem_euclid(360.0);
    let to_mid = (a_m - a_s).rem_euclid(360.0);
    Some(if to_mid < to_end {
        (c, c.distance(s), a_s, to_end)
    } else {
        // Clockwise from s: the same arc counter-clockwise from e.
        (c, c.distance(s), a_e, 360.0 - to_end)
    })
}

/// How far the centreline lies left of the line the wall tools draw along.
fn wall_offset(doc: &Document, interior: bool) -> f64 {
    let d = doc.active_defaults();
    let ty = if interior { d.interior_wall_type } else { d.exterior_wall_type };
    doc.get::<WallType>(ty).map_or(0.0, |t| t.centerline_offset(d.wall_reference))
}

/// `p` moved `o` to the left of the direction a→b.
fn offset_pt(a: DVec2, b: DVec2, p: DVec2, o: f64) -> DVec2 {
    let d = (b - a).normalize_or_zero();
    p + DVec2::new(-d.y, d.x) * o
}

/// Where the lines a→b and b→c, each moved `o` to their left, meet.
fn miter(a: DVec2, b: DVec2, c: DVec2, o: f64) -> DVec2 {
    let (p1, d1) = (offset_pt(a, b, a, o), (b - a).normalize_or_zero());
    let (p2, d2) = (offset_pt(b, c, b, o), (c - b).normalize_or_zero());
    let den = d1.perp_dot(d2);
    if den.abs() < 1e-9 {
        return offset_pt(a, b, b, o);
    }
    let t = (p2 - p1).perp_dot(d2) / den;
    p1 + d1 * t
}

/// What the next plan click is for, after an edit button.
#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    /// Match Properties: each click copies this object's properties.
    Match(Id),
    /// Center Object: one click on a wall, line or object centres these.
    Center(Vec<Id>),
    /// Layer Hider: each click hides the layer of what was clicked.
    HideLayer,
    /// Layer Painter: the first click picks a layer (Alt+click picks
    /// again); later clicks move objects onto it.
    PaintLayer(Option<Id>),
}

impl Pick {
    /// Status-bar instructions while waiting for the click.
    pub fn hint(&self) -> &'static str {
        match self {
            Pick::Match(_) => "Match Properties: click each object of the same kind to copy onto. Esc ends.",
            Pick::Center(_) => "Center Object: click a wall or line to centre along, or an object to centre on. Esc cancels.",
            Pick::HideLayer => "Layer Hider: click objects to hide their layers. Esc ends.",
            Pick::PaintLayer(None) => "Layer Painter: click an object to pick up its layer.",
            Pick::PaintLayer(Some(_)) => "Layer Painter: click objects to move them onto the picked layer (Alt+click picks another). Esc ends.",
        }
    }
}

/// Model-space bounds of everything drawn for `ids`.
fn object_bounds(cache: &Cache, ids: &[Id]) -> Option<(DVec2, DVec2)> {
    housecraft_api::geom::math::bounds(
        cache
            .drawing
            .items
            .iter()
            .filter(|i| i.object.is_some_and(|o| ids.contains(&o)))
            .flat_map(|i| prim_points(&i.prim)),
    )
}

impl PlanState {
    /// Bounds of what is drawn for `ids` in the plan (once drawn).
    pub fn bounds_of(&self, ids: &[Id]) -> Option<(DVec2, DVec2)> {
        object_bounds(self.cache.as_ref()?, ids)
    }
}

/// Moving a lone wall drags joined walls with it; anything else is a translate.
fn move_command(doc: &Document, ids: &[Id], d: DVec2) -> Command {
    if let [id] = ids
        && let Some(w) = doc.get::<Wall>(*id)
    {
        return Command::MoveWall {
            id: *id,
            start: Some(pt(v2(w.start) + d)),
            end: Some(pt(v2(w.end) + d)),
            detach: false,
        };
    }
    Command::Transform {
        ids: ids.to_vec(),
        transform: TransformOp::Translate {
            dx: Length::from_mm(d.x),
            dy: Length::from_mm(d.y),
        },
        copies: 0,
    }
}

fn drawing_tool(app: &mut App, ui: &Ui, resp: &Response, view: &View, commands: &mut Vec<Command>) {
    let tool = app.plan.tool;
    let placing = app.library.placing.clone();
    // Material for new regions: the painter's, else the first in the plan.
    let region_material = app
        .view3d
        .painter
        .and_then(|p| p.material)
        .or_else(|| app.doc().iter::<Material>().next().map(|m| m.id));
    let group = app.library.group.clone();
    let floor = Some(app.plan.target());
    let doc = app.session.document();
    if app.plan.sheet.is_some() && !tool.is_drafting() {
        if resp.clicked() {
            app.error(format!(
                "{} works on floor plans; in views and CAD details use the CAD, text and dimension tools.",
                tool.name()
            ));
        }
        return;
    }

    // Typed length while drawing walls and lines (DIM-05).
    let length_tools = matches!(
        tool,
        Tool::ExteriorWall | Tool::InteriorWall | Tool::Line | Tool::Arrow | Tool::Polyline
    );
    if length_tools && !app.plan.pending.is_empty() && app.plan.text_prompt.is_none() {
        let typed: String = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .collect()
        });
        if !typed.is_empty() {
            app.plan
                .length_input
                .get_or_insert_with(String::new)
                .push_str(&typed);
        }
        if let Some(buf) = &mut app.plan.length_input
            && ui.input(|i| i.key_pressed(egui::Key::Backspace))
        {
            buf.pop();
        }
    }

    let mut click: Option<DVec2> = None;
    if resp.clicked() {
        click = app.plan.snap.map(|s| s.point);
    }
    if let Some(buf) = app.plan.length_input.clone()
        && ui.input(|i| i.key_pressed(egui::Key::Enter))
    {
        app.plan.length_input = None;
        if let (Some(len), Some(last), Some(s)) = (
            crate::util::parse(doc, &buf),
            app.plan.pending.last(),
            app.plan.snap,
        ) {
            let dir = (s.point - *last).normalize_or(DVec2::X);
            click = Some(*last + dir * len.mm());
        }
    }
    let finish = resp.double_clicked()
        || resp.secondary_clicked()
        || (app.plan.length_input.is_none() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        if app.plan.pending.is_empty() && app.plan.length_input.is_none() {
            app.set_tool(Tool::Select);
        }
        app.plan.pending.clear();
        app.plan.length_input = None;
        return;
    }

    let app_sides = app.plan.polygon_sides;
    let app_space_type = app.plan.space_type.clone();
    let pending = &mut app.plan.pending;
    if let Some(p) = click {
        match tool {
            Tool::ExteriorWall | Tool::InteriorWall => {
                if let Some(last) = pending.last().copied() {
                    if last.distance(p) >= 1.0 {
                        let o = wall_offset(doc, tool == Tool::InteriorWall);
                        let closes = pending.first().is_some_and(|f| f.distance(p) < 1.0)
                            && pending.len() > 1;
                        if o.abs() < 0.01 {
                            commands.push(Command::AddWall {
                                floor,
                                start: pt(last),
                                end: pt(p),
                                wall_type: None,
                                interior: tool == Tool::InteriorWall,
                                height: None,
                            });
                        } else {
                            // Drawn along a surface: offset to the centreline and
                            // mitre the corner with the previous wall.
                            let prev = pending.len().checked_sub(2).map(|i| pending[i]);
                            let start = match prev {
                                Some(a) => miter(a, last, p, o),
                                None => offset_pt(last, p, last, o),
                            };
                            let mut end = offset_pt(last, p, p, o);
                            if let (Some(prev_wall), Some(_)) = (app.plan.last_wall, prev) {
                                commands.push(Command::MoveWall { id: prev_wall, start: None, end: Some(pt(start)), detach: true });
                            }
                            if closes && pending.len() > 2 {
                                // Closing: mitre the last corner with the first wall too.
                                end = miter(last, pending[0], pending[1], o);
                            }
                            commands.push(Command::AddWall {
                                floor,
                                start: pt(start),
                                end: pt(end),
                                wall_type: None,
                                interior: tool == Tool::InteriorWall,
                                height: None,
                            });
                            if closes && pending.len() > 2
                                && let Some(first) = app.plan.first_wall
                            {
                                commands.push(Command::MoveWall { id: first, start: Some(pt(end)), end: None, detach: true });
                            }
                        }
                        if closes {
                            pending.clear();
                        } else {
                            pending.push(p);
                        }
                    }
                } else {
                    pending.push(p);
                    app.plan.last_wall = None;
                    app.plan.first_wall = None;
                }
            }
            Tool::Dormer(style) => commands.push(Command::AddDormer {
                floor: Some(app.plan.floor),
                at: pt(p),
                facing: None,
                width: None,
                wall_height: None,
                pitch: None,
                style,
            }),
            Tool::Skylight => commands.push(Command::AddSkylight { at: pt(p), width: None, length: None }),
            Tool::BayWindow(_) | Tool::CornerWindow => {
                let tol = PICK_PX * view.mpp;
                let hit = wall_geoms(doc, app.plan.floor)
                    .into_iter()
                    .map(|g| (housecraft_api::geom::math::distance_to_segment(p, g.start, g.end) - g.half, g))
                    .filter(|(d, _)| *d <= tol)
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, g)| g);
                if let Some(g) = hit {
                    let u = (p - g.start).dot(g.dir).clamp(0.0, g.length);
                    commands.push(match tool {
                        Tool::BayWindow(bay) => {
                            let bow = bay == housecraft_api::BayStyle::Bow;
                            Command::AddBayWindow {
                                wall: g.id,
                                position: Length::from_mm(u),
                                width: Length::from_feet(8.0),
                                depth: Length::from_inches(if bow { 18.0 } else { 24.0 }),
                                bay,
                                faces: None,
                                style: None,
                            }
                        }
                        _ => Command::AddCornerWindow {
                            wall: g.id,
                            at_start: u < g.length / 2.0,
                            width: Length::from_feet(3.0),
                            height: None,
                            sill: None,
                            style: None,
                        },
                    });
                }
            }
            Tool::Door(_) | Tool::Window(_) => {
                let tol = PICK_PX * view.mpp;
                let wall = wall_geoms(doc, app.plan.floor)
                    .into_iter()
                    .map(|g| {
                        (
                            housecraft_api::geom::math::distance_to_segment(p, g.start, g.end)
                                - g.half,
                            g.id,
                        )
                    })
                    .filter(|(d, _)| *d <= tol)
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, id)| id);
                let template = app
                    .library
                    .opening
                    .as_ref()
                    .filter(|(t, _)| *t == tool)
                    .map(|(_, v)| v.clone());
                match wall {
                    Some(wall) if template.is_some() => commands.push(Command::PlaceOpening {
                        wall,
                        at: pt(p),
                        template: template.expect("checked"),
                    }),
                    Some(wall) => commands.push(match tool {
                        Tool::Door(style) => Command::AddDoor {
                            wall,
                            position: None,
                            at: Some(pt(p)),
                            style: Some(style),
                            width: None,
                            height: None,
                            hinge: None,
                            swing_in: None,
                        },
                        Tool::Window(ws) => Command::AddWindow {
                            wall,
                            position: None,
                            at: Some(pt(p)),
                            style: Some(ws),
                            width: None,
                            height: None,
                            sill: None,
                        },
                        _ => unreachable!(),
                    }),
                    None => app.status = Some(("Click on a wall to place it.".into(), true)),
                }
            }
            Tool::Line | Tool::Arrow => {
                if let Some(a) = pending.pop() {
                    if a.distance(p) > 0.5 {
                        commands.push(cad(
                            floor,
                            Shape::Line {
                                a: pt(a),
                                b: pt(p),
                                arrow_start: false,
                                arrow_end: tool == Tool::Arrow,
                            },
                        ));
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::Stair(kind) => {
                if let Some(a) = pending.pop() {
                    if a.distance(p) > 1.0 {
                        let dir = p - a;
                        let shape = match kind {
                            StairKind::Straight => StairShape::Straight,
                            StairKind::L => StairShape::L {
                                turn: Turn::Right,
                                first_treads: 0,
                            },
                            StairKind::U => StairShape::U {
                                turn: Turn::Right,
                                first_treads: 0,
                                gap: Length::ZERO,
                            },
                        };
                        commands.push(Command::AddStair {
                            floor,
                            start: pt(a),
                            direction: dir.y.atan2(dir.x).to_degrees(),
                            shape,
                            width: None,
                            rise: None,
                            railing_left: None,
                            railing_right: None,
                            stairwell: true,
                        });
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::Electrical(kind) => commands.push(Command::AddElectrical {
                floor,
                electrical_kind: kind,
                at: pt(p),
                rotation: None,
                height: None,
                controls: Vec::new(),
            }),
            Tool::ConnectSwitch => {
                let reach = (PICK_PX * view.mpp).max(250.0);
                let hit = doc
                    .iter::<Electrical>()
                    .filter(|e| e.floor == app.plan.floor)
                    .map(|e| (v2(e.position).distance(p), e))
                    .filter(|(d, _)| *d <= reach)
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, e)| e);
                match (hit, app.plan.wiring) {
                    (Some(e), _) if e.electrical_kind.is_switch() => {
                        app.plan.wiring = Some(e.id);
                        app.status =
                            Some(("Now click the fixtures this switch controls.".into(), false));
                    }
                    (Some(e), Some(sw)) => {
                        let wired = doc
                            .get::<Electrical>(sw)
                            .is_some_and(|s| s.controls.contains(&e.id));
                        commands.push(Command::ConnectSwitch {
                            switch: sw,
                            fixtures: vec![e.id],
                            disconnect: wired,
                        });
                    }
                    (Some(_), None) => {
                        app.status = Some(("Click a switch first.".into(), true));
                    }
                    (None, _) => {
                        app.status = Some(("Click on a switch or fixture.".into(), true));
                    }
                }
            }
            Tool::SectionLine(kind) => {
                if let Some(eye) = pending.pop() {
                    let look = (p - eye).normalize_or_zero();
                    if look != DVec2::ZERO {
                        // The view looks left of its line, so run the line across the look.
                        let along = DVec2::new(look.y, -look.x);
                        let half = 1219.2; // 4' each side, just for the callout
                        commands.push(Command::AddSectionView {
                            name: None,
                            floor,
                            a: pt(eye - along * half),
                            b: pt(eye + along * half),
                            section_kind: kind,
                            depth: None,
                            clip_sides: false,
                        });
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::RoofPlane => pending.push(p),
            Tool::Polyline | Tool::Spline => {
                let closes = pending.len() > 2 && pending[0].distance(p) < PICK_PX * view.mpp;
                if closes {
                    let points: Vec<Point2> = pending.drain(..).map(pt).collect();
                    commands.push(cad(
                        floor,
                        if tool == Tool::Spline {
                            Shape::Spline {
                                points,
                                closed: true,
                            }
                        } else {
                            Shape::Polyline {
                                points,
                                closed: true,
                            }
                        },
                    ));
                } else {
                    pending.push(p);
                }
            }
            Tool::Box | Tool::Ellipse => {
                if let Some(a) = pending.pop() {
                    let (lo, hi) = (a.min(p), a.max(p));
                    if (hi - lo).min_element() > 0.5 {
                        commands.push(cad(
                            floor,
                            if tool == Tool::Box {
                                Shape::Polyline {
                                    points: vec![
                                        pt(lo),
                                        pt(DVec2::new(hi.x, lo.y)),
                                        pt(hi),
                                        pt(DVec2::new(lo.x, hi.y)),
                                    ],
                                    closed: true,
                                }
                            } else {
                                Shape::Ellipse {
                                    center: pt((lo + hi) / 2.0),
                                    rx: Length::from_mm((hi.x - lo.x) / 2.0),
                                    ry: Length::from_mm((hi.y - lo.y) / 2.0),
                                    rotation: 0.0,
                                }
                            },
                        ));
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::Circle => {
                if let Some(c) = pending.pop() {
                    if c.distance(p) > 0.5 {
                        commands.push(cad(
                            floor,
                            Shape::Circle {
                                center: pt(c),
                                radius: Length::from_mm(c.distance(p)),
                            },
                        ));
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::Arc => {
                pending.push(p);
                if pending.len() == 3 {
                    let (c, s, e) = (pending[0], pending[1], pending[2]);
                    pending.clear();
                    let a0 = (s - c).to_angle().to_degrees();
                    let mut sweep = ((e - c).to_angle().to_degrees() - a0).rem_euclid(360.0);
                    if sweep < 0.01 {
                        sweep = 360.0;
                    }
                    if c.distance(s) > 0.5 {
                        commands.push(cad(
                            floor,
                            Shape::Arc {
                                center: pt(c),
                                radius: Length::from_mm(c.distance(s)),
                                start_angle: a0,
                                sweep,
                            },
                        ));
                    }
                }
            }
            Tool::ArcThreePoint => {
                pending.push(p);
                if pending.len() == 3 {
                    let (s, m, e) = (pending[0], pending[1], pending[2]);
                    pending.clear();
                    match arc_through(s, m, e) {
                        Some((c, r, a0, sweep)) => commands.push(cad(
                            floor,
                            Shape::Arc { center: pt(c), radius: Length::from_mm(r), start_angle: a0, sweep },
                        )),
                        // In a line: just a line.
                        None => commands.push(cad(floor, Shape::Line { a: pt(s), b: pt(e), arrow_start: false, arrow_end: false })),
                    }
                }
            }
            Tool::RoomBox | Tool::SpaceBox => {
                if let Some(a) = pending.pop() {
                    if (a.x - p.x).abs() > 300.0 && (a.y - p.y).abs() > 300.0 {
                        let corners = [a, DVec2::new(p.x, a.y), p, DVec2::new(a.x, p.y)];
                        if tool == Tool::RoomBox {
                            commands.push(Command::AddWalls {
                                floor,
                                points: corners.iter().map(|c| pt(*c)).collect(),
                                closed: true,
                                wall_type: None,
                                interior: true,
                                height: None,
                            });
                        } else {
                            commands.push(cad(floor, Shape::Polyline { points: corners.iter().map(|c| pt(*c)).collect(), closed: true }));
                            commands.push(Command::AddText {
                                floor,
                                layer: None,
                                position: pt((a + p) / 2.0),
                                text: app_space_type.clone(),
                                style: None,
                                rotation: 0.0,
                                justify: Justify::Center,
                                height: None,
                                leaders: Vec::new(),
                            });
                        }
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::GridLine => {
                if let Some(a) = pending.pop() {
                    if a.distance(p) > 100.0 {
                        commands.push(Command::AddGridLine { floor, a: pt(a), b: pt(p) });
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::RegularPolygon => {
                if let Some(c) = pending.pop() {
                    let r = c.distance(p);
                    if r > 0.5 {
                        let n = app_sides.max(3);
                        let a0 = (p - c).to_angle();
                        let points = (0..n)
                            .map(|i| {
                                let a = a0 + i as f64 * std::f64::consts::TAU / n as f64;
                                pt(c + DVec2::from_angle(a) * r)
                            })
                            .collect();
                        commands.push(cad(floor, Shape::Polyline { points, closed: true }));
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::Point => commands.push(cad(floor, Shape::Point { at: pt(p) })),
            Tool::Cabinet(kind) => commands.push(Command::AddCabinet {
                floor,
                cabinet_kind: kind,
                at: pt(p),
                facing: None,
                width: None,
                depth: None,
                height: None,
                shape: CabinetShape::Standard,
                sink: false,
            }),
            Tool::SinkCabinet => commands.push(Command::AddCabinet {
                floor,
                cabinet_kind: CabinetKind::Base,
                at: pt(p),
                facing: None,
                width: None,
                depth: None,
                height: None,
                shape: CabinetShape::Standard,
                sink: true,
            }),
            Tool::Appliance(kind) => commands.push(Command::AddAppliance {
                floor,
                appliance_kind: kind,
                at: pt(p),
                facing: None,
                width: None,
                depth: None,
                height: None,
            }),
            Tool::Countertop
            | Tool::Backsplash
            | Tool::MoldingPolyline
            | Tool::FloorRegion
            | Tool::TerrainPerimeter
            | Tool::ElevationLine
            | Tool::FlatRegion
            | Tool::RetainingWall
            | Tool::Road(_)
            | Tool::Fence
            | Tool::GardenBed
            | Tool::SteppingStones
            | Tool::Stream
            | Tool::Curb
            | Tool::Deck
            | Tool::RaisedPlatform
            | Tool::RopeLight
            | Tool::Grass => pending.push(p),
            Tool::Plant(form) => commands.push(Command::AddPlant {
                floor,
                at: pt(p),
                name: None,
                form,
                height: None,
                spread: None,
            }),
            Tool::ElevationPoint => commands.push(Command::AddElevationData {
                floor,
                shape: ElevationShape::Point {
                    at: pt(p),
                    elevation: Length::from_mm(ground_at(doc, p)),
                },
            }),
            Tool::Post | Tool::Beam => {
                let fl = doc.get::<Floor>(app.plan.floor);
                let (z0, zc) = fl.map_or((0.0, 2438.4), |f| (f.elevation.mm(), (f.elevation + f.ceiling_height).mm()));
                let p3 = |q: DVec2, z: f64| housecraft_api::units::Point3::from_mm(q.x, q.y, z);
                if tool == Tool::Post {
                    commands.push(Command::AddFramingMember {
                        floor,
                        role: FramingRole::Post,
                        a: p3(p, z0),
                        b: p3(p, zc),
                        width: None,
                        depth: None,
                        rotation: 0.0,
                        material: FramingMaterial::Lumber,
                        plies: None,
                    });
                } else if let Some(a) = pending.pop() {
                    if a.distance(p) > 100.0 {
                        let depth = doc.active_defaults().framing.floor_joist_depth.mm();
                        commands.push(Command::AddFramingMember {
                            floor,
                            role: FramingRole::Beam,
                            a: p3(a, zc - depth / 2.0),
                            b: p3(p, zc - depth / 2.0),
                            width: None,
                            depth: None,
                            rotation: 0.0,
                            material: FramingMaterial::Lumber,
                            plies: Some(2),
                        });
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::JoistDirection => {
                if let Some(c) = pending.pop() {
                    if c.distance(p) > 10.0 {
                        commands.push(Command::AddJoistDirection {
                            floor,
                            at: pt(c),
                            angle: (p - c).to_angle().to_degrees(),
                            ceiling: false,
                        });
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::Hill | Tool::Valley => {
                if let Some(c) = pending.pop() {
                    let radius = Length::from_mm(c.distance(p));
                    if radius.mm() > 10.0 {
                        commands.push(Command::AddElevationData {
                            floor,
                            shape: if tool == Tool::Hill {
                                ElevationShape::Hill { center: pt(c), radius, height: Length::from_feet(4.0) }
                            } else {
                                ElevationShape::Valley { center: pt(c), radius, depth: Length::from_feet(3.0) }
                            },
                        });
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::WallRegion => {
                if let Some(a) = pending.pop() {
                    if let Some(cmd) = wall_region(doc, app.plan.floor, a, p, region_material) {
                        commands.push(cmd);
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::ObjectPainter => {
                let tol = PICK_PX * view.mpp;
                let hit = app
                    .plan
                    .cache
                    .as_ref()
                    .and_then(|c| hit_test(doc, c, p, tol).into_iter().next());
                let alt = ui.input(|i| i.modifiers.alt);
                if let Some(id) = hit {
                    match &app.object_painter {
                        Some(op) if !alt => {
                            if let Some(cmd) = op.apply_to(doc, id) {
                                commands.push(cmd);
                            }
                        }
                        _ => app.object_painter = Some(crate::object_painter::ObjectPainter::new(id)),
                    }
                }
            }
            Tool::PlaceGroup => {
                if let Some((objects, origin)) = &group {
                    commands.push(Command::InsertObjects {
                        objects: objects.clone(),
                        floor,
                        dx: Length::from_mm(p.x - origin[0]),
                        dy: Length::from_mm(p.y - origin[1]),
                    });
                }
            }
            Tool::PlaceSymbol(def) => commands.push(Command::AddSymbol {
                floor,
                def,
                at: pt(p),
                facing: None,
                elevation: None,
            }),
            Tool::PlaceTemplate => {
                if let Some((kind, template, _)) = &placing {
                    commands.push(Command::PlaceObject {
                        floor,
                        kind: *kind,
                        template: template.clone(),
                        at: pt(p),
                        facing: None,
                    });
                }
            }
            Tool::PlaceBlock(block) => commands.push(Command::AddBlockRef {
                floor,
                block,
                at: pt(p),
                rotation: 0.0,
                scale: None,
                layer: None,
            }),
            Tool::Dimension => {
                pending.push(p);
                if pending.len() == 3 {
                    let (a, b, o) = (pending[0], pending[1], pending[2]);
                    pending.clear();
                    let u = (b - a).normalize_or(DVec2::X);
                    let offset = (o - a).dot(DVec2::new(-u.y, u.x));
                    if a.distance(b) > 0.5 {
                        commands.push(Command::AddDimension {
                            floor,
                            layer: None,
                            geometry: DimensionGeometry::Linear {
                                points: vec![pt(a), pt(b)],
                                orientation: DimOrientation::Aligned,
                                offset: Length::from_mm(offset),
                            },
                            text_override: None,
                        });
                    }
                }
            }
            Tool::AngularDimension => {
                pending.push(p);
                if pending.len() == 4 {
                    let (c, s, e, r) = (pending[0], pending[1], pending[2], pending[3]);
                    pending.clear();
                    if c.distance(s) > 0.5 && c.distance(e) > 0.5 {
                        commands.push(Command::AddDimension {
                            floor,
                            layer: None,
                            geometry: DimensionGeometry::Angular {
                                center: pt(c),
                                start: pt(s),
                                end: pt(e),
                                radius: Length::from_mm(c.distance(r).max(100.0)),
                            },
                            text_override: None,
                        });
                    }
                }
            }
            Tool::Text => app.plan.text_prompt = Some((p, String::new())),
            Tool::TextArrow => {
                if let Some(tip) = pending.pop() {
                    app.plan.prompt_leaders = vec![tip];
                    app.plan.text_prompt = Some((p, String::new()));
                } else {
                    pending.push(p);
                }
            }
            Tool::Solid => {
                if let Some(a) = pending.pop() {
                    if (a.x - p.x).abs() > 1.0 && (a.y - p.y).abs() > 1.0 {
                        let s = &doc.active_defaults().solids;
                        commands.push(Command::AddSolid {
                            floor,
                            shape: s.preset.shape(pt(a), pt(p), s.height),
                            elevation: Length::ZERO,
                            color: None,
                        });
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::CrossBox | Tool::BlockingBox | Tool::Insulation => {
                if let Some(a) = pending.pop() {
                    if a.distance(p) > 1.0 {
                        let (a, b) = (pt(a), pt(p));
                        let width =
                            Length::from_inches(if tool == Tool::Insulation { 3.5 } else { 5.5 });
                        commands.push(cad(
                            floor,
                            match tool {
                                Tool::CrossBox => Shape::CrossBox { a, b, width },
                                Tool::BlockingBox => Shape::BlockingBox { a, b, width },
                                _ => Shape::Insulation { a, b, width },
                            },
                        ));
                    }
                } else {
                    pending.push(p);
                }
            }
            Tool::Callout => {
                if let Some(target) = pending.pop() {
                    commands.push(Command::AddCallout {
                        floor,
                        text: "Note".into(),
                        at: pt(p),
                        target: Some(pt(target)),
                        marker: None,
                    });
                } else {
                    pending.push(p);
                }
            }
            Tool::Marker => {
                let n = doc.iter::<Callout>().filter(|c| c.marker.is_some()).count() + 1;
                commands.push(Command::AddCallout {
                    floor,
                    text: n.to_string(),
                    at: pt(p),
                    target: None,
                    marker: Some(MarkerShape::Hexagon),
                });
            }
            Tool::RevisionCloud => pending.push(p),
            Tool::Select => {}
        }
    }

    if finish && tool == Tool::RevisionCloud {
        let pending = std::mem::take(&mut app.plan.pending);
        if pending.len() >= 3 {
            commands.push(Command::AddRevisionCloud {
                floor,
                points: pending.iter().map(|p| pt(*p)).collect(),
                arc: None,
                label: None,
            });
        }
        return;
    }
    if finish && matches!(tool, Tool::Deck | Tool::RaisedPlatform) {
        let pending = std::mem::take(&mut app.plan.pending);
        if pending.len() >= 3 {
            commands.push(Command::AddDeck {
                floor,
                points: pending.iter().map(|p| pt(*p)).collect(),
                height: None,
                platform: tool == Tool::RaisedPlatform,
                curved: false,
            });
        }
        return;
    }
    if finish && let Some(kind) = tool.site_kind() {
        let pending = std::mem::take(&mut app.plan.pending);
        let min = if kind.closed() { 3 } else { 2 };
        if pending.len() >= min {
            commands.push(Command::AddSiteFeature {
                floor,
                points: pending.iter().map(|p| pt(*p)).collect(),
                feature: kind,
            });
        }
        return;
    }
    if finish && matches!(tool, Tool::TerrainPerimeter | Tool::ElevationLine | Tool::FlatRegion) {
        let pending = std::mem::take(&mut app.plan.pending);
        let points: Vec<Point2> = pending.iter().map(|p| pt(*p)).collect();
        let at = pending.first().map_or(0.0, |p| ground_at(doc, *p));
        match tool {
            Tool::TerrainPerimeter if points.len() >= 3 => commands.push(Command::AddTerrain { floor, points }),
            Tool::ElevationLine if points.len() >= 2 => commands.push(Command::AddElevationData {
                floor,
                shape: ElevationShape::Line { points, elevation: Length::from_mm(at), smooth: true },
            }),
            Tool::FlatRegion if points.len() >= 3 => commands.push(Command::AddElevationData {
                floor,
                shape: ElevationShape::Region { points, elevation: Length::from_mm(at) },
            }),
            _ => {}
        }
        return;
    }
    if finish && tool == Tool::FloorRegion {
        let pending = std::mem::take(&mut app.plan.pending);
        if pending.len() >= 3
            && let Some(material) = region_material
        {
            commands.push(Command::AddMaterialRegion {
                floor,
                material,
                target: RegionTarget::Floor {
                    points: pending.iter().map(|p| pt(*p)).collect(),
                },
            });
        }
        return;
    }
    if finish && tool == Tool::MoldingPolyline {
        let pending = std::mem::take(&mut app.plan.pending);
        if pending.len() >= 2 {
            commands.push(Command::AddMoldingPolyline {
                floor,
                points: pending.iter().map(|p| pt(*p)).collect(),
                closed: false,
                elevation: Length::from_inches(36.0),
                stack: vec![RoomMolding::base().spec],
            });
        }
        return;
    }
    if finish && tool == Tool::Backsplash {
        let pending = std::mem::take(&mut app.plan.pending);
        if pending.len() >= 2 {
            commands.push(Command::AddBacksplash {
                floor,
                points: pending.iter().map(|p| pt(*p)).collect(),
                bottom: None,
                height: None,
                thickness: None,
            });
        }
        return;
    }
    if finish && tool == Tool::Countertop {
        let pending = std::mem::take(&mut app.plan.pending);
        if pending.len() >= 3 {
            commands.push(Command::AddCountertop {
                floor,
                points: pending.iter().map(|p| pt(*p)).collect(),
                height: None,
                thickness: None,
                edge: None,
                holes: Vec::new(),
            });
        }
        return;
    }
    if finish && tool == Tool::RoofPlane {
        let pending = std::mem::take(&mut app.plan.pending);
        if pending.len() >= 3 {
            let doc = app.session.document();
            let d = doc.active_defaults();
            let eave_height = doc
                .get::<Floor>(app.plan.floor)
                .map_or(Length::from_feet(8.0), |f| f.ceiling_height)
                + d.roof.heel;
            commands.push(Command::AddRoofPlane {
                floor,
                points: pending.iter().map(|p| pt(*p)).collect(),
                eave_a: pt(pending[0]),
                eave_b: pt(pending[1]),
                eave_height,
                pitch: d.roof.pitch,
            });
        }
        return;
    }
    if finish {
        let pending = std::mem::take(&mut app.plan.pending);
        if matches!(tool, Tool::Polyline | Tool::Spline) && pending.len() >= 2 {
            let points: Vec<Point2> = pending.into_iter().map(pt).collect();
            commands.push(cad(
                floor,
                if tool == Tool::Spline {
                    Shape::Spline {
                        points,
                        closed: false,
                    }
                } else {
                    Shape::Polyline {
                        points,
                        closed: false,
                    }
                },
            ));
        }
        app.plan.length_input = None;
    }
}

/// A wall region between two clicks on one side of the nearest wall
/// (48" high, a wainscot, to start with).
fn wall_region(doc: &Document, floor: Id, a: DVec2, b: DVec2, material: Option<Id>) -> Option<Command> {
    let material = material?;
    let g = wall_geoms(doc, floor).into_iter().min_by(|g, h| {
        let d = |g: &housecraft_api::geom::walls::WallGeom| {
            housecraft_api::geom::math::distance_to_segment(a, g.start, g.end)
        };
        d(g).total_cmp(&d(h))
    })?;
    let along = |p: DVec2| (p - g.start).dot(g.dir).clamp(0.0, g.length);
    let (u0, u1) = (along(a).min(along(b)), along(a).max(along(b)));
    if u1 - u0 < 10.0 {
        return None;
    }
    let exterior = (a - g.start).dot(g.left) > 0.0;
    Some(Command::AddMaterialRegion {
        floor: Some(floor),
        material,
        target: RegionTarget::Wall {
            wall: g.id,
            exterior,
            u0: Length::from_mm(u0),
            u1: Length::from_mm(u1),
            z0: Length::ZERO,
            z1: Length::from_inches(48.0),
        },
    })
}

/// The terrain's height at `p` (mm), or 0 without a terrain: new elevation
/// data starts at the ground it is drawn on.
fn ground_at(doc: &Document, p: DVec2) -> f64 {
    doc.iter::<Terrain>()
        .next()
        .and_then(|t| housecraft_api::geom::terrain::surface(doc, t))
        .map_or(0.0, |s| s.height_at(p))
}

fn cad(floor: Option<Id>, shape: Shape) -> Command {
    Command::AddCad {
        floor,
        layer: None,
        shape,
        color: None,
        fill: None,
        fill_style: None,
    }
}

fn paint(app: &App, ui: &Ui, painter: &Painter, view: &View) {
    let rect = view.rect;
    let dark = ui.visuals().dark_mode;
    painter.rect_filled(
        rect,
        0.0,
        if dark {
            Color32::from_gray(28)
        } else {
            Color32::WHITE
        },
    );
    if app.prefs.show_grid {
        paint_grid(app, painter, view, dark);
    }
    let Some(cache) = &app.plan.cache else {
        return;
    };
    let accent = ui.visuals().selection.stroke.color;
    let accent = if accent == Color32::TRANSPARENT {
        Color32::from_rgb(255, 140, 0)
    } else {
        accent
    };
    let selected = |id: Option<Id>| id.is_some_and(|i| app.selection.contains(&i));

    // Selected rooms: tint their floor area.
    for id in &app.selection {
        if let Some(room) = app.doc().get::<Room>(*id)
            && let Some(i) = cache.regions.room_at(v2(room.anchor))
        {
            let poly = &cache.regions.rooms[i].polygon;
            paint_fill(
                painter,
                view,
                poly,
                &triangulate_poly(poly),
                accent.gamma_multiply(0.18),
            );
        }
    }

    let moving = match &app.plan.drag {
        Some(Drag::Move { grab, .. }) => app.plan.cursor.map(|c| c - *grab),
        _ => None,
    };

    if app.plan.sheet.is_some() {
        // Views paint far to near: fills and lines in the drawing's order, so
        // nearer surfaces hide what is behind them.
        let mut fills = cache.fills.iter();
        for item in &cache.drawing.items {
            if let Prim::Polygon { .. } = item.prim {
                if let Some((points, tris, fill)) = fills.next() {
                    paint_fill(painter, view, points, tris, fill_color(ui, *fill));
                }
            } else {
                paint_prim(ui, painter, view, &item.prim, None, 0.0);
            }
        }
    } else {
        for (points, tris, fill) in &cache.fills {
            paint_fill(painter, view, points, tris, fill_color(ui, *fill));
        }
        for item in &cache.drawing.items {
            paint_prim(ui, painter, view, &item.prim, None, 0.0);
        }
    }
    for item in &cache.drawing.items {
        if selected(item.object) {
            paint_prim(ui, painter, view, &item.prim, Some(accent), 2.0);
            if let Some(d) = moving {
                let shifted = shift_prim(&item.prim, d);
                paint_prim(
                    ui,
                    painter,
                    view,
                    &shifted,
                    Some(accent.gamma_multiply(0.6)),
                    1.0,
                );
            }
        } else if item.object.is_some() && item.object == app.plan.hover && app.plan.drag.is_none()
        {
            paint_prim(
                ui,
                painter,
                view,
                &item.prim,
                Some(accent.gamma_multiply(0.55)),
                1.0,
            );
        }
    }

    // Handles and temporary dimension for a single selection.
    if app.selection.len() == 1 && app.plan.tool == Tool::Select {
        let id = app.selection[0];
        for (_, p) in handles(app.doc(), id) {
            let s = view.to_screen(p);
            painter.rect(
                Rect::from_center_size(s, vec2(8.0, 8.0)),
                1.0,
                Color32::WHITE,
                Stroke::new(1.5, accent),
                egui::StrokeKind::Middle,
            );
        }
        temp_dimension(app, painter, view, id, accent);
    }
    if let Some(Drag::Handle { id, handle }) = &app.plan.drag
        && let (Some(s), Some(anchor)) = (app.plan.snap, handle_anchor(app.doc(), *id, *handle))
    {
        rubber_band(app, painter, view, &[anchor], s.point, accent);
    }
    if let Some(Drag::Box { start }) = &app.plan.drag
        && let Some(c) = app.plan.cursor
    {
        let r = Rect::from_two_pos(*start, view.to_screen(c));
        painter.rect(
            r,
            0.0,
            accent.gamma_multiply(0.08),
            Stroke::new(1.0, accent),
            egui::StrokeKind::Middle,
        );
    }

    // Tool previews.
    if let Some(s) = app.plan.snap {
        if app.plan.tool != Tool::Select && !app.plan.pending.is_empty() {
            rubber_band(
                app,
                painter,
                view,
                &app.plan.pending.clone(),
                s.point,
                accent,
            );
        }
        if app.plan.tool != Tool::Select {
            let sp = view.to_screen(s.point);
            match s.kind {
                SnapKind::Free => {}
                SnapKind::Endpoint | SnapKind::Midpoint | SnapKind::Center => {
                    painter.rect_stroke(
                        Rect::from_center_size(sp, vec2(10.0, 10.0)),
                        0.0,
                        Stroke::new(1.5, accent),
                        egui::StrokeKind::Middle,
                    );
                }
                _ => {
                    painter.circle_stroke(sp, 5.0, Stroke::new(1.5, accent));
                }
            }
            let c = if dark {
                Color32::from_gray(160)
            } else {
                Color32::from_gray(90)
            };
            painter.line_segment(
                [sp - vec2(12.0, 0.0), sp + vec2(12.0, 0.0)],
                Stroke::new(1.0, c),
            );
            painter.line_segment(
                [sp - vec2(0.0, 12.0), sp + vec2(0.0, 12.0)],
                Stroke::new(1.0, c),
            );
        }
    }
}

fn paint_grid(app: &App, painter: &Painter, view: &View, dark: bool) {
    let mut step = app.prefs.snap.grid_mm.max(1.0);
    while view.len_px(step) < 10.0 {
        step *= 5.0;
    }
    let lo = view.to_model(view.rect.left_bottom());
    let hi = view.to_model(view.rect.right_top());
    // Graph paper: light lavender lines with a stronger line every fourth.
    let (minor, major) = if dark {
        (Color32::from_gray(40), Color32::from_gray(58))
    } else {
        (Color32::from_rgb(232, 232, 248), Color32::from_rgb(206, 206, 238))
    };
    let pick = |i: f64| {
        if (i.round() as i64).rem_euclid(4) == 0 {
            Stroke::new(1.0, major)
        } else {
            Stroke::new(1.0, minor)
        }
    };
    let mut i = (lo.x / step).floor();
    while i * step <= hi.x {
        let s = view.to_screen(DVec2::new(i * step, 0.0)).x;
        painter.line_segment([pos2(s, view.rect.top()), pos2(s, view.rect.bottom())], pick(i));
        i += 1.0;
    }
    let mut j = (lo.y / step).floor();
    while j * step <= hi.y {
        let s = view.to_screen(DVec2::new(0.0, j * step)).y;
        painter.line_segment([pos2(view.rect.left(), s), pos2(view.rect.right(), s)], pick(j));
        j += 1.0;
    }
}

const AXIS_X: Color32 = Color32::from_rgb(220, 40, 40);
const AXIS_Y: Color32 = Color32::from_rgb(40, 90, 230);

/// The plan origin (red X and blue Y arms at 0,0) and an axis indicator in
/// the lower-left corner.
fn paint_origin(painter: &Painter, view: &View) {
    let o = view.to_screen(DVec2::ZERO);
    if view.rect.expand(40.0).contains(o) {
        painter.line_segment([o, o + egui::vec2(28.0, 0.0)], Stroke::new(1.5, AXIS_X));
        painter.line_segment([o, o - egui::vec2(0.0, 28.0)], Stroke::new(1.5, AXIS_Y));
        painter.circle_stroke(o, 3.0, Stroke::new(1.0, Color32::from_gray(120)));
    }
    let c = view.rect.left_bottom() + egui::vec2(18.0, -18.0);
    let font = egui::FontId::proportional(11.0);
    painter.arrow(c, egui::vec2(34.0, 0.0), Stroke::new(1.5, AXIS_X));
    painter.arrow(c, egui::vec2(0.0, -34.0), Stroke::new(1.5, AXIS_Y));
    painter.text(c + egui::vec2(40.0, 0.0), egui::Align2::LEFT_CENTER, "X", font.clone(), AXIS_X);
    painter.text(c + egui::vec2(0.0, -40.0), egui::Align2::CENTER_BOTTOM, "Y", font, AXIS_Y);
}

pub(crate) fn paint_fill(
    painter: &Painter,
    view: &View,
    points: &[DVec2],
    tris: &[[usize; 3]],
    color: Color32,
) {
    let mut mesh = egui::Mesh::default();
    for p in points {
        mesh.colored_vertex(view.to_screen(*p), color);
    }
    for t in tris {
        mesh.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
    }
    painter.add(EShape::mesh(mesh));
}

fn shift_prim(p: &Prim, d: DVec2) -> Prim {
    let mut p = p.clone();
    match &mut p {
        Prim::Polyline { points, .. } | Prim::Polygon { points, .. } => {
            points.iter_mut().for_each(|q| *q += d)
        }
        Prim::Text { at, .. } => *at += d,
        Prim::Image { corners, .. } => corners.iter_mut().for_each(|p| *p += d),
    }
    p
}

/// Draw a primitive; `highlight` replaces its colour and widens strokes.
pub(crate) fn paint_prim(
    ui: &Ui,
    painter: &Painter,
    view: &View,
    prim: &Prim,
    highlight: Option<Color32>,
    extra: f32,
) {
    match prim {
        Prim::Image {
            corners,
            path,
            opacity,
            transparent,
        } => {
            let screen = corners.map(|p| view.to_screen(p));
            match highlight {
                Some(h) => {
                    let mut pts = screen.to_vec();
                    pts.push(screen[0]);
                    painter.add(EShape::line(pts, Stroke::new(1.0 + extra, h)));
                }
                None => crate::images::paint(ui.ctx(), painter, screen, path, *opacity, *transparent),
            }
        }
        Prim::Polyline {
            points,
            closed,
            stroke,
        } => {
            let mut pts: Vec<Pos2> = points.iter().map(|p| view.to_screen(*p)).collect();
            if *closed && pts.len() > 2 {
                pts.push(pts[0]);
            }
            let color = highlight.unwrap_or_else(|| ink_color(ui, stroke.ink));
            let s = Stroke::new(pen_px(stroke.width) + extra, color);
            if stroke.dash.is_empty() || highlight.is_some() {
                painter.add(EShape::line(pts, s));
            } else {
                let on = (stroke.dash[0] * 3.0).max(2.0) as f32;
                let off = (stroke.dash.get(1).copied().unwrap_or(1.0) * 3.0).max(2.0) as f32;
                painter.extend(EShape::dashed_line(&pts, s, on, off));
            }
        }
        Prim::Polygon { points, .. } => {
            if let Some(h) = highlight {
                let mut pts: Vec<Pos2> = points.iter().map(|p| view.to_screen(*p)).collect();
                pts.push(pts[0]);
                painter.add(EShape::line(pts, Stroke::new(1.0 + extra, h)));
            }
        }
        Prim::Text {
            at,
            text,
            height,
            rotation,
            justify,
            middle,
            bold,
            ink,
        } => {
            let px = view.len_px(height / 0.72);
            if px < 3.0 {
                return;
            }
            let color = highlight.unwrap_or_else(|| ink_color(ui, *ink));
            let mut font = egui::FontId::proportional(px);
            if *bold {
                font.family = egui::FontFamily::Proportional;
            }
            let galley = painter.layout_no_wrap(text.clone(), font, color);
            let size = galley.size();
            let fx = match justify {
                Justify::Left => 0.0,
                Justify::Center => 0.5,
                Justify::Right => 1.0,
            };
            // Anchor: baseline (or middle) at `at`.
            let ascent = if *middle { size.y * 0.5 } else { size.y * 0.78 };
            let angle = -(rotation.to_radians() as f32);
            let offset = egui::emath::Rot2::from_angle(angle) * vec2(-size.x * fx, -ascent);
            let pos = view.to_screen(*at) + offset;
            painter.add(EShape::Text(
                egui::epaint::TextShape::new(pos, galley, color).with_angle(angle),
            ));
            if *bold && highlight.is_none() {
                // Fake bold: a second pass offset by a fraction of a point.
                let galley =
                    painter.layout_no_wrap(text.clone(), egui::FontId::proportional(px), color);
                painter.add(EShape::Text(
                    egui::epaint::TextShape::new(
                        pos + egui::emath::Rot2::from_angle(angle) * vec2(0.6, 0.0),
                        galley,
                        color,
                    )
                    .with_angle(angle),
                ));
            }
        }
    }
}

/// Live preview from the pending points to the cursor, with a length label.
fn rubber_band(
    app: &App,
    painter: &Painter,
    view: &View,
    pending: &[DVec2],
    cursor: DVec2,
    accent: Color32,
) {
    let doc = app.doc();
    let stroke = Stroke::new(1.5, accent);
    let last = *pending.last().expect("non-empty");
    match app.plan.tool {
        Tool::ExteriorWall | Tool::InteriorWall => {
            let d = doc.active_defaults();
            let ty = if app.plan.tool == Tool::InteriorWall {
                d.interior_wall_type
            } else {
                d.exterior_wall_type
            };
            let half = doc
                .get::<WallType>(ty)
                .map_or(70.0, |t| t.thickness().mm() / 2.0);
            let o = wall_offset(doc, app.plan.tool == Tool::InteriorWall);
            let dir = (cursor - last).normalize_or_zero();
            let left = DVec2::new(-dir.y, dir.x);
            let shift = left * o;
            let n = left * half;
            let quad = [last + shift + n, cursor + shift + n, cursor + shift - n, last + shift - n];
            let mut pts: Vec<Pos2> = quad.iter().map(|p| view.to_screen(*p)).collect();
            pts.push(pts[0]);
            painter.add(EShape::line(pts, stroke));
        }
        Tool::Circle | Tool::Hill | Tool::Valley => {
            painter.circle_stroke(
                view.to_screen(last),
                view.len_px(last.distance(cursor)),
                stroke,
            );
        }
        Tool::Box | Tool::Ellipse => {
            painter.rect_stroke(
                Rect::from_two_pos(view.to_screen(last), view.to_screen(cursor)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        _ => {
            let mut pts: Vec<Pos2> = pending.iter().map(|p| view.to_screen(*p)).collect();
            pts.push(view.to_screen(cursor));
            painter.add(EShape::line(pts, stroke));
        }
    }
    // Length label (or the typed length).
    let len = Length::from_mm(last.distance(cursor));
    let label = match &app.plan.length_input {
        Some(buf) => format!("Length: {buf}▏"),
        None => crate::util::fmt(doc, len),
    };
    let mid = view.to_screen((last + cursor) / 2.0) + vec2(0.0, -14.0);
    let galley = painter.layout_no_wrap(label, egui::FontId::proportional(13.0), Color32::WHITE);
    let r = Rect::from_center_size(mid, galley.size() + vec2(8.0, 4.0));
    painter.rect_filled(r, 3.0, accent);
    painter.galley(r.min + vec2(4.0, 2.0), galley, Color32::WHITE);
}

/// Screen rect of the clickable temporary dimension label for `id`.
fn temp_dim_label(app: &App, view: &View, id: Id) -> Option<(Rect, String, Pos2, Pos2)> {
    let doc = app.doc();
    let (a, b) = match doc.object(id)? {
        Object::Wall(w) => (v2(w.start), v2(w.end)),
        Object::Cad(Cad {
            shape: Shape::Line { a, b, .. },
            ..
        }) => (v2(*a), v2(*b)),
        _ => return None,
    };
    let dir = (b - a).normalize_or(DVec2::X);
    let n = DVec2::new(-dir.y, dir.x) * (28.0 * view.mpp);
    let (sa, sb) = (view.to_screen(a + n), view.to_screen(b + n));
    let text = crate::util::fmt(doc, Length::from_mm(a.distance(b)));
    let mid = pos2((sa.x + sb.x) / 2.0, (sa.y + sb.y) / 2.0);
    Some((
        Rect::from_center_size(mid, vec2(text.len() as f32 * 7.5 + 10.0, 18.0)),
        text,
        sa,
        sb,
    ))
}

fn temp_dimension(app: &App, painter: &Painter, view: &View, id: Id, accent: Color32) {
    let Some((r, text, sa, sb)) = temp_dim_label(app, view, id) else {
        return;
    };
    let s = Stroke::new(1.0, accent);
    painter.line_segment([sa, sb], s);
    painter.rect(
        r,
        3.0,
        Color32::from_black_alpha(10),
        s,
        egui::StrokeKind::Middle,
    );
    painter.text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(13.0),
        accent,
    );
}

/// Text prompt, typed-length box and the editable temporary dimension.
fn overlays(app: &mut App, ui: &mut Ui, view: &View, commands: &mut Vec<Command>) {
    let ctx = ui.ctx().clone();
    // Click the temporary dimension of a selected wall or line to type a length.
    if app.selection.len() == 1
        && app.plan.tool == Tool::Select
        && app.plan.temp_dim_edit.is_none()
        && let Some((r, text, ..)) = temp_dim_label(app, view, app.selection[0])
        && ctx.input(|i| i.pointer.primary_clicked())
        && ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| r.contains(p))
    {
        app.plan.temp_dim_edit = Some((app.selection[0], text, true));
    }
    if let Some((id, mut buf, first)) = app.plan.temp_dim_edit.take()
        && let Some((r, ..)) = temp_dim_label(app, view, id)
    {
        let mut keep = true;
        egui::Area::new(egui::Id::unique("temp_dim_edit"))
            .fixed_pos(r.left_top())
            .order(egui::Order::Foreground)
            .show(&ctx, |ui| {
                let edit = ui.add(egui::TextEdit::singleline(&mut buf).desired_width(110.0));
                if first {
                    edit.request_focus();
                }
                if edit.lost_focus() {
                    keep = false;
                    if ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && let Some(len) = crate::util::parse(app.doc(), &buf)
                        && len.mm() > 0.0
                    {
                        let doc = app.doc();
                        match doc.object(id) {
                            Some(Object::Wall(_)) => commands.push(Command::SetWallLength {
                                id,
                                length: len,
                                anchor: LengthAnchor::Start,
                            }),
                            Some(Object::Cad(Cad {
                                shape:
                                    Shape::Line {
                                        a,
                                        b,
                                        arrow_start,
                                        arrow_end,
                                    },
                                ..
                            })) => {
                                let (pa, pb) = (v2(*a), v2(*b));
                                let nb = pa + (pb - pa).normalize_or(DVec2::X) * len.mm();
                                let shape = Shape::Line {
                                    a: *a,
                                    b: pt(nb),
                                    arrow_start: *arrow_start,
                                    arrow_end: *arrow_end,
                                };
                                commands.push(Command::Update {
                                    id,
                                    set: serde_json::json!({ "shape": shape })
                                        .as_object()
                                        .cloned()
                                        .unwrap_or_default(),
                                });
                            }
                            _ => {}
                        }
                    }
                }
            });
        if keep {
            app.plan.temp_dim_edit = Some((id, buf, false));
        }
    }

    if let Some((p, mut buf)) = app.plan.text_prompt.take() {
        let mut keep = true;
        egui::Window::new("Text")
            .collapsible(false)
            .resizable(false)
            .fixed_pos(view.to_screen(p) + vec2(10.0, 10.0))
            .show(&ctx, |ui| {
                let edit = ui.add(
                    egui::TextEdit::multiline(&mut buf)
                        .desired_rows(2)
                        .desired_width(220.0),
                );
                edit.request_focus();
                ui.horizontal(|ui| {
                    let ok = ui.button("OK").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);
                    if ok && !buf.trim().is_empty() {
                        commands.push(Command::AddText {
                            floor: Some(app.plan.target()),
                            layer: None,
                            position: pt(p),
                            text: buf.trim_end().to_string(),
                            style: None,
                            rotation: 0.0,
                            justify: Justify::Left,
                            height: None,
                            leaders: app.plan.prompt_leaders.drain(..).map(pt).collect(),
                        });
                        keep = false;
                    }
                    if ui.button("Cancel").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        keep = false;
                        app.plan.prompt_leaders.clear();
                    }
                    ui.weak("Ctrl+Enter to place");
                });
            });
        if keep {
            app.plan.text_prompt = Some((p, buf));
        }
    }
}
