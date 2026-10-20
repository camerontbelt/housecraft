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
    // Exterior dimensions locate the outside of the framing (as Chief's
    // do): 40' between centrelines + 2 x 2 13/32" out to the framing face.
    assert!(texts.contains(&"40'-4 13/16\""), "{texts:?}");
    assert!(texts.contains(&"30'-4 13/16\""), "{texts:?}");
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

#[test]
fn section_view_extents_jogs_hidden_lines_and_labels() {
    use housecraft_api::model::{Id, SectionView};
    let mut s = house();
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "build_foundation", "kind": "crawlspace"},
            {"op": "set_roof_style", "style": "gable"},
            {"op": "add_section_view", "name": "West", "a": ["-10'","40'"], "b": ["-10'","-10'"]},
            {"op": "add_section_view", "name": "Cut", "a": ["-10'","15'"], "b": ["50'","15'"], "section_kind": "section"}]"#,
    )
    .unwrap();
    s.apply(&cmds).unwrap();
    let by_name = |s: &Session, n: &str| s.document().iter::<SectionView>().find(|v| v.name == n).unwrap().id;
    let (west, cut) = (by_name(&s, "West"), by_name(&s, "Cut"));
    let draw = |s: &Session, id: Id| housecraft_draft::elevation::section_view(s.document(), s.document().get::<SectionView>(id).unwrap());
    let texts = |d: &housecraft_draft::Drawing| -> Vec<String> {
        d.items.iter().filter_map(|i| match &i.prim {
            Prim::Text { text, .. } => Some(text.clone()),
            _ => None,
        }).collect()
    };
    let dashed = |d: &housecraft_draft::Drawing| d.items.iter().filter(|i| matches!(&i.prim, Prim::Polyline { stroke, .. } if !stroke.dash.is_empty())).count();
    let span = |d: &housecraft_draft::Drawing| {
        let (mut lo, mut hi, mut top) = (f64::MAX, f64::MIN, f64::MIN);
        for i in &d.items {
            if let Prim::Polygon { points, .. } = &i.prim {
                for p in points {
                    lo = lo.min(p.x);
                    hi = hi.max(p.x);
                    top = top.max(p.y);
                }
            }
        }
        (lo, hi, top)
    };
    let ft = 304.8;
    // The west wall's door and window carry their schedule numbers; the
    // crawlspace below grade is dashed.
    let d = draw(&s, west);
    let t = texts(&d);
    assert!(t.iter().any(|x| x == "D01") && t.iter().any(|x| x == "W01"), "{t:?}");
    assert!(dashed(&d) > 0);
    let upd = |s: &mut Session, id: Id, set: serde_json::Value| {
        s.apply(&[Command::Update { id, set: set.as_object().unwrap().clone() }]).unwrap();
    };
    upd(&mut s, west, serde_json::json!({"schedule_labels": false, "hidden_below_grade": false}));
    let d = draw(&s, west);
    assert!(!texts(&d).iter().any(|x| x == "D01"));
    assert_eq!(dashed(&d), 0);
    // Cropped extents, and nothing above the floor's ceiling.
    upd(&mut s, west, serde_json::json!({"crop_left": "20'", "crop_right": "35'", "ignore_above": true}));
    let (lo, hi, top) = span(&draw(&s, west));
    assert!(lo >= 20.0 * ft - 1.0 && hi <= 35.0 * ft + 1.0, "{lo} {hi}");
    assert!(top < 10.0 * ft, "roof cropped off: {top}");
    // A jog moves the cut beyond the interior wall's line part of the way.
    let heavy = |d: &housecraft_draft::Drawing| -> Vec<(i64, i64)> {
        let mut v: Vec<(i64, i64)> = d.items.iter().filter_map(|i| match &i.prim {
            Prim::Polyline { stroke, points, .. } if stroke.width >= 0.5 => Some((points[0].x as i64, points[0].y as i64)),
            _ => None,
        }).collect();
        v.sort();
        v
    };
    let plain = heavy(&draw(&s, cut));
    upd(&mut s, cut, serde_json::json!({"steps": [{"at": "30'", "offset": "10'"}]}));
    let stepped = heavy(&draw(&s, cut));
    assert_ne!(plain, stepped);
}

