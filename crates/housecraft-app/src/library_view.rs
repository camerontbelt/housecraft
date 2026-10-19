//! The Library Browser: built-in and user library items in a category
//! tree, with search. Clicking an item places it (cabinets, appliances,
//! blocks) or adds it to the plan (materials, wall types).

use egui::Ui;
use housecraft_api::Command;
use housecraft_api::model::*;
use housecraft_io_native::library::{self, LibraryItem, LibraryKind, Query};
use serde_json::{Value, json};

use crate::app::App;
use crate::tools::Tool;

#[derive(Default)]
pub struct LibraryState {
    search: String,
    items: Option<Vec<LibraryItem>>,
    /// The cabinet or appliance template being placed.
    pub placing: Option<(ObjectKind, Value, String)>,
    /// A group of objects being placed (relative to its corner).
    pub group: Option<Vec<Value>>,
}

impl LibraryState {
    fn items(&mut self) -> &[LibraryItem] {
        self.items.get_or_insert_with(|| {
            let mut all = library::builtin();
            if let Some(dir) = library::library_dir() {
                all.extend(library::user_items(&dir));
            }
            all
        })
    }

    /// Re-read the user library after it changes.
    pub fn refresh(&mut self) {
        self.items = None;
    }
}

/// A group item's objects ready for this plan: on its cabinet layer.
fn group_objects(app: &App, item: &LibraryItem) -> Vec<Value> {
    let d = app.doc().active_defaults();
    let layer = d.layers.cabinets.unwrap_or(d.layers.cad);
    item.data
        .get("objects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut o| {
            if let Some(m) = o.as_object_mut()
                && m.contains_key("layer")
            {
                m.insert("layer".into(), json!(layer));
            }
            o
        })
        .collect()
}

fn kind_of(item: &LibraryItem) -> Option<ObjectKind> {
    match item.kind {
        LibraryKind::Cabinet => Some(ObjectKind::Cabinet),
        LibraryKind::Appliance => Some(ObjectKind::Appliance),
        _ => None,
    }
}

/// Add a material or wall type to the plan unless one of that name is
/// there; its id either way.
fn ensure_in_plan(app: &mut App, item: &LibraryItem) -> Option<Id> {
    let doc = app.doc();
    let existing = match item.kind {
        LibraryKind::Material => doc.iter::<Material>().find(|m| m.name == item.name).map(|m| m.id),
        LibraryKind::WallType => doc.iter::<WallType>().find(|w| w.name == item.name).map(|w| w.id),
        _ => None,
    };
    if existing.is_some() {
        return existing;
    }
    let kind = match item.kind {
        LibraryKind::Material => "material",
        LibraryKind::WallType => "wall_type",
        _ => return None,
    };
    let mut v = item.data.clone();
    let m = v.as_object_mut()?;
    m.insert("kind".into(), json!(kind));
    m.insert("id".into(), json!(0));
    app.run1(Command::InsertObjects {
        objects: vec![v],
        floor: None,
        dx: Default::default(),
        dy: Default::default(),
    })
    .and_then(|o| o.created.first().copied())
}

fn activate(app: &mut App, item: &LibraryItem) {
    match item.kind {
        LibraryKind::Cabinet | LibraryKind::Appliance => {
            let kind = kind_of(item).expect("placeable");
            app.library.placing = Some((kind, item.data.clone(), item.name.clone()));
            app.set_tool(Tool::PlaceTemplate);
        }
        LibraryKind::Block => {
            if let Ok(b) = serde_json::from_value::<library::LibraryBlock>(item.data.clone()) {
                app.place_library_block(&b);
            }
        }
        LibraryKind::Group => {
            app.library.group = Some(group_objects(app, item));
            app.set_tool(Tool::PlaceGroup);
        }
        LibraryKind::Material => {
            if let Some(id) = ensure_in_plan(app, item) {
                // Pick it up in the material painter.
                app.mode = crate::app::Mode::ThreeD;
                let mut p = crate::view3d::Painter::new(app.doc());
                p.material = Some(id);
                app.view3d.painter = Some(p);
                app.info(format!("Painting with {}", item.name));
            }
        }
        LibraryKind::WallType => {
            if ensure_in_plan(app, item).is_some() {
                app.info(format!(
                    "{} is in the plan; choose it in a wall's specification or Default Settings",
                    item.name
                ));
            }
        }
    }
}

fn replace(app: &mut App, item: &LibraryItem, all_identical: bool) {
    let ids: Vec<Id> = app
        .selection
        .iter()
        .copied()
        .filter(|id| {
            matches!(
                app.doc().kind_of(*id),
                Some(ObjectKind::Cabinet | ObjectKind::Appliance)
            )
        })
        .collect();
    if ids.is_empty() {
        app.error("Select the cabinets or appliances to replace first.");
        return;
    }
    if let Some(out) = app.run1(Command::ReplaceWith {
        ids,
        template: item.data.clone(),
        all_identical,
    }) {
        app.info(format!("Replaced {} with {}", out.modified.len(), item.name));
    }
}

