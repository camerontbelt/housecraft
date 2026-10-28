//! Cabinet, countertop and appliance geometry: plan outlines, door and
//! drawer layout, and 3D meshes.
//!
//! Cabinet geometry is built in local coordinates — `u` along the back
//! from the cabinet's left to its right (as seen from the front), `v` from
//! the back toward the front, `z` up — then placed in the plan.

use glam::{DVec2, DVec3};
use housecraft_model::{
    Appliance, ApplianceKind, Symbol, SymbolDef, Cabinet, CabinetConstruction, CabinetKind, CabinetShape, Countertop,
    DoorStyleCab, Document, EdgeProfile, FaceItem, Floor, HardwareStyle, HoleKind, Id, MoldingPlace,
    Side, Surface,
};

use crate::math::{inset_polygon, minus_convex_holes, perp, point_in_polygon, signed_area, v2};
use housecraft_model::{CabinetAccessory, LegStyle};
use crate::mesh::Mesh;

const IN: f64 = 25.4;
/// Door and drawer front thickness.
const DOOR_T: f64 = 0.75 * IN;

/// A cabinet's or appliance's placement in the plan.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// Centre of the back edge.
    pub origin: DVec2,
    /// Left to right as seen from the front.
    pub along: DVec2,
    /// Toward the front.
    pub out: DVec2,
}

impl Frame {
    pub fn new(position: DVec2, facing_deg: f64) -> Self {
        let out = DVec2::from_angle(facing_deg.to_radians());
        Frame {
            origin: position,
            along: perp(out),
            out,
        }
    }

    pub fn world(&self, local: DVec2) -> DVec2 {
        self.origin + self.along * local.x + self.out * local.y
    }

    fn world3(&self, p: DVec3) -> DVec3 {
        self.world(p.truncate()).extend(p.z)
    }

    fn world_dir(&self, d: DVec3) -> DVec3 {
        (self.along * d.x + self.out * d.y).extend(d.z)
    }
}

fn ccw(mut poly: Vec<DVec2>) -> Vec<DVec2> {
    if signed_area(&poly) < 0.0 {
        poly.reverse();
    }
    poly
}

pub fn frame(c: &Cabinet) -> Frame {
    Frame::new(v2(c.position), c.facing)
}

/// Points of an arc in local coordinates (for bow and curved shapes).
fn arc(center: DVec2, r: f64, a0: f64, a1: f64, n: usize) -> Vec<DVec2> {
    (0..=n)
        .map(|i| center + DVec2::from_angle(a0 + (a1 - a0) * i as f64 / n as f64) * r)
        .collect()
}

/// The box outline in local coordinates, with its depth reduced by
/// `setback` at the front (for the toe kick), and the front edge from the
/// cabinet's left to its right.
fn local_shape(c: &Cabinet, setback: f64) -> (Vec<DVec2>, Vec<DVec2>) {
    let w = c.width.mm();
    let d = (c.depth.mm() - setback).max(1.0);
    let h = w / 2.0;
    match c.shape {
        CabinetShape::Standard => {
            let front = vec![DVec2::new(-h, d), DVec2::new(h, d)];
            let Some(corner) = c.corner.filter(|k| k.size.mm() > 1.0) else {
                return (vec![DVec2::new(-h, 0.0), DVec2::new(h, 0.0), front[1], front[0]], front);
            };
            // Front corners cut back: a quarter circle or a straight chamfer.
            let r = corner.size.mm().min(h).min(d);
            let mut outline = vec![DVec2::new(-h, 0.0), DVec2::new(h, 0.0)];
            let pi = std::f64::consts::PI;
            if corner.right {
                if corner.round {
                    outline.extend(arc(DVec2::new(h - r, d - r), r, 0.0, pi / 2.0, 8));
                } else {
                    outline.extend([DVec2::new(h, d - r), DVec2::new(h - r, d)]);
                }
            } else {
                outline.push(front[1]);
            }
            if corner.left {
                if corner.round {
                    outline.extend(arc(DVec2::new(-h + r, d - r), r, pi / 2.0, pi, 8));
                } else {
                    outline.extend([DVec2::new(-h + r, d), DVec2::new(-h, d - r)]);
                }
            } else {
                outline.push(front[0]);
            }
            let front = vec![DVec2::new(-h + if corner.left { r } else { 0.0 }, d), DVec2::new(h - if corner.right { r } else { 0.0 }, d)];
            (outline, front)
        }
        CabinetShape::Angled { left, right } => {
            let (l, r) = (left.mm().min(d - 1.0), right.mm().min(d - 1.0));
            let front = vec![DVec2::new(-h + l, d), DVec2::new(h - r, d)];
            (
                vec![
                    DVec2::new(-h, 0.0),
                    DVec2::new(h, 0.0),
                    DVec2::new(h, d - r),
                    front[1],
                    front[0],
                    DVec2::new(-h, d - l),
                ],
                front,
            )
        }
        CabinetShape::Bow { depth } => {
            // A circular arc through both front corners, `b` proud at the middle.
            let b = depth.mm().max(1.0);
            let r = (h * h + b * b) / (2.0 * b);
            let center = DVec2::new(0.0, d + b - r);
            let half = (h / r).asin();
            let pi2 = std::f64::consts::FRAC_PI_2;
            // From the left corner (angle pi/2 + half) to the right (pi/2 - half).
            let front = arc(center, r, pi2 + half, pi2 - half, 16);
            let mut outline = vec![DVec2::new(-h, 0.0), DVec2::new(h, 0.0)];
            outline.extend(front.iter().rev());
            (outline, front)
        }
        CabinetShape::Curved { radius } => {
            let r = radius.mm();
            let pi2 = std::f64::consts::FRAC_PI_2;
            if r > 0.0 {
                // Convex front: the centre is behind the back.
                let center = DVec2::new(0.0, -r);
                let half = h / r;
                let back = arc(center, r, pi2 - half, pi2 + half, 16);
                let front = arc(center, r + d, pi2 + half, pi2 - half, 16);
                let mut outline = back.clone();
                outline.extend(front.iter());
                (outline, front)
            } else {
                // Concave front: the centre is in front.
                let r = -r;
                let center = DVec2::new(0.0, r);
                let half = h / r;
                let back = arc(center, r, -pi2 - half, -pi2 + half, 16);
                let front = arc(center, (r - d).max(1.0), -pi2 - half, -pi2 + half, 16);
                let mut outline = back.clone();
                outline.extend(front.iter().rev());
                (outline, front)
            }
        }
    }
}

/// The cabinet box outline in plan (counter-clockwise), without fillers.
pub fn outline(c: &Cabinet) -> Vec<DVec2> {
    let f = frame(c);
    ccw(local_shape(c, 0.0).0.into_iter().map(|p| f.world(p)).collect())
}

/// Outline including fillers, for plan drawing and snapping.
pub fn outline_with_fillers(c: &Cabinet) -> Vec<DVec2> {
    if c.filler_left.mm() <= 0.0 && c.filler_right.mm() <= 0.0 {
        return outline(c);
    }
    let f = frame(c);
    let (h, d) = (c.width.mm() / 2.0, c.depth.mm());
    let (l, r) = (c.filler_left.mm(), c.filler_right.mm());
    match c.shape {
        CabinetShape::Standard => ccw(vec![
            f.world(DVec2::new(-h - l, 0.0)),
            f.world(DVec2::new(h + r, 0.0)),
            f.world(DVec2::new(h + r, d)),
            f.world(DVec2::new(-h - l, d)),
        ]),
        _ => outline(c),
    }
}

/// The front edge in plan, from the cabinet's left to its right.
pub fn front_line(c: &Cabinet) -> Vec<DVec2> {
    let f = frame(c);
    local_shape(c, 0.0).1.into_iter().map(|p| f.world(p)).collect()
}

/// The cabinet's own countertop outline in plan: the box plus fillers,
/// pushed forward by the overhang.
pub fn countertop_outline(c: &Cabinet) -> Option<Vec<DVec2>> {
    let top = c.countertop.as_ref()?;
    let f = frame(c);
    let o = top.overhang.mm();
    let (shape, _) = local_shape(c, -o);
    let (l, r) = (c.filler_left.mm() + top.side_overhang.mm(), c.filler_right.mm() + top.side_overhang.mm());
    let back = top.back_overhang.mm();
    let h = c.width.mm() / 2.0;
    let local: Vec<DVec2> = shape
        .into_iter()
        .map(|p| {
            // Out over the back when asked.
            let p = if p.y.abs() < 1e-6 { DVec2::new(p.x, -back) } else { p };
            // Widen straight ends to cover fillers and the side overhang.
            if (p.x + h).abs() < 1e-6 {
                DVec2::new(p.x - l, p.y)
            } else if (p.x - h).abs() < 1e-6 {
                DVec2::new(p.x + r, p.y)
            } else {
                p
            }
        })
        .collect();
    let local = round_front_corners(local, top, c.shape == CabinetShape::Standard);
    Some(ccw(local.into_iter().map(|p| f.world(p)).collect()))
}

