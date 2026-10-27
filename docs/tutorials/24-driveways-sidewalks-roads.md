# Tutorial 24, Driveways, Sidewalks, and Roads: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Landscaping Tutorials › Driveways, Sidewalks, and Roads**
(pages 420–432). It carries on from [Tutorial 23](23-terrain-elevation.md). The steps also run as the automatic test
[24-driveways-sidewalks-roads.json](24-driveways-sidewalks-roads.json).

**Road**, **Driveway** and **Sidewalk** are on the **Terrain** menu (and the Terrain tool list). Click along the
centre line, then double-click or press Enter to finish. Open one to set its **Width**, **Thickness** and curbs. Each
lies on the terrain.

## The road (pages 421–425)

1. *Not in HouseCraft yet:* road defaults. A new road is 24' wide; open it and set **Width** `28'`.
2. **Road**: click the bottom-right corner of the terrain, then the lower-left corner. Press Enter.
3. *Not in HouseCraft yet:* curved roads (Change Line/Arc) and road stripes. For a gentle curve, draw the road with more
   corners. To snap the road onto the lot edge, use **Edit › Point to Point Move**.

## Road in 3D and its height (pages 425–427)

1. Chief only shows roads inside the terrain perimeter. HouseCraft draws them anyway, but pull the terrain's edge
   past the road so the ground meets it.
2. Copy the front elevation line (`-8'`) to the middle of the road. The slope down to the road gets gentler. The
   ground doesn't pass exactly through an elevation line where the data is steep; smoothing can leave it a foot or
   so off.

## Driveway (pages 427–428)

1. *Not in HouseCraft yet:* driveway defaults and flared ends.
2. **Driveway**: click at the middle of the Garage door, then straight down at the road. Press Enter. Open it:
   **Width** `13'`.
3. For blacktop, use **3D › Material Painter** on the driveway with an asphalt material.

## Sidewalks (pages 428–431)

1. *Not in HouseCraft yet:* Auto Generate Sidewalks along a road.
2. **Sidewalk**: click at the foot of the porch stairs, down a few feet, then across to the driveway. Press Enter.
   Open it: **Width** `5'`.
3. *Not in HouseCraft yet:* curved sidewalks. Add a few extra corners to round the bend.

## Annotations and revision (pages 431–432)

*Not in HouseCraft yet:* a street-name label on the road, and a hardscaping schedule. Use **Text** for the street
name. Then **File › Make a Copy…** `Chic Cottage-Hardscape`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Road defaults, curves, stripes, flares, curb cuts, auto sidewalks, labels, schedule | SITE-32 |
| Terrain passing exactly through elevation data | SITE-33 |