fn item_row(app: &mut App, ui: &mut Ui, item: &LibraryItem, show_category: bool) {
    let resp = ui
        .horizontal(|ui| {
            let r = ui.selectable_label(false, &item.name);
            if show_category {
                ui.weak(&item.category);
            }
            if item.is_user() {
                ui.weak("★");
            }
            r
        })
        .inner;
    let hint = match item.kind {
        LibraryKind::Cabinet | LibraryKind::Appliance | LibraryKind::Block | LibraryKind::Group => {
            "Click, then click in the plan to place it"
        }
        LibraryKind::Material => "Click to paint with it in 3D",
        LibraryKind::WallType => "Click to add it to the plan",
    };
    let resp = resp.on_hover_text(hint);
    if resp.clicked() {
        activate(app, item);
    }
    resp.context_menu(|ui| {
        if kind_of(item).is_some() {
            if ui.button("Replace Selected").clicked() {
                replace(app, item, false);
                ui.close();
            }
            if ui.button("Replace All Identical").clicked() {
                replace(app, item, true);
                ui.close();
            }
        }
        if item.is_user() && ui.button("Delete from Library").clicked() {
            match library::delete_item(item) {
                Ok(()) => app.library.refresh(),
                Err(e) => app.error(format!("Could not delete: {e}")),
            }
            ui.close();
        }
    });
}

/// Items grouped into nested category folders.
fn tree(app: &mut App, ui: &mut Ui, items: &[LibraryItem], prefix: &str) {
    let depth = if prefix.is_empty() { 0 } else { prefix.matches('/').count() + 1 };
    let mut folders: Vec<String> = Vec::new();
    for i in items {
        let parts: Vec<&str> = i.category.split('/').collect();
        if parts.len() > depth && (prefix.is_empty() || i.category.starts_with(&format!("{prefix}/")) || i.category == prefix) {
            let f = parts[..=depth].join("/");
            if f != prefix && !folders.contains(&f) {
                folders.push(f);
            }
        }
    }
    for f in folders {
        let name = f.rsplit('/').next().unwrap_or(&f).to_string();
        egui::CollapsingHeader::new(name)
            .id_salt(("lib", &f))
            .default_open(false)
            .show(ui, |ui| tree(app, ui, items, &f));
    }
    if !prefix.is_empty() {
        for i in items.iter().filter(|i| i.category == prefix) {
            item_row(app, ui, i, false);
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.add(egui::TextEdit::singleline(&mut app.library.search).desired_width(ui.available_width()))
            .on_hover_text(
                "Words must all match; use OR, NOT or -word, (brackets), \"phrases\",\n\
                 kind:cabinet, category:bath, tag:sink, width>=30 (inches)",
            );
    });
    ui.horizontal(|ui| {
        let mut selected = app
            .selection
            .iter()
            .filter_map(|id| library::item_from_object(app.doc(), *id))
            .collect::<Vec<_>>();
        // Several cabinets and appliances together save as one group.
        if app.selection.len() > 1
            && let Some(g) = library::group_from_objects(app.doc(), &app.selection)
        {
            selected = vec![g];
        }
        if ui
            .add_enabled(!selected.is_empty(), egui::Button::new("Add Selected"))
            .on_hover_text("Save the selected cabinets, appliances, blocks, materials or wall types in your library")
            .clicked()
            && let Some(dir) = library::library_dir()
        {
            let mut saved = 0;
            for item in &selected {
                if library::save_item(&dir, item).is_ok() {
                    saved += 1;
                }
            }
            app.library.refresh();
            app.info(format!("Added {saved} item{} to your library", if saved == 1 { "" } else { "s" }));
        }
        if ui.button("Import…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("HouseCraft library", &["hclib", "json"])
                .pick_file()
            && let Some(dir) = library::library_dir()
        {
            match library::import_bundle(&dir, &path) {
                Ok(n) => {
                    app.library.refresh();
                    app.info(format!("Imported {n} library items"));
                }
                Err(e) => app.error(format!("Import failed: {e}")),
            }
        }
        if ui.button("Export…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("HouseCraft library", &["hclib"])
                .set_file_name("My Library.hclib")
                .save_file()
        {
            let mine: Vec<LibraryItem> = app.library.items().iter().filter(|i| i.is_user()).cloned().collect();
            match library::export_bundle(&path, &mine) {
                Ok(()) => app.info(format!("Exported {} items", mine.len())),
                Err(e) => app.error(format!("Export failed: {e}")),
            }
        }
    });
    if let Some((_, _, name)) = &app.library.placing
        && app.plan.tool == Tool::PlaceTemplate
    {
        ui.label(format!("Placing: {name} (Esc to stop)"));
    }
    ui.separator();
    let search = app.library.search.clone();
    let items = app.library.items().to_vec();
    egui::ScrollArea::vertical().id_salt("library_items").show(ui, |ui| {
        if search.trim().is_empty() {
            tree(app, ui, &items, "");
            return;
        }
        match Query::parse(&search) {
            Ok(q) => {
                let found: Vec<&LibraryItem> = items.iter().filter(|i| q.matches(i)).collect();
                ui.weak(format!("{} found", found.len()));
                for i in found.into_iter().take(500) {
                    item_row(app, ui, i, true);
                }
            }
            Err(e) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
        }
    });
}
