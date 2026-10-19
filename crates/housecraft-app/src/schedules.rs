//! The table window: schedules and the materials list as live tables, with
//! Find (select in plan), Open (specification), export and, for the
//! materials list, unit prices and markup.

use egui::Context;
use housecraft_api::Command;
use housecraft_api::materials::{MaterialsOptions, materials, price_key};
use housecraft_api::model::{Id, ScheduleKind};
use housecraft_api::schedule::{ScheduleOptions, ScheduleTable, TableFormat, schedule};

use crate::app::App;

/// What the table window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableSource {
    Schedule(ScheduleKind),
    Materials,
}

impl TableSource {
    fn title(self) -> &'static str {
        match self {
            TableSource::Schedule(k) => k.name(),
            TableSource::Materials => "Materials List",
        }
    }
}

pub fn table_window(app: &mut App, ctx: &Context) {
    let Some(source) = app.table_view else {
        return;
    };
    let mut open = true;
    let mut this_floor = app.table_this_floor;
    let floor = this_floor.then_some(app.plan.floor);
    let doc = app.session.document();
    let (table, list) = match source {
        TableSource::Schedule(kind) => (
            schedule(
                doc,
                kind,
                &ScheduleOptions {
                    floor,
                    ..Default::default()
                },
            ),
            None,
        ),
        TableSource::Materials => {
            let list = materials(
                doc,
                &MaterialsOptions {
                    floor,
                    layer_set: Some(app.materials_layer_set.unwrap_or(app.plan.layer_set)),
                },
            );
            (list.table(), Some(list))
        }
    };
    let mut find = None;
    let mut spec = None;
    let mut set_layer_set = None;
    let mut master: Option<bool> = None;
    let mut export = None;
    let mut commands: Vec<Command> = Vec::new();
    egui::Window::new(source.title())
        .open(&mut open)
        .resizable(true)
        .default_width(680.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut this_floor, "Current floor only");
                if let Some(list) = &list {
                    ui.separator();
                    // Which layers' objects are counted.
                    let current = app.materials_layer_set.unwrap_or(app.plan.layer_set);
                    let sets: Vec<(Id, String)> = doc.iter::<housecraft_api::model::LayerSet>().map(|s| (s.id, s.name.clone())).collect();
                    let mut chosen = current;
                    egui::ComboBox::from_id_salt("materials_layer_set")
                        .selected_text(sets.iter().find(|s| s.0 == current).map_or("?".into(), |s| s.1.clone()))
                        .show_ui(ui, |ui| {
                            for (id, name) in &sets {
                                ui.selectable_value(&mut chosen, *id, name);
                            }
                        })
                        .response
                        .on_hover_text("Count objects on the layers this layer set shows");
                    if chosen != current {
                        set_layer_set = Some(chosen);
                    }
                    ui.separator();
                    if ui.button("Save Master Prices").on_hover_text("Keep this plan's prices as the master price list for every plan").clicked() {
                        master = Some(true);
                    }
                    if ui.button("Load Master Prices").on_hover_text("Fill in prices this plan lacks from the master list").clicked() {
                        master = Some(false);
                    }
                    ui.separator();
                    ui.label("Markup");
                    let mut m = list.markup_percent;
                    let r = ui.add(egui::DragValue::new(&mut m).range(0.0..=500.0).suffix("%"));
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        commands.push(Command::SetMarkup { percent: m });
                    }
                }
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
            if list.is_some() {
                ui.weak(
                    "Quantities come from the layers the current plan view shows. Enter unit prices to total the list.",
                );
            }
            ui.separator();
            if table.rows.is_empty() {
                ui.weak("Nothing to list yet.");
                return;
            }
            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new(("table_grid", source.title()))
                    .striped(true)
                    .num_columns(table.columns.len() + 1)
                    .show(ui, |ui| {
                        for c in &table.columns {
                            ui.strong(c);
                        }
                        ui.label("");
                        ui.end_row();
                        for (i, row) in table.rows.iter().enumerate() {
                            let line = list.as_ref().and_then(|l| l.lines.get(i));
                            for (c, cell) in row.cells.iter().enumerate() {
                                match line {
                                    // Unit price column: editable.
                                    Some(l) if table.columns[c] == "Unit Price" => {
                                        let mut p = l.unit_price;
                                        let r = ui.add(
                                            egui::DragValue::new(&mut p)
                                                .range(0.0..=1e7)
                                                .speed(0.05)
                                                .max_decimals(2),
                                        );
                                        if r.drag_stopped() || (r.changed() && !r.dragged()) {
                                            commands.push(Command::SetMaterialPrice {
                                                item: price_key(&l.category, &l.item),
                                                price: (p > 0.0).then_some(p),
                                            });
                                        }
                                    }
                                    _ => {
                                        ui.label(cell);
                                    }
                                }
                            }
                            if row.ids.is_empty() {
                                ui.label("");
                            } else {
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
                            }
                            ui.end_row();
                        }
                    });
            });
        });
    app.table_this_floor = this_floor;
    if let Some(s) = set_layer_set {
        app.materials_layer_set = Some(s);
    }
    match master {
        Some(true) => {
            let prices = app.session.document().settings.prices.clone();
            match housecraft_io_native::library::save_master_prices(&prices) {
                Ok(path) => app.info(format!("{} prices saved to {}", prices.len(), path.display())),
                Err(e) => app.error(format!("Could not save the master prices: {e}")),
            }
        }
        Some(false) => match housecraft_io_native::library::load_master_prices() {
            Ok(prices) if prices.is_empty() => app.info("The master price list is empty: Save Master Prices first"),
            Ok(prices) => {
                let n = prices.len();
                if app.run1(Command::ImportPrices { prices, overwrite: false }).is_some() {
                    app.info(format!("Loaded the master price list ({n} prices)"));
                }
            }
            Err(e) => app.error(format!("Could not read the master prices: {e}")),
        },
        None => {}
    }
    if !open {
        app.table_view = None;
    }
    for c in commands {
        app.run1(c);
    }
    if let Some(i) = find {
        select(app, &table.rows[i].ids);
    }
    if let Some(i) = spec
        && let Some(id) = table.rows[i].ids.first()
        && !app.spec.contains(id)
    {
        app.spec.push(*id);
    }
    if let Some(f) = export {
        export_table(app, &table, source.title(), f);
    }
}

