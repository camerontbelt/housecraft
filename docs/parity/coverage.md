# Chief Architect parity audit

This compares HouseCraft with Chief Architect Premier X18 as its Reference Manual describes it: every menu command
and drawing tool named in the manual (about 460 menu paths and the tool lists of 39 "Tools" sections), grouped into
244 items by the manual's chapters. The item list with notes is [inventory.txt](inventory.txt); names are
Chief's menu and tool names, descriptions are ours. The manual itself is not kept in the repository.

FEATURES.md was built from Chief's marketing feature list, so it describes what Chief advertises. The manual also
covers the everyday commands that marketing lists skip (Fix Wall Connections, Edit Area, Tape Measure…), and that
is where most of the gaps below come from.

**Score** = (has + half of partly) / (everything not out of scope).

**Overall: 64%**: 101 has it, 102 partly, 33 missing, 8 out of scope.

## By chapter

| Chapter | Has | Partly | Missing | Out of scope | Score |
|---|---|---|---|---|---|
| File Management | 3 | 6 | 5 | 1 | 43% |
| Project Planning | 3 | 1 | 1 | 1 | 70% |
| Preferences and Default Settings | 3 | 3 | 0 | 0 | 75% |
| Toolbars and Hotkeys | 1 | 2 | 1 | 1 | 50% |
| Window and View Tools | 3 | 6 | 2 | 0 | 55% |
| Creating Objects | 0 | 2 | 1 | 0 | 33% |
| Displaying Objects | 5 | 2 | 0 | 0 | 86% |
| Positioning Objects | 2 | 1 | 0 | 0 | 83% |
| Editing Objects | 4 | 3 | 2 | 0 | 61% |
| CAD Objects | 8 | 6 | 3 | 0 | 65% |
| Walls, Railings, and Fencing | 4 | 8 | 3 | 0 | 53% |
| Rooms | 4 | 1 | 0 | 0 | 90% |
| Dimensions | 4 | 4 | 3 | 0 | 55% |
| Text, Callouts, and Markers | 2 | 3 | 1 | 0 | 58% |
| Doors | 3 | 1 | 0 | 0 | 88% |
| Windows | 2 | 3 | 0 | 0 | 70% |
| Cabinets | 5 | 1 | 0 | 1 | 92% |
| Electrical | 4 | 0 | 1 | 0 | 80% |
| Schedules and Object Labels | 0 | 3 | 0 | 0 | 50% |
| Foundations | 2 | 1 | 0 | 0 | 83% |
| Multiple Floor Levels | 1 | 1 | 0 | 0 | 75% |
| Stairs, Ramps, and Landings | 2 | 1 | 0 | 0 | 83% |
| Roofs | 2 | 6 | 0 | 0 | 62% |
| Framing | 6 | 2 | 1 | 0 | 78% |
| Trim and Moldings | 3 | 0 | 1 | 0 | 75% |
| The Library | 1 | 1 | 0 | 1 | 75% |
| Symbol Objects | 1 | 1 | 0 | 0 | 75% |
| Architectural Blocks | 1 | 0 | 0 | 0 | 100% |
| Other Objects | 1 | 2 | 0 | 0 | 67% |
| Materials | 3 | 1 | 2 | 0 | 58% |
| 3D Views | 5 | 2 | 0 | 1 | 86% |
| 3D Rendering | 2 | 4 | 0 | 0 | 67% |
| Pictures, Images, and Walkthroughs | 1 | 1 | 0 | 0 | 75% |
| Walkthrough Videos | 0 | 2 | 0 | 0 | 50% |
| Importing and Exporting | 1 | 6 | 2 | 2 | 44% |
| Terrain | 2 | 3 | 0 | 0 | 70% |
| Roads, Driveways, and Sidewalks | 2 | 0 | 0 | 0 | 100% |
| Plants and Sprinklers | 1 | 1 | 2 | 0 | 38% |
| Materials Lists | 1 | 2 | 1 | 0 | 50% |
| Layout | 2 | 3 | 0 | 0 | 70% |
| Printing and Plotting | 0 | 3 | 0 | 0 | 50% |
| Ruby in Chief Architect | 0 | 0 | 1 | 0 | 0% |
| Resources and Support | 0 | 1 | 0 | 0 | 50% |
| Tools menu (misc) | 1 | 2 | 0 | 0 | 67% |

## Missing

