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
    /// A group of objects being placed, and its corner.
    pub group: Option<(Vec<Value>, [f64; 2])>,
    /// A door or window template and the tool that places it.
    pub opening: Option<(Tool, Value)>,
    filters: Option<Vec<library::SavedFilter>>,
    shortcuts: Option<Vec<String>>,
    /// A shortcut's folder shown on its own.
    folder: Option<String>,
    favorites: Option<std::collections::BTreeSet<String>>,
    /// The item being edited and its fields as text.
    editing: Option<ItemEdit>,
}

struct ItemEdit {
    item: LibraryItem,
    name: String,
    category: String,
    tags: String,
    data: String,
    error: Option<String>,
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

    fn filters(&mut self) -> &mut Vec<library::SavedFilter> {
        self.filters
            .get_or_insert_with(|| library::library_dir().map(|d| library::load_filters(&d)).unwrap_or_default())
    }

    fn shortcuts(&mut self) -> &mut Vec<String> {
        self.shortcuts.get_or_insert_with(|| library::library_dir().map(|d| library::load_shortcuts(&d)).unwrap_or_default())
    }

    fn save_shortcuts(&mut self) {
        let list = self.shortcuts().clone();
        if let Some(dir) = library::library_dir() {
            let _ = library::save_shortcuts(&dir, &list);
        }
    }

    fn favorites(&mut self) -> &mut std::collections::BTreeSet<String> {
        self.favorites
            .get_or_insert_with(|| library::library_dir().map(|d| library::load_favorites(&d)).unwrap_or_default())
    }
}

/// A group item's objects ready for this plan (on this plan's layers for
/// their kind), and the group's corner in mm.
fn group_objects(app: &App, item: &LibraryItem) -> (Vec<Value>, [f64; 2]) {
    let d = app.doc().active_defaults();
    let objects = item
        .data
        .get("objects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut o| {
            if let Some(m) = o.as_object_mut()
                && m.contains_key("layer")
            {
                let layer = match m.get("kind").and_then(Value::as_str) {
                    Some("stair") => d.layers.stairs.unwrap_or(d.layers.walls),
                    Some("solid") => d.layers.cad,
                Some("text" | "callout" | "schedule") => d.layers.text,
                    _ => d.layers.cabinets.unwrap_or(d.layers.cad),
                };
                m.insert("layer".into(), json!(layer));
            }
            o
        })
        .collect();
    let origin = item
        .data
        .get("origin")
        .and_then(|v| serde_json::from_value::<[housecraft_api::units::Length; 2]>(v.clone()).ok())
        .map_or([0.0, 0.0], |[x, y]| [x.mm(), y.mm()]);
    (objects, origin)
}

