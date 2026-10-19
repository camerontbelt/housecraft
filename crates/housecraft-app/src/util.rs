//! Small helpers shared by the UI modules.

use std::collections::HashMap;

use egui::{Color32, Ui};
use housecraft_api::model::{Color, Document};
use housecraft_api::units::{DisplayUnit, Length, LengthUnit};

/// Unit for bare numbers typed by the user: inches for imperial display, mm otherwise.
pub fn input_unit(doc: &Document) -> LengthUnit {
    match doc.settings.display_unit {
        DisplayUnit::FeetInches { .. } | DisplayUnit::Inches { .. } => LengthUnit::Inches,
        DisplayUnit::Millimeters | DisplayUnit::Meters { .. } => LengthUnit::Millimeters,
    }
}

pub fn fmt(doc: &Document, l: Length) -> String {
    l.format(doc.settings.display_unit)
}

pub fn parse(doc: &Document, s: &str) -> Option<Length> {
    Length::parse_expr(s, input_unit(doc)).ok()
}

pub fn to_color32(c: Color) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

pub fn from_color32(c: Color32) -> Color {
    Color(c.to_srgba_unmultiplied())
}

/// Text-edit buffers for fields that commit on Enter or focus loss.
#[derive(Default)]
pub struct EditBuffers(HashMap<egui::Id, String>);

impl EditBuffers {
    /// A single-line field showing `current`; returns the committed text when
    /// the user presses Enter or leaves the field after changing it.
    pub fn text(&mut self, ui: &mut Ui, id: egui::Id, current: &str, width: f32) -> Option<String> {
        let buf = self.0.entry(id).or_insert_with(|| current.to_string());
        // Sized explicitly: inside a grid a text edit's desired width is not kept.
        let resp = ui.add_sized([width, ui.spacing().interact_size.y], egui::TextEdit::singleline(buf).id(id));
        if resp.lost_focus() {
            let value = self.0.remove(&id).unwrap_or_default();
            return (value != current).then_some(value);
        }
        if !resp.has_focus() {
            self.0.remove(&id);
        }
        None
    }

    /// A multi-line field; commits when the user leaves it after changing it.
    pub fn multiline(
        &mut self,
        ui: &mut Ui,
        id: egui::Id,
        current: &str,
        width: f32,
    ) -> Option<String> {
        let buf = self.0.entry(id).or_insert_with(|| current.to_string());
        let resp = ui.add(
            egui::TextEdit::multiline(buf)
                .id(id)
                .desired_rows(4)
                .desired_width(width),
        );
        if resp.lost_focus() {
            let value = self.0.remove(&id).unwrap_or_default();
            return (value != current).then_some(value);
        }
        if !resp.has_focus() {
            self.0.remove(&id);
        }
        None
    }

    /// A length field; accepts arithmetic like `12'6" + 3"`.
    pub fn length(
        &mut self,
        ui: &mut Ui,
        doc: &Document,
        id: egui::Id,
        value: Length,
    ) -> Option<Length> {
        let shown = fmt(doc, value);
        let text = self.text(ui, id, &shown, 110.0)?;
        parse(doc, &text).filter(|l| l.is_finite())
    }

    pub fn number(&mut self, ui: &mut Ui, id: egui::Id, value: f64) -> Option<f64> {
        let shown = format!("{value:.2}");
        self.text(ui, id, &shown, 80.0)?.trim().parse().ok()
    }
}

/// "6/12" style pitch text for an angle in degrees.
pub fn fmt_pitch(deg: f64) -> String {
    let rise = deg.to_radians().tan() * 12.0;
    if (rise - rise.round()).abs() < 0.01 {
        format!("{}/12", rise.round())
    } else {
        format!("{rise:.2}/12")
    }
}

/// Parse a pitch: "6/12", "6" (rise in 12), or "30°" / "30deg" (degrees).
pub fn parse_pitch(s: &str) -> Option<f64> {
    let s = s.trim();
    let deg = if let Some(d) = s.strip_suffix('°').or_else(|| s.strip_suffix("deg")) {
        d.trim().parse::<f64>().ok()?
    } else if let Some((rise, run)) = s.split_once('/') {
        let (rise, run): (f64, f64) = (rise.trim().parse().ok()?, run.trim().parse().ok()?);
        if run <= 0.0 {
            return None;
        }
        (rise / run).atan().to_degrees()
    } else {
        (s.parse::<f64>().ok()? / 12.0).atan().to_degrees()
    };
    (deg > 0.0 && deg < 90.0).then_some(deg)
}
