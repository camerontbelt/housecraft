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
                {{"op": "add_layout", "name": "Sheets"}},
                {{"op": "add_callout", "text": "note", "at": [0,0]}},
                {{"op": "add_revision_cloud", "points": [[0,0],[1000,0],[0,1000]]}},
                {{"op": "add_cad_detail", "name": "Eave", "shapes": [{{"shape": {{"shape": "line", "a": [0,0], "b": [100,0]}}, "fill": "#808080"}}], "texts": [{{"position": [0,0], "text": "x"}}]}},
                {{"op": "add_block", "name": "B", "shapes": [{{"shape": {{"shape": "point", "at": [0,0]}}}}]}},
                {{"op": "add_cabinet", "cabinet_kind": "base", "at": ["1'","5'"], "sink": true}},
                {{"op": "add_countertop", "points": [["8'","8'"], ["12'","8'"], ["12'","11'"]], "holes": [{{"hole_kind": "round", "center": ["10'","9'"], "width": "2\"", "depth": "2\""}}]}},
                {{"op": "add_appliance", "appliance_kind": "toilet", "at": ["15'","1'"]}},
                {{"op": "add_backsplash", "points": [["1'","1'"], ["3'","1'"]]}},
                {{"op": "add_molding_profile", "name": "P", "points": [[0,0],[10,0],[0,10]]}},
                {{"op": "save_style", "name": "S"}},
                {{"op": "add_molding_polyline", "points": [["1'","2'"], ["3'","2'"]], "stack": [{{"height": 50, "depth": 10}}]}},
                {{"op": "add_terrain", "points": [["-20'","-20'"], ["40'","-20'"], ["40'","50'"]]}},
                {{"op": "add_elevation_data", "shape": {{"type": "hill", "center": [0,0], "radius": "10'", "height": "3'"}}}},
                {{"op": "add_site_feature", "points": [[0,0],["10'",0]], "feature": {{"type": "fence", "style": "picket", "height": "4'", "post_spacing": "8'"}}}},
                {{"op": "add_plant", "at": ["30'","30'"], "form": "conifer"}},
                {{"op": "add_framing_member", "role": "post", "a": [0,0,0], "b": [0,0,"8'"]}},
                {{"op": "add_joist_direction", "at": ["5'","5'"], "angle": 90}},
                {{"op": "add_deck", "points": [["20'",0],["30'",0],["30'","10'"],["20'","10'"]]}},
                {{"op": "add_dormer", "at": ["10'","3'"]}},
                {{"op": "add_skylight", "at": ["10'","27'"]}},
                {{"op": "orbit_walkthrough", "name": "Orbit"}},
                {{"op": "add_annotation3d", "mark": {{"type": "note", "at": [0,0,1000], "text": "Hi"}}}},
                {{"op": "add_solid", "shape": {{"type": "extrude", "points": [[0,0],[1000,0],[0,1000]], "height": 500}}}},
                {{"op": "add_picture", "path": "x.png", "position": [0,0], "width": "1'"}},
                {{"op": "add_symbol_def", "name": "S", "positions": [[0,0,0],[1,0,0],[0,-1,0],[0,0,1]], "triangles": [[0,1,2],[0,1,3]]}},
                {{"op": "add_camera", "name": "Cam", "projection": {{"type": "orthographic", "height": 5000}}, "eye": [0,0,1000], "target": [1000,1000,0]}}]"##,
            w = wall.0
        ),
    );
    let layout = s.document().iter::<Layout>().next().unwrap().id;
    let block = s.document().iter::<Block>().next().unwrap().id;
    let material = s.document().iter::<Material>().next().unwrap().id;
    let symbol_def = s.document().iter::<SymbolDef>().next().unwrap().id;
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "send_to_layout", "layout": {}, "source": {{"type": "text", "text": "notes"}}}},
                {{"op": "add_block_ref", "block": {}, "at": [0,0], "rotation": 30}},
                {{"op": "paint_material", "material": {}, "ids": [{}], "surface": "wall"}},
                {{"op": "add_symbol", "def": {}, "at": ["5'","5'"]}},
                {{"op": "add_material_region", "material": {}, "target": {{"type": "floor", "points": [[0,0],[1000,0],[0,1000]]}}}}]"#,
            layout.0, block.0, material.0, wall.0, symbol_def.0, material.0
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
    // Saving goes through a JSON value, where a field that clashes with the
    // "kind" tag would be lost.
    let value = serde_json::to_value(doc).unwrap();
    let back: Document = serde_json::from_value(value).unwrap();
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
    assert_eq!(g.width, Length::from_feet(9.0), "a single-car door, as in Chief");
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
            ..Default::default()
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

#[test]
fn drafting_in_views_and_details() {
    let mut s = boxed();
    apply(&mut s, r#"[{"op": "auto_elevations"}]"#);
    let view = s.document().iter::<SectionView>().next().unwrap().id;
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_dimension", "floor": {v}, "geometry": {{"type": "linear", "points": [[0,0], [0,"8'"]], "orientation": "vertical", "offset": "-2'"}}}},
                {{"op": "add_text", "floor": {v}, "position": [0,"9'"], "text": "Ridge"}},
                {{"op": "add_cad_detail", "name": "Eave"}}]"#,
            v = view.0
        ),
    );
    let doc = s.document();
    assert_eq!(doc.floor_of(out.created[0]), Some(view));
    let detail = out.created[2];
    assert_eq!(doc.get::<CadDetail>(detail).unwrap().name, "Eave");
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_cad", "floor": {}, "shape": {{"shape": "line", "a": [0,0], "b": ["2'",0]}}}}]"#,
            detail.0
        ),
    );
    // Drafting can't go on a wall, and walls can't go on a view.
    let wall = walls(&s)[0].id;
    assert!(
        s.apply(&commands(&format!(
            r#"[{{"op": "add_text", "floor": {}, "position": [0,0], "text": "x"}}]"#,
            wall.0
        )))
        .is_err()
    );
    assert!(
        s.apply(&commands(&format!(
            r#"[{{"op": "add_wall", "floor": {}, "start": [0,0], "end": ["8'",0]}}]"#,
            view.0
        )))
        .is_err()
    );
    // Deleting a floor removes its views and the drafting on them.
    let floor = s.document().floors_by_elevation()[0].id;
    apply(&mut s, r#"[{"op": "add_floor"}]"#);
    let out = apply(&mut s, &format!(r#"[{{"op": "delete", "ids": [{}]}}]"#, floor.0));
    assert!(out.deleted.contains(&view));
    let doc = s.document();
    assert_eq!(doc.count::<SectionView>(), 0);
    assert_eq!(doc.count::<Dimension>(), 0);
    assert!(doc.iter::<Text>().all(|t| t.text != "Ridge"));
    // The CAD detail is not on a floor, so it stays.
    assert_eq!(doc.count::<CadDetail>(), 1);
    assert!(doc.validate().is_empty(), "{:?}", doc.validate());
}

#[test]
fn blocks_make_place_mirror_and_explode() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_cad", "shape": {"shape": "line", "a": ["40'",0], "b": ["42'",0]}},
            {"op": "add_cad", "shape": {"shape": "circle", "center": ["41'","1'"], "radius": "6\""}}]"#,
    );
    let ids: Vec<u64> = out.created.iter().map(|i| i.0).collect();
    let out = apply(
        &mut s,
        &format!(r#"[{{"op": "add_block", "name": "Sink", "ids": {ids:?}}}]"#),
    );
    let doc = s.document();
    let block = out.created[0];
    let placed = out.created[1];
    let r = doc.get::<BlockRef>(placed).unwrap();
    // Inserted at the shapes' bottom-left corner: the line's start.
    assert!((r.at.x().feet() - 40.0).abs() < 1e-9);
    assert!(r.at.y().feet().abs() < 1e-9);
    assert_eq!(doc.iter::<Cad>().filter(|c| c.floor == block).count(), 2);
    assert!(doc.validate().is_empty(), "{:?}", doc.validate());
    assert!(
        s.apply(&commands(r#"[{"op": "add_block", "name": "Sink", "shapes": []}]"#))
            .is_err(),
        "names are unique"
    );

    // Mirror across x = 50': the copy flips and stays where the mirror puts it.
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "transform", "ids": [{}], "transform": {{"type": "mirror", "a": ["50'",0], "b": ["50'","10'"]}}}}]"#,
            placed.0
        ),
    );
    let r = s.document().get::<BlockRef>(placed).unwrap().clone();
    assert!(r.mirrored);
    let out = apply(&mut s, &format!(r#"[{{"op": "explode_block", "id": {}}}]"#, placed.0));
    let doc = s.document();
    assert!(doc.get::<BlockRef>(placed).is_none());
    let line = out
        .created
        .iter()
        .find_map(|id| match &doc.get::<Cad>(*id)?.shape {
            Shape::Line { a, b, .. } => Some((a.x().feet(), b.x().feet())),
            _ => None,
        })
        .unwrap();
    // The 40'..42' line mirrored about 50' lies at 60'..58'.
    assert!((line.0 - 60.0).abs() < 1e-6 && (line.1 - 58.0).abs() < 1e-6, "{line:?}");
    // Deleting the block removes its definition shapes and any copies.
    apply(
        &mut s,
        &format!(r#"[{{"op": "add_block_ref", "block": {b}, "at": [0,0]}}, {{"op": "delete", "ids": [{b}]}}]"#, b = block.0),
    );
    let doc = s.document();
    assert_eq!(doc.count::<BlockRef>(), 0);
    assert!(doc.iter::<Cad>().all(|c| c.floor != block));
    assert!(doc.validate().is_empty(), "{:?}", doc.validate());
}

#[test]
fn camera_default_sets_choose_drafting_layers() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_default_set", "name": "Sections"}, {"op": "add_layer", "name": "Section Notes"}, {"op": "auto_elevations"}]"#,
    );
    let (set, layer) = (out.created[0], out.created[1]);
    let view = s.document().iter::<SectionView>().next().unwrap().id;
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "update", "id": {set}, "set": {{"layers": {{"text": {layer}}}}}}},
                {{"op": "set_view_defaults", "view_kind": "elevation", "default_set": {set}}},
                {{"op": "add_text", "floor": {view}, "position": [0,0], "text": "A"}},
                {{"op": "add_text", "position": [0,0], "text": "B"}}]"#,
            set = set.0,
            layer = layer.0,
            view = view.0
        ),
    );
    let doc = s.document();
    let layer_of = |t: &str| doc.iter::<Text>().find(|x| x.text == t).unwrap().layer;
    assert_eq!(layer_of("A"), layer);
    assert_ne!(layer_of("B"), layer);
    // Deleting the set falls back to the active one.
    apply(&mut s, &format!(r#"[{{"op": "delete", "ids": [{}]}}]"#, set.0));
    assert_eq!(s.document().settings.view_defaults.elevation, None);
}

#[test]
fn materials_paint_replace_and_merge() {
    let mut s = boxed();
    let doc = s.document();
    let find = |doc: &Document, name: &str| doc.iter::<Material>().find(|m| m.name == name).unwrap().id;
    let (walnut, oak, tile) = (find(doc, "Walnut Flooring"), find(doc, "Oak Flooring"), find(doc, "Porcelain Tile"));
    let room = doc.iter::<Room>().next().unwrap().id;
    let wall = walls(&s)[0].id;
    let floor = doc.floors_by_elevation()[0].id;
    let index = |s: &Session, id: Id, surf: Surface| {
        MaterialIndex::new(s.document()).resolve(id, surf).map(|m| m.name.clone())
    };
    // Defaults come from the default set.
    assert_eq!(index(&s, room, Surface::Floor).as_deref(), Some("Oak Flooring"));
    // Paint the floor of the room, then the whole floor level.
    apply(
        &mut s,
        &format!(r#"[{{"op": "paint_material", "material": {}, "ids": [{}], "surface": "floor"}}]"#, walnut.0, room.0),
    );
    assert_eq!(index(&s, room, Surface::Floor).as_deref(), Some("Walnut Flooring"));
    apply(
        &mut s,
        &format!(r#"[{{"op": "paint_material", "material": {}, "scope": "floor", "ids": [{}], "surface": "wall"}}]"#, tile.0, wall.0),
    );
    assert_eq!(index(&s, wall, Surface::Wall).as_deref(), Some("Porcelain Tile"));
    assert!(s.document().iter::<MaterialAssignment>().any(|a| a.object == floor));
    // Default painter: reset the room's floor.
    apply(
        &mut s,
        &format!(r#"[{{"op": "paint_material", "material": null, "ids": [{}], "surface": "floor"}}]"#, room.0),
    );
    assert_eq!(index(&s, room, Surface::Floor).as_deref(), Some("Oak Flooring"));
    // Merge oak into walnut: the default slot follows, oak is gone.
    apply(
        &mut s,
        &format!(r#"[{{"op": "replace_material", "from": {}, "to": {}, "delete": true}}]"#, oak.0, walnut.0),
    );
    assert!(s.document().get::<Material>(oak).is_none());
    assert_eq!(index(&s, room, Surface::Floor).as_deref(), Some("Walnut Flooring"));
    // Deleting a material drops its paint and default slot; deleting the
    // painted floor's wall keeps the floor-level paint.
    apply(&mut s, &format!(r#"[{{"op": "delete", "ids": [{}]}}]"#, tile.0));
    assert_eq!(s.document().count::<MaterialAssignment>(), 0);
    assert!(s.document().validate().is_empty(), "{:?}", s.document().validate());
    // The 3D mesh shows the resolved colour.
    let mesh = housecraft_api::geom::mesh::build(s.document(), None);
    let walnut_rgb = housecraft_api::geom::mesh::look_of(s.document().get::<Material>(walnut).unwrap()).color;
    assert!(mesh.colors.contains(&walnut_rgb));
}

#[test]
fn cabinets_snap_fill_mirror_and_mesh() {
    let mut s = boxed();
    // The box's west wall runs along x = 0; its inside face is at the wall thickness.
    let out = apply(
        &mut s,
        r#"[{"op": "add_cabinet", "cabinet_kind": "base", "at": ["1'", "5'"]},
            {"op": "add_cabinet", "cabinet_kind": "base", "at": ["1'", "7'8\""], "sink": true},
            {"op": "add_cabinet", "cabinet_kind": "wall", "at": ["1'", "5'"]},
            {"op": "add_appliance", "appliance_kind": "refrigerator", "at": ["1'", "11'"]},
            {"op": "add_countertop", "points": [["8'","8'"], ["12'","8'"], ["12'","11'"], ["8'","11'"]],
             "holes": [{"hole_kind": "sink", "center": ["10'","9'6\""], "width": "30\"", "depth": "18\""}]}]"#,
    );
    let doc = s.document();
    let base = doc.get::<Cabinet>(out.created[0]).unwrap().clone();
    let t = 7.1875;
    // Backed onto the wall's inside face, facing east into the room.
    assert!((base.position.x().inches() - t / 2.0 - 0.0).abs() < 4.0, "{:?}", base.position);
    assert!(base.facing.abs() < 1e-6, "faces east: {}", base.facing);
    assert!(base.countertop.is_some() && base.toe_kick.is_some());
    let sink = doc.get::<Cabinet>(out.created[1]).unwrap().clone();
    assert!(sink.sink && (sink.width.inches() - 36.0).abs() < 1e-9);
    assert_eq!(sink.face[0].items, vec![FaceItem::Panel]);
    let wall_cab = doc.get::<Cabinet>(out.created[2]).unwrap();
    assert!((wall_cab.elevation.inches() - 54.0).abs() < 1e-9 && wall_cab.countertop.is_none());
    let fridge = doc.get::<Appliance>(out.created[3]).unwrap();
    assert!((fridge.width.inches() - 36.0).abs() < 1e-9);
    assert!(doc.validate().is_empty(), "{:?}", doc.validate());

    // The 24" base (centre 5') and the 36" sink base (centre 7'8") leave a 2"
    // gap; auto fillers split it.
    let out = apply(&mut s, r#"[{"op": "auto_fillers"}]"#);
    assert!(!out.modified.is_empty());
    let doc = s.document();
    let a = doc.get::<Cabinet>(base.id).unwrap();
    let b = doc.get::<Cabinet>(sink.id).unwrap();
    let total = a.filler_left.inches() + a.filler_right.inches() + b.filler_left.inches() + b.filler_right.inches();
    assert!((total - 2.0).abs() < 0.05, "fillers total {total}");

    // Mirror the sink base: its facing flips and left/right items swap.
    let mut copy = b.clone();
    copy.face[1].items = vec![FaceItem::DoorLeft];
    let id = b.id;
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "update", "id": {}, "set": {{"face": {}}}}},
                {{"op": "transform", "ids": [{}], "transform": {{"type": "mirror", "a": ["10'",0], "b": ["10'","10'"]}}}}]"#,
            id.0,
            serde_json::to_string(&copy.face).unwrap(),
            id.0
        ),
    );
    let m = s.document().get::<Cabinet>(id).unwrap();
    assert!((m.facing.rem_euclid(360.0) - 180.0).abs() < 1e-6, "{}", m.facing);
    assert_eq!(m.face[1].items, vec![FaceItem::DoorRight]);

    // The 3D model has cabinet boxes, doors, countertops and hardware.
    let mesh = housecraft_api::geom::mesh::build(s.document(), None);
    for surface in [Surface::CabinetBox, Surface::CabinetDoor, Surface::Countertop, Surface::Hardware] {
        assert!(mesh.surfaces.contains(&Some(surface)), "{surface:?}");
    }
}

#[test]
fn light_fixtures_light_the_model() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_electrical", "electrical_kind": "ceiling_light", "at": ["10'","15'"]},
            {"op": "add_electrical", "electrical_kind": "recessed_light", "at": ["5'","5'"]},
            {"op": "add_electrical", "electrical_kind": "outlet", "at": ["0'","5'"]}]"#,
    );
    let (ceiling, recessed) = (out.created[0], out.created[1]);
    let mesh = housecraft_api::geom::mesh::build(s.document(), None);
    assert_eq!(mesh.lights.len(), 2, "outlets give no light");
    let l = mesh.lights.iter().find(|l| l.object == ceiling).unwrap();
    // Just under an 8' ceiling.
    assert!(l.position[2] > 2.2 && l.position[2] < 2.44, "{:?}", l.position);
    assert!(l.color[0] > l.color[2], "warm white is redder than it is blue");
    // Dim one, switch the other off.
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "update", "id": {}, "set": {{"light": {{"on": true, "lumens": 1100, "dimmer": 0.5, "kelvin": 5000, "shadows": false}}}}}},
                {{"op": "update", "id": {}, "set": {{"light": {{"on": false, "lumens": 650}}}}}}]"#,
            ceiling.0, recessed.0
        ),
    );
    let mesh = housecraft_api::geom::mesh::build(s.document(), None);
    assert_eq!(mesh.lights.len(), 1);
    let l = mesh.lights[0];
    assert!((l.intensity - 1100.0 / 800.0 * 0.5).abs() < 1e-4 && !l.shadows);
    // Lenses of the switched-off light no longer glow.
    let lit = mesh
        .surfaces
        .iter()
        .zip(&mesh.pbr)
        .filter(|(s, p)| **s == Some(Surface::Lamp) && p[2] > 0.0)
        .count();
    assert!(lit > 0);
    // Light settings on an outlet are rejected.
    let outlet = out.created[2];
    assert!(
        s.apply(&commands(&format!(
            r#"[{{"op": "update", "id": {}, "set": {{"light": {{"on": true, "lumens": 100}}}}}}]"#,
            outlet.0
        )))
        .is_err()
    );
}

