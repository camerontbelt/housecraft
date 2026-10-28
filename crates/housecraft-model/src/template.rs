//! The contents of a new document.

use std::collections::BTreeMap;

use housecraft_units::{Length, Point3};

use crate::*;

const PLACEHOLDER: Id = Id(0);

/// The starter material catalogue, and the default material for each surface.
pub(crate) fn standard_materials(doc: &mut Document) -> SurfaceMaterials {
    use MaterialClass as C;
    let mut add = |name: &str, category: &str, class: C, (r, g, b): (u8, u8, u8)| {
        doc.insert(Material::new(name, category, class, Color::rgb(r, g, b)))
    };
    let paint = add("Eggshell White Paint", "Paint", C::Generic, (237, 232, 222));
    let ceiling = add("Flat Ceiling White", "Paint", C::Generic, (245, 245, 242));
    let oak = add("Oak Flooring", "Wood", C::Plastic, (184, 153, 115));
    add("Walnut Flooring", "Wood", C::Plastic, (110, 78, 54));
    add("Porcelain Tile", "Tile", C::Plastic, (214, 210, 202));
    add("Gray Carpet", "Carpet", C::Generic, (140, 138, 134));
    let shingles = add("Charcoal Asphalt Shingles", "Roofing", C::Generic, (92, 87, 84));
    add("Galvalume Metal Roofing", "Roofing", C::Metal, (170, 172, 172));
    let trim = add("White Trim Paint", "Paint", C::Plastic, (247, 247, 247));
    let door = add("Stained Wood Door", "Wood", C::Plastic, (140, 97, 61));
    let glass = add("Clear Glass", "Glass", C::Glass, (140, 191, 224));
    add("Frosted Glass", "Glass", C::Translucent, (225, 232, 236));
    add("Mirror", "Glass", C::Mirror, (235, 238, 240));
    let concrete = add("Concrete", "Masonry", C::Generic, (173, 173, 168));
    add("Red Brick", "Masonry", C::Generic, (150, 74, 54));
    add("Fiber Cement Siding", "Siding", C::Generic, (191, 196, 189));
    let plywood = add("Plywood Subfloor", "Wood", C::Generic, (209, 189, 153));
    let tread = add("Red Oak Treads", "Wood", C::Plastic, (173, 133, 92));
    let stringer = add("Painted Stringer", "Paint", C::Plastic, (148, 112, 77));
    let rail = add("White Rail Paint", "Paint", C::Plastic, (242, 242, 237));
    let device = add("White Device Plastic", "Plastic", C::Plastic, (245, 242, 235));
    let lamp = add("Warm Lamp Lens", "Lighting", C::Emissive, (255, 237, 158));
    let maple = add("Natural Maple Cabinet", "Cabinets", C::Plastic, (214, 182, 138));
    add("White Painted Cabinet", "Cabinets", C::Plastic, (240, 238, 232));
    add("Espresso Cabinet", "Cabinets", C::Plastic, (66, 46, 36));
    let granite = add("Granite Countertop", "Stone", C::Plastic, (120, 112, 104));
    add("White Quartz Countertop", "Stone", C::Plastic, (235, 233, 228));
    add("Butcher Block", "Wood", C::Plastic, (196, 151, 102));
    let subway = add("White Subway Tile", "Tile", C::Plastic, (240, 240, 236));
    let nickel = add("Brushed Nickel", "Metal", C::Metal, (196, 194, 188));
    add("Stainless Steel", "Metal", C::Metal, (206, 208, 210));
    add("Matte Black Metal", "Metal", C::Metal, (38, 38, 40));
    let furniture = add("Cherry Furniture", "Wood", C::Plastic, (122, 72, 46));
    let fabric = add("Chenille - Pebble", "Fabric", C::Generic, (178, 170, 158));
    use Surface as S;
    SurfaceMaterials::from([
        (S::Wall, paint),
        (S::Floor, oak),
        (S::Ceiling, ceiling),
        (S::Roof, shingles),
        (S::Soffit, trim),
        (S::Door, door),
        (S::Glass, glass),
        (S::Frame, trim),
        (S::Concrete, concrete),
        (S::Platform, plywood),
        (S::Stair, tread),
        (S::Stringer, stringer),
        (S::Railing, rail),
        (S::Device, device),
        (S::Lamp, lamp),
        (S::CabinetBox, maple),
        (S::CabinetDoor, maple),
        (S::Countertop, granite),
        (S::Backsplash, subway),
        (S::Hardware, nickel),
        (S::Trim, trim),
        (S::Furniture, furniture),
        (S::Upholstery, fabric),
    ])
}

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
        joist_spacing: None,
        rim_material: None,
                framing_origin: None,
                bearing_lines: Vec::new(),
        platform: Vec::new(),
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
            fill: None,
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
    let electrical = layer(
        &mut doc,
        "Electrical",
        0.18,
        Some(Color::rgb(0x8a, 0x2b, 0x8f)),
    );

    let cabinets = layer(
        &mut doc,
        "Cabinets",
        0.25,
        Some(Color::rgb(0x6b, 0x4f, 0x2a)),
    );

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
        main: false,
        r_value: 0.0,
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
        cladding: Some(Cladding::Lap { exposure: inch(6.0) }),
        corner_trim: Some(CornerTrim::Board { width: inch(4.0) }),
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
        cladding: None,
        corner_trim: None,
    });
    doc.insert(WallType {
        id: PLACEHOLDER,
        name: "Railing".into(),
        layers: vec![layer_of("Rail", 3.5, "Painted wood")],
        railing: Some(Railing::default()),
        cladding: None,
        corner_trim: None,
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
            moldings: if living { vec![RoomMolding::base()] } else { Vec::new() },
            exterior: matches!(name, "Porch" | "Deck"),
        });
        if name == "Unspecified" {
            unspecified = id;
        }
    }

    let materials = standard_materials(&mut doc);
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
            electrical: Some(electrical),
            cabinets: Some(cabinets),
            framing: None,
        },
        dimensions: DimensionDefaults {
            text_style: dim_text,
            arrow: ArrowStyle::Tick,
            extension_gap: inch(4.0),
            extension_overshoot: inch(4.0),
            exterior_offset: ft(4.0),
            inches_below: inch(24.0),
            locate: Default::default(),
            auto: Default::default(),
        },
        door: DoorDefaults {
            style: DoorStyle::Hinged,
            width: inch(32.0),
            height: inch(80.0),
            hinge: Hinge::Left,
            sizes: Vec::new(),
        },
        window: WindowDefaults {
            style: WindowStyle::DoubleHung,
            width: inch(36.0),
            height: inch(48.0),
            head_height: inch(80.0),
        },
        roof: RoofDefaults::default(),
        stair: StairDefaults::default(),
        exterior_stair: StairDefaults::exterior(),
        cabinets: CabinetDefaults::default(),
        solids: Default::default(),
        framing: Default::default(),
        deck: Default::default(),
        materials,
        labels: Default::default(),
        wall_reference: Default::default(),
        room_labels: Default::default(),
        dimension_style: None,
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
        references: Vec::new(),
        phase: Phase::All,
    });
    doc.insert(Camera {
        id: PLACEHOLDER,
        name: "Overview".into(),
        projection: Projection::Perspective { fov: 45.0 },
        eye: Point3::from_mm(-9000.0, -12000.0, 9000.0),
        target: Point3::from_mm(4000.0, 3000.0, 1200.0),
        layer_set: working,
        backdrop: None,
        render: Default::default(),
    });
    doc
}
