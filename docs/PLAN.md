# HouseCraft — planning doc

Project name **HouseCraft** (must not reference "Chief" or imply affiliation).
Goal: a Rust, cross-platform (Windows / macOS / Linux / Web) residential design & construction-documents
app with feature parity to Chief Architect Premier, plus a headless CLI, an MCP server for LLMs, and the
ability to open Chief Architect files (`.plan`, `.layout`, `.calib`, `.calibz`).

---

## 1. Reality check

Chief Architect has been in development for about 30 years. Full parity is a multi-year effort, so the plan
ships in vertical slices. Each slice is usable on its own and is scriptable from day one through the CLI and MCP.
Those two interfaces are where we can beat the incumbent early.

### Legal guardrails (decide before starting)
- **File formats are undocumented.** Reverse engineering for *interoperability* is generally allowed
  (US DMCA §1201(f), EU Software Directive Art. 6). The Chief Architect EULA may still restrict it, so review it.
  Have a lawyer take a quick look before anything ships publicly.
- **Use a clean-room process.** One person or agent analyzes binaries and samples and writes a *format spec*
  (prose plus field tables, with no decompiled code). A separate implementer writes the Rust reader only from that spec.
  Keep the spec in `docs/formats/`.
- **Library content is copyrighted.** That covers manufacturer catalogs, the 4,000-plant library, the 500 SSA CAD
  details and textures. We may *read* a user's own `.calibz`, but we never redistribute that content. Our own
  default library has to be original or CC0 (for example Poly Haven or ambientCG textures).
- No trademarks: we don't use their names, icons or UI chrome.

---

## 2. Feature inventory (from the Premier product page and the X17 changelog)

> **Superseded by [FEATURES.md](FEATURES.md)**, a 434-item backlog built from the X18 detailed feature list.
> Its §1 lists architecture implications (default sets, layer sets, saved views, auto-rebuild with manual
> override, scoping, object info fields) that M1 must design for. The summary below is kept for orientation.

Priority tags: **P0** = MVP, **P1** = credible v1, **P2** = parity, **P3** = later.

### 2.1 Core modeling (BIM-lite object model)
| Feature | Pri |
|---|---|
| Multi-floor building model; floors, ceiling heights, foundation level | P0 |
| Walls: draw, wall types (layered assemblies), joins, auto room detection | P0 |
| Rooms: auto-detected from walls, room types, floor/ceiling finishes, area | P0 |
| Doors & windows: parametric, placed in walls, swing, casing, trim | P0 |
| Simultaneous 2D plan + 3D model editing (single source of truth) | P0 |
| Foundations: slab, crawl, basement, piers, footings | P1 |
| Stairs, ramps, landings, railings (auto + manual) | P1 |
| Roofs: auto roof generation from walls (hip/gable/shed/gambrel/etc.), manual roof planes, dormers, skylights | P1 |
| Ceilings: flat, vaulted, tray, coffered | P1 |
| Framing: stick (walls/floors/roofs) and trusses, auto + manual, headers | P2 |
| Decks: auto/manual framing, planking, railings | P2 |
| Electrical: outlets, switches, fixtures, circuits, ganging (X17), electrical dimensions | P1 |
| Plumbing / HVAC symbols | P2 |
| Moldings, trim, shadow boards, corner boards, quoins | P2 |
| As-built vs remodel layers (existing / demo / new) | P1 |
| CAD-to-Walls (convert imported DWG lines to walls with layer mapping) | P2 |

### 2.2 Kitchen, bath & interiors
| Feature | Pri |
|---|---|
| Smart cabinets: base/wall/full-height, doors/drawers, fillers, box thickness (X17), countertops, backsplash | P1 |
| Appliances, fixtures, furniture placement from library | P1 |
| NKBA kitchen & bath auto-dimensioning | P2 |
| Style palettes (save and apply door/window/cabinet/color sets) | P2 |
| Manufacturer catalogs (third-party content pipeline) | P3 |
| Interactive Material Builder (custom tile/material patterns, X17) | P2 |

