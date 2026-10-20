# Tutorial 19, Floor Framing: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Framing Tutorials › Floor Framing** (pages 331–348). It
carries on from [Tutorial 18](18-electrical-objects.md). The steps also run as the automatic test
[19-floor-framing.json](19-floor-framing.json).

One difference to know first: Chief draws a floor's joists on the floor *below* it (Floor 1's joists show on
Floor 0). HouseCraft keeps them with the floor they hold up. Floor 1's joists are on **Floor 1**, and the
basement's lowered-ceiling joists are on the **Foundation** floor.

## Defaults (pages 331–335)

1. *Not in HouseCraft yet:* editable Framing Types (I-Joist, Lumber, LVL). Each member is lumber, LVL, PSL, VSL or
   steel; pick it in the member's dialog.
2. Snaps: **Tools › Snap Settings**. Turn grid snaps off and object snaps on.
3. Pick **Framing Plan** from the plan view list on the top toolbar (added by **View › Add Standard Plan Views** in
   Tutorial 17).
4. **Edit › Default Settings…** › **Framing**: **Floor joist spacing** `16"`, **Lumber thickness** `1 1/2"`. *Not in HouseCraft
   yet:* a different spacing per floor, and LVL rim joists.
5. *Not in HouseCraft yet:* the Framing Reference Marker. Joists are laid out from the edge of the building.

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
3. *Not in HouseCraft yet:* joists turning to span onto the bearing wall. HouseCraft runs joists the short way
   across the whole building and only splits them where they cross a bearing wall. Here the wall runs the same way
   as the joists. To get Chief's result, draw a horizontal **Joist Direction** line.

## Posts and beams (pages 338–342)

1. *Not in HouseCraft yet:* Manual Framing defaults and Post with Footing. Type each member's size instead.
2. On the **Foundation** floor: **Post** at the top end of the stair wall, and another at the top-left corner of
   the Garage area. Open each one: `3 1/2"` × `3 1/2"`.
3. **Beam** from the first post up to the back wall. Open it: **Depth** `12"`, **Width** `3 1/2"`, its top level
   with the joists.
4. A second **Beam** from the Garage post to the back wall, set 11 7/8" lower.
5. *Not in HouseCraft yet:* joists butting into the first beam and sitting on the second one.

## Editing joists (pages 342–344)

*Not in HouseCraft yet:* bumping and pushing joists, and trimming joists to another member. Select a joist and drag
its ends, or open it and type its end points.

## Annotations (pages 344–346)

1. *Not in HouseCraft yet:* uppercase text defaults, border margins, and underlining part of a line.
2. **CAD › Text** on the Foundation floor, near the front left corner. Type `ALL FLOOR JOISTS ARE U.N.O.:`, press
   Enter, then `2x12s SPF #2 OR BETTER @16" O.C.`. Centre it, and tick **Border** in its dialog.
3. Copy it beside the deck joists. Change it to `DECK` and `2x10s`.

## Revision (page 346)

**File › Make a Copy…** `Chic Cottage-Floor Framing`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Joists that span onto bearing walls and beams; Framing Reference Marker | FR-33 |
| Per-floor joist spacing, LVL rims, post/beam defaults, Post with Footing | FR-34 |
| Automatic framing rebuild | FR-35 |
| Trim joists to a member | FR-36 |
| Editable framing types | FR-07 |
| Bumping and pushing | CAD-28 |
| Text uppercase, margins, partial underline | TXT-24 |