/// Round or chamfer the countertop's front corners (standard cabinets:
/// points 2 = front right and 3 = front left).
fn round_front_corners(pts: Vec<DVec2>, top: &housecraft_model::CabinetTop, standard: bool) -> Vec<DVec2> {
    use housecraft_model::CornerTreatment as T;
    if !standard || pts.len() != 4 || top.corner == T::Square {
        return pts;
    }
    let mut out = Vec::new();
    for (i, p) in pts.iter().enumerate() {
        let on = (i == 2 && top.corner_right) || (i == 3 && top.corner_left);
        if !on {
            out.push(*p);
            continue;
        }
        let prev = pts[(i + 3) % 4];
        let next = pts[(i + 1) % 4];
        let (a, bb) = ((prev - *p).normalize_or_zero(), (next - *p).normalize_or_zero());
        match top.corner {
            T::Chamfer { size } => {
                let k = size.mm().max(0.0);
                out.push(*p + a * k);
                out.push(*p + bb * k);
            }
            T::Round { radius } => {
                let r = radius.mm().max(0.0);
                let center = *p + (a + bb) * r;
                let start = (*p + a * r - center).to_angle();
                let end = (*p + bb * r - center).to_angle();
                let mut sweep = end - start;
                while sweep > std::f64::consts::PI {
                    sweep -= std::f64::consts::TAU;
                }
                while sweep < -std::f64::consts::PI {
                    sweep += std::f64::consts::TAU;
                }
                for k in 0..=6 {
                    out.push(center + DVec2::from_angle(start + sweep * k as f64 / 6.0) * r);
                }
            }
            T::Square => out.push(*p),
        }
    }
    out
}

/// The sink cut-out in a sink cabinet's countertop (plan, counter-clockwise).
pub fn sink_hole(c: &Cabinet) -> Option<Vec<DVec2>> {
    if !c.sink || c.countertop.is_none() {
        return None;
    }
    let f = frame(c);
    let w = (c.width.mm() - 6.0 * IN).clamp(12.0 * IN, 33.0 * IN);
    let d = (c.depth.mm() - 6.0 * IN).clamp(10.0 * IN, 19.0 * IN);
    let center = DVec2::new(0.0, c.depth.mm() / 2.0 + IN);
    Some(ccw(
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .iter()
            .map(|(a, b)| f.world(center + DVec2::new(a * w / 2.0, b * d / 2.0)))
            .collect(),
    ))
}

/// One door, drawer or opening on a cabinet front: the door or drawer
/// face (mm along the front from the left end, and mm above the cabinet
/// bottom) and the opening in the box behind it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub item: FaceItem,
    pub s0: f64,
    pub s1: f64,
    pub z0: f64,
    pub z1: f64,
    /// For doors: the hinge is at s0 (left) rather than s1.
    pub hinge_left: bool,
    /// The opening: s0, s1, z0, z1.
    pub opening: [f64; 4],
    /// Which face row (top first).
    pub row: usize,
}

fn length(pts: &[DVec2]) -> f64 {
    pts.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Face-frame stile/rail width and the gap between openings.
fn frame_sizes(c: &Cabinet) -> (f64, f64) {
    match c.construction {
        CabinetConstruction::Framed => (1.5 * IN, 1.0 * IN),
        CabinetConstruction::Frameless => {
            let r = c.build.reveal.mm().max(0.0);
            (r / 2.0, r)
        }
    }
}

/// Height of the box bottom above the cabinet's base (the toe kick or legs).
pub fn base_height(c: &Cabinet) -> f64 {
    match c.cabinet_kind {
        CabinetKind::Wall => 0.0,
        _ => c.toe_kick.map_or(0.0, |(h, _)| h.mm()),
    }
}

/// Lay out the cabinet front: rows top to bottom, cells left to right. A
/// blind corner's hidden part becomes a fixed panel.
pub fn cells(c: &Cabinet) -> Vec<Cell> {
    let front = local_shape(c, 0.0).1;
    let len = length(&front);
    let (edge, gap) = frame_sizes(c);
    let bottom = base_height(c) + edge;
    let top = c.height.mm() - edge;
    let rows = &c.face;
    if rows.is_empty() || top <= bottom {
        return Vec::new();
    }
    let mut out = Vec::new();
    // The blind part of the front.
    let (mut start, mut end) = (edge, len - edge);
    if let Some(b) = c.blind {
        let w = b.width.mm().clamp(0.0, len - 2.0 * edge - 4.0 * IN);
        let panel = match b.side {
            Side::Left => {
                start = edge + w + gap;
                (edge, edge + w)
            }
            Side::Right => {
                end = len - edge - w - gap;
                (len - edge - w, len - edge)
            }
        };
        out.push(Cell {
            item: FaceItem::Panel,
            s0: panel.0,
            s1: panel.1,
            z0: bottom,
            z1: top,
            hinge_left: false,
            opening: [panel.0, panel.1, bottom, top],
            row: 0,
        });
    }
    let fixed: f64 = rows.iter().filter_map(|r| r.height).map(|h| h.mm()).sum();
    let flexible = rows.iter().filter(|r| r.height.is_none()).count();
    let gaps = gap * (rows.len() - 1) as f64;
    let share = if flexible > 0 {
        ((top - bottom - gaps - fixed) / flexible as f64).max(0.0)
    } else {
        0.0
    };
    let framed = c.construction == CabinetConstruction::Framed;
    let [ol, or, ot, ob] = c.build.overlay.map(|l| l.mm().max(0.0));
    // Doors overlap shared rails and mullions by at most half the gap.
    let inner = (gap / 2.0 - 0.0625 * IN).max(0.0);
    let nrows = rows.len();
    let mut z = top;
    for (r, row) in rows.iter().enumerate() {
        let h = row.height.map_or(share, |h| h.mm());
        let (z0, z1) = ((z - h).max(bottom), z);
        z = z0 - gap;
        let n = row.items.len().max(1);
        let span = end - start;
        let cw = (span - gap * (n as f64 - 1.0)) / n as f64;
        for (k, item) in row.items.iter().enumerate() {
            let o0 = start + k as f64 * (cw + gap);
            let o1 = o0 + cw;
            let opening = [o0, o1, z0, z1];
            // The face: framed doors overlay the frame around the opening.
            let (mut f0, mut f1, mut g0, mut g1) = (o0, o1, z0, z1);
            if framed {
                f0 -= if k == 0 { ol } else { ol.min(inner) };
                f1 += if k == n - 1 { or } else { or.min(inner) };
                g1 += if r == 0 { ot } else { ot.min(inner) };
                g0 -= if r == nrows - 1 { ob } else { ob.min(inner) };
            }
            if *item == FaceItem::DoubleDoor {
                let mid = (f0 + f1) / 2.0;
                let g = 0.0625 * IN;
                out.push(Cell { item: FaceItem::DoorLeft, s0: f0, s1: mid - g, z0: g0, z1: g1, hinge_left: true, opening, row: r });
                out.push(Cell { item: FaceItem::DoorRight, s0: mid + g, s1: f1, z0: g0, z1: g1, hinge_left: false, opening, row: r });
            } else {
                let hinge_left = matches!(item, FaceItem::DoorLeft | FaceItem::GlassDoor);
                out.push(Cell { item: *item, s0: f0, s1: f1, z0: g0, z1: g1, hinge_left, opening, row: r });
            }
        }
    }
    out
}

/// Face sizes: (width, height) in mm.
pub type Sizes = Vec<(f64, f64)>;

/// Door and drawer face sizes for reports.
pub fn front_sizes(c: &Cabinet) -> (Sizes, Sizes) {
    let mut doors = Vec::new();
    let mut drawers = Vec::new();
    for cell in cells(c) {
        let size = (cell.s1 - cell.s0, cell.z1 - cell.z0);
        match cell.item {
            FaceItem::Drawer => drawers.push(size),
            FaceItem::DoorLeft | FaceItem::DoorRight | FaceItem::GlassDoor => doors.push(size),
            _ => {}
        }
    }
    (doors, drawers)
}

/// The point and outward normal at distance `s` along a polyline (local).
fn at(front: &[DVec2], s: f64) -> (DVec2, DVec2) {
    let mut rest = s;
    let last = front.len().saturating_sub(2);
    for (i, w) in front.windows(2).enumerate() {
        let l = w[0].distance(w[1]);
        if rest <= l || i == last {
            let t = (rest / l.max(1e-9)).clamp(0.0, 1.0);
            let dir = (w[1] - w[0]).normalize_or(DVec2::X);
            return (w[0].lerp(w[1], t), perp(dir));
        }
        rest -= l;
    }
    (front[0], DVec2::Y)
}

/// The front between `s0` and `s1`, with every corner in between.
fn sub_front(front: &[DVec2], s0: f64, s1: f64) -> Vec<(DVec2, DVec2)> {
    let mut pts = vec![at(front, s0)];
    let mut acc = 0.0;
    for i in 1..front.len().saturating_sub(1) {
        acc += front[i - 1].distance(front[i]);
        if acc > s0 && acc < s1 {
            let n0 = perp((front[i] - front[i - 1]).normalize_or(DVec2::X));
            let n1 = perp((front[i + 1] - front[i]).normalize_or(DVec2::X));
            pts.push((front[i], (n0 + n1).normalize_or(n0)));
        }
    }
    pts.push(at(front, s1));
    pts
}

/// Adds faces to the mesh in local coordinates, each turned to face `want`.
struct Builder<'a> {
    f: Frame,
    z0: f64,
    mesh: &'a mut Mesh,
    /// A local move applied before placing (an opening door or drawer):
    /// rotate by the angle about the pivot, then shift.
    xf: Option<(DVec2, f64, DVec2)>,
}

