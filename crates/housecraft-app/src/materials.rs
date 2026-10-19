//! Plan Materials: browse, search, edit, copy, replace and merge the plan's
//! materials, and choose the default material for each kind of surface.

use egui::{Color32, Context, Ui};
use housecraft_api::model::*;
use housecraft_api::{Command, PaintScope};

use crate::app::App;

#[derive(Default)]
pub struct MaterialsDialog {
    pub open: bool,
    search: String,
    selected: Option<Id>,
    /// Image files waiting to become materials (path, category).
    imports: Vec<(std::path::PathBuf, String)>,
    to_library: Option<Id>,
    /// The plan selection when the dialog was drawn.
    selection: Vec<Id>,
    style_cabinets: bool,
    replace_with: Option<Id>,
    merge: bool,
}

pub fn swatch(ui: &mut Ui, color: Color, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let [r, g, b, _] = color.0;
    ui.painter()
        .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
    ui.painter().rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
        egui::StrokeKind::Inside,
    );
    resp
}

/// How often a material is used: painted surfaces and default slots.
fn uses(doc: &Document, id: Id) -> usize {
    doc.iter::<MaterialAssignment>()
        .filter(|a| a.material == id)
        .count()
        + doc
            .iter::<DefaultSet>()
            .map(|d| d.materials.values().filter(|m| **m == id).count())
            .sum::<usize>()
}

/// Case-insensitive match of every word in `query` against the material's
/// name, category or type.
pub fn matches(m: &Material, query: &str) -> bool {
    let hay = format!("{} {} {}", m.name, m.category, m.class.name()).to_lowercase();
    query
        .split_whitespace()
        .all(|w| hay.contains(&w.to_lowercase()))
}

/// Pick a material from a combo box.
pub fn material_combo(
    ui: &mut Ui,
    doc: &Document,
    salt: impl std::hash::Hash + std::fmt::Debug,
    current: Option<Id>,
    none_label: &str,
) -> Option<Option<Id>> {
    let name = current
        .and_then(|c| doc.get::<Material>(c))
        .map_or(none_label.to_string(), |m| m.name.clone());
    let mut chosen = None;
    egui::ComboBox::from_id_salt(salt)
        .selected_text(name)
        .width(200.0)
        .height(400.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), none_label).clicked() && current.is_some() {
                chosen = Some(None);
            }
            let mut ms: Vec<&Material> = doc.iter::<Material>().collect();
            ms.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));
            for m in ms {
                ui.horizontal(|ui| {
                    swatch(ui, m.color, 12.0);
                    if ui
                        .selectable_label(current == Some(m.id), &m.name)
                        .clicked()
                        && current != Some(m.id)
                    {
                        chosen = Some(Some(m.id));
                    }
                });
            }
        });
    chosen
}

