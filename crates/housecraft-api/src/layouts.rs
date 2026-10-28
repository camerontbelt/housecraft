//! Layout sheets: placing views on pages.

use housecraft_geom::DVec2;
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
        LayoutSource::Plan { plan_view: Some(v), .. } if doc.get::<PlanView>(*v).is_some() => {
            doc.get::<PlanView>(*v).map_or_else(String::new, |v| v.name.clone())
        }
        LayoutSource::Plan { floor, .. } => {
            format!(
                "{} Plan",
                doc.get::<Floor>(*floor).map_or("?", |f| f.name.as_str())
            )
        }
        LayoutSource::View { view } => doc
            .get::<SectionView>(*view)
            .map_or("?".into(), |v| v.name.clone()),
        LayoutSource::Detail { detail } => doc
            .get::<CadDetail>(*detail)
            .map_or("?".into(), |d| d.name.clone()),
        LayoutSource::Schedule { schedule_kind, .. } => schedule_kind.name().to_string(),
        LayoutSource::MaterialsList => "Materials List".into(),
        LayoutSource::Revisions { .. } => "Revisions".into(),
        LayoutSource::SheetIndex { .. } => "Sheet Index".into(),
        LayoutSource::Text { .. } => "Notes".into(),
        LayoutSource::Picture { .. } => "Picture".into(),
        LayoutSource::Lines { .. } => "Lines".into(),
    }
}

/// The kind of thing a box shows, as `DeleteLayoutBoxes` names it.
pub fn source_kind(s: &LayoutSource) -> &'static str {
    match s {
        LayoutSource::Plan { .. } => "plan",
        LayoutSource::View { .. } => "view",
        LayoutSource::Detail { .. } => "detail",
        LayoutSource::Schedule { .. } => "schedule",
        LayoutSource::MaterialsList => "materials_list",
        LayoutSource::Revisions { .. } => "revisions",
        LayoutSource::SheetIndex { .. } => "sheet_index",
        LayoutSource::Text { .. } => "text",
        LayoutSource::Picture { .. } => "picture",
        LayoutSource::Lines { .. } => "lines",
    }
}

/// Whether a box shows a scaled drawing of the model (not a table or notes).
pub fn is_drawing(s: &LayoutSource) -> bool {
    matches!(s, LayoutSource::Plan { .. } | LayoutSource::View { .. } | LayoutSource::Detail { .. })
}

/// Boxes showing drawings are numbered on their page, top to bottom then
/// left to right: the "1" in a "1/A-3" callout.
pub fn box_number(doc: &Document, b: &LayoutBox) -> Option<u32> {
    if !is_drawing(&b.source) {
        return None;
    }
    let mut boxes: Vec<&LayoutBox> = doc
        .iter::<LayoutBox>()
        .filter(|o| o.layout == b.layout && o.page == b.page && is_drawing(&o.source))
        .collect();
    boxes.sort_by(|p, q| {
        let top = |o: &LayoutBox| -(o.y.mm() + o.height.mm());
        top(p).total_cmp(&top(q)).then(p.x.mm().total_cmp(&q.x.mm())).then(p.id.cmp(&q.id))
    });
    boxes.iter().position(|o| o.id == b.id).map(|i| i as u32 + 1)
}

/// The revision table.
pub fn revision_table(doc: &Document) -> crate::schedule::ScheduleTable {
    revision_table_for(doc, None, 0)
}

/// The revision table as drawn on sheet `sheet` (a page name): only the
/// revisions for that sheet, padded with blank rows to `min_rows`.
pub fn revision_table_for(doc: &Document, sheet: Option<&str>, min_rows: u32) -> crate::schedule::ScheduleTable {
    let mut rows: Vec<_> = doc
        .settings
        .revisions
        .iter()
        .filter(|r| r.sheets.is_empty() || sheet.is_none_or(|s| r.sheets.iter().any(|x| x == s)))
        .map(|r| crate::schedule::ScheduleRow {
            ids: Vec::new(),
            cells: vec![r.number.to_string(), r.date.clone(), r.description.clone(), r.by.clone()],
        })
        .collect();
    while rows.len() < min_rows as usize {
        rows.push(crate::schedule::ScheduleRow { ids: Vec::new(), cells: vec![String::new(); 4] });
    }
    crate::schedule::ScheduleTable {
        title: "Revisions".into(),
        columns: ["Rev", "Date", "Description", "By"].map(String::from).to_vec(),
        rows,
    }
}

