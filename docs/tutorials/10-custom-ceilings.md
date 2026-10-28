# Tutorial 10, Custom Ceilings: following it in HouseCraft

Keep this open beside **Chief Architect Tutorial Guide › Interior Design Tutorials › Custom Ceilings**
(pages 177–190). It carries on from [Tutorial 8](08-chic-cottage-roof.md), not the dormer option. The steps also run
as the automatic test [10-custom-ceilings.json](10-custom-ceilings.json).

## Where ceilings are set

Double-click a room. Under **Ceiling** choose **Flat**, **Vaulted** (follows the roof above: Chief's cathedral
ceiling), **Tray**, **Coffered**, **Barrel** or **None**. The room's **Ceiling Height** sets how high it is.

## Ceiling finishes (pages 178–183)

Ceiling finish layers hang below a room's ceiling height, so they lower the finished ceiling. Type them top layer
first, each as a name and a thickness, separated by semicolons, such as `Drywall 1/2"; Paint 1/32"`.

1. Plan default: **Edit › Default Settings…** › **Ceiling Finish**: `Drywall 1/2"; Paint 1/32"`.
2. Garage: open the Garage room type (Project Browser › Room Types) and set its **Ceiling Finish** to
   `Rigid insulation 1 1/2"; Fire-rated drywall 5/8"`.
3. **Tools › Plan Materials…**: make a new material `Hat Channel`, grey. For the framing, **Edit › Default
   Settings…** › **Framing** › **Ceiling joists**: **Hat Channel**. Ceiling joists built from then on are steel hat
   channel.
4. **Lowered basement ceiling:** open the basement room on the Foundation floor. In **Ceiling Finish** type
   `Hat Channel 1"; Drywall 1/2"; Paint 1/32"`. The ceiling drops by the hat channel. The dialog shows where the
   finished ceiling now sits.
5. To take the finish off a room, click **Remove Finish** in its dialog (**Use Room Type's** puts it back).

## Cathedral ceiling (pages 183–184)

Open the deck and tick **Vaulted**: the flat ceiling under its pitched roof goes, and you see the underside of
the roof. On a room, **Ceiling: Vaulted** does the same under the house roof.

## Tray and coffered ceilings (pages 184–188)

*Not in HouseCraft yet:* the **Soffit** tool. Use the basement room's ceiling style instead:

1. Open the basement room: **Ceiling: Tray**, inset `36"`, rise `12"`. That's the 36" × 12" soffits round the room.
2. For the coffered version: **Ceiling: Coffered**, beams `36"` wide, `12"` deep, every `6'`.

## Annotations (pages 188–189)

**Text** tool: `36" X 12" SOFFITS` in the basement, `2:12 CATHEDRAL CEILING` by the deck.

## What this tutorial showed HouseCraft still needs

| Missing | FEATURES row |
|---|---|
| Soffit tool | CB-26 |
