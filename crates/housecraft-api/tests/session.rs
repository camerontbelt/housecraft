use housecraft_api::model::*;
use housecraft_api::units::Length;
use housecraft_api::{ApiError, Command, Session, query};

fn commands(json: &str) -> Vec<Command> {
    serde_json::from_str(json).expect("valid commands")
}

fn apply(s: &mut Session, json: &str) -> housecraft_api::Outcome {
    s.apply(&commands(json)).unwrap_or_else(|e| panic!("{e}"))
}

fn patch(json: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    json.as_object().unwrap().clone()
}

/// 20' x 30' box drawn clockwise (exterior on the left).
fn boxed() -> Session {
    let mut s = Session::new(Document::new("Box"));
    apply(
        &mut s,
        r#"[{"op": "add_walls", "points": [[0,0], [0,"30'"], ["20'","30'"], ["20'",0]], "closed": true}]"#,
    );
    s
}

fn walls(s: &Session) -> Vec<Wall> {
    s.document().iter::<Wall>().cloned().collect()
}

#[test]
fn closed_walls_create_a_room() {
    let s = boxed();
    let sum = query::summary(s.document());
    assert_eq!(sum.wall_count, 4);
    assert_eq!(sum.floors[0].total_wall_length, "100'-0\"");
    assert_eq!(sum.floors[0].rooms.len(), 1);
    // Interior is 20'x30' minus a 2x6 wall thickness on each side.
    let t = 7.1875 / 12.0;
    let expected = format!("{:.0} sq ft", (20.0 - t) * (30.0 - t));
    assert_eq!(sum.floors[0].rooms[0].area, expected);
}

#[test]
fn partition_splits_room_and_keeps_properties() {
    let mut s = boxed();
    let room = s.document().iter::<Room>().next().unwrap().id;
    let kitchen = s.document().find_named::<RoomType>("Kitchen").unwrap().id;
    s.apply(&[Command::Update {
        id: room,
        set: patch(serde_json::json!({"room_type": kitchen.0, "name": "Big Kitchen"})),
    }])
    .unwrap();
    apply(
        &mut s,
        r#"[{"op": "add_wall", "start": [0,"10'"], "end": ["20'","10'"], "interior": true}]"#,
    );
    let rooms: Vec<Room> = s.document().iter::<Room>().cloned().collect();
    assert_eq!(rooms.len(), 2);
    assert!(rooms.iter().any(|r| r.id == room && r.room_type == kitchen));
    // Removing the partition merges them back into the original room.
    let partition = walls(&s).into_iter().max_by_key(|w| w.id).unwrap().id;
    s.apply(&[Command::Delete {
        ids: vec![partition],
    }])
    .unwrap();
    let rooms: Vec<Room> = s.document().iter::<Room>().cloned().collect();
    assert_eq!(rooms.len(), 1);
    assert_eq!(rooms[0].id, room);
    assert_eq!(rooms[0].name.as_deref(), Some("Big Kitchen"));
}

#[test]
fn failed_batch_changes_nothing() {
    let mut s = Session::new(Document::new("Atomic"));
    let before = s.document().clone();
    let err = s
        .apply(&commands(
            r#"[{"op": "add_wall", "start": [0,0], "end": [3000,0]},
                {"op": "add_wall", "start": [0,0], "end": [0,0]}]"#,
        ))
        .unwrap_err();
    assert!(matches!(err, ApiError::InCommand { index: 1, .. }), "{err}");
    assert_eq!(s.document(), &before);
    assert!(!s.can_undo());
}

#[test]
fn update_is_validated() {
    let mut s = boxed();
    let wall = walls(&s)[0].id;
    let err = s
        .apply(&[Command::Update {
            id: wall,
            set: patch(serde_json::json!({"wall_type": 99999})),
        }])
        .unwrap_err();
    assert!(err.to_string().contains("missing"), "{err}");
    s.apply(&[Command::Update {
        id: wall,
        set: patch(serde_json::json!({"height": "9'"})),
    }])
    .unwrap();
    let w = s.document().get::<Wall>(wall).unwrap();
    assert!((w.height.unwrap().feet() - 9.0).abs() < 1e-9);
}

#[test]
fn doors_and_windows_place_and_validate() {
    let mut s = boxed();
    let wall = walls(&s)[0].id; // 30' west wall
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_door", "wall": {w}}}, {{"op": "add_window", "wall": {w}, "position": "5'"}}]"#,
            w = wall.0
        ),
    );
    assert_eq!(out.created.len(), 2);
    let window = s.document().get::<Opening>(out.created[1]).unwrap();
    assert!((window.sill.inches() - 32.0).abs() < 1e-9);
    // A door wider than the wall is refused.
    let err = s
        .apply(&commands(&format!(
            r#"[{{"op": "add_door", "wall": {}, "width": "40'"}}]"#,
            wall.0
        )))
        .unwrap_err();
    assert!(err.to_string().contains("does not fit"), "{err}");
    // Shrinking the wall below its openings is refused by validation.
    let err = s
        .apply(&[Command::SetWallLength {
            id: wall,
            length: Length::from_feet(4.0),
            anchor: housecraft_api::LengthAnchor::Start,
        }])
        .unwrap_err();
    assert!(err.to_string().contains("does not fit"), "{err}");
}

#[test]
fn set_wall_length_drags_joined_walls() {
    let mut s = boxed();
    // North wall runs (0,30') -> (20',30'); stretch it to 24' from its start.
    let north = walls(&s)
        .into_iter()
        .find(|w| {
            (w.start.y().feet() - 30.0).abs() < 1e-6 && (w.end.y().feet() - 30.0).abs() < 1e-6
        })
        .unwrap();
    s.apply(&[Command::SetWallLength {
        id: north.id,
        length: Length::from_feet(24.0),
        anchor: housecraft_api::LengthAnchor::Start,
    }])
    .unwrap();
    let east = walls(&s)
        .into_iter()
        .find(|w| (w.end.x().feet() - 20.0).abs() < 1e-6 && w.end.y().mm().abs() < 1e-6)
        .unwrap();
    assert!((east.start.x().feet() - 24.0).abs() < 1e-6, "{east:?}");
    assert_eq!(s.document().count::<Room>(), 1);
}