#[test]
fn place_and_replace_from_templates() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_cabinet", "cabinet_kind": "base", "at": ["1'","5'"]},
            {"op": "add_cabinet", "cabinet_kind": "base", "at": ["1'","9'"]},
            {"op": "add_cabinet", "cabinet_kind": "base", "at": ["1'","13'"], "width": "30\""}]"#,
    );
    let (a, b, c) = (out.created[0], out.created[1], out.created[2]);
    let doc = s.document();
    // A template is the cabinet without where it is.
    let mut template = serde_json::to_value(doc.get::<Cabinet>(c).unwrap()).unwrap();
    for k in ["id", "floor", "layer", "position", "facing"] {
        template.as_object_mut().unwrap().remove(k);
    }
    template["width"] = serde_json::to_value(Length::from_inches(18.0)).unwrap();
    // Place one by a wall: it backs onto it like the cabinet tool.
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "place_object", "kind": "cabinet", "template": {template}, "at": ["10'","1'"]}}]"#
        ),
    );
    let placed = s.document().get::<Cabinet>(out.created[0]).unwrap();
    assert!((placed.width.inches() - 18.0).abs() < 1e-9);
    assert!((placed.facing - 90.0).abs() < 1e-6, "faces north off the south wall: {}", placed.facing);
    // Replace one 24" cabinet: only that one changes.
    apply(
        &mut s,
        &format!(r#"[{{"op": "replace_with", "ids": [{}], "template": {template}}}]"#, a.0),
    );
    let w = |s: &Session, id: Id| s.document().get::<Cabinet>(id).unwrap().width.inches();
    assert!((w(&s, a) - 18.0).abs() < 1e-9 && (w(&s, b) - 24.0).abs() < 1e-9);
    // Undo and replace all identical: both 24" cabinets change, the 30" one doesn't.
    s.undo();
    template["width"] = serde_json::to_value(Length::from_inches(21.0)).unwrap();
    apply(
        &mut s,
        &format!(r#"[{{"op": "replace_with", "ids": [{}], "template": {template}, "all_identical": true}}]"#, a.0),
    );
    assert!((w(&s, a) - 21.0).abs() < 1e-9 && (w(&s, b) - 21.0).abs() < 1e-9);
    assert!((w(&s, c) - 30.0).abs() < 1e-9);
    // Positions are kept.
    assert!((s.document().get::<Cabinet>(b).unwrap().position.y().feet() - 9.0).abs() < 0.1);
    assert!(s.document().validate().is_empty(), "{:?}", s.document().validate());
}

#[test]
fn pictures_and_3d_symbols() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r##"[{"op": "add_picture", "path": "survey.png", "position": [0,0], "width": "20'", "pixels": [400, 200]},
            {"op": "add_symbol_def", "name": "Box", "positions": [[-500,-400,0],[500,-400,0],[500,0,0],[-500,0,0],[0,-200,800]],
             "triangles": [[0,1,4],[1,2,4],[2,3,4],[3,0,4],[0,2,1],[0,3,2]],
             "groups": [{"first": 0, "count": 6, "color": "#ff0000"}]}]"##,
    );
    let pic = s.document().get::<Picture>(out.created[0]).unwrap();
    assert!((pic.height.feet() - 10.0).abs() < 1e-9, "keeps proportions");
    let def = out.created[1];
    // Placed by a wall at its imported size, facing into the room.
    let out = apply(
        &mut s,
        &format!(r#"[{{"op": "add_symbol", "def": {}, "at": ["10'","1'"]}}]"#, def.0),
    );
    let sym = s.document().get::<Symbol>(out.created[0]).unwrap().clone();
    assert!((sym.width.mm() - 1000.0).abs() < 1e-6 && (sym.depth.mm() - 400.0).abs() < 1e-6);
    assert!((sym.facing - 90.0).abs() < 1e-6);
    // Its 3D triangles are in its colour and it has a plan outline.
    let mesh = housecraft_api::geom::mesh::build(s.document(), None);
    assert!(mesh.colors.contains(&[1.0, 0.0, 0.0, 1.0]));
    let outline = housecraft_api::geom::cabinets::symbol_outline(s.document(), &sym);
    assert!(outline.len() >= 4);
    // Every point of the outline lies in front of the wall face (y >= face).
    let face = sym.position.y().mm();
    assert!(outline.iter().all(|p| p.y >= face - 1e-6), "{outline:?}");
    // Deleting the model deletes its copies.
    apply(&mut s, &format!(r#"[{{"op": "delete", "ids": [{}]}}]"#, def.0));
    assert_eq!(s.document().count::<Symbol>(), 0);
    assert!(s.document().validate().is_empty(), "{:?}", s.document().validate());
}

#[test]
fn cabinet_details_schedule_and_dimensions() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_cabinet", "cabinet_kind": "base", "at": ["1'","5'"]},
            {"op": "add_cabinet", "cabinet_kind": "base", "at": ["1'","7'"], "sink": true, "width": "24\""},
            {"op": "add_cabinet", "cabinet_kind": "wall", "at": ["1'","5'"]}]"#,
    );
    let (a, sink, wall) = (out.created[0], out.created[1], out.created[2]);
    // Open doors, crown molding, end panel, legs, rounded waterfall top.
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "update", "id": {a}, "set": {{"open": 0.5, "ends": {{"left": true, "right": false, "legs": true}},
                 "countertop": {{"corner": {{"type": "round", "radius": "3\""}}, "corner_left": true, "waterfall_left": true}}}}}},
                {{"op": "update", "id": {w}, "set": {{"moldings": [{{"place": "top", "shape": "crown", "height": "3\"", "projection": "2\""}},
                                                                   {{"place": "bottom", "shape": "bead", "height": "1\"", "projection": "1\""}}]}}}}]"#,
            a = a.0,
            w = wall.0
        ),
    );
    let doc = s.document();
    assert!(doc.validate().is_empty(), "{:?}", doc.validate());
    assert_eq!(doc.get::<Cabinet>(a).unwrap().code(), "B24");
    assert_eq!(doc.get::<Cabinet>(sink).unwrap().code(), "SB24");
    assert_eq!(doc.get::<Cabinet>(wall).unwrap().code(), "W2430");
    // The 3D model builds (moldings, open doors, waterfall) without trouble.
    let mesh = housecraft_api::geom::mesh::build(doc, None);
    assert!(mesh.surfaces.contains(&Some(Surface::Countertop)));
    // The cabinet schedule lists door and drawer sizes.
    let t = housecraft_api::schedule::schedule(
        doc,
        ScheduleKind::Cabinets,
        &housecraft_api::schedule::ScheduleOptions::default(),
    );
    let csv = t.export(housecraft_api::schedule::TableFormat::Csv);
    assert!(csv.contains("B24") && csv.contains("SB24") && csv.contains(" @ "), "{csv}");
    // Auto dimensions: one chain across the run of two base cabinets.
    let out = apply(&mut s, r#"[{"op": "auto_cabinet_dimensions"}]"#);
    assert_eq!(out.created.len(), 1);
    let dim = s.document().get::<Dimension>(out.created[0]).unwrap();
    let DimensionGeometry::Linear { points, .. } = &dim.geometry else { panic!() };
    assert!(points.len() >= 3, "{points:?}");
    // A backsplash along the wall.
    apply(&mut s, r#"[{"op": "add_backsplash", "points": [["7\"","4'"], ["7\"","8'"]]}]"#);
    assert!(s.document().validate().is_empty());
}

#[test]
fn trim_moldings_cladding_and_profiles() {
    let mut s = boxed();
    // New plans: exterior walls carry lap siding and corner boards; living
    // room types have a baseboard.
    let doc = s.document();
    let ext = doc.get::<WallType>(doc.active_defaults().exterior_wall_type).unwrap();
    assert!(matches!(ext.cladding, Some(Cladding::Lap { .. })));
    let mesh = housecraft_api::geom::mesh::build(doc, None);
    assert!(mesh.surfaces.contains(&Some(Surface::Trim)), "baseboard / corner boards");
    // A custom profile from a CAD box, used on a chair rail and a molding polyline.
    let out = apply(
        &mut s,
        r#"[{"op": "add_cad", "shape": {"shape": "polyline", "points": [[0,0],[20,0],[20,60],[0,60]], "closed": true}}]"#,
    );
    let cad = out.created[0];
    let out = apply(
        &mut s,
        &format!(r#"[{{"op": "add_molding_profile", "name": "Cap", "from_cad": {}}}]"#, cad.0),
    );
    let profile = out.created[0];
    let p = s.document().get::<MoldingProfile>(profile).unwrap();
    assert_eq!(p.points[0], [0.0, 0.0]);
    let spec = format!(
        r#"{{"profile": {{"type": "custom", "profile": {}}}, "height": "2\"", "depth": "1\""}}"#,
        profile.0
    );
    let room = s.document().iter::<Room>().next().unwrap().id;
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "update", "id": {room}, "set": {{"moldings": [{{"place": "chair", "offset": "32\"", "spec": {spec}}}]}}}},
                {{"op": "add_molding_polyline", "points": [["2'","2'"], ["8'","2'"]], "elevation": "36\"", "stack": [{spec}, {spec}]}}]"#,
            room = room.0
        ),
    );
    let poly = out.created[0];
    assert!(s.document().validate().is_empty(), "{:?}", s.document().validate());
    // Deleting the profile falls back to flat boards everywhere.
    apply(&mut s, &format!(r#"[{{"op": "delete", "ids": [{}]}}]"#, profile.0));
    let doc = s.document();
    assert!(doc.validate().is_empty(), "{:?}", doc.validate());
    assert_eq!(doc.get::<MoldingPolyline>(poly).unwrap().stack[0].profile, ProfileRef::default());
    // A shadow board on the fascia builds.
    let set = doc.active_defaults().id;
    apply(
        &mut s,
        &format!(r#"[{{"op": "update", "id": {}, "set": {{"roof": {{"shadow_board": "2\""}}}}}}, {{"op": "build_roof"}}]"#, set.0),
    );
    let mesh = housecraft_api::geom::mesh::build(s.document(), None);
    assert!(!mesh.positions.is_empty());
}

#[test]
fn textures_patterns_regions_and_styles() {
    let mut s = boxed();
    let doc = s.document();
    let tile = doc.iter::<Material>().find(|m| m.name == "Porcelain Tile").unwrap().id;
    let wall = walls(&s)[0].id;
    // A herringbone pattern on the tile material, a floor inlay and a wall wainscot.
    let out = apply(
        &mut s,
        &format!(
            r##"[{{"op": "update", "id": {t}, "set": {{"pattern": {{"kind": "herringbone", "tile_width": "12\"", "tile_height": "3\"", "grout": "0.125\"", "grout_color": "#404040"}}, "texture_angle": 45}}}},
                {{"op": "add_material_region", "material": {t}, "target": {{"type": "floor", "points": [["4'","4'"], ["10'","4'"], ["10'","10'"], ["4'","10'"]]}}}},
                {{"op": "add_material_region", "material": {t}, "target": {{"type": "wall", "wall": {w}, "exterior": false, "u0": "2'", "u1": "8'", "z0": 0, "z1": "4'"}}}}]"##,
            t = tile.0,
            w = wall.0
        ),
    );
    assert_eq!(out.created.len(), 2);
    let doc = s.document();
    assert!(doc.validate().is_empty(), "{:?}", doc.validate());
    // The 3D mesh uses the pattern as a texture.
    let mesh = housecraft_api::geom::mesh::build(doc, None);
    assert!(mesh.textures.iter().any(|t| matches!(t, housecraft_api::geom::mesh::TextureKey::Pattern(..))));
    assert!(mesh.tex.iter().any(|t| *t >= 0.0));
    // The plan hatches the patterned inlay.
    // Styles: save, change a default, apply back.
    let out = apply(&mut s, r#"[{"op": "save_style", "name": "Farmhouse"}]"#);
    let style = out.created[0];
    let doc = s.document();
    let walnut = doc.iter::<Material>().find(|m| m.name == "Walnut Flooring").unwrap().id;
    apply(
        &mut s,
        &format!(r#"[{{"op": "paint_material", "material": {}, "scope": "plan", "surface": "floor"}}]"#, walnut.0),
    );
    apply(&mut s, &format!(r#"[{{"op": "apply_style", "style": {}}}]"#, style.0));
    let d = s.document().active_defaults();
    assert_ne!(d.materials[&Surface::Floor], walnut, "the style's floor came back");
    // Copy a material for one wall: it gets its own copy.
    let paint = s.document().active_defaults().materials[&Surface::Wall];
    let out = apply(
        &mut s,
        &format!(r#"[{{"op": "copy_material_for", "material": {}, "ids": [{}]}}]"#, paint.0, wall.0),
    );
    let copy = out.created[0];
    assert!(s.document().get::<Material>(copy).unwrap().name.ends_with("(copy)"));
    // Deleting the wall drops its region.
    apply(&mut s, &format!(r#"[{{"op": "delete", "ids": [{}]}}]"#, wall.0));
    assert_eq!(s.document().count::<MaterialRegion>(), 1);
    assert!(s.document().validate().is_empty(), "{:?}", s.document().validate());
}

#[test]
fn shapes_combine_and_become_solids() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_cad", "shape": {"shape": "polyline", "points": [[0,0],[2000,0],[2000,2000],[0,2000]], "closed": true}},
            {"op": "add_cad", "shape": {"shape": "polyline", "points": [[1000,1000],[3000,1000],[3000,3000],[1000,3000]], "closed": true}},
            {"op": "add_cad", "shape": {"shape": "circle", "center": [5000,5000], "radius": 300}}]"#,
    );
    let (a, b, circle) = (out.created[0], out.created[1], out.created[2]);
    let joined = apply(
        &mut s,
        &format!(r#"[{{"op": "cad_boolean", "ids": [{}, {}], "operation": "union"}}]"#, a.0, b.0),
    );
    assert_eq!(joined.created.len(), 1);
    let doc = s.document();
    let Shape::Polyline { points, closed } = &doc.get::<Cad>(joined.created[0]).unwrap().shape else {
        panic!("not a polyline");
    };
    assert!(*closed && points.len() == 8, "{points:?}");
    let solid = apply(
        &mut s,
        &format!(r#"[{{"op": "extrude_cad", "ids": [{}], "height": "3'"}}]"#, joined.created[0].0),
    );
    let doc = s.document();
    assert_eq!(doc.count::<Cad>(), 1, "the shape became a solid");
    assert_eq!(doc.get::<Solid>(solid.created[0]).unwrap().shape.height(), Length::from_feet(3.0));
    apply(&mut s, &format!(r#"[{{"op": "revolve_cad", "id": {}}}]"#, circle.0));
    let mesh = housecraft_api::geom::mesh::build(s.document(), None);
    assert_eq!(mesh.parts.iter().filter(|p| s.document().kind_of(p.object) == Some(ObjectKind::Solid)).count(), 2);
    // Solids move, copy and round-trip like other objects.
    apply(
        &mut s,
        &format!(r#"[{{"op": "transform", "ids": [{}], "transform": {{"type": "translate", "dx": 100, "dy": 0}}}}]"#, solid.created[0].0),
    );
    assert!(s.document().validate().is_empty(), "{:?}", s.document().validate());
}

#[test]
fn symbols_convert_cut_counters_light_rooms_and_dress_stairs() {
    let mut s = boxed();
    // A cabinet becomes a symbol of the same size in the same place.
    let cab = apply(&mut s, r#"[{"op": "add_cabinet", "cabinet_kind": "base", "at": ["5'","10'"], "facing": 0}]"#).created[0];
    let before = housecraft_api::geom::mesh::build(s.document(), None);
    let bounds = |m: &housecraft_api::geom::mesh::Mesh, id: Id| {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for part in m.parts.iter().filter(|p| p.object == id) {
            for i in &m.indices[part.first_index as usize..(part.first_index + part.index_count) as usize] {
                let p = m.positions[*i as usize];
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        (lo, hi)
    };
    let (lo0, hi0) = bounds(&before, cab);
    let out = apply(&mut s, &format!(r#"[{{"op": "convert_to_symbol", "ids": [{}], "name": "Vanity"}}]"#, cab.0));
    let (def, sym) = (out.created[0], out.created[1]);
    assert!(s.document().get::<Cabinet>(cab).is_none());
    let after = housecraft_api::geom::mesh::build(s.document(), None);
    let (lo1, hi1) = bounds(&after, sym);
    for k in 0..3 {
        assert!((lo0[k] - lo1[k]).abs() < 0.002 && (hi0[k] - hi1[k]).abs() < 0.002, "{lo0:?} {hi0:?} vs {lo1:?} {hi1:?}");
    }
    // As a sink, it cuts a counter it stands in.
    let top = apply(
        &mut s,
        r#"[{"op": "add_countertop", "points": [["2'","8'"], ["8'","8'"], ["8'","12'"], ["2'","12'"]]}]"#,
    )
    .created[0];
    let tris = |s: &Session| {
        let m = housecraft_api::geom::mesh::build(s.document(), None);
        m.parts.iter().filter(|p| p.object == top).map(|p| p.index_count).sum::<u32>()
    };
    let plain = tris(&s);
    apply(&mut s, &format!(r#"[{{"op": "update", "id": {}, "set": {{"behavior": {{"type": "sink"}}}}}}]"#, def.0));
    assert!(tris(&s) > plain, "the counter has a hole");
    // As a light, it lights the room.
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "update", "id": {}, "set": {{"behavior": {{"type": "light", "light": {{"on": true, "lumens": 1600}}}}}}}}]"#,
            def.0
        ),
    );
    let m = housecraft_api::geom::mesh::build(s.document(), None);
    assert!(m.lights.iter().any(|l| l.object == sym && (l.intensity - 2.0).abs() < 1e-3));
    // Wall-hung symbols hang at their height.
    apply(
        &mut s,
        &format!(r#"[{{"op": "update", "id": {}, "set": {{"behavior": {{"type": "wall_hung", "height": "4'"}}}}}}]"#, def.0),
    );
    let hung = apply(&mut s, &format!(r#"[{{"op": "add_symbol", "def": {}, "at": ["10'","1'"]}}]"#, def.0)).created[0];
    assert_eq!(s.document().get::<Symbol>(hung).unwrap().elevation, Length::from_feet(4.0));
    // A stair dressed with the symbol as balusters and newels.
    let stair = apply(&mut s, r#"[{"op": "add_stair", "start": ["15'","5'"], "direction": 90}]"#).created[0];
    let plain = housecraft_api::geom::mesh::build(s.document(), None);
    let count = |m: &housecraft_api::geom::mesh::Mesh| {
        m.parts.iter().filter(|p| p.object == stair).map(|p| p.index_count).sum::<u32>()
    };
    apply(
        &mut s,
        &format!(r#"[{{"op": "update", "id": {}, "set": {{"baluster": {}, "newel": {}}}}}]"#, stair.0, def.0, def.0),
    );
    assert!(count(&housecraft_api::geom::mesh::build(s.document(), None)) > count(&plain));
    // Pasting the symbol's definition again reuses it.
    let doc = s.document();
    let objects = vec![
        serde_json::to_value(doc.object(def).unwrap()).unwrap(),
        serde_json::to_value(doc.object(hung).unwrap()).unwrap(),
    ];
    let out = s
        .apply(&[housecraft_api::Command::InsertObjects {
            objects,
            floor: None,
            dx: Length::from_feet(2.0),
            dy: Length::ZERO,
        }])
        .unwrap();
    assert_eq!(out.created.len(), 1, "only the placed symbol is new");
    assert_eq!(s.document().count::<SymbolDef>(), 1);
    // Deleting the symbol drops the stair's custom parts.
    apply(&mut s, &format!(r#"[{{"op": "delete", "ids": [{}]}}]"#, def.0));
    assert_eq!(s.document().get::<Stair>(stair).unwrap().baluster, None);
    assert!(s.document().validate().is_empty(), "{:?}", s.document().validate());
}

#[test]
fn window_grilles_shutters_and_plan_symbols() {
    let mut s = boxed();
    let wall = walls(&s)[0].id;
    let win = apply(&mut s, &format!(r#"[{{"op": "add_window", "wall": {}, "position": "10'"}}]"#, wall.0)).created[0];
    let count = |s: &Session| {
        let m = housecraft_api::geom::mesh::build(s.document(), None);
        m.parts.iter().filter(|p| p.object == win).map(|p| p.index_count).sum::<u32>()
    };
    let plain = count(&s);
    let cad = apply(
        &mut s,
        r#"[{"op": "add_cad", "shape": {"shape": "polyline", "points": [[0,0],[100,100],[200,0]], "closed": false}}]"#,
    )
    .created[0];
    apply(&mut s, &format!(r#"[{{"op": "muntins_from_cad", "opening": {}, "ids": [{}]}}]"#, win.0, cad.0));
    let m = s.document().get::<Opening>(win).unwrap().details.muntins.clone().unwrap();
    assert_eq!(m.custom, vec![[[0.0, 0.0], [0.5, 1.0]], [[0.5, 1.0], [1.0, 0.0]]]);
    let block = apply(&mut s, r#"[{"op": "add_block", "name": "W", "shapes": [{"shape": {"shape": "line", "a": [0,0], "b": [100,0]}}]}]"#).created[0];
    apply(
        &mut s,
        &format!(
            r##"[{{"op": "update", "id": {}, "set": {{"details": {{"muntins": {{"across": 2, "up": 3}}, "shutters": {{"style": "board", "color": "#204030"}}, "treatment": {{"kind": "blinds", "color": "#ffffff", "open": 0.3}}, "plan_block": {}}}}}}}]"##,
            win.0, block.0
        ),
    );
    assert!(count(&s) > plain, "grille, shutters and blinds add to the window's mesh");
    // Deleting the block puts the standard symbol back.
    apply(&mut s, &format!(r#"[{{"op": "delete", "ids": [{}]}}]"#, block.0));
    assert_eq!(s.document().get::<Opening>(win).unwrap().details.plan_block, None);
    assert!(s.document().validate().is_empty(), "{:?}", s.document().validate());
}

#[test]
fn doors_place_from_library_templates() {
    let mut s = boxed();
    let wall = walls(&s)[0].id;
    let out = apply(
        &mut s,
        &format!(
            r##"[{{"op": "place_opening", "wall": {}, "at": [0, "10'"], "template": {{"opening_kind": {{"type": "window", "style": "casement"}}, "width": "2'", "height": "5'", "sill": "2'", "hinge": "left", "swing_in": true, "details": {{"shutters": {{"style": "panel", "color": "#203040"}}}}}}}}]"##,
            wall.0
        ),
    );
    let o = s.document().get::<Opening>(out.created[0]).unwrap();
    assert_eq!(o.width, Length::from_feet(2.0));
    assert!(o.details.shutters.is_some());
    assert!((o.position.mm() - Length::from_feet(10.0).mm()).abs() < 1.0);
    assert!(s.document().validate().is_empty(), "{:?}", s.document().validate());
}

#[test]
fn plot_plans_from_bearings_and_site_plan_details() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_plot_lines", "start": ["-20'","-20'"], "setback": "10'", "legs": [
            {"bearing": "N 0°00'00\" E", "distance": "80'"},
            {"bearing": "S 90°00'00\" E", "distance": "60'"},
            {"bearing": "S 0 W", "distance": "80'"},
            {"bearing": "N 90 W", "distance": "60'"}]}]"#,
    );
    let doc = s.document();
    let lots: Vec<&Cad> = out.created.iter().filter_map(|id| doc.get::<Cad>(*id)).collect();
    assert_eq!(lots.len(), 2, "the lot and its setback line");
    let Shape::Polyline { points, closed } = &lots[0].shape else { panic!() };
    assert!(*closed && points.len() == 4);
    assert!((points[2].x().mm() - Length::from_feet(40.0).mm()).abs() < 1.0);
    assert_eq!(out.created.iter().filter(|id| doc.get::<Text>(**id).is_some()).count(), 4);
    assert!(s.apply(&commands(r#"[{"op": "add_plot_lines", "start": [0,0], "legs": [{"bearing": "NE", "distance": 10}]}]"#)).is_err());
    let out = apply(&mut s, r#"[{"op": "site_plan_detail", "name": "Site"}]"#);
    let detail = out.created[0];
    assert!(s.document().iter::<Cad>().filter(|c| c.floor == detail).count() >= 1, "the house footprint");
}

#[test]
fn framing_builds_rebuilds_and_lists() {
    let mut s = boxed();
    let wall = walls(&s)[0].id;
    apply(&mut s, &format!(r#"[{{"op": "add_window", "wall": {}, "position": "10'"}}, {{"op": "build_roof"}}]"#, wall.0));
    let manual = apply(&mut s, r#"[{"op": "add_framing_member", "role": "post", "a": ["5'","5'",0], "b": ["5'","5'","8'"]}]"#).created[0];
    apply(&mut s, r#"[{"op": "build_framing"}]"#);
    let count = |s: &Session, r: FramingRole| s.document().iter::<FramingMember>().filter(|m| m.role == r).count();
    assert!(count(&s, FramingRole::Stud) > 40, "{}", count(&s, FramingRole::Stud));
    assert_eq!(count(&s, FramingRole::Header), 1);
    assert!(count(&s, FramingRole::CeilingJoist) > 10);
    assert!(count(&s, FramingRole::Rafter) > 10);
    assert!(count(&s, FramingRole::Ridge) >= 1);
    let total = s.document().count::<FramingMember>();
    // Rebuilding replaces automatic members and keeps the hand-placed one.
    apply(&mut s, r#"[{"op": "build_framing"}]"#);
    assert_eq!(s.document().count::<FramingMember>(), total);
    assert!(s.document().get::<FramingMember>(manual).is_some());
    // Rebuilding one wall touches only that wall's members.
    let out = apply(&mut s, &format!(r#"[{{"op": "build_framing", "hosts": [{}], "parts": {{"floors": false, "ceilings": false, "roofs": false}}}}]"#, wall.0));
    let doc = s.document();
    assert!(out.created.iter().all(|id| doc.get::<FramingMember>(*id).is_some_and(|m| m.host == Some(wall))));
    assert_eq!(doc.count::<FramingMember>(), total);
    // The framing layer is hidden in plan.
    let layer = doc.active_defaults().layers.framing.unwrap();
    assert!(!doc.iter::<LayerSet>().next().unwrap().display(layer).visible);
    // Cut list and buy list.
    let t = housecraft_api::schedule::schedule(
        doc,
        ScheduleKind::Framing,
        &housecraft_api::schedule::ScheduleOptions::default(),
    );
    let csv = t.export(housecraft_api::schedule::TableFormat::Csv);
    assert!(csv.contains("2x6") && csv.contains("Stud") && csv.contains("Rafter"), "{csv}");
    let list = housecraft_api::materials::materials(doc, &Default::default());
    assert!(list.lines.iter().any(|l| l.category == "Framing" && l.item.starts_with("2x6 x 8'")), "studs bought as 8' stock");
    // A wall framing detail.
    let out = apply(&mut s, &format!(r#"[{{"op": "wall_framing_detail", "wall": {}}}]"#, wall.0));
    let detail = out.created[0];
    assert!(s.document().iter::<Cad>().filter(|c| c.floor == detail).count() > 10);
    // Clearing keeps hand-placed members.
    apply(&mut s, r#"[{"op": "clear_framing"}]"#);
    assert_eq!(s.document().count::<FramingMember>(), 1);
    assert!(s.document().validate().is_empty(), "{:?}", s.document().validate());
}

#[test]
fn trusses_posts_and_beams() {
    let mut s = boxed();
    apply(&mut s, r#"[{"op": "build_roof"}]"#);
    let set = s.document().active_defaults().id;
    apply(&mut s, &format!(r#"[{{"op": "update", "id": {}, "set": {{"framing": {{"trusses": true}}}}}}]"#, set.0));
    apply(&mut s, r#"[{"op": "build_framing"}]"#);
    let doc = s.document();
    let count = |r: FramingRole| doc.iter::<FramingMember>().filter(|m| m.role == r).count();
    assert_eq!(count(FramingRole::Rafter), 0, "trusses replace rafters");
    assert_eq!(count(FramingRole::CeilingJoist), 0, "and ceiling joists");
    assert!(count(FramingRole::Chord) > 30, "{}", count(FramingRole::Chord));
    assert!(count(FramingRole::Web) > 30);
    let chord = doc.iter::<FramingMember>().find(|m| m.role == FramingRole::Chord).unwrap().id;
    let out = apply(&mut s, &format!(r#"[{{"op": "truss_detail", "member": {}}}]"#, chord.0));
    let detail = out.created[0];
    let pieces = s.document().iter::<Cad>().filter(|c| c.floor == detail).count();
    assert!((5..=20).contains(&pieces), "one truss, {pieces} pieces");
    // A two-ply beam on a post.
    let out = apply(
        &mut s,
        r#"[{"op": "add_framing_member", "role": "post", "a": ["10'","10'",0], "b": ["10'","10'","8'"]},
            {"op": "add_framing_member", "role": "beam", "a": [0,"10'","7'"], "b": ["20'","10'","7'"], "plies": 2}]"#,
    );
    let beam = s.document().get::<FramingMember>(out.created[1]).unwrap();
    assert_eq!(beam.size_name(), "2-ply 2x10");
    assert!(s.document().validate().is_empty(), "{:?}", s.document().validate());
}

#[test]
fn match_properties_copies_settings_not_placement() {
    let mut s = boxed();
    let w = walls(&s);
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_window", "wall": {a}, "position": "5'", "width": "4'"}},
                {{"op": "add_window", "wall": {b}, "position": "3'"}}]"#,
            a = w[0].id.0,
            b = w[1].id.0
        ),
    );
    let (src, dst) = (out.created[0], out.created[1]);
    s.apply(&[Command::MatchProperties { from: src, to: vec![dst], fields: Vec::new() }]).unwrap();
    let doc = s.document();
    let (a, b) = (doc.get::<Opening>(src).unwrap(), doc.get::<Opening>(dst).unwrap());
    assert_eq!(a.width, b.width);
    assert_eq!(b.wall, w[1].id, "stays in its own wall");
    assert!((b.position.feet() - 3.0).abs() < 1e-9, "stays where it was");
    // Different kinds don't match.
    let err = s.apply(&[Command::MatchProperties { from: src, to: vec![w[0].id], fields: Vec::new() }]).unwrap_err();
    assert!(err.to_string().contains("can't match"), "{err}");
}

#[test]
fn room_polyline_and_living_area_override() {
    let mut s = boxed();
    let room = s.document().iter::<Room>().next().unwrap().id;
    let living = |s: &Session| query::summary(s.document()).living_area;
    let before = living(&s);
    let out = s.apply(&[Command::RoomPolyline { room }]).unwrap();
    let cad = s.document().get::<Cad>(out.created[0]).unwrap();
    assert!(matches!(&cad.shape, Shape::Polyline { points, closed: true } if points.len() == 4));
    s.apply(&[Command::Update { id: room, set: patch(serde_json::json!({ "living_area": false })) }]).unwrap();
    assert_ne!(living(&s), before, "excluded room drops out of the living area");
}

#[test]
fn wall_reference_offsets() {
    let doc = Document::new("x");
    let d = doc.active_defaults();
    let t = doc.get::<WallType>(d.exterior_wall_type).unwrap();
    let half = t.thickness().mm() / 2.0;
    assert_eq!(t.centerline_offset(WallReference::Center), 0.0);
    assert!((t.centerline_offset(WallReference::Exterior) + half).abs() < 1e-9);
    assert!((t.centerline_offset(WallReference::Interior) - half).abs() < 1e-9);
    let m = t.centerline_offset(WallReference::Main);
    assert!(m.abs() < half, "main layer lies within the wall: {m}");
}

#[test]
fn line_types_add_use_and_merge() {
    let mut s = boxed();
    let wall_layer = walls(&s)[0].layer;
    s.apply(&[
        Command::SetLineType { index: None, line_type: LineType { name: "A".into(), pattern: vec![1.0, 1.0] } },
        Command::SetLineType { index: None, line_type: LineType { name: "B".into(), pattern: vec![2.0, 1.0] } },
        Command::Update { id: wall_layer, set: patch(serde_json::json!({ "line_style": { "custom": 1 } })) },
    ])
    .unwrap();
    assert_eq!(housecraft_api::ops_line_type_uses(s.document(), 1), 1);
    // Deleting A shifts B down to index 0, still used by the layer.
    s.apply(&[Command::DeleteLineType { index: 0, merge_into: LineStyle::Solid }]).unwrap();
    let l = s.document().get::<Layer>(wall_layer).unwrap();
    assert_eq!(l.line_style, LineStyle::Custom(0));
    assert_eq!(l.line_style.dash(&s.document().settings), &[2.0, 1.0]);
    s.apply(&[Command::DeleteLineType { index: 0, merge_into: LineStyle::Dashed }]).unwrap();
    assert_eq!(s.document().get::<Layer>(wall_layer).unwrap().line_style, LineStyle::Dashed);
}

#[test]
fn paste_into_another_plan_matches_layers_by_name() {
    let src = boxed();
    let w = walls(&src)[0].clone();
    let layer = src.document().get::<Layer>(w.layer).unwrap().clone();
    let wt = src.document().get::<WallType>(w.wall_type).unwrap().clone();
    let mut dst = Session::new(Document::new("Other"));
    let objects = vec![
        serde_json::to_value(Object::Wall(w)).unwrap(),
        serde_json::to_value(Object::Layer(layer.clone())).unwrap(),
        serde_json::to_value(Object::WallType(wt)).unwrap(),
    ];
    let before = dst.document().iter::<Layer>().count();
    let out = dst.apply(&[Command::InsertObjects { objects, floor: None, dx: Length::ZERO, dy: Length::ZERO }]).unwrap();
    let doc = dst.document();
    let pasted = doc.get::<Wall>(out.created[0]).unwrap();
    assert_eq!(doc.get::<Layer>(pasted.layer).unwrap().name, layer.name);
    assert_eq!(doc.iter::<Layer>().count(), before, "the same-named layer was reused");
}

#[test]
fn standard_plan_views_have_their_own_sets() {
    let mut s = boxed();
    let views = s.document().iter::<PlanView>().count();
    s.apply(&[Command::AddStandardPlanViews]).unwrap();
    s.apply(&[Command::AddStandardPlanViews]).unwrap();
    let doc = s.document();
    assert_eq!(doc.iter::<PlanView>().count(), views + 5, "added once");
    let c = doc.iter::<PlanView>().find(|v| v.name == "Construction Plan").unwrap();
    assert_eq!(doc.get::<DefaultSet>(c.default_set).unwrap().labels.windows, "{size}");
}

#[test]
fn auto_exterior_dimensions_follow_the_walls() {
    let mut s = boxed();
    s.apply(&[Command::AddExteriorDimensions { floor: None, options: None }]).unwrap();
    let longest = |s: &Session| {
        s.document()
            .iter::<Dimension>()
            .filter(|d| d.auto)
            .filter_map(|d| match &d.geometry {
                DimensionGeometry::Linear { points, .. } => {
                    Some(points.iter().map(|p| p.1.mm()).fold(f64::MIN, f64::max))
                }
                _ => None,
            })
            .fold(f64::MIN, f64::max)
    };
    let before = longest(&s);
    let w = walls(&s)[0].id; // the 30' west wall
    s.apply(&[Command::SetWallLength { id: w, length: Length::from_feet(40.0), anchor: housecraft_api::LengthAnchor::Start }]).unwrap();
    assert!(longest(&s) > before + 3000.0, "dimensions grew with the building");
}

#[test]
fn object_info_and_dimension_styles() {
    let mut s = boxed();
    let w = walls(&s)[0].id;
    let info = InfoFields { code: "W-1".into(), ..Default::default() };
    s.apply(&[Command::SetObjectInfo { id: w, info: Some(info) }]).unwrap();
    assert_eq!(s.document().settings.object_info[&w].code, "W-1");
    let dims = s.document().active_defaults().dimensions.clone();
    s.apply(&[Command::SetDimensionStyle { name: "Framing".into(), defaults: Some(dims) }]).unwrap();
    assert_eq!(s.document().settings.dimension_styles.len(), 1);
    s.apply(&[Command::SetDimensionStyle { name: "Framing".into(), defaults: None }]).unwrap();
    assert!(s.document().settings.dimension_styles.is_empty());
}

#[test]
fn bay_and_corner_windows() {
    let mut s = boxed();
    let w = walls(&s)[1].clone(); // the 20' north wall
    let before = walls(&s).len();
    s.apply(&[Command::AddBayWindow {
        wall: w.id,
        position: Length::from_feet(10.0),
        width: Length::from_feet(8.0),
        depth: Length::from_feet(2.0),
        bay: housecraft_api::BayStyle::Bay,
        faces: None,
        style: None,
    }])
    .unwrap();
    // The wall is split in two and three bay walls added, each with a window.
    assert_eq!(walls(&s).len(), before + 4);
    assert_eq!(s.document().iter::<Opening>().count(), 3);
    let first = walls(&s)[0].id;
    s.apply(&[Command::AddCornerWindow { wall: first, at_start: false, width: Length::from_feet(3.0), height: None, sill: None, style: None }])
        .unwrap();
    assert_eq!(s.document().iter::<Opening>().count(), 5);
}

#[test]
fn roof_pitch_pivots_on_eave_or_ridge() {
    let mut s = boxed();
    s.apply(&[Command::BuildRoof { floor: None, auto_rebuild: false }]).unwrap();
    let plane = s.document().iter::<RoofPlane>().find(|r| !r.gable).unwrap().clone();
    let low = |r: &RoofPlane| r.outline.iter().map(|p| p.2.mm()).fold(f64::MAX, f64::min);
    let high = |r: &RoofPlane| r.outline.iter().map(|p| p.2.mm()).fold(f64::MIN, f64::max);
    s.apply(&[Command::SetRoofPitch { id: plane.id, pitch: plane.pitch + 10.0, keep_ridge: false }]).unwrap();
    let r = s.document().get::<RoofPlane>(plane.id).unwrap().clone();
    assert!((low(&r) - low(&plane)).abs() < 1.0, "eave kept");
    assert!(high(&r) > high(&plane) + 100.0, "ridge rose");
    s.apply(&[Command::SetRoofPitch { id: plane.id, pitch: plane.pitch, keep_ridge: true }]).unwrap();
    let r2 = s.document().get::<RoofPlane>(plane.id).unwrap().clone();
    assert!((high(&r2) - high(&r)).abs() < 1.0, "ridge kept");
}

#[test]
fn space_plan_boxes_become_typed_rooms_and_get_outlets() {
    let mut s = Session::new(Document::new("Plan"));
    let kitchen = s.document().iter::<RoomType>().find(|t| t.name == "Kitchen").unwrap().id;
    let out = apply(
        &mut s,
        r#"[{"op": "add_cad", "shape": {"shape": "polyline", "closed": true, "points": [[0,0],["12'",0],["12'","10'"],[0,"10'"]]}},
            {"op": "add_text", "position": ["6'","5'"], "text": "Kitchen"},
            {"op": "add_cad", "shape": {"shape": "polyline", "closed": true, "points": [["12'",0],["24'",0],["24'","10'"],["12'","10'"]]}}]"#,
    );
    let boxes = vec![out.created[0], out.created[2]];
    s.apply(&[Command::ConvertSpacePlan { ids: boxes }]).unwrap();
    let doc = s.document();
    // Seven walls: the shared side once.
    assert_eq!(doc.iter::<Wall>().count(), 7);
    assert_eq!(doc.iter::<Room>().count(), 2);
    assert!(doc.iter::<Room>().any(|r| r.room_type == kitchen));
    s.apply(&[Command::AutoOutlets { floor: None }]).unwrap();
    let doc = s.document();
    let gfci = doc.iter::<Electrical>().filter(|e| e.electrical_kind == ElectricalKind::GfciOutlet).count();
    let plain = doc.iter::<Electrical>().filter(|e| e.electrical_kind == ElectricalKind::Outlet).count();
    assert!(gfci >= 3 && plain >= 3, "kitchen GFCI {gfci}, other {plain}");
}

#[test]
fn schedules_mix_kinds_filter_renovation_and_number_from_a_start() {
    use housecraft_api::schedule::{ScheduleOptions, schedule};
    let mut s = boxed();
    let w = walls(&s)[0].id;
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_window", "wall": {w}, "position": "5'"}},
                {{"op": "add_window", "wall": {w}, "position": "15'", "width": "4'"}},
                {{"op": "add_door", "wall": {w}, "position": "25'"}},
                {{"op": "add_cad", "shape": {{"shape": "polyline", "points": [[0,0],["10'",0],["10'","10'"],[0,"10'"]], "closed": true}}}}]"#,
            w = w.0
        ),
    );
    let win = s.document().iter::<Opening>().find(|o| !o.opening_kind.is_door()).unwrap().id;
    s.apply(&[Command::Update { id: win, set: patch(serde_json::json!({"details": {"u_factor": 0.28, "shgc": 0.3}, "renovation": "existing"})) }]).unwrap();
    let doc = s.document();
    let t = schedule(doc, ScheduleKind::Windows, &ScheduleOptions { start: Some(5), ..Default::default() });
    let col = |name: &str| t.columns.iter().position(|c| c == name).unwrap();
    let labels: Vec<&str> = t.rows.iter().map(|r| r.cells[col("Label")].as_str()).collect();
    assert_eq!(labels, ["W05", "W06"]);
    let rated = t.rows.iter().find(|r| r.ids.contains(&win)).unwrap();
    assert_eq!(rated.cells[col("U-Factor")], "0.28");
    assert!(!rated.cells[col("Glass Area")].is_empty());
    // Existing windows only.
    let t = schedule(doc, ScheduleKind::Windows, &ScheduleOptions { renovation: Some(Renovation::Existing), ..Default::default() });
    assert_eq!(t.rows.len(), 1);
    // Doors and windows in one table, with a Kind column.
    let t = schedule(doc, ScheduleKind::Doors, &ScheduleOptions { also: vec![ScheduleKind::Windows], ..Default::default() });
    assert_eq!(t.columns[0], "Kind");
    assert_eq!(t.rows.len(), 3);
    // Closed CAD shapes with their area.
    let t = schedule(doc, ScheduleKind::Shapes, &ScheduleOptions::default());
    assert_eq!(t.rows.len(), 1);
    assert!(t.rows[0].cells[t.columns.iter().position(|c| c == "Area").unwrap()].starts_with("100"), "{:?}", t.rows[0]);
}

#[test]
fn materials_components_insulation_and_master_prices() {
    use housecraft_api::materials::{materials, price_key};
    let mut s = boxed();
    let wt = walls(&s)[0].wall_type;
    let mut t = s.document().get::<WallType>(wt).unwrap().clone();
    t.layers[0].r_value = 21.0;
    s.apply(&[Command::Update { id: wt, set: patch(serde_json::json!({"layers": t.layers})) }]).unwrap();
    let doc = s.document();
    let list = materials(doc, &Default::default());
    assert!(list.lines.iter().any(|l| l.category == "Insulation" && l.item == "R-21 wall insulation" && l.quantity > 0.0), "{:?}", list.lines.iter().map(|l| &l.item).collect::<Vec<_>>());
    // A wall's own components, replacing what the take-off counted for it.
    let w = walls(&s)[0].id;
    let before = materials(s.document(), &Default::default()).lines.iter().filter(|l| l.ids.contains(&w)).count();
    assert!(before > 0);
    let comps: ObjectComponents = serde_json::from_value(serde_json::json!({"replace": true, "items": [{"category": "Framing", "item": "Steel stud wall", "quantity": 12, "unit": "lin ft", "price": 4.5}]})).unwrap();
    s.apply(&[Command::SetComponents { id: w, components: Some(comps) }]).unwrap();
    let list = materials(s.document(), &Default::default());
    let mine: Vec<_> = list.lines.iter().filter(|l| l.ids.contains(&w)).collect();
    assert_eq!(mine.len(), 1);
    assert_eq!((mine[0].item.as_str(), mine[0].total), ("Steel stud wall", 54.0));
    // Prices merged in keep the plan's own unless overwriting.
    let key = price_key("Framing", "Steel stud wall");
    s.apply(&[Command::SetMaterialPrice { item: "Doors: x".into(), price: Some(10.0) }]).unwrap();
    let prices: std::collections::BTreeMap<String, f64> = [("Doors: x".to_string(), 99.0), (key.clone(), 3.0)].into();
    s.apply(&[Command::ImportPrices { prices: prices.clone(), overwrite: false }]).unwrap();
    assert_eq!(s.document().settings.prices["Doors: x"], 10.0);
    assert_eq!(s.document().settings.prices[&key], 3.0);
    s.apply(&[Command::ImportPrices { prices, overwrite: true }]).unwrap();
    assert_eq!(s.document().settings.prices["Doors: x"], 99.0);
}

#[test]
fn auto_dimension_tiers_presets_grids_and_view_heights() {
    let mut s = boxed();
    let w = walls(&s);
    // A window in the west wall, an interior wall meeting the south wall.
    apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_window", "wall": {w}, "position": "10'"}},
                {{"op": "add_wall", "start": ["8'",0], "end": ["8'","12'"], "interior": true}}]"#,
            w = w[0].id.0
        ),
    );
    let count = |s: &Session| s.document().iter::<Dimension>().filter(|d| d.auto).count();
    s.apply(&[Command::AddExteriorDimensions { floor: None, options: None }]).unwrap();
    let plain = count(&s);
    let all = AutoDims { openings: true, interior_walls: true, corners: true, overall: true, surfaces: false };
    s.apply(&[Command::AddExteriorDimensions { floor: None, options: Some(all) }]).unwrap();
    let tiered = count(&s);
    assert_eq!(tiered, plain + 2, "an openings row on the west and a partition row on the south");
    // The chosen tiers stay when walls change.
    apply(&mut s, r#"[{"op": "add_wall", "start": ["14'","30'"], "end": ["14'","20'"], "interior": true}]"#);
    assert_eq!(count(&s), tiered + 1, "a partition row on the north too");
    s.apply(&[Command::SetAutoDimPreset { name: "Permit".into(), dims: Some(all) }]).unwrap();
    assert_eq!(s.document().settings.auto_dim_presets[0].name, "Permit");

    // Grid lines number and letter themselves by position.
    let floor = s.document().floors_by_elevation()[0].id;
    let p = |x: f64, y: f64| housecraft_api::units::Point2::from_mm(x * 304.8, y * 304.8);
    s.apply(&[
        Command::AddGridLine { floor: None, a: p(10.0, -5.0), b: p(10.0, 35.0) },
        Command::AddGridLine { floor: None, a: p(0.0, -5.0), b: p(0.0, 35.0) },
        Command::AddGridLine { floor: None, a: p(-5.0, 0.0), b: p(25.0, 0.0) },
    ])
    .unwrap();
    let labels = |s: &Session| -> Vec<String> {
        let mut v: Vec<String> = s.document().iter::<Callout>().filter(|c| c.floor == floor && c.marker.is_some()).map(|c| c.text.clone()).collect();
        v.sort();
        v
    };
    assert_eq!(labels(&s), ["1", "2", "A"]);
    let first = s.document().iter::<Cad>().find(|c| matches!(c.shape, Shape::Line { a, .. } if a == p(10.0, -5.0))).unwrap().id;
    s.apply(&[Command::Delete { ids: vec![first] }]).unwrap();
    assert_eq!(labels(&s), ["1", "A"], "the bubble goes with its line");

    // Heights in an elevation view.
    apply(&mut s, r#"[{"op": "set_roof_style", "style": "gable"}, {"op": "add_section_view", "name": "South", "a": ["-10'","-10'"], "b": ["30'","-10'"]}]"#);
    let view = s.document().iter::<SectionView>().next().unwrap().id;
    let out = s.apply(&[Command::ElevationDimensions { view }, Command::StoryPole { view }]).unwrap();
    let doc = s.document();
    let dims: Vec<&Dimension> = doc.iter::<Dimension>().filter(|d| d.floor == view).collect();
    assert_eq!(dims.len(), 2);
    let DimensionGeometry::Linear { points, .. } = &dims[0].geometry else { panic!() };
    assert!(points.len() >= 3, "floor, ceiling and ridge");
    assert!(doc.iter::<Text>().any(|t| t.floor == view && t.text.starts_with("Ridge")));
    assert!(out.created.len() > 4);
}

#[test]
fn plan_check_finds_egress_doors_walls_and_stairs() {
    use housecraft_api::check::{Severity, plan_check};
    let mut s = boxed();
    let bedroom = s.document().iter::<RoomType>().find(|t| t.name.to_lowercase().contains("bed")).unwrap().id;
    let room = s.document().iter::<Room>().next().unwrap().id;
    s.apply(&[Command::Update { id: room, set: patch(serde_json::json!({"room_type": bedroom.0})) }]).unwrap();
    let issues = plan_check(s.document());
    let has = |issues: &[housecraft_api::check::Issue], topic: &str| issues.iter().any(|i| i.topic == topic);
    assert!(has(&issues, "Egress"), "{issues:?}");
    assert!(has(&issues, "Rooms"), "no door");
    assert!(has(&issues, "Roof"));
    // A big low window fixes egress; a door fixes access.
    let w = walls(&s)[0].id;
    apply(&mut s, &format!(r#"[{{"op": "add_window", "wall": {w}, "position": "10'", "width": "3'", "height": "4'", "sill": "2'"}}, {{"op": "add_door", "wall": {w}, "position": "20'"}}]"#, w = w.0));
    let issues = plan_check(s.document());
    assert!(!has(&issues, "Egress") && !has(&issues, "Rooms"), "{issues:?}");
    // A wall end that just misses another wall.
    apply(&mut s, r#"[{"op": "add_wall", "start": ["10'","1\""], "end": ["10'","12'"], "interior": true}]"#);
    let issues = plan_check(s.document());
    assert!(issues.iter().any(|i| i.topic == "Walls" && i.severity == Severity::Problem), "{issues:?}");
}

#[test]
fn framing_options_purlins_tails_balloon_furring_girts_trays() {
    let count = |s: &Session, r: FramingRole| s.document().iter::<FramingMember>().filter(|m| m.role == r).count();
    let set_framing = |s: &mut Session, v: serde_json::Value| {
        let id = s.document().settings.active_default_set;
        s.apply(&[Command::Update { id, set: patch(serde_json::json!({ "framing": v })) }]).unwrap();
    };
    // Rafter spacing per plane, purlins and tails.
    let mut s = boxed();
    apply(&mut s, r#"[{"op": "build_roof"}, {"op": "build_framing"}]"#);
    let rafters = count(&s, FramingRole::Rafter);
    let plane = s.document().iter::<RoofPlane>().find(|r| !r.gable).unwrap().id;
    s.apply(&[Command::Update { id: plane, set: patch(serde_json::json!({"rafter_spacing": "12\""})) }]).unwrap();
    set_framing(&mut s, serde_json::json!({"purlin_spacing": "24\"", "rafter_tails": "plumb"}));
    apply(&mut s, r#"[{"op": "build_framing"}]"#);
    assert!(count(&s, FramingRole::Rafter) > rafters + 5, "{} vs {rafters}", count(&s, FramingRole::Rafter));
    assert!(count(&s, FramingRole::Purlin) > 4);
    assert!(s.document().iter::<FramingMember>().filter(|m| m.role == FramingRole::Rafter).all(|m| m.tail == TailCut::Plumb));
    // A tray ceiling: some ceiling joists step up, blocking frames the step.
    let room = s.document().iter::<Room>().next().unwrap().id;
    s.apply(&[Command::Update { id: room, set: patch(serde_json::json!({"ceiling": {"style": "tray", "inset": "2'", "rise": "1'"}})) }]).unwrap();
    set_framing(&mut s, serde_json::json!({"purlin_spacing": 0.0}));
    apply(&mut s, r#"[{"op": "build_framing"}]"#);
    let zs: Vec<f64> = s.document().iter::<FramingMember>().filter(|m| m.role == FramingRole::CeilingJoist).map(|m| m.a.mm()[2]).collect();
    let (lo, hi) = (zs.iter().copied().fold(f64::MAX, f64::min), zs.iter().copied().fold(f64::MIN, f64::max));
    assert!((hi - lo - 304.8).abs() < 1.0, "{lo} {hi}");

    // Post-and-girt walls.
    let mut s = boxed();
    set_framing(&mut s, serde_json::json!({"wall_style": "post_and_girt"}));
    apply(&mut s, r#"[{"op": "build_framing", "parts": {"floors": false, "ceilings": false, "roofs": false}}]"#);
    assert_eq!(count(&s, FramingRole::Stud), 0);
    assert!(count(&s, FramingRole::Post) >= 8 && count(&s, FramingRole::Girt) > 8);

    // Furring: a framed furring layer gets its own studs.
    let mut s = boxed();
    apply(&mut s, r#"[{"op": "build_framing", "parts": {"floors": false, "ceilings": false, "roofs": false}}]"#);
    let studs = count(&s, FramingRole::Stud);
    let wt = walls(&s)[0].wall_type;
    let mut t = s.document().get::<WallType>(wt).unwrap().clone();
    t.layers.push(WallLayer { name: "Furring".into(), thickness: Length::from_inches(1.5), material: String::new(), main: false, r_value: 0.0 });
    s.apply(&[Command::Update { id: wt, set: patch(serde_json::json!({"layers": t.layers})) }]).unwrap();
    apply(&mut s, r#"[{"op": "build_framing", "parts": {"floors": false, "ceilings": false, "roofs": false}}]"#);
    assert!(count(&s, FramingRole::Stud) > studs + 20);

    // Balloon framing over two storeys.
    let mut s = boxed();
    let f2 = apply(&mut s, r#"[{"op": "add_floor"}]"#).created[0];
    apply(&mut s, &format!(r#"[{{"op": "add_walls", "floor": {f}, "points": [[0,0], [0,"30'"], ["20'","30'"], ["20'",0]], "closed": true}}]"#, f = f2.0));
    set_framing(&mut s, serde_json::json!({"balloon": true}));
    apply(&mut s, r#"[{"op": "build_framing", "parts": {"floors": false, "ceilings": false, "roofs": false}}]"#);
    let doc = s.document();
    assert!(doc.iter::<FramingMember>().filter(|m| m.floor == f2).all(|m| m.role != FramingRole::Stud));
    let tall = doc.iter::<FramingMember>().filter(|m| m.role == FramingRole::Stud).map(|m| m.length().mm()).fold(0.0, f64::max);
    assert!(tall > 16.0 * 304.8, "{tall}");
    assert_eq!(doc.iter::<FramingMember>().filter(|m| m.role == FramingRole::Ribbon).count(), 4);

    // Framing reference markers in a view.
    let mut s = boxed();
    apply(&mut s, r#"[{"op": "add_section_view", "name": "S", "a": ["-10'","-10'"], "b": ["30'","-10'"]}]"#);
    let view = s.document().iter::<SectionView>().next().unwrap().id;
    s.apply(&[Command::FramingMarkers { view }]).unwrap();
    assert!(s.document().iter::<Text>().any(|t| t.floor == view && t.text.contains("T.O. plate")));
}

#[test]
fn scatter_copies_inside_regions_and_along_paths() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_plant", "at": ["50'","50'"], "name": "Oak"},
            {"op": "add_cad", "shape": {"shape": "polyline", "points": [["40'",0],["60'",0],["60'","20'"],["40'","20'"]], "closed": true}},
            {"op": "add_cad", "shape": {"shape": "polyline", "points": [["70'",0],["90'",0]], "closed": false}}]"#,
    );
    let (plant, area, path) = (out.created[0], out.created[1], out.created[2]);
    let out = s.apply(&[Command::Scatter { source: plant, region: area, count: 25, seed: 7, size_variation: 0.3, rotate: true }]).unwrap();
    assert_eq!(out.created.len(), 25);
    let doc = s.document();
    let ft = 304.8;
    let heights: Vec<f64> = out.created.iter().map(|id| doc.get::<Plant>(*id).unwrap().height.mm()).collect();
    assert!(heights.iter().any(|h| (h - heights[0]).abs() > 1.0), "sizes vary");
    for id in &out.created {
        let p = doc.get::<Plant>(*id).unwrap().at;
        assert!(p.x().mm() > 40.0 * ft && p.x().mm() < 60.0 * ft && p.y().mm() > 0.0 && p.y().mm() < 20.0 * ft);
    }
    // The same seed gives the same layout.
    let again = s.apply(&[Command::Scatter { source: plant, region: area, count: 25, seed: 7, size_variation: 0.3, rotate: true }]).unwrap();
    let doc = s.document();
    assert_eq!(doc.get::<Plant>(again.created[3]).unwrap().at, doc.get::<Plant>(out.created[3]).unwrap().at);
    let along = s.apply(&[Command::Scatter { source: plant, region: path, count: 10, seed: 1, size_variation: 0.0, rotate: false }]).unwrap();
    let doc = s.document();
    assert!(along.created.iter().all(|id| (doc.get::<Plant>(*id).unwrap().at.y().mm()).abs() < 3.0 * ft));
}

#[test]
fn framing_lookouts_blocking_headers_rooms_splices_steel() {
    let count = |s: &Session, r: FramingRole| s.document().iter::<FramingMember>().filter(|m| m.role == r).count();
    let set_framing = |s: &mut Session, v: serde_json::Value| {
        let id = s.document().settings.active_default_set;
        s.apply(&[Command::Update { id, set: patch(serde_json::json!({ "framing": v })) }]).unwrap();
    };
    let mut s = boxed();
    // A gable roof: rakes get lookouts, eaves get frieze blocking.
    let w = walls(&s);
    for wall in [w[0].id, w[2].id] {
        s.apply(&[Command::Update { id: wall, set: patch(serde_json::json!({"roof": {"gable": true}})) }]).unwrap();
    }
    apply(&mut s, r#"[{"op": "build_roof"}, {"op": "build_framing"}]"#);
    let blocks = count(&s, FramingRole::Blocking);
    set_framing(&mut s, serde_json::json!({"lookouts": true, "eave_blocking": true, "wall_material": "steel"}));
    apply(&mut s, r#"[{"op": "build_framing"}]"#);
    assert!(count(&s, FramingRole::Blocking) > blocks + 8, "{} vs {blocks}", count(&s, FramingRole::Blocking));
    assert!(s.document().iter::<FramingMember>().filter(|m| m.role == FramingRole::Stud).all(|m| m.material == FramingMaterial::Steel));
    // A flush 11 7/8" header.
    let out = apply(&mut s, &format!(r#"[{{"op": "add_window", "wall": {}, "position": "10'"}}]"#, w[1].id.0));
    let win = out.created[0];
    s.apply(&[Command::Update { id: win, set: patch(serde_json::json!({"details": {"header": {"depth": "11 7/8\"", "flush": true}}})) }]).unwrap();
    apply(&mut s, &format!(r#"[{{"op": "build_framing", "hosts": [{}], "parts": {{"floors": false, "ceilings": false, "roofs": false}}}}]"#, w[1].id.0));
    let doc = s.document();
    let header = doc.iter::<FramingMember>().find(|m| m.role == FramingRole::Header && m.host == Some(w[1].id)).unwrap();
    let o = doc.get::<Opening>(win).unwrap();
    let head = o.sill.mm() + o.height.mm();
    assert!(header.depth.mm() > 11.0 * 25.4, "flush header fills to the plates: {}", header.depth.mm());
    assert!((header.a.mm()[2] - header.depth.mm() / 2.0 - head).abs() < 1.0);
    assert_eq!(doc.iter::<FramingMember>().filter(|m| m.role == FramingRole::Cripple && m.host == Some(w[1].id) && m.a.mm()[2] > head).count(), 0);
    // Rebuilding a room rebuilds its walls only.
    let room = doc.iter::<Room>().next().unwrap().id;
    let out = s.apply(&[Command::BuildFraming { floor: None, parts: serde_json::from_str(r#"{"floors": false, "ceilings": false, "roofs": false}"#).unwrap(), hosts: vec![room] }]).unwrap();
    let doc = s.document();
    assert!(!out.created.is_empty() && out.created.iter().all(|id| doc.get::<FramingMember>(*id).is_some_and(|m| doc.kind_of(m.host.unwrap()) == Some(ObjectKind::Wall))));
    // Ceiling joists lap over a bearing wall.
    let mut s = boxed();
    let bw = apply(&mut s, r#"[{"op": "add_wall", "start": ["10'",0], "end": ["10'","30'"], "interior": true}]"#).created[0];
    s.apply(&[Command::Update { id: bw, set: patch(serde_json::json!({"shape": {"bearing": true}})) }]).unwrap();
    apply(&mut s, r#"[{"op": "build_framing", "parts": {"walls": false, "floors": false, "roofs": false}}]"#);
    let plain = count(&s, FramingRole::CeilingJoist);
    set_framing(&mut s, serde_json::json!({"joist_splice": "lap"}));
    apply(&mut s, r#"[{"op": "build_framing", "parts": {"walls": false, "floors": false, "roofs": false}}]"#);
    assert!(count(&s, FramingRole::CeilingJoist) > plain + 5, "{} vs {plain}", count(&s, FramingRole::CeilingJoist));
}

#[test]
fn break_lines_and_subfloor_markers_shape_the_ground() {
    let mut s = boxed();
    apply(&mut s, r#"[{"op": "add_terrain", "points": [["-50'","-50'"],["70'","-50'"],["70'","80'"],["-50'","80'"]]}]"#);
    let height = |s: &Session, x: f64, y: f64| {
        let doc = s.document();
        let t = doc.iter::<Terrain>().next().unwrap();
        housecraft_api::geom::terrain::surface(doc, t).unwrap().height_at(housecraft_api::geom::DVec2::new(x * 304.8, y * 304.8))
    };
    let shapes: Vec<ElevationShape> = serde_json::from_str(
        r#"[{"type": "break_line", "points": [["-40'","60'"],["60'","60'"]], "elevations": ["-2'","6'"]},
            {"type": "subfloor", "at": ["-2'","-2'"], "below": "8\""}]"#,
    )
    .unwrap();
    s.apply(&[Command::AddElevationShapes { floor: None, shapes }]).unwrap();
    // Along the break line the ground climbs from -2' to 6'.
    assert!((height(&s, -40.0, 60.0) - (-2.0 * 304.8)).abs() < 150.0, "{}", height(&s, -40.0, 60.0));
    assert!((height(&s, 60.0, 60.0) - 6.0 * 304.8).abs() < 150.0);
    assert!((height(&s, 10.0, 60.0) - 2.0 * 304.8).abs() < 150.0);
    // The marker holds the ground 8" under the subfloor next to the house.
    assert!((height(&s, -2.0, -2.0) + 8.0 * 25.4).abs() < 60.0, "{}", height(&s, -2.0, -2.0));
}

#[test]
fn backsplashes_stop_under_windows_and_shutters_take_a_layer() {
    let mut s = boxed();
    let w = walls(&s)[0].id; // the west wall, x = 0
    let out = apply(
        &mut s,
        &format!(
            r#"[{{"op": "add_window", "wall": {w}, "position": "10'", "width": "3'", "sill": "3'6\""}},
                {{"op": "add_backsplash", "points": [["3\"","2'"],["3\"","20'"]], "bottom": "36\"", "height": "18\""}}]"#,
            w = w.0
        ),
    );
    let (win, bs) = (out.created[0], out.created[1]);
    let doc = s.document();
    let mesh = housecraft_api::geom::mesh::build(doc, None);
    let ft = 304.8;
    // Over the window's width the backsplash only reaches the sill.
    let top_near = |y0: f64, y1: f64| {
        mesh.faces
            .iter()
            .filter(|f| mesh.parts[f.part].object == bs)
            .filter(|f| {
                let c = f.points.iter().copied().sum::<housecraft_api::geom::DVec3>() / f.points.len() as f64;
                c.y > y0 * ft && c.y < y1 * ft
            })
            .flat_map(|f| f.points.iter().map(|p| p.z))
            .fold(f64::MIN, f64::max)
    };
    assert!((top_near(9.0, 11.0) - 42.0 * 25.4).abs() < 1.0, "{}", top_near(9.0, 11.0));
    assert!((top_near(14.0, 19.0) - 54.0 * 25.4).abs() < 1.0);
    // Shutters on their own layer.
    let layer = apply(&mut s, r#"[{"op": "add_layer", "name": "Shutters"}]"#).created[0];
    s.apply(&[Command::Update { id: win, set: patch(serde_json::json!({"details": {"shutters": {"style": "louvered", "color": "#283c32", "layer": layer.0}}})) }]).unwrap();
    assert_eq!(s.document().get::<Opening>(win).unwrap().details.shutters.as_ref().unwrap().layer, Some(layer));
}

#[test]
fn truss_types_raked_walls_shared_headers_deck_blocking_kept_rafter_spacing() {
    let count = |s: &Session, r: FramingRole| s.document().iter::<FramingMember>().filter(|m| m.role == r).count();
    let set_framing = |s: &mut Session, v: serde_json::Value| {
        let id = s.document().settings.active_default_set;
        s.apply(&[Command::Update { id, set: patch(serde_json::json!({ "framing": v })) }]).unwrap();
    };
    // Scissor trusses: the bottom chord rises.
    let mut s = boxed();
    let w = walls(&s);
    for wall in [w[1].id, w[3].id] {
        s.apply(&[Command::Update { id: wall, set: patch(serde_json::json!({"roof": {"gable": true}})) }]).unwrap();
    }
    apply(&mut s, r#"[{"op": "build_roof"}]"#);
    set_framing(&mut s, serde_json::json!({"trusses": true, "truss_type": "scissor"}));
    apply(&mut s, r#"[{"op": "build_framing", "parts": {"walls": false, "floors": false, "ceilings": false}}]"#);
    let z_lo = s.document().iter::<FramingMember>().filter(|m| m.role == FramingRole::Chord).map(|m| m.a.mm()[2].min(m.b.mm()[2])).fold(f64::MAX, f64::min);
    let rising = s.document().iter::<FramingMember>().filter(|m| m.role == FramingRole::Chord && (m.a.mm()[2] - m.b.mm()[2]).abs() > 100.0 && m.a.mm()[2].min(m.b.mm()[2]) < z_lo + 10.0).count();
    assert!(rising > 4, "scissor bottom chords slope: {rising}");
    set_framing(&mut s, serde_json::json!({"truss_type": "attic"}));
    apply(&mut s, r#"[{"op": "build_framing", "parts": {"walls": false, "floors": false, "ceilings": false}}]"#);
    assert!(count(&s, FramingRole::Web) > 8);

    // Rafter spacing survives the roof rebuilding itself.
    let plane = s.document().iter::<RoofPlane>().find(|r| !r.gable).unwrap().id;
    s.apply(&[Command::Update { id: plane, set: patch(serde_json::json!({"rafter_spacing": "12\""})) }]).unwrap();
    s.apply(&[Command::Update { id: w[0].id, set: patch(serde_json::json!({"height": "9'"})) }]).unwrap();
    assert!(s.document().iter::<RoofPlane>().any(|r| r.rafter_spacing.is_some_and(|l| (l.inches() - 12.0).abs() < 0.01)));

    // A raked (knee) wall's studs stop under its sloping top.
    let mut s = boxed();
    let w = walls(&s)[0].id;
    s.apply(&[Command::Update { id: w, set: patch(serde_json::json!({"shape": {"top": [{"along": 0, "height": "4'"}, {"along": "30'", "height": "8'"}]}})) }]).unwrap();
    // Two windows 10" apart share one header.
    apply(&mut s, &format!(r#"[{{"op": "add_window", "wall": {w}, "position": "12'", "width": "2'", "height": "2'", "sill": "1'"}}, {{"op": "add_window", "wall": {w}, "position": "14'10\"", "width": "2'", "height": "2'", "sill": "1'"}}]"#, w = w.0));
    apply(&mut s, &format!(r#"[{{"op": "build_framing", "hosts": [{}], "parts": {{"floors": false, "ceilings": false, "roofs": false}}}}]"#, w.0));
    let doc = s.document();
    let studs: Vec<&FramingMember> = doc.iter::<FramingMember>().filter(|m| m.host == Some(w) && m.role == FramingRole::Stud).collect();
    let short = studs.iter().map(|m| m.length().mm()).fold(f64::MAX, f64::min);
    let tall = studs.iter().map(|m| m.length().mm()).fold(0.0, f64::max);
    assert!(short < 4.0 * 304.8 && tall > 7.0 * 304.8, "{short} {tall}");
    assert_eq!(doc.iter::<FramingMember>().filter(|m| m.host == Some(w) && m.role == FramingRole::Header).count(), 1);

    // Deck blocking.
    let mut s = boxed();
    let out = apply(&mut s, r#"[{"op": "add_deck", "points": [["30'",0],["46'",0],["46'","20'"],["30'","20'"]], "height": "3'"}]"#);
    let deck = out.created[0];
    s.apply(&[Command::Update { id: deck, set: patch(serde_json::json!({"blocking": true})) }]).unwrap();
    let dk = s.document().get::<Deck>(deck).unwrap().clone();
    let parts = housecraft_api::geom::deck::parts(s.document(), &dk).unwrap();
    assert!(parts.members.iter().any(|m| m.role == FramingRole::Blocking));
}

#[test]
fn cabinet_accessories_corners_legs_pilasters_and_clipped_stiles() {
    let mut s = Session::new(Document::new("Kitchen"));
    let out = apply(
        &mut s,
        r#"[{"op": "add_cabinet", "cabinet_kind": "base", "at": ["10'","10'"], "facing": 90, "width": "24\""},
            {"op": "add_cabinet", "cabinet_kind": "base", "at": ["12'","10'"], "facing": 90, "width": "24\""}]"#,
    );
    let (a, b) = (out.created[0], out.created[1]);
    let faces = |s: &Session, id: Id| {
        let mesh = housecraft_api::geom::mesh::build(s.document(), None);
        mesh.faces.iter().filter(|f| mesh.parts[f.part].object == id).count()
    };
    let lowest_near = |s: &Session, id: Id, x: f64| {
        let mesh = housecraft_api::geom::mesh::build(s.document(), None);
        mesh.faces
            .iter()
            .filter(|f| mesh.parts[f.part].object == id)
            .flat_map(|f| f.points.iter())
            .filter(|p| (p.x - x).abs() < 30.0 && p.y > 10.0 * 304.8 + 23.0 * 25.4)
            .map(|p| p.z)
            .fold(f64::MAX, f64::min)
    };
    let plain = faces(&s, a);
    s.apply(&[Command::Update { id: a, set: patch(serde_json::json!({"accessories": ["trash_pullout", "lazy_susan"], "ends": {"left": false, "right": false, "legs": true, "leg_style": "bun", "pilasters": true}})) }]).unwrap();
    assert!(faces(&s, a) > plain + 30, "{} vs {plain}", faces(&s, a));
    // Rounded corners: the outline gains arc points.
    s.apply(&[Command::Update { id: b, set: patch(serde_json::json!({"corner": {"round": true, "size": "3\""}})) }]).unwrap();
    let c = s.document().get::<Cabinet>(b).unwrap().clone();
    assert!(housecraft_api::geom::cabinets::outline(&c).len() > 10);
    // Extended stiles: down to the floor at the open end, not where the
    // cabinets meet.
    let mut s = Session::new(Document::new("Kitchen"));
    let out = apply(
        &mut s,
        r#"[{"op": "add_cabinet", "cabinet_kind": "base", "at": ["10'","10'"], "facing": 90, "width": "24\""},
            {"op": "add_cabinet", "cabinet_kind": "base", "at": ["12'","10'"], "facing": 90, "width": "24\""}]"#,
    );
    let a = out.created[0];
    let build = serde_json::to_value(s.document().get::<Cabinet>(a).unwrap().build).unwrap();
    let mut build = build;
    build["extended_stiles"] = serde_json::json!(true);
    s.apply(&[Command::Update { id: a, set: patch(serde_json::json!({"build": build, "construction": "framed"})) }]).unwrap();
    let open_end = lowest_near(&s, a, 9.0 * 304.8);
    let shared = lowest_near(&s, a, 11.0 * 304.8 - 10.0);
    assert!(open_end < 1.0 && shared > 50.0, "{open_end} {shared}");
}

#[test]
fn curbs_medians_cul_de_sacs_and_lot_subdivision() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_site_feature", "points": [["40'",0],["40'","60'"]], "feature": {"type": "road", "style": "road", "width": "30'", "thickness": "6\"", "curbs": true, "median": "6'", "cul_de_sac": "40'"}},
            {"op": "add_site_feature", "points": [["-10'",0],["-10'","30'"]], "feature": {"type": "curb", "height": "6\"", "width": "6\""}},
            {"op": "add_cad", "shape": {"shape": "polyline", "points": [["100'",0],["200'",0],["200'","50'"],["100'","50'"]], "closed": true}}]"#,
    );
    let (road, curb, lot) = (out.created[0], out.created[1], out.created[2]);
    let mesh = housecraft_api::geom::mesh::build(s.document(), None);
    let top = |id: Id| mesh.faces.iter().filter(|f| mesh.parts[f.part].object == id).flat_map(|f| f.points.iter().map(|p| p.z)).fold(f64::MIN, f64::max);
    let reach = |id: Id| mesh.faces.iter().filter(|f| mesh.parts[f.part].object == id).flat_map(|f| f.points.iter().map(|p| p.y)).fold(f64::MIN, f64::max);
    assert!(top(road) > 150.0, "median stands up: {}", top(road));
    assert!(reach(road) > 95.0 * 304.8, "the turnaround reaches past the road's end: {}", reach(road));
    assert!((top(curb) - 6.0 * 25.4).abs() < 30.0, "6\" over the ground: {}", top(curb));
    let out = s.apply(&[Command::SubdivideLot { lot, count: 4, angle: None }]).unwrap();
    let doc = s.document();
    let lines = out.created.iter().filter(|id| doc.get::<Cad>(**id).is_some()).count();
    let labels: Vec<&Text> = out.created.iter().filter_map(|id| doc.get::<Text>(*id)).collect();
    assert_eq!(lines, 3);
    assert_eq!(labels.len(), 4);
    assert!(labels.iter().all(|t| t.text.contains("1250 sq ft")), "{:?}", labels.iter().map(|t| &t.text).collect::<Vec<_>>());
}

#[test]
fn any_selection_moves_by_typing_on_a_dimension() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_electrical", "electrical_kind": "recessed_light", "at": ["5'","10'"]},
            {"op": "add_electrical", "electrical_kind": "recessed_light", "at": ["5'","14'"]},
            {"op": "add_dimension", "geometry": {"type": "linear", "points": [[0,"10'"],["5'","10'"]], "orientation": "horizontal", "offset": "1'"}}]"#,
    );
    let (light, other, dim) = (out.created[0], out.created[1], out.created[2]);
    let cmd: Command = serde_json::from_value(serde_json::json!({
        "op": "move_by_dimension", "dimension": dim.0, "segment": 0, "moving": 1, "ids": [light.0, other.0], "length": "21\""
    }))
    .unwrap();
    s.apply(&[cmd]).unwrap();
    let doc = s.document();
    let x = |id: Id| doc.get::<Electrical>(id).unwrap().position.0.mm() / 25.4;
    assert!((x(light) - 21.0).abs() < 0.01, "the light is 21\" from the wall line: {}", x(light));
    assert!((x(other) - 21.0).abs() < 0.01, "the other selected light goes with it");
    let DimensionGeometry::Linear { points, .. } = &doc.get::<Dimension>(dim).unwrap().geometry else { panic!() };
    assert!((points[1].0.mm() / 25.4 - 21.0).abs() < 0.01, "the dimension still measures to the light");
    // The fixed end is the other one; a point that isn't an end of the segment is refused.
    let bad: Command = serde_json::from_value(serde_json::json!({
        "op": "move_by_dimension", "dimension": dim.0, "segment": 0, "moving": 2, "ids": [light.0], "length": "2'"
    }))
    .unwrap();
    assert!(s.apply(&[bad]).is_err());
}

#[test]
fn deck_stairs_go_down_to_grade_through_the_railing() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_deck", "points": [["30'",0],["46'",0],["46'","20'"],["30'","20'"]], "height": "3'"},
            {"op": "add_stair", "start": ["38'","20'"], "direction": 270, "width": "5'"}]"#,
    );
    let (deck, stair) = (out.created[0], out.created[1]);
    let rails_before = {
        let dk = s.document().get::<Deck>(deck).unwrap();
        housecraft_api::geom::deck::parts(s.document(), dk).unwrap().rails.len()
    };
    let mut extras = s.document().get::<Stair>(stair).unwrap().extras.clone();
    extras.to_grade = true;
    s.apply(&[Command::Update { id: stair, set: patch(serde_json::json!({"extras": extras})) }]).unwrap();
    s.apply(&[Command::Update { id: deck, set: patch(serde_json::json!({"footing_depth": "30\""})) }]).unwrap();
    let doc = s.document();
    let st = doc.get::<Stair>(stair).unwrap();
    let g = housecraft_api::geom::stairs::geometry(doc, st);
    let ground = housecraft_api::geom::sun::grade(doc).unwrap();
    assert!((g.base_z - ground).abs() < 1.0, "the bottom is on the ground: {} vs {ground}", g.base_z);
    let top = g.base_z + g.risers as f64 * g.riser_height;
    assert!((top - 3.0 * 304.8).abs() < 1.0, "the top is the deck: {top}");
    assert!(g.riser_height <= st.max_riser.mm() + 0.1);
    let y = 20.0 * 304.8;
    assert!(g.steps.iter().all(|step| step.poly.iter().all(|p| p.y >= y - 1.0)), "it runs out away from the deck");

    let dk = doc.get::<Deck>(deck).unwrap();
    let parts = housecraft_api::geom::deck::parts(doc, dk).unwrap();
    assert_eq!(parts.rails.len(), rails_before + 1, "the back railing opens in two");
    let gap = |x: f64| parts.rails.iter().any(|(a, b)| (a.y - y).abs() < 1.0 && (b.y - y).abs() < 1.0 && a.x.min(b.x) < x && a.x.max(b.x) > x);
    assert!(!gap(38.0 * 304.8), "no railing across the stair");
    assert!(gap(32.0 * 304.8) && gap(44.0 * 304.8), "railing either side");
    let footing = parts.members.iter().find(|m| m.role == FramingRole::Footing).unwrap();
    assert!((footing.a.z - (ground - 30.0 * 25.4)).abs() < 1.0, "footings 30\" deep: {}", footing.a.z);
}

#[test]
fn point_to_point_move_reflect_about_and_make_parallel() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_electrical", "electrical_kind": "recessed_light", "at": ["5'","10'"]},
            {"op": "add_cad", "shape": {"shape": "line", "a": ["20'","0'"], "b": ["20'","10'"]}},
            {"op": "add_cad", "shape": {"shape": "line", "a": ["0'","30'"], "b": ["10'","31'"]}}]"#,
    );
    let (light, mirror, slanted) = (out.created[0], out.created[1], out.created[2]);
    let at = |s: &Session, id: Id| {
        let p = s.document().get::<Electrical>(id).unwrap().position;
        (p.0.mm() / 304.8, p.1.mm() / 304.8)
    };
    let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;

    // Point to Point Move: a point 1' right of the light lands on (7', 12').
    let mv: Command = serde_json::from_value(serde_json::json!({
        "op": "point_to_point_move", "ids": [light.0], "from": ["6'", "10'"], "to": ["7'", "12'"]
    }))
    .unwrap();
    s.apply(&[mv]).unwrap();
    assert!(close(at(&s, light), (6.0, 12.0)), "{:?}", at(&s, light));

    // Reflect About Object across the line x = 20', keeping the original.
    let out = s.apply(&[Command::ReflectAbout { ids: vec![light], about: mirror, copy: true }]).unwrap();
    assert!(close(at(&s, light), (6.0, 12.0)), "the original stays");
    let copy = *out.created.first().expect("a mirrored copy");
    assert!(close(at(&s, copy), (34.0, 12.0)), "{:?}", at(&s, copy));
    assert!(s.apply(&[Command::ReflectAbout { ids: vec![light], about: light, copy: false }]).is_err());

    // Make Parallel: the slanted line turns about its middle to run level.
    let ft = |x: f64, y: f64| housecraft_api::units::Point2(Length::from_mm(x * 304.8), Length::from_mm(y * 304.8));
    s.apply(&[Command::MakeParallel { ids: vec![slanted], edge: [ft(0.0, 30.0), ft(10.0, 31.0)], reference: [ft(0.0, 0.0), ft(-5.0, 0.0)] }])
        .unwrap();
    let Shape::Line { a, b, .. } = s.document().get::<Cad>(slanted).unwrap().shape else { panic!() };
    assert!((a.1.mm() - b.1.mm()).abs() < 1e-6, "level now");
    assert!(((a.1.mm() + b.1.mm()) / 2.0 / 304.8 - 30.5).abs() < 1e-6, "turned about its middle");
    // Nearly square to the reference: it turns square instead.
    s.apply(&[Command::MakeParallel { ids: vec![slanted], edge: [a, b], reference: [ft(0.0, 0.0), ft(1.0, 9.0)] }]).unwrap();
    let Shape::Line { a: a2, b: b2, .. } = s.document().get::<Cad>(slanted).unwrap().shape else { panic!() };
    let (d, r) = (housecraft_api::geom::DVec2::new(b2.0.mm() - a2.0.mm(), b2.1.mm() - a2.1.mm()), housecraft_api::geom::DVec2::new(1.0, 9.0));
    assert!(d.normalize().dot(r.normalize()).abs() < 1e-9, "square to it");
}

#[test]
fn flat_garden_beds_lie_on_the_ground_and_lots_convert_to_terrain() {
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_cad", "shape": {"shape": "polyline", "points": [["-20'","-20'"],["60'","-20'"],["60'","60'"],["-20'","60'"]], "closed": true}},
            {"op": "add_elevation_data", "shape": {"type": "line", "points": [["-30'","-15'"],["70'","-15'"]], "elevation": "0\""}},
            {"op": "add_elevation_data", "shape": {"type": "line", "points": [["-30'","55'"],["70'","55'"]], "elevation": "6'"}}]"#,
    );
    s.apply(&[Command::ConvertCad { ids: vec![out.created[0]], to: housecraft_api::CadTarget::TerrainPerimeter }]).unwrap();
    assert_eq!(s.document().iter::<Terrain>().count(), 1);
    assert!(s.document().get::<Cad>(out.created[0]).is_none(), "the outline became the terrain");
    let bed = apply(
        &mut s,
        r#"[{"op": "add_site_feature", "points": [["40'","10'"],["50'","10'"],["50'","40'"],["40'","40'"]], "feature": {"type": "garden_bed", "height": "0\""}}]"#,
    )
    .created[0];
    let mesh = housecraft_api::geom::mesh::build(s.document(), None);
    let zs: Vec<f64> = mesh.faces.iter().filter(|f| mesh.parts[f.part].object == bed).flat_map(|f| f.points.iter().map(|p| p.z)).collect();
    assert!(!zs.is_empty(), "a flat bed draws");
    let (lo, hi) = zs.iter().fold((f64::MAX, f64::MIN), |(a, b), z| (a.min(*z), b.max(*z)));
    assert!(hi - lo > 2.0 * 304.8, "it follows the slope rather than sitting flat: {lo}..{hi}");
}

#[test]
fn end_panels_girders_post_and_beam_flat_headers_and_deck_beams() {
    let count = |s: &Session, r: FramingRole| s.document().iter::<FramingMember>().filter(|m| m.role == r).count();
    let set_framing = |s: &mut Session, v: serde_json::Value| {
        let id = s.document().settings.active_default_set;
        s.apply(&[Command::Update { id, set: patch(serde_json::json!({ "framing": v })) }]).unwrap();
    };
    // Two cabinets side by side, the first against the west wall.
    let mut s = boxed();
    let out = apply(
        &mut s,
        r#"[{"op": "add_cabinet", "cabinet_kind": "base", "at": ["1'","10'"], "facing": 90, "width": "24\""},
            {"op": "add_cabinet", "cabinet_kind": "base", "at": ["3'","10'"], "facing": 90, "width": "24\""}]"#,
    );
    s.apply(&[Command::AutoEndPanels { floor: None }]).unwrap();
    let ends: Vec<(bool, bool)> = out.created.iter().map(|id| { let c = s.document().get::<Cabinet>(*id).unwrap(); (c.ends.left, c.ends.right) }).collect();
    // The first sits between the wall and the second: only the second's far end is open.
    assert_eq!(ends.iter().map(|(l, r)| *l as usize + *r as usize).sum::<usize>(), 1, "{ends:?}");
    assert_eq!(ends[0], (false, false));

    // Girder trusses double the end trusses.
    let mut s = boxed();
    let w = walls(&s);
    for wall in [w[1].id, w[3].id] {
        s.apply(&[Command::Update { id: wall, set: patch(serde_json::json!({"roof": {"gable": true}})) }]).unwrap();
    }
    apply(&mut s, r#"[{"op": "build_roof"}]"#);
    set_framing(&mut s, serde_json::json!({"trusses": true}));
    apply(&mut s, r#"[{"op": "build_framing", "parts": {"walls": false, "floors": false, "ceilings": false}}]"#);
    let chords = count(&s, FramingRole::Chord);
    set_framing(&mut s, serde_json::json!({"girder_ends": true}));
    apply(&mut s, r#"[{"op": "build_framing", "parts": {"walls": false, "floors": false, "ceilings": false}}]"#);
    assert!(count(&s, FramingRole::Chord) > chords + 2);

    // Post and beam under a wide room.
    let mut s = Session::new(Document::new("Basement"));
    apply(&mut s, r#"[{"op": "add_walls", "points": [[0,0], [0,"28'"], ["40'","28'"], ["40'",0]], "closed": true}]"#);
    let room = s.document().iter::<Room>().next().unwrap().id;
    s.apply(&[Command::AutoPostAndBeam { room, max_span: None, post_spacing: None }]).unwrap();
    assert_eq!(count(&s, FramingRole::Beam), 2, "28' wide: two beam lines");
    assert!(count(&s, FramingRole::Post) >= 8);

    // A flat header is two plies deep.
    let mut s = boxed();
    let w = walls(&s)[0].id;
    let win = apply(&mut s, &format!(r#"[{{"op": "add_window", "wall": {}, "position": "10'"}}]"#, w.0)).created[0];
    s.apply(&[Command::Update { id: win, set: patch(serde_json::json!({"details": {"header": {"flat": true}}})) }]).unwrap();
    apply(&mut s, &format!(r#"[{{"op": "build_framing", "hosts": [{}], "parts": {{"floors": false, "ceilings": false, "roofs": false}}}}]"#, w.0));
    let h = s.document().iter::<FramingMember>().find(|m| m.role == FramingRole::Header).unwrap();
    assert!((h.depth.inches() - 3.0).abs() < 0.01, "{}", h.depth.inches());

    // Deck beams placed by hand, and a deeper ledger.
    let mut s = boxed();
    let deck = apply(&mut s, r#"[{"op": "add_deck", "points": [[0,"30'"],["16'","30'"],["16'","42'"],[0,"42'"]], "height": "3'"}]"#).created[0];
    s.apply(&[Command::Update { id: deck, set: patch(serde_json::json!({"beam_offsets": ["5'", "11'"], "ledger_depth": "9 1/4\""})) }]).unwrap();
    let dk = s.document().get::<Deck>(deck).unwrap().clone();
    let parts = housecraft_api::geom::deck::parts(s.document(), &dk).unwrap();
    let beams: Vec<f64> = parts.members.iter().filter(|m| m.role == FramingRole::Beam).map(|m| m.a.y).collect();
    assert!(beams.iter().any(|y| (y - 35.0 * 304.8).abs() < 10.0) && beams.iter().any(|y| (y - 41.0 * 304.8).abs() < 10.0), "{beams:?}");
    assert!(parts.members.iter().any(|m| m.role == FramingRole::Ledger && (m.depth - 9.25 * 25.4).abs() < 0.5));
}

#[test]
fn stair_sections_split_landings_widths_auto_connect_wrap_and_sip_roofs() {
    use housecraft_api::geom::stairs::geometry;
    let ft = 304.8;
    let mut s = Session::new(Document::new("Stairs"));
    // A sectioned stair: wide first flight, split quarter-turn landing, narrower top flight.
    let id = apply(&mut s, r#"[{"op": "add_stair", "start": [0,0], "direction": 90, "width": "36\""}]"#).created[0];
    s.apply(&[Command::Update { id, set: patch(serde_json::json!({
        "shape": {"type": "sections"},
        "sections": [
            {"section": "flight", "treads": 5, "width": "48\""},
            {"section": "landing", "turn": "left", "split": true},
            {"section": "flight", "treads": 6}
        ]
    })) }]).unwrap();
    let st = s.document().get::<Stair>(id).unwrap().clone();
    let g = geometry(s.document(), &st);
    assert_eq!(g.risers, 5 + 2 + 6 + 1);
    let landings: Vec<f64> = g.steps.iter().filter(|x| x.landing).map(|x| x.z).collect();
    assert_eq!(landings.len(), 2);
    assert!((landings[1] - landings[0] - g.riser_height).abs() < 1e-6, "split a riser apart");
    let width = |st: &housecraft_api::geom::stairs::Step| {
        let p = &st.poly;
        (0..p.len()).map(|i| p[i].distance(p[(i + 1) % p.len()])).fold(0.0, f64::max)
    };
    assert!((width(&g.steps[0]) - 48.0 * 25.4).abs() < 1.0);
    assert!((width(g.steps.last().unwrap()) - 36.0 * 25.4).abs() < 1.0);

    // Breaking an L stair keeps its risers.
    let l = apply(&mut s, r#"[{"op": "add_stair", "start": ["20'",0], "direction": 90, "shape": {"type": "l", "turn": "right", "first_treads": 0}}]"#).created[0];
    let before = geometry(s.document(), s.document().get::<Stair>(l).unwrap()).risers;
    s.apply(&[Command::BreakStair { id: l }]).unwrap();
    let st = s.document().get::<Stair>(l).unwrap().clone();
    assert_eq!(st.shape, StairShape::Sections);
    assert_eq!(geometry(s.document(), &st).risers, before);

    // A landing on its own, 4' up, and a stair starting on it climbs from there.
    let landing = apply(&mut s, r#"[{"op": "add_stair", "start": ["40'",0], "direction": 90, "rise": "4'"}]"#).created[0];
    s.apply(&[Command::Update { id: landing, set: patch(serde_json::json!({"shape": {"type": "sections"}, "sections": [{"section": "landing", "depth": "6'"}]})) }]).unwrap();
    let g = geometry(s.document(), s.document().get::<Stair>(landing).unwrap());
    assert!((g.steps[0].z - 4.0 * ft).abs() < 1.0, "{}", g.steps[0].z);
    let upper = apply(&mut s, r#"[{"op": "add_stair", "start": ["40'","3'"], "direction": 90}]"#).created[0];
    let g = geometry(s.document(), s.document().get::<Stair>(upper).unwrap());
    assert!((g.base_z - 4.0 * ft).abs() < 1.0, "auto-connected: {}", g.base_z);
    assert!(g.steps.iter().all(|x| x.z > 4.0 * ft));

    // Wrap-around: the bottom steps grow round the sides.
    let w = apply(&mut s, r#"[{"op": "add_stair", "start": ["60'",0], "direction": 90}]"#).created[0];
    let plain = geometry(s.document(), s.document().get::<Stair>(w).unwrap()).steps.len();
    s.apply(&[Command::Update { id: w, set: patch(serde_json::json!({"extras": {"wrap": 2}})) }]).unwrap();
    let g = geometry(s.document(), s.document().get::<Stair>(w).unwrap());
    assert_eq!(g.steps.len(), plain + 2);
    assert!(width(&g.steps[0]) > width(&g.steps[2]) + 2.0 * 10.0 * 25.4 - 1.0);

    // A SIP roof: planes take the assembly's thickness, layers in the materials list.
    let mut s = boxed();
    let set = s.document().settings.active_default_set;
    let inch = |x: f64| Length::from_inches(x);
    s.apply(&[Command::Update { id: set, set: patch(serde_json::json!({"roof": {"layers": [
        {"name": "OSB", "thickness": inch(0.4375), "material": "OSB"},
        {"name": "EPS foam core", "thickness": inch(7.375), "material": "EPS foam core"},
        {"name": "OSB", "thickness": inch(0.4375), "material": "OSB"}
    ]}})) }]).unwrap();
    apply(&mut s, r#"[{"op": "build_roof"}]"#);
    let plane = s.document().iter::<RoofPlane>().find(|r| !r.gable).unwrap();
    assert!((plane.thickness.inches() - 8.25).abs() < 0.01);
    let list = housecraft_api::materials::materials(s.document(), &Default::default());
    assert!(list.lines.iter().any(|l| l.item.starts_with("EPS foam core")), "{:?}", list.lines.iter().map(|l| &l.item).collect::<Vec<_>>());
}

#[test]
fn pins_toggle_and_survive_save() {
    let mut s = boxed();
    let w = walls(&s)[0].id;
    s.apply(&[Command::PinObjects { ids: vec![w], pinned: true }]).unwrap();
    s.apply(&[Command::PinObjects { ids: vec![w], pinned: true }]).unwrap();
    assert_eq!(s.document().settings.pins, vec![w], "pinning twice keeps one pin");
    let json = serde_json::to_string(s.document()).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back.settings.pins, vec![w]);
    s.apply(&[Command::PinObjects { ids: vec![w], pinned: false }]).unwrap();
    assert!(s.document().settings.pins.is_empty());
    assert!(s.apply(&[Command::PinObjects { ids: vec![Id(999_999)], pinned: true }]).is_err());
}

#[test]
fn time_log_merges_totals_and_exports() {
    let mut s = boxed();
    let log = |task: &str, minutes: f64, date: &str| Command::LogTime { task: task.into(), minutes, note: String::new(), date: Some(date.into()) };
    s.apply(&[log("Design", 30.0, "2026-10-19"), log("Design", 15.0, "2026-10-19"), log("Drafting, CDs", 90.0, "2026-10-19")]).unwrap();
    assert_eq!(s.document().settings.time_log.len(), 2, "same task, same day adds up");
    assert_eq!(s.document().settings.time_log[0].minutes, 45.0);
    let totals = query::time_by_task(s.document());
    assert_eq!(totals[0], ("Drafting, CDs".to_string(), 1.5));
    let csv = query::time_log_csv(s.document());
    assert!(csv.lines().nth(2).unwrap().contains("\"Drafting, CDs\",1.50"), "{csv}");
    assert!(s.apply(&[log("x", 0.0, "2026-10-19")]).is_err());
    s.apply(&[Command::DeleteTime { index: Some(0) }]).unwrap();
    assert_eq!(s.document().settings.time_log.len(), 1);
    s.apply(&[Command::DeleteTime { index: None }]).unwrap();
    assert!(s.document().settings.time_log.is_empty());
}

#[test]
fn foundations_leave_out_porches() {
    let mut s = Session::new(Document::new("Porch"));
    apply(&mut s, r#"[{"op": "add_walls", "points": [[0,0],[9000,0],[9000,6000],[0,6000]], "closed": true},
        {"op": "build_foundation", "kind": "basement"}]"#);
    let foundation = s.document().floors_by_elevation()[0].id;
    let count = |s: &Session| s.document().walls_on(foundation).count();
    assert_eq!(count(&s), 4);
    // Half walls in front enclose a porch.
    let floor1 = s.document().floors_by_elevation()[1].id;
    apply(&mut s, &format!(r#"[{{"op": "add_walls", "floor": {}, "height": "36\"", "points": [[2000,0],[2000,-2400],[7000,-2400],[7000,0]]}}]"#, floor1.0));
    let rooms: Vec<_> = s.document().rooms_on(floor1).map(|r| (r.anchor.x().mm(), r.anchor.y().mm())).collect();
    let porch_room = s.document().rooms_on(floor1).find(|r| r.anchor.y().mm() < 0.0).unwrap_or_else(|| panic!("{rooms:?} {:?}", housecraft_api::geom::rooms::detect(s.document(), floor1).rooms.len())).id;
    let porch_type = s.document().find_named::<RoomType>("Porch").unwrap().id;
    s.apply(&[Command::Update { id: porch_room, set: patch(serde_json::json!({"room_type": porch_type})) }]).unwrap();
    let ys: Vec<f64> = s.document().walls_on(foundation).flat_map(|w| [w.start.y().mm(), w.end.y().mm()]).collect();
    assert!(ys.iter().all(|y| *y > -100.0), "the basement stays under the house: {ys:?}");
    assert_eq!(count(&s), 4);
}