/// The sheet index of `layout`: each page's sheet number and title.
pub fn sheet_index(doc: &Document, layout: Id) -> crate::schedule::ScheduleTable {
    let pages = doc.get::<Layout>(layout).map_or(Vec::new(), |l| {
        l.pages
            .iter()
            .enumerate()
            .filter(|(i, _)| !l.is_template_page(*i))
            .map(|(i, p)| crate::schedule::ScheduleRow {
                ids: Vec::new(),
                cells: vec![p.clone(), l.title(i).to_string()],
            })
            .collect()
    });
    crate::schedule::ScheduleTable {
        title: "Sheet Index".into(),
        columns: vec!["Sheet".into(), "Title".into()],
        rows: pages,
    }
}

/// The sheet index with its headings renamed by `columns` (an empty name
/// drops that column).
pub fn sheet_index_with(doc: &Document, layout: Id, columns: &[String]) -> crate::schedule::ScheduleTable {
    let mut t = sheet_index(doc, layout);
    if columns.is_empty() {
        return t;
    }
    let keep: Vec<usize> = (0..t.columns.len()).filter(|i| columns.get(*i).is_none_or(|c| !c.is_empty())).collect();
    t.columns = keep.iter().map(|i| columns.get(*i).cloned().unwrap_or_else(|| t.columns[*i].clone())).collect();
    for r in &mut t.rows {
        r.cells = keep.iter().map(|i| r.cells[*i].clone()).collect();
    }
    t
}

/// Centre a box on the page's drawing area (inside the border and above the
/// title strip), or inside box `within`.
pub(crate) fn center_box(doc: &mut Document, id: Id, within: Option<Id>) -> Result<(), ApiError> {
    let b = expect::<LayoutBox>(doc, id)?.clone();
    let (lo, hi) = match within {
        Some(w) => {
            let w = expect::<LayoutBox>(doc, w)?;
            (DVec2::new(w.x.mm(), w.y.mm()), DVec2::new(w.x.mm() + w.width.mm(), w.y.mm() + w.height.mm()))
        }
        None => {
            let l = expect::<Layout>(doc, b.layout)?;
            let bottom = MARGIN + if l.title_block { TITLE_STRIP } else { 0.0 };
            (DVec2::new(MARGIN, bottom), DVec2::new(l.paper_width.mm() - MARGIN, l.paper_height.mm() - MARGIN))
        }
    };
    let c = (lo + hi) / 2.0;
    let nb = doc.get_mut::<LayoutBox>(id).ok_or(ApiError::NotFound(id))?;
    nb.x = Length::from_mm(c.x - b.width.mm() / 2.0);
    nb.y = Length::from_mm(c.y - b.height.mm() / 2.0);
    Ok(())
}

/// Today's date (UTC) as YYYY-MM-DD.
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{m:02}-{d:02}", yoe + era * 400 + i64::from(m <= 2))
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
    let default_scale = if matches!(source, LayoutSource::Plan { .. }) { doc.settings.layout_scale } else { 0.0 };
    Ok(doc.insert(LayoutBox {
        id: Id(0),
        layout,
        page,
        source,
        x: rect[0].unwrap_or(mm(cx)),
        y: rect[1].unwrap_or(mm(cy)),
        width: rect[2].unwrap_or(mm(cw)),
        height: rect[3].unwrap_or(mm(ch)),
        // Plans take the plan's default drawing scale.
        scale: scale.unwrap_or(default_scale),
        center: None,
        label,
        rotation: 0.0,
        caption_offset: None,
        lines_only: false,
        link_page: None,
    }))
}