#[test]
fn transform_copies_and_moves() {
    let mut s = Session::new(Document::new("T"));
    let out = apply(
        &mut s,
        r#"[{"op": "add_cad", "shape": {"shape": "line", "a": [0,0], "b": [1000,0]}}]"#,
    );
    let line = out.created[0];
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "transform", "ids": [{}], "transform": {{"type": "translate", "dx": 0, "dy": 500}}, "copies": 3}}]"#,
            line.0
        ),
    );
    assert_eq!(out.created.len(), 3);
    let last = s.document().get::<Cad>(out.created[2]).unwrap();
    let Shape::Line { a, .. } = last.shape else {
        panic!()
    };
    assert!((a.y().mm() - 1500.0).abs() < 1e-9);
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "transform", "ids": [{}], "transform": {{"type": "rotate", "center": [0,0], "angle": 90}}}}]"#,
            line.0
        ),
    );
    let Shape::Line { b, .. } = s.document().get::<Cad>(line).unwrap().shape else {
        panic!()
    };
    assert!(b.x().mm().abs() < 1e-9 && (b.y().mm() - 1000.0).abs() < 1e-9);
}

#[test]
fn fillet_extend_and_offset() {
    let mut s = Session::new(Document::new("C"));
    let out = apply(
        &mut s,
        r#"[{"op": "add_cad", "shape": {"shape": "line", "a": [0,0], "b": [900,0]}},
            {"op": "add_cad", "shape": {"shape": "line", "a": [1000,100], "b": [1000,1000]}}]"#,
    );
    let (h, v) = (out.created[0], out.created[1]);
    let line_of = |s: &Session, id: Id| match s.document().get::<Cad>(id).unwrap().shape {
        Shape::Line { a, b, .. } => (a, b),
        _ => panic!("not a line"),
    };
    // Sharp corner first.
    s.apply(&[Command::Fillet {
        a: h,
        b: v,
        radius: Length::ZERO,
    }])
    .unwrap();
    assert!((line_of(&s, h).1.x().mm() - 1000.0).abs() < 1e-9);
    // Then round it.
    let out = s
        .apply(&[Command::Fillet {
            a: h,
            b: v,
            radius: Length::from_mm(200.0),
        }])
        .unwrap();
    let Shape::Arc { center, sweep, .. } = s.document().get::<Cad>(out.created[0]).unwrap().shape
    else {
        panic!()
    };
    assert!((center.x().mm() - 800.0).abs() < 1e-6 && (center.y().mm() - 200.0).abs() < 1e-6);
    assert!((sweep.abs() - 90.0).abs() < 1e-6);
    // Offset makes a parallel copy.
    let out = s
        .apply(&[Command::Offset {
            id: h,
            distance: Length::from_mm(100.0),
        }])
        .unwrap();
    let copy = out.created[0];
    assert!((line_of(&s, copy).0.y().mm() - 100.0).abs() < 1e-9);
    // Extend the copy to the vertical line.
    s.apply(&[Command::ExtendTo {
        id: copy,
        boundary: v,
    }])
    .unwrap();
    assert!((line_of(&s, copy).1.x().mm() - 1000.0).abs() < 1e-9);
}

