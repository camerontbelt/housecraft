//! Floating dormers sitting on the main roof: a front wall with a window,
//! cheek walls down to the roof, and a gable or shed roof running back until
//! it meets the main roof.

use glam::{DVec2, DVec3};
use housecraft_model::{Document, Dormer, DormerStyle, Surface};

use crate::mesh::Mesh;
use crate::roof_extras::roof_top_at;

/// Plan outline of a dormer's roof and its ridge line (for plan views).
pub struct DormerPlan {
    pub outline: Vec<DVec2>,
    pub ridge: Option<(DVec2, DVec2)>,
}

struct Frame {
    c: DVec2,
    f: DVec2,
    s: DVec2,
    half: f64,
    z_b: f64,
    z_e: f64,
}

fn frame(doc: &Document, d: &Dormer) -> Option<Frame> {
    let c = crate::math::v2(d.position);
    let f = DVec2::from_angle(d.facing.to_radians());
    let z_b = roof_top_at(doc, Some(d.floor), c - f * 20.0)?;
    Some(Frame { c, f, s: DVec2::new(-f.y, f.x), half: d.width.mm() / 2.0, z_b, z_e: z_b + d.wall_height.mm() })
}

/// How far back from the front the main roof reaches height `z` along the
/// line `p - f t` (capped at 10 m).
fn back_to(doc: &Document, d: &Dormer, p: DVec2, f: DVec2, z: f64) -> f64 {
    let mut t = 0.0;
    let mut prev = f64::MIN;
    while t < 10_000.0 {
        match roof_top_at(doc, Some(d.floor), p - f * t) {
            Some(zr) if zr >= z => return t,
            // Past the main ridge (the roof starts falling) or off the roof.
            Some(zr) if zr < prev - 1.0 => return (t - 25.0).max(0.0),
            Some(zr) => prev = zr,
            None if t > 0.0 => return t,
            None => {}
        }
        t += 25.0;
    }
    t
}

pub fn plan(doc: &Document, d: &Dormer) -> Option<DormerPlan> {
    let fr = frame(doc, d)?;
    let tan = d.pitch.to_radians().tan();
    let oh = d.overhang.mm();
    Some(match d.style {
        DormerStyle::Gable => {
            let z_r = fr.z_e + fr.half * tan;
            let t_r = back_to(doc, d, fr.c, fr.f, z_r);
            let t_e = back_to(doc, d, fr.c + fr.s * fr.half, fr.f, fr.z_e);
            let front = fr.c + fr.f * oh;
            DormerPlan {
                outline: vec![
                    front - fr.s * (fr.half + oh),
                    front + fr.s * (fr.half + oh),
                    fr.c + fr.s * (fr.half + oh) - fr.f * t_e,
                    fr.c - fr.f * t_r,
                    fr.c - fr.s * (fr.half + oh) - fr.f * t_e,
                ],
                ridge: Some((front, fr.c - fr.f * t_r)),
            }
        }
        DormerStyle::Shed => {
            let t_s = shed_back(doc, d, &fr);
            let front = fr.c + fr.f * oh;
            DormerPlan {
                outline: vec![
                    front - fr.s * (fr.half + oh),
                    front + fr.s * (fr.half + oh),
                    fr.c + fr.s * (fr.half + oh) - fr.f * t_s,
                    fr.c - fr.s * (fr.half + oh) - fr.f * t_s,
                ],
                ridge: None,
            }
        }
    })
}

/// The hole a structural dormer cuts in the main roof, in plan: from its
/// front wall back between its side walls to where they meet the roof.
pub fn opening(doc: &Document, d: &Dormer) -> Option<Vec<DVec2>> {
    if !d.structural {
        return None;
    }
    let fr = frame(doc, d)?;
    let back = match d.style {
        DormerStyle::Gable => back_to(doc, d, fr.c + fr.s * fr.half, fr.f, fr.z_e),
        DormerStyle::Shed => shed_back(doc, d, &fr),
    };
    Some(vec![fr.c - fr.s * fr.half, fr.c + fr.s * fr.half, fr.c + fr.s * fr.half - fr.f * back, fr.c - fr.s * fr.half - fr.f * back])
}

/// Where a shed dormer's roof meets the main roof.
fn shed_back(doc: &Document, d: &Dormer, fr: &Frame) -> f64 {
    let tan = d.pitch.to_radians().tan();
    let mut t = 0.0;
    let mut prev = f64::MIN;
    while t < 10_000.0 {
        let z = fr.z_e + t * tan;
        match roof_top_at(doc, Some(d.floor), fr.c - fr.f * t) {
            Some(zr) if zr >= z => return t,
            Some(zr) if zr < prev - 1.0 => return (t - 25.0).max(0.0),
            Some(zr) => prev = zr,
            None if t > 0.0 => return t,
            None => {}
        }
        t += 25.0;
    }
    t
}

/// Both faces of a flat polygon.
fn two_sided(mesh: &mut Mesh, pts: &[DVec3], surface: Surface) {
    mesh.polygon(pts, surface);
    let mut r = pts.to_vec();
    r.reverse();
    mesh.polygon(&r, surface);
}

