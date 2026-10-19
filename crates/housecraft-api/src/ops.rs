//! Implementations of the heavier commands.

use std::collections::BTreeSet;

use housecraft_geom::math::{EPS, closest_on_segment, line_intersection, perp};
use housecraft_geom::{DVec2, Transform2, pt, v2};
use housecraft_model::*;
use housecraft_units::{Length, Point2};

use crate::ApiError;
use crate::command::{Outcome, TransformOp};

pub(crate) fn invalid<T>(msg: impl Into<String>) -> Result<T, ApiError> {
    Err(ApiError::Invalid(msg.into()))
}

pub(crate) fn positive(len: Length, what: &str) -> Result<(), ApiError> {
    if len.is_finite() && len.mm() > 0.0 {
        Ok(())
    } else {
        invalid(format!("{what} must be positive"))
    }
}

/// Look up an object of type `T`, with a precise error if it is missing or another kind.
pub(crate) fn expect<T: ObjectData>(doc: &Document, id: Id) -> Result<&T, ApiError> {
    match doc.object(id) {
        None => Err(ApiError::NotFound(id)),
        Some(o) => T::from_ref(o).ok_or_else(|| ApiError::WrongKind {
            id,
            expected: format!("a {}", T::KIND),
            actual: o.kind(),
        }),
    }
}

pub(crate) fn resolve_floor(doc: &Document, floor: Option<Id>) -> Result<Id, ApiError> {
    match floor {
        Some(f) => expect::<Floor>(doc, f).map(|f| f.id),
        None => doc
            .floors_by_elevation()
            .first()
            .map(|f| f.id)
            .ok_or_else(|| ApiError::Invalid("the document has no floors; add one first".into())),
    }
}

/// Where a drafting object goes: a floor plan, an elevation or section
/// view, a CAD detail, or (when `block` is set) a block; the first floor when
/// `None`.
pub(crate) fn resolve_sheet(doc: &Document, sheet: Option<Id>, block: bool) -> Result<Id, ApiError> {
    match sheet {
        Some(s) if doc.is_sheet(s) => Ok(s),
        Some(s) if block && doc.kind_of(s) == Some(ObjectKind::Block) => Ok(s),
        Some(s) => match doc.kind_of(s) {
            Some(k) => invalid(format!(
                "{s} is a {k}; drafting goes on a floor, an elevation or section view, or a CAD detail"
            )),
            None => Err(ApiError::NotFound(s)),
        },
        None => resolve_floor(doc, None),
    }
}

pub(crate) fn resolve_wall_type(
    doc: &Document,
    d: &DefaultSet,
    wall_type: Option<Id>,
    interior: bool,
) -> Result<Id, ApiError> {
    let id = wall_type.unwrap_or(if interior {
        d.interior_wall_type
    } else {
        d.exterior_wall_type
    });
    expect::<WallType>(doc, id).map(|t| t.id)
}

pub(crate) fn add_wall(
    doc: &mut Document,
    floor: Id,
    start: Point2,
    end: Point2,
    wall_type: Id,
    height: Option<Length>,
    layer: Id,
) -> Result<Id, ApiError> {
    if start.distance(end).mm() < 1.0 {
        return invalid("wall is shorter than 1 mm");
    }
    Ok(doc.insert(Wall {
        id: Id(0),
        floor,
        start,
        end,
        wall_type,
        height,
        layer,
        footing: None,
        roof: Default::default(),
        renovation: Default::default(),
    }))
}

/// Move a wall's endpoints; walls joined at a moved endpoint follow when `drag`.
/// Openings keep their distance from the wall start, clamped to stay inside.
pub(crate) fn move_wall_ends(
    doc: &mut Document,
    wall: &Wall,
    start: Point2,
    end: Point2,
    drag: bool,
    out: &mut Outcome,
) {
    let moves = [(wall.start, start), (wall.end, end)];
    if drag {
        let joined: Vec<Wall> = doc
            .walls_on(wall.floor)
            .filter(|w| w.id != wall.id)
            .cloned()
            .collect();
        for other in joined {
            let mut changed = other.clone();
            for (old, new) in moves {
                if old == new {
                    continue;
                }
                if other.start.distance(old).mm() < EPS {
                    changed.start = new;
                }
                if other.end.distance(old).mm() < EPS {
                    changed.end = new;
                }
            }
            if changed != other {
                set_wall_keeping_openings(doc, &other, changed.start, changed.end, out);
            }
        }
    }
    set_wall_keeping_openings(doc, wall, start, end, out);
}

