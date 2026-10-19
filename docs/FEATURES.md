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
| VIS-02 | XYZ axes indicator and project-origin marker in 2D/3D | P1 | M1 ✓ red X / blue Y origin marker and corner axis indicator in plan; X/Y/Z arrows in 3D (Preferences toggle) |
| VIS-03 | Navigation: orbit, pan, dolly, tilt, walk; standard view-direction presets | P0 | M1 ✓ (orbit, pan, dolly, walk, presets) |
| VIS-04 | Auto-hide camera-facing exterior walls while navigating small rooms | P2 | M7 ✓ option to hide walls between the camera and what it looks at |
| VIS-05 | Cross-section slider: interactive cutaway from any side, live | P1 | M4 ✓ |
| VIS-06 | Poché (solid fill) for walls/roofs/platforms in plan and camera views | P1 | M1 ◐ walls and piers are poché-filled in plan; a layer's fill style replaces it (hatch, crosshatch, dots, solid); no poché in 3D camera views yet |
| VIS-07 | Walkthrough recording along spline paths; per-keyframe direction/speed; multi-floor; per-path lighting | P2 | M7 ◐ record a walkthrough as you move; smooth path, travel time and pause per keyframe, any floors, its own lighting; exported as numbered frames (no video file) |
| VIS-08 | Walkthrough editing: keyframe editing, join/break segments, pauses, multi-select | P2 | M7 ◐ edit keyframe times and pauses, delete keyframes, join stops; no multi-select or splitting |
| VIS-09 | Automatic 360° exterior orbit walkthrough | P2 | M7 ✓ 3D > Walkthroughs > Create Orbit Walkthrough |
| VIS-10 | Cloud share walkthroughs | ✗ | — |
| VIS-11 | Sun/shadow time-lapse (date/time sequences, timestamp overlay) | P2 | M7 ✓ 3D > Special Renders > Sun Time-Lapse: one time-stamped frame per step across a day |
| VIS-12 | 360° spherical render export (up to 8192×4096) | P2 | M7 ✓ 360° panorama export up to 8192 × 4096 |
| VIS-13 | Sky model: sun, moon, horizon lighting controls per camera | P2 | M7 ◐ sky colours, sun brightness and moonlight per camera; no horizon light controls |
| VIS-14 | Custom image backdrops per saved camera | P1 | M5 ✓ |
| VIS-15 | Spherical (360°) backdrops with offset/orientation/eye level | P2 | M7 ✓ Render Settings > 360° backdrop: any panorama image behind the 3D view, with turn and eye-level sliders |
| VIS-16 | HDR backdrops providing environment lighting | P2 | M7 ◐ the backdrop's average colour tints the scene's ambient light; no image-based reflections, and plain images rather than true HDR files |
| VIS-17 | HDR image export | P2 | M7 ✓ HDR (.hdr) image merged from three exposures |
| VIS-18 | Extend terrain to horizon in 3D | P2 | M6 ✓ terrain option: extend flat ground to the horizon in 3D |
| VIS-19 | Procedural grass regions (length, density, color) | P3 | M7 ✓ Terrain > Site > Grass: blades of a set length, density and colour over a region |
| VIS-20 | Render techniques: standard, PBR, watercolor, squiggle line, technical illustration, clay, vector, glass house, duotone | P1→P2 | M5/M7 ◐ standard, clay, technical illustration, watercolor, duotone and glass house; no squiggle line or vector styles |
| VIS-21 | Line-drawing overlay on standard/duotone/watercolor/PBR/clay | P2 | M7 ✓ Render Settings > Lines draws edges over any style |
| VIS-22 | GPU path-traced PBR with soft shadows | P2 | M7 ◐ High-Quality Image averages many passes (smooth edges, soft sun shadows, depth of field); not path traced |
| VIS-23 | Denoising for ray-traced views | P2 | M7 ◐ high-quality renders average passes, so there is no ray-tracing noise to remove |
| VIS-24 | Depth of field (focus point, f-stop) | P3 | M7 ✓ depth of field (focus distance and f-stop) in high-quality renders |
| VIS-25 | Camera default sets (e.g. dimension defaults for elevations vs sections) | P1 | M4 ✓ |
| VIS-26 | Eight standard isometric angles | P1 | M1 ✓ 3D view Iso menu: each corner from above and from below, orthographic |
| VIS-27 | Shadow intensity control for vector/technical views | P2 | M7 ✓ shadow strength slider per view |
| VIS-28 | Depth cueing (distance fog) | P3 | M7 ✓ distance fog per view |

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
| PRJ-01 | Plan templates; save-as-template | P1 | M1 ✓ File > Save as Template… and File > New from Template; mark one Default for New Plan |
| PRJ-02 | Recent files with pinning | P2 | M1 ✓ File > Recent Files with Pin / Unpin; pinned files stay on top |
| PRJ-03 | Space-planner room boxes: drag room bubbles, convert to walls | P2 | M3 |
| PRJ-04 | Plan Check: egress, roof, wall layout review | P2 | M4 |
| PRJ-05 | Rotate and mirror (flip) entire plan | P1 | M1 ✓ Edit > Entire Plan: Reverse Plan, rotate 90° either way (all floors) |
| PRJ-06 | Zoom to selection | P0 | M1 ✓ |
| PRJ-07 | Free viewer app for clients (read-only) | P1 | web build |
| PRJ-08 | Managed projects (auto-organized storage, no broken links) | P2 | M8 |
| PRJ-09 | Project Browser: tree of layouts, floors, views, cameras, schedules, details; multi-select; folders; thumbnails | P1 | M4 ◐ floors, plan views, cameras, layouts and pages, elevations, rooms; folders, thumbnails, multi-select later |
| PRJ-10 | Pin items in Project Browser (X18+) | P3 | M8 |
| PRJ-11 | Saved Plan Views (layer set + default set + active defaults; zoom/floor optional); tabs/windows | P0 | M1 ✓ (multiple view tabs: later) |
| PRJ-12 | Duplicate saved view copies its active layer set (X18+) | P2 | M1 |
| PRJ-13 | Default Sets per drawing type (kitchen, framing, electrical…) | P1 | M1 ✓ View > Add Standard Plan Views: Electrical, Framing, Kitchen & Bath, Construction and Site, each with its own default set and layer set |
| PRJ-14 | Multi-floor structures | P0 | M0 ✓ |
| PRJ-15 | Insert/exchange floors above/below | P1 | M3 ✓ |
| PRJ-16 | Layers: create, assign, style, group into Layer Sets | P0 | M1 ✓ |
| PRJ-17 | Fill style by layer (X18+) | P2 | M1 ✓ each layer can carry a fill style used by its walls and closed shapes |
| PRJ-18 | Select all on layer | P1 | M1 ✓ Edit > Select All on Layer |
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
| RM-02 | Custom room type definitions | P1 | M1 ✓ Tools > Room Types…: add, edit (ceiling height, finishes, living area, moldings) and delete |
| RM-03 | Living-area calculation with include/exclude | P1 | M1 ✓ living area counts room types marked living; each room can be set to Include or Exclude |
| RM-04 | Room polygon tool (preset shapes) | P2 | M3 |
| RM-05 | Room molding polylines; 3D molding polylines (stacked profiles) | P2 | M5 ✓ |
| RM-06 | Room polyline (perimeter for area, materials, slabs, moldings) | P1 | M1 ◐ room's Make Room Polyline traces its inside outline as a CAD polyline; editing a room by its polyline isn't supported |
| RM-07 | Layered floor & ceiling platforms (finish, structure, sub-layers) | P1 | M3 ✓ |
| RM-08 | Floor material regions (inlays without room boundaries) | P2 | M5 ✓ Floor Material Region tool; tile joints drawn in plan |
| RM-09 | Raised floor platforms (auto-framed) | P2 | M6 ✓ Raised Platform tool: a solid floor framed on joists |
| RM-10 | Conditioned/unconditioned spaces → insulation in materials list | P2 | M4 |
| RM-11 | Multi-select edit rooms (type, vaulted, heights, platform) | P1 | M3 ✓ |
| RM-12 | Object Information Panel (custom fields) for rooms | P1 | M1 ✓ Object Information in every placed object's and room's specification: code, description, manufacturer, comment and your own fields |
| RM-13 | Room label dimension formats | P1 | M1 ✓ Default Settings > Rooms: labels show area, size (rounded or exact) and ceiling height |

