# HouseCraft feature backlog

This is the parity target. It is derived from Chief Architect's *X18 Detailed Feature List* (Premier, July 2026),
paraphrased into short engineering items, and it supersedes the rougher inventory in [PLAN.md §2](PLAN.md#2-feature-inventory-from-the-premier-product-page-and-the-x17-changelog).
Source: https://cloud.chiefarchitect.com/1/pdf/marketing/x18-detailed-feature-list.pdf

**Pri:** P0 MVP · P1 credible v1 · P2 parity · P3 later/optional · ✗ out of scope (vendor-specific service).
**P0 status (2026-10-17):** all 29 P0 items are implemented; five are partial (◐) as noted in their rows.
**M3 status (2026-10-18):** foundations (incl. piers), roofs (hip, gable, shed, mansard, gambrel, Dutch gable),
ceilings (flat, vaulted, tray, coffered, barrel), layered platforms, stairs and railings, door and window types,
electrical/plumbing/HVAC symbols, remodel phases, reference floors, north and sun with 3D and plan shadows are in.
Of the M3 P1 items, 13 are done (✓) and 11 partial (◐); what each partial one lacks is noted in its row, and
several wait on later milestones (library styles in M5, terrain and framing in M6). The M3 P2/P3 items are not started.
**M4 status (2026-10-18):** schedules with export, materials list with prices, elevations and cross sections
(auto exterior and room elevations, plan callouts), layouts with live views and a title block exported to
multi-page PDF, callouts, markers, revision clouds, detailing shapes, text macros and a 3D cutaway are in.
Of the M4 P1 items, 14 are done (✓), 15 partial (◐) and 0 not started. CAD details, blocks with a
user library, drafting in elevation/section views and camera default sets were added in slice 6.

**M:** roadmap milestone from [PLAN.md §5](PLAN.md#5-roadmap-vertical-slices). `GUI`/`web` = front-end work, not
tied to a model milestone. `✓` = done. `(X18+)` marks a feature new in X18.

There are 434 items: 29 P0, about 120 P1, about 205 P2, about 72 P3, and 5 out of scope. Each item should eventually map to
`housecraft-api` commands so that it is scriptable from the CLI and MCP.

---

## 1. Cross-cutting design implications

These come from reading the whole list and change the architecture, not just the backlog:

1. **Default Sets everywhere.** Nearly every object type has defaults (room, wall, door, cabinet, stair,
   dimension, text, camera, framing…). Defaults are swappable as named *sets* per view.
   → The model needs a first-class `Defaults` system: per-object-type defaults, named default sets and per-view active sets.
2. **Layers and Layer Sets drive everything.** Visibility, printing, materials-list inclusion, fill styles, and line styles
   all flow from layers, and saved plan views pick a layer set. → Layers belong in the model core (M1), not as an add-on.
3. **Saved views are documents.** Plan views, cameras, elevations, sections, and CAD details are named, saved,
   copyable objects organized in a Project Browser. → Add a `View` object kind early.
4. **Auto-rebuild with manual override.** Roofs, framing, foundations, dimensions, and fillers regenerate as the design
   changes, and manual edits are pinned ("Manual" flag). → Generators must be idempotent and must respect a per-object `manual` flag.
5. **Scoping.** Painters, material edits, and replace-from-library all apply at *object / room / floor / selection / plan*
   scope. → A generic `Scope` selector belongs in the command API.
6. **Library = catalogs of parametric + symbol objects.** Everything can be saved to and loaded from the library,
   including walls, stairs, schedules, notes, and architectural blocks. → The library is a document store of serialized
   model fragments. That fits the `.housecraft` and `Command` design well.
7. **Object Information Panel.** Custom fields on every object feed schedules and the materials list.
   → Generic `attributes: Map<String, Value>` on all objects.
8. **Ruby scripting and text macros** (materials-list formulas, labels). We use **Rhai or Lua** with the same
   API surface. A Ruby-compatible macro layer is P3, only if `.plan` files embed Ruby macros we have to evaluate.
9. **Home Designer compatibility.** CA opens Home Designer plans, so the formats are related. A Home Designer sample
   is a useful extra RE corpus.
10. **File types seen:** `.plan`, `.layout`, `.calib`/`.calibz`, `.CAPROJ` (project archive), `.EZF` (EasyFrame panel export).

---

## 2. 3D rendering & visualization

| ID | Feature | Pri | M |
|---|---|---|---|
| VIS-01 | Perspective and orthographic cameras; named, saved, editable, copyable | P0 | M1 ✓ |
| VIS-02 | XYZ axes indicator and project-origin marker in 2D/3D | P1 | M1 |
| VIS-03 | Navigation: orbit, pan, dolly, tilt, walk; standard view-direction presets | P0 | M1 ✓ (orbit, pan, dolly, walk, presets) |
| VIS-04 | Auto-hide camera-facing exterior walls while navigating small rooms | P2 | M7 |
| VIS-05 | Cross-section slider: interactive cutaway from any side, live | P1 | M4 ✓ |
| VIS-06 | Poché (solid fill) for walls/roofs/platforms in plan and camera views | P1 | M1 |
| VIS-07 | Walkthrough recording along spline paths; per-keyframe direction/speed; multi-floor; per-path lighting | P2 | M7 |
| VIS-08 | Walkthrough editing: keyframe editing, join/break segments, pauses, multi-select | P2 | M7 |
| VIS-09 | Automatic 360° exterior orbit walkthrough | P2 | M7 |
| VIS-10 | Cloud share walkthroughs | ✗ | — |
| VIS-11 | Sun/shadow time-lapse (date/time sequences, timestamp overlay) | P2 | M7 |
| VIS-12 | 360° spherical render export (up to 8192×4096) | P2 | M7 |
| VIS-13 | Sky model: sun, moon, horizon lighting controls per camera | P2 | M7 |
| VIS-14 | Custom image backdrops per saved camera | P1 | M5 |
| VIS-15 | Spherical (360°) backdrops with offset/orientation/eye level | P2 | M7 |
| VIS-16 | HDR backdrops providing environment lighting | P2 | M7 |
| VIS-17 | HDR image export | P2 | M7 |
| VIS-18 | Extend terrain to horizon in 3D | P2 | M6 |
| VIS-19 | Procedural grass regions (length, density, color) | P3 | M7 |
| VIS-20 | Render techniques: standard, PBR, watercolor, squiggle line, technical illustration, clay, vector, glass house, duotone | P1→P2 | M5/M7 |
| VIS-21 | Line-drawing overlay on standard/duotone/watercolor/PBR/clay | P2 | M7 |
| VIS-22 | GPU path-traced PBR with soft shadows | P2 | M7 |
| VIS-23 | Denoising for ray-traced views | P2 | M7 |
| VIS-24 | Depth of field (focus point, f-stop) | P3 | M7 |
| VIS-25 | Camera default sets (e.g. dimension defaults for elevations vs sections) | P1 | M4 ✓ |
| VIS-26 | Eight standard isometric angles | P1 | M1 |
| VIS-27 | Shadow intensity control for vector/technical views | P2 | M7 |
| VIS-28 | Depth cueing (distance fog) | P3 | M7 |

## 3. Cross sections & elevations

| ID | Feature | Pri | M |
|---|---|---|---|
| SEC-01 | Cross-section, elevation, and back-clipped section views; side clipping | P1 | M4 ◐ elevations, cross sections, depth (back) clipping and side clipping; cut poché fill later |
| SEC-02 | Convert wall elevations to section views; expand/crop extents | P2 | M4 |
| SEC-03 | Vaulted ceilings shown in wall elevations; "ignore walls above" crop | P2 | M4 |
| SEC-04 | Stepped (jogged) cutting planes | P2 | M4 |
| SEC-05 | Below-grade items as hidden lines | P2 | M4 |
| SEC-06 | Full drafting/dimensioning in section & elevation views; auto-detail tool | P1 | M4 ◐ CAD, dimensions, text, callouts and blocks drawn in elevation/section views; auto-detail tool later |
| SEC-07 | Center CAD objects in elevation | P3 | M4 |
| SEC-08 | Copying a section camera copies its annotations | P2 | M4 |
| SEC-09 | Cameras shown as callouts in plan (labels, descriptions; survive to layout) | P1 | M4 ◐ elevation/section callouts in plan; 3D camera callouts later |
| SEC-10 | Callouts auto-filled with layout page/label info | P2 | M4 |
| SEC-11 | Automatic schedule labels in camera views | P2 | M4 |
| SEC-12 | Auto-generate exterior and interior room elevations | P1 | M4 ✓ |

## 4. Plans, layouts & project management

| ID | Feature | Pri | M |
|---|---|---|---|
| PRJ-01 | Plan templates; save-as-template | P1 | M1 |
| PRJ-02 | Recent files with pinning | P2 | M1 |
| PRJ-03 | Space-planner room boxes: drag room bubbles, convert to walls | P2 | M3 |
| PRJ-04 | Plan Check: egress, roof, wall layout review | P2 | M4 |
| PRJ-05 | Rotate and mirror (flip) entire plan | P1 | M1 |
| PRJ-06 | Zoom to selection | P0 | M1 ✓ |
| PRJ-07 | Free viewer app for clients (read-only) | P1 | web build |
| PRJ-08 | Managed projects (auto-organized storage, no broken links) | P2 | M8 |
| PRJ-09 | Project Browser: tree of layouts, floors, views, cameras, schedules, details; multi-select; folders; thumbnails | P1 | M4 ◐ floors, plan views, cameras, layouts and pages, elevations, rooms; folders, thumbnails, multi-select later |
| PRJ-10 | Pin items in Project Browser (X18+) | P3 | M8 |
| PRJ-11 | Saved Plan Views (layer set + default set + active defaults; zoom/floor optional); tabs/windows | P0 | M1 ✓ (multiple view tabs: later) |
| PRJ-12 | Duplicate saved view copies its active layer set (X18+) | P2 | M1 |
| PRJ-13 | Default Sets per drawing type (kitchen, framing, electrical…) | P1 | M1 |
| PRJ-14 | Multi-floor structures | P0 | M0 ✓ |
| PRJ-15 | Insert/exchange floors above/below | P1 | M3 ✓ |
| PRJ-16 | Layers: create, assign, style, group into Layer Sets | P0 | M1 ✓ |
| PRJ-17 | Fill style by layer (X18+) | P2 | M1 |
| PRJ-18 | Select all on layer | P1 | M1 |
| PRJ-19 | Time tracker per project/client/task with export | P3 | M8 |
| PRJ-20 | Plan database / portfolio | P3 | M8 |
| PRJ-21 | R-values for walls, roofs, ceilings, platforms | P2 | M3 |
| PRJ-22 | Client & designer info with macros and custom fields | P2 | M4 |
| PRJ-23 | Layout files: sheets with scaled, live-linked views; title-block templates; reorder pages | P1 | M4 ◐ sheets in the plan file with live scaled views, title block, several pages; separate .layout files, title-block templates, page reordering later |
| PRJ-24 | Layout viewports: polyline cropping with breaks/curves; pan/zoom; rotate | P1 | M4 ◐ rectangular crop with a set centre; polyline crops, in-box pan/zoom, rotation later |
| PRJ-25 | Layout box spec: defaults, multi-line labels, relink, rescale, change layer set; multi-edit | P1 | M4 ◐ position, size, scale, caption, notes text; relink, change layer set and multi-edit later |
| PRJ-26 | Multiple layout files open at once; copy between them | P2 | M4 |
| PRJ-27 | Edit line views on layout; rescale; relink | P2 | M4 |
| PRJ-28 | Multiple pages per layout | P1 | M4 ✓ |
| PRJ-29 | Per-page template pages | P2 | M4 |
| PRJ-30 | Revision tracking and revision tables | P2 | M4 |
| PRJ-31 | Sheet index / table of contents from page attributes | P2 | M4 |
| PRJ-32 | Page macros (label, auto-numbering, title) | P2 | M4 |
| PRJ-33 | Bulk delete by object type in layouts | P3 | M4 |
| PRJ-34 | Home Designer plan share | ✗ | — |

## 5. Rooms

| ID | Feature | Pri | M |
|---|---|---|---|
| RM-01 | Room defaults by room type (ceiling height, moldings, finishes) | P0 | M1 ✓ |
| RM-02 | Custom room type definitions | P1 | M1 |
| RM-03 | Living-area calculation with include/exclude | P1 | M1 |
| RM-04 | Room polygon tool (preset shapes) | P2 | M3 |
| RM-05 | Room molding polylines; 3D molding polylines (stacked profiles) | P2 | M5 |
| RM-06 | Room polyline (perimeter for area, materials, slabs, moldings) | P1 | M1 |
| RM-07 | Layered floor & ceiling platforms (finish, structure, sub-layers) | P1 | M3 ✓ |
| RM-08 | Floor material regions (inlays without room boundaries) | P2 | M5 |
| RM-09 | Raised floor platforms (auto-framed) | P2 | M6 |
| RM-10 | Conditioned/unconditioned spaces → insulation in materials list | P2 | M4 |
| RM-11 | Multi-select edit rooms (type, vaulted, heights, platform) | P1 | M3 ✓ |
| RM-12 | Object Information Panel (custom fields) for rooms | P1 | M1 |
| RM-13 | Room label dimension formats | P1 | M1 |

## 6. Walls

| ID | Feature | Pri | M |
|---|---|---|---|
| WL-01 | Draw/edit walls in 2D and 3D: interior, exterior, railing, pony, half, curved, angled, raked, stepped | P0 | M1 ✓ straight walls / M3 (rest) |
| WL-02 | Layered wall types: wood, stucco, ICF, SIPS, glass, railings, partitions | P0 | M0 ✓ (layers) |
| WL-03 | Pony walls (upper/lower types, caps, framing, edge line) | P2 | M3 |
| WL-04 | Railings: newels, balusters, panels, handrails, shoe, offsets | P1 | M3 ◐ railing walls with rail, balusters, height, spacing; newels, panels, shoe later |
| WL-05 | Newel positioning rules and manual newel editing | P2 | M3 |
| WL-06 | Raked & stepped walls; stepped foundations | P2 | M3 |
| WL-07 | Wall type editor; layers feed materials list | P0 | M1 ✓ |
| WL-08 | Match properties (wall to wall) | P1 | M1 |
| WL-09 | Multiple main (structural) layers for ICF/SIP connections | P2 | M3 |
| WL-10 | Wall niches | P2 | M3 |
| WL-11 | Partition walls (don't cut structural layers) | P2 | M3 |
| WL-12 | Remodel layers: new / existing / demolition | P1 | M3 ✓ |
| WL-13 | Wall hatching (labels, copy/paste, shear wall marking) | P2 | M4 |
| WL-14 | Wall definitions in library | P1 | M5 ✓ |
| WL-15 | Corner boards & quoins (auto/manual) | P2 | M5 |
| WL-16 | Wall material regions | P2 | M5 |
| WL-17 | Double walls (marriage/multi-family) incl. openings | P3 | M3 |
| WL-18 | Control which layer intersecting walls snap to | P1 | M1 |
| WL-19 | Continuous wall draw mode | P0 | M1 ✓ |

## 7. Cabinets

| ID | Feature | Pri | M |
|---|---|---|---|
| CB-01 | Parametric cabinets: framed, frameless, curved, angled, bow; doors, drawers, shelves, moldings, backsplashes | P1 | M5 ◐ framed and frameless; standard, angled, bow and curved shapes; door/drawer/open/panel/glass rows; backsplashes; shelves counted (not drawn); moldings later |
| CB-02 | Cabinet defaults | P1 | M5 ✓ |
| CB-03 | Architectural blocks (save cabinet groups/kitchens; unblock) | P2 | M5 |
| CB-04 | Items inside cabinets (shelves, storage, hardware, appliances on face) | P2 | M5 |
| CB-05 | Shelf spacing per opening | P2 | M5 |
| CB-06 | Waterfall countertop edges | P2 | M5 |
| CB-07 | Custom countertops: holes, edge profiles, thickness, height | P1 | M5 ✓ |
| CB-08 | Custom backsplash that wraps cabinets/openings/appliances | P2 | M5 |
| CB-09 | Auto/manual fillers with exclusions | P1 | M5 ✓ |
| CB-10 | Blind-corner behaviour | P2 | M5 |
| CB-11 | Independent side/back/front configuration; side & back thickness | P2 | M5 |
| CB-12 | Report door/drawer sizes | P2 | M5 |
| CB-13 | Show doors/drawers open (percent, swing angle) in 2D/3D | P3 | M5 |
| CB-14 | Box construction (top/bottom/sides on/off) | P2 | M5 |
| CB-15 | Extended stiles with auto-clipping | P2 | M5 |
| CB-16 | Per-edge door overlay | P2 | M5 |
| CB-17 | Frameless reveal controls | P2 | M5 |
| CB-18 | Separate materials per component | P1 | M5 ✓ |
| CB-19 | Rounded/chamfered box and countertop corners | P2 | M5 |
| CB-20 | Millwork & end panels (pilasters, feet, legs, moldings; auto end panels) | P2 | M5 |
| CB-21 | Hardware per door/drawer with auto-orientation and placement | P1 | M5 ✓ |
| CB-22 | Stacked moldings with auto-stack | P2 | M5 |
| CB-23 | Flush cabinet moldings | P3 | M5 |
| CB-24 | NKBA-standard display options | P2 | M5 |
| CB-25 | Manufacturer cabinet catalogs | P3 | M8 |

## 8. Doors & windows

| ID | Feature | Pri | M |
|---|---|---|---|
| DW-01 | Parametric doors/doorways/windows incl. arched, shaped, mulled; hardware library | P0 | M1 ✓ basic / M3 (arched, shaped, mulled, hardware) |
| DW-02 | Door types: interior, exterior, sliding, barn, pocket, bifold, atrium, fixed, double, shower, garage | P1 | M3 ✓ |
| DW-03 | Window types: fixed, single/double hung, slider, casement, corner, recessed, louvered, bay, box, pass-through | P1 | M3 ◐ fixed, single/double hung, slider, casement, awning, hopper; corner, bay, box, recessed (P2 items) later |
| DW-04 | Defaults per door type | P1 | M3 ✓ |
| DW-05 | Door glass per side; glass garage panels | P2 | M3 |
| DW-06 | Opening indicators; show open/partial in 3D and plan | P1 | M3 ✓ |
| DW-07 | Multi-panel / accordion / stacking doors | P2 | M3 |
| DW-08 | Bay/box/bow windows as editable groups incl. roofs and platforms | P2 | M3 |
| DW-09 | Explode bay/box/bow into components | P3 | M3 |
| DW-10 | Corner windows (with/without post) | P2 | M3 |
| DW-11 | Mulled units; mulled doors recognized in schedules | P2 | M3 |
| DW-12 | Openings in double walls | P3 | M3 |
| DW-13 | Custom plan symbols for symbol windows | P3 | M5 |
| DW-14 | Custom muntins from CAD | P3 | M5 |
| DW-15 | Window treatments (curtains, blinds) | P3 | M5 |
| DW-16 | Shutters (parametric or symbol, own layer) | P2 | M5 |
| DW-17 | Lintels, sills, aprons | P2 | M3 |
| DW-18 | Glazing area and U-factor | P2 | M4 |

## 9. Roofs & ceilings

| ID | Feature | Pri | M |
|---|---|---|---|
| RF-01 | Automatic roofs: gable, hip, shed, Dutch gable, gambrel, mansard; per-eave pitch/overhang/returns; auto-rebuild | P1 | M3 ✓ |
| RF-02 | Auto dormers (floating & structural): eyebrow, gable, hip, shed, gambrel, mansard, barrel, hip-curved | P2 | M3 |
| RF-03 | Roof options: raised plate/energy heel, curved, fascia, gutters, exposed rafters, returns | P1 | M3 ◐ raised plate/heel, fascia; curved, gutters, exposed rafters, returns later |
| RF-04 | Roof pivot locks (rafter/truss/remodel) | P2 | M3 |
| RF-05 | Draw/edit roof planes in 2D and 3D | P1 | M3 ◐ draw roof planes in 2D; 3D editing later |
| RF-06 | Ridge caps with profiles | P2 | M3 |
| RF-07 | Rafter spacing per plane | P2 | M6 |
| RF-08 | Skylights with framing; shape editor | P2 | M3 |
| RF-09 | Automatic ceilings; manual cathedral, tray, coffered, barrel | P1 | M3 ✓ |
| RF-10 | Tray ceiling tool (slope, moldings, rope lights) | P2 | M3 |
| RF-11 | Layered roof assemblies (SIP roofs) | P2 | M3 |
| RF-12 | Gutters & frieze moldings with custom profiles | P2 | M3 |
| RF-13 | Shadow boards on fascia | P3 | M5 |
| RF-14 | Convert roof/ceiling planes to polylines | P3 | M4 |

## 10. Stairs & ramps

| ID | Feature | Pri | M |
|---|---|---|---|
| ST-01 | Stair tool: rise/run/width/treads/overhang/stringers/railings/brackets; riser & tread settings | P1 | M3 ◐ rise, run, width, treads, nosing, stringers, railings; brackets later |
| ST-02 | One-click auto stairs/ramps between platforms or terrain; L and U shapes | P1 | M3 ◐ straight, L and U tools with automatic riser count; auto-connect and ramps later |
| ST-03 | Curved, flared, starter treads, winders, walk line | P2 | M3 |
| ST-04 | Wrap-around stairs | P3 | M3 |
| ST-05 | Mixed-width stair sections | P3 | M3 |
| ST-06 | Break/disconnect/edit stair sections | P2 | M3 |
| ST-07 | Split landings for L/U | P2 | M3 |
| ST-08 | Stringer/riser/trim component control | P2 | M3 |
| ST-09 | Auto stairwell (cut platform) | P1 | M3 ✓ |
| ST-10 | ADA ramps | P2 | M3 |
| ST-11 | Custom landings and auto-connect | P2 | M3 |
| ST-12 | Stair railing styles; plan display of balusters | P1 | M3 ◐ handrails and balusters in 3D, baluster dots in plan; styles later |
| ST-13 | Custom newels & balusters (library/import) | P2 | M5 |
| ST-14 | Stair defaults interior/exterior | P1 | M3 ✓ |
| ST-15 | Stairs/ramps saved to library | P2 | M5 |
| ST-16 | Plan display: overhang dashes, stringers, cut/break line, floors above/below | P1 | M3 ◐ break line, dashed treads above the cut, overhang dashes, UP/DN, floor above; stringer lines later |
| ST-17 | Handrail returns & extensions | P2 | M3 |

## 11. Foundations

| ID | Feature | Pri | M |
|---|---|---|---|
| FD-01 | Slab, pier, crawlspace, full basement, stem walls, footings, post & beam, grade beam | P1 | M3 ◐ slab, crawlspace/stem wall, basement, footings, piers with beams; post & beam framing, grade beam (M6) later |
| FD-02 | Auto-rebuild foundation from floor 1 | P1 | M3 ✓ |
| FD-03 | Round piers / square pads | P2 | M3 |
| FD-04 | Monolithic slabs with ledges and garage curbs | P2 | M3 |
| FD-05 | Footings under bearing walls; stepped footings | P2 | M3 |

## 12. Framing

| ID | Feature | Pri | M |
|---|---|---|---|
| FR-01 | Automatic framing of floors, walls, ceilings, roofs, openings, headers | P2 | M6 |
| FR-02 | Framing reference markers per level | P2 | M6 |
| FR-03 | Joist direction lines | P2 | M6 |
| FR-04 | Build All Framing / subsets | P2 | M6 |
| FR-05 | Manual framing members with info panel | P2 | M6 |
| FR-06 | Retain manual framing across auto rebuilds (X18+) | P2 | M6 |
| FR-07 | Framing materials: lumber, steel, LVL, PSL, VSL; custom types and shapes | P2 | M6 |
| FR-08 | EasyFrame panelization export (.EZF) | P3 | M8 |
| FR-09 | Roof framing: rafters, ridges, lookouts, blocking, valley fill, rafter tails | P2 | M6 |
| FR-10 | Auto-build trusses; auto-fit floor/ceiling/roof trusses (scissor, attic, girder, hip, energy heel…) | P2 | M6 |
| FR-11 | Truss rebuild tool; truss details with labels | P3 | M6 |
| FR-12 | Truss chords/webbing in materials list (X18+) | P3 | M6 |
| FR-13 | Horizontal roof framing (battens, purlins) | P3 | M6 |
| FR-14 | Decorative end profiles | P3 | M6 |
| FR-15 | Post & beam tools | P2 | M6 |
| FR-16 | Multi-ply beams/posts/girders (X18+) | P3 | M6 |
| FR-17 | Wall framing options: plates, studs, bridging, knee walls; steel C/U channels; rim joists | P2 | M6 |
| FR-18 | Balloon framing | P3 | M6 |
| FR-19 | Header controls (cripples, rotated, flush, combined) | P2 | M6 |
| FR-20 | Framing per wall-type layer (furred basement walls) | P3 | M6 |
| FR-21 | Horizontal wall framing (girts) for pole/timber buildings | P3 | M6 |
| FR-22 | Per-opening framing (box/built-up headers, trimmers, sills) | P2 | M6 |
| FR-23 | Wall framing detail views | P2 | M6 |
| FR-24 | Framing labels and member roles for schedules | P2 | M6 |
| FR-25 | Build framing per room / per wall / per roof plane | P2 | M6 |
| FR-26 | Corner & intersection framing (standard, 2-stud, laddered) | P2 | M6 |
| FR-27 | Bearing walls (butt/lap framing) | P2 | M6 |
| FR-28 | Platform holes with auto framing | P2 | M6 |
| FR-29 | Tray ceiling framing | P3 | M6 |
| FR-30 | Deck framing: planks, borders, beams, joists, posts, footings, blocking; auto-rebuild | P2 | M6 |
| FR-31 | Deck ledger controls (X18+) | P3 | M6 |
| FR-32 | Deck beam controls | P3 | M6 |

## 13. CAD tools

| ID | Feature | Pri | M |
|---|---|---|---|
| CAD-01 | Points, lines, polylines, splines, arcs, circles, ovals, boxes, arrows; trim/extend, parallel, perpendicular, reflect, replicate, transform, fillet, chamfer | P0 | M1 ✓ (perpendicular tool: later) |
| CAD-02 | Multiple arc creation modes | P1 | M1 |
| CAD-03 | Object snaps (end, center, tangent…), grid snap, reference-floor snap | P0 | M1 ◐ end/mid/center/on-object/angle/grid/reference floor; tangent, intersection, quadrant later |
| CAD-04 | Paste in place across layers/plans/floors | P1 | M1 |
| CAD-05 | Point-to-point move/copy/resize | P0 | M1 ✓ |
| CAD-06 | Detailing primitives: cross box, blocking box, insulation | P1 | M4 ✓ |
| CAD-07 | Convert 2D CAD into smart objects (solids, moldings, counters, slabs, roads, terrain, …) | P2 | M4 |
| CAD-08 | Multiple copy at preset increments | P1 | M1 |
| CAD-09 | 3D solids and faces; extrude/revolve; polyline boolean ops | P2 | M5 |
| CAD-10 | 3D solid tool with defaults | P2 | M5 |
| CAD-11 | Solid booleans across plan/elevation; fillet/chamfer solid edges | P3 | M5 |
| CAD-12 | Regular polygon tool | P1 | M1 |
| CAD-13 | Fillet all / chamfer all | P2 | M1 |
| CAD-14 | CAD-to-Walls (DWG lines → walls, windows, doors) | P2 | M4 |
| CAD-15 | Library of structural CAD details | P3 | M8 (own content) |
| CAD-16 | Align / distribute | P1 | M1 |
| CAD-17 | CAD details & blocks with insertion points; save to library | P1 | M4 ✓ |
| CAD-18 | CAD detail from any view | P1 | M4 ✓ |
| CAD-19 | Copy CAD details between plans | P2 | M4 |
| CAD-20 | Line style manager (create, merge, find usages) | P1 | M1 |

## 14. Layers

| ID | Feature | Pri | M |
|---|---|---|---|
| LY-01 | Layer sets per drawing type; quick swap | P0 | M1 ✓ |
| LY-02 | Layer management: weight, color, style; materials-list inclusion | P0 | M1 ✓ (materials list itself: M4) |
| LY-03 | Layer eyedropper & painter | P2 | M1 |
| LY-04 | Layer hider (click to hide) | P1 | M1 |
| LY-05 | Edit an object's layer properties from the toolbar | P2 | M1 |
| LY-06 | Reference displays: multiple reference floors/layer sets, draw order | P1 | M3 ◐ several reference floors; reference layer sets and draw order later |
| LY-07 | Reference display from other plan files (2D and 3D ghosting) | P2 | M8 |
| LY-08 | 3D reference display styles | P3 | M7 |
| LY-09 | Dockable layer display options panel | P1 | GUI |

## 15. Dimensions

| ID | Feature | Pri | M |
|---|---|---|---|
| DIM-01 | Baseline, running, centerline, angular, end-to-end, interior, point-to-point, temporary dimensions; one-click exterior | P0 | M1 ◐ point-to-point/running chains, angular, temporary, auto exterior; baseline, centerline, auto interior later |
| DIM-02 | Dimensions in 3D views | P2 | M7 |
| DIM-03 | Move objects by editing dimensions; arithmetic in inputs | P0 | M1 ◐ arithmetic everywhere, wall/line length via temporary dimension; moving by any placed dimension later |
| DIM-04 | Auto exterior dimensions with auto-refresh | P1 | M1 |
| DIM-05 | Wall length editing via dimensions; type exact length while drawing | P0 | M1 ✓ |
| DIM-06 | Dimension snapping rules; hotkey override | P1 | M1 |
| DIM-07 | Column/bubble grid lines with auto labeling | P2 | M4 |
| DIM-08 | Units/accuracy (to 1/128" or 0.0078125), text and arrow styles | P0 | M0 ✓ / M1 ✓ |
| DIM-09 | Dimension defaults table editor | P2 | GUI |
| DIM-10 | Centerline markers | P2 | M1 |
| DIM-11 | Inch threshold display (18" vs 1'-6") | P1 | M1 |
| DIM-12 | Secondary (dual) dimensions | P2 | M1 |
| DIM-13 | Custom/override dimension text | P1 | M1 |
| DIM-14 | Rotatable dimension text | P2 | M1 |
| DIM-15 | Laser distance meter (Bluetooth) input | P3 | M8 |
| DIM-16 | Auto elevation dimensions in elevation/section views | P2 | M4 |
| DIM-17 | Multiple named dimension defaults | P1 | M1 |
| DIM-18 | Per-tool locate defaults | P2 | M1 |
| DIM-19 | Multiple auto-dimension defaults | P2 | M4 |
| DIM-20 | Inner/outer auto-dimension controls | P2 | M4 |
| DIM-21 | NKBA automatic dimensions | P2 | M5 |
| DIM-22 | Angled dimension behaviour | P2 | M1 |
| DIM-23 | Story pole dimensions | P3 | M4 |

## 16. Lighting, electrical & HVAC

| ID | Feature | Pri | M |
|---|---|---|---|
| EL-01 | Electrical, plumbing, HVAC symbols; circuits/wiring with auto 3-way/4-way; auto-orient to slopes | P1 | M3 ◐ electrical, plumbing and HVAC symbols; switch wiring with automatic 3-way/4-way; slope orientation later |
| EL-02 | Auto-place outlets by room type (GFCI) | P2 | M3 |
| EL-03 | Light fixtures emit light in renders | P1 | M5 ✓ |
| EL-04 | Rope lighting along paths | P3 | M7 |
| EL-05 | Per-fixture light controls (shadows, intensity, dimming, on/off) | P1 | M5 ◐ on/off, brightness, dimmer, colour temperature and a shadows switch; light from fixtures doesn't cast shadows yet |
| EL-06 | Lighting manager table | P2 | M7 |
| EL-07 | Kelvin color temperature (X18+) | P2 | M5 |
| EL-08 | Lighting sets per view/camera | P2 | M7 |

## 17. Terrain, site & landscape

| ID | Feature | Pri | M |
|---|---|---|---|
| SITE-01 | North pointer, sun angles from lat/long/date/time; 2D and 3D shadows | P1 | M3 ✓ |
| SITE-02 | Copy sun angles for studies | P3 | M7 |
| SITE-03 | Bearing/angle display formats (azimuth, quadrant, DMS) | P1 | M1 |
| SITE-04 | Terrain model from elevation data: smoothing, triangulation, skirt, contours | P2 | M6 |
| SITE-05 | Plot plans, site plans, subdivisions from bearings, distances and setbacks | P2 | M6 |
| SITE-06 | Terrain import: text/GPS/GPX, DWG/DXF | P2 | M6 |
| SITE-07 | Elevation splines, lines, regions, points; subfloor marker; clear terrain | P2 | M6 |
| SITE-08 | Terrain elevation labels | P2 | M6 |
| SITE-09 | Terrain modifiers: hills, valleys, elevated and flat regions | P2 | M6 |
| SITE-10 | Retaining walls and terrain breaks | P2 | M6 |
| SITE-11 | Stepping stones, streams, curbing, garden beds | P3 | M6 |
| SITE-12 | Roads, medians, striping, curbs, gutters, driveways, sidewalks, cul-de-sacs | P2 | M6 |
| SITE-13 | Road/driveway/sidewalk volumes in materials list | P3 | M6 |
| SITE-14 | Auto plan footprint CAD detail for site plans | P2 | M6 |
| SITE-15 | Deck designer (straight, curved, polygon, covered) | P2 | M6 |
| SITE-16 | Fences that follow terrain; gates | P2 | M6 |
| SITE-17 | Sprinkler designer (zones, runs) | P3 | M8 |
| SITE-18 | Plant images/symbols in 2D/3D | P2 | M6 |
| SITE-19 | Import own plant images with growth data | P3 | M6 |
| SITE-20 | Plant chooser with hardiness zone maps | P3 | M8 |
| SITE-21 | Plant encyclopedia data | P3 | M8 (own data) |
| SITE-22 | Landscaping plans and plant schedules | P3 | M6 |
| SITE-23 | Plant growth over time | P3 | M8 |
| SITE-24 | 3D plants with shadows | P2 | M7 |

## 18. 3D design & materials

| ID | Feature | Pri | M |
|---|---|---|---|
| MAT-01 | Edit any architectural object in 2D or 3D | P0 | M1 ✓ (select and edit from 3D; direct 3D dragging later) |
| MAT-02 | Object painter / eyedropper with attribute choice and scope | P2 | M5 |
| MAT-03 | Material painter with scope; default painter (reset) | P1 | M5 ✓ |
| MAT-04 | Material eyedropper; color chooser from image | P2 | M5 |
| MAT-05 | Adjust-material scoping (whole plan vs copy for subset) | P2 | M5 |
| MAT-06 | Fill styles: pattern scale/angle/offset/transparency; gradients; fill painter | P1 | M1 |
| MAT-07 | Parametric herringbone/grid patterns | P2 | M5 |
| MAT-08 | Custom patterns to library (X18+) | P2 | M5 |
| MAT-09 | Animated water materials | P3 | M7 |
| MAT-10 | Emissive materials as area lights | P2 | M7 |
| MAT-11 | PBR maps: AO, roughness, metal, emissive, anisotropic, normal, bump | P1 | M5 ◐ colour, roughness, metallic, emissive, opacity and anisotropy values shaded live; all eight image maps stored with the material, drawn from textures later |
| MAT-12 | Interactive material editing (scale/rotation/origin in 3D) | P2 | M5 |
| MAT-13 | Material builder: custom tiles, grout; Substance (.sbsar) import | P2/P3 | M5/M8 |
| MAT-14 | Material classes (plastic, metal, mirror, glass, translucent…) | P1 | M5 ✓ |
| MAT-15 | Material maps incl. opacity; roughness/gloss inversion | P1 | M5 ◐ opacity map and gloss inversion stored; texture rendering later |
| MAT-16 | Plan materials manager (replace, merge, search) | P1 | M5 ✓ |
| MAT-17 | Hatch pattern editor | P2 | M4 |
| MAT-18 | Generate vector hatch pattern from texture | P3 | M5 |
| MAT-19 | Screen-capture image as material | P3 | M5 |
| MAT-20 | 3D cladding from moldings (board & batten, lap, standing seam) (X18+) | P2 | M5 |
| MAT-21 | Style palettes (save and apply finish sets) | P2 | M5 |
| MAT-22 | Center object between two objects | P1 | M1 |
| MAT-23 | Match / apply properties across same-type items | P1 | M1 |
| MAT-24 | Import custom 3D symbols | P1 | M5 |
| MAT-25 | Smart symbols (sinks cut counters, wall-hung items, light fixtures emit) | P2 | M5 |
| MAT-26 | Convert selection to symbol object | P2 | M5 |
| MAT-27 | Custom molding profiles; stacked, flipped, rotated, centered | P2 | M5 |
| MAT-28 | Edit molding polylines from library (X18+) | P3 | M5 |
| MAT-29 | Distributed regions/paths with randomized variation | P3 | M6 |

## 19. Library / catalog

| ID | Feature | Pri | M |
|---|---|---|---|
| LIB-01 | Library browser of objects; create, import, download | P1 | M5 ◐ built-in catalogue (cabinet sizes, appliances, fixtures, materials, wall types) and a user library: add selected objects, import/export .hclib bundles; downloading from an online catalogue later |
| LIB-02 | Catalog migration between versions | P3 | M8 |
| LIB-03 | Catalog update service | ✗ | — (own content feed later) |
| LIB-04 | Update catalogs on demand | ✗ | — |
| LIB-05 | Search with AND/OR/NOT/grouping; search attributes | P1 | M5 ✓ |
| LIB-06 | Tags, filters, favorites, saved filters | P2 | M5 |
| LIB-07 | Edit CAD blocks in place; apply to all instances | P2 | M5 |
| LIB-08 | Patterns and fill styles in library | P2 | M5 |
| LIB-09 | Notes, callouts, markers in library | P2 | M5 |
| LIB-10 | Text in library | P3 | M5 |
| LIB-11 | Library shortcuts | P3 | M5 |
| LIB-12 | Replace from library (one or all identical) | P1 | M5 ◐ cabinets and appliances, one or all identical; blocks later |
| LIB-13 | Library painter with scope | P2 | M5 |
| LIB-14 | Edit parametric items stored in library | P2 | M5 |
| LIB-15 | Edit symbol properties in library | P2 | M5 |
| LIB-16 | Find a selected object in the library | P3 | M5 |
| LIB-17 | Save components (doors, hardware, moldings) to library | P2 | M5 |
| LIB-18 | Read user CA libraries (`.calib`/`.calibz`) | P2 | M8 |

## 20. Materials list & cost estimating

| ID | Feature | Pri | M |
|---|---|---|---|
| EST-01 | Auto materials list (equipment, unit cost, markup, labor); plan / room / area scope; live update | P1 | M4 ◐ take-off with unit prices and markup, whole plan or floor; labor and room/area scope later |
| EST-02 | Master materials list (pricing master data) | P2 | M4 |
| EST-03 | Find in plan from materials list | P1 | M4 ✓ |
| EST-04 | Per-object code/comment/description/manufacturer fields | P1 | M1 |
| EST-05 | Object components panel (calculation methods) | P2 | M4 |
| EST-06 | Layer-based inclusion | P1 | M4 ✓ |
| EST-07 | Scripted formulas (Rhai/Lua; CA uses Ruby) | P2 | M8 |
| EST-08 | Materials-list layer sets | P2 | M4 |
| EST-09 | Schedules: doors, windows, cabinets, fixtures, appliances, furniture, electrical, HVAC, notes, plumbing, plants, room finish, walls, moldings, roof/ceiling planes, skylights, millwork, sprinklers, shapes; floor/room filters | P1 | M4 ◐ doors, windows, room finish, walls, electrical/plumbing/HVAC, stairs, roof planes; cabinets, appliances, furniture, moldings (M5) later |
| EST-10 | Schedule formatting: styles, transpose, sort, wrapping | P1 | M4 ◐ choose and order columns; styles, transpose, sort and wrap options later |
| EST-11 | Custom columns from object info fields | P1 | M4 ◐ any of a schedule's built-in columns; user-defined object fields later |
| EST-12 | Callout label shapes | P2 | M4 |
| EST-13 | Custom mixed-type schedules; existing/new splits | P2 | M4 |
| EST-14 | Schedules in library | P3 | M5 |
| EST-15 | Polyline objects in schedules | P2 | M4 |
| EST-16 | Find in plan from schedule | P1 | M4 ✓ |
| EST-17 | Open object spec from schedule row | P1 | M4 ✓ |
| EST-18 | Schedule thumbnails | P3 | M4 |
| EST-19 | Schedule starting number | P3 | M4 |
| EST-20 | Export schedules: CSV, TXT, XML | P1 | M4 ✓ |
| EST-21 | BuilderTREND export with NAHB accounting codes | P3 | M8 |
| EST-22 | Cut list and buy list (structural lumber) | P2 | M6 |
| EST-23 | Pricing in framing buy lists | P3 | M6 |

## 21. Text, annotation & scripting

| ID | Feature | Pri | M |
|---|---|---|---|
| TXT-01 | Editable text: font, size, weight, color, justification | P0 | M1 ◐ size, weight, color, justification; font families need font loading |
| TXT-02 | Rich text, hyperlinks | P1 | M1 |
| TXT-03 | Find/replace text | P2 | M4 |
| TXT-04 | Replace fonts | P3 | M4 |
| TXT-05 | Text arrows / leaders with auto orientation | P1 | M1 |
| TXT-06 | Custom arrowheads from CAD blocks | P2 | M4 |
| TXT-07 | Callouts & markers (styles, own layers) | P1 | M4 ✓ |
| TXT-08 | Linked callouts that navigate between views | P2 | M4 |
| TXT-09 | Object labels with macros | P1 | M1 |
| TXT-10 | Label formatting (justify, border, wrap) | P1 | M1 |
| TXT-11 | Per-view label hide (X18+) | P2 | M1 |
| TXT-12 | Auto labels for layout boxes | P2 | M4 |
| TXT-13 | Text styles per layer/default/object | P0 | M1 ✓ |
| TXT-14 | Default sets for annotation | P1 | M1 |
| TXT-15 | Annotations on 3D views | P2 | M7 |
| TXT-16 | Numbered notes with schedule; note markers in 3D | P2 | M4 |
| TXT-17 | Spell check | P3 | GUI |
| TXT-18 | Hyperlinks | P2 | M1 |
| TXT-19 | Text macros incl. user-defined (contact, file, plan info, dates) | P1 | M4 ◐ {project}, {date}, {year}, {living_area}, {floors}, {rooms} in plan text and layout notes; user-defined macros later |
| TXT-20 | Macro management (create, test) | P2 | M4 |
| TXT-21 | Scripting console (CA: Ruby → ours: Rhai/Lua over the command API) | P2 | M8 |
| TXT-22 | Revision clouds with defaults | P1 | M4 ✓ |

## 22. Printing

| ID | Feature | Pri | M |
|---|---|---|---|
| PRN-01 | Print to PDF | P0 | M1 ✓ |
| PRN-02 | Watermarks (text/image; also on walkthroughs) | P3 | M4 |
| PRN-03 | Print preview with line weights | P1 | M4 ◐ the layout page view previews sheets with line weights; a plan print preview later |
| PRN-04 | Drawing sheet setup and paper scaling | P1 | M4 ◐ layout sheet size and per-box scales; plan PDF paper and scale options |

## 23. Import / export

| ID | Feature | Pri | M |
|---|---|---|---|
| IO-01 | Export materials list: Excel, CSV, TXT, HTML | P1 | M4 ◐ CSV, text, XML and HTML; native Excel (.xlsx) later |
| IO-02 | Import PDF into plans/layouts/details (vector + raster) | P2 | M4 |
| IO-03 | DXF/DWG import & export (2D) | P1 | M1 (DXF) / M8 (DWG) |
| IO-04 | 3D import: DXF, DWG, OBJ, 3DS, SKP, STL, DAE; 3D export: DXF, DWG, 3DS, STL, DAE (+ glTF/OBJ/IFC for us) | P1→P3 | M5/M8 |
| IO-05 | Image import: BMP, JPG, GIF, TIFF, PNG, metafiles, WEBP; transparency color | P1 | M5 |
| IO-06 | High-resolution image export, transparent background | P1 | M7 |
| IO-07 | Import/export settings objects (views, default sets, layer sets, hotkeys, wall types, line styles, macros, scripts) | P2 | M5 |
| IO-08 | 3D mass import → library objects | P2 | M5 |
| IO-09 | Textures → materials in bulk | P2 | M5 |
| IO-10 | Terrain survey import | P2 | M6 |
| IO-11 | Settings migration between versions | P3 | M8 |
| IO-12 | **Open Chief Architect `.plan` / `.layout`** (ours) | P1/P2 | M2/M8 |
| IO-13 | **Open Home Designer plans** (same family) | P2 | M8 |
| IO-14 | RESCheck energy export | P3 | M8 |

## 24. Platform, UI & user resources

| ID | Feature | Pri | M |
|---|---|---|---|
| UI-01 | Windows + macOS native (ARM) (ours: + Linux + web) | P0 | GUI ◐ Windows verified; macOS/Linux in CI; web compiles (packaging later) |
| UI-02 | Import from As-Built LiDAR scan app (X18+) → ours: RoomPlan/USDZ import | P3 | M8 |
| UI-03 | 3D viewer app / cloud sharing → ours: web viewer build | P1 | web |
| UI-04 | Free read-only viewer | P1 | web |
| UI-05 | Home Designer compatibility | P2 | M8 |
| UI-06 | Multiple monitors; tear-off tabbed views | P2 | GUI |
| UI-07 | Touch (pinch/rotate) | P2 | GUI |
| UI-08 | HiDPI icons | P0 | GUI ✓ (vector icons) |
| UI-09 | Preferences & defaults dialogs | P0 | GUI ✓ |
| UI-10 | Search in default settings (X18+) | P2 | GUI |
| UI-11 | Undo history panel; multi-step undo/redo | P1 | M0 ✓ (engine) / GUI |
| UI-12 | 3Dconnexion SpaceMouse | P3 | GUI |
| UI-13 | Dark mode | P0 | GUI ✓ |
| UI-14 | Virtual graph paper grid | P1 | M1 |
| UI-15 | Auto-archive backups on schedule | P1 | M1 |
| UI-16 | Single-file project archive (`.CAPROJ` equivalent) | P2 | M8 |
| UI-17 | Crosshair thickness | P3 | GUI |
| UI-18 | UI themes | P3 | GUI |
| UI-19 | Customizable toolbars and toolbar sets | P2 | GUI |
| UI-20 | Hotkeys incl. sequential chords | P1 | GUI |
| UI-21 | VR (OpenXR) | P3 | M8 |
| UI-22 | Context-sensitive help | P2 | GUI |
| UI-23 | License activation | ✗ | — |
| UI-24 | Document tabs for open views (Dashboard, plan, 3D, layouts) under the toolbars | P1 | GUI |
| UI-25 | Plan canvas stays white with a light lavender grid in every theme; red X / blue Y origin marker | P1 | GUI |
| UI-26 | Typed Tool Search box in the top toolbar; floor stepper (down, floor number, up) beside the plan-view list | P1 | GUI |
| UI-27 | Right dock order Library, Project Browser, Active Layer Display; Project Browser with filter box, Project Folders / Projects tree, Details and Preview panes | P1 | GUI |
| UI-28 | Status bar: hint, current tool [hotkey], floor, layer, drawing group, live X/Y/Z, view size in pixels | P1 | GUI |
| UI-29 | Account menu before Help; left dock titled "Tool Palette" | P2 | GUI |
