//! The contents of a new document.

use std::collections::BTreeMap;

use housecraft_units::{Length, Point3};

use crate::*;

const PLACEHOLDER: Id = Id(0);

pub(crate) fn new_document(name: String) -> Document {
    let mut doc = Document::empty(name);
    let ft = Length::from_feet;
    let inch = Length::from_inches;

    let floor = doc.insert(Floor {
        id: PLACEHOLDER,
        name: "Floor 1".into(),
        elevation: Length::ZERO,
        ceiling_height: ft(8.0),
        floor_structure: default_floor_structure(),
        foundation: None,
        auto_roof: false,
    });

    let layer = |doc: &mut Document, name: &str, weight: f64, color: Option<Color>| {
        doc.insert(Layer {
            id: PLACEHOLDER,
            name: name.into(),
            color,
            line_weight: weight,
            line_style: LineStyle::Solid,
            text_style: None,
            in_materials_list: true,
        })
    };
    let walls = layer(&mut doc, "Walls", 0.5, None);
    let doors = layer(&mut doc, "Doors", 0.25, None);
    let windows = layer(&mut doc, "Windows", 0.25, None);
    let rooms = layer(&mut doc, "Room Labels", 0.18, None);
    let cad = layer(&mut doc, "CAD", 0.25, None);
    let dimensions = layer(
        &mut doc,
        "Dimensions",
        0.18,
        Some(Color::rgb(0x1f, 0x5f, 0xbf)),
    );
    let text = layer(&mut doc, "Text", 0.18, None);
    let roofs = layer(
        &mut doc,
        "Roof Planes",
        0.18,
        Some(Color::rgb(0x9a, 0x4a, 0x1f)),
    );
    let stairs = layer(&mut doc, "Stairs", 0.25, None);

    let style = |doc: &mut Document, name: &str, height: Length, bold: bool| {
        doc.insert(TextStyle {
            id: PLACEHOLDER,
            name: name.into(),
            height,
            font: "Sans".into(),
            bold,
            italic: false,
            color: None,
        })
    };
    let default_text = style(&mut doc, "Default", inch(6.0), false);
    let dim_text = style(&mut doc, "Dimensions", inch(5.0), false);
    let room_text = style(&mut doc, "Room Labels", inch(8.0), true);
    if let Some(l) = doc.get_mut::<Layer>(dimensions) {
        l.text_style = Some(dim_text);
    }
    if let Some(l) = doc.get_mut::<Layer>(rooms) {
        l.text_style = Some(room_text);
    }

    let layer_of = |name: &str, inches: f64, material: &str| WallLayer {
        name: name.into(),
        thickness: inch(inches),
        material: material.into(),
    };
    let exterior = doc.insert(WallType {
        id: PLACEHOLDER,
        name: "Exterior 2x6".into(),
        layers: vec![
            layer_of("Siding", 0.75, "Lap siding"),
            layer_of("Sheathing", 7.0 / 16.0, "OSB"),
            layer_of("Framing", 5.5, "2x6 stud"),
            layer_of("Interior finish", 0.5, "Gypsum board"),
        ],
        railing: None,
    });
    let interior = doc.insert(WallType {
        id: PLACEHOLDER,
        name: "Interior 2x4".into(),
        layers: vec![
            layer_of("Finish", 0.5, "Gypsum board"),
            layer_of("Framing", 3.5, "2x4 stud"),
            layer_of("Finish", 0.5, "Gypsum board"),
        ],
        railing: None,
    });
    doc.insert(WallType {
        id: PLACEHOLDER,
        name: "Railing".into(),
        layers: vec![layer_of("Rail", 3.5, "Painted wood")],
        railing: Some(Railing::default()),
    });

    let mut unspecified = PLACEHOLDER;
    for (name, living, floor_finish) in [
        ("Unspecified", true, ""),
        ("Living", true, "Hardwood"),
        ("Family", true, "Hardwood"),
        ("Kitchen", true, "Tile"),
        ("Dining", true, "Hardwood"),
        ("Bedroom", true, "Carpet"),
        ("Primary Bedroom", true, "Carpet"),
        ("Bath", true, "Tile"),
        ("Closet", true, "Carpet"),
        ("Hall", true, "Hardwood"),
        ("Entry", true, "Tile"),
        ("Laundry", true, "Vinyl"),
        ("Office", true, "Hardwood"),
        ("Garage", false, "Concrete"),
        ("Porch", false, "Concrete"),
        ("Deck", false, "Decking"),
    ] {
        let id = doc.insert(RoomType {
            id: PLACEHOLDER,
            name: name.into(),
            ceiling_height: None,
            floor_finish: floor_finish.into(),
            wall_finish: if floor_finish.is_empty() || !living {
                String::new()
            } else {
                "Paint".into()
            },
            living_area: living,
        });
        if name == "Unspecified" {
            unspecified = id;
        }
    }

    let defaults = doc.insert(DefaultSet {
        id: PLACEHOLDER,
        name: "Default".into(),
        exterior_wall_type: exterior,
        interior_wall_type: interior,
        room_type: unspecified,
        text_style: default_text,
        layers: KindLayers {
            walls,
            doors,
            windows,
            rooms,
            cad,
            dimensions,
            text,
            roofs: Some(roofs),
            stairs: Some(stairs),
        },
        dimensions: DimensionDefaults {
            text_style: dim_text,
            arrow: ArrowStyle::Tick,
            extension_gap: inch(4.0),
            extension_overshoot: inch(4.0),
            exterior_offset: ft(4.0),
            inches_below: inch(24.0),
        },
        door: DoorDefaults {
            style: DoorStyle::Hinged,
            width: inch(32.0),
            height: inch(80.0),
            hinge: Hinge::Left,
        },
        window: WindowDefaults {
            style: WindowStyle::DoubleHung,
            width: inch(36.0),
            height: inch(48.0),
            head_height: inch(80.0),
        },
        roof: RoofDefaults::default(),
        stair: StairDefaults::default(),
    });
    doc.settings.active_default_set = defaults;

    let working = doc.insert(LayerSet {
        id: PLACEHOLDER,
        name: "Working".into(),
        layers: BTreeMap::new(),
    });
    doc.insert(LayerSet {
        id: PLACEHOLDER,
        name: "Floor Plan".into(),
        layers: BTreeMap::from([
            (
                cad,
                LayerDisplay {
                    visible: false,
                    ..LayerDisplay::default()
                },
            ),
            (
                roofs,
                LayerDisplay {
                    visible: false,
                    ..LayerDisplay::default()
                },
            ),
        ]),
    });

    doc.insert(PlanView {
        id: PLACEHOLDER,
        name: "Floor Plan".into(),
        floor: Some(floor),
        layer_set: working,
        default_set: defaults,
        zoom: None,
        reference_floor: false,
    });
    doc.insert(Camera {
        id: PLACEHOLDER,
        name: "Overview".into(),
        projection: Projection::Perspective { fov: 45.0 },
        eye: Point3::from_mm(-9000.0, -12000.0, 9000.0),
        target: Point3::from_mm(4000.0, 3000.0, 1200.0),
        layer_set: working,
    });
    doc
}
