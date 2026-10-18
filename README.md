# HouseCraft

Residential design and construction documents in Rust: Windows, macOS, Linux and web,
with a headless CLI and an MCP server so LLMs can design alongside you.

See [docs/PLAN.md](docs/PLAN.md) for architecture and roadmap, and [docs/FEATURES.md](docs/FEATURES.md) for the
full feature backlog (parity target: Chief Architect Premier X18).
Current state: every **P0** backlog item is implemented (a few partially; see the backlog). That covers the
desktop app (plan and 3D views), walls, rooms, doors and windows, CAD drafting, dimensions, text, layers,
saved views and cameras, PDF/SVG/PNG output, the CLI and the MCP server.

## Crates

| Crate | What it does |
|---|---|
| `housecraft-units` | Lengths stored in mm. Parses `12'-6 1/2"`, `3.5 m`, `150mm`, etc. Formats feet-inches or metric |
| `housecraft-model` | `Document`: floors, layered wall types, walls; validation |
| `housecraft-api` | Serializable `Command`s, atomic transactions, undo/redo, queries, JSON Schema |
| `housecraft-io-native` | `.hcraft` files (zip of `manifest.json` + `document.json`, versioned) |
| `housecraft-geom` | Wall joins, room detection, opening cuts, triangulation, 3D meshes, transforms |
| `housecraft-draft` | Plan drawings as primitives; SVG, vector PDF and PNG output |
| `housecraft-app` | Desktop/web GUI (egui + wgpu): plan view, 3D view, tools, dialogs |
| `housecraft-mcp` | MCP server (stdio) exposing the API as tools |
| `housecraft-cli` | The `housecraft` binary |

Every front-end edits through `housecraft-api` commands, so anything the CLI or MCP can do, the GUI
will do the same way.

## Desktop app

```sh
cargo run -p housecraft-app --release -- path/to/plan.hcraft
```

The layout mirrors Chief Architect so users can switch without relearning. That covers the menu order, the
two-row toolbar, the Tool Palette, the Edit toolbar under the drawing, the Project Browser and Layer Display
side windows, Open Object specification dialogs, and the familiar hotkeys (Space, Tab, Ctrl+E, Shift+Q,
Shift+E, Shift+W, F6, F9...). Help > Keyboard Shortcuts lists them all. The icons are original artwork in
the same visual vocabulary.

Web: `trunk serve crates/housecraft-app/web/index.html` (needs [trunk](https://trunkrs.dev)). File
dialogs are desktop-only for now.

## CLI

```sh
housecraft new house.hcraft
housecraft apply house.hcraft ops.json     # or `-` for stdin
housecraft info house.hcraft
housecraft objects house.hcraft --kind wall
housecraft get house.hcraft 5
housecraft export house.hcraft -o plan.pdf --paper tabloid --scale 48
housecraft export house.hcraft -o plan.png  # or .svg
housecraft schema                          # JSON Schema for command batches
housecraft mcp                             # MCP server on stdin/stdout
```

Example `ops.json`. It draws a closed 20' × 30' box of exterior walls on the first floor:

```json
[
  { "op": "add_walls", "points": [[0, 0], ["20'", 0], ["20'", "30'"], [0, "30'"]], "closed": true }
]
```

## MCP

Register the binary as a stdio MCP server, for example in Claude Code:

```sh
claude mcp add housecraft -- housecraft mcp
```

Tools: `new_document`, `open_document`, `save_document`, `document_summary`, `list_objects`,
`get_object`, `apply_commands`, `undo`, `redo`, `view_plan` (returns a PNG so the model can see its
work), `export_plan`.

## Building

Requires stable Rust (edition 2024). On Windows, install the Visual Studio Build Tools with the
"Desktop development with C++" workload, which provides the MSVC linker. If several Visual Studio versions are
installed, build from a shell where the Build Tools' `vcvars64.bat` has run. On Linux, install
`libxkbcommon-dev` and `libwayland-dev` for the GUI.

```sh
cargo test --workspace
cargo run -p housecraft-cli -- --help
```
