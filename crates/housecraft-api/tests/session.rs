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
