# Chief Architect file formats: how we document them

Goal (milestone M2): open Chief Architect `.plan` files in HouseCraft, starting with floors, walls,
rooms, doors and windows, then widening coverage object type by object type.

## Ground rules

- **Interoperability only.** We read files our users created and own, so they can move their work.
  We do not ship Chief Architect content (library catalogs, textures, CAD details, sample plans).
- **Data first, code last.** Most of the format is learned from the *files*: container analysis plus
  differential samples. Analysing Chief Architect's executable is a last resort for specific
  questions the samples can't answer, done only with a licensed copy. Check the EULA and get legal
  advice before publishing a reader.
- **Spec, then code.** Findings go into [plan.md](plan.md) as prose and field tables. The reader in
  `crates/housecraft-io-ca` is written from that spec and never contains decompiled code.
- **Samples stay private.** Real `.plan` files go in `samples/` (git-ignored) and are never committed.
  Tests use small synthetic files built from the spec, plus checksums of private samples.

## Tooling

```sh
housecraft probe file.plan                  # container, zlib streams, entropy map, strings
housecraft probe file.plan --extract out/   # write every payload (ZIP entries / inflated streams)
housecraft probe-diff a.plan b.plan         # byte diff, payload by payload, numbers decoded
```

If the probe finds a ZIP or SQLite container, REA's archive and SQLite inspectors take over. If the
payload is a custom binary format, the differential samples below locate each field. REA plus Ghidra
on the executable is only needed for things like checksums or compression the probe can't undo.

## The sample corpus (highest-yield technique)

Make each of these in Chief Architect (Premier, or the free trial), **starting each from a fresh
default "New Plan"**, and save with the exact names below. Keep everything else unchanged so that
each pair differs by one thing.

| File | Make it by |
|---|---|
| `00-empty.plan` | New plan, save immediately |
| `00-empty-again.plan` | Same, saved a second time (reveals timestamps/IDs that change on every save) |
| `01-wall.plan` | One Straight Exterior Wall from (0,0) to (10',0) (use the wall's specification dialog for exact coordinates) |
| `02-wall-moved.plan` | `01` with the wall moved to start at (1",0) |
| `03-wall-longer.plan` | `01` with the wall 11' long |
| `04-wall-interior.plan` | `01` but drawn as an interior wall |
| `05-corner.plan` | Two exterior walls meeting at a corner |
| `06-room.plan` | Four exterior walls enclosing 10'×12' |
| `07-room-kitchen.plan` | `06` with the room type set to Kitchen |
| `08-door.plan` | `06` plus one hinged door, default size, centred on the bottom wall |
| `09-window.plan` | `06` plus one default window centred on the left wall |
| `10-floor2.plan` | `06` plus "Build New Floor" |
| `11-text.plan` | Empty plan plus text "HELLO" at (0,0) |
| `12-line.plan` | Empty plan plus a CAD line (0,0)–(10',0) |
| `13-dimension.plan` | `06` plus one manual dimension along the bottom wall |
| `14-ceiling.plan` | `06` with the floor's ceiling height changed to 9' |

Then add one or two real plans of yours, or Samples Gallery plans, as end-to-end targets.

Also note the **Chief Architect version and build** (Help > About) used to save them. The format
almost certainly changes between versions, so the reader keys on the version it finds in the file.
