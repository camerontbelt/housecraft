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

| # | Tutorial (Chief page) | Steps replayed | Gaps | Guide |
|---|---|---|---|---|
| 1 | Exterior Walls (3) | 14 | 7 | [01-exterior-walls.md](01-exterior-walls.md) |
| 2 | Interior Walls (28) | 16 | 11 | [02-interior-walls.md](02-interior-walls.md) |
| 3 | Multiple Floors (45) | 10 | 8 | [03-multiple-floors.md](03-multiple-floors.md) |
| 4 | Interior Stairs (60) | — | — | |
| 5 | Doors and Windows (82) | — | — | |
| 6 | Decks and Porches (105) | — | — | |
| 7–19 | Roof, Interior Design and Kitchen & Bath tutorials (127–330) | — | — | |
| 20–22 | Framing tutorials (331–381) | — | — | |
| 23–26 | Landscaping tutorials (382–450) | — | — | |
| 27–29 | Layout tutorials (451–500) | — | — | |

The tutorials build on each other. A JSON file can name the one before it in `include`; its steps run first and
the new file carries on from that plan (tutorial 2 starts a fresh plan with tutorial 1's shell in a few commands).
