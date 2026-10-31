# HouseCraft

<img src="crates/housecraft-app/icons/128x128.png" align="right" width="96" alt="">

HouseCraft is a home design program written in Rust. You draw walls, rooms, doors, windows, stairs, roofs,
cabinets, framing and site plans in 2D. You see the house in 3D. Then you print construction documents:
plans, elevations, sections, schedules and layout sheets. It runs on Windows, macOS and Linux, with a
headless command line and an MCP server so AI assistants can design alongside you.

Its menus, toolbars and hotkeys follow the layout of Chief Architect, so people who know that program feel at
home. All code and icons are original.

## Install

Download the installer for your system from the
[Releases](https://github.com/camerontbelt/housecraft/releases) page:

| System | File | Notes |
|---|---|---|
| Windows 10/11 | `HouseCraft_x.y.z_x64-setup.exe` | Unsigned: on the SmartScreen warning click **More info › Run anyway** |
| macOS (Apple silicon) | `HouseCraft_x.y.z_aarch64.dmg` | Unsigned: the first time, right-click the app and choose **Open** |
| Debian / Ubuntu | `housecraft_x.y.z_amd64.deb` | `sudo apt install ./housecraft_*.deb` |
| Other Linux | `HouseCraft_x.y.z_x86_64.AppImage` | `chmod +x` it, then run it |

Each installer includes the desktop app (`housecraft-app`) and the `housecraft` command line. The installers
are built on GitHub by [the Release workflow](.github/workflows/release.yml). Pushing a `v*` tag publishes a
release.

## Status

Early but broad. Every **P0** item in the [feature backlog](docs/FEATURES.md) is in, along with much of P1–P3.
The test suite replays all 28 chapters of Chief Architect's tutorial guide as HouseCraft commands. At last count
that is 480 steps, with 68 steps still marked as gaps. Each chapter has a plain-language guide in
[docs/tutorials](docs/tutorials/README.md) that says where HouseCraft differs. The architecture and roadmap are
in [docs/PLAN.md](docs/PLAN.md).

## Crates

| Crate | What it does |
|---|---|
| `housecraft-units` | Lengths stored in mm. Parses `12'-6 1/2"`, `3.5 m`, `150mm`, etc. Formats feet-inches or metric |
| `housecraft-model` | `Document`: floors, layered wall types, walls; validation |
| `housecraft-api` | Serializable `Command`s, atomic transactions, undo/redo, queries, JSON Schema |
| `housecraft-io-native` | `.housecraft` files (zip of `manifest.json` + `document.json`, versioned) |
| `housecraft-geom` | Wall joins, room detection, opening cuts, triangulation, 3D meshes, transforms |
| `housecraft-draft` | Plan drawings as primitives; SVG, vector PDF and PNG output |
| `housecraft-app` | Desktop/web GUI (egui + wgpu): plan view, 3D view, tools, dialogs |
| `housecraft-mcp` | MCP server (stdio) exposing the API as tools |
| `housecraft-cli` | The `housecraft` binary |

Every front-end edits through `housecraft-api` commands, so anything the CLI or MCP can do, the GUI
will do the same way.

## Desktop app

```sh
cargo run -p housecraft-app --release -- path/to/plan.housecraft
```

The layout mirrors Chief Architect so users can switch without relearning. That covers the menu order, the
two-row toolbar, the Tool Palette, the Edit toolbar under the drawing, the Project Browser and Layer Display
side windows, Open Object specification dialogs, and the familiar hotkeys (Space, Tab, Ctrl+E, Shift+Q,
Shift+E, Shift+W, F6, F9...). Help > Keyboard Shortcuts lists them all. The icons are original artwork in
the same visual vocabulary.

Plans are saved as `.housecraft` files ([format](docs/formats/housecraft.md)). To make double-clicking them
open HouseCraft, use Edit > Preferences > File Types, or run `housecraft-app --register-file-type` (Windows
and Linux; per user, no admin rights). Installers built with cargo-packager register the type themselves,
which is the only way on macOS. You can also drag a plan onto the window.

Web: `trunk serve crates/housecraft-app/web/index.html` (needs [trunk](https://trunkrs.dev)). File
dialogs are desktop-only for now.

## CLI

```sh
housecraft new house.housecraft
housecraft apply house.housecraft ops.json     # or `-` for stdin
housecraft info house.housecraft
housecraft objects house.housecraft --kind wall
housecraft get house.housecraft 5
housecraft export house.housecraft -o plan.pdf --paper tabloid --scale 48
housecraft export house.housecraft -o plan.png  # or .svg
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
`libxkbcommon-dev`, `libwayland-dev` and `libx11-dev` for the GUI.

```sh
cargo test --workspace
cargo run -p housecraft-cli -- --help
```

To build the installers yourself, on the system you're packaging for:

```sh
cargo install cargo-packager --locked
cd crates/housecraft-app
cargo packager --release --formats nsis      # Windows; or deb,appimage on Linux, dmg on macOS
```
