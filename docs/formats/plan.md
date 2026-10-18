# `.plan` format specification (working draft)

Status: **top-level layout established from three public sample plans; object fields not decoded.**
Every statement cites its evidence and is marked **[obs]** (seen in bytes) or **[inf]** (an inference
consistent with the observations, still to be confirmed).

## Samples analysed (2026-10-18)

Downloaded from Chief Architect's public Samples page into the git-ignored `samples/` folder:

| Sample | Container | `.plan` inside | Plan bytes | Format version (header u16) |
|---|---|---|---|---|
| `chic-cottage.zip` (2021) | ZIP | `Chic Cottage/Chic Cottage.plan` | 40,238,650 | `0x09A2` |
| `hawksnest-kitchen.caproj` (2025) | ZIP | `Plans and Layouts/Hawksnest-kitchen.plan` | 36,683,543 | `0x0F7F` |
| `curran-accessible.caproj` (2026) | ZIP | 3 plans + 1 layout | 8,062,565 (Bath-Sink) | `0x101A` |

Offsets below are from `Curran-Accessible-Bath-Sink.plan` unless stated.

## 0. `.caproj` project archive

- **[obs]** A `.caproj` is a standard ZIP. It holds `manifest.json`, an `Assets/` folder of textures,
  PDFs and images, and `Plans and Layouts/*.plan|*.layout`.
- **[obs]** `manifest.json` is plain JSON with `manifest_version: 5`, `schema_version: 13`,
  `filename_encoding: "Windows-1252"`, `original_export_path`, and `parent_child_content_connections`.
  The last maps library "handles" (original paths) to asset file names.
- **[inf]** Opening a `.caproj` means unzipping it, mapping assets by name, then reading each `.plan`.
  Of everything here, this is the easy, well-understood part.

## 1. File layout (2025–2026 format)

```
0x0000  file header record          (01 CA <ver>, offsets into the trailer)
0x0036  thumbnail chunk             (01 CA <ver>, PNG image)
0x45C2  index table                 (u32 count, then count+1 key/value records)
0x49EB  data region                 (binary records, zero-filled areas, text geometry, ...)
0x7AE7E3 trailer                    (root folder record + external resource table) to EOF
```

### 1.1 Chunk header

- **[obs]** `01 CA` magic, `u16` format version (little-endian), `u16` type (always `0` so far), `u32` length.
- **[obs]** For the thumbnail chunk the length excludes the 10-byte header: the PNG runs from +10 for exactly
  `length` bytes, ending with `IEND` and its CRC.
- **[obs]** For the file-header record (first chunk, 2025+), the length (`0x36` / `0x1E`) equals the offset of the
  next chunk, i.e. it *includes* the header.
- **[obs]** Version values rise with sample date: `0x09A2` (2021), `0x0F7F` (2025), `0x101A` (2026).
  **[inf]** This is a format/build number that a reader must key on.
- **[obs]** Only two genuine chunk headers exist per file (header, thumbnail). Other `01 CA` byte pairs are coincidental.

### 1.2 File header record

| Field | 2025 (`0x0F7F`, u32) | 2026 (`0x101A`, u64) | Meaning |
|---|---|---|---|
| A | 36,668,081 | 8,054,755 | **[obs]** offset of the trailer |
| B | 36,668,111 (A + 30) | 8,054,809 (A + 54) | **[obs]** offset of the resource table inside the trailer |
| C | 14,540 | 7,755 | **[obs]** = (D − B): resource table length |
| D | 36,683,542 | 8,062,564 | **[obs]** = file size − 1 |
| E | 1 | 1 | unknown (count or flag) |

- **[obs]** The offsets widened from 32-bit (2025) to 64-bit (2026).
- **[obs]** The 2021 file has no header record: it starts directly with the thumbnail chunk.
  **[inf]** The trailer/offset scheme was added between 2021 and 2025.

### 1.3 Index table