impl Builder<'_> {
    fn local(&self, p: DVec3) -> DVec3 {
        match self.xf {
            Some((pivot, angle, shift)) => {
                let q = DVec2::from_angle(angle).rotate(p.truncate() - pivot) + pivot + shift;
                q.extend(p.z)
            }
            None => p,
        }
    }

    fn local_dir(&self, d: DVec3) -> DVec3 {
        match self.xf {
            Some((_, angle, _)) => DVec2::from_angle(angle).rotate(d.truncate()).extend(d.z),
            None => d,
        }
    }

    fn face(&mut self, pts: &[DVec3], want: DVec3, surface: Surface) {
        let mut w: Vec<DVec3> = pts
            .iter()
            .map(|p| self.f.world3(self.local(*p) + DVec3::Z * self.z0))
            .collect();
        let n = crate::roof::newell(&w);
        if n.dot(self.f.world_dir(self.local_dir(want))) < 0.0 {
            w.reverse();
        }
        self.mesh.polygon(&w, surface);
    }

    /// A slab following the front from `s0` to `s1`, `off` to `off + t`
    /// in front of it, between heights `z0` and `z1`.
    #[allow(clippy::too_many_arguments)]
    fn slab(&mut self, front: &[DVec2], s0: f64, s1: f64, z0: f64, z1: f64, off: f64, t: f64, surface: Surface) {
        if s1 - s0 < 1.0 || z1 - z0 < 1.0 {
            return;
        }
        let pts = sub_front(front, s0, s1);
        let inner: Vec<DVec2> = pts.iter().map(|(p, n)| *p + *n * off).collect();
        let outer: Vec<DVec2> = pts.iter().map(|(p, n)| *p + *n * (off + t)).collect();
        let e = |p: DVec2, z: f64| p.extend(z);
        for i in 0..pts.len() - 1 {
            let n = pts[i].1 + pts[i + 1].1;
            let nn = n.extend(0.0);
            self.face(
                &[e(outer[i], z0), e(outer[i + 1], z0), e(outer[i + 1], z1), e(outer[i], z1)],
                nn,
                surface,
            );
            // Top and bottom edges.
            self.face(&[e(inner[i], z1), e(inner[i + 1], z1), e(outer[i + 1], z1), e(outer[i], z1)], DVec3::Z, surface);
            self.face(&[e(inner[i], z0), e(inner[i + 1], z0), e(outer[i + 1], z0), e(outer[i], z0)], -DVec3::Z, surface);
        }
        // Ends.
        let k = pts.len() - 1;
        let t0 = -(pts[1].0 - pts[0].0).normalize_or(DVec2::X).extend(0.0);
        let t1 = (pts[k].0 - pts[k - 1].0).normalize_or(DVec2::X).extend(0.0);
        self.face(&[e(inner[0], z0), e(outer[0], z0), e(outer[0], z1), e(inner[0], z1)], t0, surface);
        self.face(&[e(inner[k], z0), e(outer[k], z0), e(outer[k], z1), e(inner[k], z1)], t1, surface);
    }

    /// A small box standing off the front at (s, z): `w` along, `h` up.
    #[allow(clippy::too_many_arguments)]
    fn knob(&mut self, front: &[DVec2], s: f64, z: f64, w: f64, h: f64, off: f64, proud: f64) {
        let (p, n) = at(front, s);
        let t = DVec2::new(n.y, -n.x);
        let c = p + n * off;
        let base = [
            c - t * w / 2.0,
            c + t * w / 2.0,
            c + t * w / 2.0 + n * proud,
            c - t * w / 2.0 + n * proud,
        ];
        let e = |q: DVec2, zz: f64| q.extend(zz);
        let (za, zb) = (z - h / 2.0, z + h / 2.0);
        for i in 0..4 {
            let (a, b) = (base[i], base[(i + 1) % 4]);
            let mid = (a + b) / 2.0 - c - n * proud / 2.0;
            self.face(&[e(a, za), e(b, za), e(b, zb), e(a, zb)], mid.extend(0.0), Surface::Hardware);
        }
        self.face(&base.map(|q| e(q, zb)), DVec3::Z, Surface::Hardware);
        self.face(&base.map(|q| e(q, za)), -DVec3::Z, Surface::Hardware);
    }

    /// A prism of a local outline between two heights.
    fn prism(&mut self, outline: &[DVec2], z0: f64, z1: f64, surface: Surface) {
        let world: Vec<DVec2> = outline.iter().map(|p| self.f.world(*p)).collect();
        self.mesh
            .prism(&ccw(world), self.z0 + z0, self.z0 + z1, surface);
    }
}

/// A door or drawer front in the cabinet's style.
fn front_panel(b: &mut Builder, front: &[DVec2], cell: &Cell, style: DoorStyleCab, glass: bool) {
    let (s0, s1, z0, z1) = (cell.s0, cell.s1, cell.z0, cell.z1);
    let rail = (2.5 * IN).min((s1 - s0) / 4.0).min((z1 - z0) / 4.0);
    if glass {
        b.slab(front, s0, s1, z0, z0 + rail, 0.0, DOOR_T, Surface::CabinetDoor);
        b.slab(front, s0, s1, z1 - rail, z1, 0.0, DOOR_T, Surface::CabinetDoor);
        b.slab(front, s0, s0 + rail, z0 + rail, z1 - rail, 0.0, DOOR_T, Surface::CabinetDoor);
        b.slab(front, s1 - rail, s1, z0 + rail, z1 - rail, 0.0, DOOR_T, Surface::CabinetDoor);
        b.slab(front, s0 + rail, s1 - rail, z0 + rail, z1 - rail, DOOR_T * 0.3, DOOR_T * 0.2, Surface::Glass);
        return;
    }
    match style {
        DoorStyleCab::Slab => b.slab(front, s0, s1, z0, z1, 0.0, DOOR_T, Surface::CabinetDoor),
        DoorStyleCab::Shaker => {
            b.slab(front, s0, s1, z0, z1, 0.0, DOOR_T * 0.6, Surface::CabinetDoor);
            let t = DOOR_T * 0.4;
            let o = DOOR_T * 0.6;
            b.slab(front, s0, s1, z0, z0 + rail, o, t, Surface::CabinetDoor);
            b.slab(front, s0, s1, z1 - rail, z1, o, t, Surface::CabinetDoor);
            b.slab(front, s0, s0 + rail, z0 + rail, z1 - rail, o, t, Surface::CabinetDoor);
            b.slab(front, s1 - rail, s1, z0 + rail, z1 - rail, o, t, Surface::CabinetDoor);
        }
        DoorStyleCab::Raised => {
            b.slab(front, s0, s1, z0, z1, 0.0, DOOR_T, Surface::CabinetDoor);
            b.slab(front, s0 + rail, s1 - rail, z0 + rail, z1 - rail, DOOR_T, DOOR_T * 0.35, Surface::CabinetDoor);
        }
    }
}

/// Where a door's or drawer's pull goes: (s, z, vertical).
pub fn hardware_spot(c: &Cabinet, cell: &Cell) -> Option<(f64, f64, bool)> {
    let inset = 2.0 * IN;
    match cell.item {
        FaceItem::Drawer => Some(((cell.s0 + cell.s1) / 2.0, (cell.z0 + cell.z1) / 2.0, false)),
        FaceItem::DoorLeft | FaceItem::DoorRight | FaceItem::GlassDoor => {
            let s = if cell.hinge_left { cell.s1 - inset } else { cell.s0 + inset };
            // Pulls sit near the edge closest to the hand: top of base doors,
            // bottom of wall doors; tall doors at counter height.
            let e = c.handle_from_edge.map_or(3.0 * IN, |l| l.mm()).clamp(IN, (cell.z1 - cell.z0) / 2.0);
            let z = match c.cabinet_kind {
                CabinetKind::Base => cell.z1 - e,
                CabinetKind::Wall => cell.z0 + e,
                CabinetKind::Full => {
                    let counter = 36.0 * IN - c.elevation.mm();
                    counter.clamp(cell.z0 + 3.0 * IN, cell.z1 - 3.0 * IN)
                }
            };
            Some((s, z, true))
        }
        FaceItem::DoubleDoor | FaceItem::Open | FaceItem::Panel | FaceItem::Appliance => None,
    }
}

/// A countertop slab with its edge profile: the top surface is inset by
/// the profile's size so the edge reads as eased, bevelled or rounded.
fn counter_slab(mesh: &mut Mesh, outline: &[DVec2], holes: &[Vec<DVec2>], z0: f64, z1: f64, edge: EdgeProfile) {
    let t = z1 - z0;
    let e = match edge {
        EdgeProfile::Square => 0.0,
        EdgeProfile::Eased => 0.125 * IN,
        EdgeProfile::Bevel => 0.375 * IN,
        EdgeProfile::Bullnose => t / 2.0,
        EdgeProfile::Ogee => 0.375 * IN,
    }
    .min(t * 0.9);
    let pieces = if holes.is_empty() {
        vec![outline.to_vec()]
    } else {
        minus_convex_holes(outline, holes)
    };
    if e < 0.5 || !holes.is_empty() {
        for p in pieces {
            mesh.prism(&ccw(p), z0, z1, Surface::Countertop);
        }
        return;
    }
    let outline = ccw(outline.to_vec());
    let top = inset_polygon(&outline, e);
    if top.len() != outline.len() {
        mesh.prism(&outline, z0, z1, Surface::Countertop);
        return;
    }
    let zs = z1 - e;
    let at = |p: DVec2, z: f64| p.extend(z);
    mesh.polygon(&top.iter().map(|p| at(*p, z1)).collect::<Vec<_>>(), Surface::Countertop);
    mesh.polygon(&outline.iter().rev().map(|p| at(*p, z0)).collect::<Vec<_>>(), Surface::Countertop);
    let n = outline.len();
    for i in 0..n {
        let (a, b) = (outline[i], outline[(i + 1) % n]);
        let (ta, tb) = (top[i], top[(i + 1) % n]);
        mesh.polygon(&[at(a, z0), at(b, z0), at(b, zs), at(a, zs)], Surface::Countertop);
        if edge == EdgeProfile::Bullnose || edge == EdgeProfile::Ogee {
            // Round over in three steps.
            let mut prev = (a, b, zs);
            for k in 1..=3 {
                let ang = k as f64 / 3.0 * std::f64::consts::FRAC_PI_2;
                let inward = 1.0 - ang.cos();
                let up = ang.sin();
                let pa = a.lerp(ta, inward);
                let pb = b.lerp(tb, inward);
                let z = zs + e * up;
                mesh.polygon(&[at(prev.0, prev.2), at(prev.1, prev.2), at(pb, z), at(pa, z)], Surface::Countertop);
                prev = (pa, pb, z);
            }
        } else {
            mesh.polygon(&[at(a, zs), at(b, zs), at(tb, z1), at(ta, z1)], Surface::Countertop);
        }
    }
}