| Chapter | Chief item | FEATURES row | Note |
|---|---|---|---|
| File Management | Dashboard (start page: recent files, templates, news) | UI-24 | no start page |
| File Management | Save Thumbnail Image | — | portfolio has no thumbnails |
| File Management | View File Information (created/saved by, build, sizes) | — |  |
| File Management | Send (email the plan) | — | could open the mail client with the archive attached |
| File Management | File locking (plan open elsewhere) | — |  |
| Project Planning | Loan Calculator | — | small; low value |
| Toolbars and Hotkeys | Toolbar customization and toolbar sets | UI-19 |  |
| Window and View Tools | Rotate Plan View (view only, not the model) | — | plan can be rotated as a model edit only |
| Window and View Tools | Reference document offset (move the reference plan) | LY-07 | reference plan has no offset |
| Creating Objects | Get Last Symbol (place another of the last library item) | — |  |
| Editing Objects | Edit Area (and visible / all floors / polyline) | — | region select-and-transform across object types |
| Editing Objects | Stretch CAD | — |  |
| CAD Objects | Auto Detail (fill a polyline with a detail pattern) | SEC-06 |  |
| CAD Objects | Construction-line order management | — |  |
| CAD Objects | Fillet all / chamfer all corners | CAD-13 |  |
| Walls, Railings, and Fencing | Glass walls and glass pony walls | WL-02 | glass wall type possible; no tool |
| Walls, Railings, and Fencing | Room dividers / invisible walls | — |  |
| Walls, Railings, and Fencing | Fix Wall Connections | — | heal gaps and overlaps |
| Dimensions | Interior, baseline, centerline dimensions | DIM-01, DIM-10 |  |
| Dimensions | Tape Measure | — |  |
| Dimensions | Secondary (dual-unit) dimensions | DIM-12 |  |
| Text, Callouts, and Markers | Spell check | TXT-17 |  |
| Electrical | Ganged electrical blocks | — |  |
| Framing | Bearing lines | FR-27 | bearing walls only |
| Trim and Moldings | Replace Moldings (swap a profile plan-wide) | MAT-27 |  |
| Materials | Material Builder (custom tiles, grout) | MAT-13 |  |
| Materials | Delete surface (hide one face in 3D) | — |  |
| Importing and Exporting | 3D model export: DXF/DWG, 3DS, STL, COLLADA | IO-04 | no 3D model export yet |
| Importing and Exporting | Thermal envelope data export | IO-14 | energy export not started |
| Plants and Sprinklers | Plant chooser and hardiness zones | SITE-20 | M8c |
| Plants and Sprinklers | Sprinkler heads and lines | SITE-17 | M8c |
| Materials Lists | Materials list polyline (area take-off) | EST-01 |  |
| Ruby in Chief Architect | Ruby console and scripting | TXT-21, EST-07 | M8d own scripting |

## FEATURES.md corrections

- **WL-01** is marked done, but HouseCraft has no curved walls; only straight walls (with pony, raked and stepped
  shapes) exist. It should be ◐ until curved walls land (proposed row below).
- **DIM-01** is ◐ and right to be: interior, baseline and centerline dimensions are still missing.

## Proposed new FEATURES rows

Gaps that no existing row covers. IDs continue each area's numbering; milestone **M9** would be a new
"everyday tools" milestone after M8. Nothing here has been added to FEATURES.md yet.

