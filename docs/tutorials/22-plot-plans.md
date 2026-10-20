# Tutorial 22, Plot Plans: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Landscaping Tutorials › Plot Plans** (pages 382–402). It
carries on from [Tutorial 21](21-roof-and-ceiling-framing.md). The steps also run as the automatic test
[22-plot-plans.json](22-plot-plans.json).

Chief draws the plot plan in its own file and pastes it into the house plan. You can do the same in HouseCraft
(**File › New**, then copy and paste). Or draw it straight in CHIC COTTAGE-CURRENT, which is what the steps below
do. *Not in HouseCraft yet:* a separate north for the site plan.

## Plan view and CAD defaults (pages 384–385)

1. Pick **Site Plan** from the plan view list on the top toolbar.
2. *Not in HouseCraft yet:* lengths in decimal feet and angles as quadrant bearings on CAD lines. The lot labels
   below show the bearings you type, with lengths in feet and inches.

## Drawing the lot (pages 385–390)

HouseCraft draws a whole lot from its description in one dialog.

1. **Terrain › Plot Plan from Bearings…**. Type one side per line, bearing then distance:

   ```
   N 61 25 10 E, 155.69'
   S 28 29 35 E, 118.65'
   S 49 59 11 W, 154.37'
   N 30 11 36 W, 149.31'
   ```

   The third side is typed reversed (S…W), as in the tutorial. The lot starts at the plan's origin (Chief's Input
   Point at 0,0).
2. *Not in HouseCraft yet:* curved lot sides. The fourth side is a 450' radius arc, 150' long. Type its chord
   instead: `N 30 11 36 W, 149.31'`. Then the lot closes.
3. **Building setback** `10'` draws the setback line inside the lot. Click **Draw Lot**. Each side is labelled with
   its bearing and length.

## Setback line (pages 392–393)

*Not in HouseCraft yet:* the Concentric edit behaviour. The setback came from the dialog. **Offset…** on the edit
toolbar makes another one if you need it.

1. Open the setback line and pick a dashed line style.
2. **View › Layers and Layer Sets…**: add a layer `CAD, Setback Lines`. Open the setback line again and put it on that
   layer.

## Turning the lot into terrain (pages 393–396)

1. Select the lot outline (not the setback).
2. **CAD › Convert CAD To › Terrain Perimeter**. It replaces the terrain perimeter from Tutorial 6 and keeps its
   settings, so there's nothing to delete first.

## North and rotating the lot (pages 397–399)

1. **3D › Sun Angle and North…**: **North points** `0` (straight up).
2. Select the terrain and the setback line. Edit toolbar **Move / Copy / Rotate / Reflect…** › **Rotate**: angle
   `1.506944` (1° 30' 25"), around the lot's start corner.
3. Set **North points** to `358.493` so north turns with the lot. *Not in HouseCraft yet:* a north pointer you select
   and rotate along with the lot.

## Positioning the house on the lot (pages 399–401)

Measure from the back right corner of the roof to the back and right setback lines with dimensions. Then move the
terrain and setback together with **Move**, typing the X and Y distances that make those 50' and 30'. *Not in
HouseCraft yet:* moving a selection by typing on a dimension, point markers, and Point to Point Move.

## Revision (page 401)

**File › Make a Copy…** `Chic Cottage-Plot Plan`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Separate site north; north pointer that rotates with the lot | SITE-28 |
| Decimal feet and bearings on CAD lines; curved lot sides | SITE-29 |
| Concentric edit behaviour | CAD-32 |
| Moving a selection by dimension; Point to Point Move | DIM-03, CAD-31 |

**New while following this tutorial:** **CAD › Convert CAD To › Terrain Perimeter** turns a closed CAD outline into the
terrain's perimeter.
