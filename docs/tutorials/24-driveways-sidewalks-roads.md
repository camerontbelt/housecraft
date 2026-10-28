# Tutorial 24, Driveways, Sidewalks, and Roads: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Landscaping Tutorials › Driveways, Sidewalks, and Roads**
(pages 420–432). It carries on from [Tutorial 23](23-terrain-elevation.md). The steps also run as the automatic test
[24-driveways-sidewalks-roads.json](24-driveways-sidewalks-roads.json).

**Road**, **Driveway** and **Sidewalk** are on the **Terrain** menu (and the Terrain tool list). Click along the
centre line, then double-click or press Enter to finish. Open one to set its **Width**, **Thickness** and curbs. Each
lies on the terrain. Roads also have **End Flare**, **Stripes** and **Street Name**.

## The road (pages 421–425)

1. Road defaults: **Edit › Default Settings…** › **Roads, Driveways and Sidewalks**: **Road width** `28'`, tick **Road
   curbs**. New roads start with these. Where a driveway or sidewalk ends at a road with curbs, the curb is cut away
   for it (in plan and 3D).
2. **Road**: click the bottom-right corner of the terrain, then the lower-left corner. Press Enter.
3. To curve the road along the lot's curved edge, select it, choose **Edit › Change Line/Arc…**, click the edge, type the **Radius** and click **OK** (tick **Bow to the other side** to curve it the other way). Use `450'`. To snap the road onto the lot
   edge, use **Edit › Point to Point Move**.
4. Road stripes for a bike lane: open the road and type `8', -8'` in **Stripes**. Each number is how far a painted
   line sits from the road's centre line (+ is to the left as drawn, − to the right). On a 28' road, 8' is 6' in from
   each edge. Painted stripes replace the dashed centre line.

## Road in 3D and its height (pages 425–427)

1. Chief only shows roads inside the terrain perimeter. HouseCraft draws them anyway, but pull the terrain's edge
   past the road so the ground meets it.
2. Copy the front elevation line (`-8'`) to the middle of the road. The slope down to the road gets gentler. The
   ground doesn't pass exactly through an elevation line where the data is steep; smoothing can leave it a foot or
   so off.

## Driveway (pages 427–428)

1. In the same defaults: **Driveway width** `13'`, **Driveway end flare** `3'`.
2. **Driveway**: click at the middle of the Garage door, then straight down at the road. Press Enter. Open it:
   **Width** `13'`, **End Flare** `3'`. The flare widens the end you finished at (where it meets the road) by 3' on
   each side.
3. For blacktop, use **3D › Material Painter** on the driveway with an asphalt material.

## Sidewalks (pages 428–431)

1. Open the road and click **Auto Generate Sidewalks**. You get a 5' sidewalk down each side, 5' (60") beyond the curb.
2. **Sidewalk**: click at the foot of the porch stairs, down a few feet, then across to the driveway. Press Enter.
   Open it: **Width** `5'`.
3. For a curved sidewalk, select it, choose **Edit › Change Line/Arc…**, click the edge, type the **Radius** and click **OK** (tick **Bow to the other side** to curve it the other way).

## Annotations and revision (pages 431–432)

Open the road and type its name in **Street Name**. The name is drawn along the road in plan, always reading left to
right. For a hardscaping schedule, **Tools › Schedules › Place in Plan › Hardscape Schedule** and click beside the lot: it
lists each road, driveway and sidewalk with its width, length and area. Then **File › Make a Copy…** `Chic Cottage-Hardscape`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Terrain passing exactly through elevation data | SITE-33 |