- **[obs]** At the end of the thumbnail: `u32` count (17), followed by records of
  `[u32 key_len][key][u8 flag][u32 value_len][value][u8 flag]`. The walker parsed 18 records
  (count + 1) cleanly, ending at `0x49EB`.
- **[obs]** Key lengths are 24 or 16 bytes (one is 1 byte). 24-byte keys share 8-byte prefixes in a few
  groups (`f63306beb15ed00e`, `fdc18e969f9539a9`, `fc04886e936f33b9`), followed by 16 random-looking bytes.
  **[inf]** The prefix is a type or class ID and the rest an object GUID.
- **[obs]** Values are 8–104 bytes. Some contain values like `0x007A80E8` that point inside the file.
  **[inf]** The index maps named blocks to their location in the data region.

### 1.4 Data region

- **[obs]** Mostly low-to-medium entropy (structured, uncompressed binary), with zero-filled spans and some
  high-entropy blocks.
- **[obs]** `0x7A80E8` (pointed at by an index value) holds ASCII text geometry: tokens such as `Wi`, `Fa`, `Sh`,
  bit strings like `0101100`, and signed coordinate lists (`+28 0 -11 0 +8 0 *`).
  **[inf]** This is a boundary-representation text format for 3D solids or symbols (wire, face, shell).
- **[obs]** Contains 2 PNG signatures and about 20 zlib-header byte pairs; the probe inflated only one
  (an ICC colour profile). No JPEG, gzip, zstd, LZ4 or ZIP signatures.

### 1.5 Trailer

- **[obs]** Starts with a record holding a GUID and the string `"Root Folder"`.
- **[obs]** The resource table is a sequence of entries: a GUID, padding, then a length-prefixed absolute path to an
  external file (`.../Assets/BrushedMetalBump_CAAB.jpg`, `.../K-6304_spec.pdf`, `.../Thermostat Screen.png`),
  running to the end of the file.
  **[inf]** These are the plan's linked textures and documents. In a `.caproj` they resolve to `Assets/<name>`.

## 2. Units and coordinates — partly decoded

