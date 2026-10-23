//! "What did my last button press do?" (0.9.2): turns the simulation's own
//! events into short lines about the player's abilities, shown above the
//! action bar. Every number comes from an event or an ability definition;
//! nothing is estimated. AoE hits are grouped into one line, auto-attacks and
//! over-time ticks are left to the floating text, and companions only appear
//! when they interrupt something.

use dungeon_sim::Role;
use dungeon_sim::class::Effect;
use dungeon_sim::metrics::ActorMetrics;
use dungeon_sim::sim::{ActorId, AuraKind};
use dungeon_sim::stats::HitOutcome;
use dungeon_sim::{Event, EventKind, Label, Sim};

/// How a line is coloured, in order of importance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Your interrupt landed.
    Interrupt,
    Crit,
    Hit,
    Heal,
    /// Shields, defensives, taunts, crowd control, dispels.
    Utility,
    /// Missed, dodged, nothing to interrupt, crowd control broken.
    Failed,
    /// A companion's interrupt.
    Companion,
    /// Lab B (0.10): an opening appeared on your target.
    Opening,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeedLine {
    pub text: String,
    pub tone: Tone,
}

/// The little state the feed needs between frames.
#[derive(Debug, Default)]
pub struct CombatFeed {
    /// The enemy your crowd control is on, to report when it breaks (the
    /// event doesn't say whose crowd control it was).
    cc_on: Option<ActorId>,
    /// When each enemy last had an opening line (at most one per enemy
    /// every `OPENING_LINE_SECS`, 0.11).
    opening_lines: Vec<(ActorId, f32)>,
}

/// Seconds between two opening lines about the same enemy.
const OPENING_LINE_SECS: f32 = 2.0;

enum Pending {
    Damage {
        what: Label,
        time: f32,
        hits: Vec<(ActorId, f32, f32, HitOutcome)>,
    },
    Heal {
        what: Label,
        time: f32,
        hits: Vec<(ActorId, f32, f32, bool)>,
    },
    Line(FeedLine),
}

/// The effect of one of the player's abilities, found by its label.
pub fn own_effect(sim: &Sim, p: ActorId, what: &str) -> Option<Effect> {
    let a = &sim.actors[p];
    a.ability_labels
        .iter()
        .position(|l| &**l == what)
        .and_then(|i| a.abilities.get(i))
        .map(|id| id.def().effect)
}

/// The label of the player's first ability whose effect matches.
fn own_label(sim: &Sim, p: ActorId, pick: impl Fn(&Effect) -> bool) -> Option<Label> {
    let a = &sim.actors[p];
    a.abilities
        .iter()
        .position(|id| pick(&id.def().effect))
        .and_then(|i| a.ability_labels.get(i).cloned())
}

fn name(sim: &Sim, p: ActorId, id: ActorId) -> String {
    if id == p {
        "you".to_string()
    } else {
        sim.actors[id].name.clone()
    }
}

fn avoided_word(o: HitOutcome) -> &'static str {
    match o {
        HitOutcome::Miss => "missed",
        HitOutcome::Dodge => "dodged",
        HitOutcome::Parry => "parried",
        HitOutcome::Resist => "resisted",
        _ => "avoided",
    }
}

fn secs(s: f32) -> String {
    format!("{s:.0} s")
}

