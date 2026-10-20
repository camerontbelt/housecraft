# Tutorial 1, Exterior Walls: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Floor Plan Tutorials › Exterior Walls** (pages 3–27).
It goes through the tutorial section by section and says what to click in HouseCraft where it differs from Chief.
Where HouseCraft can't do something yet, it says so and gives the nearest way round.

The same steps run as an automatic test ([01-exterior-walls.json](01-exterior-walls.json)), so if this page and
HouseCraft ever disagree, the test catches it.

## Names that differ

| Chief says | In HouseCraft |
|---|---|
| Residential Template, "Siding-6" wall | File › New Plan; its exterior wall type is **Exterior 2x6** |
| Project, Project Browser folders | A project is a folder of plans. The Project Browser's **Projects** list shows the open plan's folder, its plans and your sub-folders; below it are the open plan's floors, views and cameras |
| Open Object (Ctrl+E) | Double-click the object, or select it and click **Open Object** under Details on the right |
| Default Settings (Edit menu) | **Edit › Default Settings…** (the same idea; fewer categories) |
| Snap Settings (Edit menu) | **Tools › Snap Settings** |
| Define Wall Types | **Build › Wall › Wall Types…** or **Tools › Wall Types…** |
| Make a Copy | **File › Make a Copy…** |

## File management, saving, plan views (pages 3–6)

1. **File › New Plan.** Chief's New Project makes a project folder; in HouseCraft, save the plan (Ctrl+S) into a new
   folder called `Chic Cottage` as `Chic Cottage-Current`. That folder is the project: it shows under **Projects** at
   the top of the Project Browser, with every plan you save or copy into it.
2. The Project Browser is the right-hand panel's **Project Browser** tab. Under **Plan Views**, right-click
   **Floor Plan** to see what a saved view keeps (floor, layer set, default set, reference floors).
3. The tutorial turns off auto roofs and auto dimensions for clearer pictures. In HouseCraft both start off anyway:
   roofs appear when you choose **Build › Roof › Build Roof**, and dimensions with **CAD › Dimensions › Auto Exterior
   Dimensions** (Shift+A).

## Drawing walls (pages 7–8)

