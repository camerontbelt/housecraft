# Tutorial 6, Decks and Porches: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Floor Plan Tutorials › Decks and Porches** (pages 105–126).
It carries on from [Tutorial 5](05-doors-and-windows.md). The steps also run as the automatic test
[06-decks-and-porches.json](06-decks-and-porches.json).

## The big difference: decks

In Chief, a deck is a **room** you enclose with **Deck Railings**, and the program frames it. In HouseCraft a deck is
**one object** drawn with the **Deck** tool (**Terrain › Site › Deck**). Click its corners, starting against the
house so it gets a ledger. It frames itself (planks, joists, beams, posts, footings) and has a railing round the
free edges. The result looks the same; you just don't draw railings first.

## Names that differ

| Chief says | In HouseCraft |
|---|---|
| Straight Half Wall | **Straight Exterior Wall**, then set the wall's **Height** to 36" |
| Straight Deck Railing (deck room) | **Terrain › Site › Deck** |
| Terrain › Create Terrain Perimeter | **Terrain › Terrain Perimeter** tool (click the corners) |
| Terrain Specification › Surface at Reference Point | **Terrain › Terrain Specification** › base elevation |

## Railing defaults (pages 107–109)

*Not in HouseCraft yet:* separate Deck Railing, Railing and Half Wall defaults, and a Half Wall tool. You set
these on each wall or deck instead.

## Porch (pages 109–110)

1. On Floor 1, choose **Straight Exterior Wall** and draw two walls round the area outside the front door: from the
   front wall, in line with the Dining/Foyer wall, out 8', then across to the Garage wall. Press **Esc**.
2. Select both, open them, and set **Height** to `36"`. (For the stone base, tick **Pony Wall** with Stone-6
   at 20".)
3. Click inside the new room, open it, and set **Room Type** to **Porch**.
4. **Fixed for this tutorial:** a Porch (or Deck) room is now left out of the foundation. Before, the basement
   grew to go under the porch, and the basement walls you'd furred in Tutorial 3 lost their type.

## Deck (pages 110–112)

1. **Terrain › Site › Deck**: click the four corners: along the back wall 36" in from each side, then 8' out.
   Press **Enter**.
2. Open the deck: **Plank thickness** `1"`, **Plank gap** `1/4"`. It has a railing on its open sides.
3. *Not in HouseCraft yet:* the deck as a room enclosed by deck railings, deck post footing depth, and the
   footings' own layer.

## Dimensioning exterior rooms (pages 112–115)

*Not in HouseCraft yet:* moving the automatic dimensions out of the deck (they're rebuilt automatically), and the
extension-line / add-segment handles. Draw manual dimensions to the deck and porch instead.

## Terrain (pages 115–118)

1. **Terrain › Terrain Perimeter**: click four corners round the house, about 60' deep. Double-click to finish.
2. **Terrain › Terrain Specification**: set the base elevation to `-28"`. *Not in HouseCraft yet:* the terrain
   reference point. The base elevation applies wherever there's no elevation data.
3. *Not in HouseCraft yet:* porch stem walls with the Straight Foundation Wall tool, and a room's stem-wall
   height. You can draw walls on the Foundation floor under the porch and set their height.

## Deck and porch stairs (pages 119–121)

*Not in HouseCraft yet:* stairs that go **down** to the terrain. HouseCraft's stairs only climb to the floor
above, so skip the deck stairs and the concrete porch steps for now. They're the top item for the next round of
work (FEATURES ST-20).

## Annotations (pages 121–124)

1. **Text** tool: `CONCRETE STEPS TO GRADE` below the porch and `FRAMED STEPS TO GRADE` by the deck.
2. **Callout** by the deck: `EXTERIOR LANDING 36" MINIMUM IN DIRECTION OF TRAVEL`, tick **Numbered Note**,
   and copy it to the porch.
3. *Not in HouseCraft yet:* reordering schedule rows.

## Revisions and folders (page 124)

**File › Save As…** `Chic Cottage-Porches`. Keep the archive copies in a folder of their own in Windows
Explorer. HouseCraft has no Project Browser folders for documents.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Stairs down to the terrain; concrete stairs | ST-20 |
| Half Wall / Railing / Deck Railing tools and defaults | WL-29 |
| Decks as rooms; deck footing settings | SITE-26 |
| Moving automatic dimensions; extension-line handles | DIM-27, DIM-28 |
| Terrain reference point | SITE-27 |
| Foundation wall tool; per-room stem walls | FD-09 |
| Reordering schedule rows | EST-28 |

Fixed for this tutorial: **porches are left out of the foundation** (Porch and Deck room types are "outdoor"),
and basement walls rebuilt along the same line keep the type you gave them.
