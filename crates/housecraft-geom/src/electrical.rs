//! Placement helpers for electrical devices.

use glam::DVec2;
use housecraft_model::{Document, Id};

use crate::math::closest_on_segment;
use crate::walls::wall_geoms;

/// The nearest wall face to `p` on `floor` within `reach` mm: the point on
/// the face and the direction (degrees) pointing away from the wall.
pub fn snap_to_wall(doc: &Document, floor: Id, p: DVec2, reach: f64) -> Option<(DVec2, f64)> {
    let mut best: Option<(f64, DVec2, f64)> = None;
    for g in wall_geoms(doc, floor) {
        let len = (g.end - g.start).length();
        for (a, b, normal) in [
            (g.left_at(0.0), g.left_at(len), g.left),
            (g.right_at(0.0), g.right_at(len), -g.left),
        ] {
            let (q, _) = closest_on_segment(p, a, b);
            let d = q.distance(p);
            if d <= reach && best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, q, normal.y.atan2(normal.x).to_degrees()));
            }
        }
    }
    best.map(|(_, q, angle)| (q, angle))
}