/// A box in local coordinates: u, v and z ranges.
fn block(b: &mut Builder, u: (f64, f64), v: (f64, f64), z: (f64, f64), surface: Surface) {
    if u.1 - u.0 < 0.1 || v.1 - v.0 < 0.1 || z.1 - z.0 < 0.1 {
        return;
    }
    let pts = [
        DVec2::new(u.0, v.0),
        DVec2::new(u.1, v.0),
        DVec2::new(u.1, v.1),
        DVec2::new(u.0, v.1),
    ];
    b.prism(&pts, z.0, z.1, surface);
}

/// The 3D cabinet: box (panels for standard cabinets, so open doors show
/// shelves), face frame, toe kick or legs, end panels, fillers, doors and
/// drawers (opened by `open`), hardware, moldings, countertop with sink,
/// waterfall ends and backsplash.
pub fn cabinet_mesh(doc: &Document, c: &Cabinet, mesh: &mut Mesh) {
    let Some(floor) = doc.get::<Floor>(c.floor) else {
        return;
    };
    let z0 = floor.elevation.mm() + c.elevation.mm();
    let h = c.height.mm();
    let d = c.depth.mm();
    let half = c.width.mm() / 2.0;
    let mut b = Builder { f: frame(c), z0, mesh, xf: None };
    let (shape, front) = local_shape(c, 0.0);
    let base = base_height(c);
    let toe = match (c.cabinet_kind, c.toe_kick) {
        (CabinetKind::Wall, _) | (_, None) => None,
        (_, Some((th, td))) => Some((th.mm(), td.mm())),
    };
    let build = c.build;
    let standard = c.shape == CabinetShape::Standard && c.corner.is_none_or(|k| k.size.mm() <= 1.0);
    let (edge, gap) = frame_sizes(c);
    let all = cells(c);
    if standard {
        let t = build.side_thickness.mm().max(1.0);
        let bt = build.back_thickness.mm().max(1.0);
        let (ul, ur) = (-half, half);
        if build.left {
            block(&mut b, (ul, ul + t), (0.0, d), (base, h), Surface::CabinetBox);
        }
        if build.right {
            block(&mut b, (ur - t, ur), (0.0, d), (base, h), Surface::CabinetBox);
        }
        let (il, ir) = (ul + if build.left { t } else { 0.0 }, ur - if build.right { t } else { 0.0 });
        if build.top {
            block(&mut b, (il, ir), (0.0, d), (h - t, h), Surface::CabinetBox);
        }
        if build.bottom {
            block(&mut b, (il, ir), (0.0, d), (base, base + t), Surface::CabinetBox);
        }
        if build.back {
            block(&mut b, (il, ir), (0.0, bt), (base, h), Surface::CabinetBox);
        }
        // Shelves behind doors and in open bays.
        for cell in &all {
            if !matches!(cell.item, FaceItem::DoorLeft | FaceItem::DoorRight | FaceItem::GlassDoor | FaceItem::Open) {
                continue;
            }
            // Each opening once (double doors share one).
            if cell.item == FaceItem::DoorRight
                && all.iter().any(|o| o.opening == cell.opening && o.item == FaceItem::DoorLeft)
            {
                continue;
            }
            let n = c.face.get(cell.row).and_then(|r| r.shelves).unwrap_or(c.shelves);
            let [o0, o1, oz0, oz1] = cell.opening;
            for k in 1..=n {
                let z = oz0 + (oz1 - oz0) * k as f64 / (n + 1) as f64;
                block(
                    &mut b,
                    (ul + o0.max(t), ul + o1.min(2.0 * half - t)),
                    (bt, d - IN),
                    (z - 0.375 * IN, z + 0.375 * IN),
                    Surface::CabinetBox,
                );
            }
        }
        // Fittings in the first door opening.
        if let Some(cell) = all.iter().find(|x| matches!(x.item, FaceItem::DoorLeft | FaceItem::DoorRight | FaceItem::GlassDoor | FaceItem::Open)) {
            let [o0, o1, oz0, oz1] = cell.opening;
            let (u0, u1) = (ul + o0.max(t) + 0.5 * IN, ul + o1.min(2.0 * half - t) - 0.5 * IN);
            let (v0, v1) = (bt + 0.5 * IN, d - 1.5 * IN);
            for acc in &c.accessories {
                match acc {
                    CabinetAccessory::TrashPullout => {
                        let mid = (u0 + u1) / 2.0;
                        let top = (oz0 + 20.0 * IN).min(oz1 - IN);
                        block(&mut b, (u0, u1), (v0, v1), (oz0 + 0.5 * IN, oz0 + 1.0 * IN), Surface::CabinetBox);
                        for (a, z) in [(u0 + 0.25 * IN, mid - 0.25 * IN), (mid + 0.25 * IN, u1 - 0.25 * IN)] {
                            block(&mut b, (a, z), (v0 + IN, v1 - IN), (oz0 + IN, top), Surface::Hardware);
                        }
                    }
                    CabinetAccessory::LazySusan => {
                        let c0 = DVec2::new((u0 + u1) / 2.0, (v0 + v1) / 2.0);
                        let r = ((u1 - u0).min(v1 - v0) / 2.0 - 0.5 * IN).max(2.0 * IN);
                        let disc: Vec<DVec2> = arc(c0, r, 0.0, std::f64::consts::TAU, 24).into_iter().take(24).collect();
                        for f in [0.1, 0.55] {
                            let z = oz0 + (oz1 - oz0) * f;
                            b.prism(&disc, z, z + 0.75 * IN, Surface::CabinetBox);
                        }
                        let pole: Vec<DVec2> = arc(c0, 0.5 * IN, 0.0, std::f64::consts::TAU, 8).into_iter().take(8).collect();
                        b.prism(&pole, oz0, oz1 - 0.5 * IN, Surface::Hardware);
                    }
                    CabinetAccessory::RollOuts => {
                        for k in 0..3 {
                            let z = oz0 + 0.5 * IN + (oz1 - oz0) * k as f64 / 3.0;
                            block(&mut b, (u0, u1), (v0, v1), (z, z + 0.5 * IN), Surface::CabinetBox);
                            block(&mut b, (u0, u1), (v1 - 0.5 * IN, v1), (z, z + 3.0 * IN), Surface::CabinetBox);
                        }
                    }
                    CabinetAccessory::TrayDividers => {
                        let mut u = u0 + 3.0 * IN;
                        while u < u1 - IN {
                            block(&mut b, (u - 0.125 * IN, u + 0.125 * IN), (v0, v1), (oz0 + 0.5 * IN, oz1 - IN), Surface::CabinetBox);
                            u += 3.0 * IN;
                        }
                    }
                }
            }
        }
        // Toe kick board, set back.
        if let Some((th, td)) = toe
            && !c.ends.legs
        {
            block(&mut b, (ul, ur), (d - td - 0.5 * IN, d - td), (0.0, th), Surface::CabinetBox);
        }
    } else {
        match toe {
            Some((th, td)) if th > 0.0 && !c.ends.legs => {
                b.prism(&local_shape(c, td).0, 0.0, th, Surface::CabinetBox);
                b.prism(&shape, th, h, Surface::CabinetBox);
            }
            _ => b.prism(&shape, base, h, Surface::CabinetBox),
        }
    }
    // Legs at the corners instead of a toe kick.
    if c.ends.legs && base > 0.0 {
        let s = 2.5 * IN;
        for (u, v) in [(-half, 0.0), (half - s, 0.0), (-half, d - s), (half - s, d - s)] {
            let mid = DVec2::new(u + s / 2.0, v + s / 2.0);
            let ring = |r: f64| -> Vec<DVec2> { arc(mid, r, 0.0, std::f64::consts::TAU, 16).into_iter().take(16).collect() };
            match c.ends.leg_style {
                LegStyle::Square => block(&mut b, (u, u + s), (v, v + s), (0.0, base), Surface::CabinetBox),
                // A turned leg: wider collar at the top, slimmer shaft.
                LegStyle::Round => {
                    b.prism(&ring(s * 0.35), 0.0, base * 0.8, Surface::CabinetBox);
                    b.prism(&ring(s * 0.5), base * 0.8, base, Surface::CabinetBox);
                }
                // Bun feet: a squat round foot.
                LegStyle::Bun => {
                    b.prism(&ring(s * 0.45), 0.0, base * 0.25, Surface::CabinetBox);
                    b.prism(&ring(s * 0.62), base * 0.25, base * 0.75, Surface::CabinetBox);
                    b.prism(&ring(s * 0.45), base * 0.75, base, Surface::CabinetBox);
                }
            }
        }
    }
    // Pilasters: columns proud of the front corners, with a base and cap.
    if c.ends.pilasters {
        let pw = 2.5 * IN;
        for (u0, u1) in [(-half, -half + pw), (half - pw, half)] {
            block(&mut b, (u0, u1), (d, d + 0.75 * IN), (base, h), Surface::CabinetBox);
            block(&mut b, (u0 - 0.25 * IN, u1 + 0.25 * IN), (d, d + 1.0 * IN), (base, base + 3.0 * IN), Surface::CabinetBox);
            block(&mut b, (u0 - 0.25 * IN, u1 + 0.25 * IN), (d, d + 1.0 * IN), (h - 2.0 * IN, h), Surface::CabinetBox);
        }
    }
    // Face frame: stiles, rails, mullions (framed cabinets).
    if c.construction == CabinetConstruction::Framed {
        let len = length(&front);
        let ft = 0.75 * IN;
        // Extended stiles stop at the box where another cabinet butts up.
        let neighbour = |u: f64| {
            let p = b.f.world(DVec2::new(u, d / 2.0));
            doc.iter::<Cabinet>().any(|o| o.id != c.id && o.floor == c.floor && o.cabinet_kind != CabinetKind::Wall && point_in_polygon(p, &outline(o)))
        };
        let zl = if build.extended_stiles && !neighbour(-half - c.filler_left.mm() - 10.0) { 0.0 } else { base };
        let zr = if build.extended_stiles && !neighbour(half + c.filler_right.mm() + 10.0) { 0.0 } else { base };
        b.slab(&front, 0.0, edge, zl, h, -ft, ft, Surface::CabinetBox);
        b.slab(&front, len - edge, len, zr, h, -ft, ft, Surface::CabinetBox);
        b.slab(&front, edge, len - edge, h - edge, h, -ft, ft, Surface::CabinetBox);
        b.slab(&front, edge, len - edge, base, base + edge, -ft, ft, Surface::CabinetBox);
        let mut rows: Vec<[f64; 4]> = all.iter().map(|c| c.opening).collect();
        rows.dedup();
        for (k, o) in rows.iter().enumerate() {
            // Rail below each opening that isn't the bottom one.
            if o[2] - gap > base + edge + 1.0 {
                b.slab(&front, o[0], o[1], o[2] - gap, o[2], -ft, ft, Surface::CabinetBox);
            }
            // Mullion to the right of each opening that has a neighbour.
            if rows.get(k + 1).is_some_and(|n| (n[2] - o[2]).abs() < 1.0 && n[0] > o[1]) {
                b.slab(&front, o[1], rows[k + 1][0], o[2], o[3], -ft, ft, Surface::CabinetBox);
            }
        }
    }
    // End panels.
    let et = 0.75 * IN;
    if c.ends.left {
        block(&mut b, (-half - c.filler_left.mm() - et, -half - c.filler_left.mm()), (0.0, d + et), (0.0, h), Surface::CabinetBox);
    }
    if c.ends.right {
        block(&mut b, (half + c.filler_right.mm(), half + c.filler_right.mm() + et), (0.0, d + et), (0.0, h), Surface::CabinetBox);
    }
    // Fillers: strips flush with the front beside the box.
    for (u0, u1) in [(-half - c.filler_left.mm(), -half), (half, half + c.filler_right.mm())] {
        if u1 - u0 > 0.5 {
            b.prism(
                &[DVec2::new(u0, d - IN), DVec2::new(u1, d - IN), DVec2::new(u1, d), DVec2::new(u0, d)],
                base,
                h,
                Surface::CabinetBox,
            );
        }
    }
    // Doors and drawers, opened by `open`.
    let open = c.open.clamp(0.0, 1.0);
    let straight = front.len() == 2;
    for cell in &all {
        b.xf = None;
        if open > 0.0 && straight {
            let t = (front[1] - front[0]).normalize_or(DVec2::X);
            match cell.item {
                FaceItem::DoorLeft | FaceItem::DoorRight | FaceItem::GlassDoor => {
                    let hinge = if cell.hinge_left { cell.s0 } else { cell.s1 };
                    let pivot = front[0] + t * hinge + perp(t) * DOOR_T;
                    let sign = if cell.hinge_left { 1.0 } else { -1.0 };
                    b.xf = Some((pivot, sign * open * 95f64.to_radians(), DVec2::ZERO));
                }
                FaceItem::Drawer => {
                    b.xf = Some((DVec2::ZERO, 0.0, perp(t) * open * d * 0.7));
                }
                _ => {}
            }
        }
        match cell.item {
            FaceItem::Open => {
                if !standard {
                    b.slab(&front, cell.s0, cell.s1, cell.z0, cell.z1, -0.5 * IN, 0.4 * IN, Surface::CabinetBox);
                }
            }
            FaceItem::GlassDoor => front_panel(&mut b, &front, cell, c.door_style, true),
            FaceItem::Appliance => {
                b.slab(&front, cell.s0, cell.s1, cell.z0, cell.z1, 0.0, DOOR_T, Surface::Hardware);
                let m = ((cell.s1 - cell.s0).min(cell.z1 - cell.z0) * 0.15).min(3.0 * IN);
                b.slab(&front, cell.s0 + m, cell.s1 - m, cell.z0 + m, cell.z1 - 2.0 * m, DOOR_T, 0.1 * IN, Surface::Glass);
            }
            _ => front_panel(&mut b, &front, cell, c.door_style, false),
        }
        if c.hardware != HardwareStyle::None
            && let Some((s, z, vertical)) = hardware_spot(c, cell)
        {
            let proud = DOOR_T + 0.2 * IN;
            match (c.hardware, vertical) {
                (HardwareStyle::Knob, _) => b.knob(&front, s, z, IN, IN, proud, IN),
                (_, true) => b.knob(&front, s, z, 0.5 * IN, 5.0 * IN, proud, IN),
                (_, false) => b.knob(&front, s, z, 5.0 * IN, 0.5 * IN, proud, IN),
            }
        }
    }
    b.xf = None;
    // Moldings, stacked outward from the box at the top and bottom.
    if !c.moldings.is_empty() {
        let (el, er) = (
            c.filler_left.mm() + if c.ends.left { et } else { 0.0 },
            c.filler_right.mm() + if c.ends.right { et } else { 0.0 },
        );
        let fv = d + DOOR_T;
        let path: Vec<DVec2> = [
            DVec2::new(-half - el, 0.0),
            DVec2::new(-half - el, fv),
            DVec2::new(half + er, fv),
            DVec2::new(half + er, 0.0),
        ]
        .iter()
        .map(|p| b.f.world(*p))
        .collect();
        let (mut top, mut bot) = (h, 0.0);
        for m in &c.moldings {
            let (mh, mp) = (m.height.mm().max(1.0), m.projection.mm().max(1.0));
            let prof = crate::sweep::profile(m.shape, mh, mp);
            let zb = match (m.place, m.flush) {
                (MoldingPlace::Top, false) => {
                    let z = top;
                    top += mh;
                    z
                }
                (MoldingPlace::Top, true) => h - mh,
                (MoldingPlace::Bottom, false) => {
                    bot -= mh;
                    bot
                }
                (MoldingPlace::Bottom, true) => 0.0,
            };
            // Bottom moldings hang with their profile upside down.
            let prof: Vec<DVec2> = if m.place == MoldingPlace::Bottom {
                prof.iter().rev().map(|q| DVec2::new(q.x, mh - q.y)).collect()
            } else {
                prof
            };
            crate::sweep::sweep(b.mesh, &path, false, &prof, z0 + zb, Surface::CabinetBox);
        }
    }
    // Countertop, waterfall ends and backsplash.
    if let (Some(top), Some(outline)) = (&c.countertop, countertop_outline(c)) {
        let zt = z0 + h;
        let tt = top.thickness.mm();
        let holes: Vec<Vec<DVec2>> = sink_hole(c).into_iter().collect();
        let mut cuts = holes.clone();
        cuts.extend(sink_symbol_holes(doc, c.floor, &outline));
        counter_slab(b.mesh, &outline, &cuts, zt, zt + tt, top.edge);
        if let Some(hole) = holes.first() {
            // The sink bowl under the cut-out.
            b.mesh.prism(hole, zt - 8.0 * IN, zt - 7.5 * IN, Surface::Hardware);
            let n = hole.len();
            for i in 0..n {
                let (p, q) = (hole[i], hole[(i + 1) % n]);
                b.mesh.polygon(
                    &[p.extend(zt - 7.5 * IN), q.extend(zt - 7.5 * IN), q.extend(zt), p.extend(zt)],
                    Surface::Hardware,
                );
            }
        }
        let (l, r) = (c.filler_left.mm(), c.filler_right.mm());
        let o = top.overhang.mm();
        if top.waterfall_left {
            block(&mut b, (-half - l - tt, -half - l), (0.0, d + o), (0.0, h + tt), Surface::Countertop);
        }
        if top.waterfall_right {
            block(&mut b, (half + r, half + r + tt), (0.0, d + o), (0.0, h + tt), Surface::Countertop);
        }
        let bs = top.backsplash.mm();
        if bs > 0.0 {
            let zb = tt + h;
            block(&mut b, (-half - l, half + r), (0.0, 0.75 * IN), (zb, zb + bs), Surface::Backsplash);
            // Side splashes up the ends, the depth of the top.
            if top.side_splash_left {
                block(&mut b, (-half - l, -half - l + 0.75 * IN), (0.0, d + o), (zb, zb + bs), Surface::Backsplash);
            }
            if top.side_splash_right {
                block(&mut b, (half + r - 0.75 * IN, half + r), (0.0, d + o), (zb, zb + bs), Surface::Backsplash);
            }
        }
    }
}

