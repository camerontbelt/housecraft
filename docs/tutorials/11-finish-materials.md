# Tutorial 11, Finish Materials: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Interior Design Tutorials › Finish Materials**
(pages 191–210). It carries on from [Tutorial 10](10-custom-ceilings.md). The steps also run as the automatic test
[11-finish-materials.json](11-finish-materials.json).

## A colour palette (pages 192–194)

*Different in HouseCraft:* there's no shipped colour catalog to copy from. Make the five colours as plan
materials instead: **Tools › Plan Materials…** › new: `Color - Butter`, `Color - Dew`, `Color - Fresh`,
`Color - Robin's Egg`, `Color - Maya Gold`. Pick each colour with the colour box.

## Painting walls (pages 194–198)

Open a 3D view and choose **3D › Material Painter**. The bar at the top of the view picks the material and how far
the paint reaches:

| Chief | HouseCraft Material Painter reach |
|---|---|
| All rooms (Floor/Ceiling defaults) | **Whole plan**, then click any interior wall |
| Room Mode | **Whole object**, then click inside the room's wall (paints that room's walls) |
| Object Mode (one wall) | **This surface**, then click the wall |

1. Butter everywhere: **Whole plan**, click a wall.
2. **Material Eyedropper:** with the Material Painter on, **Alt+click** a Butter wall to pick its material up.
3. Garage walls **Color - Fresh**, Bedroom walls **Color - Dew**: **Whole object**, click inside each room.
4. One wall **Color - Maya Gold**: **This surface**, click the wall.
5. *Not in HouseCraft yet:* a wall that faces three rooms taking each room's colour on its side.

## Wall coverings and the tiled shower (pages 198–203)

1. *Not in HouseCraft yet:* wall coverings (a beadboard wainscot band round a room) and material regions made of
   layers. Use a **Wall Material Region** for both.
2. Make a `Shower Tile` material. Choose **Wall Material Region** (CAD Tools), click the Bath's wall, then open
   the region and set it to start 18" up and stop 12" below the ceiling.
3. *Not in HouseCraft yet:* the single-wall Wall Elevation view, and Simplify Polyline. Place a second region on
   the opposite wall the same way.

## Flooring (pages 203–209)

Make a `Red Oak 3-4-5" Plank - Natural` material and paint it with **Whole plan** on any floor. Rooms whose
type has its own floor finish (Kitchen tile, Bath tile, Garage concrete) keep theirs.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| A shipped colour/material catalog; folders of materials | LIB-01 |
| Walls painted per room they face | MAT-31 |
| Wall coverings | MAT-32 |
| Material regions as layers that cut the finish | WL-16 |
| Single-wall Wall Elevation view | SEC-14 |
