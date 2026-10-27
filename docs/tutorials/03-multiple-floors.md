# Tutorial 3, Multiple Floors: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Floor Plan Tutorials › Multiple Floors** (pages 45–59).
It carries on from [Tutorial 2](02-interior-walls.md). The steps also run as the automatic test
[03-multiple-floors.json](03-multiple-floors.json), which starts from the end of Tutorial 2.

## Names that differ

| Chief says | In HouseCraft |
|---|---|
| Down / Up One Floor buttons, Change Floor/Reference | The floor buttons beside the plan view list on the top toolbar (**Build › Floor › Up One Floor**, Shift+N), or click a floor under **Project Browser › Floors** |
| Reference Display (F9) | **View › Reference Floor Display** (F9) |
| "8" Concrete Stem Wall" | The foundation's own wall type (made when you build the foundation) |
| Tools › Schedules › Wall Schedule | **Tools › Schedules › Place in Plan › Wall Schedule** |

## A second floor (pages 47–48)

1. **Build › Floor › Build New Floor** (Shift+X). *Not in HouseCraft yet:* "Derive new 2nd floor plan from the
   1st floor plan". The new floor is empty, so:
   1. Go down to Floor 1, box-select the exterior walls and press **Ctrl+C**.
   2. Go up to Floor 2 and choose **Edit › Paste Hold Position**. The walls land exactly above.
2. *Not in HouseCraft yet:* the new floor doesn't take the floor structure you set for Floor 1. Double-click
   **Floor 2** in the Project Browser and set **Floor Structure** to `12"`. The ceiling height (97 1/8") is
   copied from the floor below. Floor 2 then sits at 9'-1 1/8", as in Chief.
3. **Shingle-6:** **Build › Wall › Wall Types…**, copy **Exterior 2x6**, rename it `Shingle-6` and make layer 1
   `Shake - Natural`, 3/4".
4. Select the walls on Floor 2 (Shift+click each, or box-select), double-click one, and set **Wall Type** to Shingle-6.

## A basement foundation (pages 49–52)

1. **Build › Floor › Build Foundation…** (Ctrl+F). Choose **Basement**, set **Wall height** to `113 1/8"`, and
   leave **Rebuild automatically when the floor above changes** ticked.
2. *Not in HouseCraft yet:* the garage options (a separate, shallower garage foundation with a 12" curb). For now
   the basement walls run under the garage as well.
3. **Furred basement walls:** **Build › Wall › Wall Types…**, make `8" Concrete with Furring` with the layers:
   Concrete 8" (**Main**), Insulation Air Gap 1", Fir Framing 2 3 1/2", Drywall 1/2".
4. On the Foundation floor, select the basement's outside walls (not the garage's), double-click, and choose the new
   type. **This now sticks:** the foundation only rebuilds walls that no longer fit Floor 1's outline, so your wall
   types and any walls you add down there are kept.
5. *Not in HouseCraft yet:* **Center Footing on Main Layer**. The footing stays centred on the whole wall.

## Aligning walls between floors (pages 52–55)

1. On the Foundation floor, press **F9** for the Reference Display. Floor 1's walls show faintly.
2. Draw an interior wall over the Dining/Foyer wall above it. Snapping to the faint reference walls lines it up.
3. Press an arrow key to nudge it 1" at a time until it sits under the wall above. *Not in HouseCraft yet:* the
   **Align With Wall Above** button.
4. **Copy and Paste Hold Position:** select the wall, **Ctrl+C**, go up two floors to Floor 2, then
   **Edit › Paste Hold Position**. *Not in HouseCraft yet:* tiled plan views (Window › Tile Vertically).
   Switch floors in the one view instead.

## A wall legend (pages 55–58)

1. **Tools › Schedules › Place in Plan › Wall Schedule** puts a wall schedule in the middle of the view.
   Double-click it to choose columns (Type, Thickness, Qty…) and its title (`Wall Legend`).
2. Room dividers are left out of wall schedules automatically, so there's nothing to untick.
3. To keep it off the plan, make a CAD detail for legends: **CAD › CAD Details › New CAD Detail**, named
   `Legends`. With the detail open, **Tools › Schedules › Place in Plan › Wall Schedule** puts the schedule in the
   detail, where it lists every floor. Delete the one on the floor plan.
4. *Not in HouseCraft yet:* the "Wall Construction, Upper/Lower" columns, and renaming or dragging columns wider.

## Revisions (page 59)

**File › Make a Copy…** `Chic Cottage-Floors`. You stay in `Chic Cottage-Current`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Derive a new floor from the floor below | PRJ-44 |
| Floor/Ceiling Platform defaults used by new floors | RM-16 |
| Separate garage foundation with a curb | FD-07 |
| Center footing on main layer | FD-08 |
| Align With Wall Above/Below | WL-28 |
| Tiled plan views | UI-32 |
| Wall construction columns; rename/resize columns | EST-25, EST-10 |

Fixed for this tutorial: **the foundation kept rebuilding its walls**. It wiped the furred wall type, and any
wall drawn on the foundation floor, as soon as anything changed. Now it only replaces outside walls that no
longer match Floor 1.
