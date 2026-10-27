# Tutorial 25, Landscaping Design: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Landscaping Tutorials › Landscaping Design** (pages 433–450).
It carries on from [Tutorial 24](24-driveways-sidewalks-roads.md). The steps also run as the automatic test
[25-landscaping-design.json](25-landscaping-design.json).

**Garden Bed**, **Fence** and the plant tools (**Tree**, **Conifer**, **Shrub**, **Grass / Perennial**) are on the
**Terrain** menu and in the Terrain tool list.

## Layers and materials (pages 434–435)

1. In the **Layer Display** side window, with **Landscaping Layer Set** active, lock the road and sidewalk layers and
   hide the elevation data.
2. *Not in HouseCraft yet:* a "Dirt" material default for garden beds. Paint a mulch material onto a bed with
   **3D › Material Painter** if you want one.

## Garden bed (pages 435–437)

1. **Garden Bed**: click round the area between the porch stairs, the Garage, the sidewalk and the driveway. Press
   Enter to close it.
2. Open it and set **Height** `0`. That makes a flat mulch bed lying on the ground, like Chief's. A height above 0
   makes a raised bed with edging.
3. *Not in HouseCraft yet:* Add Break, moving a single edge, and curved edges on a bed. Drag its corners into place.

## Planting border (pages 437–440)

1. Draw a CAD polyline round the border you want (or copy the lot outline and **Edit › Paste Hold Position**).
2. Select it, then **CAD › Convert CAD To › Garden Bed**. Open the bed and set **Height** `0`.
3. To line an edge up with a setback line, select the bed, choose **Edit › Make Parallel / Perpendicular**, click
   the setback line, then click the bed's edge. The whole bed turns. *Not in HouseCraft yet:* moving a single edge,
   so drag the corners onto the setback lines with object snaps on.

## Fencing and a gate (pages 440–443)

1. *Not in HouseCraft yet:* fencing defaults (panel styles, rail and post sizes).
2. **Fence**: click along the left setback line from the sidewalk to the back, along the back, and down the right
   angled setback line. Press Enter. Open it: **Style** Privacy, **Height** `6'`.
3. HouseCraft fences don't replace retaining walls, so there's nothing to undo.
4. In the fence's dialog, click **Add Gate** under **Gates** and set where it starts along the fence and its width.
   *Not in HouseCraft yet:* gate styles from the library and a separate gate height.

## Plants (pages 443–447)

1. *Not in HouseCraft yet:* the Plant Chooser and plant encyclopedia. Use **Conifer** and open it: **Name**
   `Balsam Fir`. Place it at the top right corner of the lot.
2. **Tree** twice behind the retaining walls. Open each: **Name** `Paper Birch`, **Mature Height** `40'`,
   **Spread** `25'`. Make one a little smaller so they don't look identical. HouseCraft plants are 3D shapes, so
   there's no image to reverse.
3. Three **Shrubs** close together, named `Rhododendron (cream)`. Select all three and copy them along the angled
   retaining wall (**Multiple Copy…**). *Not in HouseCraft yet:* architectural blocks to group them.

## Schedule and revision (pages 447–449)

1. **Tools › Schedules › Place in Plan › Plant Schedule**, then click beside the lot.
2. **File › Make a Copy…** `Chic Cottage-Landscaping`. Make a `Landscaping Tutorials` folder under
   **Projects** and drag the landscaping revisions into it.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Dirt material default; fence panels, rails, posts; gate styles | SITE-34 |
| Add Break, edge moves and arcs on garden beds | SITE-30 |
| Plant Chooser and encyclopedia | SITE-20, SITE-21 |
| Architectural blocks | CB-27 |

**New while following this tutorial:**

- Garden beds can be flat (height 0), lying on the ground like a mulch bed.
- **CAD › Convert CAD To › Garden Bed**.
