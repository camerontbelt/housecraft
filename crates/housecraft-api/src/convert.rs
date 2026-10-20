//! Turning 2D CAD into building objects (walls, counters, decks, site
//! features, moldings, solids) and roof planes into CAD outlines.

use housecraft_geom::math::shape_polylines;
use housecraft_geom::{DVec2, v2};
use housecraft_model::*;
use housecraft_units::{Length, Point2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ApiError, Command, Outcome};

/// What [`Command::ConvertCad`] makes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CadTarget {
    /// A wall along every line and polyline segment.
    Walls {
        #[serde(default)]
        interior: bool,
    },
    /// Pairs of parallel lines (a drawn wall's two faces) become one wall
    /// between them, of the wall type nearest their spacing; lone lines
    /// become walls of the default type.
    DoubleLineWalls,
    Countertop,
    Deck,
    /// A road, driveway, sidewalk, fence or other site feature.
    SiteFeature { feature: SiteKind },
    /// A baseboard-like molding along the shape.
    Molding,
    /// The terrain's outline (the lot): replaces the existing perimeter and
    /// keeps the terrain's settings.
    TerrainPerimeter,
    /// A solid extruded up from the shape.
    Solid {
        #[serde(default)]
        height: Option<Length>,
    },
}

fn pt(p: DVec2) -> Point2 {
    Point2::from_mm(p.x, p.y)
}

pub(crate) fn convert_cad(doc: &mut Document, ids: &[Id], to: &CadTarget, out: &mut Outcome) -> Result<(), ApiError> {
    let cads: Vec<Cad> = ids.iter().filter_map(|id| doc.get::<Cad>(*id).cloned()).collect();
    if cads.is_empty() {
        return Err(ApiError::Invalid("select CAD lines or shapes to convert".into()));
    }
    let mut run = |doc: &mut Document, c: Command| -> Result<(), ApiError> {
        let o = c.apply(doc)?;
        out.merge(o);
        Ok(())
    };
    match to {
        CadTarget::Solid { height } => {
            run(doc, Command::ExtrudeCad { ids: cads.iter().map(|c| c.id).collect(), height: *height })?;
            return Ok(());
        }
        CadTarget::DoubleLineWalls => {
            double_line_walls(doc, &cads, out)?;
        }
        _ => {
            for c in &cads {
                for (pts, closed) in shape_polylines(&c.shape) {
                    if pts.len() < 2 {
                        continue;
                    }
                    let points: Vec<Point2> = pts.iter().map(|p| pt(*p)).collect();
                    let floor = Some(c.floor);
                    let cmd = match to {
                        CadTarget::Walls { interior } => Command::AddWalls { floor, points, closed, wall_type: None, interior: *interior, height: None },
                        CadTarget::Countertop if closed => Command::AddCountertop { floor, points, height: None, thickness: None, edge: None, holes: Vec::new() },
                        CadTarget::Deck if closed => Command::AddDeck { floor, points, height: None, platform: false, curved: false },
                        CadTarget::TerrainPerimeter if closed => Command::AddTerrain { floor, points },
                        CadTarget::SiteFeature { feature } => Command::AddSiteFeature { floor, points, feature: feature.clone() },
                        CadTarget::Molding => Command::AddMoldingPolyline {
                            floor,
                            points,
                            closed,
                            elevation: Length::ZERO,
                            stack: vec![MoldingSpec {
                                profile: ProfileRef::default(),
                                height: Length::from_inches(3.5),
                                depth: Length::from_inches(0.75),
                                flip: false,
                                rotate: false,
                                center: false,
                            }],
                        },
                        _ => return Err(ApiError::Invalid("countertops, decks and terrain perimeters need closed shapes".into())),
                    };
                    run(doc, cmd)?;
                }
            }
        }
    }
    for c in &cads {
        if doc.get::<Cad>(c.id).is_some() {
            doc.remove(c.id);
            out.deleted.push(c.id);
        }
    }
    Ok(())
}