fn set_wall_keeping_openings(
    doc: &mut Document,
    wall: &Wall,
    start: Point2,
    end: Point2,
    out: &mut Outcome,
) {
    // Keep openings anchored to the end that did not move.
    let start_moved = wall.start != start;
    let old_len = wall.length().mm();
    let new_len = start.distance(end).mm();
    let opening_ids: Vec<Id> = doc.openings_in(wall.id).map(|o| o.id).collect();
    for id in opening_ids {
        if let Some(o) = doc.get_mut::<Opening>(id) {
            if start_moved && wall.end == end {
                o.position = Length::from_mm(o.position.mm() + new_len - old_len);
            }
            out.modified.push(id);
        }
    }
    if let Some(w) = doc.get_mut::<Wall>(wall.id) {
        w.start = start;
        w.end = end;
        out.modified.push(wall.id);
    }
}

/// Resolve where along a wall an opening goes (distance from start to centre).
pub(crate) fn opening_position(
    wall: &Wall,
    position: Option<Length>,
    at: Option<Point2>,
    width: Length,
) -> Result<Length, ApiError> {
    positive(width, "width")?;
    let len = wall.length().mm();
    let pos = match (position, at) {
        (Some(_), Some(_)) => return invalid("give either position or at, not both"),
        (Some(p), None) => p.mm(),
        (None, Some(p)) => {
            let (_, t) = closest_on_segment(v2(p), v2(wall.start), v2(wall.end));
            t * len
        }
        (None, None) => len / 2.0,
    };
    let half = width.mm() / 2.0;
    if width.mm() > len {
        return invalid(format!(
            "a {:.0} mm opening does not fit in a {:.0} mm wall",
            width.mm(),
            len
        ));
    }
    // Snap inside the wall when placed by point near an end.
    Ok(Length::from_mm(pos.clamp(half, len - half)))
}

pub(crate) fn exterior_dimensions(
    doc: &mut Document,
    floor: Id,
    d: &DefaultSet,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let regions = housecraft_geom::rooms::detect(doc, floor);
    if regions.footprints.is_empty() {
        return invalid("no enclosed building outline on this floor");
    }
    let off = d.dimensions.exterior_offset.mm();
    for fp in &regions.footprints {
        let (lo, hi) = housecraft_geom::math::bounds(fp.iter().copied()).expect("non-empty");
        let mut xs: Vec<f64> = fp.iter().map(|p| p.x).collect();
        let mut ys: Vec<f64> = fp.iter().map(|p| p.y).collect();
        for v in [&mut xs, &mut ys] {
            v.sort_by(f64::total_cmp);
            v.dedup_by(|a, b| (*a - *b).abs() < EPS);
        }
        let mut add = |points: Vec<Point2>, orientation: DimOrientation, offset: f64| {
            out.created.push(doc.insert(Dimension {
                id: Id(0),
                floor,
                layer: d.layers.dimensions,
                geometry: DimensionGeometry::Linear {
                    points,
                    orientation,
                    offset: Length::from_mm(offset),
                },
                text_override: None,
            }));
        };
        let row = |xs: &[f64], y: f64| {
            xs.iter()
                .map(|x| Point2::from_mm(*x, y))
                .collect::<Vec<_>>()
        };
        let col = |ys: &[f64], x: f64| {
            ys.iter()
                .map(|y| Point2::from_mm(x, *y))
                .collect::<Vec<_>>()
        };
        let ends = |v: &[f64]| vec![v[0], v[v.len() - 1]];
        // Detail tier, then overall tier further out, on all four sides.
        for (tier, scale) in [(false, 1.0), (true, 1.6)] {
            let pick = |v: &Vec<f64>| if tier { ends(v) } else { v.clone() };
            if tier || xs.len() > 2 {
                add(
                    row(&pick(&xs), lo.y),
                    DimOrientation::Horizontal,
                    -off * scale,
                );
                add(
                    row(&pick(&xs), hi.y),
                    DimOrientation::Horizontal,
                    off * scale,
                );
            }
            if tier || ys.len() > 2 {
                add(
                    col(&pick(&ys), lo.x),
                    DimOrientation::Vertical,
                    -off * scale,
                );
                add(col(&pick(&ys), hi.x), DimOrientation::Vertical, off * scale);
            }
        }
    }
    Ok(())
}

/// JSON merge patch (RFC 7386) of `patch` into `target`.
fn merge_patch(target: &mut serde_json::Value, patch: &serde_json::Value) {
    if let serde_json::Value::Object(p) = patch {
        if !target.is_object() {
            *target = serde_json::Value::Object(Default::default());
        }
        let t = target.as_object_mut().expect("object");
        for (k, v) in p {
            if v.is_null() {
                t.insert(k.clone(), serde_json::Value::Null);
            } else {
                merge_patch(t.entry(k.clone()).or_insert(serde_json::Value::Null), v);
            }
        }
    } else {
        *target = patch.clone();
    }
}

