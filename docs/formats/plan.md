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

## 2. Units and coordinates — open

Needs sample pairs `01`/`02`/`03` from [README.md](README.md).

## 3. Object records (walls, rooms, doors, windows, …) — open

The index and data region hold them, but locating a wall's coordinates needs the controlled samples
(one wall, then the same wall moved 1"). Large real plans have too many objects to isolate fields by inspection.

## 4. Next steps

1. Build the `.caproj` unpacker and the header/thumbnail/index/trailer parsers in
   `housecraft-io-ca` from sections 0–1.5. These are low risk, and the thumbnail and asset list are useful on their own.
2. Create the controlled sample corpus (requires Chief Architect or its trial) to decode units and wall records.
3. Compare the 2021 layout (no header record) once the 2025+ layout is understood.

## 5. Mapping to HouseCraft

| Chief Architect | HouseCraft command / object |
|---|---|
| Wall | `add_wall` + wall type mapped by name |
| Room | rooms are derived; map room type by name, keep custom name |
| Door / window | `add_door` / `add_window` with `position` along the wall |
| Linked textures (trailer / `Assets/`) | material assets, once materials exist (M5) |
