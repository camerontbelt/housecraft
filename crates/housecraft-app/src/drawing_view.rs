//! The 2D drawing view: elevations, cross sections and layout pages, with
//! pan, zoom and fit, painted in the drawing's own order (far faces first).
//! On a layout page, dragging a box moves it and double-clicking opens it.

use egui::{Color32, Sense, Ui};
use housecraft_api::Command;
use housecraft_api::geom::DVec2;
use housecraft_api::geom::triangulate::triangulate;
use housecraft_api::model::{Id, Layout, LayoutBox, SectionView};
use housecraft_api::units::Length;
use housecraft_draft::{Drawing, Prim};

use crate::app::App;
use crate::canvas::{View, fill_color, paint_fill, paint_prim};

/// What the drawing view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawingSource {
    View(Id),
    /// A layout and page index.
    Layout(Id, usize),
}

#[derive(Default)]
pub struct DrawingViewState {
    pub center: DVec2,
    /// Millimetres per screen point.
    pub mpp: f64,
    pub fit_requested: bool,
    cache: Option<Cached>,
    /// A layout box being dragged, and how far so far (paper mm).
    dragging: Option<(Id, DVec2)>,
}

struct Cached {
    rev: u64,
    source: DrawingSource,
    drawing: Drawing,
    tris: Vec<Vec<[usize; 3]>>,
}

