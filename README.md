# HouseCraft

Residential design and construction documents in Rust: Windows, macOS, Linux and web,
with a headless CLI and an MCP server so LLMs can design alongside you.

See [docs/PLAN.md](docs/PLAN.md) for architecture and roadmap, and [docs/FEATURES.md](docs/FEATURES.md) for the
full feature backlog (parity target: Chief Architect Premier X18).
This is milestone **M0**: the document model, command API, native file format, CLI and MCP server.
There is no GUI or geometry yet.

## Crates

| Crate | What it does |
|---|---|
| `housecraft-units` | Lengths stored in mm. Parses `12'-6 1/2"`, `3.5 m`, `150mm`, etc. Formats feet-inches or metric |
| `housecraft-model` | `Document`: floors, layered wall types, walls; validation |
| `housecraft-api` | Serializable `Command`s, atomic transactions, undo/redo, queries, JSON Schema |
| `housecraft-io-native` | `.hcraft` files (zip of `manifest.json` + `document.json`, versioned) |
| `housecraft-mcp` | MCP server (stdio) exposing the API as tools |
| `housecraft-cli` | The `housecraft` binary |

Every front-end edits through `housecraft-api` commands, so anything the CLI or MCP can do, the GUI
will do the same way.

## CLI

```sh
housecraft new house.hcraft
housecraft apply house.hcraft ops.json     # or `-` for stdin
housecraft info house.hcraft
housecraft objects house.hcraft --kind wall
housecraft get house.hcraft 5
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
`get_object`, `apply_commands`, `undo`, `redo`.

## Building

Requires stable Rust (edition 2024). On Windows, install the Visual Studio Build Tools with the
"Desktop development with C++" workload, which provides the MSVC linker.

```sh
cargo test --workspace
cargo run -p housecraft-cli -- --help
```
