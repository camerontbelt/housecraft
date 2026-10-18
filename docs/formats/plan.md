# `.plan` format specification (working draft)

Status: **not started.** Waiting on the sample corpus described in [README.md](README.md).
Every statement below must cite the sample pair or probe result it came from, and say whether it
is an *observation* (seen in bytes) or an *inference* (a guess that fits the observations).

## 1. Container

| Question | Answer | Evidence |
|---|---|---|
| Magic bytes / header | ? | `housecraft probe` head |
| Container type (ZIP / SQLite / zlib / custom) | ? | probe `container`, entropy map |
| Compression | ? | zlib streams found |
| Version field location and values seen | ? | diff of files saved by different versions |
| Checksums or signatures | ? | does a 1-byte edit still open in Chief Architect? |
| Bytes that change on every save | ? | `00-empty` vs `00-empty-again` |

## 2. Units and coordinates

- Length unit (inches? mm?), number type (f32/f64/fixed), coordinate origin and axis directions.
- Evidence: `01` vs `02` (1" move), `01` vs `03` (1' length change).

## 3. Object records

For each object type: how records are delimited, the type tag, the id, and a field table.

### Wall

| Offset / tag | Type | Meaning | Evidence |
|---|---|---|---|
| | | | |

### Room, Floor, Door, Window, Text, CAD line, Dimension

(Same table per type.)

## 4. References

How objects point at each other (wall → wall type, door → wall, object → layer, object → floor).

## 5. Mapping to HouseCraft

| Chief Architect | HouseCraft command / object |
|---|---|
| Wall | `add_wall` + wall type mapped by name |
| Room | rooms are derived; map room type by name, keep custom name |
| Door / window | `add_door` / `add_window` with `position` along the wall |
