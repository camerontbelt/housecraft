//! Keep `Room` objects in step with the regions enclosed by walls.

use housecraft_geom::{pt, rooms, v2};
use housecraft_model::{Document, Floor, Id, Room};

use crate::command::Outcome;

/// Match each enclosed region to a room: existing rooms keep their properties
/// while their anchor stays inside a region (or near one that lost its room);
/// new regions get new rooms; rooms whose region is gone are removed.
pub(crate) fn sync(doc: &mut Document, out: &mut Outcome) {
    let floors: Vec<Id> = doc.ids::<Floor>();
    let d = doc.active_defaults().clone();
    for floor in floors {
        let regions = rooms::detect(doc, floor);
        let mut rooms: Vec<Room> = doc.rooms_on(floor).cloned().collect();
        rooms.sort_by_key(|r| r.id);
        let mut owner: Vec<Option<Id>> = vec![None; regions.rooms.len()];
        let mut homeless: Vec<Room> = Vec::new();
        for room in rooms {
            match regions.room_at(v2(room.anchor)) {
                Some(i) if owner[i].is_none() => owner[i] = Some(room.id),
                _ => homeless.push(room),
            }
        }
        // A wall moved past a label: re-home the room in the nearest free region.
        for room in homeless {
            let nearest = regions
                .rooms
                .iter()
                .enumerate()
                .filter(|(i, _)| owner[*i].is_none())
                .map(|(i, r)| (i, r.label_point.distance(v2(room.anchor))))
                .filter(|(_, dist)| *dist < 3000.0)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            match nearest {
                Some((i, _)) => {
                    owner[i] = Some(room.id);
                    if let Some(r) = doc.get_mut::<Room>(room.id) {
                        r.anchor = pt(regions.rooms[i].label_point);
                    }
                    out.modified.push(room.id);
                }
                None => {
                    doc.remove(room.id);
                    out.deleted.push(room.id);
                }
            }
        }
        for (i, region) in regions.rooms.iter().enumerate() {
            if owner[i].is_none() {
                out.created.push(doc.insert(Room {
                    id: Id(0),
                    floor,
                    anchor: pt(region.label_point),
                    name: None,
                    room_type: d.room_type,
                    ceiling_height: None,
                    floor_finish: None,
                    layer: d.layers.rooms,
                    ceiling: Default::default(),
                }));
            }
        }
    }
}