#[test]
fn copied_views_keep_their_annotations_and_callouts_show_pages() {
    use housecraft_api::model::{SectionView, Text};
    let mut s = house();
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "add_section_view", "name": "South", "a": ["-10'","-10'"], "b": ["50'","-10'"]},
            {"op": "add_layout", "name": "Set", "pages": 3}]"#,
    )
    .unwrap();
    let out = s.apply(&cmds).unwrap();
    let (view, layout) = (out.created[0], out.created[1]);
    let cmds: Vec<Command> = serde_json::from_str(&format!(
        r#"[{{"op": "add_text", "floor": {v}, "position": ["5'","5'"], "text": "Brick veneer"}},
            {{"op": "send_to_layout", "layout": {l}, "page": 2, "source": {{"type": "view", "view": {v}}}}}]"#,
        v = view.0,
        l = layout.0
    ))
    .unwrap();
    s.apply(&cmds).unwrap();
    let doc = s.document();
    let plan_d = plan(doc, &PlanOptions::first(doc).unwrap());
    assert!(plan_d.items.iter().any(|i| i.object == Some(view) && matches!(&i.prim, Prim::Text { text, .. } if text == "A-3")));
    // Copy and paste the view: the note comes along onto the copy.
    let copy = serde_json::to_value(doc.object(view).unwrap()).unwrap();
    let out = s
        .apply(&[serde_json::from_value(serde_json::json!({"op": "insert_objects", "objects": [copy], "dx": "1'", "dy": 0})).unwrap()])
        .unwrap();
    let doc = s.document();
    let new = out.created.iter().copied().find(|id| doc.get::<SectionView>(*id).is_some()).unwrap();
    assert!(doc.iter::<Text>().any(|t| t.floor == new && t.text == "Brick veneer"));
    assert!(doc.iter::<Text>().any(|t| t.floor == view));
}

