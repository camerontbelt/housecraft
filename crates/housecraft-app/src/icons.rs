//! Toolbar icons, drawn as vectors so they stay sharp at any DPI.
//!
//! These are original drawings that use the same visual metaphors as other
//! residential CAD tools (a brick wall for exterior walls, a door swing, a
//! camera...) so the toolbar feels familiar without copying anyone's artwork.

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    New,
    Open,
    Save,
    Print,
    Undo,
    Redo,
    Help,
    Defaults,
    Preferences,
    Select,
    ExteriorWall,
    InteriorWall,
    Door,
    DoubleDoor,
    SlidingDoor,
    PocketDoor,
    BifoldDoor,
    Doorway,
    Window,
    Cabinet,
    Stairs,
    Roof,
    Electrical,
    Dimension,
    AngularDimension,
    AutoDimension,
    Text,
    Line,
    Arrow,
    Polyline,
    Box,
    Circle,
    Arc,
    Ellipse,
    Spline,
    Point,
    Camera,
    Overview,
    OrthoOverview,
    Plan,
    ZoomIn,
    ZoomOut,
    FillWindow,
    ZoomSelection,
    FloorUp,
    FloorDown,
    Reference,
    OpenObject,
    Delete,
    Copy,
    Move,
    Rotate,
    Reflect,
    MultiCopy,
    Offset,
    Fillet,
    Chamfer,
    Extend,
    SelectNext,
    Polygon,
    MatchProperties,
    CenterObject,
    AlignLeft,
    LayerHider,
    Layers,
    Library,
}

const BRICK: Color32 = Color32::from_rgb(196, 64, 48);
const WOOD: Color32 = Color32::from_rgb(150, 98, 50);
const GLASS: Color32 = Color32::from_rgb(70, 150, 210);
const DIM: Color32 = Color32::from_rgb(40, 110, 220);
const GREEN: Color32 = Color32::from_rgb(60, 160, 80);
const RED: Color32 = Color32::from_rgb(210, 50, 50);
const GOLD: Color32 = Color32::from_rgb(220, 170, 40);