fn build(app: &App, source: DrawingSource) -> Option<(Drawing, String)> {
    let doc = app.session.document();
    match source {
        DrawingSource::View(id) => {
            let v = doc.get::<SectionView>(id)?;
            Some((
                housecraft_draft::elevation::section_view(doc, v),
                v.name.clone(),
            ))
        }
        DrawingSource::Layout(id, page) => {
            let l = doc.get::<Layout>(id)?;
            let mut d = housecraft_draft::layout::layout_page(doc, l, page);
            // The sheet itself, under everything.
            let (w, h) = (l.paper_width.mm(), l.paper_height.mm());
            d.items.insert(
                0,
                housecraft_draft::Item {
                    object: None,
                    prim: Prim::Polygon {
                        points: vec![
                            DVec2::ZERO,
                            DVec2::new(w, 0.0),
                            DVec2::new(w, h),
                            DVec2::new(0.0, h),
                        ],
                        fill: housecraft_draft::Fill::Paper,
                    },
                },
            );
            let name = l.pages.get(page).cloned().unwrap_or_default();
            Some((d, format!("{} · {}", l.name, name)))
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui, source: DrawingSource) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = resp.rect;
    let stale = app
        .drawing_view
        .cache
        .as_ref()
        .is_none_or(|c| c.rev != app.rev || c.source != source);
    let mut title = String::new();
    if stale {
        let Some((drawing, t)) = build(app, source) else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "This view no longer exists.",
                egui::FontId::proportional(16.0),
                ui.visuals().text_color(),
            );
            return;
        };
        title = t;
        let tris = drawing
            .items
            .iter()
            .map(|i| match &i.prim {
                Prim::Polygon { points, .. } => triangulate(points),
                _ => Vec::new(),
            })
            .collect();
        let first = app
            .drawing_view
            .cache
            .as_ref()
            .is_none_or(|c| c.source != source);
        app.drawing_view.cache = Some(Cached {
            rev: app.rev,
            source,
            drawing,
            tris,
        });
        if first {
            app.drawing_view.fit_requested = true;
        }
    } else if let Some((_, t)) = build_title(app, source) {
        title = t;
    }
    let doc = app.session.document();
    let state = &mut app.drawing_view;
    let cache = state.cache.as_ref().expect("cached");
    if std::mem::take(&mut state.fit_requested) || state.mpp <= 0.0 {
        if let Some((lo, hi)) = cache.drawing.bounds() {
            state.center = (lo + hi) / 2.0;
            let size = (hi - lo).max(DVec2::splat(1.0));
            state.mpp = (size.x / rect.width() as f64).max(size.y / rect.height() as f64) * 1.08;
        } else {
            state.mpp = 20.0;
        }
    }
    let to_view = |state: &DrawingViewState| View {
        rect,
        center: state.center,
        mpp: state.mpp,
    };
    // Zoom about the cursor.
    if let Some(hover) = resp.hover_pos() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta().y, i.zoom_delta()));
        let factor = (-scroll as f64 * 0.0015).exp() / zoom as f64;
        if (factor - 1.0).abs() > 1e-6 {
            let before = to_view(state).to_model(hover);
            state.mpp = (state.mpp * factor).clamp(0.002, 5000.0);
            let after = to_view(state).to_model(hover);
            state.center += before - after;
        }
    }

    // The layout box under a point (the last drawn wins).
    let box_at = |p: DVec2| -> Option<Id> {
        let DrawingSource::Layout(layout, page) = source else {
            return None;
        };
        doc.iter::<LayoutBox>()
            .filter(|b| b.layout == layout && b.page as usize == page)
            .filter(|b| {
                let lo = DVec2::new(b.x.mm(), b.y.mm());
                let hi = lo + DVec2::new(b.width.mm(), b.height.mm());
                p.cmpge(lo).all() && p.cmple(hi).all()
            })
            .map(|b| b.id)
            .last()
    };
    let mut commands = Vec::new();
    let mut select: Option<Vec<Id>> = None;
    let mut open = None;
    if resp.drag_started_by(egui::PointerButton::Primary)
        && let Some(p) = resp.interact_pointer_pos()
        && let Some(id) = box_at(to_view(state).to_model(p))
    {
        state.dragging = Some((id, DVec2::ZERO));
        select = Some(vec![id]);
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        let delta = DVec2::new(d.x as f64, -d.y as f64) * state.mpp;
        match &mut state.dragging {
            Some((_, moved)) => *moved += delta,
            None => state.center -= delta,
        }
    }
    if resp.drag_stopped()
        && let Some((id, moved)) = state.dragging.take()
        && let Some(b) = doc.get::<LayoutBox>(id)
        && moved.length() > 0.1
    {
        commands.push(Command::Update {
            id,
            set: serde_json::json!({
                "x": Length::from_mm(b.x.mm() + moved.x),
                "y": Length::from_mm(b.y.mm() + moved.y),
            })
            .as_object()
            .expect("object")
            .clone(),
        });
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        select = Some(box_at(to_view(state).to_model(p)).into_iter().collect());
    }
    if resp.double_clicked() {
        match resp
            .interact_pointer_pos()
            .and_then(|p| box_at(to_view(state).to_model(p)))
        {
            Some(id) => open = Some(id),
            None => state.fit_requested = true,
        }
    }

    let v = to_view(state);
    let background = if matches!(source, DrawingSource::Layout(..)) {
        Color32::from_gray(110)
    } else {
        Color32::WHITE
    };
    painter.rect_filled(rect, 0.0, background);
    // Paint with light-theme inks: drawings are on paper.
    let mut paper = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    paper.visuals_mut().dark_mode = false;
    let shift = state.dragging;
    for (item, tri) in cache.drawing.items.iter().zip(&cache.tris) {
        let moving = shift
            .filter(|(id, _)| item.object == Some(*id))
            .map(|(_, d)| d);
        match (&item.prim, moving) {
            (Prim::Polygon { points, fill }, None) => {
                paint_fill(&painter, &v, points, tri, fill_color(&paper, *fill));
            }
            (prim, None) => paint_prim(&paper, &painter, &v, prim, None, 0.0),
            (prim, Some(d)) => {
                let mut prim = prim.clone();
                match &mut prim {
                    Prim::Polyline { points, .. } | Prim::Polygon { points, .. } => {
                        points.iter_mut().for_each(|p| *p += d)
                    }
                    Prim::Text { at, .. } => *at += d,
                }
                if let Prim::Polygon { points, fill } = &prim {
                    paint_fill(&painter, &v, points, tri, fill_color(&paper, *fill));
                } else {
                    paint_prim(&paper, &painter, &v, &prim, None, 0.0);
                }
            }
        }
    }
    // Outline selected layout boxes.
    let selection = select.clone().unwrap_or_else(|| app.selection.clone());
    for id in &selection {
        if let Some(b) = doc.get::<LayoutBox>(*id) {
            let d = shift
                .filter(|(s, _)| s == id)
                .map_or(DVec2::ZERO, |(_, d)| d);
            let lo = DVec2::new(b.x.mm(), b.y.mm()) + d;
            let hi = lo + DVec2::new(b.width.mm(), b.height.mm());
            let r = egui::Rect::from_two_pos(v.to_screen(lo), v.to_screen(hi));
            painter.rect_stroke(
                r,
                0.0,
                egui::Stroke::new(1.5, Color32::from_rgb(255, 140, 26)),
                egui::StrokeKind::Outside,
            );
        }
    }
    let hint = match source {
        DrawingSource::View(_) => "drag to pan, scroll to zoom, double-click to fit, F2 for plan",
        DrawingSource::Layout(..) => {
            "drag a box to move it, double-click to edit it; drag elsewhere to pan, F2 for plan"
        }
    };
    painter.text(
        rect.left_top() + egui::vec2(10.0, 8.0),
        egui::Align2::LEFT_TOP,
        format!("{title}  ·  {hint}"),
        egui::FontId::proportional(13.0),
        Color32::from_gray(40),
    );
    if let Some(s) = select {
        app.selection = s;
    }
    if let Some(id) = open
        && !app.spec.contains(&id)
    {
        app.spec.push(id);
    }
    for c in commands {
        app.run1(c);
    }
}

/// Just the title, without rebuilding the drawing.
fn build_title(app: &App, source: DrawingSource) -> Option<((), String)> {
    let doc = app.session.document();
    match source {
        DrawingSource::View(id) => doc.get::<SectionView>(id).map(|v| ((), v.name.clone())),
        DrawingSource::Layout(id, page) => doc.get::<Layout>(id).map(|l| {
            let name = l.pages.get(page).cloned().unwrap_or_default();
            ((), format!("{} · {}", l.name, name))
        }),
    }
}
