# Roof tutorials, roof styles: following them in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Roof Tutorials** (pages 127–135: Getting Started, Hip,
Gable, Dutch Gable, Shed, Offset Gable, Gambrel, Gull Wing, Half Hip, Mansard). These use a fresh plan, not the
Chic Cottage. The steps also run as the automatic test [07-roof-styles.json](07-roof-styles.json).

## Setting up

1. **File › New Plan**. Double-click **Floor 1** in the Project Browser and set **Ceiling Height** to `109 1/8"`
   (Chief's default; the heights below assume it).
2. **Straight Exterior Wall**: click round a rectangle about 34' × 24', **clockwise**.

## Where the roof settings are

Select one or more walls (Shift+click) and double-click. Under **Roof**:

| Chief (Wall Specification › Roof) | HouseCraft |
|---|---|
| Full Gable Wall | **Full Gable Wall** |
| Pitch | **Roof Pitch**: type `8/12` (or leave empty for the default) |
| Upper Pitch + Starts at Height | **Upper Roof** › **Break Height** and **Upper Pitch** |
| Dutch Gable Wall | **Upper Roof** with **Gable above the break** |
| High Shed/Gable Wall | **Full Gable Wall** on the high side |

**Important difference:** Chief's **Starts at Height** is measured from the floor. HouseCraft's **Break Height**
is measured from the **top of the wall**, so subtract the ceiling height: with 109 1/8" ceilings, Chief's
`156"` is HouseCraft's `46 7/8"`.

The default pitch is in **Edit › Default Settings… › Roofs**. Build the roof with **Build › Roof › Build Roof**
(Ctrl+R), and remove it with **Build › Roof › Remove Roof**. *Not in HouseCraft yet:* Edit › Reset to Defaults
for roof directives, and a "Walls, Attic" layer.

## The styles

| Style | Walls | Pitch | Break Height (HouseCraft) |
|---|---|---|---|
| Hip | nothing set | 8/12 default | — |
| Gable | left and right **Full Gable Wall** | | — |
| Dutch gable | left and right: Upper Roof, **Gable above the break** | 8/12 | `70 7/8"` (Chief 180") |
| Shed | left, right **and the high wall** Full Gable Wall | default 2/12 | — |
| Offset gable | gables left/right; bottom wall Roof Pitch `12/12` | default 2/12 | — |
| Gambrel | gables left/right; top and bottom Upper Pitch `6/12` | default 12/12 | `46 7/8"` (Chief 156") |
| Gull wing | gables left/right; top and bottom Upper Pitch `12/12` | default 3/12 | `15 7/8"` (Chief 125") |
| Half hip | left and right Full Gable Wall **and** Upper Pitch `3/12` | 8/12 | `60 7/8"` (Chief 170") |
| Mansard | all four walls Upper Pitch `1.5/12` | default 24/12 | `22 7/8"` (Chief 132") |

**Build › Roof › Roof Style** sets Hip, Gable, Shed, Mansard, Gambrel or Dutch Gable for the whole house in one step.

HouseCraft draws a Dutch gable's or half hip's long sides in two pieces (below and above the break) at the same
pitch. In 3D it looks the same as Chief's.

**Fixed for these tutorials:** a shed roof (one sloping side, the other three walls gables) built nothing.
It now builds the single plane with gable walls under it.

## Finding the start of an upper pitch (page 135)

Make a cross section (**3D › Elevations and Sections › Cross Section / Elevation Tool**) and measure with a manual
dimension from the top of the wall (not the floor) to where the pitch should change. That's the Break Height.

## Not in HouseCraft yet

| Missing | FEATURES row |
|---|---|
| High Shed/Gable option; Starts at Height from the floor or In From Baseline; attic wall layer; Reset to Defaults | RF-17 |
