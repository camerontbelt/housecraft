# Tutorial 18, Electrical Objects: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Kitchen and Bath Tutorials › Electrical Objects**
(pages 317–330). It carries on from [Tutorial 17](17-light-fixtures.md). The steps also run as the automatic test
[18-electrical-objects.json](18-electrical-objects.json).

Stay in the **Electrical Plan** view from Tutorial 17. Outlets, switches and detectors are under
**Build › Electrical**. Wall devices snap to the nearest wall within 2' and face into the room. On an outside wall
they face out.

## Outlets (pages 319–321)

1. **Duplex Outlet**: click on the wall behind the refrigerator, then on the wall left of the sliding door to the
   Deck. Click a third time on the outside of that wall, on the Deck.
2. Open one (double-click): **Height** is blank, which means the standard 12".
3. *Not in HouseCraft yet:* the WP mark on outdoor outlets and a floor-outlet symbol. Recessing the outlet into the
   wall (distance from wall, wall cut) isn't there either. Our outlet already sits on the wall's outer face.

## Counter outlets (pages 321–323)

1. **GFCI Outlet** on the Kitchen's left wall, under a window. Open it and set **Height** `39"`. *Not in HouseCraft
   yet:* outlets starting at counter height when they're over a countertop, and turning an outlet sideways in an
   elevation.
2. **Center Object**, then click the sink base: the outlet centres on it.
3. Select it and open the edit toolbar's **Move / Copy / Rotate / Reflect…** › **Move**. Move it `72"` along the
   wall without a copy. Then copy it three times `-48"` with **Multiple Copy…** (**Number of copies** `3`).
4. One more **GFCI Outlet** on the Dining room wall, left of the refrigerator. Height `39"`.
5. **240V Outlet** on the island where the range is. *Not in HouseCraft yet:* the floor-mounted 240V symbol and a
   garbage disposal outlet.

## Switches (pages 324–325)

1. **Switch**: on the back wall right of the sliding door, and right of the doorway into the Dining room.
2. Two more switches beside the Deck one. *Not in HouseCraft yet:* Make Ganged Electrical Block, and the countertop
   air switch.

## Wiring (pages 326–328)

**Build › Electrical › Connect Electrical**. Click a switch, then click each fixture it controls. Click a wired
fixture again to unwire it.

1. The Deck switch: the outside wall light, then the Deck ceiling light.
2. The middle Deck switch: the four right-hand Kitchen can lights. Then the Dining doorway switch: the same four
   lights. Both switches become **3-way** automatically, because they share fixtures.
3. *Not in HouseCraft yet:* editable wiring splines from fixture to fixture. HouseCraft draws a wire from the
   switch to each fixture.

## Data and security (page 328)

**Smoke Detector** in the Bedroom. *Not in HouseCraft yet:* data jacks, alarms and other security symbols.

## Legend (page 328)

**Tools › Schedules › Place in Plan › Electrical, Plumbing and HVAC Schedule**, then click beside the plan. Open it
and tick **Thumbnails** for a picture on each row. *Not in HouseCraft yet:* renaming it "Electrical Legend", and
putting it in a CAD detail.

## Revisions and folders (page 329)

**File › Make a Copy…** `Chic Cottage-Electrical`. Then file the Kitchen and Bath revisions in a
`Kitchen and Bath Tutorials` folder: **Projects** › right-click the project › **New Folder…**, and drag them in.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| WP outlets, floor outlets, counter-height outlets, recessing, sideways outlets | EL-14 |
| Data/security symbols, air switch, disposal and floor outlets | EL-13 |
| Ganged electrical blocks | EL-09 |
| Wiring splines | EL-01 |
| Schedule title; schedules in CAD details | EST-31 |
