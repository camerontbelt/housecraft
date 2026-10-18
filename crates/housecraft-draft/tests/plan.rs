use housecraft_api::model::Document;
use housecraft_api::{Command, Session};
use housecraft_draft::pdf::{Paper, PdfOptions, Scale, scale_label, to_pdf};
use housecraft_draft::svg::{SvgOptions, to_svg};
use housecraft_draft::{PlanOptions, Prim, plan};

fn house() -> Session {
    let mut s = Session::new(Document::new("Test House"));
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "add_walls", "points": [[0,0], [0,"30'"], ["40'","30'"], ["40'",0]], "closed": true},
            {"op": "add_wall", "start": ["16'",0], "end": ["16'","30'"], "interior": true},
            {"op": "add_exterior_dimensions"},
            {"op": "add_text", "position": ["2'","-6'"], "text": "Front"},
            {"op": "add_cad", "shape": {"shape": "circle", "center": ["8'","15'"], "radius": "2'"}}]"#,
    )
    .unwrap();
    s.apply(&cmds).unwrap();
    let wall = s
        .document()
        .iter::<housecraft_api::model::Wall>()
        .next()
        .unwrap()
        .id;
    let cmds: Vec<Command> = serde_json::from_str(&format!(
        r#"[{{"op": "add_door", "wall": {w}, "position": "20'"}}, {{"op": "add_window", "wall": {w}, "position": "6'"}}]"#,
        w = wall.0
    ))
    .unwrap();
    s.apply(&cmds).unwrap();
    s
}

#[test]
fn plan_contains_every_kind_of_object() {
    let s = house();
    let doc = s.document();
    let d = plan(doc, &PlanOptions::first(doc).unwrap());
    let texts: Vec<&str> = d
        .items
        .iter()
        .filter_map(|i| match &i.prim {
            Prim::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"Front"));
    assert!(
        texts.iter().filter(|t| **t == "Unspecified").count() == 2,
        "{texts:?}"
    );
    // Exterior dimensions run face to face: 40' + a 2x6 wall on each side.
    assert!(texts.contains(&"40'-7 3/16\""), "{texts:?}");
    assert!(texts.contains(&"30'-7 3/16\""), "{texts:?}");
    for kind in ["wall", "opening", "cad", "dimension", "room"] {
        let any = d.items.iter().any(|i| {
            i.object
                .and_then(|id| doc.kind_of(id))
                .is_some_and(|k| k.name() == kind)
        });
        assert!(any, "nothing drawn for {kind}");
    }
}

#[test]
fn svg_and_pdf_render() {
    let s = house();
    let doc = s.document();
    let d = plan(doc, &PlanOptions::first(doc).unwrap());
    let svg = to_svg(&d, &SvgOptions::default());
    assert!(svg.starts_with("<svg") && svg.contains("Front") && svg.trim_end().ends_with("</svg>"));
    let (pdf, scale) = to_pdf(
        &d,
        &PdfOptions {
            paper: Paper::Letter,
            landscape: true,
            scale: Scale::Fit,
            title: "Test House - Floor 1".into(),
            imperial: true,
        },
    );
    assert!(pdf.starts_with(b"%PDF"));
    assert!(
        scale >= 48.0,
        "a 40' house needs at least 1/4\" scale on letter: {scale}"
    );
}

#[test]
fn hidden_layers_are_not_drawn() {
    let mut s = house();
    let (set, cad_layer) = {
        let doc = s.document();
        (
            PlanOptions::first(doc).unwrap().layer_set,
            doc.active_defaults().layers.cad,
        )
    };
    s.apply(&[Command::SetLayerDisplay {
        layer_set: set,
        layer: cad_layer,
        visible: Some(false),
        color: None,
        line_weight: None,
        line_style: None,
    }])
    .unwrap();
    let doc = s.document();
    let d = plan(doc, &PlanOptions::first(doc).unwrap());
    assert!(!d.items.iter().any(|i| {
        i.object
            .and_then(|id| doc.kind_of(id))
            .is_some_and(|k| k.name() == "cad")
    }));
}

