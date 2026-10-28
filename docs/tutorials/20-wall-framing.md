# Tutorial 20, Wall Framing: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Framing Tutorials › Wall Framing** (pages 349–363). It
carries on from [Tutorial 19](19-floor-framing.md). The steps also run as the automatic test
[20-wall-framing.json](20-wall-framing.json).

## Setting up (pages 349–351)

1. Use your usual plan view, and pick **Framing Set - Working** in the **Layer Display** side window.
2. **Edit › Default Settings…** › **Framing** has the wall settings: stud spacing, top plates, corner style,
   blocking, and whether wall studs are lumber or steel. Under **Edit › Default Settings…** › **Framing** › **Member styles in plan**, click
   **Add Member Style**, pick **Stud**, tick **Fill** and choose white.

## Building wall framing (page 351)

1. **Build › Framing › Build Wall Framing (this floor)**, on each floor. **Build All Framing** also does floors,
   ceilings and roofs.
2. Header labels: add another member style for **Header** with the label `{plies}-{nominal_size} HDR`. Each header
   in plan then reads like `2-2x10 HDR`. Use the 3D view for the plates and sills.

## Editing studs (pages 351–353)

1. Zoom in on the lower-left corner of the Porch. Select the upright corner stud and drag it up out of the corner.
2. Select the flat corner stud. Edit toolbar **Multiple Copy…**: **Y distance** `1 1/2"`, **Number of copies** `3`.
3. Open the stud you moved: **Width** `3 1/2"`.
4. With the stud selected, **Edit › Point to Point Move**: click a point on the selection (it snaps to corners and ends), then the point it should land on.

Any member you move or edit by hand is kept when framing is rebuilt. Chief needs **Retain Wall Framing** for that;
HouseCraft always keeps hand edits.

## Stud spacing (pages 353–354)

1. *Not in HouseCraft yet:* stud spacing per wall type. **Edit › Default Settings…** › **Framing** › **Stud
   spacing** `24"` changes it for the whole plan.
2. **Build Wall Framing** again. The walls you didn't edit rebuild at the new spacing. Your Porch corner is untouched.
3. Set the spacing back to `16"`. Select the basement walls (Shift+click each), then
   **Build › Framing › Rebuild Framing of Selected Walls, Rooms or Roof Planes**.

## Wall detail (pages 355–357)

1. Select the Kitchen window wall, then **Build › Framing › Wall Framing Detail**. It's a drawing of the wall's
   framing, seen from outside.
2. Rename it `Kitchen Window Wall` in the Project Browser.
3. *Not in HouseCraft yet:* live wall detail views where you edit framing directly. A HouseCraft detail is a picture;
   make it again after you change the framing.
4. **Blocking:** in plan, **Build › Framing › Blocking**. Click either end of the wall and the side of the first
   window: a row of blocking half-way up. Repeat between the windows and at the other end. Then make the detail again
   to see it.

## Dimensioning the detail (pages 357–362)

Draw **End to End** dimensions in the detail for the wall's height and length and the window openings. Select one
and use **Edit › Add Extension Line** / **Remove Extension Line**, or drag an extension line's handle. *Not in
HouseCraft yet:* a per-view default set for details, a secondary (feet) label, and a running dimension style.

## Revision (page 362)

**File › Make a Copy…** `Chic Cottage-Wall Framing`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Live, editable wall details named from the wall's label | FR-37 |
| Stud spacing per wall type | FR-39 |
| Detail default sets; framing locate | DIM-29, DIM-30 |
| Secondary dimension format | DIM-12 |

**Fixed while following this tutorial:**

- Editing a framing member in its dialog now counts as a hand edit, so rebuilding framing keeps it. Before, only moved
  members were kept.
- There was no way to draw blocking by hand. **Build › Framing › Blocking** is new.
