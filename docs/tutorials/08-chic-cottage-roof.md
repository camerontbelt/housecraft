# Tutorial 8, Chic Cottage Roof: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Roof Tutorials › Chic Cottage Roof** (pages 136–158).
It carries on from [Tutorial 6](06-decks-and-porches.md). Read [the roof styles page](07-roof-styles.md) first for
where the roof settings are. The steps also run as the automatic test
[08-chic-cottage-roof.json](08-chic-cottage-roof.json).

## Views (pages 138–140)

**View › Add Standard Plan Views** adds a **Roof Plan** view with its own layer set (roofs on; rooms, electrical,
cabinets and dimensions off). **Window › Tile Vertically With › 3D Overview** puts the 3D view beside it.

## First roof, over Floor 2 (pages 140–145)

1. **Edit › Default Settings… › Roofs**: pitch `12/12`.
2. Go up to Floor 2. Open the left wall and tick **Full Gable Wall**; do the same for the wall over the Garage door.
3. **Nested gable:** turn on **View › Reference Floor Display** (F9). On Floor 2, use **Straight Exterior Wall** to
   draw over the porch half walls below. Set both to **Shingle-6**, and make the front one a **Full Gable Wall**.
4. **Reverse gable:** choose **Break Wall** (Wall Tools; **new**) and click the right wall level with the back of
   the Garage. Make the back piece a **Full Gable Wall**.
5. **Build › Roof › Build Roof** (Ctrl+R).
6. **Roof over the deck:** open the deck and tick **Covered**, then set **Roof Pitch** `2` (2 in 12) and tick
   **Gable Ends**. The roof sits on posts at the deck's corners; its ridge runs along the deck's long side.

## Story and a half (pages 145–148)

Chief builds the roof over Floor 1 (Ignore Top Floor) and raises it 18 1/8". In HouseCraft:

1. On Floor 2: **Build › Roof › Remove Roof**.
2. **Edit › Default Settings… › Roofs**: set **Raised plate (heel)** to `18 1/8"`. That's Chief's "Raise/Lower All
   Roof Planes".
3. On **Floor 1**, make these Full Gable Walls: the left wall, the Garage front, the Porch's front half wall, and (after
   **Break Wall** level with the back of the Garage) the back piece of the right wall.
4. Still on Floor 1: **Build › Roof › Build Roof**. The roof now sits on Floor 1's walls, 115 1/4" up.
5. **New:** Floor 2's walls now stop under this roof in 3D, as in Chief. Before, they stuck up through it.
6. Close the gap behind the Garage gable: on Floor 2, drag the end of the porch-front wall to the right wall.
   Open the little room it makes and set its **Room Type** to **Attic**.

## Curved roof and roof details (pages 148–154)

1. **Gull wing over the porch:** open the porch's left half wall: **Roof Pitch** `4/12`, **Upper Roof** on, **Upper
   Pitch** `12/12`, **Break Height** `28"`. Chief's "In From Baseline 7'" at 4/12 is 28" up.
2. *Not in HouseCraft yet:* editing roof planes edge by edge (Add Break on a roof edge, dragging corners, Join Roof
   Planes, curved roof planes), automatic roof returns, and adding frieze from the library to chosen planes. Frieze
   under all eaves is a setting in **Default Settings › Roofs**.

## Roof plan display and annotations (pages 154–157)

1. *Not in HouseCraft yet:* showing the roof planes on the floor above (they show on Floor 1, where they're built).
2. **O.H. dimension:** use the **Manual Dimension** tool from the eave to the wall. Open it and set its text to
   `O.H. {}` (`{}` is replaced with the measurement).

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Edit roof planes: break edges, corners, join, curved planes | RF-18 |
| Raise/Lower and Ignore Top Floor in Build Roof; Display On Floor Above | RF-19 |
| Automatic roof returns; frieze per plane | RF-03, RF-12 |

Added for this tutorial: **Break Wall** (Wall Tools), and **walls under a roof built on a lower floor stop at
the roof** (the story-and-a-half look).
