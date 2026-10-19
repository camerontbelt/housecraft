//! Decks and raised platforms: decking, border, and the framing under it
//! (joists, rims, ledger, beams, posts, footings), railings and a cover.

use glam::{DVec2, DVec3};
use housecraft_model::*;

use crate::framing::{Member, clip_line};
use crate::math::{distance_to_polyline, inset_polygon, perp, signed_area, spline_points, v2};
use crate::mesh::{Category, Look, Mesh};

const IN: f64 = 25.4;

/// Everything a deck is made of.
#[derive(Debug, Clone, Default)]
pub struct DeckParts {
    /// The outline (counter-clockwise, curves smoothed).
    pub outline: Vec<DVec2>,
    /// Top of the decking (mm).
    pub top: f64,
    /// Each plank (or the platform's one slab), in plan.
    pub planks: Vec<Vec<DVec2>>,
    /// Border boards round the edge.
    pub border: Vec<Vec<DVec2>>,
    pub members: Vec<Member>,
    /// Edges with a railing.
    pub rails: Vec<(DVec2, DVec2)>,
    /// Edges fastened to the house.
    pub ledgers: Vec<(DVec2, DVec2)>,
}

/// A strip `half` either side of the line `o + dir·t` for t in (t0, t1).
fn strip(o: DVec2, dir: DVec2, t0: f64, t1: f64, half: f64) -> Vec<DVec2> {
    let n = perp(dir) * half;
    let (a, b) = (o + dir * t0, o + dir * t1);
    vec![a - n, b - n, b + n, a + n]
}

/// Positions along `n` from `lo` to `hi` (inclusive ends) about `step` apart.
fn stations(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    let mut v = vec![lo];
    let mut k = 1;
    while lo + k as f64 * step < hi - step * 0.25 {
        v.push(lo + k as f64 * step);
        k += 1;
    }
    v.push(hi);
    v
}