/// The plan's copy of a library symbol (added if it isn't there yet).
fn ensure_symbol(app: &mut App, item: &LibraryItem) -> Option<Id> {
    let mut v = item.data.clone();
    let m = v.as_object_mut()?;
    m.insert("id".into(), json!(0));
    let def: SymbolDef = serde_json::from_value(v).ok()?;
    if let Some(same) = app.doc().iter::<SymbolDef>().find(|d| {
        d.name == def.name && d.positions == def.positions && d.triangles == def.triangles
    }) {
        return Some(same.id);
    }
    app.run1(Command::AddSymbolDef {
        name: def.name,
        positions: def.positions,
        triangles: def.triangles,
        groups: def.groups,
        behavior: def.behavior,
    })
    .and_then(|o| o.created.first().copied())
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
        LibraryKind::Molding => doc.iter::<MoldingProfile>().find(|m| m.name == item.name).map(|m| m.id),
        LibraryKind::WallType => doc.iter::<WallType>().find(|w| w.name == item.name).map(|w| w.id),
        _ => None,
    };
    if existing.is_some() {
        return existing;
    }
    let kind = match item.kind {
        LibraryKind::Material => "material",
        LibraryKind::WallType => "wall_type",
        LibraryKind::Molding => "molding_profile",
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
        LibraryKind::Molding => {
            if let Some(id) = ensure_in_plan(app, item) {
                app.spec.push(id);
                app.info(format!("{} is in the plan; use it in a molding's profile", item.name));
            }
        }
        LibraryKind::Opening => {
            let door = item.data.get("opening_kind").and_then(|k| k.get("type")) == Some(&json!("door"));
            let tool = serde_json::from_value::<OpeningKind>(item.data.get("opening_kind").cloned().unwrap_or_default())
                .map(|k| match k {
                    OpeningKind::Door(s) => Tool::Door(s),
                    OpeningKind::Window(s) => Tool::Window(s),
                })
                .unwrap_or(if door { Tool::Door(DoorStyle::Hinged) } else { Tool::Window(WindowStyle::DoubleHung) });
            app.set_tool(tool);
            app.library.opening = Some((tool, item.data.clone()));
            app.info(format!("Click on a wall to place {}", item.name));
        }
        LibraryKind::Symbol => {
            if let Some(def) = ensure_symbol(app, item) {
                app.set_tool(Tool::PlaceSymbol(def));
                app.info(format!("Click in the plan to place {}", item.name));
            }
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
        LibraryKind::HatchPattern => {
            let Ok(p) = serde_json::from_value::<HatchPattern>(item.data.clone()) else { return };
            if app.doc().settings.hatch_patterns.iter().any(|h| h.name == p.name) {
                app.info(format!("{} is already in the plan's fill patterns", p.name));
            } else if app.run1(Command::SetHatchPattern { index: None, pattern: p.clone() }).is_some() {
                app.info(format!("{} added: pick it in any fill's pattern list", p.name));
            }
        }
        LibraryKind::FillStyle => {
            let Ok(mut f) = serde_json::from_value::<FillStyle>(item.data.clone()) else { return };
            // A fill naming a user pattern refers to this plan's list; keep plain hatching otherwise.
            if let FillPattern::UserPattern(i) = f.pattern
                && i as usize >= app.doc().settings.hatch_patterns.len()
            {
                f.pattern = FillPattern::Hatch;
            }
            let cads: Vec<Id> = app.selection.iter().copied().filter(|id| app.doc().kind_of(*id) == Some(ObjectKind::Cad)).collect();
            if cads.is_empty() {
                app.error("Select the CAD shapes to fill first.");
                return;
            }
            let cmds: Vec<Command> = cads
                .iter()
                .map(|id| Command::Update { id: *id, set: json!({"fill_style": f}).as_object().cloned().unwrap_or_default() })
                .collect();
            if app.run(cmds).is_some() {
                app.info(format!("Filled {} shape(s) with {}", cads.len(), item.name));
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

/// Swap the selected block references (or all like them) to this block.
fn replace_blocks(app: &mut App, item: &LibraryItem, all_identical: bool) {
    let Ok(b) = serde_json::from_value::<library::LibraryBlock>(item.data.clone()) else { return };
    let doc = app.doc();
    let picked: Vec<&BlockRef> = app.selection.iter().filter_map(|id| doc.get::<BlockRef>(*id)).collect();
    if picked.is_empty() {
        app.error("Select the placed blocks to replace first.");
        return;
    }
    let ids: Vec<Id> = if all_identical {
        let defs: Vec<Id> = picked.iter().map(|r| r.block).collect();
        doc.iter::<BlockRef>().filter(|r| defs.contains(&r.block)).map(|r| r.id).collect()
    } else {
        picked.iter().map(|r| r.id).collect()
    };
    let Some(def) = app.ensure_library_block(&b) else { return };
    let cmds: Vec<Command> = ids.iter().map(|id| Command::Update { id: *id, set: json!({"block": def.0}).as_object().cloned().unwrap_or_default() }).collect();
    if app.run(cmds).is_some() {
        app.info(format!("Replaced {} block(s) with {}", ids.len(), item.name));
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
    let key = library::favorite_key(item);
    let favorite = app.library.favorites().contains(&key);
    let resp = ui
        .horizontal(|ui| {
            let r = ui.selectable_label(false, &item.name);
            if show_category {
                ui.weak(&item.category);
            }
            if item.is_user() {
                ui.weak("·mine");
            }
            if favorite {
                ui.weak("★");
            }
            r
        })
        .inner;
    let hint = match item.kind {
        LibraryKind::Cabinet
        | LibraryKind::Appliance
        | LibraryKind::Block
        | LibraryKind::Group
        | LibraryKind::Symbol => "Click, then click in the plan to place it",
        LibraryKind::Opening => "Click, then click on a wall to place it",
        LibraryKind::Molding => "Click to add the profile to the plan",
        LibraryKind::Material => "Click to paint with it in 3D",
        LibraryKind::WallType => "Click to add it to the plan",
        LibraryKind::HatchPattern => "Click to add the pattern to the plan's fill patterns",
        LibraryKind::FillStyle => "Click to fill the selected CAD shapes with it",
    };
    let resp = resp.on_hover_text(hint);
    if resp.clicked() {
        activate(app, item);
    }
    resp.context_menu(|ui| {
        if item.kind == LibraryKind::Block {
            if ui.button("Replace Selected Blocks").clicked() {
                replace_blocks(app, item, false);
                ui.close();
            }
            if ui.button("Replace All Like Selected").clicked() {
                replace_blocks(app, item, true);
                ui.close();
            }
        }
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
        let fav_label = if favorite { "Remove from Favorites" } else { "Add to Favorites" };
        if ui.button(fav_label).clicked() {
            let favs = app.library.favorites();
            if favorite {
                favs.remove(&key);
            } else {
                favs.insert(key.clone());
            }
            let favs = favs.clone();
            if let Some(dir) = library::library_dir()
                && let Err(e) = library::save_favorites(&dir, &favs)
            {
                app.error(format!("Could not save favorites: {e}"));
            }
            ui.close();
        }
        if ui
            .button(if item.is_user() { "Edit…" } else { "Edit a Copy…" })
            .on_hover_text("Change its name, folder, tags and properties")
            .clicked()
        {
            app.library.editing = Some(ItemEdit {
                item: item.clone(),
                name: item.name.clone(),
                category: if item.is_user() { item.category.clone() } else { format!("My Library/{}", item.category) },
                tags: item.tags.join(", "),
                data: serde_json::to_string_pretty(&item.data).unwrap_or_default(),
                error: None,
            });
            ui.close();
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

/// The window editing a library item.
fn edit_window(app: &mut App, ui: &mut Ui) {
    let Some(mut e) = app.library.editing.take() else {
        return;
    };
    let mut open = true;
    let mut save = false;
    egui::Window::new(format!("Edit Library Item: {}", e.item.name))
        .open(&mut open)
        .default_width(420.0)
        .show(ui.ctx(), |ui| {
            egui::Grid::new("lib_edit").num_columns(2).show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut e.name);
                ui.end_row();
                ui.label("Folder");
                ui.text_edit_singleline(&mut e.category);
                ui.end_row();
                ui.label("Tags");
                ui.text_edit_singleline(&mut e.tags).on_hover_text("Separate tags with commas");
                ui.end_row();
            });
            if e.item.kind == LibraryKind::Symbol
                && let Ok(mut v) = serde_json::from_str::<Value>(&e.data)
            {
                let b: SymbolBehavior = v
                    .get("behavior")
                    .and_then(|b| serde_json::from_value(b.clone()).ok())
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.label("Behaves as");
                    for choice in [
                        SymbolBehavior::Plain,
                        SymbolBehavior::Sink,
                        SymbolBehavior::WallHung { height: housecraft_api::units::Length::from_inches(48.0) },
                        SymbolBehavior::light(),
                    ] {
                        let same = std::mem::discriminant(&choice) == std::mem::discriminant(&b);
                        if ui.selectable_label(same, choice.name()).clicked() && !same
                            && let Some(m) = v.as_object_mut()
                        {
                            m.insert("behavior".into(), json!(choice));
                            e.data = serde_json::to_string_pretty(&v).unwrap_or_default();
                        }
                    }
                });
            }
            ui.label("Properties");
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut e.data).code_editor().desired_width(f32::INFINITY));
            });
            if let Some(err) = &e.error {
                ui.colored_label(ui.visuals().error_fg_color, err);
            }
            save = ui.button("Save").clicked();
        });
    if save {
        match serde_json::from_str::<Value>(&e.data) {
            Ok(data) => {
                let new = LibraryItem {
                    name: e.name.trim().to_string(),
                    category: e.category.trim().trim_matches('/').to_string(),
                    kind: e.item.kind,
                    tags: e.tags.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
                    data,
                    source: Default::default(),
                };
                match library::library_dir().map(|d| library::update_item(&d, &e.item, &new)) {
                    Some(Ok(_)) => {
                        app.library.refresh();
                        app.info(format!("Saved {}", new.name));
                        return;
                    }
                    Some(Err(err)) => e.error = Some(format!("Could not save: {err}")),
                    None => e.error = Some("No library folder".into()),
                }
            }
            Err(err) => e.error = Some(format!("Properties aren't valid: {err}")),
        }
    }
    if open {
        app.library.editing = Some(e);
    }
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
        let r = egui::CollapsingHeader::new(name)
            .id_salt(("lib", &f))
            .default_open(false)
            .show(ui, |ui| tree(app, ui, items, &f));
        r.header_response.context_menu(|ui| {
            let has = app.library.shortcuts().contains(&f);
            if !has && ui.button("Add Shortcut").on_hover_text("A button at the top of the library opens this folder").clicked() {
                app.library.shortcuts().push(f.clone());
                app.library.save_shortcuts();
                ui.close();
            }
            if has && ui.button("Remove Shortcut").clicked() {
                app.library.shortcuts().retain(|s| *s != f);
                app.library.save_shortcuts();
                ui.close();
            }
        });
    }
    if !prefix.is_empty() {
        for i in items.iter().filter(|i| i.category == prefix) {
            item_row(app, ui, i, false);
        }
    }
}