pub(crate) fn update(
    doc: &mut Document,
    id: Id,
    set: &serde_json::Map<String, serde_json::Value>,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let object = doc.object(id).ok_or(ApiError::NotFound(id))?;
    for forbidden in ["id", "kind"] {
        if set.contains_key(forbidden) {
            return invalid(format!("`{forbidden}` cannot be changed"));
        }
    }
    let kind = object.kind();
    let mut value = serde_json::to_value(object).expect("objects serialize");
    merge_patch(&mut value, &serde_json::Value::Object(set.clone()));
    let updated: Object = serde_json::from_value(value)
        .map_err(|e| ApiError::Invalid(format!("invalid {kind} properties: {e}")))?;
    debug_assert_eq!(updated.kind(), kind);
    doc.objects.insert(id, updated);
    out.modified.push(id);
    Ok(())
}

fn to_transform(op: &TransformOp) -> Result<Transform2, ApiError> {
    Ok(match op {
        TransformOp::Translate { dx, dy } => Transform2::translate(DVec2::new(dx.mm(), dy.mm())),
        TransformOp::Rotate { center, angle } => Transform2::rotate(v2(*center), *angle),
        TransformOp::Mirror { a, b } => Transform2::mirror(v2(*a), v2(*b))
            .ok_or_else(|| ApiError::Invalid("mirror line needs two distinct points".into()))?,
        TransformOp::Scale { center, factor } => {
            if !(factor.is_finite() && *factor > 0.0) {
                return invalid("scale factor must be positive");
            }
            Transform2::scale(v2(*center), *factor)
        }
    })
}

/// Apply `t` to a placed object (not openings or rooms). Returns the new object.
fn transformed(doc: &Document, object: &Object, t: &Transform2) -> Result<Object, ApiError> {
    Ok(match object {
        Object::Wall(w) => {
            let mut w = w.clone();
            let (s, e) = (t.apply_point(w.start), t.apply_point(w.end));
            // Keep the exterior face on the same side after a reflection.
            if t.is_reflection() {
                (w.start, w.end) = (e, s);
            } else {
                (w.start, w.end) = (s, e);
            }
            Object::Wall(w)
        }
        Object::Cad(c) => Object::Cad(Cad {
            shape: t.apply_shape(&c.shape),
            ..c.clone()
        }),
        Object::Dimension(d) => Object::Dimension(Dimension {
            geometry: t.apply_dimension(&d.geometry),
            ..d.clone()
        }),
        Object::Stair(st) => {
            let mut st = st.clone();
            st.start = t.apply_point(st.start);
            st.direction = t.apply_angle(st.direction);
            if t.is_reflection() {
                let flip = |turn: &mut Turn| {
                    *turn = match turn {
                        Turn::Left => Turn::Right,
                        Turn::Right => Turn::Left,
                    }
                };
                match &mut st.shape {
                    StairShape::L { turn, .. } | StairShape::U { turn, .. } => flip(turn),
                    StairShape::Straight => {}
                }
                // Mirroring also swaps which side each railing is on.
                std::mem::swap(&mut st.railing_left, &mut st.railing_right);
            }
            Object::Stair(st)
        }
        Object::Electrical(e) => {
            let mut e = e.clone();
            e.position = t.apply_point(e.position);
            e.rotation = t.apply_angle(e.rotation);
            Object::Electrical(e)
        }
        Object::Schedule(s) => {
            let mut s = s.clone();
            s.position = t.apply_point(s.position);
            Object::Schedule(s)
        }
        Object::SectionView(v) => {
            let mut v = v.clone();
            v.a = t.apply_point(v.a);
            v.b = t.apply_point(v.b);
            if t.is_reflection() {
                // Keep looking at the same (mirrored) side.
                std::mem::swap(&mut v.a, &mut v.b);
            }
            Object::SectionView(v)
        }
        Object::Callout(c) => {
            let mut c = c.clone();
            c.at = t.apply_point(c.at);
            c.target = c.target.map(|p| t.apply_point(p));
            Object::Callout(c)
        }
        Object::RevisionCloud(c) => {
            let mut c = c.clone();
            c.points = c.points.iter().map(|p| t.apply_point(*p)).collect();
            Object::RevisionCloud(c)
        }
        Object::RoofPlane(r) => {
            let mut r = r.clone();
            for p in r.outline.iter_mut() {
                let [x, y, z] = p.mm();
                let q = t.apply(DVec2::new(x, y));
                *p = housecraft_units::Point3::from_mm(q.x, q.y, z);
            }
            Object::RoofPlane(r)
        }
        Object::Cabinet(c) => {
            let mut c = c.clone();
            c.position = t.apply_point(c.position);
            c.facing = t.apply_angle(c.facing);
            if t.is_reflection() {
                // Left and right swap in a mirror image.
                std::mem::swap(&mut c.filler_left, &mut c.filler_right);
                if let CabinetShape::Angled { left, right } = c.shape {
                    c.shape = CabinetShape::Angled { left: right, right: left };
                }
                for row in &mut c.face {
                    row.items.reverse();
                    for item in &mut row.items {
                        *item = match *item {
                            FaceItem::DoorLeft => FaceItem::DoorRight,
                            FaceItem::DoorRight => FaceItem::DoorLeft,
                            other => other,
                        };
                    }
                }
            }
            let k = t.scale_factor();
            if (k - 1.0).abs() > 1e-9 {
                c.width = c.width * k;
                c.depth = c.depth * k;
            }
            Object::Cabinet(c)
        }
        Object::Appliance(a) => {
            let mut a = a.clone();
            a.position = t.apply_point(a.position);
            a.facing = t.apply_angle(a.facing);
            Object::Appliance(a)
        }
        Object::Countertop(c) => {
            let mut c = c.clone();
            c.points = c.points.iter().map(|p| t.apply_point(*p)).collect();
            if t.is_reflection() {
                c.points.reverse();
            }
            for h in &mut c.holes {
                h.center = t.apply_point(h.center);
                h.rotation = t.apply_angle(h.rotation);
                h.width = t.apply_length(h.width);
                h.depth = t.apply_length(h.depth);
            }
            Object::Countertop(c)
        }
        Object::BlockRef(b) => {
            let mut b = b.clone();
            let flip = |m: bool| if m { 180.0 } else { 0.0 };
            let mirrored = b.mirrored != t.is_reflection();
            b.rotation = t.apply_angle(b.rotation + flip(b.mirrored)) - flip(mirrored);
            b.mirrored = mirrored;
            b.at = t.apply_point(b.at);
            b.scale *= t.scale_factor();
            Object::BlockRef(b)
        }
        Object::Text(x) => {
            let mut x = x.clone();
            x.position = t.apply_point(x.position);
            if !t.is_reflection() {
                x.rotation = t.apply_angle(x.rotation);
            }
            if let Some(h) = x.height {
                x.height = Some(t.apply_length(h));
            }
            Object::Text(x)
        }
        other => {
            let _ = doc;
            return invalid(format!("a {} cannot be transformed", other.kind()));
        }
    })
}

