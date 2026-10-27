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
2. Open the deck: **Plank thickness** `1"`, **Plank gap** `1/4"`, **Footing Depth** `30"` (how far the post
   footings reach into the ground). It has a railing on its open sides.
3. *Not in HouseCraft yet:* the deck as a room enclosed by deck railings, and a layer of its own for the footings.

## Dimensioning exterior rooms (pages 112–115)

*Not in HouseCraft yet:* moving the automatic dimensions out of the deck (they're rebuilt automatically). Draw a
manual dimension along the deck's outer edge instead.

1. Select it and choose **Edit › Add Extension Line**, then click the middle of the edge: the dimension now
   measures to that point too. Each extension line has a small square handle you can drag.
2. **Edit › Remove Extension Line**, then click that extension line to take it away again.
3. To extend the dimension (Add Segment), use **Add Extension Line** and click past its end, at the house's right
   side.

## Terrain (pages 115–118)

1. **Terrain › Terrain Perimeter**: click four corners round the house, about 60' deep. Double-click to finish.
2. **Terrain › Terrain Specification**: set the base elevation to `-28"`. Tick **Reference Point** and set
   **Reference At** to the Garage's front corner. The ground is held at `-28"` there and slopes to any elevation
   data you add later.
3. *Not in HouseCraft yet:* porch stem walls with the Straight Foundation Wall tool, and a room's stem-wall
   height. You can draw walls on the Foundation floor under the porch and set their height.

## Deck and porch stairs (pages 119–121)

1. **Deck stairs:** pick the **Straight Stair** tool and click just outside the deck's left railing, pointing the
   stair at the deck. Double-click the stair to open it, set **Width** `5'`, keep both railings, and under **Style**
   tick **Down to grade**. The stair now starts at the deck's top and steps down to the ground, the plan shows
   **DN**, and the deck railing opens where the stair meets it.
2. **Porch steps:** another Straight Stair at the middle of the porch's front edge, pointing at the house.
   **Width** `6'`, tick **Down to grade**, and untick **Stringers** so the steps are solid blocks.
3. *Not in HouseCraft yet:* a concrete material just for the stairs, and stairs opening a half wall. Add a break in
   the porch's front half wall on each side of the steps (**Edit › Add Break**) and delete the piece between.

## Annotations (pages 121–124)

1. **Text** tool: `CONCRETE STEPS TO GRADE` below the porch and `FRAMED STEPS TO GRADE` by the deck.
2. **Callout** by the deck: `EXTERIOR LANDING 36" MINIMUM IN DIRECTION OF TRAVEL`, tick **Numbered Note**,
   and copy it to the porch.
3. **Tools › Schedules › Place in Plan › Note Schedule** beside the plan. To change the order, open it and use the
   **▲** / **▼** buttons under **Row Order** (Move Up / Move Down in Schedule). **Usual Order** puts it back.
   *Not in HouseCraft yet:* dragging rows in the schedule itself.

## Revisions and folders (page 124)

**File › Make a Copy…** `Chic Cottage-Porches`. Then, under **Projects** in the Project Browser, right-click the
project (`Chic Cottage`) and choose **New Folder…**. Name it, then drag the revision plans onto it (Ctrl+click or
Shift+click to pick several first). They move on disk too.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Stairs opening half walls; concrete stair material | ST-20, ST-12 |
| Half Wall / Railing / Deck Railing tools and defaults | WL-29 |
| Decks as rooms; a footing layer | SITE-26 |
| Moving automatic dimensions | DIM-27 |
| Foundation wall tool; per-room stem walls | FD-09 |
| Dragging schedule rows | EST-28 |

Fixed for this tutorial: **porches are left out of the foundation** (Porch and Deck room types are "outdoor"),
and basement walls rebuilt along the same line keep the type you gave them.