/// A custom countertop's 3D slab with its holes.
pub fn countertop_mesh(doc: &Document, t: &Countertop, mesh: &mut Mesh) {
    let Some(floor) = doc.get::<Floor>(t.floor) else {
        return;
    };
    let top = floor.elevation.mm() + t.height.mm();
    let outline: Vec<DVec2> = t.points.iter().map(|p| v2(*p)).collect();
    let mut holes: Vec<Vec<DVec2>> = t.holes.iter().map(hole_outline).collect();
    holes.extend(sink_symbol_holes(doc, t.floor, &outline));
    counter_slab(mesh, &outline, &holes, top - t.thickness.mm(), top, t.edge);
}

/// Cut-outs for sink symbols standing inside `outline` on `floor`: each
/// symbol's footprint, an inch in so the rim rests on the counter.
pub fn sink_symbol_holes(doc: &Document, floor: Id, outline: &[DVec2]) -> Vec<Vec<DVec2>> {
    doc.iter::<Symbol>()
        .filter(|s| s.floor == floor)
        .filter(|s| {
            doc.get::<SymbolDef>(s.def)
                .is_some_and(|d| d.behavior == housecraft_model::SymbolBehavior::Sink)
        })
        .map(|s| symbol_outline(doc, s))
        .filter(|hull| hull.len() >= 3 && crate::math::point_in_polygon(crate::math::centroid(hull), outline))
        .map(|hull| {
            let inset = inset_polygon(&hull, IN);
            if inset.len() >= 3 { inset } else { hull }
        })
        .collect()
}