#[test]
fn exterior_dimensions_surround_building() {
    let mut s = boxed();
    let out = apply(&mut s, r#"[{"op": "add_exterior_dimensions"}]"#);
    assert_eq!(
        out.created.len(),
        4,
        "a plain box only gets the overall tier"
    );
}

#[test]
fn undo_and_redo() {
    let mut s = Session::new(Document::new("Undo"));
    apply(
        &mut s,
        r#"[{"op": "add_wall", "start": [0,0], "end": [3000,0]}]"#,
    );
    assert_eq!(s.document().count::<Wall>(), 1);
    assert!(s.undo());
    assert_eq!(s.document().count::<Wall>(), 0);
    assert!(s.redo());
    assert_eq!(s.document().count::<Wall>(), 1);
    assert!(!s.redo());
}

#[test]
fn deleting_a_floor_deletes_its_contents() {
    let mut s = boxed();
    let out = apply(&mut s, r#"[{"op": "add_floor"}]"#);
    let upper = out.created[0];
    // On top of floor 1's 8' ceiling plus the new floor's 12 5/8" platform.
    let expected = 8.0 * 12.0 + 12.625;
    assert!((s.document().get::<Floor>(upper).unwrap().elevation.inches() - expected).abs() < 1e-9);
    let ground = s.document().floors_by_elevation()[0].id;
    s.apply(&[Command::Delete { ids: vec![ground] }]).unwrap();
    assert_eq!(s.document().count::<Wall>(), 0);
    assert_eq!(s.document().count::<Room>(), 0);
    assert!(s.document().validate().is_empty());
    let err = s
        .apply(&[Command::Delete { ids: vec![upper] }])
        .unwrap_err();
    assert!(err.to_string().contains("at least one floor"), "{err}");
}

#[test]
fn referenced_objects_cannot_be_deleted() {
    let mut s = boxed();
    let ty = s.document().active_defaults().exterior_wall_type;
    assert!(s.apply(&[Command::Delete { ids: vec![ty] }]).is_err());
    let layer = s.document().active_defaults().layers.walls;
    assert!(s.apply(&[Command::Delete { ids: vec![layer] }]).is_err());
}

#[test]
fn wrong_kind_is_reported() {
    let mut s = Session::new(Document::new("Kinds"));
    let floor = s.document().floors_by_elevation()[0].id;
    let err = s
        .apply(&[Command::MoveWall {
            id: floor,
            start: None,
            end: None,
            detach: false,
        }])
        .unwrap_err();
    let ApiError::InCommand { error, .. } = err else {
        panic!()
    };
    assert!(matches!(
        *error,
        ApiError::WrongKind {
            actual: ObjectKind::Floor,
            ..
        }
    ));
    assert!(query::get_object(s.document(), Id(9999)).is_none());
}

#[test]
fn schema_lists_every_op() {
    let schema = housecraft_api::command_schema().to_string();
    for op in [
        "add_wall",
        "add_door",
        "add_window",
        "update",
        "transform",
        "fillet",
        "delete",
    ] {
        assert!(schema.contains(op), "schema missing {op}");
    }
}

#[test]
fn every_object_kind_round_trips_through_json() {
    let mut s = boxed();
    let wall = walls(&s)[0].id;
    apply(
        &mut s,
        &format!(
            r##"[{{"op": "add_door", "wall": {w}}},
                {{"op": "add_window", "wall": {w}, "position": "5'"}},
                {{"op": "add_cad", "shape": {{"shape": "arc", "center": [0,0], "radius": 500, "start_angle": 0, "sweep": 90}}}},
                {{"op": "add_dimension", "geometry": {{"type": "linear", "points": [[0,0],[1000,0]], "offset": 300}}}},
                {{"op": "add_dimension", "geometry": {{"type": "angular", "center": [0,0], "start": [1000,0], "end": [0,1000], "radius": 500}}}},
                {{"op": "add_text", "position": [0,0], "text": "hi"}},
                {{"op": "add_layer", "name": "Extra", "color": "#ff000080"}},
                {{"op": "add_plan_view", "name": "Second"}},
                {{"op": "build_roof"}},
                {{"op": "add_stair", "start": ["5'","5'"], "direction": 90}},
                {{"op": "add_electrical", "electrical_kind": "outlet", "at": [0,"5'"]}},
                {{"op": "add_schedule", "schedule_kind": "rooms", "position": [0,0]}},
                {{"op": "add_section_view", "a": [0,"-5'"], "b": ["20'","-5'"]}},
                {{"op": "add_camera", "name": "Cam", "projection": {{"type": "orthographic", "height": 5000}}, "eye": [0,0,1000], "target": [1000,1000,0]}}]"##,
            w = wall.0
        ),
    );
    let doc = s.document();
    for kind in ObjectKind::ALL {
        assert!(
            doc.objects.values().any(|o| o.kind() == *kind),
            "test document lacks a {kind}"
        );
    }
    let json = serde_json::to_string(doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(&back, doc);
}

#[test]
fn insert_and_swap_floors() {
    let mut s = boxed();
    let ground = s.document().floors_by_elevation()[0].id;
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_floor", "below": {}, "name": "Basement"}}]"#,
            ground.0
        ),
    );
    let basement = out.created[0];
    let floors = s.document().floors_by_elevation();
    assert_eq!(floors[0].id, basement);
    assert_eq!(floors[1].id, ground);
    assert!(
        floors[1].elevation.mm() > 0.0,
        "the old ground floor moved up"
    );
    assert_eq!(s.document().walls_on(ground).count(), 4);
    s.apply(&[Command::SwapFloors {
        a: ground,
        b: basement,
    }])
    .unwrap();
    assert_eq!(s.document().walls_on(basement).count(), 4);
    assert_eq!(s.document().walls_on(ground).count(), 0);
    assert_eq!(s.document().rooms_on(basement).count(), 1);
}

#[test]
fn foundation_follows_the_house() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "build_foundation", "kind": "crawlspace"}]"#,
    );
    let doc = s.document();
    let floors = doc.floors_by_elevation();
    assert_eq!(floors.len(), 2);
    let fnd = floors[0];
    let ground = floors[1];
    let (fnd_id, ground_id) = (fnd.id, ground.id);
    assert!(fnd.foundation.is_some());
    // Stem walls reach the underside of the floor platform.
    let top = fnd.elevation + fnd.ceiling_height;
    assert!((top.mm() - (ground.elevation - ground.floor_structure).mm()).abs() < 1e-6);
    let walls: Vec<&Wall> = doc.walls_on(fnd.id).collect();
    assert_eq!(walls.len(), 4, "{out:?}");
    assert!(walls.iter().all(|w| w.footing.is_some()));
    // Outer face of the foundation matches the outer face of the walls above.
    let t = doc
        .get::<WallType>(walls[0].wall_type)
        .unwrap()
        .thickness()
        .mm();
    let xs: Vec<f64> = walls
        .iter()
        .flat_map(|w| [w.start.x().mm(), w.end.x().mm()])
        .collect();
    let min_x = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let ext_half = 7.1875 * 25.4 / 2.0;
    assert!((min_x - (-ext_half + t / 2.0)).abs() < 1e-6, "{min_x}");
    assert_eq!(doc.rooms_on(fnd.id).count(), 1, "crawlspace becomes a room");

    // Widen the house by moving the east wall: the foundation follows.
    let east = doc
        .walls_on(ground_id)
        .find(|w| {
            (w.start.x().feet() - 20.0).abs() < 1e-6 && (w.end.x().feet() - 20.0).abs() < 1e-6
        })
        .unwrap()
        .id;
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "move_wall", "id": {}, "start": ["26'", "30'"], "end": ["26'", 0]}}]"#,
            east.0
        ),
    );
    let doc = s.document();
    let max_x = doc
        .walls_on(fnd_id)
        .flat_map(|w| [w.start.x().mm(), w.end.x().mm()])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        (max_x - (Length::from_feet(26.0).mm() + ext_half - t / 2.0)).abs() < 1e-6,
        "{max_x}"
    );
}

