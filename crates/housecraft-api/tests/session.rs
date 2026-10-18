use housecraft_api::model::{Document, Id, ObjectKind};
use housecraft_api::{ApiError, Command, Session, query};

fn commands(json: &str) -> Vec<Command> {
    serde_json::from_str(json).expect("valid commands")
}

#[test]
fn draws_a_closed_box_from_json() {
    let mut s = Session::new(Document::new("Box"));
    let out = s
        .apply(&commands(
            r#"[{"op": "add_walls", "points": [[0,0], ["20'",0], ["20'","30'"], [0,"30'"]], "closed": true}]"#,
        ))
        .unwrap();
    assert_eq!(out.created.len(), 4);
    let sum = query::summary(s.document());
    assert_eq!(sum.wall_count, 4);
    assert_eq!(sum.floors[0].total_wall_length, "100'-0\"");
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
fn undo_and_redo() {
    let mut s = Session::new(Document::new("Undo"));
    s.apply(&commands(
        r#"[{"op": "add_wall", "start": [0,0], "end": [3000,0]}]"#,
    ))
    .unwrap();
    assert_eq!(s.document().walls.len(), 1);
    assert!(s.undo());
    assert_eq!(s.document().walls.len(), 0);
    assert!(s.redo());
    assert_eq!(s.document().walls.len(), 1);
    assert!(!s.redo());
}

#[test]
fn deleting_a_floor_deletes_its_walls() {
    let mut s = Session::new(Document::new("Floors"));
    let out = s
        .apply(&commands(
            r#"[{"op": "add_floor"},
                {"op": "add_wall", "start": [0,0], "end": [3000,0]}]"#,
        ))
        .unwrap();
    let new_floor = out.created[0];
    let floors = s.document().floors_by_elevation();
    assert_eq!(floors[1].id, new_floor);
    assert!((floors[1].elevation.feet() - 8.0).abs() < 1e-9);

    let floor1 = floors[0].id;
    let out = s.apply(&[Command::Delete { id: floor1 }]).unwrap();
    assert_eq!(out.deleted.len(), 2);
    assert!(s.document().walls.is_empty());
    assert!(s.document().validate().is_empty());
}

#[test]
fn wall_type_in_use_cannot_be_deleted() {
    let mut s = Session::new(Document::new("Types"));
    s.apply(&commands(
        r#"[{"op": "add_wall", "start": [0,0], "end": [3000,0]}]"#,
    ))
    .unwrap();
    let ty = s.document().settings.default_wall_type.unwrap();
    assert!(s.apply(&[Command::Delete { id: ty }]).is_err());
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
        }])
        .unwrap_err();
    let ApiError::InCommand { error, .. } = err else {
        panic!()
    };
    assert_eq!(
        *error,
        ApiError::WrongKind {
            id: floor,
            expected: ObjectKind::Wall,
            actual: ObjectKind::Floor
        }
    );
    assert!(query::get_object(s.document(), Id(9999)).is_none());
}

#[test]
fn schema_lists_every_op() {
    let schema = housecraft_api::command_schema().to_string();
    for op in [
        "add_wall",
        "add_walls",
        "add_floor",
        "delete",
        "update_wall",
    ] {
        assert!(schema.contains(op), "schema missing {op}");
    }
}
