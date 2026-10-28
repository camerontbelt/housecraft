//! Building framing, and a wall's framing drawn as a CAD detail.

use housecraft_geom::framing::{Member, ceiling_framing, floor_framing, roof_framing, roof_trusses, wall_framing};
use housecraft_geom::walls::wall_geoms;
use housecraft_geom::{DVec2, pt};
use housecraft_model::*;
use housecraft_units::{Length, Point3};

use crate::ApiError;
use crate::command::Outcome;
use crate::ops::{self, invalid};

/// What [`crate::Command::BuildFraming`] builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct FramingParts {
    #[serde(default = "yes")]
    pub walls: bool,
    #[serde(default = "yes")]
    pub floors: bool,
    #[serde(default = "yes")]
    pub ceilings: bool,
    #[serde(default = "yes")]
    pub roofs: bool,
}

fn yes() -> bool {
    true
}

impl FramingParts {
    pub fn all() -> Self {
        FramingParts { walls: true, floors: true, ceilings: true, roofs: true }
    }
}

/// The "Footings, Deck Post" layer, made (hidden in every layer set) the
/// first time a deck is added.
pub(crate) fn footing_layer(doc: &mut Document, out: &mut Outcome) -> Id {
    const NAME: &str = "Footings, Deck Post";
    if let Some(id) = doc.iter::<Layer>().find(|l| l.name == NAME).map(|l| l.id) {
        return id;
    }
    let id = doc.insert(Layer {
        id: Id(0),
        name: NAME.into(),
        color: Some(Color::rgb(120, 120, 120)),
        line_weight: 0.18,
        line_style: LineStyle::Dashed,
        text_style: None,
        in_materials_list: true,
        fill: None,
    });
    out.created.push(id);
    let sets: Vec<Id> = doc.iter::<LayerSet>().map(|s| s.id).collect();
    for s in sets {
        if let Some(ls) = doc.get_mut::<LayerSet>(s) {
            ls.layers.insert(id, LayerDisplay { visible: false, ..Default::default() });
            out.modified.push(s);
        }
    }
    id
}

/// The framing layer, made (hidden in every layer set) the first time.
fn framing_layer(doc: &mut Document, out: &mut Outcome) -> Id {
    let set = doc.settings.active_default_set;
    if let Some(l) = doc.active_defaults().layers.framing.filter(|l| doc.get::<Layer>(*l).is_some()) {
        return l;
    }
    let existing = doc.iter::<Layer>().find(|l| l.name == "Framing").map(|l| l.id);
    let id = match existing {
        Some(id) => id,
        None => {
            let id = doc.insert(Layer {
                id: Id(0),
                name: "Framing".into(),
                color: Some(Color::rgb(170, 110, 50)),
                line_weight: 0.18,
                line_style: LineStyle::Solid,
                text_style: None,
                in_materials_list: true,
                fill: None,
            });
            out.created.push(id);
            let sets: Vec<Id> = doc.iter::<LayerSet>().map(|s| s.id).collect();
            for s in sets {
                if let Some(ls) = doc.get_mut::<LayerSet>(s) {
                    ls.layers.insert(id, LayerDisplay { visible: false, ..Default::default() });
                    out.modified.push(s);
                }
            }
            id
        }
    };
    if let Some(d) = doc.get_mut::<DefaultSet>(set) {
        d.layers.framing = Some(id);
        out.modified.push(set);
    }
    id
}

fn insert(doc: &mut Document, layer: Id, m: Member, d: &FramingDefaults, out: &mut Outcome) {
    let tails = d.rafter_tails;
    // Steel walls: studs, plates and their blocking in light-gauge steel.
    let material = if doc.kind_of(m.host) == Some(ObjectKind::Wall) && !matches!(m.role, FramingRole::Header | FramingRole::Post | FramingRole::Girt) {
        d.wall_material
    } else if m.role == FramingRole::RimJoist
        && let Some(rim) = doc.get::<Floor>(m.host).and_then(|f| f.rim_material)
    {
        rim
    } else {
        FramingMaterial::Lumber
    };
    out.created.push(doc.insert(FramingMember {
        id: Id(0),
        floor: m.floor,
        layer,
        role: m.role,
        a: Point3::from_mm(m.a.x, m.a.y, m.a.z),
        b: Point3::from_mm(m.b.x, m.b.y, m.b.z),
        width: Length::from_mm(m.width),
        depth: Length::from_mm(m.depth),
        rotation: m.rotation,
        material,
        host: Some(m.host),
        manual: false,
        plies: 1,
        tail: if m.role == FramingRole::Rafter { tails } else { TailCut::Square },
    }));
}

