use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use housecraft_api::model::{Block, CadDetail, Document, Id, ObjectKind, Phase, SectionView};
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
    /// Build the 3D model and report how long it took and its size by
    /// kind of object (for finding slow or heavy parts).
    MeshStats { path: PathBuf },
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
    /// Bring a DXF drawing's lines, arcs, circles and text into a plan as CAD.
    ImportDxf {
        path: PathBuf,
        dxf: PathBuf,
        /// Floor name or id (default: lowest floor).
        #[arg(long)]
        floor: Option<String>,
        /// Millimetres per DXF unit, overriding the file's units (25.4 = inches).
        #[arg(long)]
        unit_mm: Option<f64>,
    },
    /// Draw a floor plan to SVG, PDF, PNG or DXF (chosen by the output extension).
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
        /// Show this floor (name or id) faded instead of the floor below; repeatable.
        #[arg(long)]
        reference_floor: Vec<String>,
        /// Export this elevation, section view or CAD detail (name or id) instead of a plan.
        #[arg(long)]
        view: Option<String>,
        /// Draw sun shadows on the ground around the lowest storey.
        #[arg(long)]
        shadows: bool,
        /// Remodel phase: all, as-built, demolition or proposed.
        #[arg(long, default_value = "all")]
        phase: String,
        /// PNG size in pixels (longest side).
        #[arg(long, default_value_t = 1600)]
        pixels: u32,
    },
    /// Analyse an unknown file (e.g. a Chief Architect .plan): container,
    /// compressed streams, entropy and strings.
    Probe {
        path: PathBuf,
        #[arg(long)]
        json: bool,
        /// Write each payload (ZIP entries, inflated zlib streams) to this directory.
        #[arg(long)]
        extract: Option<PathBuf>,
    },
    /// Byte-diff two files, payload by payload (for "change one thing" sample pairs).
    ProbeDiff {
        a: PathBuf,
        b: PathBuf,
        /// Merge changes separated by fewer than this many equal bytes.
        #[arg(long, default_value_t = 4)]
        gap: usize,
        #[arg(long)]
        json: bool,
    },
    /// List the walls found in a Chief Architect .plan or undo snapshot
    /// (experimental: records are matched by shape while the format is decoded).
    CaWalls {
        path: PathBuf,
        /// Undo snapshots: show every copy, not just the state after the action.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// Print or export a schedule: doors, windows, rooms, walls, fixtures,
    /// stairs or roof-planes.
    Schedule {
        path: PathBuf,
        kind: String,
        /// Floor name or id (default: every floor).
        #[arg(long)]
        floor: Option<String>,
        /// Comma-separated columns to include, in order.
        #[arg(long, value_delimiter = ',')]
        columns: Vec<String>,
        /// Write here; .csv, .txt, .xml or .html picks the format (default: text to stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Print or export the materials list.
    Materials {
        path: PathBuf,
        /// Floor name or id (default: every floor).
        #[arg(long)]
        floor: Option<String>,
        /// Write here; .csv, .txt, .xml or .html picks the format (default: text to stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Export a layout: all pages to PDF, or one page to SVG / PNG.
    ExportLayout {
        path: PathBuf,
        /// Layout name or id.
        layout: String,
        #[arg(short, long)]
        output: PathBuf,
        /// Page number (from 1) for SVG / PNG.
        #[arg(long, default_value_t = 1)]
        page: usize,
        /// PNG size in pixels (longest side).
        #[arg(long, default_value_t = 2000)]
        pixels: u32,
    },
    /// Make a CAD detail of editable lines and text from a floor plan or an
    /// elevation/section view (CAD Detail from View).
    DetailFromView {
        path: PathBuf,
        /// Name for the new CAD detail.
        #[arg(long)]
        name: String,
        /// Elevation or section view (name or id); a floor plan when omitted.
        #[arg(long)]
        view: Option<String>,
        /// Floor (name or id) when converting a plan (default: lowest floor).
        #[arg(long)]
        floor: Option<String>,
        /// Write the result here instead of overwriting the input.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Save a CAD block (name or id) to the user library, or to `--library`.
    SaveBlock {
        path: PathBuf,
        block: String,
        #[arg(long)]
        library: Option<PathBuf>,
    },
    /// Print the JSON Schema for command batches.
    Schema,
    /// Run the MCP server on stdin/stdout.
    Mcp,
}

/// A CAD detail by name or `#id`.
fn find_detail(doc: &Document, key: &str) -> Option<Id> {
    doc.iter::<CadDetail>()
        .find(|d| d.name.eq_ignore_ascii_case(key) || format!("#{}", d.id.0) == key || d.id.0.to_string() == key)
        .map(|d| d.id)
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
        Cmd::MeshStats { path } => {
            let doc = load(&path)?;
            let start = std::time::Instant::now();
            let mesh = housecraft_api::geom::mesh::build(&doc, None);
            let took = start.elapsed();
            let mut by_kind: std::collections::BTreeMap<String, (usize, u32)> = Default::default();
            for part in &mesh.parts {
                let kind = doc.kind_of(part.object).map_or("(none)".into(), |k| k.to_string());
                let e = by_kind.entry(kind).or_default();
                e.0 += 1;
                e.1 += part.index_count / 3;
            }
            println!("built in {:.2?}: {} triangles, {} lights", took, mesh.indices.len() / 3, mesh.lights.len());
            for (kind, (parts, tris)) in by_kind {
                println!("  {kind:<20} {parts:>6} parts {tris:>9} triangles");
            }
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
        Cmd::DetailFromView {
            path,
            name,
            view,
            floor,
            output,
        } => {
            let doc = load(&path)?;
            let drawing = match &view {
                Some(v) => {
                    let sv = doc
                        .iter::<SectionView>()
                        .find(|s| s.name.eq_ignore_ascii_case(v) || format!("#{}", s.id.0) == *v)
                        .with_context(|| format!("no elevation or section view {v:?}"))?;
                    housecraft_draft::elevation::section_view(&doc, sv)
                }
                None => {
                    let opts = PlanOptions::resolve(&doc, floor.as_deref(), None)
                        .map_err(anyhow::Error::msg)?;
                    plan(&doc, &opts)
                }
            };
            let command = housecraft_draft::detail::detail_from_drawing(&drawing, &name);
            let mut session = Session::new(doc);
            let outcome = session.apply(&[command])?;
            print_json(&outcome)?;
            let target = output.as_deref().unwrap_or(&path);
            save(session.document(), target)?;
            eprintln!("saved {}", target.display());
        }
        Cmd::SaveBlock {
            path,
            block,
            library,
        } => {
            use housecraft_io_native::library;
            let doc = load(&path)?;
            let b = doc
                .iter::<Block>()
                .find(|b| b.name.eq_ignore_ascii_case(&block) || format!("#{}", b.id.0) == block)
                .with_context(|| format!("no block {block:?}"))?;
            let dir = library
                .or_else(library::library_dir)
                .context("no library folder; pass --library")?;
            let saved = library::save_block(
                &dir,
                &library::LibraryBlock {
                    name: b.name.clone(),
                    shapes: housecraft_api::blocks::block_shapes(&doc, b.id),
                },
            )?;
            println!("{}", saved.display());
        }
        Cmd::ImportDxf { path, dxf, floor, unit_mm } => {
            let mut doc = load(&path)?;
            let text = std::fs::read_to_string(&dxf).with_context(|| format!("reading {}", dxf.display()))?;
            let d = housecraft_io_native::dxf::read(&text, unit_mm).map_err(anyhow::Error::msg)?;
            let floor = match floor {
                Some(f) => Some(PlanOptions::resolve(&doc, Some(&f), None).map_err(anyhow::Error::msg)?.floor),
                None => None,
            };
            let command = dxf_command(&d, floor);
            let mut session = housecraft_api::Session::new(doc);
            let out = session.apply(&[command])?;
            doc = session.document().clone();
            housecraft_io_native::save_path(&doc, &path)?;
            println!(
                "imported {} shapes and {} notes ({} objects created){}",
                d.shapes.len(),
                d.texts.len(),
                out.created.len(),
                if d.skipped.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; skipped {}",
                        d.skipped.iter().map(|(n, c)| format!("{c} {n}")).collect::<Vec<_>>().join(", ")
                    )
                }
            );
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
            reference_floor,
            view,
            shadows,
            phase,
            pixels,
        } => {
            let doc = load(&path)?;
            let mut opts = PlanOptions::resolve(&doc, floor.as_deref(), layer_set.as_deref())
                .map_err(anyhow::Error::msg)?;
            opts.reference_floor = reference || !reference_floor.is_empty();
            for r in &reference_floor {
                opts.references.push(
                    PlanOptions::resolve(&doc, Some(r), None)
                        .map_err(anyhow::Error::msg)?
                        .floor,
                );
            }
            opts.sun_shadows = shadows;
            opts.phase = match phase.as_str() {
                "all" => Phase::All,
                "as-built" | "as_built" => Phase::AsBuilt,
                "demolition" => Phase::Demolition,
                "proposed" => Phase::Proposed,
                other => bail!("unknown phase {other:?} (all, as-built, demolition, proposed)"),
            };
            let drawing = match &view {
                Some(v) => {
                    let found = v
                        .trim_start_matches('#')
                        .parse::<u64>()
                        .ok()
                        .and_then(|n| doc.get::<SectionView>(Id(n)))
                        .or_else(|| {
                            doc.iter::<SectionView>()
                                .find(|s| s.name.eq_ignore_ascii_case(v))
                        });
                    match (found, find_detail(&doc, v)) {
                        (Some(sv), _) => housecraft_draft::elevation::section_view(&doc, sv),
                        (None, Some(d)) => {
                            housecraft_draft::detail::cad_detail(&doc, d, Some(opts.layer_set))
                        }
                        (None, None) => {
                            let names: Vec<&str> = doc
                                .iter::<SectionView>()
                                .map(|s| s.name.as_str())
                                .chain(doc.iter::<CadDetail>().map(|d| d.name.as_str()))
                                .collect();
                            bail!("no view or detail {v:?} (have: {})", names.join(", "));
                        }
                    }
                }
                None => plan(&doc, &opts),
            };
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
                "dxf" => housecraft_draft::dxf::to_dxf(
                    &doc,
                    &drawing,
                    &housecraft_draft::dxf::DxfOptions { imperial },
                )
                .into_bytes(),
                other => bail!("unsupported output type {other:?}; use .svg, .pdf, .png or .dxf"),
            };
            std::fs::write(&output, bytes)
                .with_context(|| format!("writing {}", output.display()))?;
            println!("wrote {}", output.display());
        }
        Cmd::Probe {
            path,
            json,
            extract,
        } => {
            let data =
                std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            let report = housecraft_io_ca::probe::probe(&data);
            if let Some(dir) = extract {
                std::fs::create_dir_all(&dir)?;
                for (name, bytes) in housecraft_io_ca::probe::payloads(&data) {
                    let safe: String = name
                        .chars()
                        .map(|c| {
                            if c.is_ascii_alphanumeric() || "._-@".contains(c) {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect();
                    std::fs::write(dir.join(format!("{safe}.bin")), bytes)?;
                }
                eprintln!("payloads written to {}", dir.display());
            }
            if json {
                print_json(&report)?;
            } else {
                print_probe(&report);
            }
        }
        Cmd::ProbeDiff { a, b, gap, json } => {
            let (da, db) = (std::fs::read(&a)?, std::fs::read(&b)?);
            for (path, data) in [(&a, &da), (&b, &db)] {
                if let Some(h) = housecraft_io_ca::undo::header(data) {
                    eprintln!(
                        "{}: undo snapshot v{:#06x}, before \"{}\"",
                        path.display(),
                        h.version,
                        h.action
                    );
                }
            }
            let (pa, pb) = (
                housecraft_io_ca::probe::payloads(&da),
                housecraft_io_ca::probe::payloads(&db),
            );
            let mut results = Vec::new();
            for (name, bytes_a) in &pa {
                match pb.iter().find(|(n, _)| n == name) {
                    Some((_, bytes_b)) => results.push((
                        name.clone(),
                        housecraft_io_ca::diff::diff(bytes_a, bytes_b, gap),
                    )),
                    None => eprintln!("payload {name} only in {}", a.display()),
                }
            }
            for (name, _) in pb.iter().filter(|(n, _)| !pa.iter().any(|(m, _)| m == n)) {
                eprintln!("payload {name} only in {}", b.display());
            }
            if json {
                let v: Vec<_> = results
                    .iter()
                    .map(|(n, d)| serde_json::json!({ "payload": n, "diff": d }))
                    .collect();
                print_json(&v)?;
            } else {
                for (name, d) in &results {
                    if d.identical {
                        println!("{name}: identical ({} bytes)", d.a_len);
                        continue;
                    }
                    println!(
                        "{name}: {} -> {} bytes, {} changed region(s){}",
                        d.a_len,
                        d.b_len,
                        d.hunks.len(),
                        if d.approximate { " (approximate)" } else { "" }
                    );
                    for h in d.hunks.iter().take(50) {
                        println!(
                            "  @{:#010x} -{} +{}
    a: {}
    b: {}",
                            h.a_offset, h.a_len, h.b_len, h.a_bytes, h.b_bytes
                        );
                        if let Some(n) = &h.as_numbers {
                            println!("    {n}");
                        }
                        for c in &h.doubles {
                            println!(
                                "    f64 @{:#010x}: {} -> {} (Δ {})",
                                c.a_offset,
                                c.old,
                                c.new,
                                c.new - c.old
                            );
                        }
                    }
                }
            }
        }
        Cmd::CaWalls { path, all, json } => {
            let data = std::fs::read(&path)?;
            if let Some(h) = housecraft_io_ca::undo::header(&data) {
                eprintln!("undo snapshot v{:#06x}, before \"{}\"", h.version, h.action);
            }
            let walls = if all {
                housecraft_io_ca::plan::scan_walls(&data)
            } else {
                housecraft_io_ca::plan::latest_walls(&data)
            };
            if json {
                print_json(&walls)?;
            } else {
                println!("{} wall(s)", walls.len());
                for w in &walls {
                    println!(
                        "  @{:#08x}  ({:.3}, {:.3}) -> ({:.3}, {:.3})  len {:.3}\"  thick {:.4}\"  height {}",
                        w.offset,
                        w.start[0],
                        w.start[1],
                        w.end[0],
                        w.end[1],
                        w.length,
                        w.thickness,
                        w.height.map_or("?".to_string(), |h| format!("{h:.3}\""))
                    );
                }
            }
        }
        Cmd::Schedule {
            path,
            kind,
            floor,
            columns,
            output,
        } => {
            use housecraft_api::schedule::{ScheduleOptions, TableFormat, schedule};
            let doc = load(&path)?;
            let kind: housecraft_api::model::ScheduleKind =
                serde_json::from_value(serde_json::Value::String(kind.replace('-', "_")))
                    .map_err(|_| anyhow::anyhow!("unknown schedule {kind:?} (doors, windows, rooms, walls, fixtures, stairs, roof-planes)"))?;
            let floor = match floor {
                Some(f) => Some(
                    PlanOptions::resolve(&doc, Some(&f), None)
                        .map_err(anyhow::Error::msg)?
                        .floor,
                ),
                None => None,
            };
            let table = schedule(&doc, kind, &ScheduleOptions { floor, columns });
            match output {
                Some(out) => {
                    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let format = TableFormat::from_extension(ext)
                        .ok_or_else(|| anyhow::anyhow!("use a .csv, .txt, .xml or .html output"))?;
                    std::fs::write(&out, table.export(format))?;
                    println!("wrote {}", out.display());
                }
                None => print!("{}", table.export(TableFormat::Txt)),
            }
        }
        Cmd::Materials {
            path,
            floor,
            output,
        } => {
            use housecraft_api::schedule::TableFormat;
            let doc = load(&path)?;
            let floor = match floor {
                Some(f) => Some(
                    PlanOptions::resolve(&doc, Some(&f), None)
                        .map_err(anyhow::Error::msg)?
                        .floor,
                ),
                None => None,
            };
            let table = housecraft_api::materials::materials(
                &doc,
                &housecraft_api::materials::MaterialsOptions {
                    floor,
                    layer_set: None,
                },
            )
            .table();
            match output {
                Some(out) => {
                    let ext = out.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let format = TableFormat::from_extension(ext)
                        .ok_or_else(|| anyhow::anyhow!("use a .csv, .txt, .xml or .html output"))?;
                    std::fs::write(&out, table.export(format))?;
                    println!("wrote {}", out.display());
                }
                None => print!("{}", table.export(TableFormat::Txt)),
            }
        }
        Cmd::ExportLayout {
            path,
            layout,
            output,
            page,
            pixels,
        } => {
            let doc = load(&path)?;
            let found = layout
                .trim_start_matches('#')
                .parse::<u64>()
                .ok()
                .and_then(|n| doc.get::<housecraft_api::model::Layout>(Id(n)))
                .or_else(|| {
                    doc.iter::<housecraft_api::model::Layout>()
                        .find(|l| l.name.eq_ignore_ascii_case(&layout))
                });
            let Some(l) = found else {
                let names: Vec<&str> = doc
                    .iter::<housecraft_api::model::Layout>()
                    .map(|l| l.name.as_str())
                    .collect();
                bail!("no layout {layout:?} (have: {})", names.join(", "));
            };
            let (w, h) = (l.paper_width.mm(), l.paper_height.mm());
            let ext = output
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "pdf" {
                let pages = housecraft_draft::layout::layout_pages(&doc, l);
                std::fs::write(&output, housecraft_draft::pdf::pages_to_pdf(&pages, w, h))?;
            } else {
                if page == 0 || page > l.pages.len() {
                    bail!("{} has pages 1 to {}", l.name, l.pages.len());
                }
                let mut d = housecraft_draft::layout::layout_page(&doc, l, page - 1);
                // A sheet-sized white frame so the image is the whole sheet.
                d.items.insert(
                    0,
                    housecraft_draft::Item {
                        object: None,
                        prim: housecraft_draft::Prim::Polygon {
                            points: vec![
                                housecraft_api::geom::DVec2::ZERO,
                                housecraft_api::geom::DVec2::new(w, 0.0),
                                housecraft_api::geom::DVec2::new(w, h),
                                housecraft_api::geom::DVec2::new(0.0, h),
                            ],
                            fill: housecraft_draft::Fill::Paper,
                        },
                    },
                );
                let svg = housecraft_draft::svg::to_svg(
                    &d,
                    &SvgOptions {
                        scale: 1.0,
                        margin: 0.0,
                        ..SvgOptions::default()
                    },
                );
                match ext.as_str() {
                    "svg" => std::fs::write(&output, svg)?,
                    "png" => std::fs::write(
                        &output,
                        housecraft_draft::svg::svg_to_png(&svg, pixels)
                            .map_err(anyhow::Error::msg)?,
                    )?,
                    _ => bail!("use a .pdf, .svg or .png output"),
                }
            }
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

fn print_probe(r: &housecraft_io_ca::probe::ProbeReport) {
    println!("size      {} bytes", r.size);
    println!("sha256    {}", r.sha256);
    println!("head      {}", r.head);
    println!(
        "container {}",
        serde_json::to_string(&r.container).unwrap_or_default()
    );
    const LEVELS: &[char] = &[' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
    let map: String = r
        .entropy
        .iter()
        .map(|e| LEVELS[((e / 8.0) * (LEVELS.len() - 1) as f32).round() as usize])
        .collect();
    println!(
        "entropy   per {} KiB block, ' ' = 0 ... '@' = 8 bits/byte:",
        housecraft_io_ca::probe::ENTROPY_BLOCK / 1024
    );
    for line in map.as_bytes().chunks(64) {
        println!("          |{}|", String::from_utf8_lossy(line));
    }
    println!("zlib streams: {}", r.zlib_streams.len());
    for s in r.zlib_streams.iter().take(40) {
        println!(
            "  @{:#010x}  {} -> {} bytes  {}",
            s.offset, s.compressed_len, s.inflated_len, s.inflated_head
        );
    }
    println!("ascii strings (first 40 of {}):", r.ascii_strings.len());
    for s in r.ascii_strings.iter().take(40) {
        println!("  @{:#010x}  {}", s.offset, s.text);
    }
    println!("utf-16 strings (first 40 of {}):", r.utf16_strings.len());
    for s in r.utf16_strings.iter().take(40) {
        println!("  @{:#010x}  {}", s.offset, s.text);
    }
}

/// The command that brings a read DXF drawing into a plan.
fn dxf_command(d: &housecraft_io_native::dxf::DxfDrawing, floor: Option<Id>) -> housecraft_api::Command {
    housecraft_api::Command::ImportDrawing {
        floor,
        shapes: d
            .shapes
            .iter()
            .map(|(layer, shape)| housecraft_api::NamedShape { layer: layer.clone(), shape: shape.clone() })
            .collect(),
        texts: d
            .texts
            .iter()
            .map(|t| housecraft_api::NamedText {
                layer: t.layer.clone(),
                position: t.at,
                text: t.text.clone(),
                height: t.height,
                rotation: t.rotation,
            })
            .collect(),
        offset: None,
    }
}
