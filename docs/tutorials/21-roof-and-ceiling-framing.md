# Tutorial 21, Roof and Ceiling Framing: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Framing Tutorials › Roof and Ceiling Framing**
(pages 364–381). It carries on from [Tutorial 20](20-wall-framing.md). The steps also run as the automatic test
[21-roof-and-ceiling-framing.json](21-roof-and-ceiling-framing.json).

The Chic Cottage roof was built from Floor 1 in Tutorial 8 (story and a half), so its roof planes and rafters belong
to **Floor 1**. Floor 2 has the ceiling joists.

## Roof framing defaults (pages 365–367)

1. Pick **Framing Plan** from the plan view list on the top toolbar.
2. **Edit › Default Settings…** › **Framing**: **Rafter spacing** `16"`, **Rafter depth** `11 1/4"`. *Not in
   HouseCraft yet:* a ridge board size. The ridge is as thick as the rafters and 2" deeper.
3. **Build › Framing › Build Roof Framing**.

Chief keeps the old rafter depth until you edit the roof planes. HouseCraft frames every plane with the current
defaults, so the rafters are 11 1/4" straight away.

## One roof plane (pages 368–370)

Select the roof plane over the Deck and open it: **Rafter Spacing** `24"`. With the plane still selected, choose
**Build › Framing › Rebuild Framing of Selected Walls, Rooms or Roof Planes**. *Not in HouseCraft yet:* a different
rafter depth for one plane, and choosing which height stays put when it changes.

## Posts under the ridge (pages 370–373)

1. Go to Floor 2. **Build › Framing › Post**: click in the wall right of the stairwell, under the ridge. Open it:
   `3 1/2"` × `3 1/2"`.
2. *Not in HouseCraft yet:* Add Break on the ridge board, so it can't be split over the post.
3. Select the post, **Ctrl+C**. Go down to Floor 1 and choose **Edit › Paste Hold Position**. Open the copy and set
   its top end down to the underside of Floor 2's joists (108 3/8").
4. Do the same on the Foundation floor, with its top at -3/4" (under Floor 1's joists) and its bottom on the slab.
5. Check it with **3D › Elevations and Sections › Cross Section / Elevation Tool** across the posts.

## Ceiling framing (pages 374–376)

1. *Not in HouseCraft yet:* the Bearing Line. Ceiling joists bear on walls ticked **Bearing Wall**.
2. **Joist Direction** on Floor 2: a horizontal line in the lower part, a vertical one in the upper part. Open each and
   tick **Ceiling** so they steer the ceiling joists.
3. Go to Floor 2 and choose **Build › Framing › Build Floor and Ceiling Framing (this floor)**.

## Shifting the ceiling joists (pages 376–377)

1. **View › Reference Floor Display** (F9) shows the floor below faintly. *Not in HouseCraft yet:* a separate layer
   set for the reference display.
2. Select the ceiling joists (Shift+click, or drag a box; *not in HouseCraft yet:* a box that only picks joists).
   Edit toolbar **Move / Copy / Rotate / Reflect…** › **Move**: **X distance** `-3/4"`. Shifted joists are hand
   edits, so rebuilding keeps them.

## Labels (pages 377–378)

*Not in HouseCraft yet:* fill colours and labels such as `%nominal_size% VALLEY RAFTER` on framing members.

## Revision and folders (pages 378–379)

*Not in HouseCraft yet:* Make a Copy and Project Browser folders. Use **File › Save As…**
`Chic Cottage-Roof Framing`.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Ridge size; rafter depth per roof plane; height reference | FR-40 |
| Add Break on a framing member | FR-36 |
| Bearing Line | FR-33 |
| Framing fill and labels | FR-38 |
| Select by type with a tool's marquee | UI-38 |
| Reference display layer set | LY-07 |
| Make a Copy; folders | PRJ-45, PRJ-09 |