**[obs]** Lengths are IEEE-754 little-endian `f64` values in **inches**, and they are not aligned to 8 bytes.
Between two undo snapshots that differ only by one "Edge Move" of a wall, a single `f64` changed
from 458.4737 to 466.4737 (+8"). The next two moves took it to 521.4737 and then -2.5263.
**[inf]** That value is one wall end's X coordinate.

## 2a. Undo snapshots: the sample source that works with the trial

The X18 trial can't save plans, but every edit writes a snapshot to
`%TEMP%\undo_<project>_<pid>_<n>.undoplan`. Each snapshot is the plan state *before* the action it names.

| Offset | Size | Field |
|---|---|---|
| 0 | u32 | length, 28 |
| 4 | 28 | `Chief Architect Undo Archive` |
| 32 | u8 | 0 |
| 33 | u16 | version, `0x101A` (same as X18 `.plan`) |
| 35 | u16 | 0 |
| 37 | u32 | action name length *n* |
| 41 | *n* | action name, e.g. `EditBehavior: Create Wall`, `EditBehavior: Edge Move` |

**[obs]** 184 of 222 sampled 48-byte windows of the empty-plan snapshot occur verbatim in the archived empty
`.plan`, so the object encoding is the `.plan` data region's. Snapshots are about 100 KB, not 7.6 MB, because they
omit thumbnails and library resources. Snapshot pairs are therefore one-change diffs:
make a change, then any other undoable edit, and the second edit's snapshot holds the first edit's result.
Chief probably deletes them on exit, so copy them out of `%TEMP%` while it's running.

Corpus so far (`samples/x18/undo/`, git-ignored; each row is the state before the named action):

| Snapshots | State |
|---|---|
| 1 | empty plan |
| 2–6 | one 10' wall, before a dimension move and four edge moves |
| 7 | before a second wall |
| 10–14 | both walls, moved apart and joined again; the first wall ends at x 2", y 0 |
| 15 | before a door |
| 16 | door added |
| 17 | window added |
| 18, 21, 24, 27 | a separate rectangle of walls being drawn; 27 has it closed (room created) |
| 8–9, 19–20, 22–23, 25–26 | tiny "Modify Lights" / "Modify Construction Lines" bookkeeping (~2 KB) |

**[obs]** In snapshot 2 the wall appears to be stored in at least three places (offsets ~1400, ~29260, ~56550).
Near 29260 are the doubles 120.0 (the 10' length), 109.125 (Chief's default 9'-1 1/8" wall height, twice
per entry), and four 89-byte entries each holding a pair such as (-3.4737, 123.4737). **[inf]** These are
corner or extent records of the wall's 3D faces; they are not yet tied to fields.

## 3. Object records (walls, rooms, doors, windows, …) — walls decoded

Tooling: `housecraft probe-diff A B` now prints each changed `f64` (old → new), and
`housecraft ca-walls FILE` lists wall records (`housecraft_io_ca::plan`).

**[obs]** Every undo snapshot holds two copies of the plan: the state before its action, then the state after it.
In snapshot 27 ("before Edge Move") the first copy has the rectangle's bottom wall at y 283.75 and the second at
y 265.75. Coordinates that "lag" by one snapshot (section 2) are the before copy.

### 3.1 Wall record

Located by shape (the surrounding object framing isn't understood yet). Offsets are relative to the start point:

| Offset | Type | Field | Evidence |
|---|---|---|---|
| +0x00, +0x08 | f64 ×2 | start x, y (inches) | point-to-point move took (2, −2.53) → (−116, 38.47) |
| +0x10, +0x18 | f64 ×2 | unit direction | (1, 0) and (0, 1) / (0, −1) / (−1, 0) on the four rectangle sides |
| +0x20 | f64 | length | 120 for the 10' wall; start + direction × length = the other wall's start where walls join |
| +0x71, +0x79 | f64 ×2 | first layer entry: heights | 109.125 (9'-1 1/8") for default walls; 113.73 after a room formed; 0 in one joined wall |
| +0x81, +0x89 | f64 ×2 | first layer entry: span along the wall | starts at −thickness/2: −3.474 (6.9475" exterior), −2.25 (4.5" interior) |

**[obs]** Layer entries repeat with an 89-byte (0x59) stride, four for the default exterior wall, each repeating the
span. Walls in a closed rectangle are stored clockwise around their interior (end of one = start of the next).
**[open]** wall type reference, per-layer materials, which side is exterior, the record header/id.

### 3.2 Door and window records — hypotheses

**[obs]** Both kinds share a run of five `f64`s:

| | f64 0 | f64 1 | f64 2 | f64 3 | f64 4 |
|---|---|---|---|---|---|
| door (snapshot 15) | 117.24 | 30 | 0.625 | 80 | 80 |
| window (snapshot 16) | 56.707 | 32 | 0.625 | 48 | 80 |

**[inf]** f64 0 is the position along the host wall (the door's 117.24 exceeds the horizontal wall's 116.89, so it is
on the vertical wall); f64 4 is the header height (6'-8"); f64 3 is the height. f64 1 is the width, or for the
window possibly the sill height (80 − 48 = 32). The string "BM11 - Base Molding" sits nearby (casing / molding
settings). Needs the actual sizes from Chief's door and window specification dialogs to confirm.

## 4. Next steps

1. Build the `.caproj` unpacker and the header/thumbnail/index/trailer parsers in
   `housecraft-io-ca` from sections 0–1.5. These are low risk, and the thumbnail and asset list are useful on their own.
2. Grow the undo-snapshot corpus (section 2a) to decode wall, door, window and room records.
3. Compare the 2021 layout (no header record) once the 2025+ layout is understood.

## 5. Mapping to HouseCraft

| Chief Architect | HouseCraft command / object |
|---|---|
| Wall | `add_wall` + wall type mapped by name |
| Room | rooms are derived; map room type by name, keep custom name |
| Door / window | `add_door` / `add_window` with `position` along the wall |
| Linked textures (trailer / `Assets/`) | material assets, once materials exist (M5) |