### 2.3 Site, terrain & landscape
| Feature | Pri |
|---|---|
| Terrain perimeter, elevation data, survey import, contours | P2 |
| Roads, sidewalks, hills/valleys, retaining walls, terrain breaks | P2 |
| Plant library with hardiness zones and growth | P3 |
| North pointer, latitude/longitude, sun angle, seasonal settings | P1 |
| Multi-lot site plans | P3 |

### 2.4 2D drafting & construction documents
| Feature | Pri |
|---|---|
| CAD primitives: line, polyline, arc, circle, spline, solids, hatches, text, callouts | P0 |
| Edit ops: move, copy, multi-copy, rotate, reflect, align, replicate at intervals, trim/extend, offset | P0 |
| Layers and layer sets per view; line styles, weights, colors | P0 |
| Dimensions: auto exterior/interior, manual, temporary dims (direct-edit), angular | P0 |
| Cross-sections and elevations (auto-generated from the model, live-linked) | P1 |
| Layout sheets (`.layout`): title blocks, viewports at user scales, live-linked to the plan | P1 |
| Schedules: door, window, room, cabinet, electrical (live tables) | P1 |
| CAD detail library / CAD details from view | P2 |
| Framing plans, electrical plans, roof plans, plot plans | P2 |
| Text styles, labels, auto-labels, annotation sets, saved plan views | P1 |

### 2.5 3D visualization
| Feature | Pri |
|---|---|
| Real-time 3D: camera, orbit, perspective/ortho, full overview, cross-section camera | P0 |
| Materials/textures (PBR), lights | P1 |
| Rendering styles: standard, line drawing, watercolor, glass house, technical illustration | P2 |
| Progressive ray tracing (GPU path tracer) | P2 |
| 360° panoramas, virtual tours | P3 |
| Walkthrough paths, video export, stationary walkthrough (X17) | P2 |
| Sun/shadow studies, time-lapse | P2 |
| 3D text, 3D moldings, animated water (X17) | P3 |
| Web/mobile viewer for clients (counterpart to their 3D Viewer app) | P1, which comes free with the web target |

### 2.6 Estimating & output
| Feature | Pri |
|---|---|
| Materials list (auto from model; by floor, room, area, or whole plan) with pricing | P1 |
| Export: PDF (vector), DXF/DWG, glTF/GLB, OBJ, Collada, STL, image (PNG/JPG), IFC | P0–P2 |
| Print/plot to scale | P1 |
| Import: DXF/DWG, PDF (vector and raster underlay; "PDF Import Wizard"), images, OBJ/glTF/3DS/SKP symbols | P1–P2 |
| Point cloud / scanned room import (their "As-Built" app); LiDAR via USDZ/RoomPlan | P3 |

### 2.7 Project & library management
| Feature | Pri |
|---|---|
| Native file format, autosave, backups, undo/redo, archive | P0 |
| Library browser: core + user + imported libraries, search, drag-drop | P1 |
| Project browser with folders and filter; reference-safe project mode (X17) | P2 |
| Plan defaults & preferences (default wall types, dimensions, text styles, etc.) | P0 |
| Template plans | P1 |

### 2.8 Differentiators (not in Chief Architect)
| Feature | Pri |
|---|---|
| **Headless CLI**: open, query, modify, export, render, generate materials list, batch convert | P0 |
| **MCP server**: an LLM can inspect and edit the model through structured tools | P0 |
| Scripting (Rhai or Lua, sharing the command API with CLI and MCP) | P2 |
| Git-friendly text-diffable native format option | P1 |
| Real-time collaboration (CRDT-based), since multi-user editing is a known Chief Architect gap | P3 |

### 2.9 Chief Architect file compatibility
| Feature | Pri |
|---|---|
| Read `.plan` (walls, rooms, openings, floors first, then expand coverage) | P1 |
| Read `.layout` (sheets, viewports referencing plans) | P2 |
| Read `.calib`/`.calibz` (user library items, textures) | P2 |
| Write `.plan` | P3 (risky: version churn and fidelity) |
| Version matrix: the format changes per release (X10 to X17+), so target the newest first | — |