## 6. Walls

| ID | Feature | Pri | M |
|---|---|---|---|
| WL-01 | Draw/edit walls in 2D and 3D: interior, exterior, railing, pony, half, curved, angled, raked, stepped | P0 | M1 ✓ straight walls / M3 (rest) |
| WL-02 | Layered wall types: wood, stucco, ICF, SIPS, glass, railings, partitions | P0 | M0 ✓ (layers) |
| WL-03 | Pony walls (upper/lower types, caps, framing, edge line) | P2 | M3 ◐ Wall specification > Pony Wall: a lower wall type below a split height, flush outside, with an edge line in plan; no cap or framing split yet |
| WL-04 | Railings: newels, balusters, panels, handrails, shoe, offsets | P1 | M3 ◐ railing walls with rail, balusters, height, spacing; newels, panels, shoe later |
| WL-05 | Newel positioning rules and manual newel editing | P2 | M3 ◐ railing types place newels at the ends and at most every N; extra newels at typed positions; no dragging newels |
| WL-06 | Raked & stepped walls; stepped foundations | P2 | M3 ◐ Wall specification > Top: raked (start/end heights) or stepped walls in 3D and views; stepped foundations not yet |
| WL-07 | Wall type editor; layers feed materials list | P0 | M1 ✓ |
| WL-08 | Match properties (wall to wall) | P1 | M1 ✓ Match Properties button on the edit toolbar: click other walls to copy type, height, layer and settings |
| WL-09 | Multiple main (structural) layers for ICF/SIP connections | P2 | M3 ◐ wall-type layers can be flagged Main (several for ICF/SIP); drawing along and dimension snapping use them; framing doesn't yet |
| WL-10 | Wall niches | P2 | M3 ✓ Wall specification > Niches: recesses in either face with width, height, sill and depth (3D, plan outline) |
| WL-11 | Partition walls (don't cut structural layers) | P2 | M3 ✓ walls meeting another wall stop at its surface, so they never cut its structural layers |
| WL-12 | Remodel layers: new / existing / demolition | P1 | M3 ✓ |
| WL-13 | Wall hatching (labels, copy/paste, shear wall marking) | P2 | M4 |
| WL-14 | Wall definitions in library | P1 | M5 ✓ |
| WL-15 | Corner boards & quoins (auto/manual) | P2 | M5 ✓ |
| WL-16 | Wall material regions | P2 | M5 ✓ Wall Material Region tool (wainscot from two clicks, editable extents) |
| WL-17 | Double walls (marriage/multi-family) incl. openings | P3 | M3 ◐ walls built back to back (up to 1' apart) act as a double wall: openings cut through both; no single double-wall type |
| WL-18 | Control which layer intersecting walls snap to | P1 | M1 ◐ Default Settings > Walls > Draw walls along: centre, exterior or interior surface, or main (framing) layer, with mitred corners; walls still join at centrelines |
| WL-19 | Continuous wall draw mode | P0 | M1 ✓ |

## 7. Cabinets

| ID | Feature | Pri | M |
|---|---|---|---|
| CB-01 | Parametric cabinets: framed, frameless, curved, angled, bow; doors, drawers, shelves, moldings, backsplashes | P1 | M5 ✓ |
| CB-02 | Cabinet defaults | P1 | M5 ✓ |
| CB-03 | Architectural blocks (save cabinet groups/kitchens; unblock) | P2 | M5 ✓ |
| CB-04 | Items inside cabinets (shelves, storage, hardware, appliances on face) | P2 | M5 ◐ shelves drawn and spaced per opening, built-in appliance faces; storage accessories later |
| CB-05 | Shelf spacing per opening | P2 | M5 ✓ |
| CB-06 | Waterfall countertop edges | P2 | M5 ✓ |
| CB-07 | Custom countertops: holes, edge profiles, thickness, height | P1 | M5 ✓ |
| CB-08 | Custom backsplash that wraps cabinets/openings/appliances | P2 | M5 ◐ backsplash along any path, wrapping corners and ends; automatic cut-outs at windows later |
| CB-09 | Auto/manual fillers with exclusions | P1 | M5 ✓ |
| CB-10 | Blind-corner behaviour | P2 | M5 ✓ |
| CB-11 | Independent side/back/front configuration; side & back thickness | P2 | M5 ✓ |
| CB-12 | Report door/drawer sizes | P2 | M5 ✓ |
| CB-13 | Show doors/drawers open (percent, swing angle) in 2D/3D | P3 | M5 ✓ |
| CB-14 | Box construction (top/bottom/sides on/off) | P2 | M5 ✓ |
| CB-15 | Extended stiles with auto-clipping | P2 | M5 ◐ stiles extend to the floor; auto-clipping against neighbours later |
| CB-16 | Per-edge door overlay | P2 | M5 ✓ |
| CB-17 | Frameless reveal controls | P2 | M5 ✓ |
| CB-18 | Separate materials per component | P1 | M5 ✓ |
| CB-19 | Rounded/chamfered box and countertop corners | P2 | M5 ◐ countertop front corners round or chamfer; box corners later |
| CB-20 | Millwork & end panels (pilasters, feet, legs, moldings; auto end panels) | P2 | M5 ◐ end panels and legs; pilasters and feet styles later |
| CB-21 | Hardware per door/drawer with auto-orientation and placement | P1 | M5 ✓ |
| CB-22 | Stacked moldings with auto-stack | P2 | M5 ✓ |
| CB-23 | Flush cabinet moldings | P3 | M5 ✓ |
| CB-24 | NKBA-standard display options | P2 | M5 ✓ |
| CB-25 | Manufacturer cabinet catalogs | P3 | M8 |

## 8. Doors & windows

| ID | Feature | Pri | M |
|---|---|---|---|
| DW-01 | Parametric doors/doorways/windows incl. arched, shaped, mulled; hardware library | P0 | M1 ✓ basic / M3 (arched, shaped, mulled, hardware) |
| DW-02 | Door types: interior, exterior, sliding, barn, pocket, bifold, atrium, fixed, double, shower, garage | P1 | M3 ✓ |
| DW-03 | Window types: fixed, single/double hung, slider, casement, corner, recessed, louvered, bay, box, pass-through | P1 | M3 ◐ fixed, single/double hung, slider, casement, awning, hopper; corner, bay, box, recessed (P2 items) later |
| DW-04 | Defaults per door type | P1 | M3 ✓ |
| DW-05 | Door glass per side; glass garage panels | P2 | M3 ◐ door glass, full or upper lites, in hinged, double and atrium leaves; garage doors with glass sections; one glass material for both sides |
| DW-06 | Opening indicators; show open/partial in 3D and plan | P1 | M3 ✓ |
| DW-07 | Multi-panel / accordion / stacking doors | P2 | M3 ✓ sliding doors with any number of panels in staggered tracks (stacking), bifolds with many leaves (accordion) |
| DW-08 | Bay/box/bow windows as editable groups incl. roofs and platforms | P2 | M3 ◐ Bay, Box and Bow Window tools build the projecting walls with a window in each face; no roof or separate platform, and the parts are edited one by one |
| DW-09 | Explode bay/box/bow into components | P3 | M3 ✓ bay, box and bow windows are always ordinary walls and windows you can edit or delete |
| DW-10 | Corner windows (with/without post) | P2 | M3 ◐ Corner Window tool: a matched pair meeting at a corner post; no post-less glass corner |
| DW-11 | Mulled units; mulled doors recognized in schedules | P2 | M3 ◐ windows can be mulled (2–8 units with mullions), shown in descriptions and schedules; doors with sidelights not yet |
| DW-12 | Openings in double walls | P3 | M3 ✓ a door or window in either wall of a back-to-back pair cuts through both, in plan and 3D |
| DW-13 | Custom plan symbols for symbol windows | P3 | M5 ✓ any CAD block as a window's plan symbol, stretched over the opening |
| DW-14 | Custom muntins from CAD | P3 | M5 ✓ CAD > Make Window Grille from Selection; plus across × up grids |
| DW-15 | Window treatments (curtains, blinds) | P3 | M5 ✓ curtains, blinds and roller shades with an open amount, in plan and 3D |
| DW-16 | Shutters (parametric or symbol, own layer) | P2 | M5 ◐ louvered, raised-panel and board shutters in plan and 3D; drawn on the window's layer, not their own |
| DW-17 | Lintels, sills, aprons | P2 | M3 ✓ opening Trim: exterior lintel and sill, interior apron (3D) |
| DW-18 | Glazing area and U-factor | P2 | M4 |

## 9. Roofs & ceilings

| ID | Feature | Pri | M |
|---|---|---|---|
| RF-01 | Automatic roofs: gable, hip, shed, Dutch gable, gambrel, mansard; per-eave pitch/overhang/returns; auto-rebuild | P1 | M3 ✓ |
| RF-02 | Auto dormers (floating & structural): eyebrow, gable, hip, shed, gambrel, mansard, barrel, hip-curved | P2 | M3 ◐ Gable and Shed Dormer tools: floating dormers with a window, cheek walls and a roof that runs back into the main roof; no structural, eyebrow, hip or barrel dormers |
| RF-03 | Roof options: raised plate/energy heel, curved, fascia, gutters, exposed rafters, returns | P1 | M3 ◐ raised plate/heel, fascia; curved, gutters, exposed rafters, returns later |
| RF-04 | Roof pivot locks (rafter/truss/remodel) | P2 | M3 ◐ roof plane Change Pitch, keeping the eave or the ridge in place; no per-plane locks for trusses or remodels |
| RF-05 | Draw/edit roof planes in 2D and 3D | P1 | M3 ◐ draw roof planes in 2D; 3D editing later |
| RF-06 | Ridge caps with profiles | P2 | M3 ◐ ridge caps along every ridge and hip (Default Settings > Roofs); one cap profile |
| RF-07 | Rafter spacing per plane | P2 | M6 |
| RF-08 | Skylights with framing; shape editor | P2 | M3 ◐ Skylight tool: curb skylights in roof planes, shown in plan; no framing or shape editor, and rebuilding an automatic roof drops them |
| RF-09 | Automatic ceilings; manual cathedral, tray, coffered, barrel | P1 | M3 ✓ |
| RF-10 | Tray ceiling tool (slope, moldings, rope lights) | P2 | M3 ✓ tray ceilings with inset and rise, straight or sloped sides, and an optional rope light in the cove |
| RF-11 | Layered roof assemblies (SIP roofs) | P2 | M3 |
| RF-12 | Gutters & frieze moldings with custom profiles | P2 | M3 ◐ gutters along the eaves and a frieze board under them (Default Settings > Roofs); fixed profiles |
| RF-13 | Shadow boards on fascia | P3 | M5 ✓ |
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
| ST-13 | Custom newels & balusters (library/import) | P2 | M5 ✓ any 3D symbol as the stair's balusters and newel posts |
| ST-14 | Stair defaults interior/exterior | P1 | M3 ✓ |
| ST-15 | Stairs/ramps saved to library | P2 | M5 ✓ Add Selected saves stairs (and solids, symbols) to the library; groups carry their symbols |
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
| FR-01 | Automatic framing of floors, walls, ceilings, roofs, openings, headers | P2 | M6 ✓ Build > Framing builds wall studs, plates, openings and headers, floor and ceiling joists, and rafters |
| FR-02 | Framing reference markers per level | P2 | M6 |
| FR-03 | Joist direction lines | P2 | M6 ✓ Joist Direction tool sets which way a room's joists run (shown as an arrow) |
| FR-04 | Build All Framing / subsets | P2 | M6 ✓ Build All Framing, or walls / floors and ceilings / roofs separately |
| FR-05 | Manual framing members with info panel | P2 | M6 ✓ add_framing_member; members show size, role and length in their specification |
| FR-06 | Retain manual framing across auto rebuilds (X18+) | P2 | M6 ✓ members placed, moved or edited by hand are kept when framing is rebuilt |
| FR-07 | Framing materials: lumber, steel, LVL, PSL, VSL; custom types and shapes | P2 | M6 ◐ lumber, LVL, PSL, VSL and steel members; no custom section shapes |
| FR-08 | EasyFrame panelization export (.EZF) | P3 | M8 |
| FR-09 | Roof framing: rafters, ridges, lookouts, blocking, valley fill, rafter tails | P2 | M6 ◐ rafters and ridges under each roof plane; no lookouts, blocking, valley fill or rafter tails |
| FR-10 | Auto-build trusses; auto-fit floor/ceiling/roof trusses (scissor, attic, girder, hip, energy heel…) | P2 | M6 ◐ trusses fitted under the roof (common, stepped-down at hips) with Fink webs and heels; no scissor, attic, girder or energy-heel types |
| FR-11 | Truss rebuild tool; truss details with labels | P3 | M6 ✓ Build Roof Framing rebuilds trusses; Truss Detail draws one with its member sizes |
| FR-12 | Truss chords/webbing in materials list (X18+) | P3 | M6 ✓ truss chords and webs in the cut list and buy list |
| FR-13 | Horizontal roof framing (battens, purlins) | P3 | M6 |
| FR-14 | Decorative end profiles | P3 | M6 |
| FR-15 | Post & beam tools | P2 | M6 ◐ Post and Beam tools (6x6 posts, 2-ply beams); no automatic post-and-beam layout |
| FR-16 | Multi-ply beams/posts/girders (X18+) | P3 | M6 ✓ members can be 1–4 plies |
| FR-17 | Wall framing options: plates, studs, bridging, knee walls; steel C/U channels; rim joists | P2 | M6 ◐ stud spacing, 1–3 top plates, mid-height blocking, rim joists; no knee walls or steel channels |
| FR-18 | Balloon framing | P3 | M6 |
| FR-19 | Header controls (cripples, rotated, flush, combined) | P2 | M6 ◐ headers sized by span (2x6 to 2x12, built-up); no per-header options |
| FR-20 | Framing per wall-type layer (furred basement walls) | P3 | M6 |
| FR-21 | Horizontal wall framing (girts) for pole/timber buildings | P3 | M6 |
| FR-22 | Per-opening framing (box/built-up headers, trimmers, sills) | P2 | M6 ✓ king and jack studs, headers, cripples above and below, rough sills |
| FR-23 | Wall framing detail views | P2 | M6 ✓ Build > Framing > Wall Framing Detail draws a wall's framing as a CAD detail |
| FR-24 | Framing labels and member roles for schedules | P2 | M6 ✓ every member has a role; Framing Cut List schedule |
| FR-25 | Build framing per room / per wall / per roof plane | P2 | M6 ◐ rebuild per floor, or for selected walls and roof planes; not per room |
| FR-26 | Corner & intersection framing (standard, 2-stud, laddered) | P2 | M6 ✓ three-stud, two-stud and ladder corners |
| FR-27 | Bearing walls (butt/lap framing) | P2 | M6 |
| FR-28 | Platform holes with auto framing | P2 | M6 ✓ joists stop at stairwell holes, with headers |
| FR-29 | Tray ceiling framing | P3 | M6 |
| FR-30 | Deck framing: planks, borders, beams, joists, posts, footings, blocking; auto-rebuild | P2 | M6 ◐ decks rebuild themselves: planks, border, joists, rims, ledger, beams, posts, footings, railings; no blocking |
| FR-31 | Deck ledger controls (X18+) | P3 | M6 ◐ a ledger is used automatically where a deck meets the house (on/off per deck) |
| FR-32 | Deck beam controls | P3 | M6 ◐ beam inset and largest joist span set where beams go; no per-beam editing |

## 13. CAD tools

| ID | Feature | Pri | M |
|---|---|---|---|
| CAD-01 | Points, lines, polylines, splines, arcs, circles, ovals, boxes, arrows; trim/extend, parallel, perpendicular, reflect, replicate, transform, fillet, chamfer | P0 | M1 ✓ (perpendicular tool: later) |
| CAD-02 | Multiple arc creation modes | P1 | M1 ◐ arcs by centre-start-end and through three points; no tangent arcs |
| CAD-03 | Object snaps (end, center, tangent…), grid snap, reference-floor snap | P0 | M1 ◐ end/mid/center/on-object/angle/grid/reference floor; tangent, intersection, quadrant later |
| CAD-04 | Paste in place across layers/plans/floors | P1 | M1 ✓ Paste Hold Position onto the current floor, into another plan (layers and types matched by name, else added), or to a chosen layer |
| CAD-05 | Point-to-point move/copy/resize | P0 | M1 ✓ |
| CAD-06 | Detailing primitives: cross box, blocking box, insulation | P1 | M4 ✓ |
| CAD-07 | Convert 2D CAD into smart objects (solids, moldings, counters, slabs, roads, terrain, …) | P2 | M4 |
| CAD-08 | Multiple copy at preset increments | P1 | M1 ✓ Multiple Copy repeats a move N times; Rotate with copies makes a circular array |
| CAD-09 | 3D solids and faces; extrude/revolve; polyline boolean ops | P2 | M5 ✓ CAD > Shapes and Solids: join, intersect and subtract closed shapes; extrude or revolve them into 3D solids |
| CAD-10 | 3D solid tool with defaults | P2 | M5 ✓ 3D Solid tool draws a box, cylinder, cone or sphere; shape, height and colour in Default Settings |
| CAD-11 | Solid booleans across plan/elevation; fillet/chamfer solid edges | P3 | M5 ◐ cut shapes before extruding (plan booleans); no 3D solid booleans or edge fillets yet |
| CAD-12 | Regular polygon tool | P1 | M1 ✓ CAD > Regular Polygon: centre, then a corner; sides set in the Tool Palette |
| CAD-13 | Fillet all / chamfer all | P2 | M1 |
| CAD-14 | CAD-to-Walls (DWG lines → walls, windows, doors) | P2 | M4 |
| CAD-15 | Library of structural CAD details | P3 | M8 (own content) |
| CAD-16 | Align / distribute | P1 | M1 ✓ edit toolbar and Edit > Edit Objects > Align and Distribute: edges, centres, even spacing |
| CAD-17 | CAD details & blocks with insertion points; save to library | P1 | M4 ✓ |
| CAD-18 | CAD detail from any view | P1 | M4 ✓ |
| CAD-19 | Copy CAD details between plans | P2 | M4 |
| CAD-20 | Line style manager (create, merge, find usages) | P1 | M1 ✓ Tools > Line Styles…: your own dash patterns, usage counts, delete and merge into another style |

## 14. Layers

| ID | Feature | Pri | M |
|---|---|---|---|
| LY-01 | Layer sets per drawing type; quick swap | P0 | M1 ✓ |
| LY-02 | Layer management: weight, color, style; materials-list inclusion | P0 | M1 ✓ (materials list itself: M4) |
| LY-03 | Layer eyedropper & painter | P2 | M1 ✓ CAD Tools > Layer Painter: click to pick up a layer, then click objects to move them onto it (Alt+click re-picks) |
| LY-04 | Layer hider (click to hide) | P1 | M1 ✓ CAD Tools > Layer Hider: click objects to hide their layers |
| LY-05 | Edit an object's layer properties from the toolbar | P2 | M1 |
| LY-06 | Reference displays: multiple reference floors/layer sets, draw order | P1 | M3 ◐ several reference floors; reference layer sets and draw order later |
| LY-07 | Reference display from other plan files (2D and 3D ghosting) | P2 | M8 |
| LY-08 | 3D reference display styles | P3 | M7 ✓ 3D view Display menu: show, ghost (faint, see-through, not clickable) or hide each kind of object; Framing Overview ghosts walls and roofs |
| LY-09 | Dockable layer display options panel | P1 | GUI ✓ Layer Display tab in the right dock, or Pop Out into a floating window and Dock It Here again |

## 15. Dimensions

| ID | Feature | Pri | M |
|---|---|---|---|
| DIM-01 | Baseline, running, centerline, angular, end-to-end, interior, point-to-point, temporary dimensions; one-click exterior | P0 | M1 ◐ point-to-point/running chains, angular, temporary, auto exterior; baseline, centerline, auto interior later |
| DIM-02 | Dimensions in 3D views | P2 | M7 ✓ 3D > Notes and Dimensions: click two points on the model; kept per camera |
| DIM-03 | Move objects by editing dimensions; arithmetic in inputs | P0 | M1 ◐ arithmetic everywhere, wall/line length via temporary dimension; moving by any placed dimension later |
| DIM-04 | Auto exterior dimensions with auto-refresh | P1 | M1 ✓ Auto Exterior Dimensions are marked automatic and rebuilt whenever that floor's walls or openings change |
| DIM-05 | Wall length editing via dimensions; type exact length while drawing | P0 | M1 ✓ |
| DIM-06 | Dimension snapping rules; hotkey override | P1 | M1 ✓ Default Settings > Text and Dimensions: dimension tool snaps to wall surfaces, framing, centres and opening edges as chosen; hold Alt to snap to nothing |
| DIM-07 | Column/bubble grid lines with auto labeling | P2 | M4 |
| DIM-08 | Units/accuracy (to 1/128" or 0.0078125), text and arrow styles | P0 | M0 ✓ / M1 ✓ |
| DIM-09 | Dimension defaults table editor | P2 | GUI |
| DIM-10 | Centerline markers | P2 | M1 |
| DIM-11 | Inch threshold display (18" vs 1'-6") | P1 | M1 ✓ Default Settings > Text and Dimensions > Show inches below (e.g. 24" shows 18" not 1'-6") |
| DIM-12 | Secondary (dual) dimensions | P2 | M1 |
| DIM-13 | Custom/override dimension text | P1 | M1 ✓ dimension Text Override; {} puts the measured value back in |
| DIM-14 | Rotatable dimension text | P2 | M1 |
| DIM-15 | Laser distance meter (Bluetooth) input | P3 | M8 |
| DIM-16 | Auto elevation dimensions in elevation/section views | P2 | M4 |
| DIM-17 | Multiple named dimension defaults | P1 | M1 ✓ named dimension styles saved in the plan; each dimension and each default set can pick one |
| DIM-18 | Per-tool locate defaults | P2 | M1 |
| DIM-19 | Multiple auto-dimension defaults | P2 | M4 |
| DIM-20 | Inner/outer auto-dimension controls | P2 | M4 |
| DIM-21 | NKBA automatic dimensions | P2 | M5 ✓ |
| DIM-22 | Angled dimension behaviour | P2 | M1 |
| DIM-23 | Story pole dimensions | P3 | M4 |

## 16. Lighting, electrical & HVAC

| ID | Feature | Pri | M |
|---|---|---|---|
| EL-01 | Electrical, plumbing, HVAC symbols; circuits/wiring with auto 3-way/4-way; auto-orient to slopes | P1 | M3 ◐ electrical, plumbing and HVAC symbols; switch wiring with automatic 3-way/4-way; slope orientation later |
| EL-02 | Auto-place outlets by room type (GFCI) | P2 | M3 |
| EL-03 | Light fixtures emit light in renders | P1 | M5 ✓ |
| EL-04 | Rope lighting along paths | P3 | M7 ✓ Terrain > Site > Rope Light: glows in 3D, with height, brightness per foot and colour temperature |
| EL-05 | Per-fixture light controls (shadows, intensity, dimming, on/off) | P1 | M5 ◐ on/off, brightness, dimmer, colour temperature and a shadows switch; light from fixtures doesn't cast shadows yet |
| EL-06 | Lighting manager table | P2 | M7 ✓ 3D > Lighting Manager: every light in one table: on/off, dimmer, brightness, colour temperature and shadows |
| EL-07 | Kelvin color temperature (X18+) | P2 | M5 ✓ |
| EL-08 | Lighting sets per view/camera | P2 | M7 ✓ each camera can turn individual lights off (a lighting set) |

## 17. Terrain, site & landscape

| ID | Feature | Pri | M |
|---|---|---|---|
| SITE-01 | North pointer, sun angles from lat/long/date/time; 2D and 3D shadows | P1 | M3 ✓ |
| SITE-02 | Copy sun angles for studies | P3 | M7 ✓ a camera can keep its own sun angle (sun studies) |
| SITE-03 | Bearing/angle display formats (azimuth, quadrant, DMS) | P1 | M1 ◐ Preferences > Angles: decimal, DMS, bearings or azimuth for angular dimensions and line/wall directions; plot-plan labels keep the bearing as typed |
| SITE-04 | Terrain model from elevation data: smoothing, triangulation, skirt, contours | P2 | M6 ✓ terrain from elevation data: trend plane plus distance blending, smoothing passes, building cut-out, skirt, contours |
| SITE-05 | Plot plans, site plans, subdivisions from bearings, distances and setbacks | P2 | M6 ◐ Terrain > Plot Plan from Bearings: lot lines from bearings and distances, labelled, with a setback line; no multi-lot subdivisions |
| SITE-06 | Terrain import: text/GPS/GPX, DWG/DXF | P2 | M6 ◐ text/CSV x,y,z and GPX survey points; no DWG/DXF terrain import |
| SITE-07 | Elevation splines, lines, regions, points; subfloor marker; clear terrain | P2 | M6 ◐ elevation points, lines (smooth splines) and flat regions; Clear Terrain; no subfloor marker |
| SITE-08 | Terrain elevation labels | P2 | M6 ✓ spot heights and major contours labelled in plan |
| SITE-09 | Terrain modifiers: hills, valleys, elevated and flat regions | P2 | M6 ✓ hills, valleys and flat (elevated) regions |
| SITE-10 | Retaining walls and terrain breaks | P2 | M6 ◐ retaining walls with a top height that holds the terrain along them; no separate terrain-break lines |
| SITE-11 | Stepping stones, streams, curbing, garden beds | P3 | M6 ◐ stepping stones, streams and raised garden beds; curbing comes with roads |
| SITE-12 | Roads, medians, striping, curbs, gutters, driveways, sidewalks, cul-de-sacs | P2 | M6 ◐ roads (curbs, gutters, centre striping), driveways and sidewalks that follow the ground; no medians or cul-de-sacs |
| SITE-13 | Road/driveway/sidewalk volumes in materials list | P3 | M6 ✓ paving area and base volume in the materials list |
| SITE-14 | Auto plan footprint CAD detail for site plans | P2 | M6 ✓ Terrain > Site Plan Detail makes a CAD detail of the lot, footprints, site work and plants |
| SITE-15 | Deck designer (straight, curved, polygon, covered) | P2 | M6 ✓ Deck tool: straight, polygon or curved outlines, optional cover on posts |
| SITE-16 | Fences that follow terrain; gates | P2 | M6 ✓ picket, privacy and split-rail fences with posts following the ground; gates |
| SITE-17 | Sprinkler designer (zones, runs) | P3 | M8 |
| SITE-18 | Plant images/symbols in 2D/3D | P2 | M6 ✓ trees, conifers, shrubs and grasses as plan canopies and 3D shapes |
| SITE-19 | Import own plant images with growth data | P3 | M6 ◐ growth data (years to mature, show at age); no imported plant images |
| SITE-20 | Plant chooser with hardiness zone maps | P3 | M8 |
| SITE-21 | Plant encyclopedia data | P3 | M8 (own data) |
| SITE-22 | Landscaping plans and plant schedules | P3 | M6 ✓ plant schedule; plants and site work in the materials list |
| SITE-23 | Plant growth over time | P3 | M8 |
| SITE-24 | 3D plants with shadows | P2 | M7 ✓ plants cast sun shadows in 3D |

## 18. 3D design & materials

| ID | Feature | Pri | M |
|---|---|---|---|
| MAT-01 | Edit any architectural object in 2D or 3D | P0 | M1 ✓ (select and edit from 3D; direct 3D dragging later) |
| MAT-02 | Object painter / eyedropper with attribute choice and scope | P2 | M5 ✓ Object Painter tool: Alt+click source, tick properties, click or paint the whole floor |
| MAT-03 | Material painter with scope; default painter (reset) | P1 | M5 ✓ |
| MAT-04 | Material eyedropper; color chooser from image | P2 | M5 ◐ colour from an image's average; no on-screen eyedropper |
| MAT-05 | Adjust-material scoping (whole plan vs copy for subset) | P2 | M5 ✓ Copy for Selection gives chosen objects their own copy |
| MAT-06 | Fill styles: pattern scale/angle/offset/transparency; gradients; fill painter | P1 | M1 ◐ fill styles per layer or shape: solid (with opacity), hatch, crosshatch, dots, with spacing, angle and colour; no gradients, offset or fill painter |
| MAT-07 | Parametric herringbone/grid patterns | P2 | M5 ✓ grid, running bond, herringbone and basketweave tile patterns |
| MAT-08 | Custom patterns to library (X18+) | P2 | M5 ✓ Add to Library keeps the pattern |
| MAT-09 | Animated water materials | P3 | M7 ✓ Water material class ripples in 3D |
| MAT-10 | Emissive materials as area lights | P2 | M7 ✓ emissive materials light their surroundings |
| MAT-11 | PBR maps: AO, roughness, metal, emissive, anisotropic, normal, bump | P1 | M5 ✓ colour maps textured in 3D with mipmaps; PBR values shaded live; other image maps stored |
| MAT-12 | Interactive material editing (scale/rotation/origin in 3D) | P2 | M5 ✓ painter bar adjusts tile size and turn live; angle and offset in the material |
| MAT-13 | Material builder: custom tiles, grout; Substance (.sbsar) import | P2/P3 | M5/M8 |
| MAT-14 | Material classes (plastic, metal, mirror, glass, translucent…) | P1 | M5 ✓ |
| MAT-15 | Material maps incl. opacity; roughness/gloss inversion | P1 | M5 ✓ maps stored; colour textures rendered |
| MAT-16 | Plan materials manager (replace, merge, search) | P1 | M5 ✓ |
| MAT-17 | Hatch pattern editor | P2 | M4 |
| MAT-18 | Generate vector hatch pattern from texture | P3 | M5 ◐ patterned floor regions get a tile-joint hatch in plan |
| MAT-19 | Screen-capture image as material | P3 | M5 |
| MAT-20 | 3D cladding from moldings (board & batten, lap, standing seam) (X18+) | P2 | M5 ✓ |
| MAT-21 | Style palettes (save and apply finish sets) | P2 | M5 ✓ save and apply styles (materials, door style, hardware) |
| MAT-22 | Center object between two objects | P1 | M1 ✓ Center Object button: then click a wall or line to centre along it, or an object to centre on it |
| MAT-23 | Match / apply properties across same-type items | P1 | M1 ✓ Match Properties works for any kind of object; placement and facing are kept |
| MAT-24 | Import custom 3D symbols | P1 | M5 ◐ OBJ (with MTL colours) and STL models stored in the plan, placed and stretched like cabinets, paintable; glTF, DAE and SKP later |
| MAT-25 | Smart symbols (sinks cut counters, wall-hung items, light fixtures emit) | P2 | M5 ✓ symbols can behave as sinks (cut countertops), wall-hung (hang at a set height) or light fixtures (light the 3D view) |
| MAT-26 | Convert selection to symbol object | P2 | M5 ✓ Library > Convert Selection to Symbol keeps the 3D look in one symbol |
| MAT-27 | Custom molding profiles; stacked, flipped, rotated, centered | P2 | M5 ✓ |
| MAT-28 | Edit molding polylines from library (X18+) | P3 | M5 ◐ custom profiles made from CAD shapes and edited per molding; library storage later |
| MAT-29 | Distributed regions/paths with randomized variation | P3 | M6 |

## 19. Library / catalog

| ID | Feature | Pri | M |
|---|---|---|---|
| LIB-01 | Library browser of objects; create, import, download | P1 | M5 ◐ built-in catalogue (cabinet sizes, appliances, fixtures, materials, wall types) and a user library: add selected objects, import/export .hclib bundles; downloading from an online catalogue later |
| LIB-02 | Catalog migration between versions | P3 | M8 |
| LIB-03 | Catalog update service | ✗ | — (own content feed later) |
| LIB-04 | Update catalogs on demand | ✗ | — |
| LIB-05 | Search with AND/OR/NOT/grouping; search attributes | P1 | M5 ✓ |
| LIB-06 | Tags, filters, favorites, saved filters | P2 | M5 ✓ tags (Edit…), favourites folder, saved searches as buttons |
| LIB-07 | Edit CAD blocks in place; apply to all instances | P2 | M5 ✓ CAD > Blocks > Edit Block opens the block; every copy updates |
| LIB-08 | Patterns and fill styles in library | P2 | M5 |
| LIB-09 | Notes, callouts, markers in library | P2 | M5 ✓ built-in notes and callouts; save your own with Add Selected |
| LIB-10 | Text in library | P3 | M5 ✓ text saves to and places from the library |
| LIB-11 | Library shortcuts | P3 | M5 |
| LIB-12 | Replace from library (one or all identical) | P1 | M5 ◐ cabinets and appliances, one or all identical; blocks later |
| LIB-13 | Library painter with scope | P2 | M5 ✓ |
| LIB-14 | Edit parametric items stored in library | P2 | M5 ✓ right-click > Edit… changes an item's name, folder, tags and properties (built-ins save an edited copy) |
| LIB-15 | Edit symbol properties in library | P2 | M5 ✓ symbol behaviour and properties editable in the library |
| LIB-16 | Find a selected object in the library | P3 | M5 ✓ Find Selected searches the library for the selected object |
| LIB-17 | Save components (doors, hardware, moldings) to library | P2 | M5 ◐ doors, windows (with grilles, shutters, curtains) and molding profiles; no separate hardware items |
| LIB-18 | Read user CA libraries (`.calib`/`.calibz`) | P2 | M8 |

## 20. Materials list & cost estimating

| ID | Feature | Pri | M |
|---|---|---|---|
| EST-01 | Auto materials list (equipment, unit cost, markup, labor); plan / room / area scope; live update | P1 | M4 ◐ take-off with unit prices and markup, whole plan or floor; labor and room/area scope later |
| EST-02 | Master materials list (pricing master data) | P2 | M4 |
| EST-03 | Find in plan from materials list | P1 | M4 ✓ |
| EST-04 | Per-object code/comment/description/manufacturer fields | P1 | M1 ◐ Object Information fields on any object, usable in labels ({code}, {description}…); not yet columns in schedules or the materials list |
| EST-05 | Object components panel (calculation methods) | P2 | M4 |
| EST-06 | Layer-based inclusion | P1 | M4 ✓ |
| EST-07 | Scripted formulas (Rhai/Lua; CA uses Ruby) | P2 | M8 |
| EST-08 | Materials-list layer sets | P2 | M4 |
| EST-09 | Schedules: doors, windows, cabinets, fixtures, appliances, furniture, electrical, HVAC, notes, plumbing, plants, room finish, walls, moldings, roof/ceiling planes, skylights, millwork, sprinklers, shapes; floor/room filters | P1 | M4 ◐ doors, windows, room finish, walls, electrical/plumbing/HVAC, stairs, roof planes; cabinets, appliances, furniture, moldings (M5) later |
| EST-10 | Schedule formatting: styles, transpose, sort, wrapping | P1 | M4 ◐ choose and order columns; styles, transpose, sort and wrap options later |
| EST-11 | Custom columns from object info fields | P1 | M4 ◐ any of a schedule's built-in columns; user-defined object fields later |
| EST-12 | Callout label shapes | P2 | M4 |
| EST-13 | Custom mixed-type schedules; existing/new splits | P2 | M4 |
| EST-14 | Schedules in library | P3 | M5 ✓ schedules save to and place from the library |
| EST-15 | Polyline objects in schedules | P2 | M4 |
| EST-16 | Find in plan from schedule | P1 | M4 ✓ |
| EST-17 | Open object spec from schedule row | P1 | M4 ✓ |
| EST-18 | Schedule thumbnails | P3 | M4 |
| EST-19 | Schedule starting number | P3 | M4 |
| EST-20 | Export schedules: CSV, TXT, XML | P1 | M4 ✓ |
| EST-21 | BuilderTREND export with NAHB accounting codes | P3 | M8 |
| EST-22 | Cut list and buy list (structural lumber) | P2 | M6 ✓ Framing Cut List schedule; buy list (stock lengths) in the materials list |
| EST-23 | Pricing in framing buy lists | P3 | M6 ✓ framing buy-list lines take prices like any materials-list line |

## 21. Text, annotation & scripting

| ID | Feature | Pri | M |
|---|---|---|---|
| TXT-01 | Editable text: font, size, weight, color, justification | P0 | M1 ◐ size, weight, color, justification; font families need font loading |
| TXT-02 | Rich text, hyperlinks | P1 | M1 ◐ note markup: **bold**, __underline__, [label](link), {#rrggbb}colour{/}; no italic or mixed fonts |
| TXT-03 | Find/replace text | P2 | M4 |
| TXT-04 | Replace fonts | P3 | M4 |
| TXT-05 | Text arrows / leaders with auto orientation | P1 | M1 ✓ Text with Arrow tool; leaders start from whichever side of the note faces the tip and move with it |
| TXT-06 | Custom arrowheads from CAD blocks | P2 | M4 |
| TXT-07 | Callouts & markers (styles, own layers) | P1 | M4 ✓ |
| TXT-08 | Linked callouts that navigate between views | P2 | M4 |
| TXT-09 | Object labels with macros | P1 | M1 ✓ Default Settings > Object Labels: templates for doors, windows, cabinets, fixtures with {width} {size} {type} {code}…; per-object label or - to hide |
| TXT-10 | Label formatting (justify, border, wrap) | P1 | M1 ◐ border and wrap width for notes and labels, justify for notes; labels are always centred |
| TXT-11 | Per-view label hide (X18+) | P2 | M1 |
| TXT-12 | Auto labels for layout boxes | P2 | M4 |
| TXT-13 | Text styles per layer/default/object | P0 | M1 ✓ |
| TXT-14 | Default sets for annotation | P1 | M1 ✓ default sets carry the text style, dimension defaults or style, label templates and layers for annotation; plan views and camera default sets pick them |
| TXT-15 | Annotations on 3D views | P2 | M7 ✓ 3D > Notes and Dimensions: notes pinned to the model, shown for one camera or all |
| TXT-16 | Numbered notes with schedule; note markers in 3D | P2 | M4 |
| TXT-17 | Spell check | P3 | GUI |
| TXT-18 | Hyperlinks | P2 | M1 ◐ [label](url) links in notes, Alt+click opens them; not yet clickable in exported PDFs |
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
| IO-03 | DXF/DWG import & export (2D) | P1 | M1 (DXF) / M8 (DWG) ◐ DXF (R12) export of plans, views and details from File > Export and the CLI; DXF import of lines, polylines, arcs, circles and text onto matching layers (blocks and hatches skipped); no DWG |
| IO-04 | 3D import: DXF, DWG, OBJ, 3DS, SKP, STL, DAE; 3D export: DXF, DWG, 3DS, STL, DAE (+ glTF/OBJ/IFC for us) | P1→P3 | M5/M8 |
| IO-05 | Image import: BMP, JPG, GIF, TIFF, PNG, metafiles, WEBP; transparency color | P1 | M5 ◐ PNG, JPEG, BMP and TIFF pictures with a transparency colour, in plans, views, details and layouts (SVG embeds them); GIF, WebP, metafiles and pictures in PDF/PNG exports later |
| IO-06 | High-resolution image export, transparent background | P1 | M7 ✓ |
| IO-07 | Import/export settings objects (views, default sets, layer sets, hotkeys, wall types, line styles, macros, scripts) | P2 | M5 ◐ File > Export/Import Settings: wall types, room types, text styles, layers, materials, molding profiles (not views, default sets, hotkeys or macros) |
| IO-08 | 3D mass import → library objects | P2 | M5 ✓ Library Browser > Import 3D Models… adds every OBJ/STL in a folder as library symbols |
| IO-09 | Textures → materials in bulk | P2 | M5 ✓ Import Textures… makes a material per image in a folder |
| IO-10 | Terrain survey import | P2 | M6 ✓ Terrain > Import Survey Points… (x,y,z text or GPX) |
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
| UI-14 | Virtual graph paper grid | P1 | M1 ✓ graph-paper reference grid: lavender minor lines with a stronger line every fourth |
| UI-15 | Auto-archive backups on schedule | P1 | M1 ✓ changed plans are copied every N minutes (Preferences) into a "<plan> Archive" folder, keeping the newest few; File > Show Auto-Archive Folder |
| UI-16 | Single-file project archive (`.CAPROJ` equivalent) | P2 | M8 |
| UI-17 | Crosshair thickness | P3 | GUI |
| UI-18 | UI themes | P3 | GUI |
| UI-19 | Customizable toolbars and toolbar sets | P2 | GUI |
| UI-20 | Hotkeys incl. sequential chords | P1 | GUI ◐ Help > Keyboard Shortcuts: give any tool a key or a two-key sequence ("G W"); the built-in keys can't be changed yet |
| UI-21 | VR (OpenXR) | P3 | M8 |
| UI-22 | Context-sensitive help | P2 | GUI |
| UI-23 | License activation | ✗ | — |
| UI-24 | Document tabs for open views (Dashboard, plan, 3D, layouts) under the toolbars | P1 | GUI ◐ tab strip under the toolbars for the plan, 3D views and layout pages, with close buttons; no Dashboard tab |
| UI-25 | Plan canvas stays white with a light lavender grid in every theme; red X / blue Y origin marker | P1 | GUI ✓ plans stay on white paper in every theme (Preferences toggle), with lavender grid and red X / blue Y origin |
| UI-26 | Typed Tool Search box in the top toolbar; floor stepper (down, floor number, up) beside the plan-view list | P1 | GUI ✓ Tool Search box and floor stepper (down, number, up) in the top toolbar |
| UI-27 | Right dock order Library, Project Browser, Active Layer Display; Project Browser with filter box, Project Folders / Projects tree, Details and Preview panes | P1 | GUI ◐ right dock order Library, Project Browser, Layer Display; browser filter box and Details pane; no Project Folders tree or preview |
| UI-28 | Status bar: hint, current tool [hotkey], floor, layer, drawing group, live X/Y/Z, view size in pixels | P1 | GUI ◐ status bar shows hint, tool [hotkey], floor, layer, X/Y/Z and view size in pixels; no drawing groups yet |
| UI-29 | Account menu before Help; left dock titled "Tool Palette" | P2 | GUI |
