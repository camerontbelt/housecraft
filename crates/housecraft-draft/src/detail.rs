//! CAD details: blank drafting sheets, and turning any 2D view into one.

use housecraft_api::{Command, DetailText};
use housecraft_geom::pt;
use housecraft_model::*;
use housecraft_units::Length;

use crate::{Drawing, Fill, Ink, Prim};

/// The drawing of CAD detail `id`.
pub fn cad_detail(doc: &Document, id: Id, layer_set: Option<Id>) -> Drawing {
    crate::plan::drafting(doc, id, layer_set)
}

/// A command that makes CAD detail `name` holding `drawing` as plain,
/// editable CAD lines, fills and text (CAD Detail from View). A filled
/// polygon followed by its outline becomes one filled closed polyline, so
/// nearer surfaces still hide what is behind them.
pub fn detail_from_drawing(drawing: &Drawing, name: &str) -> Command {
    let color = |ink: Ink| match ink {
        Ink::Color(c) => Some(c),
        Ink::Default | Ink::Faded => None,
    };
    let fill_color = |fill: Fill| match fill {
        Fill::Ink(ink) => color(ink).or(Some(Color::rgb(0, 0, 0))),
        Fill::Poche => Some(Color::rgb(150, 150, 150)),
        Fill::Paper => Some(Color::rgb(255, 255, 255)),
        Fill::Shadow => Some(Color::rgb(215, 215, 222)),
    };
    let mut shapes: Vec<DetailShape> = Vec::new();
    let mut texts = Vec::new();
    let items = &drawing.items;
    let mut i = 0;
    while i < items.len() {
        match &items[i].prim {
            Prim::Polygon { points, fill } if points.len() >= 3 => {
                let mut shape = DetailShape {
                    shape: Shape::Polyline {
                        points: points.iter().map(|p| pt(*p)).collect(),
                        closed: true,
                    },
                    color: None,
                    fill: fill_color(*fill),
                };
                // Its outline, if drawn next, joins it.
                match items.get(i + 1).map(|n| &n.prim) {
                    Some(Prim::Polyline {
                        points: outline,
                        closed: true,
                        stroke,
                    }) if outline == points => {
                        shape.color = color(stroke.ink);
                        i += 1;
                    }
                    // Unoutlined fills get an outline in their own colour.
                    _ => shape.color = shape.fill,
                }
                shapes.push(shape);
            }
            Prim::Polyline {
                points,
                closed,
                stroke,
            } if points.len() >= 2 => shapes.push(DetailShape {
                shape: Shape::Polyline {
                    points: points.iter().map(|p| pt(*p)).collect(),
                    closed: *closed,
                },
                color: color(stroke.ink),
                fill: None,
            }),
            Prim::Text {
                at,
                text,
                height,
                rotation,
                justify,
                ..
            } => texts.push(DetailText {
                position: pt(*at),
                text: text.clone(),
                height: Some(Length::from_mm(*height)),
                rotation: *rotation,
                justify: *justify,
            }),
            _ => {}
        }
        i += 1;
    }
    Command::AddCadDetail {
        name: name.to_string(),
        shapes,
        texts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Stroke;
    use housecraft_geom::DVec2;

    #[test]
    fn fill_and_outline_merge() {
        let square = vec![
            DVec2::ZERO,
            DVec2::new(1000.0, 0.0),
            DVec2::new(1000.0, 1000.0),
            DVec2::new(0.0, 1000.0),
        ];
        let mut d = Drawing::default();
        d.push(
            None,
            Prim::Polygon {
                points: square.clone(),
                fill: Fill::Ink(Ink::Color(Color::rgb(200, 10, 10))),
            },
        );
        d.push(
            None,
            Prim::Polyline {
                points: square,
                closed: true,
                stroke: Stroke {
                    ink: Ink::Default,
                    width: 0.13,
                    dash: &[],
                },
            },
        );
        let Command::AddCadDetail { shapes, .. } = detail_from_drawing(&d, "D") else {
            panic!()
        };
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].fill, Some(Color::rgb(200, 10, 10)));
        assert_eq!(shapes[0].color, None);
    }
}