/// Rebuild automatic framing on `floor` (every floor when `None`), only
/// for `hosts` (walls, floors or roof planes) when given. Hand-placed or
/// edited members are kept.
pub(crate) fn build(
    doc: &mut Document,
    floor: Option<Id>,
    parts: FramingParts,
    hosts: &[Id],
    out: &mut Outcome,
) -> Result<(), ApiError> {
    if let Some(f) = floor {
        ops::resolve_floor(doc, Some(f))?;
    }
    let d = doc.active_defaults().framing.clone();
    let on = |f: Id| floor.is_none_or(|x| x == f);
    // A room stands for the walls round it.
    let mut hosts = hosts.to_vec();
    let rooms: Vec<Room> = hosts.iter().filter_map(|h| doc.get::<Room>(*h).cloned()).collect();
    for r in rooms {
        let regions = housecraft_geom::rooms::detect(doc, r.floor);
        if let Some(i) = regions.room_at(housecraft_geom::v2(r.anchor)) {
            hosts.extend(regions.rooms[i].walls.iter().copied());
        }
    }
    let hosts = &hosts[..];
    let wanted = |h: Id| hosts.is_empty() || hosts.contains(&h);
    let mut members = Vec::new();
    let mut rebuilt_hosts = Vec::new();
    let floors: Vec<Floor> = doc.floors_by_elevation().into_iter().cloned().collect();
    for fl in floors.iter().filter(|f| on(f.id)) {
        if parts.walls {
            for g in wall_geoms(doc, fl.id) {
                if !wanted(g.id) {
                    continue;
                }
                if let Some(w) = doc.get::<Wall>(g.id) {
                    members.extend(wall_framing(doc, &d, w, &g));
                    rebuilt_hosts.push(g.id);
                }
            }
        }
        if wanted(fl.id) {
            if parts.floors {
                let ms = floor_framing(doc, &d, fl);
                members.extend(ms.into_iter().filter(|m| m.role != FramingRole::CeilingJoist));
            }
            // Trusses carry the ceiling under a trussed roof.
            let trussed = d.trusses && doc.iter::<RoofPlane>().any(|p| p.floor == fl.id);
            if parts.ceilings && !trussed {
                members.extend(ceiling_framing(doc, &d, fl));
            }
            if parts.roofs && d.trusses {
                members.extend(roof_trusses(doc, &d, fl));
            }
        }
    }
    let mut ridges = Vec::new();
    let overhang = doc.active_defaults().roof.overhang.mm();
    if parts.roofs {
        let planes: Vec<RoofPlane> = doc.iter::<RoofPlane>().filter(|p| on(p.floor) && wanted(p.id)).cloned().collect();
        for p in &planes {
            if !d.trusses {
                members.extend(roof_framing(&d, p, overhang, &mut ridges));
            }
            rebuilt_hosts.push(p.id);
        }
    }
    // Old automatic members of what is being rebuilt go.
    let roles_floor = |r: FramingRole| matches!(r, FramingRole::FloorJoist | FramingRole::RimJoist | FramingRole::Header);
    let doomed: Vec<Id> = doc
        .iter::<FramingMember>()
        .filter(|m| !m.manual && on(m.floor))
        .filter(|m| {
            let host = m.host.unwrap_or(Id(0));
            let is_floor_host = doc.kind_of(host) == Some(ObjectKind::Floor);
            if is_floor_host {
                wanted(host)
                    && ((parts.floors && roles_floor(m.role))
                        || (parts.ceilings && m.role == FramingRole::CeilingJoist)
                        || (parts.roofs && matches!(m.role, FramingRole::Chord | FramingRole::Web)))
            } else {
                rebuilt_hosts.contains(&host) || doc.object(host).is_none()
            }
        })
        .map(|m| m.id)
        .collect();
    if !doomed.is_empty() {
        ops::delete(doc, &doomed, out)?;
    }
    let layer = framing_layer(doc, out);
    for m in members {
        insert(doc, layer, m, &d, out);
    }
    Ok(())
}

