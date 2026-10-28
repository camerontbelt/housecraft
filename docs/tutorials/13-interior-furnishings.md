# Tutorial 13, Interior Furnishings: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Interior Design Tutorials › Interior Furnishings**
(pages 222–236). It carries on from [Tutorial 12](12-room-moldings.md). The steps also run as the automatic test
[13-interior-furnishings.json](13-interior-furnishings.json).

## New for this tutorial: furniture

HouseCraft now has a small built-in furniture set in the **Library** tab, under **Furniture**: Queen Bed, End
Table, Dresser, Sofa, Arm Chair, Coffee Table, Dining Table and Dining Chair. **Furniture/Accessories** has a Vase
and a Picture Frame. Click one in the library, then click in the plan. Beds, end tables, dressers, sofas, arm chairs
and frames turn their back to the nearest wall. Tables, chairs and vases stand where you click; open them to
turn them.

There's one style of each, not Chief's catalog of brands and styles. For a particular piece, import a 3D model
(**File › Import › 3D Symbol**) and use that.

## Material defaults (page 223)

**Tools › Plan Materials…** › **Default material for each surface** now lists **Furniture** (wood) and
**Furniture Upholstery** (fabric, Chenille - Pebble). Set **Accent Upholstery** too (Twill Cornflower): it goes on pillows and seat cushions.

## Placing furniture (pages 223–227)

1. Bedroom: **Queen Bed** under the windows, an **End Table** either side.
2. Foyer: a **Dresser** against the Dining room wall.
3. Living room: a **Sofa** against the back wall, an **Arm Chair** by the Bedroom wall, a **Coffee Table** in
   front of the sofa.
4. Dining room: a **Dining Table** and six **Dining Chairs**. Hold **Ctrl** while dragging to slide chairs under the
   table. To draw the table over the chairs, double-click it and click **Move Forward** on the **Draw Order** row.
   (**Move Backward** sends a piece under what it overlaps.)
5. A **Vase** on the dresser (it stands at dresser height, 34") and a **Picture Frame** on the wall above.

## Customising (pages 227–230)

1. Open the frame: set its width, height and height off the floor (**Elevation**; 44" puts the top of a 36" frame at
   80").
2. **Mirror:** **3D › Material Painter** with reach **This surface**, material **Mirror**. Click the picture in the frame.
3. *Not in HouseCraft yet:* Replace From Library with another style (there's one style of each). Change the size
   instead.

## Blocks, library folders, schedule (pages 230–236)

1. Click the dresser, Shift+click the vase and the frame, then **Edit › Make Architectural Block**. The three now
   select, move, copy and delete together. Open the block (double-click) to rename it `Dresser Set`.
2. To change one piece, select the block and choose **Edit › Explode Architectural Block**, edit the piece, then make
   the block again. *Not in HouseCraft yet:* editing a piece without exploding the block.
3. **Schedule › Furniture Schedule**, then click in the plan. Matching pieces share a line (the six dining chairs
   show as Qty 6), and accessories like the vase and frame are left out.
4. *Not in HouseCraft yet:* user library folders and links.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| More furniture styles; replace with another style | LIB-12, MAT-24 |
| Editing a piece inside an architectural block | CB-27 |
| Library folders | LIB-01 |

Added for this tutorial: **built-in furniture and accessories** (LIB-19), with Furniture and Upholstery material
defaults.