/// Segments of the shapes (polylines broken into their edges).
fn segments(cads: &[Cad]) -> Vec<(Id, DVec2, DVec2)> {
    let mut v = Vec::new();
    for c in cads {
        for (pts, closed) in shape_polylines(&c.shape) {
            let n = pts.len();
            let count = if closed { n } else { n.saturating_sub(1) };
            for i in 0..count {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                if a.distance(b) > 1.0 {
                    v.push((c.floor, a, b));
                }
            }
        }
    }
    v
}

fn double_line_walls(doc: &mut Document, cads: &[Cad], out: &mut Outcome) -> Result<(), ApiError> {
    let segs = segments(cads);
    let types: Vec<(Id, f64)> = doc.iter::<WallType>().filter(|t| t.railing.is_none()).map(|t| (t.id, t.thickness().mm())).collect();
    let mut used = vec![false; segs.len()];
    let mut made = 0;
    for i in 0..segs.len() {
        if used[i] {
            continue;
        }
        let (floor, a, b) = segs[i];
        let dir = (b - a).normalize();
        let len = a.distance(b);
        // The nearest parallel partner 2"–16" away that overlaps by half.
        let mut best: Option<(usize, f64, f64, f64)> = None;
        for j in (i + 1)..segs.len() {
            let (f2, c, d) = segs[j];
            if used[j] || f2 != floor {
                continue;
            }
            let dir2 = (d - c).normalize();
            if dir.dot(dir2).abs() < 0.999 {
                continue;
            }
            let gap = (c - a).perp_dot(dir).abs();
            if !(50.0..=410.0).contains(&gap) {
                continue;
            }
            let (u0, u1) = {
                let (p, q) = ((c - a).dot(dir), (d - a).dot(dir));
                (p.min(q).max(0.0), p.max(q).min(len))
            };
            if u1 - u0 < 0.5 * len.min(c.distance(d)) {
                continue;
            }
            if best.is_none_or(|x| gap < x.1) {
                best = Some((j, gap, u0, u1));
            }
        }
        let Some((j, gap, u0, u1)) = best else { continue };
        used[i] = true;
        used[j] = true;
        let (_, c, _) = segs[j];
        let side = dir.perp() * (c - a).dot(dir.perp()).signum();
        let mid = |u: f64| a + dir * u + side * gap / 2.0;
        let wall_type = types
            .iter()
            .min_by(|x, y| (x.1 - gap).abs().total_cmp(&(y.1 - gap).abs()))
            .map(|t| t.0)
            .unwrap_or(doc.defaults_for(floor).exterior_wall_type);
        let layer = doc.defaults_for(floor).layers.walls;
        out.created.push(crate::ops::add_wall(doc, floor, pt(mid(u0)), pt(mid(u1)), wall_type, None, layer)?);
        made += 1;
    }
    // Lone lines: single walls.
    for (k, (floor, a, b)) in segs.iter().enumerate() {
        if used[k] {
            continue;
        }
        let d = doc.defaults_for(*floor).clone();
        out.created.push(crate::ops::add_wall(doc, *floor, pt(*a), pt(*b), d.exterior_wall_type, None, d.layers.walls)?);
        made += 1;
    }
    if made == 0 {
        return Err(ApiError::Invalid("no lines to make walls from".into()));
    }
    Ok(())
}

pub(crate) fn roof_outlines(doc: &mut Document, ids: &[Id], out: &mut Outcome) -> Result<(), ApiError> {
    let planes: Vec<RoofPlane> = if ids.is_empty() {
        doc.iter::<RoofPlane>().filter(|r| !r.gable).cloned().collect()
    } else {
        ids.iter().filter_map(|id| doc.get::<RoofPlane>(*id).cloned()).collect()
    };
    if planes.is_empty() {
        return Err(ApiError::Invalid("no roof planes to trace".into()));
    }
    for r in planes {
        let layer = doc.defaults_for(r.floor).layers.cad;
        let points: Vec<Point2> = r.outline.iter().map(|p| {
            let [x, y, _] = p.mm();
            Point2::from_mm(x, y)
        }).collect();
        out.created.push(doc.insert(Cad {
            id: Id(0),
            floor: r.floor,
            layer,
            shape: Shape::Polyline { points, closed: true },
            color: None,
            fill: None,
            fill_style: None,
        }));
    }
    let _ = v2;
    Ok(())
}

