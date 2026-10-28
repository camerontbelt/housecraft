# Tutorial 9, Dormers: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Roof Tutorials › Dormers** (pages 159–176). It works on
a copy of the plan from [Tutorial 8](08-chic-cottage-roof.md). The steps also run as the automatic test
[09-dormers.json](09-dormers.json).

## Make the option file

**File › Save As…** `Chic Cottage-Dormer Option`, and work in that file. (Chief makes a copy and opens it; Save As does
both. **File › Make a Copy…** would leave you in the original.)

## Auto floating dormer (pages 161–163)

1. Dormer defaults: **Edit › Default Settings…** › **Dormers**: **Width** `96"`, tick **Frieze**. The dormer's walls
   look like the plan's exterior walls. *Not in HouseCraft yet:* picking a wall type for dormers.
2. **Edit › Default Settings… › Windows**: width `72"`, height `48"`, head height `93 1/2"`.
3. On **Floor 1** (the roof is built there), choose **Roof Tools › Gable Dormer** and click on the front slope,
   left of the stairs. It starts 96" wide. Drag it so it's centred on the Dining room with its front 2' in from the
   front wall.
4. Open the dormer to set its window: **Window Width** `72"`, **Window Height** `48"`, **Window Top** `56"` (above
   the dormer's floor) and **Sashes Across** `2` for a double casement.

## Structural dormer (pages 163–165)

1. Draw an interior wall on Floor 2, 8' in from the back wall. That's the knee wall. *Not in HouseCraft yet:* the
   Knee Wall roof setting, ceiling break lines, and the Attic room type.
2. Place another **Gable Dormer** on the back slope, centred on the first, 2' in from the back wall.
   Open it and tick **Structural**: the main roof is cut away under it, so it opens into the attic. (Untick it for a
   floating dormer that just sits on the roof, like the front one. Tick **Structural** in the dormer defaults to make
   new dormers structural.)

## Manual dormer (pages 166–171)

1. *Not in HouseCraft yet:* the Roof Hole tool.
2. On Floor 2, draw three exterior walls for the dormer: a 3' and an 8' side and a front 2' from the back wall.
   Put a window in the front one.
3. Draw the two dormer roof planes with **Roof Tools › Roof Plane**, one over each side wall.
4. *Not in HouseCraft yet:* Join Roof Planes (for the ridge and the valleys) and fitting a roof hole to the walls.

## Annotations (pages 171–175)

1. **Text** tool: `VALLEY FLASHING`. Open it and set the rotation to match the valley.
2. **Rectangular Polyline** (CAD) along the valley, and **Text with Arrow**: `FLASHING AT WALL`.
3. To line the text or polyline up with the valley, select it, choose **Edit › Make Parallel / Perpendicular**, click
   the valley line, then click an edge of your selection. For the opposite valley, select the notes and use
   **Edit › Reflect About Object**, Ctrl+clicking a CAD line drawn along the ridge to make mirrored copies.

## Filing the roof plans (page 175)

Under **Projects** in the Project Browser, right-click the project › **New Folder…** `Roof Tutorials`. Ctrl+click
Chic Cottage-Roof and Chic Cottage-Dormer Option, then drag them onto the new folder.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| A wall type for dormer walls; framing cut round structural dormers | RF-22 |
| Knee Wall setting, ceiling break lines | RF-21 |
| Attic room type | RM-18 |
| Roof Hole tool | RF-16 |
| Join Roof Planes | RF-18 |
