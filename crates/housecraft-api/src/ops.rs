//! Implementations of the heavier commands.

use std::collections::BTreeSet;

use housecraft_geom::math::{EPS, closest_on_segment, line_intersection, perp};
use housecraft_geom::{DVec2, Transform2, pt, v2};
use housecraft_model::*;
use housecraft_units::{Length, Point2, Point3};

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
        shape: Default::default(),
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
    // Replace the floor's earlier automatic dimensions.
    let old: Vec<Id> = doc.iter::<Dimension>().filter(|x| x.floor == floor && x.auto).map(|x| x.id).collect();
    for id in old {
        doc.remove(id);
        out.deleted.push(id);
    }
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
                auto: true,
                style: d.dimension_style.clone(),
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

/// Within `v`, replace `{"custom": index}` line styles with `target` and
/// shift later custom styles down by one. True if anything changed.
pub(crate) fn repoint_line_styles(v: &mut serde_json::Value, index: u32, target: &serde_json::Value) -> bool {
    use serde_json::Value;
    if let Value::Object(m) = v
        && m.len() == 1
        && let Some(Value::Number(n)) = m.get("custom")
        && let Some(k) = n.as_u64()
    {
        let k = k as u32;
        if k == index {
            *v = target.clone();
            return true;
        }
        if k > index {
            *v = serde_json::json!({ "custom": k - 1 });
            return true;
        }
        return false;
    }
    // Visit every child (no short-circuit): each may need re-pointing.
    let mut changed = false;
    match v {
        Value::Object(m) => {
            for x in m.values_mut() {
                changed |= repoint_line_styles(x, index, target);
            }
        }
        Value::Array(a) => {
            for x in a.iter_mut() {
                changed |= repoint_line_styles(x, index, target);
            }
        }
        _ => {}
    }
    changed
}

/// How many objects use line style `Custom(index)`.
pub fn line_type_uses(doc: &Document, index: u32) -> usize {
    fn uses(v: &serde_json::Value, index: u32) -> bool {
        match v {
            serde_json::Value::Object(m) => {
                (m.len() == 1 && m.get("custom").and_then(|n| n.as_u64()) == Some(index as u64))
                    || m.values().any(|x| uses(x, index))
            }
            serde_json::Value::Array(a) => a.iter().any(|x| uses(x, index)),
            _ => false,
        }
    }
    doc.objects
        .values()
        .filter(|o| serde_json::to_value(o).is_ok_and(|v| uses(&v, index)))
        .count()
}

/// Fields Match Properties leaves alone: identity, host, placement and facing.
const PLACEMENT_KEYS: &[&str] = &[
    "id", "kind", "floor", "wall", "host", "name", "label", "text", "position", "offset", "at",
    "center", "start", "end", "a", "b", "points", "eye", "target", "rotation", "angle", "facing",
    "hinge", "swing_in", "room", "camera", "mark", "elevation",
];

/// A point or a list of points: `[x, y]`, `[x, y, z]` or nested lists of them.
fn is_geometry(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Array(a) if (a.len() == 2 || a.len() == 3) && a.iter().all(|x| x.is_number()) => true,
        serde_json::Value::Array(a) => !a.is_empty() && a.iter().all(is_geometry),
        _ => false,
    }
}