---

## 3. Architecture

### 3.1 Principle: one command API, four front-ends
Every mutation is a serializable **Command** (`AddWall { from, to, wall_type }`, `SetProperty { id, path, value }`, …)
applied to a **Document** through a transaction. That gives us:
- undo/redo for free,
- GUI, CLI, MCP and scripts as thin adapters over the same API,
- deterministic replay for tests and bug reports,
- a path to collaboration later (commands → ops).

```
            ┌────────┐ ┌───────┐ ┌───────┐ ┌─────────┐
            │  GUI   │ │  CLI  │ │  MCP  │ │ Scripts │
            └───┬────┘ └───┬───┘ └───┬───┘ └────┬────┘
                └──────────┴────┬────┴──────────┘
                         housecraft-api  (commands, queries, transactions)
                                │
         ┌────────────────┬───────┴─────────┬──────────────────┐
  housecraft-model  housecraft-build  housecraft-draft   housecraft-io-*
   (ECS-ish doc)     (walls→rooms,    (2D views, dims,   (native, CA, DXF,
                      roofs, framing,  sections,          PDF, glTF, IFC…)
                      stairs, cabs)    layouts)
                                │
                         housecraft-geom (2D/3D kernel)
                                │
                     housecraft-render (wgpu; raster + path tracer)
```

### 3.2 Crate layout (cargo workspace)
| Crate | Responsibility | Key deps (candidates) |
|---|---|---|
| `housecraft-units` | Lengths (imperial/metric, fractional inches), angles, parsing/formatting | — |
| `housecraft-geom` | 2D polygons/booleans/offsets, 3D meshes, extrusion, CSG, roof-plane solving | `glam`, `i_overlay` or `geo`, `parry3d`, `manifold` (C++ bindings, which compile to wasm) or a pure-Rust B-rep later |
| `housecraft-model` | Document, entity IDs, floors, objects, parametric dependencies, defaults | `slotmap`, `serde` |
| `housecraft-build` | Generators: rooms from walls, auto roofs, framing, stairs, cabinets, foundations | geom, model |
| `housecraft-draft` | 2D view generation (plan, elevation, section), dimensions, annotations, layouts, hidden-line removal | geom |
| `housecraft-api` | Commands, queries, transactions, undo, events, JSON schema for every command | `schemars` |
| `housecraft-io-native` | `.housecraft` format: zip of CBOR/JSON plus assets, versioned migrations | `zip`, `ciborium` |
| `housecraft-io-ca` | Chief Architect `.plan`/`.layout`/`.calib(z)` readers (written from the clean-room spec) | `binrw` |
| `housecraft-io-cad` | DXF in/out, DWG in (see note), PDF out (vector), PDF import | `dxf`, `krilla` or `printpdf`, `pdfium-render` |
| `housecraft-io-mesh` | glTF/GLB, OBJ, STL, Collada; IFC later | `gltf` |
| `housecraft-render` | wgpu renderer (native + WebGPU/WebGL2), PBR, shadows, line/NPR styles; later a progressive path tracer in compute | `wgpu` |
| `housecraft-app` | Desktop + web GUI | `egui`/`eframe` (fastest route to a CAD-style UI that runs on all four targets) |
| `housecraft-cli` | `housecraft open/info/query/apply/export/render/materials` | `clap` |
| `housecraft-mcp` | MCP server (stdio + streamable HTTP) exposing api commands and queries | `rmcp` (official Rust MCP SDK) |

**DWG caveat:** the only mature open reader is LibreDWG, which is **GPL**. Options are the ODA Teigha SDK
(paid membership), shipping DXF-only, or isolating DWG support in an optional GPL plugin. Decide based on the license
we pick for HouseCraft.

### 3.3 Web target
- egui and wgpu compile to `wasm32-unknown-unknown`. The web build runs in-browser with WebGPU and falls back to WebGL2.
- File I/O goes through the File System Access API and IndexedDB, and threads come from `wasm-bindgen-rayon` (needs COOP/COEP headers).
- The path tracer and heavy jobs can also run server-side through the same CLI binary.

