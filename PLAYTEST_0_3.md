# Playtest script — 0.3 (combat UX, controls, polish)

These scenarios need a human with a keyboard and mouse. Everything below was
built and checked by automated runs (see PROGRESS.md), but the *feel* has not
been judged by a person yet. Note anything that feels wrong, slow, unclear or
unfair; that is the point.

## Setup

```bash
cargo run --release
```

To keep your real profile untouched, start with a scratch profile instead:

```bash
DUNGEON_SAVE_DIR=./saves_playtest cargo run --release
```

In the hub: the right-hand panel has tabs (click them or press 1-8, P hides the
panel). **Party** tab: *Change class*, *Party (5)* / *Raid (10)*, recruit and AI
profile buttons. **Dungeons** tab: pick dungeon and difficulty. Walk into the
glowing portal and press **E**. Hover anything for a tooltip.

Controls to try everywhere: W/S move, A/D turn, Q/E strafe, right-drag steer,
left-drag look around, both buttons run, wheel zoom, V toggles camera
auto-follow, Tab/click target, T assist, G kill target, 1-5 abilities, F1-F5
party members, Esc clear target, Backspace leave.

For each scenario, write down: what you expected, what happened, and how it
felt (1-5).

## 1. Camera in corridors and combat (any class, The Sunken Vault, Normal)

1. Walk the first corridor with W only, turning with A/D. The camera should
   swing in behind you smoothly (auto-follow). Press **V** to turn it off and
   repeat: the camera should now stay where you left it.
2. Hold the right button and move the mouse while walking: the character turns
   with the camera. Hold the left button and move the mouse: only the camera
   moves; when you release, the character still faces its old direction.
3. Hold both buttons: you run forward and can steer with the mouse.
4. Back into a wall and rotate the camera around yourself; zoom out fully next
   to a pillar. The camera must never show the inside of a wall and must not
   jitter or pump in and out.
5. Start casting a cast-time spell (Mender's Mend or Arcanist's Arc Bolt) and
   then press W: the cast must be cancelled. Rotating the camera (either button)
   must *not* cancel it.
6. Click an enemy (short click, no drag): it becomes your target. Drag-look
   starting on an enemy must not target it.

Check: does auto-follow ever fight you? Is 1.2 s after manual input the right
delay? Is the turn speed (A/D) comfortable?

## 2. Ranged DPS (Arcanist, The Gloomroot Warrens, Normal, level 4+)

1. Tab to an enemy far away: slot 1 (Arc Bolt) should be red with "Out of
   range". Walk closer: it turns normal exactly when a cast is allowed.
2. Cast while the GCD runs: the thin bar along the top of each slot is the GCD,
   the dark curtain is the ability's own cooldown (Flare, 6 s). Are they easy to
   tell apart?
3. Mash 1 while out of range: one message "Arc Bolt: Out of range", not a
   flickering stream.
4. Watch your threat on the target frame ("Threat 45% of pull") and the bar. Burst
   until it turns yellow (≥ 80%), then use Veil (3) and watch it drop.
5. Interrupt a Gloomroot healer-caster with Silence Pulse (5) when its plate says
   "(kick!)".

## 3. Tanking packs and losing aggro (Warden, The Sunken Vault, Normal)

1. Pull a skulker pack with Challenge (2) and hold it with Shield Slam (1) and
   Quake Stomp (5). Nameplates should be green ("secure").
2. Set the companions' AI to **Aggressive** in the Party tab beforehand so the
   DPS push threat. Plates should turn yellow when someone gets close (target
   frame: "CONTESTED - 85% of the way to a pull").
3. When an enemy turns on a DPS: its plate turns red and one alert appears
   ("Aggro lost: ... - taunt!"). Taunt it with Challenge: "Taunted ..." appears
   and the plate turns green immediately.
4. Rage: the readout should explain where rage comes from; slots show "need 15
   rage" in blue when short.

Check: are the colours readable at a glance in a 4-mob pack? Are alerts too
frequent or too rare?

## 4. Healing with clickable frames (Mender, The Sunken Vault, Normal)

1. Click a party frame: it gets a gold border and becomes your target. Click the
   world behind the frames: nothing behind the panel is selected.
2. With that ally selected press 1 (Mend): it heals the selected ally.
3. F1-F5 still select party members.
4. Click an ally's frame while they're out of range: Mend shows "Out of range".
5. Purify (5) shows "nothing to dispel" until someone has a dispellable debuff
   (an orange status line with a `*` on the frame — Mother Brakkis's poison in
   the Warrens is a good test).
6. Mana: the readout shows "five-second rule: full regen in 3.2s" after each
   cast; stop casting and watch the regen rate go up.

## 5. Mouseover healing while an enemy is targeted (Mender)

1. Target an enemy (Tab). Hover the tank's party frame (blue border; blue ring
   under the tank) and press 1: the tank is healed, your target stays on the
   enemy.
2. Hover the tank's 3D character instead of the frame and press 2 (Renew): same
   result.
3. Hover nobody and press 1 with an enemy targeted: you heal yourself.
4. Hover an ally and press 3 (Smite): it still hits your enemy target (hostile
   spells ignore mouseover).
5. Hover the target-of-target frame (top, right of the target frame) while the
   boss is attacking the tank and press 1: the tank is healed.

Check: is mouseover healing faster than click-then-cast? Any surprises about who
got healed?

## 6. Melee energy (Striker, The Sunken Vault, Normal)

1. Spend energy down to 0: every slot should show "need 40 energy - 4.0s" (time
   until affordable) in blue, even while the GCD is running.
2. The readout shows "+10/s, full in 9.9s".
3. Attack from behind the brute to avoid parries; Kick (4) an interruptible
   cast; Evade (3) when the plate goes yellow/red.

## 7. Boss mechanics, GCDs, interrupts and cooldowns (any class, Warrens or Ashen Spire)

1. Fight a boss with interruptible casts; watch the target frame's cast line and
   the "(kick!)" plate.
2. Stand in a ground warning: the red "MOVE OUT!" border must be obvious; step
   out.
3. Use every ability once and watch each slot's cooldown curtain and seconds.
4. Hover each ability for its tooltip: are the numbers and rules understandable?
5. Hover a debuff line on a party frame or your own aura row above the resource
   bar: duration, stacks and dispellability should be shown.

## 8. 10-person raid (any class, Party tab → Raid (10), The Cinder Court or Raid (10) difficulty)

1. Ten compact frames in two columns; click and mouseover work the same.
2. As a healer: is the frame size enough to see who's low? Are debuffs on
   compact frames (first debuff only) enough?
3. As a tank: Twinmaw needs tank swaps; plates show blue ("held by another
   tank") when the other tank has it.
4. Is anything overlapping at 1280×760 (log, frames, action bar, tooltip)?

## Reporting

For each scenario: pass/fail, a 1-5 feel score, and notes. Screenshots help.
