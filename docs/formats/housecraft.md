# The `.housecraft` file format

HouseCraft's native document format. MIME type `application/x-housecraft`.
Early builds wrote `.hcraft`; those files are identical inside and still open.

## Decision

A **ZIP container** with a versioned JSON manifest and a JSON document. This is the same pattern as
`.docx`, `.3mf`, `.sketch` and Chief Architect's own `.caproj`.

| Option | Why not (or why) |
|---|---|
| Single JSON/CBOR file | No room for binary assets (textures, 3D symbols, thumbnails) without base64 bloat |
| SQLite | Great for incremental saves of huge projects, but harder to inspect and diff, and it needs a SQLite build on the web target |
| Custom binary | Fast, but every reader (CLI, MCP, web, third parties) needs our parser; easy to corrupt silently |
| **ZIP + JSON** | Any tool can open it, payloads are human-readable, assets sit alongside, entries compress well, works everywhere including wasm |

If documents ever get big enough that JSON parsing time matters, `document.json` can become
`document.cbor` behind a format-version bump without changing the container.

## Layout (format version 1)

```
manifest.json      {"format": "housecraft", "format_version": 1, "generator": "housecraft 0.1.0"}
document.json      the Document: name, settings, next_id, objects (id-keyed, tagged by "kind")
assets/            (reserved) textures, 3D symbols; content-addressed file names
thumbnail.png      (reserved) preview for file browsers and the project browser
```

## Rules

- **Readers check `format` and refuse versions newer than they know** (`TooNew`). Older versions are upgraded
  by `migrate()` in `housecraft-io-native` before deserializing.
- **Writers save atomically**: write `name.housecraft.tmp`, then rename over the target.
- **Loading validates**: a document with dangling references or broken invariants is rejected, never
  half-loaded.
- **Lengths are millimetres as `f64`**, written with round-trip precision.
- Unknown extra ZIP entries are ignored, so future versions can add optional content without breaking
  older readers.
