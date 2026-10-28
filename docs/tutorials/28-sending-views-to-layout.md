# Tutorial 28, Sending Views to Layout: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Layout Tutorials › Sending Views to Layout** (pages 483–500).
It carries on from [Tutorial 27](27-title-blocks-and-borders.md). The steps also run as the automatic test
[28-sending-views-to-layout.json](28-sending-views-to-layout.json).

The page numbers below match the layout from Tutorial 26: page 4 Site Plan (A1.1), page 6 Floor 1 Plan (A1.3), page 7
Elevations (A2.1), page 8 Sections (A2.2), page 9 Electrical Floor 1 (E1.1). HouseCraft's menus count pages from 1,
so page 6 here appears as "page 7" in the menu (Chief's page 0 is HouseCraft's first page).

## Plan views (pages 485–488)

1. Default drawing scale: open the layout (Project Browser › **Layouts**) and set **Default plan scale** to
   `1/4" = 1'-0"`. Plans sent without a scale take it; you can still change each box's **Scale**.
2. Pick the dimensioned floor plan view on the top toolbar. **File › Layout › Send to Layout › Chic Cottage, page …**
   for Floor 1 Plan. Open the new box and set **Scale** `1/4" = 1'-0"`.
3. Switch to **Electrical Plan** and send it to the Electrical Floor 1 page the same way. The box is linked to that
   saved plan view, so it shows the view's layers and follows any change to the view. (Open a box and pick another
   view under **Shows** to relink it.)
4. Switch to **Site Plan** and send it to the Site Plan page. Open the box and set **Scale** `1" = 100'-0"`.
5. Crop and move it with **Left**, **Bottom**, **Width** and **Height** (paper inches) in the box's dialog, or drag its
   handles. **Rotation** `90` turns the plan inside the box. **Center Object › On Page** centres the box inside the
   border. **Caption Shift Right / Up** moves its caption.

## Sections and elevations (pages 489–494)

1. In the Project Browser under **Elevations and Sections**, open **Stair Section**. Send it to the Sections page and
   set **Scale** `1/2" = 1'-0"`.
2. **3D › Elevations and Sections › Cross Section / Elevation Tool**: click outside the right side of the house, then
   click towards it. Name the view `Exterior Elevation - Right`. Send it to the Elevations page at `1/4" = 1'-0"`.
3. Crop it and move it into a corner. Section and elevation boxes are live drawings that follow the model, colour
   filled like Chief's colour plot lines. Tick **Lines Only** in the box's dialog for plain lines.
4. Open the Stair Section box. Set **Link to Page** to A1.3 and type the **Caption** `S{number} {view}  {scale}
   ({link_sheet})`: it reads `S1 STAIR SECTION  1/2" = 1'-0"  (A1.3)` and keeps up if the box is renumbered or the
   pages are relabelled. **Marker** puts the caption in a circle, hexagon, square or diamond.

## Perspective views (pages 494–496)

Save the camera view as a picture with **File › Export › Export 3D View as Image…**, then on the Cover Sheet page
choose **File › Layout › Add Picture…** and pick it. Open the box to size and place it. *Not in HouseCraft yet:* a
live 3D view on a page that updates with the model.

## Printing (pages 496–497)

With the layout open, **File › Layout › Export Layout as PDF…** writes every page at the layout's sheet size (set to
36" × 24" in Tutorial 27).

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Live 3D views on layout pages | PRJ-48 |

**Fixed while following this tutorial:** a layout box's **Scale** list now has engineering scales for site plans
(1" = 10', 20', 50' and 100'), and they're labelled that way.
