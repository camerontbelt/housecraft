//! 2D affine transforms applied to model objects (move, rotate, mirror, scale).

use glam::{DMat2, DVec2};
use housecraft_model::{DimensionGeometry, Shape};
use housecraft_units::{Length, Point2};

use crate::math::{pt, v2};

/// `p ↦ m * p + t`, with `m` a rotation, reflection or uniform scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2 {
    pub m: DMat2,
    pub t: DVec2,
}

impl Transform2 {
    pub const IDENTITY: Transform2 = Transform2 {
        m: DMat2::IDENTITY,
        t: DVec2::ZERO,
    };

    pub fn translate(d: DVec2) -> Self {
        Transform2 {
            m: DMat2::IDENTITY,
            t: d,
        }
    }

    pub fn rotate(center: DVec2, degrees: f64) -> Self {
        let m = DMat2::from_angle(degrees.to_radians());
        Transform2 {
            m,
            t: center - m * center,
        }
    }

    /// Reflect across the line through `a` and `b`.
    pub fn mirror(a: DVec2, b: DVec2) -> Option<Self> {
        let d = (b - a).try_normalize()?;
        let m = DMat2::from_cols(
            DVec2::new(2.0 * d.x * d.x - 1.0, 2.0 * d.x * d.y),
            DVec2::new(2.0 * d.x * d.y, 2.0 * d.y * d.y - 1.0),
        );
        Some(Transform2 { m, t: a - m * a })
    }

    pub fn scale(center: DVec2, factor: f64) -> Self {
        let m = DMat2::from_diagonal(DVec2::splat(factor));
        Transform2 {
            m,
            t: center - m * center,
        }
    }

    /// Where a placed block's shapes go: mirrored left to right (if asked),
    /// scaled, rotated by `degrees`, then moved to `at`.
    pub fn placement(at: DVec2, degrees: f64, scale: f64, mirrored: bool) -> Self {
        let flip = DMat2::from_diagonal(DVec2::new(if mirrored { -1.0 } else { 1.0 }, 1.0));
        Transform2 {
            m: DMat2::from_angle(degrees.to_radians()) * DMat2::from_diagonal(DVec2::splat(scale)) * flip,
            t: at,
        }
    }

    pub fn then(self, next: Transform2) -> Transform2 {
        Transform2 {
            m: next.m * self.m,
            t: next.m * self.t + next.t,
        }
    }

    pub fn apply(&self, p: DVec2) -> DVec2 {
        self.m * p + self.t
    }

    pub fn apply_point(&self, p: Point2) -> Point2 {
        pt(self.apply(v2(p)))
    }

    pub fn apply_vector(&self, v: DVec2) -> DVec2 {
        self.m * v
    }

    pub fn is_reflection(&self) -> bool {
        self.m.determinant() < 0.0
    }

    pub fn scale_factor(&self) -> f64 {
        self.m.determinant().abs().sqrt()
    }

    /// Map a direction angle (degrees CCW from east).
    pub fn apply_angle(&self, degrees: f64) -> f64 {
        let d = self.apply_vector(DVec2::from_angle(degrees.to_radians()));
        d.y.atan2(d.x).to_degrees()
    }

    pub fn apply_length(&self, l: Length) -> Length {
        l * self.scale_factor()
    }

    pub fn apply_shape(&self, shape: &Shape) -> Shape {
        let p = |q: &Point2| self.apply_point(*q);
        match shape {
            Shape::Point { at } => Shape::Point { at: p(at) },
            Shape::Line {
                a,
                b,
                arrow_start,
                arrow_end,
            } => Shape::Line {
                a: p(a),
                b: p(b),
                arrow_start: *arrow_start,
                arrow_end: *arrow_end,
            },
            Shape::Polyline { points, closed } => Shape::Polyline {
                points: points.iter().map(p).collect(),
                closed: *closed,
            },
            Shape::Spline { points, closed } => Shape::Spline {
                points: points.iter().map(p).collect(),
                closed: *closed,
            },
            Shape::Arc {
                center,
                radius,
                start_angle,
                sweep,
            } => {
                let start = self.apply_angle(*start_angle);
                Shape::Arc {
                    center: p(center),
                    radius: self.apply_length(*radius),
                    start_angle: start,
                    sweep: if self.is_reflection() { -sweep } else { *sweep },
                }
            }
            Shape::Circle { center, radius } => Shape::Circle {
                center: p(center),
                radius: self.apply_length(*radius),
            },
            Shape::CrossBox { a, b, width } => Shape::CrossBox {
                a: p(a),
                b: p(b),
                width: self.apply_length(*width),
            },
            Shape::BlockingBox { a, b, width } => Shape::BlockingBox {
                a: p(a),
                b: p(b),
                width: self.apply_length(*width),
            },
            Shape::Insulation { a, b, width } => Shape::Insulation {
                a: p(a),
                b: p(b),
                width: self.apply_length(*width),
            },
            Shape::Ellipse {
                center,
                rx,
                ry,
                rotation,
            } => {
                let rot = self.apply_angle(*rotation);
                Shape::Ellipse {
                    center: p(center),
                    rx: self.apply_length(*rx),
                    ry: self.apply_length(*ry),
                    rotation: rot,
                }
            }
        }
    }

    pub fn apply_dimension(&self, g: &DimensionGeometry) -> DimensionGeometry {
        let p = |q: &Point2| self.apply_point(*q);
        match g {
            DimensionGeometry::Linear {
                points,
                orientation,
                offset,
            } => DimensionGeometry::Linear {
                points: points.iter().map(p).collect(),
                orientation: *orientation,
                // Reflection swaps which side "left" is.
                offset: if self.is_reflection() {
                    -self.apply_length(*offset)
                } else {
                    self.apply_length(*offset)
                },
            },
            DimensionGeometry::Angular {
                center,
                start,
                end,
                radius,
            } => {
                let (s, e) = if self.is_reflection() {
                    (p(end), p(start))
                } else {
                    (p(start), p(end))
                };
                DimensionGeometry::Angular {
                    center: p(center),
                    start: s,
                    end: e,
                    radius: self.apply_length(*radius),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: DVec2, b: DVec2) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn rotate_mirror_scale() {
        let r = Transform2::rotate(DVec2::new(1.0, 1.0), 90.0);
        assert!(close(r.apply(DVec2::new(2.0, 1.0)), DVec2::new(1.0, 2.0)));
        let m = Transform2::mirror(DVec2::ZERO, DVec2::Y).unwrap();
        assert!(close(m.apply(DVec2::new(3.0, 4.0)), DVec2::new(-3.0, 4.0)));
        assert!(m.is_reflection());
        assert!((m.apply_angle(0.0) - 180.0).abs() < 1e-9);
        let s = Transform2::scale(DVec2::ZERO, 2.0);
        assert!((s.scale_factor() - 2.0).abs() < 1e-12);
        let both = r.then(Transform2::translate(DVec2::X));
        assert!(close(
            both.apply(DVec2::new(2.0, 1.0)),
            DVec2::new(2.0, 2.0)
        ));
    }
}
