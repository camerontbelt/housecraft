use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use housecraft_api::model::{Document, Id, ObjectKind};
use housecraft_api::{Command, Session, query};
use housecraft_draft::pdf::{Paper, PdfOptions, Scale};
use housecraft_draft::svg::SvgOptions;
use housecraft_draft::{PlanOptions, plan};

/// HouseCraft: residential design, headless.
#[derive(Parser)]
#[command(name = "housecraft", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a new plan with one floor and standard wall types.
    New {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Summarize a plan: floors, wall types, wall totals.
    Info {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// List objects in a plan.
    Objects {
        path: PathBuf,
        /// Only this kind: floor, wall_type, wall, opening, room, room_type, layer,
        /// layer_set, text_style, default_set, cad, dimension, text, plan_view, camera.
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Print one object as JSON.
    Get { path: PathBuf, id: u64 },
    /// Apply a JSON command batch (a command or an array of commands) atomically.
    Apply {
        path: PathBuf,
        /// File containing the commands, or `-` for stdin.
        commands: PathBuf,
        /// Write the result here instead of overwriting the input.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Validate and report without saving.
        #[arg(long)]
        dry_run: bool,
    },
    /// Draw a floor plan to SVG, PDF or PNG (chosen by the output extension).
    Export {
        path: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Floor name or id (default: lowest floor).
        #[arg(long)]
        floor: Option<String>,
        /// Layer set name or id (default: the first).
        #[arg(long)]
        layer_set: Option<String>,
        /// Model units per paper unit (48 = 1/4" = 1'-0"), or "fit".
        #[arg(long, default_value = "fit")]
        scale: String,
        /// PDF paper: letter, legal, tabloid, arch-c, arch-d, a4, a3, a2, a1.
        #[arg(long, default_value = "letter")]
        paper: String,
        #[arg(long)]
        portrait: bool,
        /// Show the floor below, faded.
        #[arg(long)]
        reference: bool,
        /// PNG size in pixels (longest side).
        #[arg(long, default_value_t = 1600)]
        pixels: u32,
    },
    /// Print the JSON Schema for command batches.
    Schema,
    /// Run the MCP server on stdin/stdout.
    Mcp,
}

fn parse_kind(s: &str) -> Result<ObjectKind> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).with_context(|| {
        let all: Vec<&str> = ObjectKind::ALL.iter().map(|k| k.name()).collect();
        format!("unknown kind {s:?}; one of {}", all.join(", "))
    })
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Cmd::New { path, name, force } => {
            if path.exists() && !force {
                bail!(
                    "{} already exists (use --force to overwrite)",
                    path.display()
                );
            }
            let name = name.unwrap_or_else(|| {
                path.file_stem()
                    .map_or("Untitled".into(), |s| s.to_string_lossy().into_owned())
            });
            save(&Document::new(name), &path)?;
            println!("created {}", path.display());
        }
        Cmd::Info { path, json } => {
            let summary = query::summary(&load(&path)?);
            if json {
                print_json(&summary)?;
            } else {
                println!("{}", summary.name);
                println!("floors:");
                for f in &summary.floors {
                    println!(
                        "  {:>5}  {:<16} elev {:<12} ceiling {:<12} {} walls, {} total",
                        f.id.to_string(),
                        f.name,
                        f.elevation,
                        f.ceiling_height,
                        f.wall_count,
                        f.total_wall_length
                    );
                }
                for f in &summary.floors {
                    if !f.rooms.is_empty() {
                        println!("rooms on {}:", f.name);
                    }
                    for r in &f.rooms {
                        println!(
                            "  {:>5}  {:<20} {:<16} {:>12}  ceiling {}",
                            r.id.to_string(),
                            r.name,
                            r.room_type,
                            r.area,
                            r.ceiling_height
                        );
                    }
                }
                println!("living area: {}", summary.living_area);
                println!("wall types:");
                for t in &summary.wall_types {
                    println!("  {:>5}  {}", t.id.to_string(), t.label);
                }
            }
        }
        Cmd::Objects { path, kind, json } => {
            let kind = kind.as_deref().map(parse_kind).transpose()?;
            let objects = query::list_objects(&load(&path)?, kind);
            if json {
                print_json(&objects)?;
            } else {
                for o in objects {
                    println!(
                        "{:>5}  {:<10} {}",
                        o.id.to_string(),
                        o.kind.to_string(),
                        o.label
                    );
                }
            }
        }
        Cmd::Get { path, id } => {
            let doc = load(&path)?;
            let obj =
                query::get_object(&doc, Id(id)).with_context(|| format!("#{id} not found"))?;
            print_json(&obj)?;
        }
        Cmd::Apply {
            path,
            commands,
            output,
            dry_run,
        } => {
            let commands = read_commands(&commands)?;
            let mut session = Session::new(load(&path)?);
            let outcome = session.apply(&commands)?;
            print_json(&outcome)?;
            if !dry_run {
                let target = output.as_deref().unwrap_or(&path);
                save(session.document(), target)?;
                eprintln!("saved {}", target.display());
            }
        }
        Cmd::Export {
            path,
            output,
            floor,
            layer_set,
            scale,
            paper,
            portrait,
            reference,
            pixels,
        } => {
            let doc = load(&path)?;
            let mut opts = PlanOptions::resolve(&doc, floor.as_deref(), layer_set.as_deref())
                .map_err(anyhow::Error::msg)?;
            opts.reference_floor = reference;
            let drawing = plan(&doc, &opts);
            let imperial = matches!(
                doc.settings.display_unit,
                housecraft_api::units::DisplayUnit::FeetInches { .. }
                    | housecraft_api::units::DisplayUnit::Inches { .. }
            );
            let ratio = if scale.eq_ignore_ascii_case("fit") {
                None
            } else {
                Some(
                    scale
                        .parse::<f64>()
                        .context("--scale must be a number or \"fit\"")?,
                )
            };
            let ext = output
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let svg_opts = SvgOptions {
                scale: ratio.unwrap_or(if imperial { 48.0 } else { 50.0 }),
                ..SvgOptions::default()
            };
            let bytes = match ext.as_str() {
                "svg" => housecraft_draft::svg::to_svg(&drawing, &svg_opts).into_bytes(),
                "png" => housecraft_draft::svg::svg_to_png(
                    &housecraft_draft::svg::to_svg(&drawing, &svg_opts),
                    pixels,
                )
                .map_err(anyhow::Error::msg)?,
                "pdf" => {
                    let floor_name = doc
                        .get::<housecraft_api::model::Floor>(opts.floor)
                        .map_or(String::new(), |f| f.name.clone());
                    let (bytes, used) = housecraft_draft::pdf::to_pdf(
                        &drawing,
                        &PdfOptions {
                            paper: Paper::parse(&paper)
                                .with_context(|| format!("unknown paper {paper:?}"))?,
                            landscape: !portrait,
                            scale: ratio.map_or(Scale::Fit, Scale::Ratio),
                            title: format!("{} - {}", doc.name, floor_name),
                            imperial,
                        },
                    );
                    eprintln!(
                        "scale {}",
                        housecraft_draft::pdf::scale_label(used, imperial)
                    );
                    bytes
                }
                other => bail!("unsupported output type {other:?}; use .svg, .pdf or .png"),
            };
            std::fs::write(&output, bytes)
                .with_context(|| format!("writing {}", output.display()))?;
            println!("wrote {}", output.display());
        }
        Cmd::Schema => print_json(&housecraft_api::command_schema())?,
        Cmd::Mcp => tokio::runtime::Runtime::new()?.block_on(housecraft_mcp::serve_stdio())?,
    }
    Ok(())
}

fn load(path: &Path) -> Result<Document> {
    housecraft_io_native::load_path(path).with_context(|| format!("opening {}", path.display()))
}

fn save(doc: &Document, path: &Path) -> Result<()> {
    housecraft_io_native::save_path(doc, path).with_context(|| format!("saving {}", path.display()))
}

fn read_commands(source: &Path) -> Result<Vec<Command>> {
    let text = if source.as_os_str() == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        std::fs::read_to_string(source).with_context(|| format!("reading {}", source.display()))?
    };
    // Windows PowerShell pipes and Notepad add a UTF-8 BOM.
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let value: serde_json::Value =
        serde_json::from_str(text).context("commands are not valid JSON")?;
    let commands = if value.is_array() {
        serde_json::from_value(value)
    } else {
        serde_json::from_value(value).map(|c| vec![c])
    };
    commands.context("commands do not match the schema (see `housecraft schema`)")
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