#[test]
fn slab_foundation_removes_the_floor_platform() {
    let mut s = boxed();
    apply(&mut s, r#"[{"op": "build_foundation", "kind": "slab"}]"#);
    let doc = s.document();
    let floors = doc.floors_by_elevation();
    assert_eq!(floors[1].floor_structure.mm(), 0.0);
    assert!((floors[0].elevation.inches() + 4.0).abs() < 1e-9);
    let err = Session::new(Document::new("empty"))
        .apply(&commands(
            r#"[{"op": "build_foundation", "kind": "basement"}]"#,
        ))
        .unwrap_err();
    assert!(err.to_string().contains("enclose"), "{err}");
}

#[test]
fn crawlspace_is_not_living_area() {
    let mut s = boxed();
    let before = query::summary(s.document()).living_area;
    apply(
        &mut s,
        r#"[{"op": "build_foundation", "kind": "crawlspace"}]"#,
    );
    assert_eq!(query::summary(s.document()).living_area, before);
}

#[test]
fn roof_builds_and_follows_walls() {
    let mut s = boxed();
    let out = apply(&mut s, r#"[{"op": "build_roof"}]"#);
    let planes: Vec<RoofPlane> = s.document().iter::<RoofPlane>().cloned().collect();
    assert_eq!(planes.len(), 4, "{out:?}");
    assert!(planes.iter().all(|p| p.auto && !p.gable));
    // Make the short walls gables: two sloped planes plus two gable infills.
    let ids: Vec<(Id, bool)> = walls(&s)
        .iter()
        .map(|w| (w.id, (w.start.y().mm() - w.end.y().mm()).abs() < 1.0))
        .collect();
    let cmds: Vec<Command> = ids
        .iter()
        .filter(|(_, horizontal)| *horizontal)
        .map(|(id, _)| Command::Update {
            id: *id,
            set: patch(serde_json::json!({"roof": {"gable": true}})),
        })
        .collect();
    s.apply(&cmds).unwrap();
    let planes: Vec<RoofPlane> = s.document().iter::<RoofPlane>().cloned().collect();
    assert_eq!(planes.iter().filter(|p| p.gable).count(), 2);
    assert_eq!(planes.iter().filter(|p| !p.gable).count(), 2);
    // Removing it clears every plane and stops rebuilding.
    apply(&mut s, r#"[{"op": "remove_roof"}]"#);
    assert_eq!(s.document().count::<RoofPlane>(), 0);
    apply(
        &mut s,
        r#"[{"op": "add_wall", "start": [0,"10'"], "end": ["20'","10'"], "interior": true}]"#,
    );
    assert_eq!(s.document().count::<RoofPlane>(), 0);
}

#[test]
fn hand_drawn_roof_plane_rises_from_its_eave() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_roof_plane", "points": [[0,0],["10'",0],["10'","10'"],[0,"10'"]],
             "eave_a": [0,0], "eave_b": ["10'",0], "eave_height": "8'", "pitch": 45}]"#,
    );
    let plane = s.document().get::<RoofPlane>(out.created[0]).unwrap();
    let top = plane.outline.iter().map(|p| p.mm()[2]).fold(0.0, f64::max);
    assert!((top - Length::from_feet(18.0).mm()).abs() < 1e-6, "{top}");
}

#[test]
fn stair_climbs_to_the_floor_above_and_cuts_a_stairwell() {
    let mut s = boxed();
    apply(&mut s, r#"[{"op": "add_floor"}]"#);
    let floors = s.document().floors_by_elevation();
    let (ground, upper) = (floors[0].id, floors[1].id);
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_walls", "floor": {}, "points": [[0,0], [0,"30'"], ["20'","30'"], ["20'",0]], "closed": true}}]"#,
            upper.0
        ),
    );
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_stair", "floor": {}, "start": ["10'","5'"], "direction": 90, "shape": {{"type": "l", "turn": "left", "first_treads": 0}}}}]"#,
            ground.0
        ),
    );
    let doc = s.document();
    let stair = doc.get::<Stair>(out.created[0]).unwrap();
    // Floor-to-floor = ceiling height + floor structure of the floor above.
    let rise = doc.stair_rise(stair).mm();
    let g = housecraft_api::geom::stairs::geometry(doc, stair);
    assert!(g.riser_height <= stair.max_riser.mm() + 1e-9);
    assert!((g.risers as f64 * g.riser_height - rise).abs() < 1e-6);
    // An L stair splits evenly around its landing.
    let StairShape::L { first_treads, .. } = stair.shape else {
        panic!("not an L stair")
    };
    assert_eq!(g.flights, vec![first_treads, g.flights[1]]);
    assert!(first_treads.abs_diff(g.flights[1]) <= 1);
    assert!(
        query::describe(doc, doc.object(out.created[0]).unwrap())
            .label
            .contains("L-shaped stair")
    );

    // The upper floor's surface has a hole where the stair comes up.
    let mesh = housecraft_api::geom::mesh::build(doc, None);
    let floor_area = |mesh: &housecraft_api::geom::mesh::Mesh, owner_floor: Id| {
        let mut area = 0.0;
        for part in mesh
            .parts
            .iter()
            .filter(|p| p.category == housecraft_api::geom::mesh::Category::Floor)
        {
            let Some(room) = doc.get::<Room>(part.object) else {
                continue;
            };
            if room.floor != owner_floor {
                continue;
            }
            let idx = &mesh.indices
                [part.first_index as usize..(part.first_index + part.index_count) as usize];
            for t in idx.chunks(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.positions[i as usize]);
                if mesh.normals[t[0] as usize][2] > 0.9 && (a[2] - b[2]).abs() < 1e-3 {
                    area += 0.5
                        * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs()
                            as f64;
                }
            }
        }
        area
    };
    let with_hole = floor_area(&mesh, upper);
    let footprint: f64 = g
        .footprint()
        .iter()
        .map(|p| housecraft_api::geom::math::signed_area(p).abs())
        .sum();
    let mut s2 = s.clone();
    s2.apply(&[Command::Update {
        id: stair.id,
        set: patch(serde_json::json!({ "stairwell": false })),
    }])
    .unwrap();
    let solid = floor_area(
        &housecraft_api::geom::mesh::build(s2.document(), None),
        upper,
    );
    // Mesh positions are metres; the hole goes through the floor surface
    // and the top of every platform layer beneath it.
    let layers = doc.get::<Floor>(upper).unwrap().platform.len().max(1) as f64;
    let expected = (1.0 + layers) * footprint / 1e6;
    assert!(
        ((solid - with_hole) - expected).abs() / expected < 0.01,
        "solid {solid} hole {with_hole} footprint {footprint}"
    );
    assert!(
        mesh.parts
            .iter()
            .any(|p| p.category == housecraft_api::geom::mesh::Category::Stair)
    );
}

