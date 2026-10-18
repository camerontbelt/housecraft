//! A 2D drawing view for elevations and cross sections: pan, zoom, fit, and
//! painting in the drawing's own order (far faces first).

use egui::{Color32, Sense, Ui};
use housecraft_api::geom::DVec2;
use housecraft_api::geom::triangulate::triangulate;
use housecraft_api::model::{Id, SectionView};
use housecraft_draft::{Drawing, Prim};

use crate::app::App;
use crate::canvas::{View, fill_color, paint_fill, paint_prim};

#[derive(Default)]
pub struct DrawingViewState {
    pub center: DVec2,
    /// Millimetres per screen point.
    pub mpp: f64,
    pub fit_requested: bool,
    cache: Option<Cached>,
}

/// (document revision, view id, drawing, triangulated polygon fills).
type Cached = (u64, Id, Drawing, Vec<Vec<[usize; 3]>>);

fn bounds(d: &Drawing) -> Option<(DVec2, DVec2)> {
    let mut it = d.items.iter().flat_map(|i| match &i.prim {
        Prim::Polyline { points, .. } | Prim::Polygon { points, .. } => points.clone(),
        Prim::Text { at, .. } => vec![*at],
    });
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}

/// Show the elevation or section `id`.
pub fn show(app: &mut App, ui: &mut Ui, id: Id) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = resp.rect;
    let doc = app.session.document();
    let Some(view) = doc.get::<SectionView>(id) else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "This view no longer exists.",
            egui::FontId::proportional(16.0),
            ui.visuals().text_color(),
        );
        return;
    };
    let state = &mut app.drawing_view;
    let stale = state
        .cache
        .as_ref()
        .is_none_or(|(rev, vid, ..)| *rev != app.rev || *vid != id);
    if stale {
        let drawing = housecraft_draft::elevation::section_view(doc, view);
        let tris = drawing
            .items
            .iter()
            .map(|i| match &i.prim {
                Prim::Polygon { points, .. } => triangulate(points),
                _ => Vec::new(),
            })
            .collect();
        let first = state.cache.as_ref().is_none_or(|(_, vid, ..)| *vid != id);
        state.cache = Some((app.rev, id, drawing, tris));
        if first {
            state.fit_requested = true;
        }
    }
    let (_, _, drawing, tris) = state.cache.as_ref().expect("cached");
    if std::mem::take(&mut state.fit_requested) || state.mpp <= 0.0 {
        if let Some((lo, hi)) = bounds(drawing) {
            state.center = (lo + hi) / 2.0;
            let size = (hi - lo).max(DVec2::splat(1.0));
            state.mpp = (size.x / rect.width() as f64).max(size.y / rect.height() as f64) * 1.1;
        } else {
            state.mpp = 20.0;
        }
    }
    // Zoom about the cursor; drag (any button) pans.
    if let Some(hover) = resp.hover_pos() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta().y, i.zoom_delta()));
        let factor = (-scroll as f64 * 0.0015).exp() / zoom as f64;
        if (factor - 1.0).abs() > 1e-6 {
            let v = View {
                rect,
                center: state.center,
                mpp: state.mpp,
            };
            let before = v.to_model(hover);
            state.mpp = (state.mpp * factor).clamp(0.05, 5000.0);
            let after = View {
                mpp: state.mpp,
                ..v
            }
            .to_model(hover);
            state.center += before - after;
        }
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        state.center -= DVec2::new(d.x as f64, -d.y as f64) * state.mpp;
    }
    if resp.double_clicked() {
        state.fit_requested = true;
    }

    let v = View {
        rect,
        center: state.center,
        mpp: state.mpp,
    };
    // Drawings are on paper: white in either theme, like Chief's views.
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    // Paint with light-theme inks on the white paper.
    let mut paper = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    paper.visuals_mut().dark_mode = false;
    for (item, tri) in drawing.items.iter().zip(tris) {
        if let Prim::Polygon { points, fill } = &item.prim {
            paint_fill(&painter, &v, points, tri, fill_color(&paper, *fill));
        } else {
            paint_prim(&paper, &painter, &v, &item.prim, None, 0.0);
        }
    }
    painter.text(
        rect.left_top() + egui::vec2(10.0, 8.0),
        egui::Align2::LEFT_TOP,
        format!(
            "{}  ·  drag to pan, scroll to zoom, double-click to fit, F2 for plan",
            view.name
        ),
        egui::FontId::proportional(13.0),
        Color32::from_gray(90),
    );
}