1. Grid snaps: **Tools › Snap Settings** (they're on by default).
2. Click **Straight Exterior Wall** in the Wall Tools list on the left (or Build › Wall).
3. HouseCraft draws walls by **clicking**, not dragging: click where the wall starts, then click where it ends.
   You can type a length and press Enter instead of the second click. Press **Esc** (or double-click) to stop.
4. Draw the first wall right to left, about 13'6" long. The length shows beside the wall while you draw.
5. Zooming: scroll the mouse wheel; drag with the middle button to pan. **F6** (Window › Fill Window) fits the plan.

## Wall types (pages 8–11)

1. Select the wall and open it (double-click). The **Wall Type** list and the layer table show its layers.
2. **Build › Wall › Wall Types…**, pick **Exterior 2x6**, click **Copy**, then **Rename / Edit** and call it `Stone-6`.
3. Change the layers to: Stacked stone 3", Thinset mortar 3/8", Metal lath 1/16", Rigid insulation 1",
   OSB 7/16", 2x6 stud 5 1/2" (tick **Main**), Gypsum board 1/2".
4. *Not in HouseCraft yet:* the "Foundation to Exterior of Layer" and "Dimension to Exterior of Layer" settings.
   HouseCraft always uses the layer marked **Main** (or else the thickest layer) for both, which gives the same
   result here.

## Setting the defaults (pages 11–14)

1. **Edit › Default Settings…**, open **Walls**, set **Exterior wall type** to `Stone-6`.
2. Draw the second wall: start on the left end of the first wall and go up about 10'. It comes out as Stone-6.
   Open the first wall and change its type to Stone-6 too.
3. **Pony wall defaults:** *not in HouseCraft yet* (no Straight Pony Wall tool). Instead, draw ordinary exterior
   walls (next section) and turn each into a pony wall afterwards: open the wall, tick **Pony Wall › Lower part of
   another type**, choose `Stone-6` and set **Split at** to `20"`.
4. **Floor structure and ceiling height:** in the Project Browser, under **Floors**, double-click **Floor 1**. Set
   **Ceiling Height** to `97 1/8"` and **Floor Structure** to `12"` (or list the layers: 3/4" OSB on 11 1/4" joists).

## Drawing the exterior walls (page 14)

1. With **Straight Exterior Wall**, start at the top of the 10' wall and click round the building **clockwise**:
   about 28' left, about 38' up, about 42' right, then down to where you began. Clockwise keeps the siding on the
   outside: HouseCraft puts a wall's outside on the left of the direction you draw it.
2. Turn those four walls into pony walls (Stone-6 below, split at 20"), as described above.

## Room definition and 3D views (pages 15–17)

1. When the walls close, the room appears with its area. Click inside it, then open it: its ceiling height follows
   Floor 1 (97 1/8").
2. **Camera:** click the camera button on the top toolbar (or press **Shift+J**). Click **where the camera stands**
   inside the building, then click **where it looks**. The 3D view opens and the camera is saved under
   Project Browser › Cameras. (In Chief you drag; in HouseCraft it's two clicks.)
3. Close the 3D view with the × on its tab.
4. **Back-clipped cross section:** **3D › Elevations and Sections › Cross Section / Elevation Tool**. Click inside the
   bump-out, then click in the direction to look.
5. **Tape Measure:** *not in HouseCraft yet.* For now, use the **Manual Dimension** tool (Dimension Tools) and
   delete the dimension afterwards.

## Dimensions and moving walls (pages 18–23)

1. **CAD › Dimensions › Auto Exterior Dimensions** (Shift+A). As in Chief, they measure to the **outside of the framing** (the Main layer).
   To measure to the outside surfaces instead, use **CAD › Dimensions › Auto Dimension Tiers…** and choose "The
   outside surfaces".
2. **Moving a wall with a dimension** works as in Chief:
   1. Click a wall to select it.
   2. Click the number on a dimension that measures from that wall to another wall.
   3. A box appears with the current value. Type the new distance (`28'`, `38'4"`, `28'8"`; plain numbers are
      inches) and press **Enter**. The wall moves and the walls joined to it stretch to follow.
3. Work round the building clockwise, as the tutorial says. The final sizes should read **42'** across the top and
   bottom, **28'** on the left, **38'4"** on the right, and **28'8"** and **13'4"** along the bottom with the
   **10'4"** bump-out.
4. **Resizing a wall:** select it and click the length shown along the wall (the temporary dimension), then type
   the new length. *Not in HouseCraft yet:* the Move Left End / Right End / Both Ends buttons; the start of the
   wall stays put.
5. Moving with the mouse: drag the selected wall. Or nudge it with the arrow keys: 1" a press, 1' with Shift.

## Annotations and revisions (pages 24–26)

1. **Rich text and Uppercase:** *not in HouseCraft yet.* Use the **Text** tool and type the note in capitals:
   `2X6 EXTERIOR WALLS 24" OC U.N.O.`
2. **Spell check:** *not in HouseCraft yet.*
3. **File › Make a Copy…**: name the copy `Chic Cottage-Shell`. It's saved beside your plan, and you stay in
   `Chic Cottage-Current` to carry on.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Pony wall tool and defaults | WL-03 |
| Tape Measure | DIM-24 |
| Rich text with uppercase | TXT-02 |
| Spell check | TXT-17 |
| Move Left End / Right End / Both Ends when resizing by dimension | DIM-03 |
| "Foundation/Dimension to Exterior of Layer" wall type settings | FD-05 |

Added while writing this guide: moving a wall by typing on a dimension, automatic dimensions that measure to the
framing, and drawing a Full Camera in the plan.