#[test]
fn scale_labels() {
    assert_eq!(scale_label(48.0, true), "1/4\" = 1'-0\"");
    assert_eq!(scale_label(96.0, true), "1/8\" = 1'-0\"");
    assert_eq!(scale_label(12.0, true), "1\" = 1'-0\"");
    assert_eq!(scale_label(100.0, false), "1:100");
}

#[cfg(feature = "png")]
#[test]
fn png_renders() {
    let s = house();
    let doc = s.document();
    let d = plan(doc, &PlanOptions::first(doc).unwrap());
    let png = housecraft_draft::svg::svg_to_png(&to_svg(&d, &SvgOptions::default()), 800).unwrap();
    assert_eq!(&png[1..4], b"PNG");
}

#[test]
fn renovation_phases_show_the_right_walls() {
    use housecraft_api::model::{Phase, Renovation, Wall};
    let mut s = house();
    let walls: Vec<_> = s.document().iter::<Wall>().map(|w| w.id).collect();
    let interior = *walls.last().unwrap();
    // Everything existing, then the interior wall comes out and a new one goes in.
    s.apply(&[
        Command::SetRenovation {
            ids: walls.clone(),
            status: Renovation::Existing,
        },
        Command::SetRenovation {
            ids: vec![interior],
            status: Renovation::Demolish,
        },
    ])
    .unwrap();
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "add_wall", "start": ["28'",0], "end": ["28'","30'"], "interior": true}]"#,
    )
    .unwrap();
    let new = s.apply(&cmds).unwrap().created[0];
    let doc = s.document();
    let draw = |phase: Phase| {
        let mut opts = PlanOptions::first(doc).unwrap();
        opts.phase = phase;
        plan(doc, &opts)
    };
    let has = |d: &housecraft_draft::Drawing, id| d.items.iter().any(|i| i.object == Some(id));
    let filled = |d: &housecraft_draft::Drawing, id| {
        d.items.iter().find_map(|i| match &i.prim {
            Prim::Polygon { fill, .. } if i.object == Some(id) => Some(*fill),
            _ => None,
        })
    };
    let all = draw(Phase::All);
    assert!(has(&all, interior) && has(&all, new));
    // Demolished walls are outlines only (dashed), never filled.
    assert_eq!(filled(&all, interior), None);
    let demo = draw(Phase::Demolition);
    assert!(has(&demo, interior) && !has(&demo, new));
    let proposed = draw(Phase::Proposed);
    assert!(!has(&proposed, interior) && has(&proposed, new));
    // Proposed plans keep existing walls grey and fill new walls solid.
    assert_eq!(
        filled(&proposed, walls[0]),
        Some(housecraft_draft::Fill::Poche)
    );
    assert!(matches!(
        filled(&proposed, new),
        Some(housecraft_draft::Fill::Ink(_))
    ));
}