### 3.4 MCP surface (first cut)
- Resources: `doc://current/summary`, `doc://floors/{n}`, `doc://objects/{id}`, `doc://schedules/{name}`
- Tools: `open_document`, `list_objects(filter)`, `get_object`, `apply_commands([...])` (transactional),
  `undo`, `export(format, view)`, `render_view(camera) → image`, `materials_list(scope)`,
  `plan_image(floor) → PNG` (so the LLM can *see* what it drew)
- Every tool's input schema is generated from the `housecraft-api` command types, so the surface never drifts.

### 3.5 CLI (first cut)
```
housecraft info house.plan                      # floors, rooms, areas
housecraft convert house.plan house.housecraft
housecraft export house.housecraft --format pdf --layout A1 -o set.pdf
housecraft export house.housecraft --format gltf -o house.glb
housecraft apply house.housecraft ops.json             # batch commands
housecraft render house.housecraft --camera front --spp 256 -o front.png
housecraft materials house.housecraft --format csv
housecraft mcp --stdio                          # run as MCP server
```

---

## 4. Reverse-engineering plan for Chief Architect formats

1. **Collect a corpus.** You need a licensed install (trial or owned). Make minimal plans: an empty plan,
   one wall, two walls, a wall with a door, change one property, and so on. Save each across versions where possible.
2. **Analyze the container first.** Run entropy and magic-byte checks, look for compression (zlib/zstd),
   section tables and string tables. **Differential analysis** of the minimal-plan pairs is the highest-yield technique.
3. **Analyze the binary with REA** only where the diffs get stuck: locate the serializer/deserializer routines
   and recover the object type registry, field order and version gates.
4. **Write the spec** in `docs/formats/plan.md`, with field tables and per-version deltas.
5. **Implementer side:** a `binrw`-based reader plus a golden-file test suite
   (each sample → expected JSON of walls, rooms and openings).
6. Coverage grows object type by object type, prioritized by what real plans contain.

---

## 5. Roadmap (vertical slices)

| Milestone | Contents | Proves |
|---|---|---|
| **M0 — skeleton** | Workspace, units, model, api, native format, CLI `info/apply`, MCP stub, CI for win/mac/linux/wasm | Architecture end-to-end |
| **M1 — walls & rooms** | Draw walls, auto rooms, doors/windows, 2D plan view, basic 3D view, dimensions, DXF + PDF export | Core loop works in GUI, CLI and MCP |
| **M2 — CA read v0** | `.plan` reader: floors, walls, rooms, openings → native | Interop is feasible |
| **M3 — building systems** | Foundations, auto roofs, stairs, ceilings, electrical | Real house shells |
| **M4 — documents** | Elevations, sections, layouts/sheets, schedules, materials list | Permit-set capable |
| **M5 — interiors** | Cabinets, countertops, library browser, materials/textures, PBR render | Kitchen/bath designers |
| **M6 — framing & site** | Stick/truss framing, decks, terrain | Builder workflows |
| **M7 — visualization** | Path tracer, NPR styles, walkthroughs, 360° | Presentation parity |
| **M8+** | `.layout`/`.calib` read, IFC, DWG, collaboration, scripting | Parity and beyond |

---

## Sources
- Chief Architect Premier product page: https://www.chiefarchitect.com/products/premier/
- X17 changelog: https://cloud.chiefarchitect.com/1/pdf/documentation/chief-architect-x17-changelog.pdf
- Features introduced in X17: https://www.chiefarchitect.com/videos/playlists/197/features-introduced-in-x17.html
- Premier vs Interiors: https://www.chiefarchitect.com/blog/chief-architect-premier-vs-interiors/
- DWG/DXF export KB: https://www.chiefarchitect.com/support/article/KB-00013/converting-plan-files-to-dwg-or-dxf.html
- Library file types (.calib/.calibz): https://chieftalk.chiefarchitect.com/topic/37920-where-do-i-place-calibz-files/
- Reference manuals: https://www.chiefarchitect.com/support/documentation.html
