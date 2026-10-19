//! Building framing, and a wall's framing drawn as a CAD detail.

use housecraft_geom::framing::{Member, ceiling_framing, floor_framing, roof_framing, wall_framing};
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

fn insert(doc: &mut Document, layer: Id, m: Member, out: &mut Outcome) {
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
        material: FramingMaterial::Lumber,
        host: Some(m.host),
        manual: false,
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
            if parts.ceilings {
                members.extend(ceiling_framing(doc, &d, fl));
            }
        }
    }
    let mut ridges = Vec::new();
    if parts.roofs {
        let planes: Vec<RoofPlane> = doc.iter::<RoofPlane>().filter(|p| on(p.floor) && wanted(p.id)).cloned().collect();
        for p in &planes {
            members.extend(roof_framing(&d, p, &mut ridges));
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
                    && ((parts.floors && roles_floor(m.role)) || (parts.ceilings && m.role == FramingRole::CeilingJoist))
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
        insert(doc, layer, m, out);
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
