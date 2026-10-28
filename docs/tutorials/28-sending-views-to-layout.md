# Tutorial 28, Sending Views to Layout: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Layout Tutorials › Sending Views to Layout** (pages 483–500).
It carries on from [Tutorial 27](27-title-blocks-and-borders.md). The steps also run as the automatic test
[28-sending-views-to-layout.json](28-sending-views-to-layout.json).

The page numbers below match the layout from Tutorial 26: page 4 Site Plan (A1.1), page 6 Floor 1 Plan (A1.3), page 7
Elevations (A2.1), page 8 Sections (A2.2), page 9 Electrical Floor 1 (E1.1). HouseCraft's menus count pages from 1,
so page 6 here appears as "page 7" in the menu (Chief's page 0 is HouseCraft's first page).

## Plan views (pages 485–488)

1. *Not in HouseCraft yet:* a plan-wide default drawing scale. You pick the scale on each box.
2. Pick the dimensioned floor plan view on the top toolbar. **File › Layout › Send to Layout › Chic Cottage, page …**
   for Floor 1 Plan. Open the new box and set **Scale** `1/4" = 1'-0"`.
3. Switch to **Electrical Plan** and send it to the Electrical Floor 1 page the same way. The box keeps the view's
   layer set, so it shows the electrical layers. (Chief links the saved plan view; HouseCraft links its layer set.)
4. Switch to **Site Plan** and send it to the Site Plan page. Open the box and set **Scale** `1" = 100'-0"`.
5. Crop and move it with **Left**, **Bottom**, **Width** and **Height** (paper inches) in the box's dialog, or drag its
   handles. **Rotation** `90` turns the plan inside the box. **Center Object › On Page** centres the box inside the
   border. **Caption Shift Right / Up** moves its caption.

## Sections and elevations (pages 489–494)

1. In the Project Browser under **Elevations and Sections**, open **Stair Section**. Send it to the Sections page and
   set **Scale** `1/2" = 1'-0"`.
2. **3D › Elevations and Sections › Cross Section / Elevation Tool**: click outside the right side of the house, then
   click towards it. Name the view `Exterior Elevation - Right`. Send it to the Elevations page at `1/4" = 1'-0"`.
3. Crop it and move it into a corner. Section and elevation boxes are live line drawings that follow the model.
   *Not in HouseCraft yet:* a colour-fill option for them.
4. Open the Stair Section box and type a **Caption** such as `S1 STAIR SECTION 1/2" = 1'-0" (A1.3)`. *Not in
   HouseCraft yet:* callout-style labels with macros and links to other pages. **Marker** puts the caption in a circle,
   hexagon, square or diamond.

## Perspective views (pages 494–496)

*Not in HouseCraft yet:* 3D camera views on layout pages. For now, save the 3D view as a picture with
**File › Export › Export 3D View as Image…** for the cover sheet.

## Printing (pages 496–497)

With the layout open, **File › Layout › Export Layout as PDF…** writes every page at the layout's sheet size (set to
36" × 24" in Tutorial 27).

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| 3D views on pages; colour-filled elevations; default drawing scale | PRJ-48 |
| Callout labels; linking a saved plan view | PRJ-25 |

**Fixed while following this tutorial:** a layout box's **Scale** list now has engineering scales for site plans
(1" = 10', 20', 50' and 100'), and they're labelled that way.
