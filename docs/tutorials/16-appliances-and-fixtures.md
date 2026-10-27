# Tutorial 16, Appliances and Fixtures: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Kitchen and Bath Tutorials › Appliances and Fixtures**
(pages 277–297). It carries on from [Tutorial 15](15-cabinet-layout.md). The steps also run as the automatic test
[16-appliances-and-fixtures.json](16-appliances-and-fixtures.json).

Appliances and bathroom fixtures are under **Build › Cabinets › Appliances & Fixtures** (and in the Cabinet tools
list on the left). Pick one, then click in the plan. Refrigerators, dishwashers, ranges, toilets, tubs and sinks turn
their back to the nearest wall.

## Refrigerator (pages 278–281)

1. **Refrigerator**: click against the wall between the Kitchen and Dining room, beside the full-height cabinet.
2. **Wall Cabinet**: click over the refrigerator. Open it: **Width** `36"`, **Depth** `24"`, **Height** `15"`, and
   **Elevation** `70"` so it sits on top of the refrigerator.
3. *Not in HouseCraft yet:* the cabinet **Partition** (a finished panel beside the refrigerator, with crown).

## Dishwasher (pages 281–282)

**Dishwasher**: click in the 24" gap beside the sink base. Nudge it with the arrow keys until it lines up with the
cabinet fronts.

## Range and hood in the island (pages 282–286)

1. Our island's cabinets aren't a block, so there's nothing to explode first. (To try it: select the island cabinets,
   **Edit › Make Architectural Block**, then **Edit › Explode Architectural Block**.)
2. Click the middle island cabinet and press **Delete**.
3. **Range**: click where it was. Open it and set **Facing** to 180° so it faces the same way as the island.
4. **Range Hood**: click over the range and give it the same **Facing**.

## Built-in microwave (pages 286–288)

1. Open the wall cabinet at the top left of the Kitchen. **Depth** `18"`.
2. Under **Front (top to bottom)**, the first row stays **Pair of doors**. Click **Add row**, set its height to `17"`
   and choose **Built-in appliance**. The microwave sits in the bottom of the cabinet, under the doors.

## Bathroom (pages 288–294)

1. **Sink Base Cabinet**: against the Bath's right wall, **Width** `30"`. Add a
   **Vanity Sink** in it.
2. *Not in HouseCraft yet:* faucets, tub spouts and shower heads.
3. **Bathtub** along the Bath's left wall. Use **Center Object** (edit toolbar) and click the wall to centre it.
4. **Toilet** between the tub and the vanity.
5. *Not in HouseCraft yet:* **Reverse Symbol**, which puts the tub drain at the other end. Turn the tub 180°
   instead if the room allows it.

## Wall elevation with dimensions (pages 294–297)

1. **3D › Elevations and Sections › Cross Section / Elevation Tool**: click in the Kitchen, then click towards the
   left wall.
2. Rename it `Kitchen Elevation` in the Project Browser under **Elevations and Sections**, and open it.
3. **CAD › Dimensions › Elevation Dimensions**: cabinet, appliance and ceiling heights are dimensioned for you.
4. *Not in HouseCraft yet:* handles for dragging extension lines, and a fill colour for those handles.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Cabinet partitions | CB-30 |
| Faucets, tub spouts, shower heads | EL-10 |
| Reverse Symbol (mirror a fixture) | CB-31 |
| Extension-line handles on dimensions | DIM-28 |