/// Openings of a wall after the wall is transformed by `t`.
fn transformed_openings(old: &Wall, openings: &[Opening], t: &Transform2) -> Vec<Opening> {
    let k = t.scale_factor();
    let new_len = old.length().mm() * k;
    openings
        .iter()
        .map(|o| {
            let mut o = o.clone();
            let p = o.position.mm() * k;
            if t.is_reflection() {
                o.position = Length::from_mm(new_len - p);
                o.hinge = match o.hinge {
                    Hinge::Left => Hinge::Right,
                    Hinge::Right => Hinge::Left,
                };
            } else {
                o.position = Length::from_mm(p);
            }
            o
        })
        .collect()
}

pub(crate) fn transform(
    doc: &mut Document,
    ids: &[Id],
    op: &TransformOp,
    copies: u32,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let t = to_transform(op)?;
    let selected: BTreeSet<Id> = ids.iter().copied().collect();
    let mut targets: Vec<Object> = Vec::new();
    let mut lone_openings: Vec<Opening> = Vec::new();
    for id in &selected {
        let object = doc.object(*id).ok_or(ApiError::NotFound(*id))?;
        match object {
            Object::Room(_) => {} // rooms follow their walls
            Object::Opening(o) if selected.contains(&o.wall) => {} // moves with its wall
            Object::Opening(o) => lone_openings.push(o.clone()),
            o if o.kind().is_placed() => targets.push(o.clone()),
            o => return invalid(format!("a {} cannot be transformed", o.kind())),
        }
    }

    if copies == 0 {
        for object in &targets {
            let new = transformed(doc, object, &t)?;
            if let Object::Wall(old) = object {
                let openings: Vec<Opening> = doc.openings_in(old.id).cloned().collect();
                for o in transformed_openings(old, &openings, &t) {
                    out.modified.push(o.id);
                    doc.objects.insert(o.id, Object::Opening(o));
                }
            }
            out.modified.push(new.id());
            doc.objects.insert(new.id(), new);
        }
        for o in lone_openings {
            slide_opening(doc, &o, &t, out)?;
        }
        return Ok(());
    }

    if !lone_openings.is_empty() {
        return invalid("copy doors and windows together with their wall");
    }
    let mut step = t;
    for _ in 0..copies {
        for object in &targets {
            let new = transformed(doc, object, &step)?;
            let new_id = doc.insert(new);
            out.created.push(new_id);
            if let Object::Wall(old) = object {
                let openings: Vec<Opening> = doc.openings_in(old.id).cloned().collect();
                for mut o in transformed_openings(old, &openings, &step) {
                    o.wall = new_id;
                    out.created.push(doc.insert(o));
                }
            }
        }
        step = step.then(t);
    }
    Ok(())
}

