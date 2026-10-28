# Tutorial 17, Light Fixtures: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Kitchen and Bath Tutorials › Light Fixtures**
(pages 298–316). It carries on from [Tutorial 16](16-appliances-and-fixtures.md). The steps also run as the automatic
test [17-light-fixtures.json](17-light-fixtures.json).

Lights are under **Build › Electrical**. There are three: **Ceiling Light**, **Recessed Light** and **Wall Light**.
Chief has one Light tool that picks the fixture from where you click. In HouseCraft you pick the tool for the kind
of light you want.

## Defaults and the electrical plan view (pages 299–301)

1. *Not in HouseCraft yet:* Electrical Defaults that choose which fixture style each tool places. You set the style
   on each light instead (below).
2. If the plan view list on the top toolbar has no **Electrical Plan**, choose **View › Add Standard Plan Views**
   once. Then pick **Electrical Plan** from the list. Labels, furniture and the section camera are hidden in it.

## Ceiling lights (pages 301–302)

1. **Recessed Light**: click in the Kitchen between the island and the dishwasher.
2. **Ceiling Light**: one in the Dining room, one in the Foyer near the front door, one on the Deck.
   **Recessed Light**: one in the Foyer in line with the Garage door.
3. Give them styles: double-click the Dining light, set **Fixture** to **Chandelier**. Do the same for the Foyer
   light (**Flush Mount**) and the Deck light (**Lantern**). Each style shows at its real size in plan and in 3D.
   **Fixture Width** is how wide it is; **Drop** is how far it hangs below the ceiling.

## Positioning the can light (pages 302–304)

1. *Not in HouseCraft yet:* dimension defaults that choose what a manual dimension snaps to. Dimensions snap to any
   object edge already.
2. A **Manual Dimension** from the refrigerator to the island shows the 42" gap.
3. Draw the dimension from the Kitchen's left wall to the light. Then select it, choose **Edit › Add Extension Line**
   and click the Kitchen's right wall: one dimension now measures both gaps. Select the light, click the dimension's number, type `1'9"` and press Enter: the light moves and the
   dimension follows it. Do the same with a dimension from the dishwasher, typing `21"`.

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
4. Open the chandelier and set **Fixture Width** `30"`. *Not in HouseCraft yet:* Retain Aspect Ratio, so set the
   **Drop** yourself if you want it to grow taller too.

## Table and floor lamps (pages 312–313)

1. **Ceiling Light**: click either side of the bed in the Bedroom, and once in a corner of the Living room.
2. Open each Bedroom one: **Fixture** **Table Lamp**, **Height** `26"` (the end table's top).
3. Open the Living room one: **Fixture** **Floor Lamp**. Lamps stand on the table or floor instead of hanging from
   the ceiling.

## Lighting schedule (pages 313–314)

**Tools › Schedules › Place in Plan › Electrical, Plumbing and HVAC Schedule**, then click beside the plan. Open it:
**Only in Room** the Kitchen, **Only Rows With** `light`, and **Title** `Kitchen Lighting`.

## Annotations (pages 314–316)

1. Make a text style `Electrical Notes` (**Tools › Text Styles…**) and tick **Uppercase**: text using it shows in
   capitals whatever you type.
2. **Text with Arrow**: from the Deck light, type `CEILING LIGHT`, or `{ref_name}` to show the name of the light the
   arrow points at (Lantern here). `{ref_description}` shows its description.
3. **File › Make a Copy…** `Chic Cottage-Lighting`. You keep working in CHIC COTTAGE-CURRENT.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| More fixture styles (bollards, track) and Electrical Defaults choosing a style | EL-11 |
| One context-sensitive Light tool; path lights | EL-12 |
| Locate settings for manual dimensions | DIM-29 |
| Multiple Copy interval and 2D array | CAD-08 |
