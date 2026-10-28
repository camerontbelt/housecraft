# Tutorial 21, Roof and Ceiling Framing: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Framing Tutorials › Roof and Ceiling Framing**
(pages 364–381). It carries on from [Tutorial 20](20-wall-framing.md). The steps also run as the automatic test
[21-roof-and-ceiling-framing.json](21-roof-and-ceiling-framing.json).

The Chic Cottage roof was built from Floor 1 in Tutorial 8 (story and a half), so its roof planes and rafters belong
to **Floor 1**. Floor 2 has the ceiling joists.

## Roof framing defaults (pages 365–367)

1. Pick **Framing Plan** from the plan view list on the top toolbar.
2. **Edit › Default Settings…** › **Framing**: **Rafter spacing** `16"`, **Rafter depth** `11 1/4"`. Set
   **Ridge thickness** `3 1/2"` and **Ridge depth** `15 1/4"`.
3. **Build › Framing › Build Roof Framing**.

Chief keeps the old rafter depth until you edit the roof planes. HouseCraft frames every plane with the current
defaults, so the rafters are 11 1/4" straight away.

## One roof plane (pages 368–370)

Select the roof plane over the Deck and open it: **Rafter Spacing** `24"`, **Rafter Depth** `11 1/4"`. With the
plane still selected, choose **Build › Framing › Rebuild Framing of Selected Walls, Rooms or Roof Planes**. The
rafters hang from the roof surface, so their tops stay put and the deeper rafters reach further down. (HouseCraft's
covered deck has its roof as part of the deck, so the tutorial's scenario uses the back roof plane beside it.)

## Posts under the ridge (pages 370–373)

1. Go to Floor 2. **Build › Framing › Post**: click in the wall right of the stairwell, under the ridge. Open it:
   `3 1/2"` × `3 1/2"`.
2. Break the ridge board over the post: pick **Build › Wall › Break Wall** with the framing showing and click the
   ridge right above the post.
3. Select the post, **Ctrl+C**. Go down to Floor 1 and choose **Edit › Paste Hold Position**. Open the copy and set
   its top end down to the underside of Floor 2's joists (108 3/8").
4. Do the same on the Foundation floor, with its top at -3/4" (under Floor 1's joists) and its bottom on the slab.
5. Check it with **3D › Elevations and Sections › Cross Section / Elevation Tool** across the posts.

## Ceiling framing (pages 374–376)

1. **Build › Framing › Bearing Line**: click both ends of a line across the ceiling joists where they should
   splice. Ceiling joists bear on it as on a wall ticked **Bearing Wall**.
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

Add a member style (**Edit › Default Settings…** › **Framing** › **Member styles in plan**) for **Valley Rafter**: tick **Fill**, pick grey, and type the label
`{nominal_size} VALLEY RAFTER`. Roof framing builds hip and valley rafters wherever two planes meet on a slope.

## Revision and folders (pages 378–379)

**File › Make a Copy…** `Chic Cottage-Roof Framing`. Make a `Framing Tutorials` folder under
**Projects** (right-click the project › **New Folder…**) and drag the framing revisions into it.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Select by type with a tool's marquee | UI-38 |
| Reference display layer set | LY-07 |
