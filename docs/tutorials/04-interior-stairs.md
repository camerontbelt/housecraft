# Tutorial 4, Interior Stairs: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Floor Plan Tutorials › Interior Stairs** (pages 60–81).
It carries on from [Tutorial 3](03-multiple-floors.md). The steps also run as the automatic test
[04-interior-stairs.json](04-interior-stairs.json).

## Names that differ

| Chief says | In HouseCraft |
|---|---|
| Build › Stairs › Draw Stairs / Straight Stairs | **Stair Tools › Straight Stair** (Build › Stairs) |
| Staircase Specification | Double-click the stair |
| Auto Stairwell button | The **Auto Stairwell** button in the stair's dialog |
| Back Clipped Cross Section | **3D › Elevations and Sections › Cross Section / Elevation Tool** |
| Note (CAD › Text › Note) | The **Callout** tool, then tick **Numbered Note** |
| Tools › Schedules › Note Schedule | **Tools › Schedules › Place in Plan › Note Schedule** |

## Stair defaults (pages 62–64)

1. **Edit › Default Settings… › Stairs**: set the width to `44"`.
2. In the same **Stairs** defaults pick **Newel posts**: **Square** (also Capped or Ball top) and the **Newel size**,
   and choose materials for the **Treads**, **Stringers** and **Railing**. New stairs get them. A stair can also
   take its own baluster and newel shapes (3D symbols) in its specification. *Not in HouseCraft yet:* a library of
   named newel and baluster styles.

## Stairs (pages 64–66)

1. **Garage steps:** *skip this one.* In HouseCraft the Garage floor isn't lower than the Foyer yet (no separate
   garage slab), so there's nothing to step up.
2. Go to Floor 1. Choose **Straight Stair**, click in the Foyer just right of the Dining wall (the bottom of the
   stair), then click further up the screen (the direction it climbs). HouseCraft works out the risers to reach
   Floor 2 with no riser taller than the maximum.
3. **Position:** draw a **Manual Dimension** from the inside of the front wall's framing to the bottom riser. Select
   the stair, click the dimension's number, type `4'6"` and press Enter. The stair moves; the wall stays.
4. Open the stair and set **Tread depth** to `10 1/2"`. It still reaches Floor 2. *Not in HouseCraft yet:* the
   Lock Bottom / Lock Top / Lock Number of Treads options and the Make Best Fit button. HouseCraft stairs always
   start where you put them and fit the floor above.

## Cross section (pages 66–68)

1. **3D › Elevations and Sections › Cross Section / Elevation Tool**: click beside the stair, then click across it.
2. Rename it: in the Project Browser under **Elevations and Sections**, open it and set the name to `Stair Section`.
3. **Window › Tile Vertically With › Stair Section** shows it beside the plan.

## Stairwell (pages 68–69)

1. The stair's **Stairwell** box is ticked by default, so the floor above is already cut open over the stair.
   Look at Floor 2 in 3D or in the section to see the opening.
2. Double-click the stair and click **Auto Stairwell**. On Floor 2, railings now run along both sides and the
   bottom end of the opening. The top end stays open, because that's where you step off the stair.
3. *Not in HouseCraft yet:* an Open Below room inside the railings, with its own label.

## Headroom (pages 69–71)

Draw a dimension in the section view from the top of a tread to the ceiling. *Not in HouseCraft yet:* moving
the point markers at the dimension's ends.

## Stacked stairs (pages 71–75)

1. Select the stair, **Ctrl+C**, go down to the Foundation, and **Edit › Paste Hold Position**. The copy climbs
   from the basement to Floor 1, with its own stairwell cut in Floor 1.
2. **Tape Measure** (Dimension tools) the clearance at the bottom of the basement stair: two clicks, read it in the
   status bar.
3. Hide stairs while you work round them: **Layer Display** tab (right side), untick **Stairs**.
4. To turn a railing into a wall, double-click it and change its **Wall Type** to an interior wall type.
   *Not in HouseCraft yet:* the Same Wall Type handle, so do each railing in turn.

## Clipping the section (pages 75–77)

Open **Stair Section** and tick **Clip sides**. *Not in HouseCraft yet:* top and bottom clip elevations and clip
lines you can drag.

## Plan notes (pages 77–80)

1. **Tools › Schedules › Place in Plan › Note Schedule**.
2. **Callout** tool (Text Tools): click near the bottom of the stairs, type `HEADROOM 6' 8" MIN.`, then open it and
   tick **Numbered Note**. It shows a number, and the Note Schedule lists the text.
3. Copy it to the Foundation with **Ctrl+C**, then **Paste Hold Position**.
4. A schedule can go in a CAD detail: open the detail, then **Tools › Schedules › Place in Plan**. *Not in
   HouseCraft yet:* note types, the capsule shape, and lining schedules up with Align/Distribute.

## Revisions (page 80)

**File › Make a Copy…** `Chic Cottage-Stairs`. You stay in `Chic Cottage-Current`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| A library of named newel and baluster styles | ST-12 |
| Lower garage floor (garage steps) | FD-07 |
| Lock top/bottom/treads; Make Best Fit | ST-19 |
| Open Below room; Same Wall Type handle | ST-18 |
| Movable dimension point markers in sections | SEC-06 |
| Top/bottom clip planes in sections | SEC-01 |
| Note types, capsule markers | TXT-16 |
