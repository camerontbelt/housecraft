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

    // (distance beyond the line, polygon in view coordinates, fill, object, cut edges)
    type Painted = (f64, Vec<DVec2>, Color, Option<Id>, Vec<[DVec2; 2]>);
    let mut faces: Vec<Painted> = Vec::new();
    let (mut umin, mut umax, mut zmin) = (f64::MAX, f64::MIN, f64::MAX);
    for face in &mesh.faces {
        let part = mesh.parts[face.part];
        // Ceilings would hide the room in elevations; sections show them cut.
        if part.category == Category::Ceiling && !section {
            continue;
        }
        // Faces turned away from the viewer can't be seen (and would be
        // painted over the surfaces in front of them, like a roof's soffit).
        if face.normal.truncate().dot(n) > 1e-6 {
            continue;
        }
        let mut poly: Vec<(DVec3, bool)> = face.points.iter().map(|p| (local(*p), false)).collect();
        if section {
            poly = clip(&poly, |p| p.y);
        }
        if let Some(d) = depth {
            poly = clip(&poly, |p| d - p.y);
        }
        if view.clip_sides {
            poly = clip(&poly, |p| p.x);
            poly = clip(&poly, |p| len - p.x);
        }
        if poly.len() < 3 {
            continue;
        }
        // Cut edges lie on the section plane: both ends created by the cut.
        let mut cuts = Vec::new();
        if section {
            for i in 0..poly.len() {
                let (p, pc) = poly[i];
                let (q, qc) = poly[(i + 1) % poly.len()];
                if pc && qc && p.y.abs() < 1e-6 && q.y.abs() < 1e-6 && p.distance(q) > 1.0 {
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
        if signed_area(&flat).abs() < 1.0 && cuts.is_empty() {
            continue; // edge-on: its edges come from neighbouring faces
        }
        let mean = poly.iter().map(|(p, _)| p.y).sum::<f64>() / poly.len() as f64;
        // Faces turned toward the viewer are lit; others darker.
        let facing = (-face.normal.truncate().dot(n)).max(0.0);
        let fill = shade(
            face.color,
            0.72 + 0.28 * facing + 0.1 * face.normal.z.max(0.0),
        );
        let object = (part.object != Id(0)).then_some(part.object);
        faces.push((mean, flat, fill, object, cuts));
    }
    faces.sort_by(|x, y| y.0.total_cmp(&x.0));

    let mut out = Drawing::default();
    let mut cut_lines = Vec::new();
    for (_, flat, fill, object, cuts) in faces {
        if signed_area(&flat).abs() >= 1.0 {
            out.push(
                object,
                Prim::Polygon {
                    points: flat.clone(),
                    fill: Fill::Ink(Ink::Color(fill)),
                },
            );
            out.push(
                object,
                Prim::Polyline {
                    points: flat,
                    closed: true,
                    stroke: thin,
                },
            );
        }
        cut_lines.extend(cuts.into_iter().map(|c| (object, c)));
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
    if umin < umax
        && let Some(g) = housecraft_geom::sun::grade(doc)
    {
        let pad = (umax - umin) * 0.05;
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
    out
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
