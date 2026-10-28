//! Snapping for drawing and editing tools.

use housecraft_api::geom::math::{closest_on_segment, shape_snap_points};
use housecraft_api::geom::walls::wall_geoms;
use housecraft_api::geom::{DVec2, v2};
use housecraft_api::model::{Cad, Document, Id};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
    Center,
    OnObject,
    Angle,
    Grid,
    Free,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            SnapKind::Endpoint => "endpoint",
            SnapKind::Midpoint => "midpoint",
            SnapKind::Center => "center",
            SnapKind::OnObject => "on object",
            SnapKind::Angle => "angle",
            SnapKind::Grid => "grid",
            SnapKind::Free => "",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Snap {
    pub point: DVec2,
    pub kind: SnapKind,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapSettings {
    pub objects: bool,
    pub grid: bool,
    pub angles: bool,
    /// Angle increment in degrees.
    pub angle_step: f64,
    /// Grid spacing in mm.
    pub grid_mm: f64,
    /// Lengths along angle snaps round to this (mm).
    pub length_step_mm: f64,
}

impl Default for SnapSettings {
    fn default() -> Self {
        SnapSettings {
            objects: true,
            grid: false,
            angles: true,
            angle_step: 15.0,
            grid_mm: 304.8,
            length_step_mm: 25.4,
        }
    }
}

/// Points and segments on a floor (and its reference floor) to snap to.
#[derive(Default)]
pub struct SnapTargets {
    points: Vec<(DVec2, SnapKind)>,
    segments: Vec<(DVec2, DVec2)>,
    /// The plan's extra allowed angles (radians).
    angles: Vec<f64>,
}

impl SnapTargets {
    pub fn build(doc: &Document, floors: &[Id]) -> Self {
        let mut t = SnapTargets { angles: doc.settings.allowed_angles.iter().map(|a| a.to_radians()).collect(), ..Default::default() };
        for &floor in floors {
            for g in wall_geoms(doc, floor) {
                t.points.push((g.start, SnapKind::Endpoint));
                t.points.push((g.end, SnapKind::Endpoint));
                t.points.push(((g.start + g.end) / 2.0, SnapKind::Midpoint));
                for c in g.corners {
                    t.points.push((c, SnapKind::Endpoint));
                }
                t.segments.push((g.start, g.end));
                t.segments.push((g.corners[0], g.corners[1]));
                t.segments.push((g.corners[3], g.corners[2]));
            }
            for cad in doc.iter::<Cad>().filter(|c| c.floor == floor) {
                for (p, center) in shape_snap_points(&cad.shape) {
                    t.points.push((
                        p,
                        if center {
                            SnapKind::Center
                        } else {
                            SnapKind::Endpoint
                        },
                    ));
                }
                for (pts, closed) in housecraft_api::geom::math::shape_polylines(&cad.shape) {
                    for w in pts.windows(2) {
                        t.segments.push((w[0], w[1]));
                        if pts.len() == 2 {
                            t.points.push(((w[0] + w[1]) / 2.0, SnapKind::Midpoint));
                        }
                    }
                    if closed && pts.len() > 2 {
                        t.segments.push((pts[pts.len() - 1], pts[0]));
                    }
                }
            }
            for dim in doc
                .iter::<housecraft_api::model::Dimension>()
                .filter(|d| d.floor == floor)
            {
                if let housecraft_api::model::DimensionGeometry::Linear { points, .. } =
                    &dim.geometry
                {
                    t.points
                        .extend(points.iter().map(|p| (v2(*p), SnapKind::Endpoint)));
                }
            }
        }
        t
    }
}

impl SnapTargets {
    /// Points the dimension tool snaps to, by the default set's locate rules:
    /// wall surfaces, framing-layer faces, centrelines and opening edges.
    pub fn build_dims(doc: &Document, floors: &[Id]) -> Self {
        let locate = doc.active_defaults().dimensions.locate;
        let mut t = SnapTargets::default();
        for &floor in floors {
            for g in wall_geoms(doc, floor) {
                let wall = doc.get::<housecraft_api::model::Wall>(g.id);
                let wt = wall.and_then(|w| doc.get::<housecraft_api::model::WallType>(w.wall_type));
                if locate.centers {
                    t.points.push((g.start, SnapKind::Endpoint));
                    t.points.push((g.end, SnapKind::Endpoint));
                    t.segments.push((g.start, g.end));
                }
                if locate.surfaces {
                    for c in g.corners {
                        t.points.push((c, SnapKind::Endpoint));
                    }
                    t.segments.push((g.corners[0], g.corners[1]));
                    t.segments.push((g.corners[3], g.corners[2]));
                }
                if locate.framing
                    && let Some(wt) = wt
                    && let Some(main) = wt.layers.iter().map(|l| l.thickness.mm()).reduce(f64::max)
                {
                    // The main layer's middle, then its two faces.
                    let mid = -wt.centerline_offset(housecraft_api::model::WallReference::Main);
                    for side in [-1.0, 1.0] {
                        let o = g.left * (mid + side * main / 2.0);
                        t.points.push((g.start + o, SnapKind::Endpoint));
                        t.points.push((g.end + o, SnapKind::Endpoint));
                        t.segments.push((g.start + o, g.end + o));
                    }
                }
                if locate.openings {
                    for o in doc.openings_in(g.id) {
                        let c = g.start + g.dir * o.position.mm();
                        let half_w = o.width.mm() / 2.0;
                        for along in [-half_w, 0.0, half_w] {
                            for across in [-g.half, 0.0, g.half] {
                                let kind = if along == 0.0 { SnapKind::Midpoint } else { SnapKind::Endpoint };
                                t.points.push((c + g.dir * along + g.left * across, kind));
                            }
                        }
                    }
                }
            }
            for cad in doc.iter::<Cad>().filter(|c| c.floor == floor) {
                for (p, center) in shape_snap_points(&cad.shape) {
                    t.points.push((p, if center { SnapKind::Center } else { SnapKind::Endpoint }));
                }
            }
        }
        t
    }

    /// Snap to a drawing's linework too (an elevation's edges and corners).
    pub fn add_drawing(&mut self, drawing: &housecraft_draft::Drawing) {
        for item in &drawing.items {
            if let housecraft_draft::Prim::Polyline { points, closed, .. } = &item.prim {
                self.points
                    .extend(points.iter().map(|p| (*p, SnapKind::Endpoint)));
                for w in points.windows(2) {
                    self.segments.push((w[0], w[1]));
                }
                if *closed && points.len() > 2 {
                    self.segments.push((points[points.len() - 1], points[0]));
                }
            }
        }
    }
}

/// Snap `cursor` (model mm). `tol` is the pick radius in mm; `from` is the
/// previous point of the tool, enabling angle snapping.
pub fn snap(
    cursor: DVec2,
    tol: f64,
    targets: &SnapTargets,
    s: &SnapSettings,
    from: Option<DVec2>,
) -> Snap {
    if s.objects {
        let best = targets
            .points
            .iter()
            .map(|(p, k)| (p.distance(cursor), *p, *k))
            .filter(|(d, ..)| *d <= tol)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, point, kind)) = best {
            return Snap { point, kind };
        }
    }
    if s.angles
        && let Some(f) = from
    {
        let d = cursor - f;
        if d.length() > 1.0 {
            let step = s.angle_step.max(1.0).to_radians();
            let raw = d.y.atan2(d.x);
            let mut angle = (raw / step).round() * step;
            // An allowed angle (either way along it) wins when it's nearer.
            let off = |a: f64| {
                let x = (raw - a).rem_euclid(std::f64::consts::TAU);
                x.min(std::f64::consts::TAU - x)
            };
            for a in targets.angles.iter().flat_map(|a| [*a, a + std::f64::consts::PI]) {
                if off(a) < off(angle) {
                    angle = a;
                }
            }
            let dir = DVec2::from_angle(angle);
            let mut len = d.dot(dir);
            if s.length_step_mm > 0.0 {
                len = (len / s.length_step_mm).round() * s.length_step_mm;
            }
            let p = f + dir * len;
            // Prefer an object crossing the angle ray near the cursor.
            if s.objects {
                for (a, b) in &targets.segments {
                    if let Some((x, _, _)) = housecraft_api::geom::math::segment_intersection(
                        f,
                        f + dir * (len.abs() * 2.0 + tol),
                        *a,
                        *b,
                    ) && x.distance(p) <= tol
                        && x.distance(f) > 1.0
                    {
                        return Snap {
                            point: x,
                            kind: SnapKind::OnObject,
                        };
                    }
                }
            }
            if p.distance(cursor) <= tol * 3.0 {
                return Snap {
                    point: p,
                    kind: SnapKind::Angle,
                };
            }
        }
    }
    if s.objects {
        let best = targets
            .segments
            .iter()
            .map(|(a, b)| closest_on_segment(cursor, *a, *b).0)
            .map(|p| (p.distance(cursor), p))
            .filter(|(d, _)| *d <= tol)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, point)) = best {
            return Snap {
                point,
                kind: SnapKind::OnObject,
            };
        }
    }
    if s.grid && s.grid_mm > 0.0 {
        let g = s.grid_mm;
        return Snap {
            point: DVec2::new((cursor.x / g).round() * g, (cursor.y / g).round() * g),
            kind: SnapKind::Grid,
        };
    }
    Snap {
        point: cursor,
        kind: SnapKind::Free,
    }
}