/// Paint `icon` into `rect` using `ink` for neutral strokes.
pub fn paint(p: &Painter, rect: Rect, icon: Icon, ink: Color32) {
    let r = rect.shrink(rect.width() * 0.1);
    let at = |x: f32, y: f32| pos2(r.left() + x * r.width(), r.top() + y * r.height());
    let w = (r.width() / 14.0).max(1.0);
    let s = Stroke::new(w, ink);
    let thin = Stroke::new(w * 0.7, ink);
    let line = |a: (f32, f32), b: (f32, f32), st: Stroke| {
        p.line_segment([at(a.0, a.1), at(b.0, b.1)], st);
    };
    let poly = |pts: &[(f32, f32)], closed: bool, st: Stroke| {
        let pts: Vec<Pos2> = pts.iter().map(|(x, y)| at(*x, *y)).collect();
        if closed {
            p.add(Shape::closed_line(pts, st));
        } else {
            p.add(Shape::line(pts, st));
        }
    };
    let fill = |pts: &[(f32, f32)], c: Color32| {
        let pts: Vec<Pos2> = pts.iter().map(|(x, y)| at(*x, *y)).collect();
        p.add(Shape::convex_polygon(pts, c, Stroke::NONE));
    };
    let rect_fill = |x0: f32, y0: f32, x1: f32, y1: f32, c: Color32| {
        p.rect_filled(Rect::from_two_pos(at(x0, y0), at(x1, y1)), 0.0, c);
    };
    let rect_line = |x0: f32, y0: f32, x1: f32, y1: f32, st: Stroke| {
        p.rect_stroke(
            Rect::from_two_pos(at(x0, y0), at(x1, y1)),
            0.0,
            st,
            egui::StrokeKind::Middle,
        );
    };
    let arc = |c: (f32, f32), rad: f32, a0: f32, a1: f32, st: Stroke| {
        let n = 16;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
                at(c.0 + rad * a.cos(), c.1 - rad * a.sin())
            })
            .collect();
        p.add(Shape::line(pts, st));
    };
    let circle = |c: (f32, f32), rad: f32, st: Stroke| {
        p.circle_stroke(at(c.0, c.1), rad * r.width(), st);
    };
    let dot = |c: (f32, f32), rad: f32, col: Color32| {
        p.circle_filled(at(c.0, c.1), rad * r.width(), col);
    };
    let text = |s: &str, size: f32, col: Color32| {
        p.text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            s,
            egui::FontId::proportional(r.height() * size),
            col,
        );
    };
    let arrow_head = |tip: (f32, f32), from: (f32, f32), col: Color32| {
        let d = vec2(tip.0 - from.0, tip.1 - from.1).normalized() * 0.22;
        let n = vec2(-d.y, d.x) * 0.6;
        fill(
            &[
                tip,
                (tip.0 - d.x + n.x, tip.1 - d.y + n.y),
                (tip.0 - d.x - n.x, tip.1 - d.y - n.y),
            ],
            col,
        );
    };
    match icon {
        Icon::New => {
            poly(
                &[
                    (0.2, 0.05),
                    (0.65, 0.05),
                    (0.85, 0.25),
                    (0.85, 0.95),
                    (0.2, 0.95),
                ],
                true,
                s,
            );
            poly(&[(0.65, 0.05), (0.65, 0.25), (0.85, 0.25)], false, thin);
        }
        Icon::Open => {
            fill(
                &[
                    (0.05, 0.3),
                    (0.4, 0.3),
                    (0.5, 0.4),
                    (0.95, 0.4),
                    (0.95, 0.9),
                    (0.05, 0.9),
                ],
                GOLD,
            );
            poly(
                &[
                    (0.05, 0.3),
                    (0.4, 0.3),
                    (0.5, 0.4),
                    (0.95, 0.4),
                    (0.95, 0.9),
                    (0.05, 0.9),
                ],
                true,
                thin,
            );
        }
        Icon::Save => {
            rect_fill(0.1, 0.1, 0.9, 0.9, Color32::from_rgb(60, 90, 170));
            rect_fill(0.25, 0.1, 0.75, 0.4, Color32::WHITE);
            rect_fill(0.25, 0.6, 0.75, 0.9, Color32::from_gray(220));
        }
        Icon::Print => {
            rect_line(0.25, 0.05, 0.75, 0.35, thin);
            rect_fill(0.05, 0.35, 0.95, 0.75, Color32::from_gray(120));
            rect_fill(0.25, 0.6, 0.75, 0.95, Color32::WHITE);
            rect_line(0.25, 0.6, 0.75, 0.95, thin);
        }
        Icon::Undo | Icon::Redo => {
            let flip = icon == Icon::Redo;
            let x = |v: f32| if flip { 1.0 - v } else { v };
            let n = 12;
            let pts: Vec<(f32, f32)> = (0..=n)
                .map(|i| {
                    let a = (180.0 - 180.0 * i as f32 / n as f32).to_radians();
                    (x(0.5 + 0.32 * a.cos()), 0.62 - 0.32 * a.sin())
                })
                .collect();
            poly(&pts, false, Stroke::new(w * 1.4, ink));
            arrow_head((x(0.18), 0.85), (x(0.18), 0.55), ink);
        }
        Icon::Help => text("?", 0.95, DIM),
        Icon::Defaults => {
            // A wrench.
            line((0.25, 0.8), (0.62, 0.42), Stroke::new(w * 2.4, ink));
            circle((0.68, 0.32), 0.17, Stroke::new(w * 1.6, ink));
        }
        Icon::Preferences => {
            rect_fill(0.1, 0.1, 0.9, 0.9, Color32::from_rgb(60, 120, 200));
            text("P", 0.7, Color32::WHITE);
        }
        Icon::Select => {
            let pts = [
                (0.25, 0.05),
                (0.25, 0.8),
                (0.42, 0.63),
                (0.56, 0.95),
                (0.68, 0.9),
                (0.54, 0.58),
                (0.78, 0.58),
            ];
            let filled: Vec<Pos2> = pts.iter().map(|(x, y)| at(*x, *y)).collect();
            p.add(Shape::closed_line(filled, s));
        }
        Icon::ExteriorWall => {
            rect_fill(0.05, 0.3, 0.95, 0.7, BRICK);
            for y in [0.43, 0.57] {
                line((0.05, y), (0.95, y), Stroke::new(w * 0.5, Color32::WHITE));
            }
            for (x, y0, y1) in [
                (0.3, 0.3, 0.43),
                (0.7, 0.3, 0.43),
                (0.5, 0.43, 0.57),
                (0.3, 0.57, 0.7),
                (0.7, 0.57, 0.7),
            ] {
                line((x, y0), (x, y1), Stroke::new(w * 0.5, Color32::WHITE));
            }
        }
        Icon::InteriorWall => {
            rect_line(0.05, 0.35, 0.95, 0.65, s);
            for i in 0..6 {
                let x = 0.1 + i as f32 * 0.15;
                line((x, 0.65), (x + 0.12, 0.35), thin);
            }
        }
        Icon::Door
        | Icon::DoubleDoor
        | Icon::SlidingDoor
        | Icon::PocketDoor
        | Icon::BifoldDoor
        | Icon::Doorway => {
            rect_fill(0.0, 0.8, 0.15, 0.9, ink);
            rect_fill(0.85, 0.8, 1.0, 0.9, ink);
            match icon {
                Icon::Door => {
                    line((0.15, 0.8), (0.15, 0.1), Stroke::new(w * 1.6, WOOD));
                    arc((0.15, 0.8), 0.7, 0.0, 90.0, thin);
                }
                Icon::DoubleDoor => {
                    line((0.15, 0.8), (0.15, 0.45), Stroke::new(w * 1.6, WOOD));
                    line((0.85, 0.8), (0.85, 0.45), Stroke::new(w * 1.6, WOOD));
                    arc((0.15, 0.8), 0.35, 0.0, 90.0, thin);
                    arc((0.85, 0.8), 0.35, 90.0, 180.0, thin);
                }
                Icon::SlidingDoor => {
                    line((0.15, 0.78), (0.6, 0.78), Stroke::new(w * 1.6, WOOD));
                    line((0.4, 0.88), (0.85, 0.88), Stroke::new(w * 1.6, WOOD));
                    arrow_head((0.9, 0.5), (0.5, 0.5), ink);
                    line((0.3, 0.5), (0.75, 0.5), thin);
                }
                Icon::PocketDoor => {
                    line((0.15, 0.85), (0.6, 0.85), Stroke::new(w * 1.6, WOOD));
                    line((0.0, 0.6), (0.4, 0.6), thin);
                    arrow_head((0.05, 0.6), (0.4, 0.6), ink);
                }
                Icon::BifoldDoor => {
                    poly(
                        &[
                            (0.15, 0.8),
                            (0.32, 0.45),
                            (0.5, 0.8),
                            (0.68, 0.45),
                            (0.85, 0.8),
                        ],
                        false,
                        Stroke::new(w * 1.4, WOOD),
                    );
                }
                _ => {
                    line((0.15, 0.6), (0.85, 0.6), Stroke::new(w * 0.5, ink));
                }
            }
        }
        Icon::Window => {
            rect_fill(0.1, 0.1, 0.9, 0.9, GLASS);
            rect_line(0.1, 0.1, 0.9, 0.9, s);
            line((0.5, 0.1), (0.5, 0.9), s);
            line((0.1, 0.5), (0.9, 0.5), s);
        }
        Icon::Cabinet => {
            rect_fill(0.1, 0.15, 0.9, 0.9, WOOD);
            line(
                (0.5, 0.15),
                (0.5, 0.9),
                Stroke::new(w * 0.6, Color32::WHITE),
            );
            dot((0.42, 0.5), 0.04, Color32::WHITE);
            dot((0.58, 0.5), 0.04, Color32::WHITE);
            rect_fill(0.05, 0.08, 0.95, 0.18, Color32::from_gray(160));
        }
        Icon::Stairs => {
            poly(
                &[
                    (0.05, 0.95),
                    (0.05, 0.75),
                    (0.3, 0.75),
                    (0.3, 0.55),
                    (0.55, 0.55),
                    (0.55, 0.35),
                    (0.8, 0.35),
                    (0.8, 0.15),
                    (0.95, 0.15),
                ],
                false,
                Stroke::new(w * 1.4, WOOD),
            );
        }
        Icon::Roof => {
            fill(&[(0.5, 0.1), (0.95, 0.6), (0.05, 0.6)], BRICK);
            rect_line(0.2, 0.6, 0.8, 0.95, s);
        }
        Icon::Electrical => {
            fill(
                &[
                    (0.55, 0.05),
                    (0.2, 0.55),
                    (0.48, 0.55),
                    (0.4, 0.95),
                    (0.8, 0.4),
                    (0.52, 0.4),
                ],
                GOLD,
            );
        }
        Icon::Dimension | Icon::AutoDimension => {
            let c = DIM;
            line((0.1, 0.25), (0.1, 0.75), Stroke::new(w, c));
            line((0.9, 0.25), (0.9, 0.75), Stroke::new(w, c));
            line((0.1, 0.5), (0.9, 0.5), Stroke::new(w, c));
            arrow_head((0.1, 0.5), (0.5, 0.5), c);
            arrow_head((0.9, 0.5), (0.5, 0.5), c);
            if icon == Icon::AutoDimension {
                text("A", 0.45, ink);
            }
        }
        Icon::AngularDimension => {
            line((0.1, 0.9), (0.9, 0.9), thin);
            line((0.1, 0.9), (0.7, 0.2), thin);
            arc((0.1, 0.9), 0.55, 0.0, 49.0, Stroke::new(w, DIM));
        }
        Icon::Text => text("T", 0.95, ink),
        Icon::Line => line((0.1, 0.9), (0.9, 0.1), s),
        Icon::Arrow => {
            line((0.1, 0.9), (0.85, 0.15), s);
            arrow_head((0.9, 0.1), (0.1, 0.9), ink);
        }
        Icon::Polyline => poly(
            &[(0.05, 0.85), (0.3, 0.2), (0.6, 0.65), (0.95, 0.15)],
            false,
            s,
        ),
        Icon::Box => rect_line(0.1, 0.2, 0.9, 0.8, s),
        Icon::Circle => circle((0.5, 0.5), 0.4, s),
        Icon::Arc => arc((0.5, 0.8), 0.42, 10.0, 170.0, s),
        Icon::Polygon => {
            let pts: Vec<(f32, f32)> = (0..6)
                .map(|i| {
                    let a = (30.0 + i as f32 * 60.0f32).to_radians();
                    (0.5 + 0.42 * a.cos(), 0.5 + 0.42 * a.sin())
                })
                .collect();
            poly(&pts, true, s);
        }
        Icon::MatchProperties => {
            // A dropper: bulb up right, tip down left, with a drop of colour.
            line((0.2, 0.8), (0.62, 0.38), Stroke::new(w * 2.2, ink));
            dot((0.72, 0.28), 0.14, ink);
            dot((0.14, 0.9), 0.07, GOLD);
        }
        Icon::CenterObject => {
            line((0.08, 0.15), (0.08, 0.85), s);
            line((0.92, 0.15), (0.92, 0.85), s);
            rect_fill(0.36, 0.3, 0.64, 0.7, GLASS);
            line((0.12, 0.5), (0.34, 0.5), thin);
            line((0.66, 0.5), (0.88, 0.5), thin);
        }
        Icon::AlignLeft => {
            line((0.12, 0.08), (0.12, 0.92), s);
            rect_fill(0.16, 0.18, 0.86, 0.4, GLASS);
            rect_fill(0.16, 0.58, 0.6, 0.8, GLASS);
        }
        Icon::LayerHider => {
            // An eye with a slash through it.
            arc((0.5, 0.9), 0.55, 40.0, 140.0, s);
            arc((0.5, 0.1), 0.55, 220.0, 320.0, s);
            dot((0.5, 0.5), 0.1, ink);
            line((0.15, 0.85), (0.85, 0.15), Stroke::new(w * 1.4, RED));
        }
        Icon::Ellipse => {
            let n = 24;
            let pts: Vec<(f32, f32)> = (0..n)
                .map(|i| {
                    let a = (i as f32 * 360.0 / n as f32).to_radians();
                    (0.5 + 0.45 * a.cos(), 0.5 + 0.28 * a.sin())
                })
                .collect();
            poly(&pts, true, s);
        }
        Icon::Spline => {
            let n = 20;
            let pts: Vec<(f32, f32)> = (0..=n)
                .map(|i| {
                    let t = i as f32 / n as f32;
                    (
                        0.05 + 0.9 * t,
                        0.5 - 0.35 * (t * std::f32::consts::TAU).sin(),
                    )
                })
                .collect();
            poly(&pts, false, s);
        }
        Icon::Point => {
            line((0.3, 0.3), (0.7, 0.7), s);
            line((0.3, 0.7), (0.7, 0.3), s);
        }
        Icon::Camera => {
            rect_fill(0.05, 0.3, 0.95, 0.85, Color32::from_rgb(40, 120, 190));
            rect_fill(0.3, 0.18, 0.55, 0.3, Color32::from_rgb(40, 120, 190));
            circle((0.5, 0.57), 0.18, Stroke::new(w * 1.4, Color32::WHITE));
        }
        Icon::Overview | Icon::OrthoOverview => {
            let c = if icon == Icon::Overview {
                Color32::from_rgb(40, 120, 190)
            } else {
                GREEN
            };
            fill(&[(0.5, 0.08), (0.95, 0.48), (0.05, 0.48)], c);
            rect_fill(0.18, 0.48, 0.82, 0.92, c);
            rect_fill(0.42, 0.62, 0.58, 0.92, Color32::WHITE);
        }
        Icon::Plan => {
            rect_line(0.1, 0.1, 0.9, 0.9, Stroke::new(w * 1.4, ink));
            line((0.5, 0.1), (0.5, 0.6), s);
            line((0.5, 0.6), (0.9, 0.6), s);
        }
        Icon::ZoomIn | Icon::ZoomOut | Icon::ZoomSelection => {
            circle((0.42, 0.42), 0.3, s);
            line((0.63, 0.63), (0.92, 0.92), Stroke::new(w * 2.0, ink));
            line((0.28, 0.42), (0.56, 0.42), s);
            if icon == Icon::ZoomIn {
                line((0.42, 0.28), (0.42, 0.56), s);
            }
            if icon == Icon::ZoomSelection {
                rect_line(0.3, 0.3, 0.54, 0.54, thin);
            }
        }
        Icon::FillWindow => {
            rect_line(0.1, 0.1, 0.9, 0.9, thin);
            for (a, b) in [
                ((0.35, 0.35), (0.12, 0.12)),
                ((0.65, 0.35), (0.88, 0.12)),
                ((0.35, 0.65), (0.12, 0.88)),
                ((0.65, 0.65), (0.88, 0.88)),
            ] {
                line(a, b, s);
                arrow_head(b, a, ink);
            }
        }
        Icon::FloorUp | Icon::FloorDown => {
            for y in [0.3, 0.55, 0.8] {
                line((0.05, y), (0.6, y), thin);
            }
            let (tip, from) = if icon == Icon::FloorUp {
                ((0.82, 0.1), (0.82, 0.9))
            } else {
                ((0.82, 0.9), (0.82, 0.1))
            };
            line(from, tip, Stroke::new(w * 1.4, GREEN));
            arrow_head(tip, from, GREEN);
        }
        Icon::Reference => {
            rect_line(
                0.05,
                0.05,
                0.65,
                0.65,
                Stroke::new(w, Color32::from_gray(150)),
            );
            rect_line(0.35, 0.35, 0.95, 0.95, s);
        }
        Icon::OpenObject => {
            fill(
                &[(0.2, 0.05), (0.75, 0.2), (0.75, 0.95), (0.2, 0.8)],
                Color32::from_rgb(30, 90, 140),
            );
            line((0.2, 0.05), (0.2, 0.8), s);
        }
        Icon::Delete => {
            line((0.15, 0.15), (0.85, 0.85), Stroke::new(w * 2.0, RED));
            line((0.15, 0.85), (0.85, 0.15), Stroke::new(w * 2.0, RED));
        }
        Icon::Copy | Icon::MultiCopy => {
            rect_line(0.08, 0.08, 0.6, 0.6, thin);
            rect_line(0.4, 0.4, 0.92, 0.92, s);
            if icon == Icon::MultiCopy {
                rect_line(0.24, 0.24, 0.76, 0.76, thin);
            }
        }
        Icon::Move => {
            line((0.5, 0.08), (0.5, 0.92), s);
            line((0.08, 0.5), (0.92, 0.5), s);
            for (tip, from) in [
                ((0.5, 0.05), (0.5, 0.5)),
                ((0.5, 0.95), (0.5, 0.5)),
                ((0.05, 0.5), (0.5, 0.5)),
                ((0.95, 0.5), (0.5, 0.5)),
            ] {
                arrow_head(tip, from, ink);
            }
        }
        Icon::Rotate => {
            arc((0.5, 0.5), 0.36, 30.0, 330.0, s);
            arrow_head((0.81, 0.32), (0.81, 0.7), ink);
        }
        Icon::Reflect => {
            line((0.5, 0.05), (0.5, 0.95), Stroke::new(w * 0.6, DIM));
            fill(&[(0.1, 0.8), (0.4, 0.8), (0.4, 0.25)], ink);
            poly(&[(0.9, 0.8), (0.6, 0.8), (0.6, 0.25)], true, s);
        }
        Icon::Offset => {
            poly(&[(0.1, 0.9), (0.1, 0.4), (0.6, 0.4)], false, s);
            poly(
                &[(0.35, 0.9), (0.35, 0.65), (0.9, 0.65)],
                false,
                Stroke::new(w, DIM),
            );
        }
        Icon::Fillet => {
            line((0.1, 0.9), (0.1, 0.5), s);
            arc((0.5, 0.5), 0.4, 90.0, 180.0, Stroke::new(w, DIM));
            line((0.5, 0.1), (0.9, 0.1), s);
        }
        Icon::Chamfer => {
            line((0.1, 0.9), (0.1, 0.45), s);
            line((0.1, 0.45), (0.45, 0.1), Stroke::new(w, DIM));
            line((0.45, 0.1), (0.9, 0.1), s);
        }
        Icon::Extend => {
            line((0.85, 0.05), (0.85, 0.95), Stroke::new(w * 1.4, ink));
            line((0.1, 0.5), (0.5, 0.5), s);
            line((0.5, 0.5), (0.85, 0.5), Stroke::new(w, DIM));
            arrow_head((0.8, 0.5), (0.4, 0.5), DIM);
        }
        Icon::SelectNext => {
            arc((0.5, 0.65), 0.3, 0.0, 180.0, Stroke::new(w * 1.4, RED));
            arrow_head((0.8, 0.75), (0.8, 0.5), RED);
            p.text(
                at(0.5, 0.9),
                egui::Align2::CENTER_CENTER,
                "next",
                egui::FontId::proportional(r.height() * 0.32),
                ink,
            );
        }
        Icon::Layers => {
            for (i, c) in [GLASS, GREEN, GOLD].iter().enumerate() {
                let y = 0.25 + i as f32 * 0.22;
                fill(
                    &[(0.5, y - 0.15), (0.95, y), (0.5, y + 0.15), (0.05, y)],
                    *c,
                );
            }
        }
        Icon::Library => {
            for (i, c) in [BRICK, WOOD, GLASS, GREEN].iter().enumerate() {
                let x = 0.08 + i as f32 * 0.22;
                rect_fill(x, 0.15, x + 0.17, 0.9, *c);
            }
        }
    }
}

/// A square toolbar button with an icon. `selected` draws it pressed.
pub fn button(ui: &mut Ui, icon: Icon, selected: bool, enabled: bool, tooltip: &str) -> Response {
    let size = Vec2::splat(ui.spacing().interact_size.y * 1.35);
    let (rect, resp) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&resp, selected);
        if selected || (enabled && resp.hovered()) {
            ui.painter().rect(
                rect,
                3.0,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
        }
        paint(
            ui.painter(),
            rect.shrink(2.0),
            icon,
            ui.visuals().text_color(),
        );
        if !enabled {
            let bg = ui.visuals().panel_fill.gamma_multiply(0.6);
            ui.painter().rect_filled(rect, 3.0, bg);
        }
    }
    resp.on_hover_text(tooltip)
}
