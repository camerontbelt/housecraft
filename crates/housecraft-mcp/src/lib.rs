//! MCP server exposing the HouseCraft API to LLMs.
//!
//! Every tool is a thin adapter over `housecraft-api`. Edits go through
//! `apply_commands`, whose input schema is generated from [`Command`], so the
//! MCP surface cannot drift from what the GUI and CLI can do.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use base64::Engine;
use housecraft_api::model::{Document, Id, ObjectKind};
use housecraft_api::{Command, Session, query};
use housecraft_draft::{PlanOptions, plan};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

const INSTRUCTIONS: &str = "\
HouseCraft is a residential architectural design tool. One document is open at a time.
Start with new_document or open_document, edit with apply_commands, and save with save_document.
Coordinates are plan coordinates: x points east, y points north. Points are [x, y].
Lengths are numbers in millimetres, or strings with units such as \"12'6\\\"\", \"12'-6 1/2\\\"\", \"30 1/2\\\"\", \"3.5 m\".
Walls are drawn along their centreline; wall-type layers run exterior to interior with the exterior on the
left looking from start to end, so draw exterior walls clockwise around the building.
Rooms are created automatically for every area enclosed by walls; change a room's name or type with
an `update` command. Each apply_commands call is atomic and is one undo step.
Use view_plan to look at the result as an image after making changes.";

type ToolResult = Result<String, String>;

#[derive(Default)]
struct State {
    session: Option<Session>,
    path: Option<PathBuf>,
}

#[derive(Clone)]
pub struct HouseCraftServer {
    state: Arc<Mutex<State>>,
    tool_router: ToolRouter<Self>,
}