#[test]
fn layout_pages_templates_revisions_and_project_info() {
    use housecraft_api::model::{Layout, LayoutBox};
    let mut s = house();
    let floor = s.document().floors_by_elevation()[0].id;
    let out = s
        .apply(&[serde_json::from_value(serde_json::json!({"op": "add_layout", "name": "Permit", "pages": 3})).unwrap()])
        .unwrap();
    let layout = out.created[0];
    let cmds: Vec<Command> = serde_json::from_str(&format!(
        r#"[{{"op": "set_project_info", "info": {{"client": "J. Smith", "designer": "C. Belt", "custom": {{"permit_number": "BP-77"}}}}}},
            {{"op": "add_revision", "description": "Issued for permit", "by": "CB", "date": "2026-10-01"}},
            {{"op": "add_revision", "description": "Moved kitchen window", "date": "2026-10-15"}},
            {{"op": "set_watermark", "watermark": {{"text": "DRAFT {{sheet}}"}}}},
            {{"op": "number_pages", "layout": {l}, "prefix": "S", "start": 0}},
            {{"op": "update", "id": {l}, "set": {{"titles": ["Cover", "Plans", "Details"], "templates": [null, 0, 0]}}}},
            {{"op": "send_to_layout", "layout": {l}, "page": 0, "source": {{"type": "text", "text": "{{client}} / {{permit_number}} / {{sheet}} {{page_title}} ({{page}} of {{page_count}}) rev {{revision}}"}}}},
            {{"op": "send_to_layout", "layout": {l}, "page": 0, "source": {{"type": "revisions"}}}},
            {{"op": "send_to_layout", "layout": {l}, "page": 0, "source": {{"type": "sheet_index", "layout": {l}}}}},
            {{"op": "send_to_layout", "layout": {l}, "page": 1, "source": {{"type": "plan", "floor": {f}}}, "x": "10", "y": "100", "width": "200", "height": "150"}},
            {{"op": "send_to_layout", "layout": {l}, "page": 1, "source": {{"type": "plan", "floor": {f}}}, "x": "300", "y": "300", "width": "200", "height": "150"}}]"#,
        l = layout.0,
        f = floor.0
    ))
    .unwrap();
    s.apply(&cmds).unwrap();
    let doc = s.document();
    let l = doc.get::<Layout>(layout).unwrap();
    assert_eq!(l.pages, ["S0", "S1", "S2"]);
    let texts = |page: usize| -> Vec<String> {
        housecraft_draft::layout::layout_page(doc, l, page)
            .items
            .into_iter()
            .filter_map(|i| match i.prim {
                Prim::Text { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    };
    let p0 = texts(0);
    assert!(p0.iter().any(|t| t == "J. Smith / BP-77 / S0 Cover (1 of 3) rev 2"), "{p0:?}");
    assert!(p0.iter().any(|t| t == "Moved kitchen window"));
    assert!(p0.iter().any(|t| t == "Details"), "sheet index lists titles");
    assert!(p0.iter().any(|t| t == "DRAFT S0"));
    assert!(p0.iter().any(|t| t.contains("Rev 2")), "title block shows the revision");
    // Page 1 draws the cover's notes underneath, with its own page macros.
    let p1 = texts(1);
    assert!(p1.iter().any(|t| t == "J. Smith / BP-77 / S1 Plans (2 of 3) rev 2"), "{p1:?}");
    assert!(p1.iter().any(|t| t == "PLANS"), "page title in the title block");
    // Drawing boxes are numbered top to bottom: the upper one is 1.
    let boxes: Vec<&LayoutBox> = doc.iter::<LayoutBox>().filter(|b| b.page == 1).collect();
    let upper = boxes.iter().max_by(|a, b| a.y.mm().total_cmp(&b.y.mm())).unwrap();
    assert_eq!(housecraft_api::layouts::box_number(doc, upper), Some(1));
    assert!(p1.iter().any(|t| t == "2"));
    // Bulk delete: the plans on page 1 only.
    let out = s
        .apply(&[serde_json::from_value(serde_json::json!({"op": "delete_layout_boxes", "layout": layout.0, "page": 1, "kind": "plan"})).unwrap()])
        .unwrap();
    assert_eq!(out.deleted.len(), 2);
    assert_eq!(s.document().iter::<LayoutBox>().count(), 3);
}

#[test]
fn layout_box_converts_to_editable_lines() {
    use housecraft_api::model::{Cad, LayoutBox, LayoutSource};
    let mut s = house();
    let floor = s.document().floors_by_elevation()[0].id;
    let out = s
        .apply(&[serde_json::from_value(serde_json::json!({"op": "add_layout", "name": "Set", "pages": 1})).unwrap()])
        .unwrap();
    let out = s
        .apply(&[serde_json::from_value(serde_json::json!({"op": "send_to_layout", "layout": out.created[0].0, "source": {"type": "plan", "floor": floor.0}})).unwrap()])
        .unwrap();
    let id = out.created[0];
    let doc = s.document();
    let b = doc.get::<LayoutBox>(id).unwrap();
    let scale = housecraft_draft::layout::box_scale(doc, b);
    let d = housecraft_draft::layout::box_model_drawing(doc, &b.source).unwrap();
    let (shapes, texts) = housecraft_draft::layout::to_linework(&d);
    assert!(!shapes.is_empty() && !texts.is_empty());
    let n = shapes.len();
    s.apply(&[Command::ConvertBoxToLines { id, scale, shapes, texts }]).unwrap();
    let doc = s.document();
    let b = doc.get::<LayoutBox>(id).unwrap();
    let LayoutSource::Detail { detail } = b.source else {
        panic!("box shows {:?}", b.source)
    };
    assert_eq!(b.scale, scale);
    assert_eq!(doc.iter::<Cad>().filter(|c| c.floor == detail).count(), n);
}

#[test]
fn layout_boxes_paste_into_another_plan_with_their_details() {
    use housecraft_api::model::{Cad, CadDetail, LayoutBox, LayoutSource};
    let mut a = house();
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "add_cad_detail", "name": "Footing", "shapes": [{"shape": {"shape": "line", "a": [0,0], "b": ["2'",0]}}]},
            {"op": "add_layout", "name": "Set", "pages": 1}]"#,
    )
    .unwrap();
    let out = a.apply(&cmds).unwrap();
    let (detail, layout) = (out.created[0], *out.created.last().unwrap());
    let out = a
        .apply(&[serde_json::from_value(serde_json::json!({"op": "send_to_layout", "layout": layout.0, "source": {"type": "detail", "detail": detail.0}})).unwrap()])
        .unwrap();
    let doc = a.document();
    let mut clip: Vec<serde_json::Value> = vec![housecraft_api::query::get_object(doc, out.created[0]).unwrap(), housecraft_api::query::get_object(doc, detail).unwrap()];
    clip.extend(doc.iter::<Cad>().filter(|c| c.floor == detail).map(|c| housecraft_api::query::get_object(doc, c.id).unwrap()));
    // Another plan with its own layout.
    let mut b = house();
    let out = b
        .apply(&[serde_json::from_value(serde_json::json!({"op": "add_layout", "name": "Other", "pages": 1})).unwrap()])
        .unwrap();
    let target = out.created[0];
    for v in clip.iter_mut() {
        if v["kind"] == "layout_box" {
            v["layout"] = serde_json::json!(target.0);
        }
    }
    b.apply(&[serde_json::from_value(serde_json::json!({"op": "insert_objects", "objects": clip, "dx": 0, "dy": 0})).unwrap()])
        .unwrap();
    let doc = b.document();
    let bx = doc.iter::<LayoutBox>().find(|x| x.layout == target).unwrap();
    let LayoutSource::Detail { detail: d } = bx.source else { panic!() };
    assert_eq!(doc.get::<CadDetail>(d).unwrap().name, "Footing");
    assert_eq!(doc.iter::<Cad>().filter(|c| c.floor == d).count(), 1);
}

