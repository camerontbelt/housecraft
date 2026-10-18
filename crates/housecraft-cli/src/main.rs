use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use housecraft_api::model::{Document, Id, ObjectKind};
use housecraft_api::{Command, Session, query};

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
        #[arg(long, value_enum)]
        kind: Option<Kind>,
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
    /// Print the JSON Schema for command batches.
    Schema,
    /// Run the MCP server on stdin/stdout.
    Mcp,
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    Floor,
    WallType,
    Wall,
}

impl From<Kind> for ObjectKind {
    fn from(k: Kind) -> Self {
        match k {
            Kind::Floor => ObjectKind::Floor,
            Kind::WallType => ObjectKind::WallType,
            Kind::Wall => ObjectKind::Wall,
        }
    }
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
                println!("wall types:");
                for t in &summary.wall_types {
                    println!("  {:>5}  {}", t.id.to_string(), t.label);
                }
            }
        }
        Cmd::Objects { path, kind, json } => {
            let objects = query::list_objects(&load(&path)?, kind.map(Into::into));
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