impl CombatFeed {
    /// Lines for one batch of new events. `interrupt_attempts` is how many
    /// interrupts the player used in the batch (from the metrics): an
    /// interrupt without damage leaves no event when there is nothing to stop.
    pub fn lines(&mut self, sim: &Sim, events: &[Event], interrupt_attempts: u32) -> Vec<FeedLine> {
        let Some(p) = sim.player else {
            return Vec::new();
        };
        let interrupt_label = own_label(sim, p, |e| matches!(e, Effect::Interrupt { .. }));
        // Targets you interrupted in this batch (the event names the spell
        // stopped, not your ability).
        let interrupted: Vec<ActorId> = events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::Interrupt { src, dst, .. } if *src == p => Some(*dst),
                _ => None,
            })
            .collect();
        let mut misses = interrupt_attempts.saturating_sub(interrupted.len() as u32);
        let mut out: Vec<Pending> = Vec::new();
        let mut broken: Option<ActorId> = None;
        // Lab A (0.10): Edge after the last gain in this batch, and the line
        // that earned it.
        let mut edge: Option<(usize, u32)> = None;
        // The last line that came from your own hit or interrupt.
        let mut last_own: Option<usize> = None;
        for e in events {
            match &e.kind {
                EventKind::Damage {
                    src,
                    dst,
                    what,
                    amount,
                    absorbed,
                    outcome,
                    ..
                } => {
                    if broken == Some(*dst) {
                        // The hit that broke your crowd control.
                        broken = None;
                        let by = if *src == p {
                            "you".to_string()
                        } else {
                            sim.actors[*src].name.clone()
                        };
                        if let Some(Pending::Line(l)) = out.last_mut() {
                            l.text.push_str(&format!(" (by {by})"));
                        }
                    }
                    if *src != p {
                        continue;
                    }
                    match own_effect(sim, p, what) {
                        None | Some(Effect::Dot { .. }) => continue,
                        Some(Effect::Interrupt { .. }) if interrupted.contains(dst) => continue,
                        _ => {}
                    }
                    let hit = (*dst, *amount, *absorbed, *outcome);
                    match out.iter_mut().rev().find(|x| {
                        matches!(x, Pending::Damage { what: w, time, .. }
                            if w == what && *time == e.time)
                    }) {
                        Some(Pending::Damage { hits, .. }) => hits.push(hit),
                        _ => out.push(Pending::Damage {
                            what: what.clone(),
                            time: e.time,
                            hits: vec![hit],
                        }),
                    }
                    last_own = out.iter().rposition(|x| {
                        matches!(x, Pending::Damage { what: w, time, .. }
                            if w == what && *time == e.time)
                    });
                }
                EventKind::Heal {
                    src,
                    dst,
                    what,
                    amount,
                    overheal,
                    crit,
                } => {
                    if *src != p {
                        continue;
                    }
                    match own_effect(sim, p, what) {
                        None | Some(Effect::Hot { .. }) => continue,
                        _ => {}
                    }
                    let hit = (*dst, *amount, *overheal, *crit);
                    match out.iter_mut().rev().find(|x| {
                        matches!(x, Pending::Heal { what: w, time, .. }
                            if w == what && *time == e.time)
                    }) {
                        Some(Pending::Heal { hits, .. }) => hits.push(hit),
                        _ => out.push(Pending::Heal {
                            what: what.clone(),
                            time: e.time,
                            hits: vec![hit],
                        }),
                    }
                }
                EventKind::AuraApplied {
                    src,
                    dst,
                    what,
                    stacks,
                } if *src == p && *dst == p && &**what == "Edge" => {
                    // On the line that earned it: your last hit or interrupt.
                    edge = Some((last_own.unwrap_or(out.len().saturating_sub(1)), *stacks));
                }
                EventKind::AuraApplied { src, dst, what, .. }
                    if *src == p
                        && sim.actors[*dst].is_enemy()
                        && sim.actors[*dst].auras.iter().any(|a| {
                            a.kind == AuraKind::Opening && a.source == p && a.name == *what
                        }) =>
                {
                    // Openings: only on your target, and at most one line per
                    // enemy every 2 s, to keep the feed calm.
                    let recent = self
                        .opening_lines
                        .iter()
                        .any(|&(id, t)| id == *dst && e.time - t < OPENING_LINE_SECS);
                    if sim.actors[p].target == Some(*dst) && !recent {
                        self.opening_lines.retain(|&(id, _)| id != *dst);
                        self.opening_lines.push((*dst, e.time));
                        let take =
                            own_label(sim, p, |x| matches!(x, Effect::ExploitOpening { .. }))
                                .unwrap_or_else(|| Label::from("Exploit"));
                        out.push(Pending::Line(FeedLine {
                            text: format!(
                                "Opening: {} {} - {take}!",
                                sim.actors[*dst].name,
                                opening_word(what)
                            ),
                            tone: Tone::Opening,
                        }));
                    }
                }
                EventKind::AuraApplied {
                    src,
                    dst,
                    what,
                    stacks,
                } if *src == p => {
                    let target = name(sim, p, *dst);
                    let line = match own_effect(sim, p, what) {
                        Some(Effect::EdgeBleed { duration, .. }) => {
                            let total = sim.actors[*dst]
                                .auras
                                .iter()
                                .find(|a| a.source == p && a.name == *what)
                                .map_or(0.0, |a| match a.kind {
                                    AuraKind::Dot { per_tick, .. } => per_tick * duration.max(1.0),
                                    _ => 0.0,
                                });
                            Some((
                                format!(
                                    "{what} > {target}: bleeding {total:.0} over {}",
                                    secs(duration)
                                ),
                                Tone::Hit,
                            ))
                        }
                        Some(Effect::Hot { .. }) => {
                            Some((format!("{what} > {target}: heal over time"), Tone::Heal))
                        }
                        Some(Effect::Dot { ignores_armor, .. }) => {
                            let kind = if ignores_armor {
                                "bleeding"
                            } else {
                                "damage over time"
                            };
                            let x = if *stacks > 1 {
                                format!(" x{stacks}")
                            } else {
                                String::new()
                            };
                            Some((format!("{what} > {target}: {kind}{x}"), Tone::Hit))
                        }
                        Some(Effect::Absorb { .. }) => {
                            Some((format!("{what} > {target}: shielded"), Tone::Utility))
                        }
                        Some(Effect::DamageReduction { pct, duration }) => Some((
                            format!(
                                "{what}: {:.0}% less damage taken for {}",
                                pct * 100.0,
                                secs(duration)
                            ),
                            Tone::Utility,
                        )),
                        Some(Effect::Incapacitate { duration }) => {
                            self.cc_on = Some(*dst);
                            Some((
                                format!("{what} > {target}: incapacitated {}", secs(duration)),
                                Tone::Utility,
                            ))
                        }
                        _ => None,
                    };
                    if let Some((text, tone)) = line {
                        out.push(Pending::Line(FeedLine { text, tone }));
                    }
                }
                EventKind::CcBroken { dst, what } if self.cc_on == Some(*dst) => {
                    self.cc_on = None;
                    broken = Some(*dst);
                    out.push(Pending::Line(FeedLine {
                        text: format!("{what} on {} broken", sim.actors[*dst].name),
                        tone: Tone::Failed,
                    }));
                }
                EventKind::Taunt { src, dst } if *src == p => {
                    let what = own_label(sim, p, |e| matches!(e, Effect::Taunt { .. }))
                        .unwrap_or_else(|| Label::from("Taunt"));
                    out.push(Pending::Line(FeedLine {
                        text: format!("{what} > {}: taunted", sim.actors[*dst].name),
                        tone: Tone::Utility,
                    }));
                }
                EventKind::ThreatDrop { src } if *src == p => {
                    let a = &sim.actors[p];
                    let found = a
                        .abilities
                        .iter()
                        .zip(&a.ability_labels)
                        .find_map(|(id, l)| match id.def().effect {
                            Effect::ThreatDrop { pct } => Some((l.clone(), pct)),
                            _ => None,
                        });
                    if let Some((what, pct)) = found {
                        out.push(Pending::Line(FeedLine {
                            text: format!("{what}: threat reduced {:.0}%", pct * 100.0),
                            tone: Tone::Utility,
                        }));
                    }
                }
                EventKind::Dispel { src, dst, what } if *src == p => {
                    let ability = own_label(sim, p, |e| matches!(e, Effect::Dispel))
                        .unwrap_or_else(|| Label::from("Dispel"));
                    out.push(Pending::Line(FeedLine {
                        text: format!("{ability} > {}: removed {what}", name(sim, p, *dst)),
                        tone: Tone::Utility,
                    }));
                }
                EventKind::Staggered { enemy, what } => {
                    out.push(Pending::Line(FeedLine {
                        text: format!("{what} broken: {} exposed!", sim.actors[*enemy].name),
                        tone: Tone::Interrupt,
                    }));
                }
                EventKind::Interrupt { src, dst, what } => {
                    let enemy = &sim.actors[*dst].name;
                    if *src == p {
                        let ability = interrupt_label
                            .clone()
                            .unwrap_or_else(|| Label::from("Interrupt"));
                        out.push(Pending::Line(FeedLine {
                            text: format!("{ability} > interrupted {enemy}'s {what}"),
                            tone: Tone::Interrupt,
                        }));
                        last_own = Some(out.len() - 1);
                    } else if sim.party.contains(src) {
                        out.push(Pending::Line(FeedLine {
                            text: format!("{} interrupted {enemy}'s {what}", sim.actors[*src].name),
                            tone: Tone::Companion,
                        }));
                    }
                }
                _ => {}
            }
        }
        let mut lines: Vec<FeedLine> = out
            .into_iter()
            .map(|x| match x {
                Pending::Line(l) => l,
                Pending::Damage { what, hits, .. } => {
                    let interrupt =
                        matches!(own_effect(sim, p, &what), Some(Effect::Interrupt { .. }));
                    let mut l = damage_line(sim, p, &what, &hits);
                    if interrupt && misses > 0 {
                        misses -= 1;
                        l.text.push_str(" - nothing to interrupt");
                        l.tone = Tone::Failed;
                    }
                    l
                }
                Pending::Heal { what, hits, .. } => heal_line(sim, p, &what, &hits),
            })
            .collect();
        // Lab A: the Edge you hold now, on the line that earned it.
        if let Some((i, n)) = edge
            && let Some(l) = lines.get_mut(i)
        {
            l.text.push_str(&format!(" [Edge {n}/{}]", sim.edge_cap(p)));
        }
        // Interrupts that dealt no damage and stopped nothing left no event.
        if let Some(what) = interrupt_label {
            for _ in 0..misses {
                lines.push(FeedLine {
                    text: format!("{what}: nothing to interrupt"),
                    tone: Tone::Failed,
                });
            }
        }
        lines
    }
}