#[test]
fn mirroring_an_l_stair_flips_its_turn_and_railings() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_stair", "start": ["10'","5'"], "direction": 90, "railing_left": false, "railing_right": true,
             "shape": {"type": "l", "turn": "left", "first_treads": 5}}]"#,
    );
    let id = out.created[0];
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "transform", "ids": [{}], "transform": {{"type": "mirror", "a": ["10'",0], "b": ["10'","30'"]}}}}]"#,
            id.0
        ),
    );
    let st = s.document().get::<Stair>(id).unwrap();
    assert!(matches!(
        st.shape,
        StairShape::L {
            turn: Turn::Right,
            first_treads: 5
        }
    ));
    assert!(st.railing_left && !st.railing_right);
    assert!((st.direction.rem_euclid(360.0) - 90.0).abs() < 1e-9);
}

#[test]
fn garage_doors_get_their_own_size_and_doors_open_in_3d() {
    let mut s = boxed();
    let wall = walls(&s)[0].id;
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_door", "wall": {w}, "position": "10'", "style": "garage"}},
                {{"op": "add_door", "wall": {w}, "position": "22'"}}]"#,
            w = wall.0
        ),
    );
    let (garage, door) = (out.created[0], out.created[1]);
    let g = s.document().get::<Opening>(garage).unwrap();
    assert_eq!(g.width, Length::from_feet(16.0));
    assert_eq!(g.height, Length::from_feet(7.0));
    let d = s.document().get::<Opening>(door).unwrap().clone();

    // How far the door's 3D geometry reaches from the wall centre line (x = 0).
    let reach = |s: &Session| {
        let mesh = housecraft_api::geom::mesh::build(s.document(), None);
        let part = mesh.parts.iter().find(|p| p.object == door).unwrap();
        let idx = &mesh.indices
            [part.first_index as usize..(part.first_index + part.index_count) as usize];
        idx.iter()
            .map(|i| mesh.positions[*i as usize][0].abs() as f64 * 1000.0)
            .fold(0.0, f64::max)
    };
    let closed = reach(&s);
    assert!(closed < 120.0, "closed door stays in the wall: {closed}");
    s.apply(&[Command::Update {
        id: door,
        set: patch(serde_json::json!({ "open_percent": 100 })),
    }])
    .unwrap();
    let open = reach(&s);
    assert!(
        (open - d.width.mm()).abs() < 150.0,
        "fully open leaf sticks out about its width: {open}"
    );
    // Out-of-range values are rejected.
    assert!(
        s.apply(&[Command::Update {
            id: door,
            set: patch(serde_json::json!({ "open_percent": 150 })),
        }])
        .is_err()
    );
}

#[test]
fn switches_sharing_a_light_become_three_and_four_way() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_electrical", "electrical_kind": "ceiling_light", "at": ["10'","15'"]},
            {"op": "add_electrical", "electrical_kind": "switch", "at": ["1'","5'"]},
            {"op": "add_electrical", "electrical_kind": "switch", "at": ["19'","5'"]},
            {"op": "add_electrical", "electrical_kind": "dimmer", "at": ["10'","29'"]},
            {"op": "add_electrical", "electrical_kind": "switch", "at": ["5'","29'"]}]"#,
    );
    let [light, a, b, c, lone] = out.created[..] else {
        panic!("five objects")
    };
    // Wall devices snap to the wall face and face into the room.
    let sw = s.document().get::<Electrical>(a).unwrap();
    let inner = Length::from_inches(7.1875 / 2.0).mm();
    assert!(
        (sw.position.x().mm() - inner).abs() < 1.0,
        "{:?}",
        sw.position
    );
    assert!(
        sw.rotation.abs() < 1e-6,
        "faces east, into the room: {}",
        sw.rotation
    );
    // Ceiling fixtures stay where they were clicked.
    assert_eq!(
        s.document().get::<Electrical>(light).unwrap().position,
        housecraft_api::units::Point2::from_mm(
            Length::from_feet(10.0).mm(),
            Length::from_feet(15.0).mm()
        )
    );

    let connect = |s: &mut Session, sw: Id| {
        s.apply(&[Command::ConnectSwitch {
            switch: sw,
            fixtures: vec![light],
            disconnect: false,
        }])
        .unwrap();
    };
    connect(&mut s, a);
    assert_eq!(s.document().switch_labels()[&a], "S");
    connect(&mut s, b);
    let labels = s.document().switch_labels();
    assert_eq!((labels[&a].as_str(), labels[&b].as_str()), ("S3", "S3"));
    connect(&mut s, c);
    let labels = s.document().switch_labels();
    assert_eq!(labels[&c], "S4D");
    assert_eq!(labels[&lone], "S");

    // Switches can't control switches; deleting a fixture unwires it.
    assert!(
        s.apply(&[Command::ConnectSwitch {
            switch: a,
            fixtures: vec![b],
            disconnect: false
        }])
        .is_err()
    );
    s.apply(&[Command::Delete { ids: vec![light] }]).unwrap();
    assert!(
        s.document()
            .get::<Electrical>(a)
            .unwrap()
            .controls
            .is_empty()
    );
    assert_eq!(s.document().switch_labels()[&b], "S");
}

