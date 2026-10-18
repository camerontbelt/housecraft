//! Pier-and-beam foundations: where the piers and beams go.

use glam::DVec2;
use housecraft_model::{Document, Floor, FoundationKind};

use crate::math::{inset_polygon, long_axis, perp, segment_intersection, simplify_collinear};
use crate::rooms;

/// Beam size: a built-up 6x10.
pub const BEAM_WIDTH: f64 = 5.5 * 25.4;
pub const BEAM_DEPTH: f64 = 9.25 * 25.4;

#[derive(Debug, Clone)]
pub struct PierLayout {
    pub piers: Vec<DVec2>,
    /// Beam centrelines.
    pub beams: Vec<[DVec2; 2]>,
    pub diameter: f64,
    pub footing: f64,
    pub footing_depth: f64,
    /// Ground level the piers stand on.
    pub bottom: f64,
    /// Underside of the floor platform the beams carry.
    pub top: f64,
}

/// The pier layout under the floor above a pier foundation floor: a perimeter
/// beam just inside the outline and girders every `pier_spacing` along the
/// long axis, with piers at the corners and every `pier_spacing` along beams.
pub fn layout(doc: &Document, floor: &Floor) -> Option<PierLayout> {
    let fd = floor
        .foundation
        .as_ref()
        .filter(|f| f.kind == FoundationKind::Piers)?;
    let above = doc.floor_above(floor.id)?;
    let spacing = fd.pier_spacing.mm().max(600.0);
    let mut piers = Vec::new();
    let mut beams = Vec::new();
    let along_beam = |a: DVec2, b: DVec2, ends: bool, piers: &mut Vec<DVec2>| {
        let n = ((a.distance(b) / spacing).ceil() as usize).max(1);
        let range = if ends { 0..n } else { 1..n };
        for k in range {
            piers.push(a.lerp(b, k as f64 / n as f64));
        }
    };
    for fp in rooms::detect(doc, above.id).footprints {
        let outline = simplify_collinear(&fp);
        if outline.len() < 3 {
            continue;
        }
        let ring = inset_polygon(&outline, BEAM_WIDTH / 2.0);
        let n = ring.len();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            beams.push([a, b]);
            along_beam(a, b, true, &mut piers);
        }
        // Girders across the inside, parallel to the long axis.
        let u = long_axis(&ring);
        let v = perp(u);
        let (lo, hi) = ring
            .iter()
            .map(|p| p.dot(v))
            .fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
        let reach = ring.iter().map(|p| p.length()).fold(0.0, f64::max) * 4.0 + 1e5;
        let rows = ((hi - lo) / spacing).ceil() as usize;
        for j in 1..rows {
            let c = lo + (hi - lo) * j as f64 / rows as f64;
            let (p0, p1) = (v * c - u * reach, v * c + u * reach);
            let mut ts: Vec<f64> = (0..n)
                .filter_map(|i| {
                    segment_intersection(p0, p1, ring[i], ring[(i + 1) % n]).map(|x| x.1)
                })
                .collect();
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
            for pair in ts.chunks(2) {
                if let [t0, t1] = pair {
                    let (a, b) = (p0.lerp(p1, *t0), p0.lerp(p1, *t1));
                    beams.push([a, b]);
                    along_beam(a, b, false, &mut piers);
                }
            }
        }
    }
    Some(PierLayout {
        piers,
        beams,
        diameter: fd.pier_diameter.mm(),
        footing: fd.footing_width.mm().max(fd.pier_diameter.mm()),
        footing_depth: fd.footing_depth.mm(),
        bottom: floor.elevation.mm(),
        top: (above.elevation - above.floor_structure).mm(),
    })
}
