//! Reading Chief Architect plan data (`.plan` data region and undo snapshots).
//!
//! The format is being decoded field by field (see `docs/formats/plan.md`);
//! what is here is evidence-based but partial. Wall records are found by their
//! shape rather than by parsing the object stream, which isn't understood yet.

use serde::Serialize;

/// A wall's centreline as stored by Chief Architect, in inches.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WallRecord {
    /// Byte offset of the record's start point.
    pub offset: usize,
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub length: f64,
    /// Height from the first profile entry (both stored heights are equal so
    /// far); `None` where the entry stores 0, as some joined walls do.
    pub height: Option<f64>,
    /// Overall thickness, from where the first profile entry's span starts before the start point.
    pub thickness: f64,
}

// Field offsets within a wall record, relative to the start point.
const DIR: usize = 0x10;
const LENGTH: usize = 0x20;
const LAYER: usize = 0x71;
const RECORD: usize = LAYER + 0x20;

fn f64_at(b: &[u8], o: usize) -> Option<f64> {
    Some(f64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

fn sane(v: f64) -> bool {
    v.is_finite() && v.abs() < 1e6
}

/// Read a wall record whose start point is at `o`, if the bytes there have
/// the shape of one:
///
/// | offset | f64 | meaning |
/// |---|---|---|
/// | +0x00, +0x08 | start x, y | inches |
/// | +0x10, +0x18 | unit direction | |
/// | +0x20 | length | start + direction × length = end |
/// | +0x71, +0x79 | heights | first of the wall's vertical profile entries |
/// | +0x81, +0x89 | span along the wall | starts at −thickness/2 |
pub fn wall_at(b: &[u8], o: usize) -> Option<WallRecord> {
    let (dx, dy) = (f64_at(b, o + DIR)?, f64_at(b, o + DIR + 8)?);
    if !(sane(dx) && sane(dy) && (dx * dx + dy * dy - 1.0).abs() < 1e-9) {
        return None;
    }
    let (x, y, length) = (f64_at(b, o)?, f64_at(b, o + 8)?, f64_at(b, o + LENGTH)?);
    if !(sane(x) && sane(y) && sane(length) && length > 1.0) {
        return None;
    }
    let (h1, h2) = (f64_at(b, o + LAYER)?, f64_at(b, o + LAYER + 8)?);
    let (s0, s1) = (f64_at(b, o + LAYER + 16)?, f64_at(b, o + LAYER + 24)?);
    let plausible_height = |h: f64| sane(h) && (h == 0.0 || (1.0..2000.0).contains(&h));
    if !(plausible_height(h1) && plausible_height(h2) && sane(s0) && sane(s1)) {
        return None;
    }
    // The span covers the length plus the wall's thickness at the joins.
    if !(s0 < 0.0 && s0 > -60.0 && ((s1 - s0) - length).abs() < 40.0) {
        return None;
    }
    Some(WallRecord {
        offset: o,
        start: [x, y],
        end: [x + dx * length, y + dy * length],
        length,
        height: (h1 > 0.0).then_some(h1),
        thickness: -2.0 * s0,
    })
}

/// Every wall-shaped record in `bytes`, in file order.
///
/// Undo snapshots hold the plan before and after their action, so most walls
/// appear twice; [`latest_walls`] keeps the second copy.
pub fn scan_walls(bytes: &[u8]) -> Vec<WallRecord> {
    let mut out = Vec::new();
    let mut o = 0;
    while o + RECORD <= bytes.len() {
        if let Some(w) = wall_at(bytes, o) {
            o += RECORD;
            out.push(w);
        } else {
            o += 1;
        }
    }
    out
}

/// The walls of the last plan state in an undo snapshot: the records after
/// the point where the walls start repeating.
pub fn latest_walls(bytes: &[u8]) -> Vec<WallRecord> {
    let all = scan_walls(bytes);
    // The second state begins at the first record that repeats the first one's start point.
    let restart = all
        .iter()
        .skip(1)
        .position(|w| all.first().is_some_and(|f| w.start == f.start))
        .map(|p| p + 1);
    match restart {
        Some(i) => all[i..].to_vec(),
        None => all,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(start: [f64; 2], dir: [f64; 2], length: f64, height: f64, thickness: f64) -> Vec<u8> {
        let mut b = vec![0u8; RECORD + 16];
        let put = |b: &mut Vec<u8>, o: usize, v: f64| b[o..o + 8].copy_from_slice(&v.to_le_bytes());
        put(&mut b, 0, start[0]);
        put(&mut b, 8, start[1]);
        put(&mut b, DIR, dir[0]);
        put(&mut b, DIR + 8, dir[1]);
        put(&mut b, LENGTH, length);
        put(&mut b, LAYER, height);
        put(&mut b, LAYER + 8, height);
        put(&mut b, LAYER + 16, -thickness / 2.0);
        put(&mut b, LAYER + 24, length + thickness / 2.0);
        b
    }

    #[test]
    fn finds_walls_by_shape() {
        let mut bytes = b"noise before the walls".to_vec();
        bytes.extend(record([2.0, 0.0], [1.0, 0.0], 120.0, 109.125, 6.9475));
        bytes.extend(b"between");
        bytes.extend(record([122.0, 0.0], [0.0, 1.0], 96.0, 109.125, 4.5));
        let walls = scan_walls(&bytes);
        assert_eq!(walls.len(), 2);
        assert_eq!(walls[0].end, [122.0, 0.0]);
        assert!((walls[0].thickness - 6.9475).abs() < 1e-9);
        assert_eq!(walls[1].start, [122.0, 0.0]);
        assert_eq!(walls[1].end, [122.0, 96.0]);
        assert_eq!(walls[1].height, Some(109.125));
    }

    #[test]
    fn latest_walls_keeps_the_after_state() {
        let mut bytes = Vec::new();
        bytes.extend(record([0.0, 0.0], [1.0, 0.0], 100.0, 100.0, 6.0));
        bytes.extend(record([100.0, 0.0], [0.0, 1.0], 50.0, 100.0, 6.0));
        bytes.extend(b"--- second state ---");
        bytes.extend(record([0.0, 0.0], [1.0, 0.0], 100.0, 100.0, 6.0));
        bytes.extend(record([100.0, 0.0], [0.0, 1.0], 80.0, 100.0, 6.0));
        let walls = latest_walls(&bytes);
        assert_eq!(walls.len(), 2);
        assert_eq!(walls[1].end, [100.0, 80.0]);
    }
}
