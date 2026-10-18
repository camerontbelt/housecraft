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
];

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
        _ => return None,
    })
}

/// Replace every known `{macro}` in `text`; unknown ones are left as typed.
pub fn expand(doc: &Document, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match value(doc, name.trim()) {
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
