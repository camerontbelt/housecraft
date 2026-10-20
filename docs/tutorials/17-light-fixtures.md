# Tutorial 17, Light Fixtures: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Kitchen and Bath Tutorials › Light Fixtures**
(pages 298–316). It carries on from [Tutorial 16](16-appliances-and-fixtures.md). The steps also run as the automatic
test [17-light-fixtures.json](17-light-fixtures.json).

Lights are under **Build › Electrical**. There are three: **Ceiling Light**, **Recessed Light** and **Wall Light**.
Chief has one Light tool that picks the fixture from where you click. In HouseCraft you pick the tool for the kind
of light you want.

## Defaults and the electrical plan view (pages 299–301)

1. *Not in HouseCraft yet:* Electrical Defaults that choose which library fixture each tool places. Skip it.
2. If the plan view list on the top toolbar has no **Electrical Plan**, choose **View › Add Standard Plan Views**
   once. Then pick **Electrical Plan** from the list. Labels, furniture and the section camera are hidden in it.

## Ceiling lights (pages 301–302)

1. **Recessed Light**: click in the Kitchen between the island and the dishwasher.
2. **Ceiling Light**: one in the Dining room, one in the Foyer near the front door, one on the Deck.
   **Recessed Light**: one in the Foyer in line with the Garage door.
3. *Not in HouseCraft yet:* chandeliers, pendants, flush mounts and lanterns. Every ceiling light is the same
   fixture.

## Positioning the can light (pages 302–304)

1. *Not in HouseCraft yet:* dimension defaults that choose what a manual dimension snaps to. Dimensions snap to any
   object edge already.
2. A **Manual Dimension** from the refrigerator to the island shows the 42" gap.
3. Chief moves the light by typing on that dimension. In HouseCraft that only works for walls. Instead, open the
   light (double-click it) and type its **Position**, half-way between the refrigerator and the island.
   Do the same for the gap to the dishwasher.

## Spacing the lights (pages 305–307)

Select the light, then use the edit toolbar:

1. **Move / Copy / Rotate / Reflect…** (Transform / Replicate) › **Move**: **Y distance** `40"`, tick **Make a
   copy**, **Apply**. The copy is 40" above.
2. Shift-click both lights. Chief reflects them about the island. In HouseCraft, use **Move** with a copy and type
   the X distance to the other side of the island (twice the gap from light to island centre).
3. All four lights: **Move**, **X distance** `6'6"`, copy, **Apply**.
4. The top and bottom of the new ones: **X distance** `-3'3"`, copy, **Apply**.

## Basement array (pages 307–310)

1. Go down to the **Foundation** floor (**Build › Floor › Down One Floor**).
2. **Recessed Light** in the lower-left corner. Open it and set its position 5' in from both walls.
3. **Multiple Copy…**: **Y distance** `50"`, **Number of copies** `4`. That's a column of five.
4. Select the whole column. **Multiple Copy…**: **X distance** `52"`, **Number of copies** `4`. That's the
   array. *Not in HouseCraft yet:* the interval dialog and right-drag array.

## Wall lights (pages 310–312)

1. **Wall Light**: click on the wall over the Kitchen sink. Click the back wall outside, beside the Deck's sliding
   door. It snaps to the outside face and faces out.
2. Two more **Wall Lights** on the Bath's right wall, either side of the vanity.
3. Open the Kitchen sconce: **Height** `86"`. Use **Center Object** and click the left window to centre it. Copy
   it over the other two windows the same way.
4. *Not in HouseCraft yet:* light fixture sizes (Retain Aspect Ratio, Width).

## Table and floor lamps (pages 312–313)

*Not in HouseCraft yet:* table and floor lamps.

## Lighting schedule (pages 313–314)

**Tools › Schedules › Place in Plan › Electrical, Plumbing and HVAC Schedule**, then click beside the plan. *Not in
HouseCraft yet:* a schedule for one room, lighting only, or with your own title. It lists the whole floor.

## Annotations (pages 314–316)

1. *Not in HouseCraft yet:* an uppercase setting in text defaults. Type the caps yourself.
2. **Text with Arrow**: from the Deck light, type `CEILING LIGHT`. *Not in HouseCraft yet:* the `%description%`
   macro reading the object the arrow points at.
3. **Revision copy:** *not in HouseCraft yet:* Make a Copy. **File › Save As…** `Chic Cottage-Lighting` makes the
   copy. Then reopen CHIC COTTAGE-CURRENT to keep working in it.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Light fixture library and sizes; lamps; Electrical Defaults | EL-11 |
| One context-sensitive Light tool; path lights | EL-12 |
| Locate settings for manual dimensions | DIM-29 |
| Moving objects other than walls by typing on a dimension | DIM-03 |
| Multiple Copy interval and 2D array | CAD-08 |
| Schedule from a room, category filter, title | EST-31 |
| Uppercase text defaults | TXT-24 |
| Text reading the object its arrow points at | TXT-23 |
| Make a Copy | PRJ-45 |