/// Select `ids` in plan, switching to the floor the first one is on.
pub(crate) fn select(app: &mut App, ids: &[Id]) {
    let sheet = ids
        .first()
        .and_then(|id| app.session.document().floor_of(*id));
    match sheet {
        Some(f) if app.session.document().get::<housecraft_api::model::Floor>(f).is_some() => {
            app.show_plan();
            app.plan.floor = f;
        }
        Some(s) if app.session.document().is_sheet(s) => app.open_sheet(s),
        _ => app.show_plan(),
    }
    app.selection = ids.to_vec();
    app.plan.zoom_to_selection = true;
}

fn export_table(app: &mut App, table: &ScheduleTable, title: &str, f: TableFormat) {
    let ext = match f {
        TableFormat::Csv => "csv",
        TableFormat::Txt => "txt",
        TableFormat::Xml => "xml",
        TableFormat::Html => "html",
    };
    if let Some(path) = crate::app::pick_save_path(&title.replace(' ', "_"), ext) {
        match std::fs::write(&path, table.export(f)) {
            Ok(()) => app.info(format!("Exported {}", path.display())),
            Err(e) => app.status = Some((format!("Export failed: {e}"), true)),
        }
    }
}

/// Place a schedule table on the current floor, top-left at the view centre.
pub fn place_schedule(app: &mut App, kind: ScheduleKind) {
    let at = housecraft_api::geom::pt(app.plan.center);
    if app
        .run1(Command::AddSchedule {
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