pub fn show(app: &mut App, ui: &mut Ui) {
    edit_window(app, ui);
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
            .on_hover_text(
                "Save the selected cabinets, appliances, symbols, stairs, solids, blocks, materials or wall types in your library",
            )
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
        if ui
            .button("Import 3D Models…")
            .on_hover_text("Add every OBJ and STL model in a folder to your library as symbols")
            .clicked()
            && let Some(folder) = rfd::FileDialog::new().pick_folder()
            && let Some(dir) = library::library_dir()
        {
            let name = folder.file_name().map_or("Models".into(), |f| f.to_string_lossy().into_owned());
            let (items, failed) = library::import_models(&folder, &format!("My Library/Imported/{name}"));
            let saved = items.iter().filter(|i| library::save_item(&dir, i).is_ok()).count();
            app.library.refresh();
            if failed.is_empty() {
                app.info(format!("Added {saved} models to your library"));
            } else {
                app.error(format!(
                    "Added {saved} models; {} could not be read (first: {})",
                    failed.len(),
                    failed[0].1
                ));
            }
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
    // Saved searches.
    ui.horizontal_wrapped(|ui| {
        let search = app.library.search.trim().to_string();
        if ui
            .add_enabled(!search.is_empty(), egui::Button::new("Save Search").small())
            .on_hover_text("Keep this search as a button")
            .clicked()
        {
            let filters = app.library.filters();
            filters.retain(|f| f.query != search);
            filters.push(library::SavedFilter { name: search.clone(), query: search.clone() });
            let filters = filters.clone();
            if let Some(dir) = library::library_dir() {
                let _ = library::save_filters(&dir, &filters);
            }
        }
        if ui
            .add_enabled(!app.selection.is_empty(), egui::Button::new("Find Selected").small())
            .on_hover_text("Search the library for the selected object")
            .clicked()
            && let Some(item) = app.selection.iter().find_map(|id| library::item_from_object(app.doc(), *id))
        {
            app.library.search = format!("\"{}\"", item.name);
        }
        let mut remove = None;
        for (i, f) in app.library.filters().clone().into_iter().enumerate() {
            let r = ui.small_button(&f.name).on_hover_text("Right-click to remove");
            if r.clicked() {
                app.library.search = f.query.clone();
            }
            if r.secondary_clicked() {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            let filters = app.library.filters();
            filters.remove(i);
            let filters = filters.clone();
            if let Some(dir) = library::library_dir() {
                let _ = library::save_filters(&dir, &filters);
            }
        }
    });
    if let Some((_, _, name)) = &app.library.placing
        && app.plan.tool == Tool::PlaceTemplate
    {
        ui.label(format!("Placing: {name} (Esc to stop)"));
    }
    // Folder shortcuts.
    let shortcuts = app.library.shortcuts().clone();
    if !shortcuts.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.weak("Shortcuts:");
            for f in &shortcuts {
                let name = f.rsplit('/').next().unwrap_or(f);
                let on = app.library.folder.as_deref() == Some(f.as_str());
                if ui.selectable_label(on, name).on_hover_text(f.as_str()).clicked() {
                    app.library.folder = if on { None } else { Some(f.clone()) };
                }
            }
        });
    }
    ui.separator();
    let search = app.library.search.clone();
    let items = app.library.items().to_vec();
    egui::ScrollArea::vertical().id_salt("library_items").show(ui, |ui| {
        if let Some(folder) = app.library.folder.clone().filter(|_| search.trim().is_empty()) {
            if ui.small_button("← Whole library").clicked() {
                app.library.folder = None;
            }
            ui.strong(&folder);
            tree(app, ui, &items, &folder);
            return;
        }
        if search.trim().is_empty() {
            let favs = app.library.favorites().clone();
            let starred: Vec<&LibraryItem> = items.iter().filter(|i| favs.contains(&library::favorite_key(i))).collect();
            if !starred.is_empty() {
                egui::CollapsingHeader::new("★ Favorites").id_salt("lib_favorites").show(ui, |ui| {
                    for i in starred {
                        item_row(app, ui, i, true);
                    }
                });
            }
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
