//! Working across plans and over time: another plan file shown faintly
//! under this one (reference display) and the project time tracker.

use std::path::PathBuf;

use egui::{Context, Ui};
use housecraft_api::geom::mesh::Mesh;
use housecraft_api::model::{Document, Id, Phase};
use housecraft_api::{Command, query};

use crate::app::App;

/// Mesh parts of the reference plan carry this id (never picked, always ghosted).
pub const REFERENCE_PLAN: Id = Id(u64::MAX - 100);

/// Another plan file shown for reference: its floor matching the current
/// one, faded in plan views and ghosted in 3D.
pub struct RefPlan {
    pub path: PathBuf,
    pub doc: Document,
    pub show_2d: bool,
    pub show_3d: bool,
    drawing: Option<(usize, housecraft_draft::Drawing)>,
    mesh: Option<Mesh>,
}

impl RefPlan {
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let doc = housecraft_io_native::load_path(&path).map_err(|e| e.to_string())?;
        Ok(RefPlan { path, doc, show_2d: true, show_3d: true, drawing: None, mesh: None })
    }

    /// The plan of the reference's floor `index` (by elevation; its top
    /// floor when it has fewer).
    pub fn drawing(&mut self, index: usize) -> &housecraft_draft::Drawing {
        if self.drawing.as_ref().is_none_or(|(i, _)| *i != index) {
            let floors = self.doc.floors_by_elevation();
            let floor = floors[index.min(floors.len() - 1)].id;
            let view = self.doc.iter::<housecraft_api::model::PlanView>().next();
            let opts = housecraft_draft::PlanOptions {
                floor,
                layer_set: view
                    .map(|v| v.layer_set)
                    .or_else(|| self.doc.iter::<housecraft_api::model::LayerSet>().next().map(|s| s.id))
                    .unwrap_or(Id(0)),
                reference_floor: false,
                references: Vec::new(),
                phase: Phase::All,
                sun_shadows: false,
                default_set: None,
            };
            self.drawing = Some((index, housecraft_draft::plan(&self.doc, &opts)));
        }
        &self.drawing.as_ref().expect("just drawn").1
    }

    /// The plan of floor `index` if already drawn.
    pub fn drawn(&self, index: usize) -> Option<&housecraft_draft::Drawing> {
        self.drawing.as_ref().filter(|(i, _)| *i == index).map(|(_, d)| d)
    }

    /// The reference plan's 3D model.
    pub fn mesh(&mut self) -> &Mesh {
        self.mesh.get_or_insert_with(|| housecraft_api::geom::mesh::build(&self.doc, None))
    }

    pub fn name(&self) -> String {
        self.path.file_stem().map_or_else(|| self.doc.name.clone(), |s| s.to_string_lossy().into_owned())
    }
}

/// View > Reference Plan File: choose, show in plan or 3D, remove.
pub fn reference_menu(app: &mut App, ui: &mut Ui) {
    let mut changed = false;
    if ui.button("Choose Plan File…").clicked() {
        ui.close();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("HouseCraft plan", &[housecraft_io_native::EXTENSION])
            .pick_file()
        {
            match RefPlan::load(path) {
                Ok(r) => {
                    app.info(format!("Showing {} for reference", r.name()));
                    app.reference = Some(r);
                    changed = true;
                }
                Err(e) => app.error(format!("Could not open the reference plan: {e}")),
            }
        }
    }
    if let Some(r) = &mut app.reference {
        ui.label(format!("Showing: {}", r.name())).on_hover_text(r.path.display().to_string());
        ui.checkbox(&mut r.show_2d, "Show in Plan Views");
        changed |= ui.checkbox(&mut r.show_3d, "Show in 3D Views").changed();
        if ui.button("Reload").clicked() {
            match RefPlan::load(r.path.clone()) {
                Ok(n) => *r = RefPlan { show_2d: r.show_2d, show_3d: r.show_3d, ..n },
                Err(e) => app.status = Some((format!("Could not reload: {e}"), true)),
            }
            changed = true;
        }
        if ui.button("Remove Reference").clicked() {
            app.reference = None;
            changed = true;
            ui.close();
        }
    } else {
        ui.weak("Shows another plan faintly under this one.");
    }
    if changed {
        app.view3d.invalidate();
    }
}

/// Idle time stops counting after this many seconds without input.
const IDLE: f64 = 300.0;

/// Counts the time actively spent on the plan for the task being worked on.
#[derive(Default)]
pub struct TimeTracker {
    pub open: bool,
    pub task: String,
    pub note: String,
    running: bool,
    /// Seconds counted and not yet logged.
    active: f64,
    last_frame: f64,
    last_input: f64,
    manual_minutes: f64,
}

