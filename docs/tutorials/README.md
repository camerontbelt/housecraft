# Chief Architect tutorials in HouseCraft

Chief Architect's Tutorial Guide (X18) teaches the program through one house, the Chic Cottage. Each tutorial here
has two files:

- **`NN-name.md`**: a guide to keep beside Chief's tutorial. It says what to click in HouseCraft where that differs,
  and what HouseCraft can't do yet (with a way round).
- **`NN-name.json`**: the same steps as HouseCraft commands with checks on the result. The
  `chief_tutorials` test (`crates/housecraft-api/tests/tutorials.rs`) replays every file. Steps HouseCraft can't do
  yet are recorded as gaps and listed when the test runs:

  ```
  cargo test -p housecraft-api --test tutorials -- --nocapture
  ```

  Set `HC_TUTORIAL_OUT` to a folder to also save each tutorial's finished plan there (to open in HouseCraft and
  compare with Chief's pictures).

| # | Tutorial (Chief page) | Steps replayed | Gaps | Guide |
|---|---|---|---|---|
| 1 | Exterior Walls (3) | 15 | 6 | [01-exterior-walls.md](01-exterior-walls.md) |
| 2 | Interior Walls (28) | 18 | 9 | [02-interior-walls.md](02-interior-walls.md) |
| 3 | Multiple Floors (45) | 11 | 7 | [03-multiple-floors.md](03-multiple-floors.md) |
| 4 | Interior Stairs (60) | 13 | 10 | [04-interior-stairs.md](04-interior-stairs.md) |
| 5 | Doors and Windows (82) | 21 | 9 | [05-doors-and-windows.md](05-doors-and-windows.md) |
| 6 | Decks and Porches (105) | 10 | 13 | [06-decks-and-porches.md](06-decks-and-porches.md) |
| 7 | Roof styles: hip, gable, Dutch gable, shed, offset gable, gambrel, gull wing, half hip, mansard (127) | 12 | 3 | [07-roof-styles.md](07-roof-styles.md) |
| 8 | Chic Cottage Roof (136) | 12 | 7 | [08-chic-cottage-roof.md](08-chic-cottage-roof.md) |
| 9 | Dormers (159) | 10 | 7 | [09-dormers.md](09-dormers.md) |
| 10 | Custom Ceilings (177) | 6 | 6 | [10-custom-ceilings.md](10-custom-ceilings.md) |
| 11 | Finish Materials (191) | 10 | 5 | [11-finish-materials.md](11-finish-materials.md) |
| 12 | Room Moldings (211) | 6 | 3 | [12-room-moldings.md](12-room-moldings.md) |
| 13 | Interior Furnishings (222) | 9 | 6 | [13-interior-furnishings.md](13-interior-furnishings.md) |
| 14 | Cabinet Styles (237) | 8 | 4 | [14-cabinet-styles.md](14-cabinet-styles.md) |
| 15 | Cabinet Layout (260) | 7 | 4 | [15-cabinet-layout.md](15-cabinet-layout.md) |
| 16 | Appliances and Fixtures (277) | 10 | 6 | [16-appliances-and-fixtures.md](16-appliances-and-fixtures.md) |
| 17 | Light Fixtures (298) | 18 | 11 | [17-light-fixtures.md](17-light-fixtures.md) |
| 18 | Electrical Objects (317) | 14 | 10 | [18-electrical-objects.md](18-electrical-objects.md) |
| 19 | Floor Framing (331) | 17 | 10 | [19-floor-framing.md](19-floor-framing.md) |
| 20 | Wall Framing (349) | 11 | 9 | [20-wall-framing.md](20-wall-framing.md) |
| 21 | Roof and Ceiling Framing (364) | 11 | 7 | [21-roof-and-ceiling-framing.md](21-roof-and-ceiling-framing.md) |
| 22 | Plot Plans (382) | 10 | 6 | [22-plot-plans.md](22-plot-plans.md) |
| 23 | Terrain Elevation (403) | 11 | 8 | [23-terrain-elevation.md](23-terrain-elevation.md) |
| 24 | Driveways, Sidewalks, and Roads (420) | 6 | 7 | [24-driveways-sidewalks-roads.md](24-driveways-sidewalks-roads.md) |
| 25 | Landscaping Design (433) | 13 | 9 | [25-landscaping-design.md](25-landscaping-design.md) |
| 26 | Layout Page Templates (451) | 7 | 4 | [26-layout-page-templates.md](26-layout-page-templates.md) |
| 27 | Title Blocks and Borders (460) | 10 | 6 | [27-title-blocks-and-borders.md](27-title-blocks-and-borders.md) |
| 28 | Sending Views to Layout (483) | 9 | 7 | [28-sending-views-to-layout.md](28-sending-views-to-layout.md) |

All 28 tutorials in the guide are covered: 315 steps replayed and 199 gaps, each gap with its FEATURES.md row.

The tutorials build on each other. A JSON file can name the one before it in `include`; its steps run first and
the new file carries on from that plan (tutorial 2 starts a fresh plan with tutorial 1's shell in a few commands).
