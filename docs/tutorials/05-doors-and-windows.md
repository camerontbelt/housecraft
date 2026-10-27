# Tutorial 5, Doors and Windows: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Floor Plan Tutorials › Doors and Windows** (pages 82–104).
It carries on from [Tutorial 4](04-interior-stairs.md). The steps also run as the automatic test
[05-doors-and-windows.json](05-doors-and-windows.json).

## Names that differ

| Chief says | In HouseCraft |
|---|---|
| Build › Door › Hinged / Sliding / Pocket / Bifold / Garage Door, Doorway | **Door Tools** (left panel, or Build › Door) |
| Build › Window › Window | **Window Tools › Window** |
| Change Opening/Hinge Side, Change Swing Side buttons | In the door's specification: **Hinge** Left/Right and **Swing** In/Out |
| Lites Across / Lites Vertical | The **Grille** box in the window's specification (across × up) |
| Transform/Replicate Object | **Edit › Multiple Copy…** |
| Center Object | **Center Object** on the edit toolbar under the plan |
| Object Information › Comment | Double-click the object › **Object Information** |

## Placing doors (pages 84–87)

1. *Not in HouseCraft yet:* door styles from the library (P04), casing profiles and per-part materials in the
   door defaults.
2. **Hinged Door:** HouseCraft places a door with a **click** (Chief uses click and drag to choose the hinge and swing).
   Click the Foyer's front wall. It becomes an exterior door because it's in an outside wall. Set the hinge and
   swing afterwards in its specification.
3. Put hinged doors between the Bedroom and Bath, the Living room and Bedroom, and the Foyer and Garage.
4. **Sliding Door** in the Kitchen's back wall. **Doorway** between the Foyer and Dining room, and between the
   Kitchen and Dining room. **Bifold Door** into the Foyer closet, **Pocket Door** into the Bedroom closet.
5. **Garage Door** in the Garage's front wall. It's 9' wide by default (Chief's single-car size).

## Editing doors (pages 87–88)

1. Open the Living/Bedroom door: **Hinge** Right, **Swing** Out. Drag the handle near the end of the swing arc
   to show it partly open.
2. Bifold: set **Width** to `60"`. You can also drag its end handles.
3. Sliding door: tick **Grille** and set 3 across, 5 up.

## Library content (pages 88–91)

*Not in HouseCraft yet:* door hardware (handles, locks), door styles from the library, casing profiles per side,
and the interior door material defaults. Skip these steps.

## Windows (pages 91–92)

1. **Edit › Default Settings… › Windows**: type **Casement**, width `30"`, height `54"`.
   *Not in HouseCraft yet:* lites in the defaults; set the grille per window.
2. **Window** tool: click on the walls to place 11 windows (see the tutorial's picture).
3. Foyer window: **Fixed**, width `12"`, height `77"`, **Sill** `3"` (top at 80"), grille 1 × 5.
4. Kitchen window: height `36"` **and Sill `44"`**. *Not in HouseCraft yet:* changing the height keeping the top
   where it was, so set the sill as well.
5. Garage window: **Fixed**, height `48"`, sill `32"` (top at 80").

## Positioning (pages 92–96)

1. Select the front door, click **Center Object**, then click the Living/Foyer room divider. The door centres on it.
   Do the same for the Garage door and the windows listed in the tutorial.
2. Draw a dimension between the two Bedroom windows. Select one window, click the dimension's number, type `4 1/2"`
   and press Enter. The window slides along its wall.

## Replicating (pages 96–98)

1. Select the Foyer window, choose **Edit › Reflect About Object** (or the mirror button on the edit toolbar), then
   hold Ctrl and click the front door. Ctrl keeps the original, so a mirrored copy appears on the other side of the
   door's centre line.
2. **Kitchen window:** select it, **Edit › Multiple Copy…**, 2 copies, moved `-35 1/2"` along the wall. **Fixed
   for this tutorial:** windows and doors can now be copied along their wall this way. Before, they could only be
   copied together with the wall.
3. Same for the Living room (`-35 1/2"` across).
4. Make the middle Living window **Fixed** and hinge the left one on the right. *Not in HouseCraft yet:* Make
   Mulled Unit from separate windows.

## Schedules (pages 99–100)

1. **Tools › Schedules › Place in Plan › Door Schedule**, then **Window Schedule**. Double-click a schedule to
   choose its columns.
2. *Not in HouseCraft yet:* placing them in a CAD detail, and swapping the schedule-number callouts for the
   objects' own labels.

## Annotations (pages 100–103)

1. Open the Foyer/Garage door › **Object Information** › Comment: `20 minute` / `self-closing`. It shows in
   the door schedule's Comment column.
2. *Not in HouseCraft yet:* a leader line whose text picks up the comment of the door it points at. Type the
   text yourself with the **Text with Arrow** tool.
3. **Callout** by the sliding door: `TEMPERED GLASS`, tick **Numbered Note**. It gets the next number. Copy it
   beside the front door's side lite.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Door styles from the library, casing per side, door material defaults | DW-20 |
| Door hardware | DW-19 |
| Lites in the window defaults | DW-21 |
| Height change keeps the top | DW-22 |
| Make Mulled Unit from separate windows | DW-11 |
| Schedules in CAD details; labels instead of schedule numbers | EST-26, EST-27 |
| Text that shows the pointed-at object's comment | TXT-23 |

Fixed for this tutorial: the default garage door (16', too wide for the Chic Cottage's 13'4" garage, so it
couldn't be placed) is now 9' × 7'. Doors and windows can also be copied along their wall with Multiple Copy.