/// A wall's framing seen from its exterior, as plain CAD in a new detail:
/// each member's outline in (along the wall, height).
pub(crate) fn wall_detail(doc: &Document, wall: Id) -> Result<(String, Vec<DetailShape>, Vec<crate::DetailText>), ApiError> {
    let w = crate::ops::expect::<Wall>(doc, wall)?;
    let g = wall_geoms(doc, w.floor)
        .into_iter()
        .find(|g| g.id == wall)
        .ok_or_else(|| ApiError::Invalid("the wall has no geometry".into()))?;
    let z0 = doc.get::<Floor>(w.floor).map_or(0.0, |f| f.elevation.mm());
    let members: Vec<&FramingMember> = doc.iter::<FramingMember>().filter(|m| m.host == Some(wall)).collect();
    if members.is_empty() {
        return invalid("build the wall's framing first (Build > Framing)");
    }
    let mut shapes = Vec::new();
    let mut texts = Vec::new();
    for m in &members {
        let c = housecraft_geom::framing::corners(m);
        let local: Vec<DVec2> = c
            .iter()
            .map(|p| DVec2::new((p.truncate() - g.start).dot(g.dir), p.z - z0))
            .collect();
        let (lo, hi) = housecraft_geom::math::bounds(local.iter().copied()).expect("corners");
        let fill = match m.role {
            FramingRole::Header => Some(Color::rgb(220, 190, 140)),
            _ => None,
        };
        shapes.push(DetailShape {
            shape: Shape::Polyline {
                points: [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)].into_iter().map(pt).collect(),
                closed: true,
            },
            color: None,
            fill,
        });
        if m.role == FramingRole::Header {
            texts.push(crate::DetailText {
                position: pt((lo + hi) / 2.0),
                text: format!("{} header", m.size_name()),
                height: None,
                rotation: 0.0,
                justify: Justify::Center,
            });
        }
    }
    let n = members.len();
    let len = Length::from_mm(g.length).format(doc.settings.display_unit);
    texts.push(crate::DetailText {
        position: pt(DVec2::new(0.0, -400.0)),
        text: format!("Wall framing: {len} long, {n} members"),
        height: None,
        rotation: 0.0,
        justify: Justify::Left,
    });
    Ok((format!("Wall Framing {}", wall.0), shapes, texts))
}

/// The truss through a chord or web member, drawn in its own plane with
/// each member's size.
pub(crate) fn truss_detail(doc: &Document, member: Id) -> Result<(String, Vec<DetailShape>, Vec<crate::DetailText>), ApiError> {
    let m = crate::ops::expect::<FramingMember>(doc, member)?;
    if !matches!(m.role, FramingRole::Chord | FramingRole::Web) {
        return invalid("choose a truss chord or web");
    }
    let (a, b) = (housecraft_geom::DVec3::from_array(m.a.mm()), housecraft_geom::DVec3::from_array(m.b.mm()));
    let flat = (b - a).truncate();
    let along = if flat.length() > 1.0 { flat.normalize() } else { DVec2::from_angle(m.rotation.to_radians()) };
    let across = housecraft_geom::math::perp(along);
    let in_plane = |p: housecraft_geom::DVec3| (p.truncate() - a.truncate()).dot(across).abs() < 30.0;
    let members: Vec<&FramingMember> = doc
        .iter::<FramingMember>()
        .filter(|x| matches!(x.role, FramingRole::Chord | FramingRole::Web))
        .filter(|x| in_plane(housecraft_geom::DVec3::from_array(x.a.mm())) && in_plane(housecraft_geom::DVec3::from_array(x.b.mm())))
        .collect();
    let to2 = |p: housecraft_geom::DVec3| DVec2::new((p.truncate() - a.truncate()).dot(along), p.z);
    let all: Vec<DVec2> = members.iter().flat_map(|x| housecraft_geom::framing::corners(x)).map(to2).collect();
    let (lo, hi) = housecraft_geom::math::bounds(all.iter().copied()).expect("members");
    let shift = |p: DVec2| p - lo;
    let mut shapes = Vec::new();
    let mut texts = Vec::new();
    for x in &members {
        let outline = housecraft_geom::framing::hull(housecraft_geom::framing::corners(x).iter().map(|p| shift(to2(*p))).collect());
        let mid = outline.iter().copied().sum::<DVec2>() / outline.len().max(1) as f64;
        shapes.push(DetailShape {
            shape: Shape::Polyline { points: outline.into_iter().map(pt).collect(), closed: true },
            color: None,
            fill: Some(Color::rgb(225, 200, 160)),
        });
        if x.role == FramingRole::Chord {
            texts.push(crate::DetailText {
                position: pt(mid + DVec2::new(0.0, 150.0)),
                text: x.size_name(),
                height: Some(Length::from_inches(3.0)),
                rotation: 0.0,
                justify: Justify::Center,
            });
        }
    }
    let unit = doc.settings.display_unit;
    texts.push(crate::DetailText {
        position: pt(DVec2::new(0.0, -500.0)),
        text: format!(
            "Truss: span {}, height {}, {} members",
            Length::from_mm(hi.x - lo.x).format(unit),
            Length::from_mm(hi.y - lo.y).format(unit),
            members.len()
        ),
        height: None,
        rotation: 0.0,
        justify: Justify::Left,
    });
    Ok(("Truss Detail".into(), shapes, texts))
}