/// Move a door or window along its wall by the along-wall part of a translation.
fn slide_opening(
    doc: &mut Document,
    o: &Opening,
    t: &Transform2,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    if t.m != housecraft_geom::DMat2::IDENTITY {
        return invalid("doors and windows can only be moved along their wall");
    }
    let wall = expect::<Wall>(doc, o.wall)?;
    let dir = (v2(wall.end) - v2(wall.start)).normalize_or_zero();
    let along = t.t.dot(dir);
    if let Some(op) = doc.get_mut::<Opening>(o.id) {
        op.position = Length::from_mm(op.position.mm() + along);
    }
    out.modified.push(o.id);
    Ok(())
}

/// A straight thing that line operations act on.
#[derive(Clone, Copy)]
enum LineRef {
    Cad(Id),
    Wall(Id),
}

fn line_ref(doc: &Document, id: Id) -> Result<(LineRef, DVec2, DVec2), ApiError> {
    match doc.object(id).ok_or(ApiError::NotFound(id))? {
        Object::Wall(w) => Ok((LineRef::Wall(id), v2(w.start), v2(w.end))),
        Object::Cad(Cad {
            shape: Shape::Line { a, b, .. },
            ..
        }) => Ok((LineRef::Cad(id), v2(*a), v2(*b))),
        o => Err(ApiError::WrongKind {
            id,
            expected: "a wall or a CAD line".into(),
            actual: o.kind(),
        }),
    }
}

fn set_line(doc: &mut Document, r: LineRef, a: DVec2, b: DVec2, out: &mut Outcome) {
    match r {
        LineRef::Wall(id) => {
            let w = doc.get::<Wall>(id).expect("checked").clone();
            set_wall_keeping_openings(doc, &w, pt(a), pt(b), out);
        }
        LineRef::Cad(id) => {
            if let Some(Cad {
                shape: Shape::Line { a: pa, b: pb, .. },
                ..
            }) = doc.get_mut::<Cad>(id)
            {
                *pa = pt(a);
                *pb = pt(b);
            }
            out.modified.push(id);
        }
    }
}

/// Move whichever end of `a-b` is nearer to `p` onto `p`. Returns (far end, new near).
fn snap_near_end(a: DVec2, b: DVec2, p: DVec2) -> (DVec2, DVec2, bool) {
    if a.distance(p) < b.distance(p) {
        (b, p, true)
    } else {
        (a, p, false)
    }
}

fn with_near(a: DVec2, b: DVec2, p: DVec2, near: DVec2) -> (DVec2, DVec2) {
    let (_, _, a_is_near) = snap_near_end(a, b, p);
    if a_is_near { (near, b) } else { (a, near) }
}

pub(crate) fn extend_to(
    doc: &mut Document,
    id: Id,
    boundary: Id,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let (r, a, b) = line_ref(doc, id)?;
    let (_, c, d) = line_ref(doc, boundary)?;
    let p = line_intersection(a, b - a, c, d - c)
        .ok_or_else(|| ApiError::Invalid("the lines are parallel".into()))?;
    let (na, nb) = with_near(a, b, p, p);
    if na.distance(nb) < 1.0 {
        return invalid("the result would have zero length");
    }
    set_line(doc, r, na, nb, out);
    Ok(())
}

fn corner(doc: &Document, a: Id, b: Id) -> Result<(LineRef, LineRef, [DVec2; 4], DVec2), ApiError> {
    let (ra, a0, a1) = line_ref(doc, a)?;
    let (rb, b0, b1) = line_ref(doc, b)?;
    let p = line_intersection(a0, a1 - a0, b0, b1 - b0)
        .ok_or_else(|| ApiError::Invalid("the lines are parallel".into()))?;
    Ok((ra, rb, [a0, a1, b0, b1], p))
}