#[test]
fn sun_and_north_settings() {
    let mut s = boxed();
    apply(
        &mut s,
        r#"[{"op": "set_sun", "latitude": 40, "day_of_year": 80, "hour": 12},
            {"op": "set_north", "angle": -30, "pointer": null}]"#,
    );
    let st = &s.document().settings;
    assert!((st.sun.azimuth - 180.0).abs() < 1.0);
    assert!((st.sun.altitude - 50.0).abs() < 1.5);
    assert_eq!(st.north, 330.0);
    // Location needs all three parts; altitude is bounded.
    assert!(
        s.apply(&commands(r#"[{"op": "set_sun", "latitude": 40}]"#))
            .is_err()
    );
    assert!(
        s.apply(&commands(r#"[{"op": "set_sun", "altitude": 120}]"#))
            .is_err()
    );
    // Older files without these settings load with north up and a default sun.
    let mut v = serde_json::to_value(s.document()).unwrap();
    let settings = v["settings"].as_object_mut().unwrap();
    settings.remove("north");
    settings.remove("sun");
    let doc: Document = serde_json::from_value(v).unwrap();
    assert_eq!(doc.settings.north, 90.0);
    assert!(doc.settings.sun.shadows);
}

#[test]
fn demolished_walls_leave_the_3d_model() {
    let mut s = boxed();
    let wall = walls(&s)[0].id;
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_door", "wall": {}, "position": "10'"}}]"#,
            wall.0
        ),
    );
    let door = out.created[0];
    let parts = |s: &Session, id: Id| {
        housecraft_api::geom::mesh::build(s.document(), None)
            .parts
            .iter()
            .filter(|p| p.object == id)
            .count()
    };
    assert!(parts(&s, door) > 0);
    s.apply(&[Command::SetRenovation {
        ids: vec![door],
        status: Renovation::Demolish,
    }])
    .unwrap();
    assert_eq!(parts(&s, door), 0, "demolished door is filled in");
    s.apply(&[Command::SetRenovation {
        ids: vec![wall],
        status: Renovation::Demolish,
    }])
    .unwrap();
    assert_eq!(parts(&s, wall), 0);
    // Only walls, openings and electrical carry a status.
    let floor = s.document().floors_by_elevation()[0].id;
    assert!(
        s.apply(&[Command::SetRenovation {
            ids: vec![floor],
            status: Renovation::New
        }])
        .is_err()
    );
}

#[test]
fn demolished_walls_no_longer_divide_rooms() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_wall", "start": [0,"15'"], "end": ["20'","15'"], "interior": true}]"#,
    );
    let wall = *out
        .created
        .iter()
        .find(|id| s.document().get::<Wall>(**id).is_some())
        .unwrap();
    let floor = s.document().floors_by_elevation()[0].id;
    assert_eq!(s.document().rooms_on(floor).count(), 2);
    s.apply(&[Command::SetRenovation {
        ids: vec![wall],
        status: Renovation::Demolish,
    }])
    .unwrap();
    assert_eq!(
        s.document().rooms_on(floor).count(),
        1,
        "rooms follow the finished design"
    );
}

#[test]
fn door_sizes_by_type_and_new_styles() {
    let mut s = boxed();
    let wall = walls(&s)[0].id;
    let set = s.document().settings.active_default_set;
    s.apply(&[Command::Update {
        id: set,
        set: patch(serde_json::json!({
            "door": { "sizes": [{ "style": "barn", "width": "42\"", "height": "84\"" }] }
        })),
    }])
    .unwrap();
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_door", "wall": {w}, "position": "4'", "style": "barn"}},
                {{"op": "add_door", "wall": {w}, "position": "12'", "style": "atrium"}},
                {{"op": "add_door", "wall": {w}, "position": "18'", "style": "shower"}},
                {{"op": "add_door", "wall": {w}, "position": "24'", "style": "fixed"}}]"#,
            w = wall.0
        ),
    );
    let doc = s.document();
    let barn = doc.get::<Opening>(out.created[0]).unwrap();
    assert_eq!(barn.width, Length::from_inches(42.0));
    assert_eq!(barn.height, Length::from_inches(84.0));
    let atrium = doc.get::<Opening>(out.created[1]).unwrap();
    assert_eq!(atrium.width, Length::from_inches(60.0));
    assert_eq!(atrium.plan_swing, 90.0);
    // Every new style produces 3D geometry.
    let mesh = housecraft_api::geom::mesh::build(doc, None);
    for id in &out.created {
        assert!(
            mesh.parts.iter().any(|p| p.object == *id),
            "{id:?} has no 3D"
        );
    }
    // Plan swing is bounded.
    assert!(
        s.apply(&[Command::Update {
            id: out.created[1],
            set: patch(serde_json::json!({ "plan_swing": 200 })),
        }])
        .is_err()
    );
}

#[test]
fn exterior_stairs_use_exterior_defaults_and_have_nosings() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_stair", "start": ["10'","5'"], "direction": 90, "rise": "8'"},
            {"op": "add_stair", "start": ["30'","5'"], "direction": 90, "rise": "3'"}]"#,
    );
    let doc = s.document();
    let inside = doc.get::<Stair>(out.created[0]).unwrap();
    let outside = doc.get::<Stair>(out.created[1]).unwrap();
    assert_eq!(inside.width, Length::from_inches(36.0));
    assert_eq!(
        outside.width,
        Length::from_inches(42.0),
        "exterior defaults"
    );
    assert!(outside.railing_left && outside.railing_right);
    let g = housecraft_api::geom::stairs::geometry(doc, inside);
    assert!(
        g.steps
            .iter()
            .filter(|s| !s.landing)
            .all(|s| s.nosing.len() == 4)
    );
    assert_eq!(g.stringers.len(), 2);
    let g = housecraft_api::geom::stairs::geometry(doc, outside);
    assert!(
        g.steps.iter().all(|s| s.nosing.is_empty()),
        "exterior: no nosing"
    );
}

