//! Tiled views: a second pane beside the main one (Window › Tile), showing
//! the 3D overview, another floor's plan, or a saved elevation or section.
//! The 2D tiles are live drawings you can pan and zoom but not edit.

use egui::{Color32, Sense, Ui};
use housecraft_api::geom::DVec2;
use housecraft_api::geom::triangulate::triangulate;
use housecraft_api::model::{Floor, Id, SectionView};
use housecraft_draft::{PlanOptions, plan};
use housecraft_draft::{Drawing, Prim};

use crate::app::App;
use crate::canvas::{View, fill_color, paint_fill, paint_prim};

/// What the tiled pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    ThreeD,
    /// A floor's plan, as the main plan view's layer set shows it.
    Floor(Id),
    /// A saved elevation or cross section.
    Section(Id),
}

/// The drawing built for a tile at a document revision, with its fills
/// triangulated.
type Cached = (u64, Tile, Drawing, Vec<Vec<[usize; 3]>>);

#[derive(Default)]
pub struct TileState {
    center: DVec2,
    /// Millimetres per screen point.
    mpp: f64,
    cache: Option<Cached>,
}

fn build(app: &App, tile: Tile) -> Option<(Drawing, String)> {
    let doc = app.session.document();
    match tile {
        Tile::ThreeD => None,
        Tile::Floor(floor) => {
            let f = doc.get::<Floor>(floor)?;
            let mut opts = PlanOptions::first(doc)?;
            opts.floor = floor;
            opts.layer_set = app.plan.layer_set;
            Some((plan(doc, &opts), f.name.clone()))
        }
        Tile::Section(id) => {
            let v = doc.get::<SectionView>(id)?;
            Some((housecraft_draft::elevation::section_view(doc, v), v.name.clone()))
        }
    }
}

/// The tile's name for menus.
pub fn name(app: &App, tile: Tile) -> String {
    let doc = app.session.document();
    match tile {
        Tile::ThreeD => "3D".into(),
        Tile::Floor(f) => doc.get::<Floor>(f).map_or("?".into(), |f| f.name.clone()),
        Tile::Section(v) => doc.get::<SectionView>(v).map_or("?".into(), |v| v.name.clone()),
    }
}

/// Paint `tile` in `ui` (the right half of the main area).
pub fn show(app: &mut App, ui: &mut Ui, tile: Tile) {
    if tile == Tile::ThreeD {
        crate::view3d::show(app, ui);
        return;
    }
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = resp.rect;
    let stale = app.tile_state.cache.as_ref().is_none_or(|c| c.0 != app.rev || c.1 != tile);
    if stale {
        let Some((drawing, _)) = build(app, tile) else {
            app.tile = None;
            return;
        };
        let tris = drawing
            .items
            .iter()
            .map(|i| match &i.prim {
                Prim::Polygon { points, .. } => triangulate(points),
                _ => Vec::new(),
            })
            .collect();
        let refit = app.tile_state.cache.as_ref().is_none_or(|c| c.1 != tile);
        if refit && let Some((lo, hi)) = drawing.bounds() {
            app.tile_state.center = (lo + hi) / 2.0;
            let size = (hi - lo).max(DVec2::splat(1.0));
            app.tile_state.mpp = (size.x / rect.width() as f64).max(size.y / rect.height() as f64) * 1.08;
        }
        app.tile_state.cache = Some((app.rev, tile, drawing, tris));
    }
    let state = &mut app.tile_state;
    if state.mpp <= 0.0 {
        state.mpp = 20.0;
    }
    // Pan by dragging, zoom about the cursor.
    if resp.dragged() {
        let d = resp.drag_delta();
        state.center -= DVec2::new(d.x as f64, -d.y as f64) * state.mpp;
    }
    if let Some(hover) = resp.hover_pos() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta().y, i.zoom_delta()));
        let factor = (-scroll as f64 * 0.0015).exp() / zoom as f64;
        if (factor - 1.0).abs() > 1e-6 {
            let view = View { rect, center: state.center, mpp: state.mpp };
            let before = view.to_model(hover);
            state.mpp = (state.mpp * factor).clamp(0.002, 5000.0);
            let after = View { rect, center: state.center, mpp: state.mpp }.to_model(hover);
            state.center += before - after;
        }
    }
    let view = View { rect, center: state.center, mpp: state.mpp };
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let mut paper = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    paper.visuals_mut().dark_mode = false;
    if let Some((_, _, drawing, tris)) = &state.cache {
        for (item, tri) in drawing.items.iter().zip(tris) {
            match &item.prim {
                Prim::Polygon { points, fill } => paint_fill(&painter, &view, points, tri, fill_color(&paper, *fill)),
                prim => paint_prim(&paper, &painter, &view, prim, None, 0.0),
            }
        }
    }
    let title = build_title(app, tile);
    painter.text(rect.left_top() + egui::vec2(8.0, 6.0), egui::Align2::LEFT_TOP, title, egui::FontId::proportional(12.0), Color32::from_gray(60));
}

fn build_title(app: &App, tile: Tile) -> String {
    format!("{} · tiled view (Window › Untile)", name(app, tile))
}
