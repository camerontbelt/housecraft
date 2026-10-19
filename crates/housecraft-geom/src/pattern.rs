//! Generated tile patterns: one repeat of a grid, running bond,
//! herringbone or basketweave layout as an RGBA image.

use housecraft_model::{Color, PatternKind, TilePattern};

/// Size of one repeat of the pattern in mm (width, height).
pub fn repeat(p: &TilePattern) -> (f64, f64) {
    let (w, h, g) = (
        p.tile_width.mm().max(1.0),
        p.tile_height.mm().max(1.0),
        p.grout.mm().max(0.0),
    );
    match p.kind {
        PatternKind::Grid => (w + g, h + g),
        PatternKind::RunningBond => (w + g, 2.0 * (h + g)),
        // Planks w long, h wide, zig-zagging at 90°: a square repeat.
        PatternKind::Herringbone => {
            let s = w + h + 2.0 * g;
            (s, s)
        }
        // Squares of `n` planks side by side; two squares turn per repeat.
        PatternKind::Basketweave => {
            let s = w + g;
            (2.0 * s, 2.0 * s)
        }
    }
}

/// Whether point (x, y) in mm within one repeat is grout.
fn is_grout(p: &TilePattern, x: f64, y: f64) -> bool {
    let (w, h, g) = (
        p.tile_width.mm().max(1.0),
        p.tile_height.mm().max(1.0),
        p.grout.mm().max(0.0),
    );
    if g <= 0.0 {
        return false;
    }
    let (rw, rh) = repeat(p);
    let (x, y) = (x.rem_euclid(rw), y.rem_euclid(rh));
    match p.kind {
        PatternKind::Grid => x % (w + g) < g || y % (h + g) < g,
        PatternKind::RunningBond => {
            let row = (y / (h + g)).floor() as i64;
            let shift = if row % 2 == 1 { (w + g) / 2.0 } else { 0.0 };
            (x + shift).rem_euclid(w + g) < g || y % (h + g) < g
        }
        PatternKind::Herringbone => {
            // Planks along +x and +y alternate along the diagonal: in
            // rotated coordinates each band is a staggered row.
            let s = w + h + 2.0 * g;
            let (u, v) = (x + y, y - x);
            let band = (u / (h + g)).floor() as i64;
            let along = v.rem_euclid(s) + if band % 2 == 0 { 0.0 } else { s / 2.0 };
            u.rem_euclid(h + g) < g * 1.4 || along.rem_euclid(s / 2.0) < g * 1.4
        }
        PatternKind::Basketweave => {
            let s = w + g;
            let (cx, cy) = ((x / s).floor() as i64, (y / s).floor() as i64);
            let (lx, ly) = (x % s, y % s);
            let planks = (w / h).round().max(1.0);
            let pitch = s / planks;
            if lx < g || ly < g {
                return true;
            }
            // Alternate squares run their planks across or along.
            if (cx + cy) % 2 == 0 { ly % pitch < g } else { lx % pitch < g }
        }
    }
}

/// One repeat of the pattern as RGBA pixels, `px` pixels on its longer
/// side; returns (width, height, pixels).
pub fn image(p: &TilePattern, tile: Color, px: u32) -> (u32, u32, Vec<u8>) {
    let (rw, rh) = repeat(p);
    let k = px as f64 / rw.max(rh);
    let (w, h) = (((rw * k).round() as u32).max(1), ((rh * k).round() as u32).max(1));
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for j in 0..h {
        for i in 0..w {
            let (x, y) = ((i as f64 + 0.5) / k, (j as f64 + 0.5) / k);
            let c = if is_grout(p, x, y) { p.grout_color } else { tile };
            // Slight variation per tile so the pattern reads in 3D.
            let [r, g, b, _] = c.0;
            out.extend([r, g, b, 255]);
        }
    }
    (w, h, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_units::Length;

    fn pat(kind: PatternKind) -> TilePattern {
        TilePattern {
            kind,
            tile_width: Length::from_inches(12.0),
            tile_height: Length::from_inches(6.0),
            grout: Length::from_inches(0.25),
            grout_color: Color::rgb(40, 40, 40),
        }
    }

    #[test]
    fn patterns_have_grout_and_tile() {
        for kind in PatternKind::ALL {
            let (w, h, px) = image(&pat(kind), Color::rgb(220, 200, 180), 128);
            assert!(w > 0 && h > 0 && px.len() == (w * h * 4) as usize);
            let grout = px.chunks(4).filter(|c| c[0] == 40).count();
            let tile = px.chunks(4).filter(|c| c[0] == 220).count();
            assert!(grout > 0 && tile > grout, "{kind:?}: {grout} grout, {tile} tile");
        }
    }
}