/// How an opening reads in a sentence.
fn opening_word(what: &str) -> String {
    match what {
        "Distracted" => "is casting".into(),
        "Recovering" => "is recovering".into(),
        "Off Balance" => "is off balance".into(),
        "Faltering" => "is faltering".into(),
        other => format!("is {}", other.to_lowercase()),
    }
}

fn damage_line(
    sim: &Sim,
    p: ActorId,
    what: &str,
    hits: &[(ActorId, f32, f32, HitOutcome)],
) -> FeedLine {
    let crits = hits.iter().filter(|h| h.3 == HitOutcome::Crit).count();
    let avoided: Vec<HitOutcome> = hits.iter().map(|h| h.3).filter(|o| o.avoided()).collect();
    let total: f32 = hits.iter().map(|h| h.1).sum();
    let absorbed: f32 = hits.iter().map(|h| h.2).sum();
    let tone = if avoided.len() == hits.len() {
        Tone::Failed
    } else if crits > 0 {
        Tone::Crit
    } else {
        Tone::Hit
    };
    let mut text = if let [(dst, amount, _, outcome)] = hits {
        let target = name(sim, p, *dst);
        if outcome.avoided() {
            format!("{what} > {target}: {}", avoided_word(*outcome))
        } else if *outcome == HitOutcome::Crit {
            format!("{what} > {target}: {amount:.0} CRIT")
        } else {
            format!("{what} > {target}: {amount:.0}")
        }
    } else {
        let mut t = format!("{what} > {} enemies: {total:.0}", hits.len());
        if crits > 0 {
            t.push_str(&format!(" ({crits} crit)"));
        }
        if !avoided.is_empty() {
            t.push_str(&format!(" ({} avoided)", avoided.len()));
        }
        t
    };
    if absorbed > 0.0 {
        text.push_str(&format!(" ({absorbed:.0} absorbed)"));
    }
    if hits.len() == 1 && hits[0].3 == HitOutcome::Block {
        text.push_str(" (blocked)");
    }
    FeedLine { text, tone }
}

