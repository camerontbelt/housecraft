//! Text macros: `{name}` placeholders in notes and labels filled from the
//! document, e.g. `{project}`, `{date}`, `{living_area}`.

use housecraft_model::Document;

/// Macros [`expand`] understands, with what they insert.
pub const MACROS: &[(&str, &str)] = &[
    ("project", "Document name"),
    ("date", "Today's date (YYYY-MM-DD)"),
    ("year", "This year"),
    ("living_area", "Total living area"),
    ("floors", "Number of above-ground floors"),
    ("rooms", "Number of rooms"),
    ("client", "Client (Project Information)"),
    ("client_address", "Client's address"),
    ("client_phone", "Client's phone"),
    ("client_email", "Client's email"),
    ("designer", "Designer"),
    ("company", "Designer's company"),
    ("designer_address", "Designer's address"),
    ("designer_phone", "Designer's phone"),
    ("designer_email", "Designer's email"),
    ("license", "Designer's license number"),
    ("revision", "Latest revision number"),
    ("revision_date", "Latest revision's date"),
];

/// Macros for layout pages (notes boxes, captions and template pages).
pub const PAGE_MACROS: &[(&str, &str)] = &[
    ("sheet", "This page's sheet number (A-3)"),
    ("page", "This page's number"),
    ("page_count", "How many pages the layout has"),
    ("page_title", "This page's title"),
    ("layout", "The layout's name"),
];

/// Macros for object labels, filled from the labelled object.
pub const OBJECT_MACROS: &[(&str, &str)] = &[
    ("width", "Width"),
    ("height", "Height"),
    ("depth", "Depth"),
    ("sill", "Sill height (windows)"),
    ("size", "Width and height as feet-inch digits, e.g. 3040"),
    ("kind", "What it is (door, cabinet…)"),
    ("type", "Its type or style"),
    ("code", "Code (Object Information, else the cabinet code)"),
    ("description", "Description (Object Information)"),
    ("manufacturer", "Manufacturer (Object Information)"),
    ("comment", "Comment (Object Information)"),
    ("layer", "Layer name"),
];

/// Feet and inches as digits: 3'-0" is "30", 2'-10" is "210".
fn size_digits(mm: f64) -> String {
    let inches = (mm / 25.4).round() as i64;
    format!("{}{}", inches / 12, inches % 12)
}

/// A JSON value as words: `"double_hung"` → "double hung".
fn words(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.replace('_', " ")),
        serde_json::Value::Object(m) => {
            let parts: Vec<String> = m.values().filter_map(words).collect();
            Some(parts.join(" "))
        }
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn object_value(doc: &Document, id: housecraft_model::Id, name: &str) -> Option<String> {
    use housecraft_model::{Cabinet, Layer, Object};
    let object = doc.object(id)?;
    let json = serde_json::to_value(object).ok()?;
    let field = |k: &str| json.get(k).cloned();
    let length = |k: &str| {
        field(k)?
            .as_f64()
            .map(|mm| housecraft_units::Length::from_mm(mm).format(doc.settings.display_unit))
    };
    let info = doc.settings.object_info.get(&id);
    let own = |s: Option<&String>| s.filter(|v| !v.is_empty()).cloned();
    if let Some(v) = info.and_then(|i| match name {
        "code" => own(Some(&i.code)),
        "description" => own(Some(&i.description)),
        "manufacturer" => own(Some(&i.manufacturer)),
        "comment" => own(Some(&i.comment)),
        other => own(i.custom.get(other)),
    }) {
        return Some(v);
    }
    Some(match name {
        "width" | "height" | "depth" | "sill" => length(name)?,
        "size" => format!("{}{}", size_digits(field("width")?.as_f64()?), size_digits(field("height")?.as_f64()?)),
        "kind" => object.kind().name().to_string(),
        "type" => ["opening_kind", "cabinet_kind", "appliance_kind", "style"]
            .iter()
            .find_map(|k| field(k).as_ref().and_then(words))?,
        "code" => match object {
            Object::Cabinet(c) => Cabinet::code(c),
            _ => return None,
        },
        "layer" => doc.layer_of(id).and_then(|l| doc.get::<Layer>(l)).map(|l| l.name.clone())?,
        other => words(&field(other)?)?,
    })
}

/// Fill object macros in `text` from object `id`, then document macros.
pub fn expand_object(doc: &Document, id: housecraft_model::Id, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}').map(|close| (&after[..close], close)) {
            Some((name, close)) if !name.starts_with('#') && !name.starts_with('/') => {
                match object_value(doc, id, name) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&rest[open..open + close + 2]),
                }
                rest = &after[close + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    expand(doc, &out)
}

/// Today's date (UTC) as YYYY-MM-DD.
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 to (year, month, day) — Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn value(doc: &Document, name: &str) -> Option<String> {
    Some(match name {
        "project" => doc.name.clone(),
        "date" => today(),
        "year" => today()[..4].to_string(),
        "living_area" => housecraft_api::query::summary(doc).living_area,
        "floors" => doc
            .floors_by_elevation()
            .iter()
            .filter(|f| f.foundation.is_none())
            .count()
            .to_string(),
        "rooms" => doc.iter::<housecraft_model::Room>().count().to_string(),
        "revision" => doc.settings.revisions.last().map_or("0".into(), |r| r.number.to_string()),
        "revision_date" => doc.settings.revisions.last().map_or(String::new(), |r| r.date.clone()),
        other => doc.settings.project.get(other)?.clone(),
    })
}

/// Fill page macros (`{sheet}`, `{page}`…) for page `page` of `layout`,
/// then the document's macros.
pub fn expand_page(doc: &Document, layout: &housecraft_model::Layout, page: usize, text: &str) -> String {
    let page_value = |name: &str| -> Option<String> {
        Some(match name {
            "sheet" => layout.pages.get(page)?.clone(),
            "page" => (page + 1).to_string(),
            "page_count" => layout.pages.len().to_string(),
            "page_title" => layout.title(page).to_string(),
            "layout" => layout.name.clone(),
            _ => return None,
        })
    };
    expand_with(doc, text, &page_value)
}

/// Replace every known `{macro}` in `text`; unknown ones are left as typed.
pub fn expand(doc: &Document, text: &str) -> String {
    expand_with(doc, text, &|_| None)
}

fn expand_with(doc: &Document, text: &str, extra: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match extra(name.trim()).or_else(|| value(doc, name.trim())) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_expansion() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_744), (2026, 10, 18));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        let doc = Document::new("Smith Residence");
        assert_eq!(
            expand(&doc, "{project} - {unknown} {"),
            "Smith Residence - {unknown} {"
        );
        assert_eq!(expand(&doc, "rooms: {rooms}"), "rooms: 0");
        assert_eq!(today().len(), 10);
    }
}
