//! Plan Check: a review of the plan for common problems (bedroom egress,
//! rooms without doors, wall layout, openings, stairs, the roof).

use housecraft_geom::{rooms, v2};
use housecraft_model::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Likely a code or construction problem.
    Problem,
    /// Worth a look.
    Note,
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Issue {
    pub severity: Severity,
    /// What it is about ("Egress", "Walls"…).
    pub topic: String,
    pub message: String,
    /// The objects concerned (for Find).
    pub ids: Vec<Id>,
}

const INCH: f64 = 25.4;

/// Review the whole plan; problems first.
pub fn plan_check(doc: &Document) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut add = |severity, topic: &str, message: String, ids: Vec<Id>| out.push(Issue { severity, topic: topic.into(), message, ids });
    let unit = doc.settings.display_unit;
    let fmt = |mm: f64| housecraft_units::Length::from_mm(mm).format(unit);

    for floor in doc.floors_by_elevation() {
        let regions = rooms::detect(doc, floor.id);
        let walls: Vec<&Wall> = doc.walls_on(floor.id).collect();
        // Rooms: egress from bedrooms, a way in to every room.
        for room in doc.rooms_on(floor.id) {
            let Some(i) = regions.room_at(v2(room.anchor)) else { continue };
            let region = &regions.rooms[i];
            let name = doc.room_name(room);
            let kind = doc.get::<RoomType>(room.room_type).map_or(String::new(), |t| t.name.to_lowercase());
            // Openings in this room's own stretch of its walls.
            let poly = &region.centerline;
            let on_edge = |p: housecraft_geom::DVec2| (0..poly.len()).any(|k| housecraft_geom::math::distance_to_segment(p, poly[k], poly[(k + 1) % poly.len()]) < 10.0);
            let openings: Vec<&Opening> = region
                .walls
                .iter()
                .filter_map(|w| doc.get::<Wall>(*w))
                .flat_map(|w| {
                    let (a, b) = (v2(w.start), v2(w.end));
                    let dir = (b - a).normalize_or_zero();
                    doc.openings_in(w.id).filter(move |o| on_edge(a + dir * o.position.mm()))
                })
                .collect();
            if kind.contains("bed") {
                // Egress: 5.7 sq ft clear opening, sill at most 44" (taken
                // here as the window's size less a 2" frame all round).
                let egress = openings.iter().any(|o| {
                    !o.opening_kind.is_door()
                        && (o.width.mm() - 4.0 * INCH) * (o.height.mm() - 4.0 * INCH) >= 5.7 * 144.0 * INCH * INCH
                        && o.sill.mm() <= 44.0 * INCH + 1.0
                });
                if !egress {
                    add(Severity::Problem, "Egress", format!("{name} on {} has no egress window (5.7 sq ft opening, sill at most 44\")", floor.name), vec![room.id]);
                }
            }
            let closet_like = ["closet", "pantry", "chase", "shaft"].iter().any(|k| kind.contains(k));
            if !closet_like && floor.foundation.is_none() && !openings.iter().any(|o| o.opening_kind.is_door()) && region.walls.iter().all(|w| doc.get::<Wall>(*w).is_some_and(|w| doc.get::<WallType>(w.wall_type).is_none_or(|t| t.railing.is_none()))) {
                add(Severity::Note, "Rooms", format!("{name} on {} has no door", floor.name), vec![room.id]);
            }
        }
        // Walls: very short, doubled, or ends that nearly meet.
        for (k, w) in walls.iter().enumerate() {
            let (a, b) = (v2(w.start), v2(w.end));
            if a.distance(b) < 6.0 * INCH {
                add(Severity::Note, "Walls", format!("A wall on {} is only {} long", floor.name, fmt(a.distance(b))), vec![w.id]);
            }
            for other in &walls[k + 1..] {
                let (c, d) = (v2(other.start), v2(other.end));
                let same = (a.distance(c) < 1.0 && b.distance(d) < 1.0) || (a.distance(d) < 1.0 && b.distance(c) < 1.0);
                if same {
                    add(Severity::Problem, "Walls", format!("Two walls on {} lie on top of each other", floor.name), vec![w.id, other.id]);
                }
            }
            for end in [a, b] {
                // Distance to the nearest other wall line (ends or a T).
                let near = walls
                    .iter()
                    .filter(|o| o.id != w.id)
                    .map(|o| housecraft_geom::math::distance_to_segment(end, v2(o.start), v2(o.end)))
                    .fold(f64::MAX, f64::min);
                if near > 1.0 && near < 2.0 * INCH {
                    add(Severity::Problem, "Walls", format!("A wall end on {} misses another wall by {}", floor.name, fmt(near)), vec![w.id]);
                }
            }
            // Openings: overlapping in one wall, or past its ends.
            let len = a.distance(b);
            let mut spans: Vec<(f64, f64, Id)> = doc.openings_in(w.id).map(|o| (o.position.mm() - o.width.mm() / 2.0, o.position.mm() + o.width.mm() / 2.0, o.id)).collect();
            spans.sort_by(|x, y| x.0.total_cmp(&y.0));
            for pair in spans.windows(2) {
                if pair[1].0 < pair[0].1 - 1.0 {
                    add(Severity::Problem, "Doors and Windows", format!("Two openings overlap in a wall on {}", floor.name), vec![pair[0].2, pair[1].2]);
                }
            }
            for (s, e, id) in &spans {
                if *s < -1.0 || *e > len + 1.0 {
                    add(Severity::Problem, "Doors and Windows", format!("An opening on {} runs past the end of its wall", floor.name), vec![*id]);
                }
            }
        }
        // Stairs: riser and tread limits (IRC: 7 3/4" rise, 10" run).
        for s in doc.iter::<Stair>().filter(|s| s.floor == floor.id) {
            let g = housecraft_geom::stairs::geometry(doc, s);
            if g.riser_height > 7.75 * INCH + 0.5 {
                add(Severity::Problem, "Stairs", format!("A stair on {} has {} risers (over 7 3/4\")", floor.name, fmt(g.riser_height)), vec![s.id]);
            }
            if s.tread_depth.mm() < 10.0 * INCH - 0.5 {
                add(Severity::Problem, "Stairs", format!("A stair on {} has {} treads (under 10\")", floor.name, fmt(s.tread_depth.mm())), vec![s.id]);
            }
        }
    }
    // The roof: the top storey with walls should be covered.
    let top = doc.floors_by_elevation().into_iter().rev().find(|f| f.foundation.is_none() && doc.walls_on(f.id).next().is_some());
    if let Some(f) = top
        && !doc.iter::<RoofPlane>().any(|r| !r.gable)
    {
        add(Severity::Note, "Roof", format!("{} has walls but the plan has no roof (Build > Roof)", f.name), Vec::new());
    }
    for r in doc.iter::<RoofPlane>().filter(|r| !r.gable) {
        let rise = r.pitch.to_radians().tan() * 12.0;
        if rise < 2.0 {
            add(Severity::Note, "Roof", format!("A roof plane is pitched {rise:.1}/12: low for shingles (2/12 minimum)"), vec![r.id]);
        }
    }
    out.sort_by(|a, b| a.severity.cmp(&b.severity).then(a.topic.cmp(&b.topic)));
    out
}