pub(crate) fn fillet(
    doc: &mut Document,
    a: Id,
    b: Id,
    radius: Length,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let (ra, rb, [a0, a1, b0, b1], p) = corner(doc, a, b)?;
    let far_a = if a0.distance(p) > a1.distance(p) {
        a0
    } else {
        a1
    };
    let far_b = if b0.distance(p) > b1.distance(p) {
        b0
    } else {
        b1
    };
    let r = radius.mm();
    if r <= 0.0 {
        let (na0, na1) = with_near(a0, a1, p, p);
        let (nb0, nb1) = with_near(b0, b1, p, p);
        set_line(doc, ra, na0, na1, out);
        set_line(doc, rb, nb0, nb1, out);
        return Ok(());
    }
    if matches!(ra, LineRef::Wall(_)) || matches!(rb, LineRef::Wall(_)) {
        return invalid(
            "walls can only be filleted with radius 0 (curved walls are not supported yet)",
        );
    }
    let ua = (far_a - p).normalize_or_zero();
    let ub = (far_b - p).normalize_or_zero();
    let theta = ua.angle_to(ub).abs() / 2.0;
    if theta < 1e-6 {
        return invalid("the lines overlap");
    }
    let tangent = r / theta.tan();
    if tangent > far_a.distance(p) || tangent > far_b.distance(p) {
        return invalid("the radius is too large for these lines");
    }
    let ta = p + ua * tangent;
    let tb = p + ub * tangent;
    let center = p + (ua + ub).normalize() * (r / theta.sin());
    let start = (ta - center).to_angle().to_degrees();
    let mut sweep = (ta - center).angle_to(tb - center).to_degrees();
    if sweep.abs() > 180.0 {
        sweep -= 360.0 * sweep.signum();
    }
    let (na0, na1) = with_near(a0, a1, p, ta);
    let (nb0, nb1) = with_near(b0, b1, p, tb);
    set_line(doc, ra, na0, na1, out);
    set_line(doc, rb, nb0, nb1, out);
    let template = doc.get::<Cad>(a).expect("a is a CAD line").clone();
    out.created.push(doc.insert(Cad {
        shape: Shape::Arc {
            center: pt(center),
            radius,
            start_angle: start,
            sweep,
        },
        ..template
    }));
    Ok(())
}

pub(crate) fn chamfer(
    doc: &mut Document,
    a: Id,
    b: Id,
    distance: Length,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let (ra, rb, [a0, a1, b0, b1], p) = corner(doc, a, b)?;
    if matches!(ra, LineRef::Wall(_)) || matches!(rb, LineRef::Wall(_)) {
        return invalid("chamfer works on CAD lines");
    }
    let far_a = if a0.distance(p) > a1.distance(p) {
        a0
    } else {
        a1
    };
    let far_b = if b0.distance(p) > b1.distance(p) {
        b0
    } else {
        b1
    };
    let dd = distance.mm();
    if dd >= far_a.distance(p) || dd >= far_b.distance(p) {
        return invalid("the chamfer is longer than the lines");
    }
    let ta = p + (far_a - p).normalize() * dd;
    let tb = p + (far_b - p).normalize() * dd;
    let (na0, na1) = with_near(a0, a1, p, ta);
    let (nb0, nb1) = with_near(b0, b1, p, tb);
    set_line(doc, ra, na0, na1, out);
    set_line(doc, rb, nb0, nb1, out);
    let template = doc.get::<Cad>(a).expect("a is a CAD line").clone();
    out.created.push(doc.insert(Cad {
        shape: Shape::Line {
            a: pt(ta),
            b: pt(tb),
            arrow_start: false,
            arrow_end: false,
        },
        ..template
    }));
    Ok(())
}

pub(crate) fn offset(
    doc: &mut Document,
    id: Id,
    distance: Length,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let dist = distance.mm();
    match doc.object(id).ok_or(ApiError::NotFound(id))?.clone() {
        Object::Wall(w) => {
            let n = perp((v2(w.end) - v2(w.start)).normalize_or_zero()) * dist;
            let new = Wall {
                start: pt(v2(w.start) + n),
                end: pt(v2(w.end) + n),
                ..w
            };
            out.created.push(doc.insert(new));
        }
        Object::Cad(c) => {
            let shape = match &c.shape {
                Shape::Line {
                    a,
                    b,
                    arrow_start,
                    arrow_end,
                } => {
                    let n = perp((v2(*b) - v2(*a)).normalize_or_zero()) * dist;
                    Shape::Line {
                        a: pt(v2(*a) + n),
                        b: pt(v2(*b) + n),
                        arrow_start: *arrow_start,
                        arrow_end: *arrow_end,
                    }
                }
                Shape::Polyline { points, closed } => Shape::Polyline {
                    points: offset_polyline(
                        &points.iter().map(|p| v2(*p)).collect::<Vec<_>>(),
                        *closed,
                        dist,
                    )
                    .into_iter()
                    .map(pt)
                    .collect(),
                    closed: *closed,
                },
                Shape::Circle { center, radius } => {
                    let r = radius.mm() + dist;
                    if r <= 0.0 {
                        return invalid("offset would make the radius non-positive");
                    }
                    Shape::Circle {
                        center: *center,
                        radius: Length::from_mm(r),
                    }
                }
                Shape::Arc {
                    center,
                    radius,
                    start_angle,
                    sweep,
                } => {
                    let r = radius.mm() + dist;
                    if r <= 0.0 {
                        return invalid("offset would make the radius non-positive");
                    }
                    Shape::Arc {
                        center: *center,
                        radius: Length::from_mm(r),
                        start_angle: *start_angle,
                        sweep: *sweep,
                    }
                }
                _ => return invalid("offset works on lines, polylines, circles, arcs and walls"),
            };
            out.created.push(doc.insert(Cad { shape, ..c }));
        }
        o => {
            return Err(ApiError::WrongKind {
                id,
                expected: "a wall or CAD shape".into(),
                actual: o.kind(),
            });
        }
    }
    Ok(())
}

