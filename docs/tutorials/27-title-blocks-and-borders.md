# Tutorial 27, Title Blocks and Borders: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Layout Tutorials › Title Blocks and Borders**
(pages 460–482). It carries on from [Tutorial 26](26-layout-page-templates.md). The steps also run as the automatic
test [27-title-blocks-and-borders.json](27-title-blocks-and-borders.json).

How it differs: a HouseCraft layout page holds **boxes**: views, CAD details, schedules, text. You don't draw lines
straight onto the sheet. So the border and title block frames are drawn in a **CAD detail** and placed on the template
page at full size (scale 1). The words go in text boxes that use page macros.

## Sheet setup (pages 462–464)

1. Open the layout (Project Browser › **Layouts**, right-click › **Open Object**): **Sheet Width** `36"`,
   **Sheet Height** `24"`.
2. Untick **Title Block** to drop HouseCraft's built-in border and title strip.
3. *Not in HouseCraft yet:* a layout grid snap and arrow-key nudging.

## Border and title block frames (pages 464–468)

1. **CAD › CAD Details › New CAD Detail**, named `Standard Title Block`. Draw rectangles (**Rectangular Polyline**)
   by their corners, in inches from the sheet's bottom-left:
   - border: 1/8" in from each edge (0.125, 0.125 to 35.875, 23.875);
   - second border: 1/4" in;
   - title block frame: 2" wide down the right side (33.5, 0.5 to 35.5, 23.5);
   - component frames inside it, 1/8" apart, from the bottom: 1", 1", 4", 5" and 4" high.

   *Not in HouseCraft yet:* the Concentric behaviour and Point to Point Move, so type the corners. Open each rectangle to
   type them exactly.
2. With the detail open, **File › Layout › Send to Layout › Chic Cottage, page 1** (the Standard Template page). Open
   the box and set **Left** `0`, **Bottom** `0`, **Width** `36"`, **Height** `24"`, **Scale** 1, and an empty
   **Caption**.

## Title block text (pages 468–475)

On the template page, **File › Layout › Add Notes Box** for each piece of text. Open the box, type in **Text**, and set
its position and size to fit its frame:

- company name and contact in the 4" frame;
- `{page_title}` (the page's title) in the 5" frame;
- `{sheet}` (the page's sheet number) in the bottom 1" frame;
- `{date}` in the 1" frame above it.

The macros fill in for whichever page the template sits under, so page A0.1 shows "General Notes A0.1". *Not in
HouseCraft yet:* rotating text 90°, and Center Object on layout pages.

## Revision table (pages 475–477)

**File › Layout › Add Revision Table** on the template page, above the sheet title. *Not in HouseCraft yet:* minimum
rows, and revisions that only show on the pages they belong to.

## Cover sheet template (pages 477–481)

1. Send the same `Standard Title Block` detail to page 2 (Cover Sheet Template) at full size, so it has the same border.
2. *Not in HouseCraft yet:* pictures (a company logo) on layout pages.
3. A notes box with `{project}` for the project name.
4. **File › Layout › Add Sheet Index** in the top-right corner, caption `Sheet Index`. *Not in HouseCraft yet:*
   renaming or removing its columns.

## Revision and folders (page 481)

*Not in HouseCraft yet:* Make a Copy and Project Browser folders. Use **File › Save As…** `Chic Cottage-Title Block`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Drafting directly on layout sheets (CAD, grid, nudge, rotated text, centring, pictures, table options) | PRJ-47 |
| Concentric edit; Point to Point Move | CAD-32, CAD-31 |
| Make a Copy; folders | PRJ-45, PRJ-09 |