impl Default for HouseCraftServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewDocumentArgs {
    /// Name of the new plan.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PathArgs {
    /// Path to a .housecraft file.
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveArgs {
    /// Where to save. Defaults to the path the document was opened from or last saved to.
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListObjectsArgs {
    /// Only list objects of this kind.
    pub kind: Option<ObjectKind>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetObjectArgs {
    pub id: Id,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApplyArgs {
    /// Commands applied in order as one atomic transaction.
    pub commands: Vec<Command>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ViewPlanArgs {
    /// Floor name or id (default: lowest floor).
    pub floor: Option<String>,
    /// Layer set name or id (default: the first).
    pub layer_set: Option<String>,
    /// Longest side of the image in pixels (default 1200).
    pub pixels: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportPlanArgs {
    /// Output file; .svg, .pdf or .png chooses the format.
    pub path: String,
    pub floor: Option<String>,
    pub layer_set: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScheduleArgs {
    /// doors, windows, rooms, walls, fixtures, stairs or roof_planes.
    pub kind: housecraft_api::model::ScheduleKind,
    /// Only this floor (id); every floor when omitted.
    pub floor: Option<Id>,
    /// Columns to include, in order; all when omitted.
    #[serde(default)]
    pub columns: Vec<String>,
    /// csv, txt, xml or html (default txt).
    pub format: Option<housecraft_api::schedule::TableFormat>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MaterialsArgs {
    /// Only this floor (id); every floor when omitted.
    pub floor: Option<Id>,
    /// csv, txt, xml or html (default txt).
    pub format: Option<housecraft_api::schedule::TableFormat>,
}

#[derive(Serialize)]
struct Status<'a> {
    message: &'a str,
    path: Option<String>,
    can_undo: bool,
    can_redo: bool,
}

fn json(value: &impl Serialize) -> ToolResult {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

impl HouseCraftServer {
    fn with_session<T>(
        &self,
        f: impl FnOnce(&mut Session, &mut Option<PathBuf>) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "server state poisoned".to_string())?;
        let State { session, path } = &mut *state;
        match session {
            Some(s) => f(s, path),
            None => Err("no document is open; call new_document or open_document first".into()),
        }
    }

    fn replace(&self, session: Session, path: Option<PathBuf>) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "server state poisoned".to_string())?;
        *state = State {
            session: Some(session),
            path,
        };
        Ok(())
    }

    fn status(&self, message: &str) -> ToolResult {
        self.with_session(|s, path| {
            json(&Status {
                message,
                path: path.as_ref().map(|p| p.display().to_string()),
                can_undo: s.can_undo(),
                can_redo: s.can_redo(),
            })
        })
    }
}

#[tool_router(router = tool_router)]
impl HouseCraftServer {
    pub fn new() -> Self {
        Self {
            state: Arc::default(),
            tool_router: Self::tool_router(),
        }
    }

    /// Create a new plan with one floor and standard wall types, replacing any open document.
    #[tool]
    async fn new_document(&self, Parameters(args): Parameters<NewDocumentArgs>) -> ToolResult {
        self.replace(Session::new(Document::new(args.name)), None)?;
        self.with_session(|s, _| json(&query::summary(s.document())))
    }

    /// Open a .housecraft file, replacing any open document. Returns its summary.
    #[tool]
    async fn open_document(&self, Parameters(args): Parameters<PathArgs>) -> ToolResult {
        let path = PathBuf::from(args.path);
        let doc = housecraft_io_native::load_path(&path).map_err(|e| e.to_string())?;
        self.replace(Session::new(doc), Some(path))?;
        self.with_session(|s, _| json(&query::summary(s.document())))
    }

    /// Save the open document as a .housecraft file.
    #[tool]
    async fn save_document(&self, Parameters(args): Parameters<SaveArgs>) -> ToolResult {
        self.with_session(|s, path| {
            let target = args
                .path
                .map(PathBuf::from)
                .or_else(|| path.clone())
                .ok_or("no path given and the document has never been saved")?;
            housecraft_io_native::save_path(s.document(), &target).map_err(|e| e.to_string())?;
            s.mark_saved();
            *path = Some(target);
            Ok(())
        })?;
        self.status("saved")
    }

    /// Summarize the open document: floors, wall types and wall totals.
    #[tool]
    async fn document_summary(&self) -> ToolResult {
        self.with_session(|s, _| json(&query::summary(s.document())))
    }

    /// List objects in the open document with ids and short labels.
    #[tool]
    async fn list_objects(&self, Parameters(args): Parameters<ListObjectsArgs>) -> ToolResult {
        self.with_session(|s, _| json(&query::list_objects(s.document(), args.kind)))
    }

    /// Get every property of one object by id.
    #[tool]
    async fn get_object(&self, Parameters(args): Parameters<GetObjectArgs>) -> ToolResult {
        self.with_session(|s, _| {
            let obj = query::get_object(s.document(), args.id)
                .ok_or_else(|| format!("{} not found", args.id))?;
            json(&obj)
        })
    }

    /// A schedule (doors, windows, rooms, walls, fixtures, stairs or roof planes) as a
    /// text, CSV, XML or HTML table, computed from the current model.
    #[tool]
    async fn schedule(&self, Parameters(args): Parameters<ScheduleArgs>) -> ToolResult {
        self.with_session(|s, _| {
            let table = housecraft_api::schedule::schedule(
                s.document(),
                args.kind,
                &housecraft_api::schedule::ScheduleOptions {
                    floor: args.floor,
                    columns: args.columns,
                },
            );
            let text = table.export(
                args.format
                    .unwrap_or(housecraft_api::schedule::TableFormat::Txt),
            );
            Ok(text)
        })
    }

    /// The materials list (quantities taken off walls, floors, ceilings, roofs, foundations,
    /// doors, windows, stairs and fixtures, priced from the document's price list) as a table.
    /// Set prices with the set_material_price command, keyed "Category: Item".
    #[tool]
    async fn materials_list(&self, Parameters(args): Parameters<MaterialsArgs>) -> ToolResult {
        self.with_session(|s, _| {
            let list = housecraft_api::materials::materials(
                s.document(),
                &housecraft_api::materials::MaterialsOptions {
                    floor: args.floor,
                    layer_set: None,
                },
            );
            Ok(list.table().export(
                args.format
                    .unwrap_or(housecraft_api::schedule::TableFormat::Txt),
            ))
        })
    }

    /// Apply edit commands as one atomic, undoable transaction. If any command fails, nothing changes.
    /// Returns the ids created, modified and deleted.
    #[tool]
    async fn apply_commands(&self, Parameters(args): Parameters<ApplyArgs>) -> ToolResult {
        self.with_session(|s, _| {
            let outcome = s.apply(&args.commands).map_err(|e| e.to_string())?;
            json(&outcome)
        })
    }

    /// Render a floor plan as a PNG image so you can see the design.
    #[tool]
    async fn view_plan(
        &self,
        Parameters(args): Parameters<ViewPlanArgs>,
    ) -> Result<CallToolResult, String> {
        let svg = self.with_session(|s, _| {
            let doc = s.document();
            let opts = PlanOptions::resolve(doc, args.floor.as_deref(), args.layer_set.as_deref())?;
            Ok(housecraft_draft::svg::to_svg(
                &plan(doc, &opts),
                &housecraft_draft::svg::SvgOptions::default(),
            ))
        })?;
        let png =
            housecraft_draft::svg::svg_to_png(&svg, args.pixels.unwrap_or(1200).clamp(200, 4000))?;
        let data = base64::engine::general_purpose::STANDARD.encode(png);
        Ok(CallToolResult::success(vec![ContentBlock::image(
            data,
            "image/png",
        )]))
    }

    /// Write a floor plan to an SVG, PDF (fit to a letter sheet) or PNG file.
    #[tool]
    async fn export_plan(&self, Parameters(args): Parameters<ExportPlanArgs>) -> ToolResult {
        let bytes = self.with_session(|s, _| {
            let doc = s.document();
            let opts = PlanOptions::resolve(doc, args.floor.as_deref(), args.layer_set.as_deref())?;
            let drawing = plan(doc, &opts);
            let svg_opts = housecraft_draft::svg::SvgOptions::default();
            let lower = args.path.to_ascii_lowercase();
            if lower.ends_with(".svg") {
                Ok(housecraft_draft::svg::to_svg(&drawing, &svg_opts).into_bytes())
            } else if lower.ends_with(".png") {
                housecraft_draft::svg::svg_to_png(
                    &housecraft_draft::svg::to_svg(&drawing, &svg_opts),
                    2000,
                )
            } else if lower.ends_with(".pdf") {
                let title = format!(
                    "{} - {}",
                    doc.name,
                    doc.get::<housecraft_api::model::Floor>(opts.floor)
                        .map_or("", |f| f.name.as_str())
                );
                Ok(housecraft_draft::pdf::to_pdf(
                    &drawing,
                    &housecraft_draft::pdf::PdfOptions {
                        paper: housecraft_draft::pdf::Paper::Letter,
                        landscape: true,
                        scale: housecraft_draft::pdf::Scale::Fit,
                        title,
                        imperial: true,
                    },
                )
                .0)
            } else {
                Err("path must end in .svg, .pdf or .png".to_string())
            }
        })?;
        std::fs::write(&args.path, bytes).map_err(|e| e.to_string())?;
        Ok(format!("wrote {}", args.path))
    }

    /// Undo the last apply_commands transaction.
    #[tool]
    async fn undo(&self) -> ToolResult {
        let did = self.with_session(|s, _| Ok(s.undo()))?;
        self.status(if did { "undone" } else { "nothing to undo" })
    }

    /// Redo the last undone transaction.
    #[tool]
    async fn redo(&self) -> ToolResult {
        let did = self.with_session(|s, _| Ok(s.redo()))?;
        self.status(if did { "redone" } else { "nothing to redo" })
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for HouseCraftServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("housecraft", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// Serve MCP over stdin/stdout until the client disconnects.
pub async fn serve_stdio() -> anyhow::Result<()> {
    let service = HouseCraftServer::new()
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_expected_tools() {
        let server = HouseCraftServer::new();
        let names: Vec<String> = server
            .tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        for expected in [
            "new_document",
            "open_document",
            "save_document",
            "document_summary",
            "list_objects",
            "get_object",
            "apply_commands",
            "undo",
            "redo",
            "view_plan",
            "export_plan",
            "schedule",
            "materials_list",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "missing tool {expected}"
            );
        }
    }

    #[tokio::test]
    async fn edits_require_an_open_document_then_work() {
        let server = HouseCraftServer::new();
        assert!(server.document_summary().await.is_err());

        server
            .new_document(Parameters(NewDocumentArgs {
                name: "Test".into(),
            }))
            .await
            .unwrap();
        let commands = serde_json::from_str(
            r#"[{"op": "add_walls", "points": [[0,0],["10'",0],["10'","10'"]], "closed": true}]"#,
        )
        .unwrap();
        let out = server
            .apply_commands(Parameters(ApplyArgs { commands }))
            .await
            .unwrap();
        assert!(out.contains("created"));
        let summary = server.document_summary().await.unwrap();
        assert!(summary.contains("\"wall_count\": 3"), "{summary}");
        server.undo().await.unwrap();
        let summary = server.document_summary().await.unwrap();
        assert!(summary.contains("\"wall_count\": 0"), "{summary}");
    }
}