fn offset_polyline(points: &[DVec2], closed: bool, dist: f64) -> Vec<DVec2> {
    let n = points.len();
    let seg_dir = |i: usize| (points[(i + 1) % n] - points[i]).normalize_or_zero();
    (0..n)
        .map(|i| {
            let has_prev = closed || i > 0;
            let has_next = closed || i + 1 < n;
            let prev = (i + n - 1) % n;
            match (has_prev, has_next) {
                (true, true) => {
                    let (d0, d1) = (seg_dir(prev), seg_dir(i));
                    let a = points[i] + perp(d0) * dist;
                    let b = points[i] + perp(d1) * dist;
                    line_intersection(a, d0, b, d1).unwrap_or(b)
                }
                (false, true) => points[i] + perp(seg_dir(i)) * dist,
                (true, false) => points[i] + perp(seg_dir(prev)) * dist,
                (false, false) => points[i],
            }
        })
        .collect()
}

pub(crate) fn delete(doc: &mut Document, ids: &[Id], out: &mut Outcome) -> Result<(), ApiError> {
    let mut doomed: BTreeSet<Id> = BTreeSet::new();
    for id in ids {
        let object = doc.object(*id).ok_or(ApiError::NotFound(*id))?;
        doomed.insert(*id);
        if let Object::Wall(_) = object {
            doomed.extend(doc.openings_in(*id).map(|o| o.id));
        }
    }
    // What is drawn on a doomed floor, view, detail or block goes with it, as
    // do the placed copies of a doomed block; repeat for views on a floor.
    loop {
        let more: Vec<Id> = doc
            .objects
            .values()
            .filter(|o| !doomed.contains(&o.id()))
            .filter(|o| {
                doc.floor_of(o.id()).is_some_and(|f| doomed.contains(&f))
                    || matches!(o, Object::BlockRef(b) if doomed.contains(&b.block))
                    || matches!(o, Object::MaterialAssignment(a)
                        if doomed.contains(&a.object) || doomed.contains(&a.material))
            })
            .map(Object::id)
            .collect();
        if more.is_empty() {
            break;
        }
        doomed.extend(more);
    }
    // Layout boxes go with their layout and with whatever they show.
    for b in doc.iter::<LayoutBox>() {
        let shows_doomed = match &b.source {
            LayoutSource::Plan {
                floor, layer_set, ..
            } => doomed.contains(floor) || layer_set.is_some_and(|s| doomed.contains(&s)),
            LayoutSource::View { view } => doomed.contains(view),
            LayoutSource::Detail { detail } => doomed.contains(detail),
            LayoutSource::Schedule { floor, .. } => floor.is_some_and(|f| doomed.contains(&f)),
            _ => false,
        };
        if doomed.contains(&b.layout) || shows_doomed {
            doomed.insert(b.id);
        }
    }
    if doc.count::<Floor>()
        == doomed
            .iter()
            .filter(|i| doc.kind_of(**i) == Some(ObjectKind::Floor))
            .count()
    {
        return invalid("a document must keep at least one floor");
    }
    for id in &doomed {
        doc.remove(*id);
        out.deleted.push(*id);
    }
    // Drop dangling soft references that have a sensible fallback.
    for object in doc.objects.values_mut() {
        match object {
            Object::LayerSet(s) => s.layers.retain(|l, _| !doomed.contains(l)),
            Object::PlanView(v) => {
                if v.floor.is_some_and(|f| doomed.contains(&f)) {
                    v.floor = None;
                }
                v.references.retain(|f| !doomed.contains(f));
            }
            Object::Layer(l) => {
                if l.text_style.is_some_and(|s| doomed.contains(&s)) {
                    l.text_style = None;
                }
            }
            Object::Text(t) if t.style.is_some_and(|s| doomed.contains(&s)) => {
                t.style = None;
            }
            Object::Electrical(e) => e.controls.retain(|c| !doomed.contains(c)),
            Object::DefaultSet(d) => d.materials.retain(|_, m| !doomed.contains(m)),
            _ => {}
        }
    }
    let vd = &mut doc.settings.view_defaults;
    for set in [&mut vd.elevation, &mut vd.section, &mut vd.detail] {
        if set.is_some_and(|s| doomed.contains(&s)) {
            *set = None;
        }
    }
    Ok(())
}