#[test]
fn north_pointer_is_drawn_where_placed() {
    let mut s = house();
    let cmds: Vec<Command> =
        serde_json::from_str(r#"[{"op": "set_north", "angle": 60, "pointer": ["50'","20'"]}]"#)
            .unwrap();
    s.apply(&cmds).unwrap();
    let doc = s.document();
    let d = plan(doc, &PlanOptions::first(doc).unwrap());
    assert!(
        d.items
            .iter()
            .any(|i| matches!(&i.prim, Prim::Text { text, .. } if text == "N"))
    );
}

#[test]
fn elevation_and_section_drawings() {
    use housecraft_api::model::{SectionKind, SectionView};
    let mut s = house();
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "set_roof_style", "style": "gable"},
            {"op": "add_section_view", "name": "South", "a": ["-10'","-10'"], "b": ["50'","-10'"]},
            {"op": "add_section_view", "name": "Cut", "a": ["-10'","15'"], "b": ["50'","15'"], "section_kind": "section"}]"#,
    )
    .unwrap();
    s.apply(&cmds).unwrap();
    let doc = s.document();
    let view = |name: &str| doc.iter::<SectionView>().find(|v| v.name == name).unwrap();
    let elev = housecraft_draft::elevation::section_view(doc, view("South"));
    let polys = elev
        .items
        .iter()
        .filter(|i| matches!(i.prim, Prim::Polygon { .. }))
        .count();
    assert!(polys > 10, "faces painted: {polys}");
    // Drawn left to right along the line, roof above the walls, ground at grade.
    let (mut xmin, mut xmax, mut ymax) = (f64::MAX, f64::MIN, f64::MIN);
    for i in &elev.items {
        if let Prim::Polygon { points, .. } = &i.prim {
            for p in points {
                xmin = xmin.min(p.x);
                xmax = xmax.max(p.x);
                ymax = ymax.max(p.y);
            }
        }
    }
    let ft = 304.8;
    assert!(xmin > 5.0 * ft && xmax < 55.0 * ft, "{xmin} {xmax}");
    assert!(ymax > 10.0 * ft, "roof rises above the 8' walls: {ymax}");
    // The cross section has heavy cut lines; the elevation has none but the ground.
    let heavy = |d: &housecraft_draft::Drawing| {
        d.items
            .iter()
            .filter(|i| matches!(&i.prim, Prim::Polyline { stroke, .. } if stroke.width >= 0.5))
            .count()
    };
    let cut = housecraft_draft::elevation::section_view(doc, view("Cut"));
    assert_eq!(view("Cut").section_kind, SectionKind::Section);
    assert!(
        heavy(&cut) > heavy(&elev) + 4,
        "cut edges: {} vs {}",
        heavy(&cut),
        heavy(&elev)
    );
    // Callouts appear in plan.
    let plan_d = plan(doc, &PlanOptions::first(doc).unwrap());
    assert!(
        plan_d
            .items
            .iter()
            .any(|i| matches!(&i.prim, Prim::Text { text, .. } if text == "Cut"))
    );
}

