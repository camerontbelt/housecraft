//! Elevation and cross-section drawings.
//!
//! The 3D model's faces are projected onto the vertical plane through the
//! view's line and painted far to near, so nearer surfaces cover what is
//! behind them. A section first discards everything in front of the line and
//! draws the cut edges heavy. Drawing coordinates are millimetres: x along the
//! line, y up (height).

use housecraft_geom::math::{perp, signed_area};
use housecraft_geom::mesh::{Category, build};
use housecraft_geom::{DVec2, DVec3, v2};
use housecraft_model::*;
use std::collections::BTreeMap;
use housecraft_units::Length;

use crate::{Drawing, Fill, Ink, Prim, Stroke};

/// Keep the part of a polygon where `f >= 0` (Sutherland–Hodgman); points
/// created on the boundary are flagged so cut edges can be found.
fn clip(poly: &[(DVec3, bool)], f: impl Fn(DVec3) -> f64) -> Vec<(DVec3, bool)> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (p, pc) = poly[i];
        let (q, _) = poly[(i + 1) % poly.len()];
        let (fp, fq) = (f(p), f(q));
        if fp >= 0.0 {
            out.push((p, pc));
        }
        if (fp >= 0.0) != (fq >= 0.0) {
            let t = fp / (fp - fq);
            out.push((p.lerp(q, t), true));
        }
    }
    out
}