#[test]
fn schedule_thumbnails_and_marker_labels() {
    use housecraft_api::model::Schedule;
    let mut s = house();
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "add_schedule", "schedule_kind": "doors", "position": ["50'","30'"]}]"#,
    )
    .unwrap();
    let out = s.apply(&cmds).unwrap();
    let sched = out.created[0];
    let set = s.document().settings.active_default_set;
    let cmds: Vec<Command> = vec![
        Command::Update { id: sched, set: serde_json::json!({"thumbnails": true}).as_object().unwrap().clone() },
        Command::Update { id: set, set: serde_json::json!({"labels": {"windows": "{schedule}", "shape": "hexagon"}}).as_object().unwrap().clone() },
    ];
    s.apply(&cmds).unwrap();
    let doc = s.document();
    let d = plan(doc, &PlanOptions::first(doc).unwrap());
    // The window's label reads its schedule number inside a hexagon.
    let win = doc.iter::<housecraft_api::model::Opening>().find(|o| !o.opening_kind.is_door()).unwrap().id;
    assert!(d.items.iter().any(|i| i.object == Some(win) && matches!(&i.prim, Prim::Text { text, .. } if text == "W01")));
    assert!(d.items.iter().any(|i| i.object == Some(win) && matches!(&i.prim, Prim::Polyline { points, closed: true, .. } if points.len() == 6)));
    // The schedule draws a picture of the door: lines inside the table.
    let s0 = doc.get::<Schedule>(sched).unwrap();
    let table_lines = d.items.iter().filter(|i| i.object == Some(sched) && matches!(i.prim, Prim::Polyline { .. })).count();
    let plain = {
        let mut doc2 = doc.clone();
        doc2.get_mut::<Schedule>(sched).unwrap().thumbnails = false;
        plan(&doc2, &PlanOptions::first(&doc2).unwrap()).items.iter().filter(|i| i.object == Some(s0.id) && matches!(i.prim, Prim::Polyline { .. })).count()
    };
    assert!(table_lines > plain + 2, "{table_lines} vs {plain}");
}