pub fn parts(doc: &Document, deck: &Deck) -> Option<DeckParts> {
    let raw: Vec<DVec2> = deck.outline.iter().map(|p| v2(*p)).collect();
    if raw.len() < 3 {
        return None;
    }
    let mut outline = if deck.curved { spline_points(&raw, true) } else { raw };
    if signed_area(&outline) < 0.0 {
        outline.reverse();
    }
    let fe = doc.get::<Floor>(deck.floor).map_or(0.0, |f| f.elevation.mm());
    let top = fe + deck.height.mm();
    let ground = crate::site::ground(doc);
    let u = DVec2::from_angle(deck.plank_angle.to_radians());
    let n = perp(u);
    let pw = deck.plank_width.mm().max(20.0);
    let t = 1.5 * IN;
    let mut parts = DeckParts { outline: outline.clone(), top, ..Default::default() };
    // Edges along the house get a ledger; the rest may get a railing.
    let footprints = crate::rooms::detect(doc, deck.floor).footprints;
    let k = outline.len();
    for i in 0..k {
        let (a, b) = (outline[i], outline[(i + 1) % k]);
        let mid = (a + b) / 2.0;
        let on_house = footprints.iter().any(|f| distance_to_polyline(mid, f, true) < 200.0);
        if deck.ledger && on_house {
            parts.ledgers.push((a, b));
        } else if deck.railing && !on_house {
            parts.rails.push((a, b));
        }
    }
    // Decking.
    if deck.platform {
        parts.planks.push(outline.clone());
    } else {
        let field = if deck.border { inset_polygon(&outline, pw) } else { outline.clone() };
        if field.len() >= 3 {
            let proj: Vec<f64> = field.iter().map(|p| p.dot(n)).collect();
            let (lo, hi) = (proj.iter().copied().fold(f64::MAX, f64::min), proj.iter().copied().fold(f64::MIN, f64::max));
            let step = pw + deck.plank_gap.mm();
            let mut s = lo + pw / 2.0;
            while s < hi {
                let o = n * s;
                for (t0, t1) in clip_line(&field, o, u) {
                    parts.planks.push(strip(o, u, t0, t1, pw / 2.0));
                }
                s += step;
            }
        }
        if deck.border {
            let inner = inset_polygon(&outline, pw);
            if inner.len() == k {
                for i in 0..k {
                    let j = (i + 1) % k;
                    parts.border.push(vec![outline[i], outline[j], inner[j], inner[i]]);
                }
            }
        }
    }
    // Framing.
    let jd = deck.joist_depth.mm();
    let joist_z = top - deck.plank_thickness.mm() - jd / 2.0;
    let push = |parts: &mut DeckParts, role, a: DVec3, b: DVec3, width: f64, depth: f64, rotation: f64| {
        parts.members.push(Member { role, a, b, width, depth, rotation, host: deck.id, floor: deck.floor });
    };
    let rim = inset_polygon(&outline, t / 2.0);
    if rim.len() == k {
        for i in 0..k {
            let (a, b) = (rim[i], rim[(i + 1) % k]);
            let ledger = parts.ledgers.iter().any(|(x, y)| x.distance(outline[i]) < 1.0 && y.distance(outline[(i + 1) % k]) < 1.0);
            let role = if ledger { FramingRole::Ledger } else { FramingRole::RimJoist };
            push(&mut parts, role, a.extend(joist_z), b.extend(joist_z), t, jd, 0.0);
        }
    }
    let inner = inset_polygon(&outline, t);
    if inner.len() >= 3 {
        // Joists run across the planks.
        let proj: Vec<f64> = inner.iter().map(|p| p.dot(u)).collect();
        let (lo, hi) = (proj.iter().copied().fold(f64::MAX, f64::min), proj.iter().copied().fold(f64::MIN, f64::max));
        for s in stations(lo + t / 2.0, hi - t / 2.0, deck.joist_spacing.mm().max(100.0)) {
            let o = u * s;
            for (t0, t1) in clip_line(&inner, o, n) {
                if t1 - t0 > 100.0 {
                    push(&mut parts, FramingRole::FloorJoist, (o + n * t0).extend(joist_z), (o + n * t1).extend(joist_z), t, jd, 0.0);
                }
            }
        }
        // Blocking across the joists: mid-span, and every 8' on longer runs.
        if deck.blocking {
            let across: Vec<f64> = inner.iter().map(|p| p.dot(n)).collect();
            let (nlo, nhi) = (across.iter().copied().fold(f64::MAX, f64::min), across.iter().copied().fold(f64::MIN, f64::max));
            let rows = ((nhi - nlo) / (96.0 * IN)).ceil().max(2.0) as usize;
            for r in 1..rows {
                let at = nlo + (nhi - nlo) * r as f64 / rows as f64;
                let o = n * at;
                for (t0, t1) in clip_line(&inner, o, u) {
                    if t1 - t0 > 100.0 {
                        push(&mut parts, FramingRole::Blocking, (o + u * t0).extend(joist_z), (o + u * t1).extend(joist_z), t, jd, 0.0);
                    }
                }
            }
        }
    }
    // Beams run along the planks, under the joists: from the side the
    // joists bear on (the ledger, or an inset beam) across at most the
    // largest span.
    let proj: Vec<f64> = outline.iter().map(|p| p.dot(n)).collect();
    let (lo, hi) = (proj.iter().copied().fold(f64::MAX, f64::min), proj.iter().copied().fold(f64::MIN, f64::max));
    let inset = deck.beam_inset.mm();
    let ledger_side = parts
        .ledgers
        .iter()
        .map(|(a, b)| ((a + b) / 2.0).dot(n))
        .reduce(|x, y| (x + y) / 2.0);
    let (start, end) = match ledger_side {
        Some(l) if (l - lo).abs() < (l - hi).abs() => (lo, hi - inset),
        Some(_) => (hi, lo + inset),
        None => (lo + inset, hi - inset),
    };
    let span = deck.max_span.mm().max(1000.0);
    let count = (((end - start).abs() / span).ceil() as usize).max(1);
    let mut beam_at: Vec<f64> = (1..=count).map(|i| start + (end - start) * i as f64 / count as f64).collect();
    if ledger_side.is_none() {
        beam_at.insert(0, start);
    }
    let bd = 9.25 * IN;
    let beam_z = joist_z - jd / 2.0 - bd / 2.0;
    let post_half = 2.75 * IN;
    // Platforms sit on the floor inside a room: no beams or posts.
    if !deck.platform {
        for s in beam_at {
            let o = n * s;
            for (t0, t1) in clip_line(&outline, o, u) {
                let (a, b) = (o + u * (t0 + 75.0), o + u * (t1 - 75.0));
                if a.distance(b) < 300.0 {
                    continue;
                }
                parts.members.push(Member { role: FramingRole::Beam, a: a.extend(beam_z), b: b.extend(beam_z), width: 2.0 * t, depth: bd, rotation: 0.0, host: deck.id, floor: deck.floor });
                let len = a.distance(b);
                let posts = ((len / deck.post_spacing.mm().max(600.0)).ceil() as usize).max(1);
                for i in 0..=posts {
                    let p = a.lerp(b, (i as f64 / posts as f64).clamp(0.02, 0.98));
                    let g = ground(p);
                    let bottom = beam_z - bd / 2.0;
                    if bottom - g > 50.0 {
                        push(&mut parts, FramingRole::Post, p.extend(g), p.extend(bottom), 2.0 * post_half, 2.0 * post_half, 0.0);
                    }
                    push(&mut parts, FramingRole::Footing, p.extend(g - 450.0), p.extend(g + 50.0), 300.0, 300.0, 0.0);
                }
            }
        }
    }
    Some(parts)
}

fn look(c: Color, shade: f32) -> Look {
    let [r, g, b, _] = c.0;
    let f = |v: u8| v as f32 / 255.0 * shade;
    Look { color: [f(r), f(g), f(b), 1.0], pbr: [0.75, 0.0, 0.0, 1.0], surface: None, tex: None }
}

