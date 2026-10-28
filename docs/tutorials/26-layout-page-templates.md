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
4. Tick **Template** in both rows (Use as Page Template). Template pages are drawn under the pages that use them,
   but they don't print, aren't counted by `{page}` and `{page_count}`, and stay out of the sheet index.

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

In the layout's dialog, type in **Label pages**: `Template, Cover Template, Cover, A0.#, A1.#, A1.#, E1.#`. Each `#`
becomes the page's number among the pages with the same prefix, so the two `A1.#` pages read A1.1 and A1.2.
(**Number Pages** gives every page one prefix: A-1, A-2…)

## Inserting pages (pages 458–459)

Each page row has **Insert Before** and **Insert After** buttons. Insert the Foundation Plan before Floor 1 Plan, and
Elevations and Sections after it; boxes already on later pages move with their pages. Then label the pages again
with `A1.#` for the plans and `A2.#` for Elevations and Sections, so they read A1.2, A1.3, A2.1 and A2.2.

## Revision (page 459)

**File › Make a Copy…** `Chic Cottage-Page Templates`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Separate layout files | PRJ-46 |
