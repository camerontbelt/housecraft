//! The schedule window: a live table of doors, windows, rooms and so on, with
//! Find (select in plan), Open (specification) and export.

use egui::Context;
use housecraft_api::model::ScheduleKind;
use housecraft_api::schedule::{ScheduleOptions, TableFormat, schedule};

use crate::app::App;

pub fn schedule_window(app: &mut App, ctx: &Context) {
    let Some(kind) = app.schedule_view else {
        return;
    };
    let mut open = true;
    let mut this_floor = app.schedule_this_floor;
    let table = schedule(
        app.session.document(),
        kind,
        &ScheduleOptions {
            floor: this_floor.then_some(app.plan.floor),
            columns: Vec::new(),
        },
    );
    let mut find = None;
    let mut spec = None;
    let mut export = None;
    egui::Window::new(kind.name())
        .open(&mut open)
        .resizable(true)
        .default_width(620.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut this_floor, "Current floor only");
                ui.separator();
                ui.label("Export:");
                for (label, f) in [
                    ("CSV", TableFormat::Csv),
                    ("Text", TableFormat::Txt),
                    ("XML", TableFormat::Xml),
                    ("HTML", TableFormat::Html),
                ] {
                    if ui.button(label).clicked() {
                        export = Some(f);
                    }
                }
            });
            ui.separator();
            if table.rows.is_empty() {
                ui.weak("Nothing to list yet.");
                return;
            }
            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new(("schedule_grid", kind as u8))
                    .striped(true)
                    .num_columns(table.columns.len() + 1)
                    .show(ui, |ui| {
                        for c in &table.columns {
                            ui.strong(c);
                        }
                        ui.label("");
                        ui.end_row();
                        for (i, row) in table.rows.iter().enumerate() {
                            for cell in &row.cells {
                                ui.label(cell);
                            }
                            ui.horizontal(|ui| {
                                if ui
                                    .small_button("Find")
                                    .on_hover_text("Select in plan")
                                    .clicked()
                                {
                                    find = Some(i);
                                }
                                if ui
                                    .small_button("Open")
                                    .on_hover_text("Open specification")
                                    .clicked()
                                {
                                    spec = Some(i);
                                }
                            });
                            ui.end_row();
                        }
                    });
            });
        });
    app.schedule_this_floor = this_floor;
    if !open {
        app.schedule_view = None;
    }
    if let Some(i) = find {
        let ids = table.rows[i].ids.clone();
        // Show the floor the first object is on, then frame the selection.
        if let Some(f) = ids
            .first()
            .and_then(|id| app.session.document().floor_of(*id))
        {
            app.plan.floor = f;
        }
        app.selection = ids;
        app.plan.zoom_to_selection = true;
        app.mode = crate::app::Mode::Plan;
    }
    if let Some(i) = spec {
        for id in table.rows[i].ids.iter().take(1) {
            if !app.spec.contains(id) {
                app.spec.push(*id);
            }
        }
    }
    if let Some(f) = export {
        let ext = match f {
            TableFormat::Csv => "csv",
            TableFormat::Txt => "txt",
            TableFormat::Xml => "xml",
            TableFormat::Html => "html",
        };
        let name = kind.name().replace(' ', "_");
        if let Some(path) = crate::app::pick_save_path(&name, ext) {
            match std::fs::write(&path, table.export(f)) {
                Ok(()) => app.info(format!("Exported {}", path.display())),
                Err(e) => app.status = Some((format!("Export failed: {e}"), true)),
            }
        }
    }
}

/// Place a schedule table on the current floor, top-left at the view centre.
pub fn place_schedule(app: &mut App, kind: ScheduleKind) {
    let at = housecraft_api::geom::pt(app.plan.center);
    if app
        .run1(housecraft_api::Command::AddSchedule {
            floor: Some(app.plan.floor),
            schedule_kind: kind,
            position: at,
            columns: Vec::new(),
            all_floors: false,
        })
        .is_some()
    {
        app.info(format!("{} placed", kind.name()));
    }
}