#[test]
fn layered_platforms_and_new_ceilings() {
    let mut s = boxed();
    let out = apply(&mut s, r#"[{"op": "add_floor"}]"#);
    let upper = out.created[0];
    let f = s.document().get::<Floor>(upper).unwrap();
    assert_eq!(
        f.platform.len(),
        3,
        "new floors get finish, subfloor and joists"
    );
    assert!((f.floor_structure.mm() - Length::from_inches(12.625).mm()).abs() < 1e-6);
    // Editing the layers keeps the structure depth in step.
    s.apply(&[Command::Update {
        id: upper,
        set: patch(serde_json::json!({ "platform": [
            { "name": "Tile", "thickness": "1/2\"" },
            { "name": "Joists", "thickness": "9 1/4\"" }
        ] })),
    }])
    .unwrap();
    let f = s.document().get::<Floor>(upper).unwrap();
    assert!((f.floor_structure.mm() - Length::from_inches(9.75).mm()).abs() < 1e-6);

    let ground = s.document().floors_by_elevation()[0].id;
    let room = s.document().rooms_on(ground).next().unwrap().id;
    let ceiling_tris = |s: &Session| {
        let mesh = housecraft_api::geom::mesh::build(s.document(), None);
        mesh.parts
            .iter()
            .filter(|p| {
                p.object == room && p.category == housecraft_api::geom::mesh::Category::Ceiling
            })
            .map(|p| p.index_count)
            .sum::<u32>()
    };
    // Plan-projected area of the room's ceiling surfaces, in m².
    let ceiling_area = |s: &Session| {
        let mesh = housecraft_api::geom::mesh::build(s.document(), None);
        let mut area = 0.0f64;
        for p in mesh.parts.iter().filter(|p| {
            p.object == room && p.category == housecraft_api::geom::mesh::Category::Ceiling
        }) {
            let idx =
                &mesh.indices[p.first_index as usize..(p.first_index + p.index_count) as usize];
            for t in idx.chunks(3) {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.positions[i as usize]);
                area += 0.5
                    * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() as f64;
            }
        }
        area
    };
    let flat_area = ceiling_area(&s);
    let flat = ceiling_tris(&s);
    for style in [
        serde_json::json!({ "style": "coffered", "spacing": "4'", "beam_width": "6\"", "depth": "8\"" }),
        serde_json::json!({ "style": "barrel", "rise": "2'" }),
    ] {
        s.apply(&[Command::Update {
            id: room,
            set: patch(serde_json::json!({ "ceiling": style })),
        }])
        .unwrap();
        assert!(ceiling_tris(&s) > flat * 4, "{style} adds geometry");
        if style["style"] == "barrel" {
            let a = ceiling_area(&s);
            assert!(
                (a - flat_area).abs() / flat_area < 0.02,
                "vault covers the room: {a} vs {flat_area}"
            );
        }
    }
    // Coffer beams must be narrower than their spacing.
    assert!(
        s.apply(&[Command::Update {
            id: room,
            set: patch(serde_json::json!({ "ceiling": { "style": "coffered", "spacing": "6\"", "beam_width": "6\"", "depth": "8\"" } })),
        }])
        .is_err()
    );
}

#[test]
fn roof_styles_set_every_wall_and_build() {
    for (style, gables) in [
        ("hip", 0),
        ("gable", 2),
        ("gambrel", 4),
        ("dutch_gable", 2),
        ("mansard", 0),
        ("shed", 3),
    ] {
        let mut s = boxed();
        apply(
            &mut s,
            &format!(r#"[{{"op": "set_roof_style", "style": "{style}"}}]"#),
        );
        let doc = s.document();
        let planes: Vec<&RoofPlane> = doc.iter::<RoofPlane>().collect();
        let g = planes.iter().filter(|p| p.gable).count();
        assert!(!planes.is_empty(), "{style}");
        assert!(
            g >= gables && (gables > 0 || g == 0),
            "{style}: {g} gable planes"
        );
        let floor = doc.floors_by_elevation()[0];
        assert!(floor.auto_roof, "{style} keeps rebuilding");
    }
}

#[test]
fn pier_and_beam_foundation() {
    let mut s = boxed();
    apply(
        &mut s,
        r#"[{"op": "build_foundation", "kind": "piers", "pier_spacing": "6'"}]"#,
    );
    let doc = s.document();
    let fnd = doc.floors_by_elevation()[0];
    assert_eq!(doc.walls_on(fnd.id).count(), 0, "piers, not stem walls");
    let l = housecraft_api::geom::piers::layout(doc, fnd).unwrap();
    // 20' x 30': perimeter piers at most 6' apart plus interior girders.
    assert!(l.piers.len() >= 20, "{} piers", l.piers.len());
    assert!(l.beams.len() > 4, "perimeter beams plus girders");
    assert!(l.top > l.bottom);
    let mesh = housecraft_api::geom::mesh::build(doc, None);
    assert!(mesh.parts.iter().any(|p| p.object == fnd.id));
    // Spacing is bounded.
    assert!(
        s.apply(&commands(
            r#"[{"op": "build_foundation", "kind": "piers", "pier_spacing": "1'"}]"#
        ))
        .is_err()
    );
}

#[test]
fn plumbing_and_hvac_symbols() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_electrical", "electrical_kind": "hose_bibb", "at": ["-1'","10'"]},
            {"op": "add_electrical", "electrical_kind": "floor_drain", "at": ["10'","10'"]},
            {"op": "add_electrical", "electrical_kind": "thermostat", "at": ["1'","20'"]},
            {"op": "add_electrical", "electrical_kind": "supply_register", "at": ["10'","20'"]},
            {"op": "add_electrical", "electrical_kind": "exhaust_fan", "at": ["5'","25'"]}]"#,
    );
    let doc = s.document();
    let bibb = doc.get::<Electrical>(out.created[0]).unwrap();
    // Snapped to the outside face of the west wall, facing west.
    assert!(bibb.position.x().mm() < 0.0);
    assert!(
        (bibb.rotation.rem_euclid(360.0) - 180.0).abs() < 1e-6,
        "{}",
        bibb.rotation
    );
    assert_eq!(bibb.electrical_kind.trade(), "Plumbing");
    let mesh = housecraft_api::geom::mesh::build(doc, None);
    for id in &out.created {
        assert!(
            mesh.parts.iter().any(|p| p.object == *id),
            "{id:?} has no 3D"
        );
    }
}

#[test]
fn gable_style_puts_the_ridge_along_the_long_side() {
    // 40' x 20' with an interior wall T-ing into both long walls.
    let mut s = Session::new(Document::new("Long"));
    apply(
        &mut s,
        r#"[{"op": "add_walls", "points": [[0,0], [0,"20'"], ["40'","20'"], ["40'",0]], "closed": true},
            {"op": "add_wall", "start": ["15'",0], "end": ["15'","20'"], "interior": true},
            {"op": "set_roof_style", "style": "gable"}]"#,
    );
    let doc = s.document();
    for w in doc
        .iter::<Wall>()
        .filter(|w| w.wall_type == doc.active_defaults().exterior_wall_type)
    {
        let runs_ns = (w.start.x() - w.end.x()).mm().abs() < 1.0;
        assert_eq!(
            w.roof.gable, runs_ns,
            "short (north-south) walls are the gable ends"
        );
    }
}