impl TimeTracker {
    fn tick(&mut self, ctx: &Context) {
        if !self.running {
            return;
        }
        let (now, input) = ctx.input(|i| (i.time, !i.events.is_empty() || i.pointer.is_moving()));
        let dt = (now - self.last_frame).max(0.0);
        // Count up to the idle limit after the last input.
        self.active += (self.last_input + IDLE - self.last_frame).clamp(0.0, dt);
        if input {
            self.last_input = now;
        }
        self.last_frame = now;
    }

    fn start(&mut self, now: f64) {
        self.running = true;
        self.last_frame = now;
        self.last_input = now;
    }

    /// The time counted so far as a log entry (merged into today's entry
    /// for the same task), leaving the timer running.
    pub fn flush(&mut self) -> Option<Command> {
        let minutes = std::mem::take(&mut self.active) / 60.0;
        (minutes >= 0.1).then(|| Command::LogTime { task: self.task.clone(), minutes, note: self.note.clone(), date: None })
    }

    /// Stop on a plan being closed (its time was flushed on save).
    pub fn reset(&mut self) {
        self.running = false;
        self.active = 0.0;
    }
}

fn hours(minutes: f64) -> String {
    format!("{}:{:02}", (minutes / 60.0).floor() as u64, (minutes % 60.0).floor() as u64)
}

/// Tools > Time Tracker: time spent on this plan by task, with a timer that
/// pauses when you stop working, and a CSV export for billing.
pub fn time_tracker_window(app: &mut App, ctx: &Context) {
    app.time.tick(ctx);
    if !app.time.open {
        return;
    }
    if app.time.running {
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }
    let mut open = true;
    let mut commands = Vec::new();
    egui::Window::new("Time Tracker").open(&mut open).resizable(true).default_width(460.0).show(ctx, |ui| {
        let client = app.doc().settings.project.client.clone();
        ui.label(if client.is_empty() { format!("Plan: {}", app.doc().name) } else { format!("Plan: {}   Client: {client}", app.doc().name) });
        egui::Grid::new("time_task").num_columns(2).show(ui, |ui| {
            ui.label("Task");
            ui.text_edit_singleline(&mut app.time.task).on_hover_text("For example Design, Drafting, Client meeting");
            ui.end_row();
            ui.label("Note");
            ui.text_edit_singleline(&mut app.time.note);
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if app.time.running {
                if ui.button("Stop").clicked() {
                    commands.extend(app.time.flush());
                    app.time.running = false;
                }
                ui.label(format!("Running: {} counted", hours(app.time.active / 60.0)));
            } else if ui.button("Start").clicked() {
                let now = ctx.input(|i| i.time);
                app.time.start(now);
            }
        })
        .response
        .on_hover_text("The timer pauses after 5 minutes without mouse or keyboard input, and is logged when you stop or save.");
        ui.horizontal(|ui| {
            ui.label("Add by hand");
            ui.add(egui::DragValue::new(&mut app.time.manual_minutes).range(0.0..=1440.0).suffix(" min"));
            if ui.add_enabled(app.time.manual_minutes > 0.0, egui::Button::new("Add")).clicked() {
                commands.push(Command::LogTime {
                    task: app.time.task.clone(),
                    minutes: app.time.manual_minutes,
                    note: app.time.note.clone(),
                    date: None,
                });
                app.time.manual_minutes = 0.0;
            }
        });
        ui.separator();
        let log = app.doc().settings.time_log.clone();
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            egui::Grid::new("time_log").striped(true).num_columns(5).show(ui, |ui| {
                for h in ["Date", "Task", "Hours", "Note", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for (i, e) in log.iter().enumerate() {
                    ui.label(&e.date);
                    ui.label(&e.task);
                    ui.label(hours(e.minutes));
                    ui.label(&e.note);
                    if ui.small_button("×").on_hover_text("Delete this entry").clicked() {
                        commands.push(Command::DeleteTime { index: Some(i) });
                    }
                    ui.end_row();
                }
            });
        });
        let totals = query::time_by_task(app.doc());
        if !totals.is_empty() {
            ui.separator();
            for (task, h) in &totals {
                ui.label(format!("{task}: {}", hours(h * 60.0)));
            }
            let total: f64 = totals.iter().map(|t| t.1).sum();
            ui.strong(format!("Total: {}", hours(total * 60.0)));
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(!log.is_empty(), egui::Button::new("Export CSV…")).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name(format!("{} time.csv", app.doc().name))
                    .save_file()
            {
                match std::fs::write(&path, query::time_log_csv(app.doc())) {
                    Ok(()) => app.info(format!("Saved {}", path.display())),
                    Err(e) => app.error(format!("Could not save: {e}")),
                }
            }
            if ui.add_enabled(!log.is_empty(), egui::Button::new("Clear Log")).clicked() {
                commands.push(Command::DeleteTime { index: None });
            }
        });
    });
    app.time.open = open;
    if !commands.is_empty() {
        app.run(commands);
    }
}