fn heal_line(sim: &Sim, p: ActorId, what: &str, hits: &[(ActorId, f32, f32, bool)]) -> FeedLine {
    let total: f32 = hits.iter().map(|h| h.1).sum();
    let over: f32 = hits.iter().map(|h| h.2).sum();
    let crit = hits.iter().any(|h| h.3);
    let target = match hits {
        [(dst, ..)] => name(sim, p, *dst),
        _ => format!("{} allies", hits.len()),
    };
    let mut text = format!("{what} > {target}: +{total:.0}");
    if crit {
        text.push_str(" CRIT");
    }
    if over > 0.0 {
        text.push_str(&format!(" ({over:.0} overheal)"));
    }
    FeedLine {
        text,
        tone: Tone::Heal,
    }
}

/// Your numbers for the current or last fight (0.9.2, toggled with K): the
/// difference of your metrics between the pull and now. Not a damage meter.
#[derive(Debug, Clone)]
pub struct Fight {
    pub start: f32,
    pub end: Option<f32>,
    /// Your metrics just before the pull.
    pub base: ActorMetrics,
}

/// Follows the player's fights: `update` once per frame.
#[derive(Debug, Default)]
pub struct FightTracker {
    pub current: Option<Fight>,
    /// Your metrics and the time at the last frame out of combat.
    idle: Option<(f32, ActorMetrics)>,
}

