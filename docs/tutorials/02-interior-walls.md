# Tutorial 2, Interior Walls: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Floor Plan Tutorials › Interior Walls** (pages 28–44).
It picks up from [Tutorial 1](01-exterior-walls.md). The steps also run as the automatic test
[02-interior-walls.json](02-interior-walls.json).

## Names that differ

| Chief says | In HouseCraft |
|---|---|
| "Interior-4" wall | **Interior 2x4** |
| "Fire-6" wall | Not built in: you make it (below) |
| Open Object | Double-click the object |
| Dashboard, pinned documents | **File › Recent Files** (with Pin / Unpin) |

## One habit to learn first

Chief splits a wall automatically where another wall crosses it. **HouseCraft doesn't yet**, so
**click at every wall you cross** while drawing. Each click starts a new piece of wall, and later steps
(making one piece a room divider, removing one piece to merge rooms) need those pieces.

## Opening the plan and defaults (pages 28–30)

1. **File › Recent Files** and pick `Chic Cottage-Current` (or **File › Open Plan…**).
2. **Edit › Default Settings… › Walls**: the interior wall type is **Interior 2x4** (2x4 studs, drywall both sides).
   Leave it.

## Drawing interior walls (pages 30–31)

1. Click **Straight Interior Wall** in the Wall Tools list.
2. Click the middle of the lower-left front wall (a snap marker shows at the midpoint), then click at the
   middle of where you'll cross later, then the back wall. Press **Esc**.
3. Draw a second vertical wall to the right of the first, front to back, again clicking where walls will cross.
4. Draw a wall across the middle from the left wall to the right wall, **clicking at each vertical wall
   you cross**. You now have six rooms.

## Room types (pages 32–33)

1. Click inside a room to select it, then double-click it to open its specification.
2. Set **Room Type**: Kitchen (back left), Living (back middle), Bedroom (back right), Dining (front left),
   Entry (front middle), Garage (front right).
3. **Cross section:** **3D › Elevations and Sections › Cross Section / Elevation Tool**; click inside the Entry,
   then click a little further in the direction to look.
4. **Perspective Floor Overview:** *not in HouseCraft yet.* Use the 3D overview (the house button on the toolbar).

## Subdividing rooms (pages 34–36)

1. Draw a wall across the Bedroom from right to left. **The new room is a Bedroom too**: as in Chief, a room you
   split off keeps the settings of the room it came from.
2. Draw a vertical wall dividing the new room in two, and a wall straight up from the Garage's left corner to the
   middle wall.
3. Name the rooms: Bath and Closet behind the Garage, Closet for the narrow room beside the Garage.
4. Rename the Entry: open it and type `Foyer` in **Name**.
5. To hide one room's label, open the room and untick **Room Label › Show the label in plan**; tick it again to
   bring it back. To turn the closet's label, type `90` in **Label Rotation**.
6. To hide the size line under the labels: make a layer called `Rooms, Size` (**Layer Display Options › New Layer**),
   then in **Edit › Default Settings… › Rooms** set **Size line layer** to it. Now untick that layer in
   **Layer Display Options** and the size lines go; the room names stay.

## Merging rooms (page 36)

1. Draw a wall across the closet beside the Foyer, about 6' from the front: two closets.
2. Chief breaks the closet's wall and drags its end. In HouseCraft, use **Break Wall** (Wall Tools) and click the
   closet's wall where the new wall meets it. (If you clicked at each junction when drawing, it's already in
   pieces.) Then click the piece between the upper closet and the Foyer and press **Delete**. The upper closet
   joins the Foyer, and the Foyer (the larger room) keeps its name and type.

## A thicker wall type (pages 37–38)

1. **Build › Wall › Wall Types…**, select **Exterior 2x6**, **Copy**, **Rename / Edit** to `Fire-6`, and set the
   layers to: Type X gypsum 5/8", 2x6 stud 5 1/2" (**Main**), Gypsum board 1/2".
2. Select the wall between the Garage and the Bath, Shift+click the wall between the Garage and the closet, open
   them and choose **Fire-6**.
3. Open the Garage/Bath wall and click **Reverse Layers** so the fire-rated side faces the Garage. Doors and
   windows in the wall stay where they are.

## Room dividers (pages 38–39)

1. Click the piece of wall between the Living room and the Foyer and press **Delete**.
2. Click **Room Divider** in the Wall Tools list (new) and click from one end of the gap to the other.
   A dashed line appears: the two rooms stay separate, but nothing is built in 3D.
3. To turn an existing wall into a divider (the Kitchen/Living wall): open it and, next to **Invisible**, tick **Room
   divider**. Room dividers are never picked up by the automatic exterior dimensions, so there's no
   "No Locate" box to tick.

## Positioning interior walls (pages 39–42)

1. Open the Bath and click **Auto Room Dimension**: each wall gets a dimension along the inside of the room.
   *Not in HouseCraft yet:* the **Interior Dimension** tool. For a single inside measurement, use the **Manual
   Dimension** tool (Dimension Tools) between the inside faces of the walls.
2. Move walls exactly as in Tutorial 1: select the wall, click the number on a dimension beside it, type the
   distance and press Enter.
3. **Edit › Delete Dimensions › Manual, This Floor** takes away the dimensions you drew (or the automatic ones, or
   all). To delete every object of a kind, select one of them and use **Edit › Delete Objects of the Selected
   Kinds**: on this floor, on all floors, or only inside a room you also selected.
4. Collinear walls: drag the wall's middle until it lines up (the snap shows when the faces meet). *Not in
   HouseCraft yet:* the Edit Wall Layer Intersection handles.

## Annotations and revisions (pages 43–44)

1. **Text** tool, near the top left of the Dining room: `2X4 INTERIOR WALLS 16" OC U.N.O.`
2. **File › Make a Copy…** `Chic Cottage-Rooms`. You stay in `Chic Cottage-Current`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Walls split where another wall crosses them | WL-27 |
| Interior Dimension tool | DIM-01 |
| Wall layer intersection handles | WL-26 |
| Floor overview | VIS-30 |
| Dashboard | UI-30 |

Added since: the **Break Wall** tool (Wall Tools). Added for this tutorial: **room dividers** (Room Divider tool and the wall's Invisible box), and **rooms that
inherit** settings when split, keeping the larger room's when merged.