fn shade(color: [f32; 4], k: f64) -> Color {
    let c = |v: f32| ((v as f64 * k).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::rgb(c(color[0]), c(color[1]), c(color[2]))
}

/// The cut's offset beyond the line at distance `u` along it.
fn step_offset(steps: &[SectionStep], u: f64) -> f64 {
    steps.iter().rfind(|s| u >= s.at.mm()).map_or(0.0, |s| s.offset.mm())
}

/// Split a polygon at the stepped plane's jogs so each piece has one cut.
fn split_at_steps(poly: Vec<(DVec3, bool)>, steps: &[SectionStep]) -> Vec<Vec<(DVec3, bool)>> {
    let mut pieces = vec![poly];
    for s in steps {
        let x = s.at.mm();
        let mut next = Vec::new();
        for piece in pieces {
            // The jog line itself is not a cut edge.
            let unflag = |v: Vec<(DVec3, bool)>| -> Vec<(DVec3, bool)> {
                v.into_iter().map(|(p, c)| (p, c && (p.x - x).abs() > 1e-6)).collect()
            };
            let lo = unflag(clip(&piece, |p| x - p.x));
            let hi = unflag(clip(&piece, |p| p.x - x));
            next.extend([lo, hi].into_iter().filter(|v| v.len() >= 3));
        }
        pieces = next;
    }
    pieces
}

/// The horizontal and vertical limits a view draws within.
struct Extents {
    left: Option<f64>,
    right: Option<f64>,
    bottom: Option<f64>,
    top: Option<f64>,
}

fn extents(doc: &Document, view: &SectionView, len: f64) -> Extents {
    let side = |crop: Option<Length>, clipped: f64| crop.map(|c| c.mm()).or(view.clip_sides.then_some(clipped));
    let floor_top = view
        .ignore_above
        .then(|| doc.get::<Floor>(view.floor))
        .flatten()
        .map(|f| f.elevation.mm() + f.ceiling_height.mm() + 1.0);
    let top = match (view.crop_top.map(|t| t.mm()), floor_top) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    Extents {
        left: side(view.crop_left, 0.0),
        right: side(view.crop_right, len),
        bottom: view.crop_bottom.map(|b| b.mm()),
        top,
    }
}

/// Keep the part of `poly` where `f >= 0`; the new boundary is an extent,
/// not a section cut.
fn crop(poly: &[(DVec3, bool)], f: impl Fn(DVec3) -> f64) -> Vec<(DVec3, bool)> {
    clip(poly, &f).into_iter().map(|(p, c)| (p, c && f(p).abs() > 1e-6)).collect()
}

// (distance beyond the line, polygon in view coordinates, fill, object, cut edges)
type Painted = (f64, Vec<DVec2>, Color, Option<Id>, Vec<[DVec2; 2]>);

/// Draw `view`: an elevation (everything beyond the line) or a cross
/// section (cut at the line, cut edges heavy).
pub fn section_view(doc: &Document, view: &SectionView) -> Drawing {
    let mesh = build(doc, None);
    let (a, b) = (v2(view.a), v2(view.b));
    let len = a.distance(b);
    let dir = (b - a) / len.max(1e-9);
    // The view looks toward the left of a→b.
    let n = perp(dir);
    let local = |p: DVec3| {
        let q = p.truncate() - a;
        DVec3::new(q.dot(dir), q.dot(n), p.z)
    };
    let section = view.section_kind == SectionKind::Section;
    let depth = view.depth.map(|d| d.mm());
    let ext = extents(doc, view, len);
    let mut steps = view.steps.clone();
    steps.sort_by(|x, y| x.at.mm().total_cmp(&y.at.mm()));
    let grade = housecraft_geom::sun::grade(doc);
    // A room elevation shows the ceiling's profile (vaults and trays).
    let ceiling_profile = view.clip_sides && !section;

    let thin = Stroke {
        ink: Ink::Default,
        width: 0.13,
        dash: &[],
    };
    let heavy = Stroke {
        ink: Ink::Default,
        width: 0.5,
        dash: &[],
    };
    let hidden = Stroke {
        ink: Ink::Default,
        width: 0.13,
        dash: &[1.5, 1.0],
    };

    let mut faces: Vec<Painted> = Vec::new();
    let mut below: Vec<(Option<Id>, Vec<DVec2>)> = Vec::new();
    let mut ceilings: Vec<(Option<Id>, Vec<DVec2>)> = Vec::new();
    // Openings seen: their extent in the view and nearest distance.
    let mut openings: BTreeMap<Id, (DVec2, DVec2, f64)> = BTreeMap::new();
    let (mut umin, mut umax, mut zmin) = (f64::MAX, f64::MIN, f64::MAX);
    for face in &mesh.faces {
        let part = mesh.parts[face.part];
        let is_ceiling = part.category == Category::Ceiling;
        // Ceilings would hide the room in elevations (a room elevation
        // outlines them); sections show them cut.
        if is_ceiling && !section && !ceiling_profile {
            continue;
        }
        // Faces turned away from the viewer can't be seen (and would be
        // painted over the surfaces in front of them, like a roof's soffit).
        if !is_ceiling && face.normal.truncate().dot(n) > 1e-6 {
            continue;
        }
        let poly: Vec<(DVec3, bool)> = face.points.iter().map(|p| (local(*p), false)).collect();
        let pieces = if section && !steps.is_empty() {
            split_at_steps(poly, &steps)
        } else {
            vec![poly]
        };
        for mut poly in pieces {
            let off = if section {
                let cx = poly.iter().map(|(p, _)| p.x).sum::<f64>() / poly.len() as f64;
                step_offset(&steps, cx)
            } else {
                0.0
            };
            if section {
                poly = clip(&poly, |p| p.y - off);
            }
            if let Some(d) = depth {
                poly = crop(&poly, |p| d - p.y);
            }
            if let Some(l) = ext.left {
                poly = crop(&poly, |p| p.x - l);
            }
            if let Some(r) = ext.right {
                poly = crop(&poly, |p| r - p.x);
            }
            if let Some(bt) = ext.bottom {
                poly = crop(&poly, |p| p.z - bt);
            }
            if let Some(t) = ext.top {
                poly = crop(&poly, |p| t - p.z);
            }
            if poly.len() < 3 {
                continue;
            }
            // Elevations: what is underground is drawn as hidden lines.
            if view.hidden_below_grade
                && !section
                && !is_ceiling
                && let Some(g) = grade
                && poly.iter().any(|(p, _)| p.z < g - 1.0)
            {
                let under = crop(&poly, |p| g - p.z);
                if under.len() >= 3 {
                    let object = (part.object != Id(0)).then_some(part.object);
                    zmin = under.iter().fold(zmin, |z, (p, _)| z.min(p.z));
                    below.push((object, under.iter().map(|(p, _)| DVec2::new(p.x, p.z)).collect()));
                }
                poly = crop(&poly, |p| p.z - g);
                if poly.len() < 3 {
                    continue;
                }
            }
            // Cut edges lie on the section plane: both ends created by the cut.
            let mut cuts = Vec::new();
            if section {
                for i in 0..poly.len() {
                    let (p, pc) = poly[i];
                    let (q, qc) = poly[(i + 1) % poly.len()];
                    if pc && qc && (p.y - off).abs() < 1e-6 && (q.y - off).abs() < 1e-6 && p.distance(q) > 1.0 {
                        cuts.push([DVec2::new(p.x, p.z), DVec2::new(q.x, q.z)]);
                    }
                }
            }
            let flat: Vec<DVec2> = poly.iter().map(|(p, _)| DVec2::new(p.x, p.z)).collect();
            for p in &flat {
                umin = umin.min(p.x);
                umax = umax.max(p.x);
                zmin = zmin.min(p.y);
            }
            let object = (part.object != Id(0)).then_some(part.object);
            if is_ceiling && !section {
                ceilings.push((object, flat));
                continue;
            }
            if signed_area(&flat).abs() < 1.0 && cuts.is_empty() {
                continue; // edge-on: its edges come from neighbouring faces
            }
            let mean = poly.iter().map(|(p, _)| p.y).sum::<f64>() / poly.len() as f64;
            if part.category == Category::Opening
                && let Some(id) = object
                && faces_view(doc, id, n)
                && let Some((lo, hi)) = housecraft_geom::math::bounds(flat.iter().copied())
            {
                let e = openings.entry(id).or_insert((lo, hi, mean));
                e.0 = e.0.min(lo);
                e.1 = e.1.max(hi);
                e.2 = e.2.min(mean);
            }
            // Faces turned toward the viewer are lit; others darker.
            let facing = (-face.normal.truncate().dot(n)).max(0.0);
            let fill = shade(
                face.color,
                0.72 + 0.28 * facing + 0.1 * face.normal.z.max(0.0),
            );
            faces.push((mean, flat, fill, object, cuts));
        }
    }
    faces.sort_by(|x, y| y.0.total_cmp(&x.0));

    let mut out = Drawing::default();
    // Hidden lines first: whatever is drawn above grade covers them.
    for (object, flat) in below {
        out.push(
            object,
            Prim::Polyline {
                points: flat,
                closed: true,
                stroke: hidden,
            },
        );
    }
    let mut cut_lines = Vec::new();
    for (_, flat, fill, object, cuts) in &faces {
        if signed_area(flat).abs() >= 1.0 {
            out.push(
                *object,
                Prim::Polygon {
                    points: flat.clone(),
                    fill: Fill::Ink(Ink::Color(*fill)),
                },
            );
            out.push(
                *object,
                Prim::Polyline {
                    points: flat.clone(),
                    closed: true,
                    stroke: thin,
                },
            );
        }
        cut_lines.extend(cuts.iter().map(|c| (*object, *c)));
    }
    // The ceiling's profile over a room elevation.
    for (object, flat) in ceilings {
        out.push(
            object,
            Prim::Polyline {
                points: flat,
                closed: true,
                stroke: Stroke { width: 0.35, ..thin },
            },
        );
    }
    // Cut edges and the ground go on top.
    for (object, [p, q]) in cut_lines {
        out.push(
            object,
            Prim::Polyline {
                points: vec![p, q],
                closed: false,
                stroke: heavy,
            },
        );
    }
    if view.schedule_labels {
        schedule_labels(doc, &faces, &openings, &mut out);
    }
    if umin < umax
        && let Some(g) = grade
    {
        let pad = if ext.left.is_some() || ext.right.is_some() {
            0.0
        } else {
            (umax - umin) * 0.05
        };
        out.push(
            None,
            Prim::Polyline {
                points: vec![DVec2::new(umin - pad, g), DVec2::new(umax + pad, g)],
                closed: false,
                stroke: heavy,
            },
        );
        out.push(
            None,
            Prim::Text {
                at: DVec2::new((umin + umax) / 2.0, g.min(zmin) - 700.0),
                text: view.name.to_uppercase(),
                height: 300.0,
                rotation: 0.0,
                justify: Justify::Center,
                middle: false,
                bold: true,
                ink: Ink::Default,
            },
        );
    }
    // Lines, dimensions and notes drawn on the view.
    out.items.extend(crate::plan::drafting(doc, view.id, None).items);
    out
}

/// The left and right ends of the model as `view` draws it (without its
/// own annotations), for centring things in the view.
pub fn model_span(doc: &Document, view: &SectionView) -> Option<(f64, f64)> {
    let annotations: std::collections::BTreeSet<Id> = crate::plan::drafting(doc, view.id, None)
        .items
        .iter()
        .filter_map(|i| i.object)
        .collect();
    let mut model = section_view(doc, view);
    model.items.retain(|i| i.object.is_some_and(|o| !annotations.contains(&o)));
    model.bounds().map(|(lo, hi)| (lo.x, hi.x))
}

/// Whether opening `id`'s wall faces the view (not seen edge-on).
fn faces_view(doc: &Document, id: Id, n: DVec2) -> bool {
    doc.get::<Opening>(id)
        .and_then(|o| doc.get::<Wall>(o.wall))
        .is_some_and(|w| perp((v2(w.end) - v2(w.start)).normalize_or_zero()).dot(n).abs() > 0.5)
}

/// Door and window schedule numbers over the openings seen in a view
/// (those not hidden behind something nearer).
fn schedule_labels(doc: &Document, faces: &[Painted], openings: &BTreeMap<Id, (DVec2, DVec2, f64)>, out: &mut Drawing) {
    if openings.is_empty() {
        return;
    }
    let mut labels = BTreeMap::new();
    for kind in [ScheduleKind::Doors, ScheduleKind::Windows] {
        let t = housecraft_api::schedule::schedule(doc, kind, &Default::default());
        let Some(col) = t.columns.iter().position(|c| c == "Label") else {
            continue;
        };
        for row in &t.rows {
            for id in &row.ids {
                labels.insert(*id, row.cells[col].clone());
            }
        }
    }
    let h = 150.0;
    let frame = Stroke {
        ink: Ink::Default,
        width: 0.18,
        dash: &[],
    };
    for (id, (lo, hi, near)) in openings {
        let Some(label) = labels.get(id) else {
            continue;
        };
        let c = (*lo + *hi) / 2.0;
        // Something else nearer covers the opening's middle: hidden.
        let covered = faces
            .iter()
            .any(|(d, poly, _, o, _)| *o != Some(*id) && *d < near - 50.0 && housecraft_geom::math::point_in_polygon(c, poly));
        if covered {
            continue;
        }
        let at = DVec2::new(c.x, hi.y + h * 1.5);
        let w = crate::approx_text_width(label, h) / 2.0 + h * 0.5;
        let corners = vec![at + DVec2::new(-w, -h), at + DVec2::new(w, -h), at + DVec2::new(w, h), at + DVec2::new(-w, h)];
        out.push(
            Some(*id),
            Prim::Polygon {
                points: corners.clone(),
                fill: Fill::Ink(Ink::Color(Color::rgb(255, 255, 255))),
            },
        );
        out.push(
            Some(*id),
            Prim::Polyline {
                points: corners,
                closed: true,
                stroke: frame,
            },
        );
        out.push(
            Some(*id),
            Prim::Text {
                at,
                text: label.clone(),
                height: h,
                rotation: 0.0,
                justify: Justify::Center,
                middle: true,
                bold: false,
                ink: Ink::Default,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_marks_cut_points() {
        let square = vec![
            (DVec3::new(0.0, -1.0, 0.0), false),
            (DVec3::new(1.0, -1.0, 0.0), false),
            (DVec3::new(1.0, 1.0, 0.0), false),
            (DVec3::new(0.0, 1.0, 0.0), false),
        ];
        let kept = clip(&square, |p| p.y);
        assert_eq!(kept.len(), 4);
        assert_eq!(kept.iter().filter(|(_, c)| *c).count(), 2);
        assert!(kept.iter().all(|(p, _)| p.y >= -1e-9));
    }
}