/// A countertop hole's outline in plan (counter-clockwise).
pub fn hole_outline(h: &housecraft_model::CounterHole) -> Vec<DVec2> {
    let c = v2(h.center);
    match h.hole_kind {
        HoleKind::Round => (0..16)
            .map(|i| c + DVec2::from_angle(i as f64 / 16.0 * std::f64::consts::TAU) * h.width.mm() / 2.0)
            .collect(),
        _ => {
            let a = DVec2::from_angle(h.rotation.to_radians());
            let n = perp(a);
            let (w, d) = (h.width.mm() / 2.0, h.depth.mm() / 2.0);
            ccw(vec![c - a * w - n * d, c + a * w - n * d, c + a * w + n * d, c - a * w + n * d])
        }
    }
}

pub fn appliance_frame(a: &Appliance) -> Frame {
    Frame::new(v2(a.position), a.facing)
}

/// An appliance's footprint in plan (counter-clockwise).
pub fn appliance_outline(a: &Appliance) -> Vec<DVec2> {
    let f = appliance_frame(a);
    let (h, d) = (a.width.mm() / 2.0, a.depth.mm());
    let local: Vec<DVec2> = match a.appliance_kind {
        ApplianceKind::Toilet => {
            // Tank at the back, round bowl in front.
            let tank = 8.0 * IN;
            let mut pts = vec![
                DVec2::new(-h, 0.0),
                DVec2::new(h, 0.0),
                DVec2::new(h, tank),
                DVec2::new(h * 0.8, tank),
            ];
            let c = DVec2::new(0.0, (tank + d) / 2.0);
            let (rx, ry) = (h * 0.8, (d - tank) / 2.0);
            pts.extend((0..=12).map(|i| {
                let t = i as f64 / 12.0 * std::f64::consts::PI;
                c + DVec2::new(rx * t.cos(), ry * t.sin())
            }));
            pts.push(DVec2::new(-h * 0.8, tank));
            pts.push(DVec2::new(-h, tank));
            pts
        }
        _ => vec![DVec2::new(-h, 0.0), DVec2::new(h, 0.0), DVec2::new(h, d), DVec2::new(-h, d)],
    };
    ccw(local.into_iter().map(|p| f.world(p)).collect())
}