#[test]
fn notes_links_macros_find_replace_and_block_arrows() {
    use housecraft_api::model::{Callout, Text, TextStyle};
    let mut s = house();
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "add_section_view", "name": "South", "a": ["-10'","-10'"], "b": ["50'","-10'"]},
            {"op": "add_layout", "name": "Set", "pages": 2},
            {"op": "set_macro", "name": "builder", "text": "Acme for {project}"},
            {"op": "add_text", "position": ["5'","5'"], "text": "Built by {builder}"},
            {"op": "add_callout", "at": ["10'","10'"], "text": "Seal all joints"},
            {"op": "add_callout", "at": ["12'","10'"], "text": "Flash sill"},
            {"op": "add_callout", "at": ["14'","10'"], "text": "See {link}"},
            {"op": "add_block", "name": "Open arrow", "shapes": [{"shape": {"shape": "polyline", "points": [[0,0],["-6\"","3\""],["-6\"","-3\""]], "closed": true}}]}]"#,
    )
    .unwrap();
    let out = s.apply(&cmds).unwrap();
    let doc = s.document();
    let view = doc.iter::<housecraft_api::model::SectionView>().next().unwrap().id;
    let layout = doc.iter::<housecraft_api::model::Layout>().next().unwrap().id;
    let calls: Vec<_> = doc.iter::<Callout>().map(|c| c.id).collect();
    let block = doc.iter::<housecraft_api::model::Block>().next().unwrap().id;
    let set = doc.settings.active_default_set;
    let _ = out;
    let p = |v: serde_json::Value| v.as_object().unwrap().clone();
    s.apply(&[
        Command::Update { id: calls[0], set: p(serde_json::json!({"note": true})) },
        Command::Update { id: calls[1], set: p(serde_json::json!({"note": true})) },
        Command::Update { id: calls[2], set: p(serde_json::json!({"link": view.0})) },
        Command::Update { id: set, set: p(serde_json::json!({"dimensions": {"arrow": {"block": block.0}}})) },
    ])
    .unwrap();
    s.apply(&[serde_json::from_value(serde_json::json!({"op": "send_to_layout", "layout": layout.0, "page": 1, "source": {"type": "view", "view": view.0}})).unwrap()])
        .unwrap();
    let doc = s.document();
    let d = plan(doc, &PlanOptions::first(doc).unwrap());
    let texts: Vec<String> = d.items.iter().filter_map(|i| match &i.prim { Prim::Text { text, .. } => Some(text.clone()), _ => None }).collect();
    assert!(texts.iter().any(|t| t == "Built by Acme for Test House"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "1") && texts.iter().any(|t| t == "2"), "note numbers");
    assert!(texts.iter().any(|t| t == "See 1/A-2"), "{texts:?}");
    let notes = housecraft_api::schedule::schedule(doc, housecraft_api::model::ScheduleKind::Notes, &Default::default());
    assert_eq!(notes.rows.iter().map(|r| r.cells[1].as_str()).collect::<Vec<_>>(), ["Seal all joints", "Flash sill"]);
    // Dimensions end in the block: closed triangles.
    let tris = d.items.iter().filter(|i| matches!(&i.prim, Prim::Polyline { points, closed: true, .. } if points.len() == 3)).count();
    assert!(tris >= 2, "{tris}");
    // Find and replace, case-insensitively; fonts too.
    assert_eq!(housecraft_api::ops_text_matches(doc, "FLASH", false).len(), 1);
    s.apply(&[Command::ReplaceText { find: "flash".into(), replace: "Caulk".into(), match_case: false, ids: Vec::new() }]).unwrap();
    assert!(s.document().iter::<Callout>().any(|c| c.text == "Caulk sill"));
    let font = s.document().iter::<TextStyle>().next().unwrap().font.clone();
    s.apply(&[Command::ReplaceFont { from: font, to: "Courier".into() }]).unwrap();
    assert!(s.document().iter::<TextStyle>().all(|t| t.font == "Courier"));
    assert!(s.document().iter::<Text>().any(|t| t.text == "Built by {builder}"));
}