/// Beams along the room's long way and posts under them, holding up the
/// floor above (a basement's post-and-beam line). Placed as hand-made
/// members so rebuilding the framing keeps them.
pub(crate) fn post_and_beam(doc: &mut Document, room: Id, max_span: f64, post_spacing: f64, out: &mut Outcome) -> Result<(), ApiError> {
    let r = doc.get::<Room>(room).cloned().ok_or(ApiError::NotFound(room))?;
    let floor = doc.get::<Floor>(r.floor).cloned().ok_or(ApiError::NotFound(r.floor))?;
    let regions = housecraft_geom::rooms::detect(doc, r.floor);
    let i = regions.room_at(housecraft_geom::v2(r.anchor)).ok_or_else(|| ApiError::Invalid("the room has no outline".into()))?;
    let poly = regions.rooms[i].polygon.clone();
    let (lo, hi) = housecraft_geom::math::bounds(poly.iter().copied()).ok_or_else(|| ApiError::Invalid("empty room".into()))?;
    let size = hi - lo;
    // Beams run the long way; joists span the short way between them.
    let (along, across) = if size.x >= size.y { (housecraft_geom::DVec2::X, housecraft_geom::DVec2::Y) } else { (housecraft_geom::DVec2::Y, housecraft_geom::DVec2::X) };
    let width = size.dot(across);
    let lines = ((width / max_span.max(600.0)).ceil() as usize).saturating_sub(1);
    if lines == 0 {
        return Err(ApiError::Invalid("the room is narrow enough for joists to span it".into()));
    }
    let layer = doc.active_defaults().layers.framing.unwrap_or(doc.active_defaults().layers.cad);
    let depth = 11.25 * 25.4;
    let top = floor.elevation.mm() + floor.ceiling_height.mm();
    let beam_z = top - depth / 2.0;
    let mut add = |doc: &mut Document, role: FramingRole, a: housecraft_geom::DVec3, b: housecraft_geom::DVec3, width: f64, depth: f64, plies: u32| {
        out.created.push(doc.insert(FramingMember {
            id: Id(0),
            floor: r.floor,
            layer,
            role,
            a: Point3::from_mm(a.x, a.y, a.z),
            b: Point3::from_mm(b.x, b.y, b.z),
            width: Length::from_mm(width),
            depth: Length::from_mm(depth),
            rotation: 0.0,
            material: FramingMaterial::Lumber,
            host: Some(room),
            manual: true,
            plies,
            tail: TailCut::Square,
        }));
    };
    let base = lo.dot(across);
    for k in 1..=lines {
        let off = base + width * k as f64 / (lines + 1) as f64;
        let o = across * off;
        for (t0, t1) in housecraft_geom::framing::clip_line(&poly, o, along) {
            let (a, b) = (o + along * t0, o + along * t1);
            if a.distance(b) < 600.0 {
                continue;
            }
            add(doc, FramingRole::Beam, a.extend(beam_z), b.extend(beam_z), 1.5 * 25.4, depth, 3);
            let len = a.distance(b);
            let posts = ((len / post_spacing.max(600.0)).ceil() as usize).max(1);
            for p in 1..posts {
                let at = a.lerp(b, p as f64 / posts as f64);
                add(doc, FramingRole::Post, at.extend(floor.elevation.mm()), at.extend(beam_z - depth / 2.0), 5.5 * 25.4, 5.5 * 25.4, 1);
            }
        }
    }
    Ok(())
}