pub(crate) fn insert_objects(
    doc: &mut Document,
    objects: &[serde_json::Value],
    floor: Option<Id>,
    dx: Length,
    dy: Length,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    if let Some(f) = floor {
        resolve_sheet(doc, Some(f), false)?;
    }
    let shift = Transform2::translate(DVec2::new(dx.mm(), dy.mm()));
    let mut parsed: Vec<Object> = objects
        .iter()
        .map(|v| {
            serde_json::from_value::<Object>(v.clone())
                .map_err(|e| ApiError::Invalid(format!("invalid object: {e}")))
        })
        .collect::<Result<_, _>>()?;
    // Walls first so their doors and windows can be re-pointed.
    parsed.sort_by_key(|o| !matches!(o, Object::Wall(_)));
    let mut remap = std::collections::BTreeMap::new();
    for object in parsed {
        let old = object.id();
        let mut object = match object {
            Object::Opening(mut o) => {
                o.wall = *remap.get(&o.wall).unwrap_or(&o.wall);
                Object::Opening(o)
            }
            o if o.kind().is_placed() => transformed(doc, &o, &shift)?,
            o => o,
        };
        if let Some(f) = floor {
            match &mut object {
                Object::Wall(w) => w.floor = f,
                Object::Cad(c) => c.floor = f,
                Object::Dimension(d) => d.floor = f,
                Object::Text(t) => t.floor = f,
                Object::RoofPlane(r) => r.floor = f,
                Object::Stair(s) => s.floor = f,
                Object::Electrical(e) => e.floor = f,
                Object::SectionView(v) => v.floor = f,
                Object::Callout(c) => c.floor = f,
                Object::RevisionCloud(c) => c.floor = f,
                Object::Schedule(s) => s.floor = f,
                Object::BlockRef(b) => b.floor = f,
                Object::Cabinet(c) => c.floor = f,
                Object::Countertop(c) => c.floor = f,
                Object::Appliance(a) => a.floor = f,
                _ => {}
            }
        }
        let new = doc.insert(object);
        remap.insert(old, new);
        out.created.push(new);
    }
    // Pasted switches control the pasted copies of their fixtures; controls
    // that were not pasted along are dropped.
    for new in &out.created {
        if let Some(e) = doc.get_mut::<Electrical>(*new) {
            e.controls = e
                .controls
                .iter()
                .filter_map(|c| remap.get(c).copied())
                .collect();
        }
    }
    Ok(())
}

/// Move every object on floor `a` to `b` and vice versa.
pub(crate) fn swap_floors(doc: &mut Document, a: Id, b: Id, out: &mut Outcome) {
    let swap = |f: &mut Id| {
        if *f == a {
            *f = b;
        } else if *f == b {
            *f = a;
        }
    };
    for object in doc.objects.values_mut() {
        let id = object.id();
        let floor = match object {
            Object::Wall(w) => &mut w.floor,
            Object::Room(r) => &mut r.floor,
            Object::Cad(c) => &mut c.floor,
            Object::Dimension(d) => &mut d.floor,
            Object::Text(t) => &mut t.floor,
            Object::RoofPlane(r) => &mut r.floor,
            Object::Stair(s) => &mut s.floor,
            Object::Electrical(e) => &mut e.floor,
            Object::SectionView(v) => &mut v.floor,
            Object::Callout(c) => &mut c.floor,
            Object::RevisionCloud(c) => &mut c.floor,
            Object::Schedule(s) => &mut s.floor,
            Object::BlockRef(b) => &mut b.floor,
            Object::Cabinet(c) => &mut c.floor,
            Object::Countertop(c) => &mut c.floor,
            Object::Appliance(a) => &mut a.floor,
            _ => continue,
        };
        let before = *floor;
        swap(floor);
        if *floor != before {
            out.modified.push(id);
        }
    }
}

/// Keep each floor's structure depth equal to its platform layers' total.
pub(crate) fn sync_platforms(doc: &mut Document, out: &mut Outcome) {
    for object in doc.objects.values_mut() {
        if let Object::Floor(f) = object
            && !f.platform.is_empty()
        {
            let total = f
                .platform
                .iter()
                .fold(Length::ZERO, |sum, l| sum + l.thickness);
            if (total - f.floor_structure).mm().abs() > 1e-6 {
                f.floor_structure = total;
                if !out.modified.contains(&f.id) {
                    out.modified.push(f.id);
                }
            }
        }
    }
}