#[test]
fn cad_conversions_roof_outlines_wall_hatches_and_user_patterns() {
    use housecraft_api::model::{Cad, Countertop, FillPattern, HatchLine, HatchPattern, Wall};
    use housecraft_api::CadTarget;
    let mut s = house();
    let p = |v: serde_json::Value| v.as_object().unwrap().clone();
    // Two parallel lines 6" apart: one wall between them, of the nearest type.
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "add_cad", "shape": {"shape": "line", "a": ["60'",0], "b": ["70'",0]}},
            {"op": "add_cad", "shape": {"shape": "line", "a": ["60'","6\""], "b": ["70'","6\""]}},
            {"op": "add_cad", "shape": {"shape": "polyline", "points": [["60'","10'"],["66'","10'"],["66'","12'"],["60'","12'"]], "closed": true}}]"#,
    )
    .unwrap();
    let out = s.apply(&cmds).unwrap();
    let (l1, l2, rect) = (out.created[0], out.created[1], out.created[2]);
    let walls_before = s.document().iter::<Wall>().count();
    s.apply(&[Command::ConvertCad { ids: vec![l1, l2], to: CadTarget::DoubleLineWalls }]).unwrap();
    let doc = s.document();
    assert_eq!(doc.iter::<Wall>().count(), walls_before + 1);
    let w = doc.iter::<Wall>().max_by_key(|w| w.id).unwrap();
    assert!((w.start.y().inches() - 3.0).abs() < 0.01, "on the centre line: {:?}", w.start);
    assert!(doc.get::<Cad>(l1).is_none() && doc.get::<Cad>(l2).is_none());
    s.apply(&[Command::ConvertCad { ids: vec![rect], to: CadTarget::Countertop }]).unwrap();
    assert_eq!(s.document().iter::<Countertop>().count(), 1);
    // Roof planes traced as CAD.
    s.apply(&serde_json::from_str::<Vec<Command>>(r#"[{"op": "set_roof_style", "style": "gable"}]"#).unwrap()).unwrap();
    let cads = s.document().iter::<Cad>().count();
    let out = s.apply(&[Command::RoofPlanesToPolylines { ids: Vec::new() }]).unwrap();
    assert!(out.created.len() >= 2 && s.document().iter::<Cad>().count() == cads + out.created.len());
    // A user pattern on a wall hatch, then deleted: the hatch falls back to plain.
    let pattern = HatchPattern { name: "Shear".into(), lines: vec![HatchLine { angle: 45.0, dash: vec![1.0, 0.5], ..Default::default() }] };
    let wall = s.document().iter::<Wall>().next().unwrap().id;
    s.apply(&[
        Command::SetHatchPattern { index: None, pattern },
        Command::Update { id: wall, set: p(serde_json::json!({"shape": {"hatch": {"pattern": {"user_pattern": 0}, "spacing": 50.0}}})) },
        Command::SetObjectLabel { id: wall, text: Some("SW1".into()) },
    ])
    .unwrap();
    let doc = s.document();
    let d = plan(doc, &PlanOptions::first(doc).unwrap());
    let hatch = d.items.iter().filter(|i| i.object == Some(wall) && matches!(&i.prim, Prim::Polyline { points, .. } if points.len() == 2)).count();
    assert!(hatch > 10, "{hatch}");
    assert!(d.items.iter().any(|i| i.object == Some(wall) && matches!(&i.prim, Prim::Text { text, .. } if text == "SW1")));
    s.apply(&[Command::DeleteHatchPattern { index: 0 }]).unwrap();
    let w = s.document().get::<Wall>(wall).unwrap();
    assert_eq!(w.shape.hatch.unwrap().pattern, FillPattern::Hatch);
}

#[test]
fn plants_can_show_their_own_picture() {
    let mut s = house();
    let out = s.apply(&serde_json::from_str::<Vec<Command>>(r#"[{"op": "add_plant", "at": ["50'","10'"], "name": "Maple"}]"#).unwrap()).unwrap();
    let plant = out.created[0];
    s.apply(&[Command::Update { id: plant, set: serde_json::json!({"image": "maple-top.png"}).as_object().unwrap().clone() }]).unwrap();
    let doc = s.document();
    let d = plan(doc, &PlanOptions::first(doc).unwrap());
    assert!(d.items.iter().any(|i| i.object == Some(plant) && matches!(&i.prim, Prim::Image { path, .. } if path == "maple-top.png")));
}

#[test]
fn template_pages_fill_page_macros_for_the_page_they_sit_under() {
    let mut s = Session::new(Document::new("Test House"));
    let cmds: Vec<Command> = serde_json::from_str(
        r#"[{"op": "add_layout", "name": "Set", "pages": 2},
            {"op": "add_cad_detail", "name": "Border", "shapes": [{"shape": {"shape": "polyline", "points": [[3,3],[911,3],[911,606],[3,606]], "closed": true}}]}]"#,
    )
    .unwrap();
    let out = s.apply(&cmds).unwrap();
    let (layout, detail) = (out.created[0], out.created[1]);
    let cmds: Vec<Command> = serde_json::from_str(&format!(
        r#"[{{"op": "update", "id": {l}, "set": {{"title_block": false, "pages": ["T", "A0.1"], "titles": ["Template", "General Notes"], "templates": [null, 0]}}}},
            {{"op": "send_to_layout", "layout": {l}, "page": 0, "source": {{"type": "detail", "detail": {d}}}, "x": 0, "y": 0, "width": "36\"", "height": "24\"", "scale": 1.0, "label": ""}},
            {{"op": "send_to_layout", "layout": {l}, "page": 0, "source": {{"type": "text", "text": "{{page_title}} {{sheet}}"}}, "x": "34\"", "y": "1\"", "width": "2\"", "height": "1\"", "label": ""}}]"#,
        l = layout.0,
        d = detail.0
    ))
    .unwrap();
    s.apply(&cmds).unwrap();
    let doc = s.document();
    let l = doc.get::<housecraft_api::model::Layout>(layout).unwrap();
    let d = housecraft_draft::layout::layout_page(doc, l, 1);
    let texts: Vec<&str> = d
        .items
        .iter()
        .filter_map(|i| match &i.prim {
            Prim::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t.contains("General Notes A0.1")), "{texts:?}");
    let lines = d.items.iter().filter(|i| !matches!(i.prim, Prim::Text { .. })).count();
    assert!(lines > 0, "the template's border is drawn under page 2");
}
