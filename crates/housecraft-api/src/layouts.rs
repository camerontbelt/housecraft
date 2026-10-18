//! Layout sheets: placing views on pages.

use housecraft_model::*;
use housecraft_units::Length;

use crate::ApiError;
use crate::ops::expect;

/// Margin around a page and the title strip along its bottom (paper mm).
pub const MARGIN: f64 = 12.0;
pub const TITLE_STRIP: f64 = 20.0;
/// Space under each box for its caption and scale.
pub const CAPTION: f64 = 12.0;

/// A short human name for what a box shows.
pub fn source_name(doc: &Document, s: &LayoutSource) -> String {
    match s {
        LayoutSource::Plan { floor, .. } => {
            format!(
                "{} Plan",
                doc.get::<Floor>(*floor).map_or("?", |f| f.name.as_str())
            )
        }
        LayoutSource::View { view } => doc
            .get::<SectionView>(*view)
            .map_or("?".into(), |v| v.name.clone()),
        LayoutSource::Schedule { schedule_kind, .. } => schedule_kind.name().to_string(),
        LayoutSource::MaterialsList => "Materials List".into(),
        LayoutSource::Text { .. } => "Notes".into(),
    }
}

/// Place a box on a layout page; unspecified position and size take the
/// next free quarter of the drawing area.
pub(crate) fn send(
    doc: &mut Document,
    layout: Id,
    page: u32,
    source: LayoutSource,
    rect: [Option<Length>; 4],
    scale: Option<f64>,
    label: Option<String>,
) -> Result<Id, ApiError> {
    let l = expect::<Layout>(doc, layout)?.clone();
    if page as usize >= l.pages.len() {
        return Err(ApiError::Invalid(format!(
            "{} has {} page(s); pages count from 0",
            l.name,
            l.pages.len()
        )));
    }
    if let Some(s) = scale
        && !(s.is_finite() && s >= 0.0)
    {
        return Err(ApiError::Invalid(
            "scale must be 0 (fit) or positive".into(),
        ));
    }
    let (pw, ph) = (l.paper_width.mm(), l.paper_height.mm());
    let bottom = MARGIN + if l.title_block { TITLE_STRIP } else { 0.0 };
    let (aw, ah) = (pw - 2.0 * MARGIN, ph - MARGIN - bottom);
    let used = doc
        .iter::<LayoutBox>()
        .filter(|b| b.layout == layout && b.page == page)
        .count();
    // Quarters in reading order: top-left, top-right, bottom-left, bottom-right,
    // each with room for its caption underneath.
    let q = used % 4;
    let gap = 6.0;
    let (cw, ch) = ((aw - gap) / 2.0, (ah - gap - 2.0 * CAPTION) / 2.0);
    let cx = MARGIN + (q % 2) as f64 * (cw + gap);
    let cy = bottom + CAPTION + if q < 2 { ch + gap + CAPTION } else { 0.0 };
    let mm = Length::from_mm;
    Ok(doc.insert(LayoutBox {
        id: Id(0),
        layout,
        page,
        source,
        x: rect[0].unwrap_or(mm(cx)),
        y: rect[1].unwrap_or(mm(cy)),
        width: rect[2].unwrap_or(mm(cw)),
        height: rect[3].unwrap_or(mm(ch)),
        scale: scale.unwrap_or(0.0),
        center: None,
        label,
    }))
}
