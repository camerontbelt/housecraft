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
        let resp = ui.add(egui::TextEdit::singleline(buf).id(id).desired_width(width));
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