#[test]
fn layouts_place_live_views_on_pages() {
    use housecraft_api::model::{Layout, LayoutBox};
    let mut s = house();
    let floor = s.document().floors_by_elevation()[0].id;
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "add_layout", "name": "Permit Set", "pages": 2},
            {"op": "add_section_view", "name": "South", "a": ["-10'","-10'"], "b": ["50'","-10'"]}]"#,
    )
    .unwrap();
    let out = s.apply(&cmds).unwrap();
    let (layout, view) = (out.created[0], out.created[1]);
    let cmds: Vec<Command> = serde_json::from_str(&format!(
        r#"[{{"op": "send_to_layout", "layout": {l}, "source": {{"type": "plan", "floor": {f}}}, "scale": 48}},
            {{"op": "send_to_layout", "layout": {l}, "source": {{"type": "view", "view": {v}}}}},
            {{"op": "send_to_layout", "layout": {l}, "page": 1, "source": {{"type": "schedule", "schedule_kind": "doors"}}}},
            {{"op": "send_to_layout", "layout": {l}, "page": 1, "source": {{"type": "text", "text": "{{project}} notes"}}}}]"#,
        l = layout.0,
        f = floor.0,
        v = view.0
    ))
    .unwrap();
    s.apply(&cmds).unwrap();
    let doc = s.document();
    let l = doc.get::<Layout>(layout).unwrap();
    assert_eq!(l.pages.len(), 2);
    // Boxes fill quarters without overlapping.
    let boxes: Vec<&LayoutBox> = doc.iter::<LayoutBox>().filter(|b| b.page == 0).collect();
    assert_eq!(boxes.len(), 2);
    assert!(boxes[0].x.mm() + boxes[0].width.mm() <= boxes[1].x.mm() + 1e-6);

    let pages = housecraft_draft::layout::layout_pages(doc, l);
    let (w, h) = (l.paper_width.mm(), l.paper_height.mm());
    for p in &pages {
        let (lo, hi) = p.bounds().unwrap();
        assert!(
            lo.x >= -1e-6 && lo.y >= -1e-6 && hi.x <= w + 1e-6 && hi.y <= h + 1e-6,
            "on the sheet"
        );
    }
    let texts = |p: &housecraft_draft::Drawing| -> Vec<String> {
        p.items
            .iter()
            .filter_map(|i| match &i.prim {
                Prim::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    let p1 = texts(&pages[0]);
    assert!(p1.iter().any(|t| t == "Scale: 1/4\" = 1'-0\""), "{p1:?}");
    assert!(p1.iter().any(|t| t == "SOUTH"), "caption: {p1:?}");
    assert!(
        p1.iter().any(|t| t.starts_with("Sheet 1 of 2")),
        "title block: {p1:?}"
    );
    let p2 = texts(&pages[1]);
    assert!(p2.iter().any(|t| t == "Door Schedule"));
    assert!(
        p2.iter().any(|t| t == "Test House notes"),
        "macros expand: {p2:?}"
    );

    let pdf = housecraft_draft::pdf::pages_to_pdf(&pages, w, h);
    assert!(pdf.starts_with(b"%PDF"));
    assert!(
        String::from_utf8_lossy(&pdf).contains("/Count 2"),
        "two pages"
    );

    // Deleting the view removes its box; deleting the layout removes the rest.
    s.apply(&[Command::Delete { ids: vec![view] }]).unwrap();
    assert_eq!(s.document().iter::<LayoutBox>().count(), 3);
    s.apply(&[Command::Delete { ids: vec![layout] }]).unwrap();
    assert_eq!(s.document().iter::<LayoutBox>().count(), 0);
}

#[test]
fn annotations_and_detailing() {
    let mut s = house();
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "add_callout", "text": "Verify in field", "at": ["50'","20'"], "target": ["40'","15'"]},
            {"op": "add_callout", "text": "3", "at": ["45'","5'"], "marker": "hexagon"},
            {"op": "add_revision_cloud", "points": [["2'","2'"],["14'","2'"],["14'","10'"],["2'","10'"]]},
            {"op": "add_cad", "shape": {"shape": "cross_box", "a": [0,"-3'"], "b": ["10'","-3'"], "width": "5.5\""}},
            {"op": "add_cad", "shape": {"shape": "insulation", "a": [0,"-5'"], "b": ["10'","-5'"], "width": "3.5\""}},
            {"op": "add_text", "position": ["2'","-8'"], "text": "{project} plan"}]"#,
    )
    .unwrap();
    let out = s.apply(&cmds).unwrap();
    let doc = s.document();
    let d = plan(doc, &PlanOptions::first(doc).unwrap());
    let texts: Vec<&str> = d
        .items
        .iter()
        .filter_map(|i| match &i.prim {
            Prim::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"Verify in field"));
    assert!(
        texts.contains(&"Test House plan"),
        "macros expand in plan text"
    );
    assert!(
        texts.contains(&"1"),
        "first revision cloud is numbered 1: {texts:?}"
    );
    // The callout's arrowhead sits at its target.
    let callout = out.created[0];
    let target = housecraft_api::geom::DVec2::new(40.0 * 304.8, 15.0 * 304.8);
    assert!(d.items.iter().any(|i| i.object == Some(callout)
        && matches!(&i.prim, Prim::Polygon { points, .. } if points[0].distance(target) < 1e-6)));
    // The cloud is scalloped: many more points than its four corners.
    let cloud = out.created[2];
    let scallops = d
        .items
        .iter()
        .filter(|i| i.object == Some(cloud))
        .filter_map(|i| match &i.prim {
            Prim::Polyline { points, .. } => Some(points.len()),
            _ => None,
        })
        .max()
        .unwrap();
    assert!(scallops > 40, "{scallops}");
    // A cross box draws its outline and both diagonals.
    let cross = out.created[3];
    assert_eq!(
        d.items
            .iter()
            .filter(|i| i.object == Some(cross) && matches!(i.prim, Prim::Polyline { .. }))
            .count(),
        3
    );
}