pub(crate) fn match_properties(
    doc: &mut Document,
    from: Id,
    to: &[Id],
    out: &mut Outcome,
) -> Result<(), ApiError> {
    let source = doc.object(from).ok_or(ApiError::NotFound(from))?;
    let kind = source.kind();
    let value = serde_json::to_value(source).expect("objects serialize");
    let set: serde_json::Map<String, serde_json::Value> = value
        .as_object()
        .expect("objects are JSON objects")
        .iter()
        .filter(|(k, v)| !PLACEMENT_KEYS.contains(&k.as_str()) && !is_geometry(v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for id in to.iter().filter(|id| **id != from) {
        let target = doc.kind_of(*id).ok_or(ApiError::NotFound(*id))?;
        if target != kind {
            return invalid(format!("can't match a {target} to a {kind}"));
        }
        update(doc, *id, &set, out)?;
    }
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
                    StairShape::L { turn, .. } | StairShape::U { turn, .. } | StairShape::Curved { turn, .. } => flip(turn),
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
        Object::MaterialRegion(r) => {
            let mut r = r.clone();
            match &mut r.target {
                RegionTarget::Floor { points } => {
                    *points = points.iter().map(|p| t.apply_point(*p)).collect();
                    if t.is_reflection() {
                        points.reverse();
                    }
                }
                // Wall regions ride on their wall.
                RegionTarget::Wall { .. } => {}
            }
            Object::MaterialRegion(r)
        }
        Object::FramingMember(m) => {
            let mut m = m.clone();
            let move3 = |p: Point3| {
                let [x, y, z] = p.mm();
                let q = t.apply_point(Point2(Length::from_mm(x), Length::from_mm(y)));
                Point3(q.0, q.1, Length::from_mm(z))
            };
            m.a = move3(m.a);
            m.b = move3(m.b);
            let [ax, ay, az] = m.a.mm();
            let [bx, by, bz] = m.b.mm();
            let vertical = (bz - az).abs() > ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt() * 50.0;
            if vertical {
                m.rotation = t.apply_angle(m.rotation);
            }
            // Moved by hand: no longer rebuilt.
            m.manual = true;
            Object::FramingMember(m)
        }
        Object::Dormer(d) => {
            let mut d = d.clone();
            d.position = t.apply_point(d.position);
            d.facing = t.apply_angle(d.facing);
            Object::Dormer(d)
        }
        Object::Deck(dk) => {
            let mut dk = dk.clone();
            dk.outline = dk.outline.iter().map(|p| t.apply_point(*p)).collect();
            if t.is_reflection() {
                dk.outline.reverse();
            }
            dk.plank_angle = t.apply_angle(dk.plank_angle);
            Object::Deck(dk)
        }
        Object::JoistDirection(j) => {
            let mut j = j.clone();
            j.at = t.apply_point(j.at);
            j.angle = t.apply_angle(j.angle);
            Object::JoistDirection(j)
        }
        Object::SiteFeature(f) => {
            let mut f = f.clone();
            f.points = f.points.iter().map(|p| t.apply_point(*p)).collect();
            if t.is_reflection() && f.feature.closed() {
                f.points.reverse();
            }
            Object::SiteFeature(f)
        }
        Object::Plant(p) => {
            let mut p = p.clone();
            p.at = t.apply_point(p.at);
            Object::Plant(p)
        }
        Object::Terrain(ter) => {
            let mut ter = ter.clone();
            ter.perimeter = ter.perimeter.iter().map(|p| t.apply_point(*p)).collect();
            if t.is_reflection() {
                ter.perimeter.reverse();
            }
            Object::Terrain(ter)
        }
        Object::ElevationData(e) => {
            let mut e = e.clone();
            let k = t.scale_factor();
            match &mut e.shape {
                ElevationShape::Point { at, .. } => *at = t.apply_point(*at),
                ElevationShape::Line { points, .. } | ElevationShape::Region { points, .. } => {
                    *points = points.iter().map(|p| t.apply_point(*p)).collect();
                }
                ElevationShape::Hill { center, radius, .. } | ElevationShape::Valley { center, radius, .. } => {
                    *center = t.apply_point(*center);
                    *radius = *radius * k;
                }
            }
            Object::ElevationData(e)
        }
        Object::Solid(x) => {
            let mut x = x.clone();
            let k = t.scale_factor();
            match &mut x.shape {
                SolidShape::Extrude { points, height } => {
                    *points = points.iter().map(|p| t.apply_point(*p)).collect();
                    if t.is_reflection() {
                        points.reverse();
                    }
                    *height = *height * k;
                }
                SolidShape::Revolve { center, profile, .. } => {
                    *center = t.apply_point(*center);
                    profile.iter_mut().flatten().for_each(|l| *l = *l * k);
                }
            }
            Object::Solid(x)
        }
        Object::MoldingPolyline(m) => {
            let mut m = m.clone();
            m.points = m.points.iter().map(|p| t.apply_point(*p)).collect();
            if t.is_reflection() {
                m.points.reverse();
            }
            Object::MoldingPolyline(m)
        }
        Object::Backsplash(b) => {
            let mut b = b.clone();
            b.points = b.points.iter().map(|p| t.apply_point(*p)).collect();
            if t.is_reflection() {
                // Keep it on the same side of the wall.
                b.points.reverse();
            }
            Object::Backsplash(b)
        }
        Object::Symbol(s) => {
            let mut s = s.clone();
            s.position = t.apply_point(s.position);
            s.facing = t.apply_angle(s.facing);
            let k = t.scale_factor();
            s.width = s.width * k;
            s.depth = s.depth * k;
            s.height = s.height * k;
            Object::Symbol(s)
        }
        Object::Picture(p) => {
            let mut p = p.clone();
            p.position = t.apply_point(p.position);
            if !t.is_reflection() {
                p.rotation = t.apply_angle(p.rotation);
            }
            p.width = t.apply_length(p.width);
            p.height = t.apply_length(p.height);
            Object::Picture(p)
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
            for tip in &mut x.leaders {
                *tip = t.apply_point(*tip);
            }
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
                    || matches!(o, Object::Symbol(s) if doomed.contains(&s.def))
                    || matches!(o, Object::MaterialRegion(r) if doomed.contains(&r.material)
                        || matches!(r.target, RegionTarget::Wall { wall, .. } if doomed.contains(&wall)))
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
            Object::Opening(o) => {
                o.details.plan_block = o.details.plan_block.filter(|b| !doomed.contains(b));
            }
            // A note on a deleted camera shows in every 3D view.
            Object::Annotation3d(a) => a.camera = a.camera.filter(|c| !doomed.contains(c)),
            Object::Stair(s) => {
                s.baluster = s.baluster.filter(|b| !doomed.contains(b));
                s.newel = s.newel.filter(|n| !doomed.contains(n));
            }
            Object::DefaultSet(d) => d.materials.retain(|_, m| !doomed.contains(m)),
            Object::StylePalette(st) => st.materials.retain(|_, m| !doomed.contains(m)),
            // Moldings using a deleted profile fall back to a flat board.
            Object::MoldingPolyline(m) => m.stack.iter_mut().for_each(|s| unprofile(s, &doomed)),
            Object::RoomType(t) => t.moldings.iter_mut().for_each(|m| unprofile(&mut m.spec, &doomed)),
            Object::Room(r) => {
                if let Some(ms) = &mut r.moldings {
                    ms.iter_mut().for_each(|m| unprofile(&mut m.spec, &doomed));
                }
            }
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

fn unprofile(spec: &mut MoldingSpec, doomed: &BTreeSet<Id>) {
    if let ProfileRef::Custom { profile } = spec.profile
        && doomed.contains(&profile)
    {
        spec.profile = ProfileRef::default();
    }
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
    // Symbol definitions and walls first so the symbols, stairs, doors and
    // windows using them can be re-pointed.
    parsed.sort_by_key(|o| match o {
        Object::Layer(_) | Object::WallType(_) | Object::TextStyle(_) | Object::RoomType(_) => -1,
        Object::SymbolDef(_) | Object::CadDetail(_) | Object::SectionView(_) | Object::Block(_) => 0,
        Object::Wall(_) => 1,
        _ => 2,
    });
    let mut remap = std::collections::BTreeMap::new();
    for object in parsed {
        let old = object.id();
        // Layers, wall types, text and room types copied along (from another
        // plan): use the one of the same name here, else add it.
        let support_name = match &object {
            Object::Layer(x) => Some(x.name.clone()),
            Object::WallType(x) => Some(x.name.clone()),
            Object::TextStyle(x) => Some(x.name.clone()),
            Object::RoomType(x) => Some(x.name.clone()),
            _ => None,
        };
        if let Some(name) = support_name {
            let kind = object.kind();
            let same = doc
                .objects
                .values()
                .find(|o| o.kind() == kind && serde_json::to_value(o).is_ok_and(|v| v.get("name").and_then(|n| n.as_str()) == Some(name.as_str())))
                .map(|o| o.id());
            let new = match same {
                Some(id) => id,
                None => doc.insert(object),
            };
            remap.insert(old, new);
            continue;
        }
        // Point references at the layers and types matched above.
        let object = if remap.is_empty() {
            object
        } else {
            let mut v = serde_json::to_value(&object).expect("objects serialize");
            // `floor` too: drafting on a CAD detail or view pasted along.
            for key in ["layer", "wall_type", "style", "room_type", "text_style", "floor", "block"] {
                if let Some(n) = v.get(key).and_then(|n| n.as_u64())
                    && let Some(new) = remap.get(&Id(n))
                {
                    v[key] = serde_json::json!(new.0);
                }
            }
            // A layout box showing a detail or view pasted along.
            if let Some(src) = v.get_mut("source") {
                for key in ["view", "detail"] {
                    if let Some(n) = src.get(key).and_then(|n| n.as_u64())
                        && let Some(new) = remap.get(&Id(n))
                    {
                        src[key] = serde_json::json!(new.0);
                    }
                }
            }
            serde_json::from_value(v).map_err(|e| ApiError::Invalid(format!("invalid object: {e}")))?
        };
        // A symbol already in the plan is reused rather than duplicated.
        if let Object::SymbolDef(def) = &object
            && let Some(same) = doc.iter::<SymbolDef>().find(|d| {
                d.name == def.name && d.positions == def.positions && d.triangles == def.triangles
            })
        {
            remap.insert(old, same.id);
            continue;
        }
        let re = |id: Id| *remap.get(&id).unwrap_or(&id);
        let object = match object {
            Object::Symbol(mut s) => {
                s.def = re(s.def);
                Object::Symbol(s)
            }
            Object::Stair(mut s) => {
                s.baluster = s.baluster.map(re);
                s.newel = s.newel.map(re);
                Object::Stair(s)
            }
            o => o,
        };
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
                Object::Picture(p) => p.floor = f,
                Object::Symbol(s) => s.floor = f,
                Object::Backsplash(b) => b.floor = f,
                Object::MoldingPolyline(m) => m.floor = f,
                Object::Solid(x) => x.floor = f,
                Object::Terrain(t) => t.floor = f,
                Object::SiteFeature(s) => s.floor = f,
                Object::FramingMember(m) => m.floor = f,
                Object::JoistDirection(j) => j.floor = f,
                Object::Deck(dk) => dk.floor = f,
                Object::Dormer(d) => d.floor = f,
                Object::Plant(p) => p.floor = f,
                Object::ElevationData(e) => e.floor = f,
                Object::MaterialRegion(r) => r.floor = f,
                _ => {}
            }
        }
        let new = doc.insert(object);
        remap.insert(old, new);
        out.created.push(new);
    }
    // A copied elevation or section brings along the lines, text and
    // dimensions drawn on it (unless they were copied with it).
    let views: Vec<(Id, Id)> = remap
        .iter()
        .filter(|(old, new)| old != new && doc.get::<SectionView>(**new).is_some() && doc.get::<SectionView>(**old).is_some())
        .map(|(o, n)| (*o, *n))
        .collect();
    for (old, new) in views {
        let notes: Vec<Object> = doc
            .objects
            .values()
            .filter(|o| doc.floor_of(o.id()) == Some(old) && !remap.contains_key(&o.id()))
            .filter(|o| {
                matches!(
                    o,
                    Object::Cad(_) | Object::Text(_) | Object::Dimension(_) | Object::Callout(_) | Object::BlockRef(_) | Object::RevisionCloud(_)
                )
            })
            .cloned()
            .collect();
        for mut o in notes {
            match &mut o {
                Object::Cad(c) => c.floor = new,
                Object::Text(t) => t.floor = new,
                Object::Dimension(d) => d.floor = new,
                Object::Callout(c) => c.floor = new,
                Object::BlockRef(b) => b.floor = new,
                Object::RevisionCloud(c) => c.floor = new,
                _ => {}
            }
            out.created.push(doc.insert(o));
        }
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
            Object::Picture(p) => &mut p.floor,
            Object::Symbol(s) => &mut s.floor,
            Object::Backsplash(b) => &mut b.floor,
            Object::MoldingPolyline(m) => &mut m.floor,
            Object::Solid(x) => &mut x.floor,
            Object::Terrain(t) => &mut t.floor,
            Object::SiteFeature(s) => &mut s.floor,
            Object::FramingMember(m) => &mut m.floor,
            Object::JoistDirection(j) => &mut j.floor,
            Object::Deck(dk) => &mut dk.floor,
            Object::Dormer(d) => &mut d.floor,
            Object::Plant(p) => &mut p.floor,
            Object::ElevationData(e) => &mut e.floor,
            Object::MaterialRegion(r) => &mut r.floor,
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

/// Plan views for each kind of drawing, each with its own default set and
/// layer set (see [`crate::Command::AddStandardPlanViews`]).
pub(crate) fn standard_plan_views(doc: &mut Document, out: &mut Outcome) -> Result<(), ApiError> {
    let base_view = doc
        .iter::<PlanView>()
        .next()
        .cloned()
        .ok_or_else(|| ApiError::Invalid("the plan has no plan view to copy".into()))?;
    let base_set = doc
        .get::<LayerSet>(base_view.layer_set)
        .cloned()
        .ok_or_else(|| ApiError::Invalid("the plan view's layer set is missing".into()))?;
    let base_defaults = doc
        .get::<DefaultSet>(base_view.default_set)
        .cloned()
        .unwrap_or_else(|| doc.active_defaults().clone());
    let l = base_defaults.layers.clone();
    let hide = |set: &mut LayerSet, layer: Option<Id>, visible: bool| {
        if let Some(layer) = layer {
            set.layers.entry(layer).or_default().visible = visible;
        }
    };
    for name in ["Electrical Plan", "Framing Plan", "Kitchen & Bath Plan", "Construction Plan", "Site Plan"] {
        if doc.iter::<PlanView>().any(|v| v.name == name) {
            continue;
        }
        let mut set = base_set.clone();
        set.id = Id(0);
        set.name = name.to_string();
        let mut d = base_defaults.clone();
        d.id = Id(0);
        d.name = name.trim_end_matches(" Plan").to_string();
        match name {
            "Electrical Plan" => {
                hide(&mut set, Some(l.dimensions), false);
                hide(&mut set, l.electrical, true);
            }
            "Framing Plan" => {
                hide(&mut set, l.framing, true);
                hide(&mut set, Some(l.rooms), false);
                hide(&mut set, l.electrical, false);
            }
            "Kitchen & Bath Plan" => {
                hide(&mut set, l.cabinets, true);
                d.labels.cabinets = "{code}".into();
                d.labels.fixtures = "{type}".into();
            }
            "Construction Plan" => {
                hide(&mut set, Some(l.dimensions), true);
                d.labels.doors = "{size}".into();
                d.labels.windows = "{size}".into();
                d.room_labels.size = RoomSizeFormat::Exact;
            }
            _ => {
                hide(&mut set, l.electrical, false);
                hide(&mut set, l.cabinets, false);
                hide(&mut set, l.framing, false);
                hide(&mut set, Some(l.dimensions), false);
            }
        }
        let set_id = doc.insert(set);
        let d_id = doc.insert(d);
        let mut view = base_view.clone();
        view.id = Id(0);
        view.name = name.to_string();
        view.layer_set = set_id;
        view.default_set = d_id;
        out.created.push(set_id);
        out.created.push(d_id);
        out.created.push(doc.insert(view));
    }
    Ok(())
}

/// Rebuild the automatic exterior dimensions of floors whose walls or
/// openings changed (compared with `before`).
pub(crate) fn refresh_auto_dimensions(doc: &mut Document, before: &Document, out: &mut Outcome) {
    let mut floors: Vec<Id> = Vec::new();
    let changed = out.created.iter().chain(&out.modified).filter_map(|id| match doc.kind_of(*id) {
        Some(ObjectKind::Wall | ObjectKind::Opening) => doc.floor_of(*id),
        _ => None,
    });
    let gone = out.deleted.iter().filter_map(|id| match before.kind_of(*id) {
        Some(ObjectKind::Wall | ObjectKind::Opening) => before.floor_of(*id),
        _ => None,
    });
    for f in changed.chain(gone) {
        if !floors.contains(&f) {
            floors.push(f);
        }
    }
    for floor in floors {
        if !doc.iter::<Dimension>().any(|d| d.floor == floor && d.auto) {
            continue;
        }
        let d = doc.defaults_for(floor).clone();
        let mut o = Outcome::default();
        if exterior_dimensions(doc, floor, &d, &mut o).is_err() {
            // No closed outline any more: just drop the old ones.
            for id in doc.iter::<Dimension>().filter(|x| x.floor == floor && x.auto).map(|x| x.id).collect::<Vec<_>>() {
                doc.remove(id);
                o.deleted.push(id);
            }
        }
        out.merge(o);
    }
}

/// Open `wall` around `position` and build a bay, box or bow projecting
/// `depth` outward, a window in each face (see `Command::AddBayWindow`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn bay_window(
    doc: &mut Document,
    wall: Id,
    position: Length,
    width: Length,
    depth: Length,
    bay: crate::command::BayStyle,
    faces: u8,
    style: Option<WindowStyle>,
    out: &mut Outcome,
) -> Result<(), ApiError> {
    use crate::command::BayStyle;
    let w = expect::<Wall>(doc, wall)?.clone();
    let (s, e) = (v2(w.start), v2(w.end));
    let len = s.distance(e);
    let (pos, wid, dep) = (position.mm(), width.mm(), depth.mm());
    let (ua, ub) = (pos - wid / 2.0, pos + wid / 2.0);
    if ua < 1.0 || ub > len - 1.0 || dep < 50.0 {
        return invalid("the bay must fit inside the wall and project at least 50 mm");
    }
    let dir = (e - s) / len;
    let out_n = perp(dir); // the exterior side
    let (a, b) = (s + dir * ua, s + dir * ub);
    // The projecting outline from a to b.
    let pts: Vec<DVec2> = match bay {
        BayStyle::Box => vec![a, a + out_n * dep, b + out_n * dep, b],
        BayStyle::Bay => {
            let inset = dep.min(wid / 2.0 - 50.0).max(0.0);
            vec![a, a + out_n * dep + dir * inset, b + out_n * dep - dir * inset, b]
        }
        BayStyle::Bow => {
            // An arc through a, the middle pushed out by `depth`, and b.
            let n = faces.clamp(3, 12) as usize;
            let half = wid / 2.0;
            let r = (half * half + dep * dep) / (2.0 * dep);
            let center = (a + b) / 2.0 + out_n * (dep - r);
            let a0 = (a - center).to_angle();
            let a1 = (b - center).to_angle();
            let mut sweep = a1 - a0;
            while sweep > std::f64::consts::PI {
                sweep -= std::f64::consts::TAU;
            }
            while sweep < -std::f64::consts::PI {
                sweep += std::f64::consts::TAU;
            }
            (0..=n).map(|i| center + DVec2::from_angle(a0 + sweep * i as f64 / n as f64) * r).collect()
        }
    };
    // Split the wall: it keeps start..a; a new wall runs b..end with the
    // openings beyond the bay; openings inside the bay go.
    let openings: Vec<Opening> = doc.openings_in(wall).cloned().collect();
    let mut tail = w.clone();
    tail.id = Id(0);
    tail.start = pt(b);
    let tail_id = doc.insert(tail);
    out.created.push(tail_id);
    if let Some(Object::Wall(x)) = doc.objects.get_mut(&wall) {
        x.end = pt(a);
    }
    out.modified.push(wall);
    for o in openings {
        let p = o.position.mm();
        if p + o.width.mm() / 2.0 <= ua {
            continue;
        }
        if p - o.width.mm() / 2.0 >= ub
            && let Some(Object::Opening(x)) = doc.objects.get_mut(&o.id)
        {
            x.wall = tail_id;
            x.position = Length::from_mm(p - ub);
            out.modified.push(o.id);
        } else {
            doc.remove(o.id);
            out.deleted.push(o.id);
        }
    }
    // The bay's walls, each with a window.
    let layer = w.layer;
    for pair in pts.windows(2) {
        let id = add_wall(doc, w.floor, pt(pair[0]), pt(pair[1]), w.wall_type, w.height, layer)?;
        out.created.push(id);
        let face = pair[0].distance(pair[1]);
        let t = doc.get::<WallType>(w.wall_type).map_or(150.0, |t| t.thickness().mm());
        // Bow faces are glass nearly edge to edge; bay and box faces keep jambs.
        let ww = if bay == BayStyle::Bow { face - 60.0 } else { face - t - 100.0 };
        if ww > 250.0 {
            let o = crate::Command::AddWindow {
                wall: id,
                position: Some(Length::from_mm(face / 2.0)),
                at: None,
                style,
                width: Some(Length::from_mm(ww)),
                height: None,
                sill: None,
            }
            .apply(doc);
            // A face too small for a window just stays wall.
            if let Ok(o) = o {
                out.merge(o);
            }
        }
    }
    Ok(())
}

/// Re-pitch a roof plane keeping its eave (or ridge) where it is: the plan
/// outline stays and the heights follow the new pitch.
pub(crate) fn set_roof_pitch(doc: &mut Document, id: Id, pitch: f64, keep_ridge: bool, out: &mut Outcome) -> Result<(), ApiError> {
    if !(1.0..=80.0).contains(&pitch) {
        return invalid("pitch must be 1°–80°");
    }
    let r = expect::<RoofPlane>(doc, id)?.clone();
    if r.gable {
        return invalid("a gable infill has no pitch");
    }
    let pts: Vec<housecraft_geom::DVec3> = r.outline.iter().map(|q| { let [x, y, z] = q.mm(); housecraft_geom::DVec3::new(x, y, z) }).collect();
    let down = housecraft_geom::roof_extras::downslope(&pts);
    // Distance up the slope, measured in plan from the lowest (or highest) point.
    let up = |p: housecraft_geom::DVec3| -(p.truncate().dot(down));
    let pivot = if keep_ridge {
        pts.iter().copied().max_by(|a, b| a.z.total_cmp(&b.z)).expect("outline")
    } else {
        pts.iter().copied().min_by(|a, b| a.z.total_cmp(&b.z)).expect("outline")
    };
    let tan = pitch.to_radians().tan();
    let outline = pts
        .iter()
        .map(|p| Point3::from_mm(p.x, p.y, pivot.z + (up(*p) - up(pivot)) * tan))
        .collect();
    if let Some(Object::RoofPlane(x)) = doc.objects.get_mut(&id) {
        x.outline = outline;
        x.pitch = pitch;
        x.auto = false;
    }
    out.modified.push(id);
    Ok(())
}

/// Outlets round each room: every 12' at most along its walls, clear of
/// doors; GFCI in wet rooms (kitchens at counter height).
pub(crate) fn auto_outlets(doc: &mut Document, floor: Option<Id>, out: &mut Outcome) -> Result<(), ApiError> {
    let floor = match floor {
        Some(f) => f,
        None => doc
            .floors_by_elevation()
            .iter()
            .find(|f| f.foundation.is_none())
            .map(|f| f.id)
            .ok_or_else(|| ApiError::Invalid("no storey to place outlets on".into()))?,
    };
    let regions = housecraft_geom::rooms::detect(doc, floor);
    let doors: Vec<(DVec2, f64)> = housecraft_geom::walls::wall_geoms(doc, floor)
        .iter()
        .flat_map(|g| {
            doc.openings_in(g.id)
                .filter(|o| o.opening_kind.is_door())
                .map(|o| (g.center_at(o.position.mm()), o.width.mm() / 2.0 + 300.0))
                .collect::<Vec<_>>()
        })
        .collect();
    let rooms: Vec<Room> = doc.rooms_on(floor).cloned().collect();
    for room in rooms {
        let Some(i) = regions.room_at(v2(room.anchor)) else {
            continue;
        };
        let name = doc.get::<RoomType>(room.room_type).map_or(String::new(), |t| t.name.to_lowercase());
        let wet = ["kitchen", "bath", "laundry", "garage", "utility"].iter().any(|w| name.contains(w));
        let kitchen = name.contains("kitchen");
        let mut poly = regions.rooms[i].polygon.clone();
        poly.push(poly[0]);
        // Evenly along each wall, at most 12' apart, never in a corner.
        let spots: Vec<DVec2> = poly
            .windows(2)
            .flat_map(|e| {
                let len = e[0].distance(e[1]);
                let n = (len / 3657.6).ceil().max(1.0) as usize;
                (0..n).map(move |k| e[0] + (e[1] - e[0]) * ((k as f64 + 0.5) / n as f64))
            })
            .collect();
        for p in spots {
            if doors.iter().any(|(c, r)| c.distance(p) < *r) {
                continue;
            }
            // No rotation: the command snaps it to the wall, facing the room.
            let o = crate::Command::AddElectrical {
                floor: Some(floor),
                electrical_kind: if wet { ElectricalKind::GfciOutlet } else { ElectricalKind::Outlet },
                at: pt(p),
                rotation: None,
                height: kitchen.then(|| Length::from_inches(44.0)),
                controls: Vec::new(),
            }
            .apply(doc)?;
            out.merge(o);
        }
    }
    Ok(())
}

/// Walls round each closed CAD box, the rooms typed from their labels.
pub(crate) fn convert_space_plan(doc: &mut Document, ids: &[Id], out: &mut Outcome) -> Result<(), ApiError> {
    let d = doc.active_defaults().clone();
    let mut typed: Vec<(DVec2, Id)> = Vec::new();
    for id in ids {
        let Some(cad) = doc.get::<Cad>(*id).cloned() else {
            continue;
        };
        let Shape::Polyline { points, closed: true } = &cad.shape else {
            continue;
        };
        let pts: Vec<DVec2> = points.iter().map(|p| v2(*p)).collect();
        // The label inside the box names its room type.
        let label = doc
            .iter::<Text>()
            .filter(|t| t.floor == cad.floor && housecraft_geom::math::point_in_polygon(v2(t.position), &pts))
            .map(|t| (t.id, t.text.trim().to_lowercase()))
            .next();
        let room_type = label.as_ref().and_then(|(_, l)| doc.iter::<RoomType>().find(|t| t.name.to_lowercase() == *l).map(|t| t.id));
        // Walls on each side not already walled (a shared side once).
        let n = pts.len();
        for k in 0..n {
            let (a, b) = (pts[k], pts[(k + 1) % n]);
            let covered = doc.walls_on(cad.floor).any(|w| {
                let (s, e) = (v2(w.start), v2(w.end));
                let on = |p: DVec2| housecraft_geom::math::distance_to_segment(p, s, e) < 5.0;
                on(a) && on(b)
            });
            if !covered {
                let wt = d.interior_wall_type;
                out.created.push(add_wall(doc, cad.floor, pt(a), pt(b), wt, None, d.layers.walls)?);
            }
        }
        doc.remove(*id);
        out.deleted.push(*id);
        if let Some((tid, _)) = label {
            doc.remove(tid);
            out.deleted.push(tid);
        }
        if let Some(t) = room_type {
            typed.push((housecraft_geom::math::centroid(&pts), t));
        }
    }
    // Make the rooms now, then type them.
    crate::rooms::sync(doc, out);
    for (c, t) in typed {
        let found = doc.iter::<Room>().find(|r| {
            let regions = housecraft_geom::rooms::detect(doc, r.floor);
            regions.room_at(v2(r.anchor)) == regions.room_at(c) && regions.room_at(c).is_some()
        }).map(|r| r.id);
        if let Some(rid) = found
            && let Some(Object::Room(r)) = doc.objects.get_mut(&rid)
        {
            r.room_type = t;
            out.modified.push(rid);
        }
    }
    Ok(())
}

/// `text` with `find` replaced (ignoring case unless `match_case`), or
/// `None` when it does not occur.
pub fn replaced(text: &str, find: &str, replace: &str, match_case: bool) -> Option<String> {
    if find.is_empty() {
        return None;
    }
    if match_case {
        return text.contains(find).then(|| text.replace(find, replace));
    }
    let lower = text.to_lowercase();
    let needle = find.to_lowercase();
    if !lower.contains(&needle) || lower.len() != text.len() {
        // Lower-casing changed byte lengths: fall back to exact matching.
        return text.contains(find).then(|| text.replace(find, replace));
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(k) = lower[i..].find(&needle) {
        out.push_str(&text[i..i + k]);
        out.push_str(replace);
        i += k + needle.len();
    }
    out.push_str(&text[i..]);
    Some(out)
}

/// Objects whose text contains `find`: notes, callouts, room names, object
/// labels and layout notes.
pub fn text_matches(doc: &Document, find: &str, match_case: bool) -> Vec<Id> {
    let mut probe = doc.clone();
    replace_text(&mut probe, find, "", match_case, &[])
}

/// Replace text in place; returns the objects changed.
pub(crate) fn replace_text(doc: &mut Document, find: &str, replace: &str, match_case: bool, only: &[Id]) -> Vec<Id> {
    let mut changed = Vec::new();
    let wanted = |id: Id| only.is_empty() || only.contains(&id);
    let swap = |s: &mut String| match replaced(s, find, replace, match_case) {
        Some(t) => {
            *s = t;
            true
        }
        None => false,
    };
    for (id, o) in doc.objects.iter_mut() {
        if !wanted(*id) {
            continue;
        }
        let hit = match o {
            Object::Text(t) => swap(&mut t.text),
            Object::Callout(c) => swap(&mut c.text),
            Object::Room(r) => r.name.as_mut().is_some_and(swap),
            Object::LayoutBox(b) => {
                let note = match &mut b.source {
                    LayoutSource::Text { text } => swap(text),
                    _ => false,
                };
                b.label.as_mut().is_some_and(swap) | note
            }
            _ => false,
        };
        if hit {
            changed.push(*id);
        }
    }
    for (id, label) in doc.settings.labels.iter_mut() {
        if wanted(*id) && swap(label) && !changed.contains(id) {
            changed.push(*id);
        }
    }
    changed
}
