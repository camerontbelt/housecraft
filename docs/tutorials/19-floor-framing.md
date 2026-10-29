# Tutorial 19, Floor Framing: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Framing Tutorials › Floor Framing** (pages 331–348). It
carries on from [Tutorial 18](18-electrical-objects.md). The steps also run as the automatic test
[19-floor-framing.json](19-floor-framing.json).

One difference to know first: Chief draws a floor's joists on the floor *below* it (Floor 1's joists show on
Floor 0). HouseCraft keeps them with the floor they hold up. Floor 1's joists are on **Floor 1**, and the
basement's lowered-ceiling joists are on the **Foundation** floor.

## Defaults (pages 331–335)

1. Framing types: **Edit › Default Settings…** › **Framing** › **Floor joists**: **I-Joist**. Members can be lumber,
   LVL, PSL, VSL, steel, I-joist or steel hat channel; change one in its own dialog. *Not in HouseCraft yet:* your
   own named framing types with custom section shapes.
2. Snaps: **Tools › Snap Settings**. Turn grid snaps off and object snaps on.
3. Pick **Framing Plan** from the plan view list on the top toolbar (added by **View › Add Standard Plan Views** in
   Tutorial 17).
4. **Edit › Default Settings…** › **Framing**: **Floor joist spacing** `16"`, **Lumber thickness** `1 1/2"`. To give one
   floor its own spacing or LVL rim joists, double-click the floor under **Floors** in the
   **Project Browser**, then set **Joist Spacing** and **Rim Joists** `LVL`.
5. **Build › Framing › Framing Reference Marker**: click the outside corner of the framing at the bottom left.
   The joists between the two end joists are then spaced from that point (16" steps) instead of from each edge.

## A working layer set (page 335)

**View › Layers and Layer Sets…** › **Layer Sets**: select the framing set and click **Copy**. Rename the copy
`Framing Set - Working`. Then turn on the ceiling joists layer in the **Layer Display** side window.

## Building the framing (pages 335–337)

1. **Build › Framing › Build All Framing**. Chief has Floors and Ceilings checkboxes; HouseCraft builds everything.
   For just floors and ceilings on one floor, use **Build Floor and Ceiling Framing (this floor)**. *Not in
   HouseCraft yet:* automatic rebuilding when the model changes. Choose Build again after a change.
2. **Joist Direction** (**Build › Framing**): on Floor 1, draw a horizontal line in the middle of the house. Choose
   **Build Floor and Ceiling Framing (this floor)**: the joists turn to run left–right.
3. Select the line, open it and set its angle to 90°. Build again: the joists run up–down.
4. Delete the line and build again: they go back.

## Bearing wall (pages 337–338)

1. Go to the **Foundation** floor. Open the wall left of the stair and tick **Bearing Wall**. It gets a footing.
2. Go back to Floor 1 and **Build Floor and Ceiling Framing (this floor)**.
3. Without a Joist Direction line, the joists run whichever way gives the shorter spans between the outside walls
   and any bearing walls, bearing lines or beams that cross them. Where they cross a bearing wall they splice over
   it (by the **Joist splice** setting in the framing defaults).

## Posts and beams (pages 338–342)

1. **Edit › Default Settings…** › **Framing**: **Post size** `3 1/2"`, **Beam thickness** `3 1/2"`, **Beam depth**
   `12"`, and tick **Square footing under posts**.
2. On the **Foundation** floor: **Post** at the top end of the stair wall, and another at the top-left corner of
   the Garage area. Each comes in at 3 1/2" square, standing on a concrete footing.
3. **Beam** from the first post up to the back wall (it comes in 12" × 3 1/2"). Set its top level with the joists.
4. A second **Beam** from the Garage post to the back wall, set 11 7/8" lower.
5. Build the floor framing again. The first beam's top is level with the joists (a flush beam), so the joists stop
   at its faces. The second sits below them (a dropped beam), so they bear on it and splice over it like a bearing
   wall.

## Editing joists (pages 342–344)

1. To trim a joist to another member: click the joist, Shift+click the member to trim it to, then choose
   **Edit › Trim / Extend to Second Object**. The end nearer the member moves onto it (shorter or longer), and the
   joist is kept when framing is rebuilt. Do the joists one at a time.
2. To split a joist, beam or rafter, use the **Break Wall** tool (Add Break) and click on the member.
3. *Not in HouseCraft yet:* bumping and pushing joists, and Sticky Mode for trimming many at once.

## Annotations (pages 344–346)

1. Make a text style `Framing Notes` with **Uppercase** ticked. *Not in HouseCraft yet:* underlining part of a line.
2. **CAD › Text** on the Foundation floor, near the front left corner. Type `ALL FLOOR JOISTS ARE U.N.O.:`, press
   Enter, then `2x12s SPF #2 OR BETTER @16" O.C.`. Centre it, give it the Framing Notes style, tick **Border** in its
   dialog and set **Border Margin** `6"`.
3. Copy it beside the deck joists. Change it to `DECK` and `2x10s`.

## Revision (page 346)

**File › Make a Copy…** `Chic Cottage-Floor Framing`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Automatic framing rebuild | FR-35 |
| Sticky Mode for trimming many joists | FR-36 |
| Named framing types with custom section shapes | FR-07 |
| Bumping and pushing | CAD-28 |
| Underlining part of a line | TXT-24 |