/// Simple 3D shapes for appliances and fixtures.
pub fn appliance_mesh(doc: &Document, a: &Appliance, mesh: &mut Mesh) {
    use ApplianceKind as K;
    let Some(floor) = doc.get::<Floor>(a.floor) else {
        return;
    };
    let z0 = floor.elevation.mm() + a.elevation.mm();
    let (w, d, h) = (a.width.mm(), a.depth.mm(), a.height.mm());
    let f = appliance_frame(a);
    let mut b = Builder { f, z0, mesh, xf: None };
    let rect = |u0: f64, v0: f64, u1: f64, v1: f64| {
        vec![DVec2::new(u0, v0), DVec2::new(u1, v0), DVec2::new(u1, v1), DVec2::new(u0, v1)]
    };
    let hw = w / 2.0;
    let steel = Surface::Hardware;
    match a.appliance_kind {
        K::Refrigerator => {
            b.prism(&rect(-hw, 0.0, hw, d), 0.0, h, steel);
            // Two doors over a freezer drawer, with handles.
            let front = vec![DVec2::new(-hw, d), DVec2::new(hw, d)];
            b.slab(&front, IN * 0.5, w / 2.0 - 0.1 * IN, h * 0.32, h - IN, 0.0, 0.5 * IN, steel);
            b.slab(&front, w / 2.0 + 0.1 * IN, w - 0.5 * IN, h * 0.32, h - IN, 0.0, 0.5 * IN, steel);
            b.slab(&front, IN * 0.5, w - 0.5 * IN, IN, h * 0.32 - 0.5 * IN, 0.0, 0.5 * IN, steel);
            b.knob(&front, w / 2.0 - 1.5 * IN, h * 0.62, 0.75 * IN, 14.0 * IN, 0.5 * IN, 1.5 * IN);
            b.knob(&front, w / 2.0 + 1.5 * IN, h * 0.62, 0.75 * IN, 14.0 * IN, 0.5 * IN, 1.5 * IN);
        }
        K::Range | K::Washer | K::Dryer | K::Dishwasher | K::WallOven | K::Microwave => {
            b.prism(&rect(-hw, 0.0, hw, d), 0.0, h, steel);
            let front = vec![DVec2::new(-hw, d), DVec2::new(hw, d)];
            match a.appliance_kind {
                K::Range => {
                    // Oven door with a window, burners on top.
                    b.slab(&front, IN, w - IN, 6.0 * IN, h - 8.0 * IN, 0.0, 0.5 * IN, steel);
                    b.slab(&front, 5.0 * IN, w - 5.0 * IN, 12.0 * IN, h - 14.0 * IN, 0.5 * IN, 0.1 * IN, Surface::Glass);
                    for (u, v) in [(-0.25, 0.3), (0.25, 0.3), (-0.25, 0.72), (0.25, 0.72)] {
                        let c = DVec2::new(u * w, v * d);
                        let disc: Vec<DVec2> = (0..12)
                            .map(|i| c + DVec2::from_angle(i as f64 / 12.0 * std::f64::consts::TAU) * 3.5 * IN)
                            .collect();
                        b.prism(&disc, h, h + 0.5 * IN, Surface::Device);
                    }
                }
                K::Washer | K::Dryer => {
                    // Door window and control panel.
                    b.slab(&front, w * 0.2, w * 0.8, h * 0.25, h * 0.75, 0.0, IN, Surface::Glass);
                    b.slab(&front, 0.0, w, h - 6.0 * IN, h, 0.0, 0.3 * IN, Surface::Device);
                }
                _ => b.slab(&front, IN * 0.5, w - IN * 0.5, IN, h - IN, 0.0, 0.5 * IN, steel),
            }
        }
        K::Cooktop => {
            b.prism(&rect(-hw, 0.0, hw, d), 0.0, h, Surface::Glass);
        }
        K::RangeHood => {
            b.prism(&rect(-hw, 0.0, hw, d), 0.0, h, steel);
            b.prism(&rect(-w * 0.2, 0.0, w * 0.2, d * 0.5), h, h + 24.0 * IN, steel);
        }
        K::Toilet => {
            let tank = 8.0 * IN;
            b.prism(&rect(-hw, 0.0, hw, tank), 15.0 * IN, h, Surface::Device);
            let bowl: Vec<DVec2> = (0..16)
                .map(|i| {
                    let t = i as f64 / 16.0 * std::f64::consts::TAU;
                    DVec2::new(0.0, (tank + d) / 2.0) + DVec2::new(hw * 0.8 * t.cos(), (d - tank) / 2.0 * t.sin())
                })
                .collect();
            b.prism(&bowl, 0.0, 15.0 * IN, Surface::Device);
        }
        K::Bathtub | K::Shower => {
            let wall = 3.0 * IN;
            b.prism(&rect(-hw, 0.0, hw, wall), 0.0, h, Surface::Device);
            b.prism(&rect(-hw, d - wall, hw, d), 0.0, h, Surface::Device);
            b.prism(&rect(-hw, wall, -hw + wall, d - wall), 0.0, h, Surface::Device);
            b.prism(&rect(hw - wall, wall, hw, d - wall), 0.0, h, Surface::Device);
            b.prism(&rect(-hw + wall, wall, hw - wall, d - wall), 0.0, 2.0 * IN, Surface::Device);
            // The drain, at the far end unless reversed.
            if a.appliance_kind == K::Bathtub {
                let u = if a.reversed { -(hw - 8.0 * IN) } else { hw - 8.0 * IN };
                b.prism(&rect(u - IN, d / 2.0 - IN, u + IN, d / 2.0 + IN), 2.0 * IN, 2.2 * IN, Surface::Hardware);
            }
        }
        K::Faucet => {
            // A round-ish riser and a spout reaching forward over the bowl.
            b.prism(&rect(-hw.min(1.25 * IN), 0.0, hw.min(1.25 * IN), 2.5 * IN), 0.0, h, steel);
            b.prism(&rect(-0.5 * IN, 0.0, 0.5 * IN, d), h - 2.0 * IN, h, steel);
        }
        K::TubSpout => {
            b.prism(&rect(-hw, 0.0, hw, d), 0.0, h, steel);
        }
        K::Partition => {
            b.prism(&rect(-hw, 0.0, hw, d), 0.0, h, Surface::CabinetBox);
            // Crown molding stepping out on the top.
            b.prism(&rect(-hw - IN, 0.0, hw + IN, d + IN), h - 3.0 * IN, h, Surface::Trim);
        }
        K::ShowerHead => {
            // An arm out of the wall, dropping to a round head.
            b.prism(&rect(-0.5 * IN, 0.0, 0.5 * IN, d * 0.7), h - IN, h, steel);
            let c = DVec2::new(0.0, d * 0.75);
            let ring: Vec<DVec2> = (0..12).map(|i| c + DVec2::from_angle(i as f64 / 12.0 * std::f64::consts::TAU) * hw).collect();
            b.prism(&ring, 0.0, IN, steel);
        }
        K::VanitySink => {
            // An oval bowl rim at counter height.
            let c = DVec2::new(0.0, d / 2.0);
            let ring: Vec<DVec2> = (0..16)
                .map(|i| {
                    let t = i as f64 / 16.0 * std::f64::consts::TAU;
                    c + DVec2::new(hw * t.cos(), d / 2.0 * t.sin())
                })
                .collect();
            b.prism(&ring, 0.0, h, Surface::Device);
        }
        K::Pedestal => {
            b.prism(&rect(-2.5 * IN, d * 0.3, 2.5 * IN, d * 0.6), 0.0, h - 6.0 * IN, Surface::Device);
            b.prism(&rect(-hw, 0.0, hw, d), h - 6.0 * IN, h, Surface::Device);
        }
        K::QueenBed => {
            let (wood, fabric) = (Surface::Furniture, Surface::Upholstery);
            b.prism(&rect(-hw, 0.0, hw, 3.0 * IN), 0.0, 44.0 * IN, wood); // headboard
            b.prism(&rect(-hw, 3.0 * IN, hw, d), 6.0 * IN, 14.0 * IN, wood); // frame
            b.prism(&rect(-hw + IN, 3.0 * IN, hw - IN, d - IN), 14.0 * IN, h, fabric); // mattress
            for u in [-hw / 2.0, hw / 2.0] {
                b.prism(&rect(u - 11.0 * IN, 5.0 * IN, u + 11.0 * IN, 20.0 * IN), h, h + 5.0 * IN, Surface::Accent); // pillows
            }
            for (u, v) in [(-hw, 3.0 * IN), (hw - 2.0 * IN, 3.0 * IN), (-hw, d - 2.0 * IN), (hw - 2.0 * IN, d - 2.0 * IN)] {
                b.prism(&rect(u, v, u + 2.0 * IN, v + 2.0 * IN), 0.0, 6.0 * IN, wood);
            }
        }
        K::EndTable | K::Dresser | K::CoffeeTable | K::DiningTable => {
            let wood = Surface::Furniture;
            let top = IN.min(h * 0.1);
            b.prism(&rect(-hw, 0.0, hw, d), h - top, h, wood);
            let leg = 2.0 * IN;
            if matches!(a.appliance_kind, K::Dresser | K::EndTable) {
                // A case of drawers.
                b.prism(&rect(-hw + IN, IN, hw - IN, d - IN), 3.0 * IN, h - top, wood);
                let front = vec![DVec2::new(-hw, d), DVec2::new(hw, d)];
                let rows = if a.appliance_kind == K::Dresser { 3 } else { 1 };
                let zh = (h - top - 4.0 * IN) / rows as f64;
                for r in 0..rows {
                    let z = 3.5 * IN + zh * r as f64;
                    b.slab(&front, 1.5 * IN, w - 1.5 * IN, z, z + zh - 0.5 * IN, -IN, 0.75 * IN, wood);
                    b.knob(&front, w / 2.0, z + zh / 2.0, 0.6 * IN, 0.6 * IN, -0.25 * IN, 0.75 * IN);
                }
            }
            for (u, v) in [(-hw, 0.0), (hw - leg, 0.0), (-hw, d - leg), (hw - leg, d - leg)] {
                b.prism(&rect(u, v, u + leg, v + leg), 0.0, h - top, wood);
            }
        }
        K::Sofa | K::ArmChair => {
            let (wood, fabric) = (Surface::Furniture, Surface::Upholstery);
            let arm = 6.0 * IN;
            let seat = 18.0 * IN;
            b.prism(&rect(-hw, 0.0, hw, d), 3.0 * IN, 10.0 * IN, fabric); // base
            b.prism(&rect(-hw, 0.0, hw, 9.0 * IN), 10.0 * IN, h, fabric); // back
            b.prism(&rect(-hw, 9.0 * IN, -hw + arm, d), 10.0 * IN, 25.0 * IN, fabric); // arms
            b.prism(&rect(hw - arm, 9.0 * IN, hw, d), 10.0 * IN, 25.0 * IN, fabric);
            b.prism(&rect(-hw + arm, 9.0 * IN, hw - arm, d), 10.0 * IN, seat, Surface::Accent); // cushions
            for (u, v) in [(-hw, 0.0), (hw - 2.0 * IN, 0.0), (-hw, d - 2.0 * IN), (hw - 2.0 * IN, d - 2.0 * IN)] {
                b.prism(&rect(u, v, u + 2.0 * IN, v + 2.0 * IN), 0.0, 3.0 * IN, wood);
            }
        }
        K::DiningChair => {
            let (wood, fabric) = (Surface::Furniture, Surface::Upholstery);
            let leg = 1.5 * IN;
            for (u, v) in [(-hw, 0.0), (hw - leg, 0.0), (-hw, d - leg), (hw - leg, d - leg)] {
                b.prism(&rect(u, v, u + leg, v + leg), 0.0, 17.0 * IN, wood);
            }
            b.prism(&rect(-hw, 0.0, hw, d), 17.0 * IN, 19.0 * IN, fabric); // seat
            b.prism(&rect(-hw, 0.0, hw, leg), 19.0 * IN, h, wood); // back
        }
        K::Vase => {
            let c = DVec2::new(0.0, d / 2.0);
            let disc = |r: f64| -> Vec<DVec2> { (0..16).map(|i| c + DVec2::from_angle(i as f64 / 16.0 * std::f64::consts::TAU) * r).collect() };
            b.prism(&disc(w / 2.0), 0.0, h * 0.7, Surface::Device);
            b.prism(&disc(w / 3.0), h * 0.7, h, Surface::Device);
        }
        K::PictureFrame => {
            let front = vec![DVec2::new(-hw, d), DVec2::new(hw, d)];
            let frame = 2.0 * IN;
            b.prism(&rect(-hw, 0.0, hw, d * 0.5), 0.0, h, Surface::Furniture); // backing
            b.slab(&front, 0.0, w, 0.0, frame, 0.0, d * 0.5, Surface::Furniture);
            b.slab(&front, 0.0, w, h - frame, h, 0.0, d * 0.5, Surface::Furniture);
            b.slab(&front, 0.0, frame, frame, h - frame, 0.0, d * 0.5, Surface::Furniture);
            b.slab(&front, w - frame, w, frame, h - frame, 0.0, d * 0.5, Surface::Furniture);
            b.slab(&front, frame, w - frame, frame, h - frame, 0.0, 0.1 * IN, Surface::Glass);
        }
        K::WaterHeater => {
            let c = DVec2::new(0.0, d / 2.0);
            let disc: Vec<DVec2> = (0..20)
                .map(|i| c + DVec2::from_angle(i as f64 / 20.0 * std::f64::consts::TAU) * w / 2.0)
                .collect();
            b.prism(&disc, 0.0, h, Surface::Device);
        }
    }
}

/// Where a symbol's model points (mm, as imported) land in the plan (mm).
fn symbol_points(s: &Symbol, def: &SymbolDef) -> impl Fn([f32; 3]) -> DVec3 {
    let f = Frame::new(v2(s.position), s.facing);
    let size = def.size();
    let k = [
        s.width.mm() / size[0].max(1e-6),
        s.depth.mm() / size[1].max(1e-6),
        s.height.mm() / size[2].max(1e-6),
    ];
    let z0 = s.elevation.mm();
    move |p: [f32; 3]| {
        let local = DVec2::new(p[0] as f64 * k[0], -(p[1] as f64) * k[1]);
        f.world(local).extend(z0 + p[2] as f64 * k[2])
    }
}

/// A placed 3D symbol's footprint in plan: the outline around all its
/// points (counter-clockwise).
pub fn symbol_outline(doc: &Document, s: &Symbol) -> Vec<DVec2> {
    let Some(def) = doc.get::<SymbolDef>(s.def) else {
        return Vec::new();
    };
    let place = symbol_points(s, def);
    let mut pts: Vec<DVec2> = def.positions.iter().map(|p| place(*p).truncate()).collect();
    convex_hull(&mut pts)
}

