# Tutorial 26, Layout Page Templates: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Layout Tutorials › Layout Page Templates** (pages 451–459).
It carries on from [Tutorial 25](25-landscaping-design.md). The steps also run as the automatic test
[26-layout-page-templates.json](26-layout-page-templates.json).

A HouseCraft layout lives inside the plan file, not in a separate `.layout` file. It shows in the **Project Browser**
under **Layouts**.

## The layout and its pages (pages 452–455)

1. **File › Layout › New Layout**. Rename it `Chic Cottage`.
2. In the **Project Browser**, right-click the layout and choose **Open Object**. Its dialog lists the **Pages**: one
   row per page with its sheet number, title, and the template page drawn underneath it. **Add Page** adds a page at
   the end.
3. Page 1: sheet `Template`, title `Standard Template`. Page 2: sheet `Cover Template`, title
   `Cover Sheet Template`.
4. *Not in HouseCraft yet:* a **Use as Page Template** tick box. Any page can be drawn under another, but template
   pages are printed with the rest.

## Assigning templates and titles (pages 455–457)

Add five more pages. Then, in the page rows:

| Page | Sheet | Title | Template page |
|---|---|---|---|
| 3 | Cover | Cover Sheet | Cover Template |
| 4 | A0.1 | General Notes | Template |
| 5 | A1.1 | Site Plan | Template |
| 6 | A1.2 | Floor 1 Plan | Template |
| 7 | E1.1 | Electrical Floor 1 | Template |

## Page numbers (pages 457–458)

*Not in HouseCraft yet:* labels such as `A1.#` that number each prefix on its own. Type the sheet numbers yourself,
as in the table. **Number Pages** gives every page one prefix (A-1, A-2…).

## Inserting pages (pages 458–459)

*Not in HouseCraft yet:* Insert Page Before / After. Add pages at the end, then retype the rows so the Foundation Plan
(A1.2) comes before Floor 1 Plan (A1.3), and Elevations (A2.1) and Sections (A2.2) come before Electrical Floor 1.

## Revision (page 459)

*Not in HouseCraft yet:* Make a Copy. Use **File › Save As…** `Chic Cottage-Page Templates`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Template flag (no print), # page labels, separate layout files | PRJ-46 |
| Insert page before / after | PRJ-41 |
| Make a Copy | PRJ-45 |
