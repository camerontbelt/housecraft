# Tutorial 12, Room Moldings: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Interior Design Tutorials › Room Moldings** (pages 211–221).
It carries on from [Tutorial 11](11-finish-materials.md). The steps also run as the automatic test
[12-room-moldings.json](12-room-moldings.json).

## Default moldings (pages 212–213)

Chief sets default moldings per floor. In HouseCraft they're set per **room type**: **Tools › Room Types…**,
pick a type, and under **Moldings** add a base molding and a **Crown** 3" high. Do it for Living, Kitchen, Dining,
Bedroom, Bath, Closet and Entry. *Not in HouseCraft yet:* the molding library (BM08, CM02, CR01). You pick a
built-in shape (flat, crown, cove, bead) and size it, or use a profile of your own (below).

## One room's moldings (pages 213–215)

1. Double-click the Dining room. Under **Moldings**, add a **Chair** rail (bead shape) 32" up. The room now has its
   own list instead of its type's.
2. For a room without the crown, open it and keep only the base molding.

## A custom profile and a molding band (pages 215–219)

1. Draw a **Rectangular Polyline** (CAD), 1 1/2" × 11 1/4". Select it and choose **CAD › Shapes and Solids ›
   Make Molding Profile from Selection**, then name it `2x12 Profile`.
2. *Not in HouseCraft yet:* the Exterior Room and Make Room Molding Polyline. Draw it with the **Molding Polyline**
   tool (CAD Tools) round the outside of the house instead. Open it: height `96"`, molding: your 2x12 Profile.
3. *Not in HouseCraft yet:* turning the molding off on one edge (the Garage front) or moving one edge (onto the
   deck). Draw the polyline round only the edges you want.

## Schedule (page 220)

**Schedule › Molding Schedule**, then click in the plan to place it. Each room gets a row for each molding it has
(base, crown, chair rail), with the molding's height and the length round the room.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Molding library profiles; default moldings per floor | MAT-27, RM-05 |
| Exterior Room; molding on/off per polyline edge | RM-20 |