| ID | Feature | Pri | M | From (manual chapter) |
|---|---|---|---|---|
| WL-20 | Curved walls (arc walls) with curved openings, railings and foundations | P1 | M9 | Walls, Railings, and Fencing |
| WL-21 | Room dividers / invisible walls that split rooms without 3D | P1 | M9 | Walls, Railings, and Fencing |
| WL-24 | Break Wall tool: split a wall at a clicked point | P1 | M9 | Walls, Railings, and Fencing |
| CAD-23 | Typed CAD input (Input Point/Line/Arc dialogs with distance and angle) | P1 | M9 | CAD Objects |
| UI-36 | Edit toolbar: context buttons for the current selection | P1 | GUI | Toolbars and Hotkeys |
| WL-22 | Fix Wall Connections: close small gaps and join overlapping walls | P2 | M9 | Walls, Railings, and Fencing |
| WL-23 | Glass wall and glass pony wall tools | P2 | M9 | Walls, Railings, and Fencing |
| CAD-21 | Edit Area: select a region and move/copy/delete everything in it (this floor, all floors) | P2 | M9 | Editing Objects |
| DIM-24 | Tape Measure tool (measure without placing a dimension) | P2 | M9 | Dimensions |
| DIM-25 | Delete Dimensions (automatic, manual or all, by floor) | P2 | M9 | Dimensions |
| FD-06 | Slab and slab-hole drawing tools (with footing option) | P2 | M9 | Foundations |
| PRJ-35 | Delete Current Floor command | P2 | M9 | Multiple Floor Levels |
| RF-15 | Manual ceiling plane tool and Edit All Roof Planes dialog | P2 | M9 | Roofs |
| RF-16 | Roof hole tool (cut a roof plane without a skylight) | P2 | M9 | Roofs |
| VIS-30 | Floor overview (one floor in 3D) in perspective and orthographic | P2 | M9 | 3D Views |
| UI-30 | Dashboard start page (recent and pinned plans, templates, new plan) | P2 | GUI | File Management |
| UI-32 | Tile views side by side; swap views; next/previous tab | P2 | GUI | Window and View Tools |
| UI-33 | View toggles: crosshairs, arc centres, line weights on screen, drawing sheet | P2 | GUI | Window and View Tools |
| UI-34 | Rotate Plan View (rotate the screen, not the model) | P2 | GUI | Window and View Tools |
| PRJ-38 | File locking: warn when a plan is open elsewhere | P2 | M9 | File Management |
| LY-10 | Current CAD Layer setting for new CAD objects | P2 | M9 | CAD Objects |
| EST-24 | Materials List Polyline: take-off for a drawn area | P2 | M9 | Materials Lists |
| PRJ-41 | Layout pages: duplicate, exchange, insert before/after | P2 | M9 | Layout |
| PRJ-42 | Referenced Files dialog: list and relink a layout's plans | P2 | M9 | Layout |
| PRJ-43 | Missing Files check with relink | P2 | M9 | Tools menu (misc) |
| CAD-26 | Delete Objects dialog in plans (by type, floor, room) | P2 | M9 | Editing Objects |
| CAD-22 | Stretch CAD: drag a fence to stretch lines and polylines | P3 | M9 | Editing Objects |
| CAD-24 | Construction lines (infinite guide lines) with order management | P3 | M9 | CAD Objects |
| CAD-25 | Edit behaviours: rotate/resize about a chosen current point; connect CAD segments | P3 | M9 | Positioning Objects |
| SEC-13 | Auto Detail: fill a drawn area with a section detail pattern (insulation, blocking) | P3 | M9 | CAD Objects |
| EL-09 | Ganged electrical blocks (outlets/switches combined in one box) | P3 | M9 | Electrical |
| MAT-30 | Replace Moldings: swap one molding profile for another plan-wide | P3 | M9 | Trim and Moldings |
| VIS-29 | Delete Surface: hide one face in a 3D view | P3 | M9 | Materials |
| VIS-31 | Create Image: save the current 3D view as a placeable image object/library item | P3 | M9 | Other Objects |
| VIS-32 | Stationary walkthroughs (look-around from one spot) | P3 | M9 | Walkthrough Videos |
| UI-31 | Close All Views / Close All 3D Views | P3 | GUI | File Management |
| UI-35 | Get Last Symbol: place another of the last library item | P3 | GUI | Creating Objects |
| UI-37 | Help menu: open manual, report a problem (logs, system information) | P3 | GUI | Resources and Support |
| PRJ-36 | Plan thumbnail saved in the file (used by portfolio and open dialog) | P3 | M9 | File Management |
| PRJ-37 | File Information: created/saved dates, program version, object counts | P3 | M9 | File Management |
| PRJ-39 | Send: email the plan (archive) from the File menu | P3 | M9 | File Management |
| PRJ-40 | Loan calculator | P3 | M9 | Project Planning |
| LY-11 | Fill Style Painter tool | P3 | M9 | Displaying Objects |
| LY-12 | Offset (move) a reference plan file relative to this one | P3 | M9 | Window and View Tools |
| CAD-27 | Paste Special: choose which attributes to paste | P3 | M9 | Editing Objects |
| CAD-28 | Bumping/pushing: objects push neighbours instead of overlapping | P3 | M9 | Creating Objects |

## Items to check by hand

A few lines are judgement calls from reading the manual and our code and are worth a second look:
Dynamic defaults (how far "follow the defaults until edited" goes), arc creation modes, pass-through windows,
and the half-wall versus railing-wall split. The X18 changelog was read too; it is mostly fixes to existing
tools, and its new features were already in FEATURES.md from the feature list.

## Next

1. Workflow tests: replay the Tutorial Guide's projects as scripted command batches with checks on the results,
   so "has it" means the feature works in a real sequence, not just that it exists.
2. Drawing comparison: rebuild the sample projects in `samples/` and compare plans, elevations and 3D views with
   their published sheets.
3. Decide which proposed rows to adopt into FEATURES.md (and whether M9 "everyday tools" becomes a milestone).
