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