#[test]
fn schedules_group_and_export() {
    use housecraft_api::schedule::{ScheduleOptions, TableFormat, schedule};
    let mut s = boxed();
    let wall = walls(&s)[0].id;
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_door", "wall": {w}, "position": "5'"}},
                {{"op": "add_door", "wall": {w}, "position": "10'"}},
                {{"op": "add_door", "wall": {w}, "position": "20'", "style": "double"}},
                {{"op": "add_window", "wall": {w}, "position": "25'"}},
                {{"op": "add_schedule", "schedule_kind": "doors", "position": ["25'","10'"]}}]"#,
            w = wall.0
        ),
    );
    let doc = s.document();
    let doors = schedule(doc, ScheduleKind::Doors, &ScheduleOptions::default());
    assert_eq!(doors.columns[0], "Label");
    assert_eq!(
        doors.rows.len(),
        2,
        "two identical hinged doors share a row"
    );
    // Widest first: the 5' double door is D01.
    assert_eq!(doors.rows[0].cells[0], "D01");
    assert_eq!(doors.rows[0].cells[2], "Double Door");
    let hinged = &doors.rows[1];
    assert_eq!(hinged.cells[1], "2");
    assert_eq!(hinged.ids.len(), 2);

    let rooms = schedule(
        doc,
        ScheduleKind::Rooms,
        &ScheduleOptions {
            floor: None,
            columns: vec!["name".into(), "Area".into()],
        },
    );
    assert_eq!(rooms.columns, vec!["Name", "Area"]);
    assert_eq!(rooms.rows.len(), 1);

    let csv = doors.export(TableFormat::Csv);
    assert!(csv.starts_with("Label,Qty,Type,Width,Height"));
    assert!(
        doors
            .export(TableFormat::Xml)
            .contains("<label>D01</label>")
    );
    assert!(doors.export(TableFormat::Html).contains("<th>Label</th>"));
    assert!(doors.export(TableFormat::Txt).contains("Door Schedule"));

    // Placed schedules validate their columns.
    assert!(
        s.apply(&commands(r#"[{"op": "add_schedule", "schedule_kind": "doors", "position": [0,0], "columns": ["Nope"]}]"#))
            .is_err()
    );
}

#[test]
fn materials_list_takes_off_and_prices() {
    use housecraft_api::materials::{MaterialsOptions, materials, price_key};
    let mut s = boxed();
    let wall = walls(&s)[0].id;
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_door", "wall": {w}, "position": "5'"}},
                {{"op": "set_roof_style", "style": "gable"}},
                {{"op": "build_foundation", "kind": "slab"}}]"#,
            w = wall.0
        ),
    );
    let list = materials(s.document(), &MaterialsOptions::default());
    let find = |cat: &str, item: &str| {
        list.lines
            .iter()
            .find(|l| l.category == cat && l.item.contains(item))
            .unwrap_or_else(|| panic!("no {cat}: {item} in {:#?}", list.lines))
    };
    // 20' x 30' outside, 2x6 studs every 16" plus 2 per door.
    let studs = find("Framing", "2x6 stud,");
    assert!(
        studs.quantity > 70.0 && studs.quantity < 85.0,
        "{}",
        studs.quantity
    );
    assert_eq!(find("Doors", "Hinged Door").quantity, 1.0);
    let roof = find("Roofing", "Roofing");
    assert_eq!(roof.unit, "sq ft");
    assert!(
        roof.quantity > 600.0,
        "6/12 roof over 600 sq ft: {}",
        roof.quantity
    );
    assert!(find("Foundation", "slab").quantity > 5.0, "cu yd of slab");
    assert!(find("Trim", "Base molding").quantity > 90.0);

    // Pricing and markup.
    let key = price_key(&studs.category, &studs.item);
    let qty = studs.quantity;
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "set_material_price", "item": {key:?}, "price": 4.5}}, {{"op": "set_markup", "percent": 10}}]"#
        ),
    );
    let list = materials(s.document(), &MaterialsOptions::default());
    assert!((list.subtotal - qty * 4.5).abs() < 1e-6);
    assert!((list.total - qty * 4.5 * 1.1).abs() < 1e-6);
    let table = list.table();
    assert_eq!(table.rows.last().unwrap().cells[1], "Total");
    assert!(
        s.apply(&commands(r#"[{"op": "set_markup", "percent": -5}]"#))
            .is_err()
    );
}

#[test]
fn elevations_and_sections() {
    let mut s = boxed();
    apply(
        &mut s,
        r#"[{"op": "set_roof_style", "style": "gable"}, {"op": "auto_elevations"}]"#,
    );
    let doc = s.document();
    let names: Vec<String> = doc.iter::<SectionView>().map(|v| v.name.clone()).collect();
    for side in ["North", "South", "East", "West"] {
        assert!(names.iter().any(|n| n.starts_with(side)), "{names:?}");
    }
    // Every elevation looks back at the building: its left side faces the footprint centre.
    let centre = housecraft_api::geom::DVec2::new(
        Length::from_feet(10.0).mm(),
        Length::from_feet(15.0).mm(),
    );
    for v in doc.iter::<SectionView>() {
        let (a, b) = (housecraft_api::geom::v2(v.a), housecraft_api::geom::v2(v.b));
        let left = housecraft_api::geom::math::perp((b - a).normalize());
        assert!(
            left.dot(centre - (a + b) / 2.0) > 0.0,
            "{} looks away",
            v.name
        );
    }
    let room = doc.iter::<Room>().next().unwrap().id;
    let out = apply(
        &mut s,
        &format!(r#"[{{"op": "room_elevations", "room": {}}}]"#, room.0),
    );
    assert_eq!(out.created.len(), 4);
    let v = s.document().get::<SectionView>(out.created[0]).unwrap();
    assert_eq!(v.section_kind, SectionKind::Section);
    assert!(v.clip_sides && v.depth.is_some());
    assert!(
        s.apply(&commands(
            r#"[{"op": "add_section_view", "a": [0,0], "b": [0,0]}]"#
        ))
        .is_err(),
        "a view needs a real line"
    );
}
