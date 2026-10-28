# Tutorial 23, Terrain Elevation: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Landscaping Tutorials › Terrain Elevation** (pages 403–419).
It carries on from [Tutorial 22](22-plot-plans.md). The steps also run as the automatic test
[23-terrain-elevation.json](23-terrain-elevation.json).

Elevation Line, Retaining Wall and the other terrain tools are on the **Terrain** menu (and in the Terrain tool list on
the left). An elevation line takes several clicks: click along it, then double-click or press Enter to finish.

## Defaults and the landscaping view (pages 404–406)

1. *Not in HouseCraft yet:* extra allowed angles for angle snaps (such as the lot's S 49 59 11 W side).
2. **View › Layers and Layer Sets…** › **Layer Sets**: copy the site plan's set and call it
   `Landscaping Layer Set`. Turn contours on and plot plan dimensions off in the **Layer Display** side window.
   *Not in HouseCraft yet:* Duplicate for a saved plan view.

## Building pad (pages 406–407)

**Terrain › Terrain Specification**: **Base Elevation** `-28"`. That's the ground height wherever there's no
elevation data, about 1" below the Garage slab. In the terrain's dialog tick **Reference Point** and set **Reference At** to the centre of the Garage door; elevation data drawn right beside it take over there.

## A slope (pages 407–410)

1. **Elevation Line**: click left of the terrain, right of it, then press Enter. A line right across the lot. Open it
   and check **Elevation** is `0"`. Move it to 1" in front of the Garage's front wall.
2. Copy it (Copy and Paste, or **Multiple Copy…** with a Y distance) down near the front of the lot, between the
   setback and the edge. Open the copy: **Elevation** `-8'`. The terrain now slopes down to the front. Watch it in the 3D
   view.

## Shaping the slope (pages 410–413)

1. Copy the first line to the back of the Deck, at `0"`. The ground is flat round the house.
2. Copy another near the back of the lot at `6'`. Move it to about 2' inside the back setback line. Add one more 3' in
   front of it at `5'`. The slope is gentle behind the Deck and steeper at the back.
3. Select the back two lines and rotate them 15° clockwise (rotate handle, or **Move / Copy / Rotate / Reflect…** ›
   **Rotate** `-15`). To give the top line its own angle with its start point locked, select it, choose
   **Edit › Edge Angle…**, click the line, type `-13` and click **OK**: the left end stays put and the right end
   swings round. To set a line parallel to the lot edge, select it, choose **Edit › Make Parallel /
   Perpendicular**, click the lot edge, then click the line. *Not in HouseCraft yet:* curved elevation lines.

## Elevation polyline (pages 413–415)

1. Turn off the contours layer for a clearer view.
2. **Elevation Line**: click on the right side of the lot, click in the middle, then click up and to the left through
   the edge of the terrain (165°). Press Enter. That's one polyline of two segments. Open it and set **Elevation** `0"`.
3. Add a break to the Deck line and drag the new corner down: select the line, choose **Edit › Add Break**, click on
   the line where the corner goes, press Esc, then drag the new corner's handle down to the middle of the house.
4. To carry on drawing an elevation line from one of its ends, select it, choose **Edit › Continue Drawing**, and
   click the new corners starting near the end you want to extend from. Press Esc when done.

## Retaining wall (pages 415–417)

1. **Retaining Wall**: click on the right setback line below the lowest angled elevation line, down the setback, then
   up and to the left at 15° to the back setback line. Press Enter. Open it and set its **Top Elevation**.
2. *Not in HouseCraft yet:* the terrain either side of a retaining wall being separate. HouseCraft holds the ground at
   the wall's top along it. It also has no temporary dimensions for placing elevation lines 8" from the wall.

## Revision (page 417)

**File › Make a Copy…** `Chic Cottage-Terrain`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Additional allowed angles | CAD-33 |
| Duplicate a saved plan view | PRJ-12 |
| Curved elevation lines | SITE-29 |
| Retaining walls separating the terrain | SITE-31 |