/// Monotone-chain convex hull (counter-clockwise).
fn convex_hull(pts: &mut [DVec2]) -> Vec<DVec2> {
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut hull: Vec<DVec2> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &DVec2>> = if pass == 0 {
            Box::new(pts.iter())
        } else {
            Box::new(pts.iter().rev())
        };
        for &p in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// A placed 3D symbol: its triangles in their imported colours, or in the
/// material painted on it.
pub fn symbol_mesh(doc: &Document, s: &Symbol, mesh: &mut Mesh) {
    let (Some(def), Some(floor)) = (doc.get::<SymbolDef>(s.def), doc.get::<Floor>(s.floor)) else {
        return;
    };
    let place = symbol_points(s, def);
    let z0 = floor.elevation.mm();
    let painted = mesh.painted_whole();
    let look_of = |c: housecraft_model::Color| {
        let [r, g, b, _] = c.0;
        let f = |v: u8| v as f32 / 255.0;
        crate::mesh::Look {
            color: [f(r), f(g), f(b), 1.0],
            pbr: [0.6, 0.0, 0.0, 1.0],
            surface: None,
            tex: None,
        }
    };
    let grey = look_of(housecraft_model::Color::rgb(190, 190, 190));
    let mut group_of = vec![grey; def.triangles.len()];
    for g in &def.groups {
        let look = look_of(g.color);
        for t in g.first as usize..(g.first + g.count) as usize {
            if let Some(slot) = group_of.get_mut(t) {
                *slot = look;
            }
        }
    }
    for (t, tri) in def.triangles.iter().enumerate() {
        let pts: Vec<DVec3> = tri
            .iter()
            .filter_map(|i| def.positions.get(*i as usize))
            .map(|p| place(*p) + DVec3::Z * z0)
            .collect();
        if pts.len() == 3 {
            mesh.polygon_look(&pts, painted.unwrap_or(group_of[t]));
        }
    }
}

/// Gaps up to `max` between cabinets of the same kind side by side, and
/// between cabinet ends and walls: (cabinet, new left filler, new right
/// filler). Cabinets marked `no_auto_filler` are left alone.
pub fn auto_fillers(doc: &Document, floor: Id, max: f64) -> Vec<(Id, f64, f64)> {
    let cabs: Vec<&Cabinet> = doc
        .iter::<Cabinet>()
        .filter(|c| c.floor == floor && c.shape == CabinetShape::Standard)
        .collect();
    let walls = crate::walls::wall_geoms(doc, floor);
    let mut out = Vec::new();
    for c in &cabs {
        if c.no_auto_filler {
            continue;
        }
        let f = frame(c);
        let half = c.width.mm() / 2.0;
        let mid_v = c.depth.mm() / 2.0;
        let mut fill = [0.0f64; 2];
        for (side, sign) in [(0, -1.0), (1, 1.0)] {
            let end = f.world(DVec2::new(sign * half, mid_v));
            let dir = f.along * sign;
            // Nearest obstacle straight out from this end.
            let mut nearest = f64::MAX;
            for o in &cabs {
                if o.id == c.id || o.cabinet_kind != c.cabinet_kind {
                    continue;
                }
                for p in outline_with_fillers(o) {
                    let rel = p - end;
                    let ahead = rel.dot(dir);
                    if ahead >= -1.0 && rel.dot(f.out).abs() <= c.depth.mm() / 2.0 + 1.0 {
                        nearest = nearest.min(ahead.max(0.0));
                    }
                }
            }
            for g in &walls {
                for (a, bb) in [(g.corners[0], g.corners[1]), (g.corners[3], g.corners[2])] {
                    if let Some((hit, _, _)) = crate::math::segment_intersection(end, end + dir * (max + 1.0), a, bb) {
                        nearest = nearest.min((hit - end).dot(dir));
                    }
                }
            }
            if nearest > 0.5 && nearest <= max {
                fill[side] = nearest;
            }
        }
        if (fill[0] - c.filler_left.mm()).abs() > 0.5 || (fill[1] - c.filler_right.mm()).abs() > 0.5 {
            // Split gaps between two cabinets so each takes half.
            out.push((c.id, fill[0], fill[1]));
        }
    }
    // A gap shared by two cabinets would be filled twice; halve those.
    let ids: Vec<Id> = out.iter().map(|o| o.0).collect();
    for entry in &mut out {
        let c = doc.get::<Cabinet>(entry.0).expect("listed");
        let f = frame(c);
        for side in 0..2 {
            let gap = if side == 0 { entry.1 } else { entry.2 };
            if gap <= 0.0 {
                continue;
            }
            let sign = if side == 0 { -1.0 } else { 1.0 };
            let probe = f.world(DVec2::new(sign * (c.width.mm() / 2.0 + gap + 1.0), c.depth.mm() / 2.0));
            let shared = ids.iter().any(|o| {
                *o != c.id
                    && doc
                        .get::<Cabinet>(*o)
                        .is_some_and(|other| crate::math::point_in_polygon(probe, &outline(other)))
            });
            if shared {
                if side == 0 {
                    entry.1 = gap / 2.0;
                } else {
                    entry.2 = gap / 2.0;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use housecraft_model::{CabinetBuild, CabinetDefaults, EndPanels, FaceRow, Renovation};
    use housecraft_units::{Length, Point2};

    fn cab(kind: CabinetKind, width_in: f64) -> Cabinet {
        let d = CabinetDefaults::default();
        Cabinet {
            id: Id(1),
            floor: Id(2),
            layer: Id(3),
            cabinet_kind: kind,
            position: Point2(Length::ZERO, Length::ZERO),
            facing: 90.0,
            width: Length::from_inches(width_in),
            depth: d.base_depth,
            height: d.base_height,
            elevation: Length::ZERO,
            construction: CabinetConstruction::Framed,
            shape: CabinetShape::Standard,
            face: d.face(kind, Length::from_inches(width_in), false),
            door_style: DoorStyleCab::Shaker,
            hardware: HardwareStyle::Pull,
            handle_from_edge: None,
            shelves: 1,
            toe_kick: Some((d.toe_kick_height, d.toe_kick_depth)),
            countertop: None,
            sink: false,
            filler_left: Length::ZERO,
            filler_right: Length::ZERO,
            no_auto_filler: false,
            renovation: Renovation::Existing,
            build: CabinetBuild::default(),
            open: 0.0,
            blind: None,
            moldings: Vec::new(),
            ends: EndPanels::default(),
            accessories: Vec::new(),
            corner: None,
        }
    }

    #[test]
    fn base_face_has_drawer_over_double_door() {
        let c = cab(CabinetKind::Base, 30.0);
        let cells = cells(&c);
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].item, FaceItem::Drawer);
        assert!(cells[0].z0 > cells[1].z1, "drawer above doors");
        assert!(cells[1].hinge_left && !cells[2].hinge_left);
        // Framed: 1.5" stiles each side; doors overlay them by 1/2".
        assert!((cells[0].opening[0] - 1.5 * IN).abs() < 1e-6);
        assert!((cells[0].opening[1] - 28.5 * IN).abs() < 1e-6);
        assert!((cells[0].s0 - 1.0 * IN).abs() < 1e-6);
        assert!((cells[0].s1 - 29.0 * IN).abs() < 1e-6);
        // Frameless: faces sit a reveal apart.
        let mut fl = c.clone();
        fl.construction = CabinetConstruction::Frameless;
        let f = super::cells(&fl);
        assert!((f[0].s0 - 0.0625 * IN).abs() < 1e-6);
        assert!(((f[2].s0 - f[1].s1) - 0.125 * IN).abs() < 0.2 * IN);
        // A blind corner turns the left 12" into a panel.
        let mut bl = c.clone();
        bl.blind = Some(housecraft_model::BlindCorner { side: housecraft_model::Side::Left, width: Length::from_inches(12.0) });
        let b = super::cells(&bl);
        assert_eq!(b[0].item, FaceItem::Panel);
        assert!(b[1].opening[0] > 13.0 * IN);
        // Pulls: drawer centred and horizontal; doors near the meeting edge, near the top.
        let (s, _, vert) = hardware_spot(&c, &cells[0]).unwrap();
        assert!(!vert && (s - 15.0 * IN).abs() < 1e-6);
        let (s, z, vert) = hardware_spot(&c, &cells[1]).unwrap();
        assert!(vert && s > 10.0 * IN && z > cells[1].z0 + (cells[1].z1 - cells[1].z0) / 2.0);
    }

    #[test]
    fn crown_molding_sits_on_top() {
        let doc = Document::new("t");
        let floor = doc.floors_by_elevation()[0].id;
        let mut c = cab(CabinetKind::Wall, 30.0);
        c.floor = floor;
        c.height = Length::from_inches(30.0);
        c.toe_kick = None;
        c.moldings = vec![housecraft_model::CabinetMolding {
            place: MoldingPlace::Top,
            shape: housecraft_model::MoldingShape::Crown,
            height: Length::from_inches(3.0),
            projection: Length::from_inches(2.0),
            flush: false,
        }];
        let mut mesh = Mesh::default();
        mesh.begin(Id(1), crate::mesh::Category::Cabinet);
        cabinet_mesh(&doc, &c, &mut mesh);
        let top = mesh.positions.iter().map(|p| p[2]).fold(f32::MIN, f32::max);
        let want = (Length::from_inches(33.0).mm() / 1000.0) as f32;
        assert!((top - want).abs() < 0.002, "top {top} want {want}");
    }

    #[test]
    fn faces_by_row_heights() {
        let mut c = cab(CabinetKind::Full, 24.0);
        c.height = Length::from_inches(84.0);
        c.face = vec![
            FaceRow { height: None, items: vec![FaceItem::DoorRight], shelves: None },
            FaceRow { height: Some(Length::from_inches(10.0)), items: vec![FaceItem::Drawer, FaceItem::Drawer], shelves: None },
        ];
        let cells = cells(&c);
        assert_eq!(cells.len(), 3);
        assert!((cells[1].opening[3] - cells[1].opening[2] - 10.0 * IN).abs() < 1e-6);
    }

    #[test]
    fn shapes_have_sensible_outlines() {
        let mut c = cab(CabinetKind::Base, 36.0);
        let area = |c: &Cabinet| signed_area(&outline(c)) / (IN * IN);
        assert!((area(&c) - 36.0 * 24.0).abs() < 1e-6);
        c.shape = CabinetShape::Angled { left: Length::from_inches(12.0), right: Length::ZERO };
        assert!((area(&c) - (36.0 * 24.0 - 72.0)).abs() < 1e-6);
        c.shape = CabinetShape::Bow { depth: Length::from_inches(3.0) };
        assert!(area(&c) > 36.0 * 24.0);
        c.shape = CabinetShape::Curved { radius: Length::from_inches(60.0) };
        assert!(area(&c) > 36.0 * 24.0, "convex curve widens toward the front");
        // Facing north: the front is at +y.
        let c = cab(CabinetKind::Base, 24.0);
        let front = front_line(&c);
        assert!(front.iter().all(|p| (p.y - 24.0 * IN).abs() < 1e-6));
        assert!(front[0].x > front[1].x, "left end is to the viewer's left (east when facing north)");
    }
}