pub fn materials_dialog(app: &mut App, ctx: &Context) {
    app.materials.selection = app.selection.clone();
    // Textures picked last frame become materials, one command each.
    for (path, category) in std::mem::take(&mut app.materials.imports) {
        let name = path.file_stem().map_or("Texture".into(), |s| s.to_string_lossy().into_owned());
        let color = crate::images::average_color(&path.to_string_lossy());
        if let Some(out) = app.run1(Command::AddMaterial {
            name,
            category: Some(category),
            class: None,
            color,
            copy_from: None,
        }) && let Some(id) = out.created.first()
        {
            app.run1(Command::Update {
                id: *id,
                set: serde_json::json!({
                    "maps": { "color": { "path": path.to_string_lossy(), "strength": 1.0 } },
                    "texture_width": housecraft_api::units::Length::from_feet(4.0),
                    "texture_height": housecraft_api::units::Length::from_feet(4.0),
                })
                .as_object()
                .expect("object")
                .clone(),
            });
        }
    }
    if let Some(id) = app.materials.to_library.take()
        && let (Some(dir), Some(item)) = (
            housecraft_io_native::library::library_dir(),
            housecraft_io_native::library::item_from_object(app.doc(), id),
        )
    {
        match housecraft_io_native::library::save_item(&dir, &item) {
            Ok(_) => {
                app.library.refresh();
                app.info(format!("Added {} to your library", item.name));
            }
            Err(e) => app.error(format!("Could not save: {e}")),
        }
    }
    let mut open = app.materials.open;
    let mut commands = Vec::new();
    let mut edit = None;
    egui::Window::new("Plan Materials")
        .open(&mut open)
        .default_width(560.0)
        .default_height(520.0)
        .show(ctx, |ui| {
            let doc = app.session.document();
            let st = &mut app.materials;
            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.text_edit_singleline(&mut st.search)
                    .on_hover_text("Words match name, category or type");
                if ui
                    .button("Import Textures…")
                    .on_hover_text("Make a material from every image in a folder")
                    .clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    let category = dir.file_name().map_or("Textures".into(), |f| f.to_string_lossy().into_owned());
                    let mut files: Vec<_> = std::fs::read_dir(&dir)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            p.extension().is_some_and(|e| {
                                matches!(e.to_string_lossy().to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff")
                            })
                        })
                        .collect();
                    files.sort();
                    st.imports = files.into_iter().map(|f| (f, category.clone())).collect();
                }
                if ui.button("New Material").clicked() {
                    commands.push(Command::AddMaterial {
                        name: "New Material".into(),
                        category: Some("Custom".into()),
                        class: None,
                        color: None,
                        copy_from: None,
                    });
                }
            });
            ui.separator();
            let mut ms: Vec<&Material> = doc
                .iter::<Material>()
                .filter(|m| matches(m, &st.search))
                .collect();
            ms.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .id_salt("materials_list")
                .show(ui, |ui| {
                    egui::Grid::new("materials_grid")
                        .num_columns(5)
                        .striped(true)
                        .show(ui, |ui| {
                            for m in &ms {
                                swatch(ui, m.color, 16.0);
                                if ui
                                    .selectable_label(st.selected == Some(m.id), &m.name)
                                    .clicked()
                                {
                                    st.selected = Some(m.id);
                                }
                                ui.weak(&m.category);
                                ui.weak(m.class.name());
                                let n = uses(doc, m.id);
                                ui.weak(if n == 0 { "unused".into() } else { format!("{n} use{}", if n == 1 { "" } else { "s" }) });
                                ui.end_row();
                            }
                        });
                    if ms.is_empty() {
                        ui.weak("No materials match.");
                    }
                });
            ui.separator();
            if let Some(sel) = st.selected.and_then(|s| doc.get::<Material>(s)) {
                let sel_id = sel.id;
                ui.horizontal(|ui| {
                    swatch(ui, sel.color, 16.0);
                    ui.strong(&sel.name);
                    if ui.button("Edit…").clicked() {
                        edit = Some(sel_id);
                    }
                    if ui.button("Copy").clicked() {
                        commands.push(Command::AddMaterial {
                            name: format!("{} copy", sel.name),
                            category: None,
                            class: None,
                            color: None,
                            copy_from: Some(sel_id),
                        });
                    }
                    if ui.button("Delete").clicked() {
                        commands.push(Command::Delete { ids: vec![sel_id] });
                        st.selected = None;
                    }
                    if ui
                        .button("Add to Library")
                        .on_hover_text("Save this material (with its pattern) in your library")
                        .clicked()
                    {
                        st.to_library = Some(sel_id);
                    }
                    if ui
                        .add_enabled(!st.selection.is_empty(), egui::Button::new("Copy for Selection"))
                        .on_hover_text("Change it for the selected objects only: they get their own copy")
                        .clicked()
                    {
                        commands.push(Command::CopyMaterialFor { material: sel_id, ids: st.selection.clone() });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Replace with:");
                    if let Some(c) = material_combo(ui, doc, "replace_with", st.replace_with, "(choose)") {
                        st.replace_with = c;
                    }
                    ui.checkbox(&mut st.merge, "and remove it (merge)");
                    let ok = st.replace_with.is_some_and(|r| r != sel_id);
                    if ui.add_enabled(ok, egui::Button::new("Replace")).clicked()
                        && let Some(to) = st.replace_with
                    {
                        commands.push(Command::ReplaceMaterial {
                            from: sel_id,
                            to,
                            delete: st.merge,
                        });
                        if st.merge {
                            st.selected = Some(to);
                        }
                    }
                });
            } else {
                ui.weak("Select a material to edit, copy, replace or delete it.");
            }
            ui.separator();
            egui::CollapsingHeader::new("Style palettes")
                .default_open(false)
                .show(ui, |ui| {
                    ui.weak("A style is a saved set of finishes: surface materials, door style and hardware.");
                    ui.horizontal(|ui| {
                        if ui.button("Save current as style").clicked() {
                            let n = doc.count::<StylePalette>() + 1;
                            commands.push(Command::SaveStyle { name: format!("Style {n}") });
                        }
                        ui.checkbox(&mut st.style_cabinets, "Also restyle existing cabinets");
                    });
                    for p in doc.iter::<StylePalette>() {
                        ui.horizontal(|ui| {
                            ui.label(&p.name);
                            if ui.small_button("Apply").clicked() {
                                commands.push(Command::ApplyStyle { style: p.id, cabinets: st.style_cabinets });
                            }
                            if ui.small_button("Rename…").clicked() {
                                edit = Some(p.id);
                            }
                        });
                    }
                });
            egui::CollapsingHeader::new("Default material for each surface")
                .default_open(false)
                .show(ui, |ui| {
                    ui.weak("Used wherever nothing has been painted (the active default set).");
                    let defaults = doc.active_defaults().materials.clone();
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .id_salt("surface_defaults")
                        .show(ui, |ui| {
                            egui::Grid::new("surface_defaults_grid")
                                .num_columns(2)
                                .show(ui, |ui| {
                                    for s in Surface::ALL {
                                        ui.label(s.name());
                                        if let Some(c) = material_combo(
                                            ui,
                                            doc,
                                            ("surface_default", s),
                                            defaults.get(&s).copied(),
                                            "Built-in color",
                                        ) {
                                            commands.push(Command::PaintMaterial {
                                                material: c,
                                                scope: PaintScope::Plan,
                                                ids: Vec::new(),
                                                surface: Some(s),
                                            });
                                        }
                                        ui.end_row();
                                    }
                                });
                        });
                });
        });
    app.materials.open = open;
    if let Some(id) = edit
        && !app.spec.contains(&id)
    {
        app.spec.push(id);
    }
    if !commands.is_empty()
        && let Some(out) = app.run(commands)
        && let Some(new) = out
            .created
            .iter()
            .find(|id| app.doc().kind_of(**id) == Some(ObjectKind::Material))
    {
        app.materials.selected = Some(*new);
    }
}