fn as_member(m: &Member) -> FramingMember {
    FramingMember {
        id: Id(0),
        floor: m.floor,
        layer: Id(0),
        role: m.role,
        a: housecraft_units::Point3::from_mm(m.a.x, m.a.y, m.a.z),
        b: housecraft_units::Point3::from_mm(m.b.x, m.b.y, m.b.z),
        width: housecraft_units::Length::from_mm(m.width),
        depth: housecraft_units::Length::from_mm(m.depth),
        rotation: m.rotation,
        material: FramingMaterial::Lumber,
        host: Some(m.host),
        manual: false,
        plies: 1,
        tail: TailCut::Square,
    }
}

pub fn deck_mesh(doc: &Document, deck: &Deck, mesh: &mut Mesh) {
    let Some(p) = parts(doc, deck) else {
        return;
    };
    let wood = mesh.painted_whole().unwrap_or(look(deck.color, 1.0));
    let dark = look(deck.color, 0.8);
    let concrete = look(Color::rgb(180, 178, 172), 1.0);
    let pt = deck.plank_thickness.mm();
    for plank in &p.planks {
        let mut q = plank.clone();
        if signed_area(&q) < 0.0 {
            q.reverse();
        }
        mesh.prism_look(&q, p.top - pt, p.top, wood);
    }
    for b in &p.border {
        let mut q = b.clone();
        if signed_area(&q) < 0.0 {
            q.reverse();
        }
        mesh.prism_look(&q, p.top - pt, p.top + 2.0, dark);
    }
    for m in &p.members {
        if m.role == FramingRole::Footing {
            let c = m.a.truncate();
            let disc: Vec<DVec2> = (0..10).map(|i| c + DVec2::from_angle(i as f64 / 10.0 * std::f64::consts::TAU) * m.width / 2.0).collect();
            mesh.prism_look(&disc, m.a.z, m.b.z, concrete);
        } else {
            crate::framing::member_mesh(&as_member(m), mesh);
        }
    }
    // Railings: posts at the ends and every 6', top and bottom rails and
    // balusters every 5".
    let rh = deck.railing_height.mm();
    for (a, b) in &p.rails {
        let len = a.distance(*b);
        let n = ((len / (72.0 * IN)).ceil() as usize).max(1);
        for i in 0..=n {
            let c = a.lerp(*b, i as f64 / n as f64);
            crate::site::post(c, 1.75 * IN, p.top, p.top + rh + 50.0, wood, mesh);
        }
        let d = (*b - *a).normalize_or_zero();
        let side = perp(d) * 1.75 * IN;
        for (z0, z1) in [(p.top + rh - 1.5 * IN, p.top + rh), (p.top + 3.0 * IN, p.top + 4.5 * IN)] {
            mesh.prism_look(&[*a - side, *b - side, *b + side, *a + side], z0, z1, wood);
        }
        let k = (len / (5.0 * IN)).floor() as usize;
        for i in 1..k {
            let c = a.lerp(*b, i as f64 / k as f64);
            crate::site::post(c, 0.75 * IN, p.top + 4.5 * IN, p.top + rh - 1.5 * IN, wood, mesh);
        }
    }
    if deck.covered {
        let roof_z = p.top + 2700.0;
        for c in &p.outline {
            crate::site::post(*c, 2.75 * IN, p.top, roof_z, wood, mesh);
        }
        let over = inset_polygon(&p.outline, -300.0);
        if over.len() >= 3 {
            mesh.prism_look(&over, roof_z, roof_z + 150.0, look(Color::rgb(110, 105, 100), 1.0));
        }
    }
}

/// Every deck (on `only_floor` when given).
pub fn build(doc: &Document, only_floor: Option<Id>, mesh: &mut Mesh) {
    for d in doc.iter::<Deck>() {
        if only_floor.is_some_and(|f| f != d.floor) {
            continue;
        }
        mesh.begin(d.id, Category::Floor);
        deck_mesh(doc, d, mesh);
        mesh.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_units::{Length, Point2};

    #[test]
    fn a_free_standing_deck_has_planks_joists_beams_posts_and_rails() {
        let doc = Document::new("t");
        let floor = doc.floors_by_elevation()[0].id;
        let p = |x: f64, y: f64| Point2(Length::from_feet(x), Length::from_feet(y));
        let deck = Deck::new(floor, Id(1), vec![p(0.0, 0.0), p(16.0, 0.0), p(16.0, 12.0), p(0.0, 12.0)], Length::from_feet(3.0));
        let parts = parts(&doc, &deck).unwrap();
        let count = |r| parts.members.iter().filter(|m| m.role == r).count();
        assert!(parts.planks.len() > 20, "{}", parts.planks.len());
        assert_eq!(parts.border.len(), 4);
        assert!(count(FramingRole::FloorJoist) >= 10);
        assert_eq!(count(FramingRole::RimJoist), 4);
        assert!(count(FramingRole::Beam) >= 2);
        assert!(count(FramingRole::Post) >= 4);
        assert_eq!(count(FramingRole::Post), count(FramingRole::Footing));
        assert_eq!(parts.rails.len(), 4, "free-standing: railing all round");
        assert!(parts.ledgers.is_empty());
    }
}
