# Playtest script — 0.4 navigation (Cavern Traverse)

The navigation, the new dungeon and the AI's movement were verified by
automated runs (see NAVIGATION_0_4.md). These scenarios are for a person:
does it *feel* like a real three-dimensional place, and do the companions
look competent while getting through it?

## Setup

```bash
DUNGEON_SAVE_DIR=./saves_playtest cargo run --release
```

The scratch save folder keeps your real profile untouched. In the hub:
1. Open the **Dungeons** tab and select **Cavern Traverse** (level 2+).
2. Walk into the portal and press **E**.

An all-AI party at level 1 clears it in 97-100% of headless runs for every
player class. With a human in the player slot this is untested.

**N** toggles the navigation overlay, which shows:
* surfaces coloured by height, with names and elevations;
* green doorways (portals) and orange drop arrows;
* each companion's planned path (thin line), next waypoint (dot) and goal (ring);
* red crosses for unreachable goals and red rings for stuck companions.

**Shift+N** adds walls and the "walk" rectangles. Click a companion's party
frame to select them; the selected actor's path is drawn white.

To watch the AI do it all without playing:

```bash
cargo run --release -- --demo --dungeon cavern_traverse --level 2 --nav-debug --zoom 30
```

`--group 10` runs it as a raid group.

For each scenario write down what you expected, what happened, and a 1-5
score. Screenshots of anything odd help a lot.

## 1. Walking the layout yourself

1. From the entrance, follow the winding passage (three legs). Corners should
   feel tight but never snag you.
2. In the Fork Hall, check the dead-end alcove to the west.
3. Walk up the **Ash Ramp** onto the **High Walkway** (5 m up). Your
   character should rise smoothly, with no pop at the top or bottom.
4. Cross the **Rope Bridge** east. Look down: the **Underpass** runs beneath
   you. You can't fall off the sides; the edges hold you.
5. Continue onto the **Hanging Gallery**, which runs directly above the
   underpass, and walk to its north end. The orange strip is the **drop**: walk
   off it and fall 8 m into the **Magma Basin**. The fall should take a
   moment, not teleport you.
6. Try to get back up: the only way is round through the stairs, the
   underpass, the lava hollow, the tunnel and the ramp. The drop is one-way.
7. Walk the **Narrow Tunnel**, circle the **lava pit** both ways, and step on the
   **Fire Vent** while it's lit (it hurts).
8. Look for the **Sniper Perch** beside the basin. You can't get on it.

Check the camera throughout:
* it follows you up and down;
* it doesn't clip through the bridge or gallery when you're under them;
* it dips lower instead of collapsing into your character;
* RMB steering, LMB free look and auto-follow (V) still behave as in 0.3.

## 2. Fighting on different levels

1. Fight the **Bridge Skulkers** on the rope bridge. Do melee and the tank find
   room on a 4 m bridge?
2. Stand on the bridge right above the **Underpass Lurker** (it waits under the
   bridge). It must **not** notice you. Then go down into the underpass and pull
   it.
3. From the bridge, try to cast at the lurker below. You should get **"Target
   not in line of sight"**, and the action bar should say "no line of sight".
   Melee can't reach it through the deck either.
4. In the lava hollow, put the lava pit's wall between you and an enemy. Casts
   should be blocked until you step round the corner.
5. As a healer (Mender), let a companion go round a corner and watch Mend
   refuse until you can see them.
6. Boss in the basin, phase 2: Magma Rings should land on the basin floor
   only.

## 3. Watching the AI (overlay on)

1. **Tank leading through the winding passage**: smooth lines, no zig-zag
   between portals, no hugging corners until stuck.
2. **Healer following up the ramp and through the narrow tunnel**: stays in
   range and in sight of the tank.
3. **Ranged DPS on elevated positions**: when the fight is on the bridge,
   ranged find spots that can see the target *and* are visible to the healer.
4. **Melee reaching enemies**: on the bridge and in the hollow they reach
   valid spots and never stand under an enemy that's on another floor.
5. **No oscillation**: nobody should twitch back and forth between two spots.
   A companion doing that for more than a second is a bug; screenshot it with
   the overlay on.
6. **Stuck**: a red ring around a companion means it was stuck and is
   recovering. Note where it happens.
7. **Raid group (10)**: Party tab → Raid (10), then run the cavern again. Watch
   the crowd at the ramp foot, the bridge and the stairs.

## 4. Old dungeons still feel the same

Play one pull in the Sunken Vault and one in the Gloomroot Warrens. Movement,
targeting and the action bar should feel unchanged.

The one intended difference is line of sight: walls now block casts and heals
around corners.

## Reporting

For each scenario give pass/fail, a score (1-5) and notes. For AI problems,
say where (area name from the overlay), which companion, and what their path
line was doing.