/// A roof slab: `top` with its underside `thick` lower, and edges.
fn slab(mesh: &mut Mesh, top: &[DVec3], thick: f64) {
    let mut up = top.to_vec();
    if crate::roof::newell(&up).z < 0.0 {
        up.reverse();
    }
    mesh.polygon(&up, Surface::Roof);
    let under: Vec<DVec3> = up.iter().rev().map(|p| *p - DVec3::Z * thick).collect();
    mesh.polygon(&under, Surface::Soffit);
    for i in 0..up.len() {
        let (a, b) = (up[i], up[(i + 1) % up.len()]);
        mesh.polygon(&[a, a - DVec3::Z * thick, b - DVec3::Z * thick, b], Surface::Frame);
    }
}

pub fn mesh(doc: &Document, d: &Dormer, mesh: &mut Mesh) {
    let Some(fr) = frame(doc, d) else {
        return;
    };
    let tan = d.pitch.to_radians().tan();
    let oh = d.overhang.mm();
    let at = |p: DVec2, z: f64| p.extend(z);
    let (l, r) = (fr.c - fr.s * fr.half, fr.c + fr.s * fr.half);
    // Front wall, up to a gable peak or the shed's eave.
    let z_r = fr.z_e + fr.half * tan;
    let mut front = vec![at(l, fr.z_b), at(r, fr.z_b), at(r, fr.z_e)];
    if d.style == DormerStyle::Gable {
        front.push(at(fr.c, z_r));
    }
    front.push(at(l, fr.z_e));
    two_sided(mesh, &front, Surface::Wall);
    // The window: its own size, or filling the front wall.
    if d.window && d.wall_height.mm() > 450.0 && fr.half > 300.0 {
        let half_w = d.window_width.map_or(fr.half - 220.0, |w| (w.mm() / 2.0).min(fr.half - 60.0));
        let (wl, wr) = (fr.c - fr.s * half_w + fr.f * 15.0, fr.c + fr.s * half_w + fr.f * 15.0);
        let z1 = d.window_top.map_or(fr.z_e - 120.0, |t| fr.z_b + t.mm());
        let z0 = d.window_height.map_or(fr.z_b + 180.0, |h| z1 - h.mm());
        two_sided(mesh, &[at(wl, z0), at(wr, z0), at(wr, z1), at(wl, z1)], Surface::Glass);
        let sill = [at(wl, z0 - 40.0), at(wr, z0 - 40.0), at(wr, z0), at(wl, z0)];
        two_sided(mesh, &sill, Surface::Frame);
        let head = [at(wl, z1), at(wr, z1), at(wr, z1 + 40.0), at(wl, z1 + 40.0)];
        two_sided(mesh, &head, Surface::Frame);
        // Mullions between the sashes.
        let lites = d.window_lites.max(1);
        for k in 1..lites {
            let p = wl.lerp(wr, k as f64 / lites as f64) + fr.f * 5.0;
            let (a, b) = (p - fr.s * 25.0, p + fr.s * 25.0);
            two_sided(mesh, &[at(a, z0), at(b, z0), at(b, z1), at(a, z1)], Surface::Frame);
        }
    }
    // A frieze board under the eave across the front.
    if d.frieze {
        let (fl, frr) = (l + fr.f * 20.0, r + fr.f * 20.0);
        two_sided(mesh, &[at(fl, fr.z_e - 140.0), at(frr, fr.z_e - 140.0), at(frr, fr.z_e), at(fl, fr.z_e)], Surface::Trim);
    }
    match d.style {
        DormerStyle::Gable => {
            let t_r = back_to(doc, d, fr.c, fr.f, z_r);
            for side in [-1.0, 1.0] {
                let e = fr.c + fr.s * side * fr.half;
                let t_e = back_to(doc, d, e, fr.f, fr.z_e);
                // Cheek wall: from the front down to where the roof meets the eave.
                two_sided(mesh, &[at(e, fr.z_b), at(e, fr.z_e), at(e - fr.f * t_e, fr.z_e)], Surface::Wall);
                // Roof half: eave to ridge, front overhang back to the main roof.
                let eave = fr.c + fr.s * side * (fr.half + oh);
                let drop = oh * tan;
                slab(
                    mesh,
                    &[
                        at(eave + fr.f * oh, fr.z_e - drop),
                        at(fr.c + fr.f * oh, z_r),
                        at(fr.c - fr.f * t_r, z_r),
                        at(eave - fr.f * t_e, fr.z_e - drop),
                    ],
                    d.thickness.mm(),
                );
            }
        }
        DormerStyle::Shed => {
            let t_s = shed_back(doc, d, &fr);
            let z_back = fr.z_e + t_s * tan;
            for side in [-1.0, 1.0] {
                let e = fr.c + fr.s * side * fr.half;
                two_sided(mesh, &[at(e, fr.z_b), at(e, fr.z_e), at(e - fr.f * t_s, z_back)], Surface::Wall);
            }
            let (el, er) = (fr.c - fr.s * (fr.half + oh), fr.c + fr.s * (fr.half + oh));
            let drop = oh * tan;
            slab(
                mesh,
                &[
                    at(el + fr.f * oh, fr.z_e - drop),
                    at(er + fr.f * oh, fr.z_e - drop),
                    at(er - fr.f * t_s, z_back),
                    at(el - fr.f * t_s, z_back),
                ],
                d.thickness.mm(),
            );
        }
    }
}