/// A small repeatable random sequence (no dependency needed).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn scatter(doc: &mut Document, source: Id, region: Id, count: u32, seed: u64, size_variation: f64, rotate: bool, out: &mut Outcome) -> Result<(), ApiError> {
    let src = doc.object(source).cloned().ok_or(ApiError::NotFound(source))?;
    let (pos_key, turn_key, size_keys): (&str, Option<&str>, &[&str]) = match &src {
        Object::Plant(_) => ("at", None, &["height", "spread"]),
        Object::Symbol(_) => ("position", Some("facing"), &["width", "depth", "height"]),
        Object::BlockRef(_) => ("position", Some("rotation"), &["scale"]),
        _ => return Err(ApiError::Invalid("scatter plants, 3D symbols or CAD blocks".into())),
    };
    let shape = doc.get::<Cad>(region).map(|c| c.shape.clone()).ok_or_else(|| ApiError::Invalid("the region must be a CAD shape".into()))?;
    let Some((pts, closed)) = shape_polylines(&shape).into_iter().next() else {
        return Err(ApiError::Invalid("the region has no outline".into()));
    };
    if count == 0 || count > 5000 {
        return Err(ApiError::Invalid("scatter 1 to 5000 copies".into()));
    }
    let mut rng = Rng(seed.wrapping_add(0x9E37_79B9_7F4A_7C15));
    let mut spots: Vec<DVec2> = Vec::new();
    if closed && pts.len() >= 3 {
        let (lo, hi) = housecraft_geom::math::bounds(pts.iter().copied()).expect("points");
        let mut tries = 0;
        while spots.len() < count as usize && tries < count as usize * 200 {
            tries += 1;
            let p = lo + (hi - lo) * DVec2::new(rng.next(), rng.next());
            if housecraft_geom::math::point_in_polygon(p, &pts) {
                spots.push(p);
            }
        }
    } else {
        // Along the path, evenly, nudged sideways a little.
        let lens: Vec<f64> = pts.windows(2).map(|w| w[0].distance(w[1])).collect();
        let total: f64 = lens.iter().sum();
        let step = total / count as f64;
        for k in 0..count {
            let mut u = (k as f64 + 0.5) * step;
            for (i, l) in lens.iter().enumerate() {
                if u <= *l || i == lens.len() - 1 {
                    let d = (pts[i + 1] - pts[i]).normalize_or_zero();
                    let side = DVec2::new(-d.y, d.x) * (rng.next() - 0.5) * step * 0.6;
                    spots.push(pts[i] + d * u.min(*l) + side);
                    break;
                }
                u -= l;
            }
        }
    }
    let base = serde_json::to_value(&src).expect("serialises");
    for p in spots {
        let mut v = base.clone();
        v[pos_key] = serde_json::to_value(pt(p)).expect("point");
        if rotate && let Some(k) = turn_key {
            v[k] = serde_json::json!(rng.next() * 360.0);
        }
        let f = 1.0 + (rng.next() * 2.0 - 1.0) * size_variation.clamp(0.0, 0.9);
        for k in size_keys {
            if let Some(x) = v.get(*k).and_then(|x| x.as_f64()) {
                v[*k] = serde_json::json!(x * f);
            }
        }
        v["id"] = serde_json::json!(0);
        let o: Object = serde_json::from_value(v).map_err(|e| ApiError::Invalid(e.to_string()))?;
        out.created.push(doc.insert(o));
    }
    let _ = v2;
    Ok(())
}
