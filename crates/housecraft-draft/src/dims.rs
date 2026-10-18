//! Dimension drawing.

use housecraft_geom::math::{arc_points, perp};
use housecraft_geom::{DVec2, v2};
use housecraft_model::{
    ArrowStyle, DefaultSet, DimOrientation, Dimension, DimensionGeometry, Document, Justify,
};
use housecraft_units::{DisplayUnit, Length};

use crate::{Drawing, Fill, Ink, Prim, Stroke};

/// Format a measured length per the document's display unit and the
/// "inches below" threshold of the dimension defaults.
pub fn format_dimension(doc: &Document, defaults: &DefaultSet, value: Length) -> String {
    let unit = doc.settings.display_unit;
    match unit {
        DisplayUnit::FeetInches { denominator }
            if value.mm() < defaults.dimensions.inches_below.mm() =>
        {
            value.format(DisplayUnit::Inches { denominator })
        }
        _ => value.format(unit),
    }
}

pub(crate) struct DimStyle {
    pub stroke: Stroke,
    pub ink: Ink,
    pub text_height: f64,
    pub arrow: ArrowStyle,
    pub gap: f64,
    pub overshoot: f64,
}

fn readable(angle_deg: f64) -> f64 {
    let a = angle_deg.rem_euclid(360.0);
    if a > 90.0 && a <= 270.0 { a - 180.0 } else { a }
}

fn terminator(out: &mut Drawing, id: housecraft_model::Id, at: DVec2, dir: DVec2, s: &DimStyle) {
    let size = s.text_height * 0.45;
    match s.arrow {
        ArrowStyle::None => {}
        ArrowStyle::Tick => {
            let d = (dir + perp(dir)).normalize() * size;
            out.push(
                Some(id),
                Prim::Polyline {
                    points: vec![at - d, at + d],
                    closed: false,
                    stroke: Stroke {
                        width: s.stroke.width * 1.8,
                        ..s.stroke
                    },
                },
            );
        }
        ArrowStyle::Arrow => {
            // `dir` points from this end toward the other end of the dimension.
            let back = at + dir * size * 2.2;
            let side = perp(dir) * size * 0.6;
            out.push(
                Some(id),
                Prim::Polygon {
                    points: vec![at, back + side, back - side],
                    fill: Fill::Ink(s.ink),
                },
            );
        }
        ArrowStyle::Dot => {
            let mut pts = arc_points(at, size * 0.45, 0.0, 360.0);
            pts.pop();
            out.push(
                Some(id),
                Prim::Polygon {
                    points: pts,
                    fill: Fill::Ink(s.ink),
                },
            );
        }
    }
}

pub(crate) fn draw_dimension(
    doc: &Document,
    defaults: &DefaultSet,
    dim: &Dimension,
    s: &DimStyle,
    out: &mut Drawing,
) {
    let id = dim.id;
    let line = |out: &mut Drawing, a: DVec2, b: DVec2| {
        out.push(
            Some(id),
            Prim::Polyline {
                points: vec![a, b],
                closed: false,
                stroke: s.stroke,
            },
        );
    };
    let label = |value: Length| -> String {
        let v = format_dimension(doc, defaults, value);
        match &dim.text_override {
            Some(t) => t.replace("{}", &v),
            None => v,
        }
    };
    match &dim.geometry {
        DimensionGeometry::Linear {
            points,
            orientation,
            offset,
        } => {
            if points.len() < 2 {
                return;
            }
            let pts: Vec<DVec2> = points.iter().map(|p| v2(*p)).collect();
            let first = pts[0];
            let (u, line_point) = match orientation {
                DimOrientation::Aligned => {
                    let u = (pts[pts.len() - 1] - first).normalize_or(DVec2::X);
                    (u, first + perp(u) * offset.mm())
                }
                DimOrientation::Horizontal => {
                    (DVec2::X, DVec2::new(first.x, first.y + offset.mm()))
                }
                DimOrientation::Vertical => (DVec2::Y, DVec2::new(first.x + offset.mm(), first.y)),
            };
            let n = perp(u);
            // Foot of each point on the dimension line, sorted along it.
            let mut feet: Vec<(f64, DVec2)> = pts
                .iter()
                .map(|p| {
                    let t = (*p - line_point).dot(u);
                    (t, *p)
                })
                .collect();
            feet.sort_by(|a, b| a.0.total_cmp(&b.0));
            feet.dedup_by(|a, b| (a.0 - b.0).abs() < 0.5);
            if feet.len() < 2 {
                return;
            }
            for (t, p) in &feet {
                let q = line_point + u * *t;
                let along_n = (q - *p).dot(n);
                let dir = if along_n >= 0.0 { n } else { -n };
                let dist = along_n.abs();
                if dist > s.gap {
                    line(out, *p + dir * s.gap, q + dir * s.overshoot);
                }
            }
            let a = line_point + u * feet[0].0;
            let b = line_point + u * feet[feet.len() - 1].0;
            line(out, a - u * s.overshoot * 0.5, b + u * s.overshoot * 0.5);
            let angle = readable(u.y.atan2(u.x).to_degrees());
            let text_up = perp(DVec2::from_angle(angle.to_radians()));
            for w in feet.windows(2) {
                let qa = line_point + u * w[0].0;
                let qb = line_point + u * w[1].0;
                terminator(out, id, qa, u, s);
                terminator(out, id, qb, -u, s);
                let value = Length::from_mm(w[1].0 - w[0].0);
                out.push(
                    Some(id),
                    Prim::Text {
                        at: (qa + qb) / 2.0 + text_up * s.text_height * 0.35,
                        text: label(value),
                        height: s.text_height,
                        rotation: angle,
                        justify: Justify::Center,
                        middle: false,
                        bold: false,
                        ink: s.ink,
                    },
                );
            }
        }
        DimensionGeometry::Angular {
            center,
            start,
            end,
            radius,
        } => {
            let c = v2(*center);
            let a0 = (v2(*start) - c).to_angle().to_degrees();
            let a1 = (v2(*end) - c).to_angle().to_degrees();
            let sweep = (a1 - a0).rem_euclid(360.0);
            let r = radius.mm();
            out.push(
                Some(id),
                Prim::Polyline {
                    points: arc_points(c, r, a0, sweep),
                    closed: false,
                    stroke: s.stroke,
                },
            );
            for (p, ang) in [(v2(*start), a0), (v2(*end), a1)] {
                let dir = DVec2::from_angle(ang.to_radians());
                if p.distance(c) < r {
                    line(
                        out,
                        p + dir * s.gap.min(r - p.distance(c)),
                        c + dir * (r + s.overshoot),
                    );
                }
            }
            let mid = (a0 + sweep / 2.0).to_radians();
            out.push(
                Some(id),
                Prim::Text {
                    at: c + DVec2::from_angle(mid) * (r + s.text_height * 0.9),
                    text: dim.text_override.clone().map_or_else(
                        || format!("{sweep:.1}°"),
                        |t| t.replace("{}", &format!("{sweep:.1}°")),
                    ),
                    height: s.text_height,
                    rotation: 0.0,
                    justify: Justify::Center,
                    middle: true,
                    bold: false,
                    ink: s.ink,
                },
            );
        }
    }
}