fn thousands(v: f32) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 { format!("-{out}") } else { out }
}

impl FightTracker {
    pub fn update(&mut self, sim: &Sim) {
        let Some(p) = sim.player else {
            return;
        };
        let now = sim.metrics.actors[p].clone();
        let fighting = !sim.engaged_enemies().is_empty();
        match (&mut self.current, fighting) {
            (Some(f), true) if f.end.is_none() => {}
            (_, true) => {
                // A new pull: count from the last frame before it.
                let (start, base) = self.idle.take().unwrap_or((sim.time, now));
                self.current = Some(Fight {
                    start,
                    end: None,
                    base,
                });
            }
            (Some(f), false) if f.end.is_none() => {
                f.end = Some(sim.time);
                self.idle = Some((sim.time, now));
            }
            (_, false) => self.idle = Some((sim.time, now)),
        }
    }

    /// The panel's lines, your role's numbers first.
    pub fn lines(&self, sim: &Sim) -> Vec<String> {
        let (Some(f), Some(p)) = (&self.current, sim.player) else {
            return vec!["No fight yet".into()];
        };
        let m = &sim.metrics.actors[p];
        let b = &f.base;
        let secs = (f.end.unwrap_or(sim.time) - f.start).max(1.0);
        let head = if f.end.is_some() {
            format!("Last fight: {secs:.0} s")
        } else {
            format!("This fight: {secs:.0} s")
        };
        let damage = m.damage_done - b.damage_done;
        let healing = m.healing_done - b.healing_done;
        let over = m.overhealing - b.overhealing;
        let absorbed = m.absorb_done - b.absorb_done;
        let taken = m.damage_taken - b.damage_taken;
        let avoidable = m.avoidable_taken - b.avoidable_taken;
        let ints = m.interrupts - b.interrupts;
        let tried = m.interrupts_attempted - b.interrupts_attempted;
        let deaths = m.deaths - b.deaths;
        let dmg = format!("Damage {} ({:.0} per s)", thousands(damage), damage / secs);
        let heal = {
            let pct = if healing + over > 0.0 {
                100.0 * over / (healing + over)
            } else {
                0.0
            };
            let mut t = format!("Healing {} ({pct:.0}% overheal)", thousands(healing));
            if absorbed > 0.0 {
                t.push_str(&format!("\nShields absorbed {}", thousands(absorbed)));
            }
            t
        };
        let took = if avoidable > 0.0 {
            format!(
                "Taken {} ({} avoidable)",
                thousands(taken),
                thousands(avoidable)
            )
        } else {
            format!("Taken {}", thousands(taken))
        };
        let int = format!("Interrupts {ints}/{tried}");
        let mut out = vec![head];
        match sim.actors[p].role() {
            Some(Role::Tank) => out.extend([took, dmg, int]),
            Some(Role::Healer) => out.extend([heal, took, int]),
            _ => out.extend([dmg, int, took]),
        }
        if deaths > 0 {
            out.push(format!("Deaths {deaths}"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dungeon_sim::class::ClassId;
    use dungeon_sim::content::Content;
    use dungeon_sim::sim::{Controller, PartyMemberSpec, RunOptions, RunOutcome};

    /// A default party with the player's slot marked, as the game builds it.
    fn party(class: ClassId) -> Vec<PartyMemberSpec> {
        let mut pr = dungeon_sim::Profile::new_default();
        pr.player_class = class;
        pr.auto_fill_party();
        pr.party_specs(true)
    }

    /// Every line the feed gives over one autopilot run (the AI plays the
    /// player's character), fed one simulation step at a time like the game.
    fn run_lines(class: ClassId, seed: u64) -> Vec<FeedLine> {
        let c = Content::builtin();
        let opts = RunOptions {
            autopilot: true,
            ..Default::default()
        };
        let mut sim = Sim::with_options(&c, &party(class), seed, opts);
        let p = sim.player.unwrap();
        // As in the demo: the AI drives the player's slot, the tank leads.
        sim.actors[p].controller = Controller::Ai;
        if let Some(t) = sim.tank() {
            sim.leader = t;
        }
        let mut feed = CombatFeed::default();
        let (mut seen, mut attempts) = (0, 0);
        let mut lines = Vec::new();
        while sim.outcome == RunOutcome::InProgress && sim.time < 900.0 {
            sim.step();
            let now = sim.metrics.actors[p].interrupts_attempted;
            lines.extend(feed.lines(&sim, &sim.events[seen..], now - attempts));
            seen = sim.events.len();
            attempts = now;
        }
        lines
    }

    fn has(lines: &[FeedLine], pat: &str) -> bool {
        lines.iter().any(|l| l.text.contains(pat))
    }

    #[test]
    fn warden_lines_name_ability_target_and_outcome() {
        let l = run_lines(ClassId::Warden, 1);
        assert!(has(&l, "Shield Slam > "), "{l:#?}");
        assert!(has(&l, "Bulwark: 40% less damage taken for 8 s"), "{l:#?}");
        // Quake Stomp on several enemies is one line.
        assert!(has(&l, "Quake Stomp > ") && has(&l, " enemies: "), "{l:#?}");
        assert!(!has(&l, "Melee"), "auto-attacks stay out of the feed");
    }

    #[test]
    fn mender_heals_report_overheal_and_shields() {
        let l = run_lines(ClassId::Mender, 1);
        assert!(has(&l, "Mend > "), "{l:#?}");
        assert!(has(&l, "Ward > ") && has(&l, ": shielded"), "{l:#?}");
        assert!(has(&l, "Renew > ") && has(&l, "heal over time"), "{l:#?}");
        // Renew ticks are not lines of their own.
        assert!(
            !l.iter()
                .any(|x| x.text.starts_with("Renew > ") && x.text.contains('+')),
            "{l:#?}"
        );
    }

    #[test]
    fn striker_and_arcanist_lines() {
        let s = run_lines(ClassId::Striker, 1);
        assert!(has(&s, "Rend Strike > "), "{s:#?}");
        assert!(has(&s, "Lacerate > ") && has(&s, "bleeding"), "{s:#?}");
        assert!(has(&s, "Evade: threat reduced 50%"), "{s:#?}");
        let a = run_lines(ClassId::Arcanist, 1);
        assert!(has(&a, "Arc Bolt > "), "{a:#?}");
    }

    #[test]
    fn interrupts_say_what_they_stopped_or_that_nothing_was_cast() {
        let mut any = Vec::new();
        for class in ClassId::ALL {
            for seed in 1..=3 {
                any.extend(run_lines(class, seed));
            }
        }
        assert!(
            any.iter().any(|l| l.tone == Tone::Interrupt
                && l.text.contains(" > interrupted ")
                && l.text.contains("'s ")),
            "no player interrupt line"
        );
        assert!(
            any.iter()
                .any(|l| l.tone == Tone::Companion && l.text.contains(" interrupted ")),
            "no companion interrupt line"
        );
    }

    #[test]
    fn fight_numbers_cover_one_pull_from_its_first_hit() {
        let c = Content::builtin();
        let opts = RunOptions {
            autopilot: true,
            ..Default::default()
        };
        let mut sim = Sim::with_options(&c, &party(ClassId::Striker), 2, opts);
        let p = sim.player.unwrap();
        sim.actors[p].controller = Controller::Ai;
        if let Some(t) = sim.tank() {
            sim.leader = t;
        }
        let mut t = FightTracker::default();
        assert_eq!(t.lines(&sim), vec!["No fight yet".to_string()]);
        while t.current.as_ref().is_none_or(|f| f.end.is_none()) {
            sim.step();
            t.update(&sim);
            assert!(sim.time < 600.0, "no fight ended");
        }
        // Out of combat before the pull, so the first fight's base is zero:
        // the panel shows everything the striker did in it.
        let f = t.current.clone().unwrap();
        assert_eq!(f.base.damage_done, 0.0);
        let dealt = sim.metrics.actors[p].damage_done;
        assert!(dealt > 0.0);
        let l = t.lines(&sim);
        assert!(l[0].starts_with("Last fight: "), "{l:?}");
        assert!(
            l[1].starts_with(&format!("Damage {} (", thousands(dealt))),
            "{l:?}"
        );
        assert_eq!(thousands(1234567.0), "1,234,567");
    }

    #[test]
    fn a_heal_reports_effective_healing_and_overheal() {
        let c = Content::builtin();
        let sim = Sim::new(&c, &party(ClassId::Mender), 1);
        let p = sim.player.unwrap();
        let tank = sim.tank().unwrap();
        let heal = |dst, amount, overheal, crit| Event {
            time: 3.0,
            kind: EventKind::Heal {
                src: p,
                dst,
                what: Label::from("Mend"),
                amount,
                overheal,
                crit,
            },
        };
        let mut feed = CombatFeed::default();
        let l = feed.lines(&sim, &[heal(tank, 120.0, 300.0, false)], 0);
        let who = &sim.actors[tank].name;
        assert_eq!(l[0].text, format!("Mend > {who}: +120 (300 overheal)"));
        let l = feed.lines(&sim, &[heal(p, 630.0, 0.0, true)], 0);
        assert_eq!(l[0].text, "Mend > you: +630 CRIT");
        // A Silence Pulse with nothing to stop leaves no event at all.
        let a = Sim::new(&c, &party(ClassId::Arcanist), 1);
        let l = CombatFeed::default().lines(&a, &[], 1);
        assert_eq!(l[0].text, "Silence Pulse: nothing to interrupt");
        assert_eq!(l[0].tone, Tone::Failed);
    }

    /// Feed lines over a lab run (0.10): the AI plays the player's Striker
    /// in `style`.
    fn lab_lines(style: dungeon_sim::class::StrikerStyle) -> Vec<FeedLine> {
        let c = Content::builtin();
        let party = dungeon_sim::lab::lab_party(3);
        let mut sim = dungeon_sim::lab::lab_sim(
            &c,
            &party,
            1,
            style,
            dungeon_sim::sim::LabPlan::Expert,
            "normal",
        );
        let p = sim.player.unwrap();
        let mut feed = CombatFeed::default();
        let (mut seen, mut attempts) = (0, 0);
        let mut lines = Vec::new();
        while sim.outcome == RunOutcome::InProgress && sim.time < 900.0 {
            sim.step();
            let now = sim.metrics.actors[p].interrupts_attempted;
            lines.extend(feed.lines(&sim, &sim.events[seen..], now - attempts));
            seen = sim.events.len();
            attempts = now;
        }
        lines
    }

    #[test]
    fn lab_styles_explain_their_mechanics_in_the_feed() {
        use dungeon_sim::class::StrikerStyle;
        let a = lab_lines(StrikerStyle::Planned);
        // Edge shows at the end of the line that earned it.
        assert!(
            a.iter()
                .any(|l| l.text.starts_with("Honed Strike > ") && l.text.ends_with("/5]")),
            "{a:#?}"
        );
        assert!(
            a.iter()
                .any(|l| l.text.starts_with("Deep Lacerate > ") && l.text.contains("bleeding")),
            "{a:#?}"
        );
        let b = lab_lines(StrikerStyle::Reactive);
        assert!(
            b.iter()
                .any(|l| l.tone == Tone::Opening && l.text.ends_with("- Exploit!")),
            "{b:#?}"
        );
        assert!(b.iter().any(|l| l.text.starts_with("Exploit > ")), "{b:#?}");
        // The classic Striker sees none of it.
        let c = lab_lines(StrikerStyle::Classic);
        assert!(
            !c.iter()
                .any(|l| l.text.contains("[Edge") || l.tone == Tone::Opening)
        );
    }

    #[test]
    fn flow_lines_name_seize_and_put_edge_on_the_hit_that_earned_it() {
        use dungeon_sim::class::StrikerStyle;
        let l = lab_lines(StrikerStyle::Flow);
        assert!(
            l.iter()
                .any(|x| x.tone == Tone::Opening && x.text.ends_with("- Seize!")),
            "{l:#?}"
        );
        // Edge tags sit on your own hits, never on an opening line.
        assert!(
            !l.iter()
                .any(|x| x.tone == Tone::Opening && x.text.contains("[Edge")),
            "{l:#?}"
        );
        assert!(
            l.iter()
                .any(|x| x.text.starts_with("Quick Cut > ") && x.text.contains("[Edge")),
            "{l:#?}"
        );
    }

    #[test]
    fn a_whirl_on_three_enemies_is_one_line() {
        let c = Content::builtin();
        let sim = Sim::new(&c, &party(ClassId::Striker), 1);
        let p = sim.player.unwrap();
        let enemies: Vec<ActorId> = sim.enemies.iter().copied().take(3).collect();
        let what = Label::from("Whirl");
        let events: Vec<Event> = enemies
            .iter()
            .enumerate()
            .map(|(i, &dst)| Event {
                time: 5.0,
                kind: EventKind::Damage {
                    src: p,
                    dst,
                    what: what.clone(),
                    amount: if i == 1 { 0.0 } else { 40.0 },
                    absorbed: 0.0,
                    outcome: if i == 0 {
                        HitOutcome::Crit
                    } else if i == 1 {
                        HitOutcome::Dodge
                    } else {
                        HitOutcome::Hit
                    },
                    school: dungeon_sim::stats::School::Physical,
                },
            })
            .collect();
        let l = CombatFeed::default().lines(&sim, &events, 0);
        assert_eq!(
            l,
            vec![FeedLine {
                text: "Whirl > 3 enemies: 80 (1 crit) (1 avoided)".into(),
                tone: Tone::Crit
            }]
        );
    }
}
