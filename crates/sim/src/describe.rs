//! Player-facing descriptions (tooltips) generated from the real definitions,
//! so the text can never drift from the rules. Every number shown here comes
//! from the ability/item/profile/difficulty data and the formulas in RULES.md.

use crate::ai::AiProfile;
use crate::class::{AbilityDef, ClassId, Effect, GCD, ResourceKind, Role, TargetRule};
use crate::content::Difficulty;
use crate::items::{ItemSlot, ItemStats, Rarity, RoleAffinity};
use crate::profile::Item;
use crate::sim::{Aura, AuraKind};
use crate::stats::HitKind;

/// A tooltip: a title line and a body (lines separated by `\n`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tip {
    pub title: String,
    pub body: String,
}

impl Tip {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Tip {
        Tip {
            title: title.into(),
            body: body.into(),
        }
    }
}

fn secs(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{v:.0} s")
    } else {
        format!("{v:.1} s")
    }
}

/// Ability tooltip. `power_mult` is the caster's damage/healing multiplier
/// (level and gear power), so the numbers match what the ability really does
/// before mitigation, variance and crits.
pub fn ability(def: &AbilityDef, power_mult: f32, resource: ResourceKind) -> Tip {
    let p = |v: f32| (v * power_mult).round();
    let mut lines = Vec::new();
    let mut head = Vec::new();
    if def.cost > 0.0 {
        head.push(format!("{:.0} {}", def.cost, resource.label()));
    }
    if def.target != TargetRule::SelfOnly {
        head.push(format!("{:.0} m range", def.range));
    }
    head.push(if def.cast_time > 0.0 {
        format!("{} cast", secs(def.cast_time))
    } else {
        "instant".into()
    });
    if def.cooldown > 0.0 {
        head.push(format!("{} cooldown", secs(def.cooldown)));
    }
    lines.push(head.join(" | "));
    lines.push(
        match def.target {
            TargetRule::Enemy => "Target: your selected enemy (in line of sight).",
            TargetRule::Ally => {
                "Target: the ally under your mouse, else your selected ally, else yourself."
            }
            TargetRule::SelfOnly => "Target: yourself.",
        }
        .into(),
    );
    let school = def.school.label();
    let hit = |k: HitKind| match k {
        HitKind::Melee => " Uses the melee table (can miss, be dodged, parried or blocked).",
        HitKind::Spell => " Spell: can miss, never dodged, parried or blocked.",
        HitKind::Periodic | HitKind::Unavoidable => " Always hits.",
    };
    lines.push(match def.effect {
        Effect::Damage {
            amount,
            threat_mult,
            kind,
        } => {
            let threat = if (threat_mult - 1.0).abs() > 0.01 {
                format!(" Generates {threat_mult:.0}x threat.")
            } else {
                String::new()
            };
            format!(
                "Deals about {:.0} {school} damage.{threat}{}",
                p(amount),
                hit(kind)
            )
        }
        Effect::AoeDamage {
            amount,
            radius,
            threat_mult,
        } => format!(
            "Deals about {:.0} {school} damage to every engaged enemy within {radius:.0} m of you. Generates {threat_mult:.0}x threat.",
            p(amount)
        ),
        Effect::Heal { amount } => format!(
            "Heals for about {:.0}. Healing generates threat on every engaged enemy.",
            p(amount)
        ),
        Effect::Hot { total, duration } => format!(
            "Heals {:.0} over {} (a tick every second).",
            p(total),
            secs(duration)
        ),
        Effect::Dot {
            total,
            duration,
            ignores_armor,
        } => format!(
            "Deals {:.0} {school} damage over {}{}.",
            p(total),
            secs(duration),
            if ignores_armor {
                "; bleeds ignore armor"
            } else {
                ""
            }
        ),
        Effect::Absorb {
            amount,
            duration,
            lockout,
        } => format!(
            "Absorbs {:.0} damage for {}. The target can't be shielded again for {} (Weakened Soul).",
            p(amount),
            secs(duration),
            secs(lockout)
        ),
        Effect::Taunt { duration } => format!(
            "Raises your threat to the top of the target's list and forces it to attack you for {}.",
            secs(duration)
        ),
        Effect::DamageReduction { pct, duration } => format!(
            "Reduces damage you take by {:.0}% for {}.",
            pct * 100.0,
            secs(duration)
        ),
        Effect::ThreatDrop { pct } => format!(
            "Reduces your threat on every enemy by {:.0}%.",
            pct * 100.0
        ),
        Effect::Interrupt { lockout, damage } => {
            let dmg = if damage > 0.0 {
                format!(" Deals about {:.0} damage.", p(damage))
            } else {
                String::new()
            };
            format!(
                "Interrupts an interruptible cast and silences the enemy for {}.{dmg}",
                secs(lockout)
            )
        }
        Effect::Incapacitate { duration } => format!(
            "Incapacitates a non-boss enemy for {} (no actions or movement). Any damage breaks it; one at a time.",
            secs(duration)
        ),
        Effect::Dispel => "Removes one dispellable harmful effect from an ally.".into(),
        Effect::Builder {
            amount,
            edge,
            behind_edge,
        } => format!(
            "Deals about {:.0} {school} damage and gives {edge} Edge ({behind_edge} if you strike from behind). Edge holds up to 5 and lasts 20 s.{}",
            p(amount),
            hit(HitKind::Melee)
        ),
        Effect::EdgeWhirl {
            base,
            per_edge,
            radius,
        } => format!(
            "Spends all your Edge: deals about {:.0} + {:.0} per Edge {school} damage to every engaged enemy within {radius:.0} m of you. Usable with no Edge.",
            p(base),
            p(per_edge)
        ),
        Effect::EdgeBleed { per_edge, duration } => format!(
            "Needs Edge. Spends all of it: the target bleeds {:.0} per Edge over {} (ignores armor). Replaces your previous bleed on it; what was left of that is lost.",
            p(per_edge),
            secs(duration)
        ),
        Effect::ExploitOpening { amount, refund } => format!(
            "Needs an opening on the target (casting, just used an ability, kicked by you, or under 30% health). Deals about {:.0} {school} damage, uses up the opening and gives back {refund:.0} energy if it lands.{}",
            p(amount),
            hit(HitKind::Melee)
        ),
    });
    if !def.description.is_empty() {
        lines.push(def.description.to_string());
    }
    lines.push(format!(
        "Triggers the {} global cooldown. Casting is cancelled by moving.",
        secs(GCD)
    ));
    Tip::new(def.name, lines.join("\n"))
}

/// Buff / debuff tooltip.
pub fn aura(a: &Aura, source: &str) -> Tip {
    let stacks = a.stacks as f32;
    let effect = match a.kind {
        AuraKind::Hot { per_tick } => format!("Heals {:.0} every second.", per_tick * stacks),
        AuraKind::Dot {
            per_tick,
            school,
            ignores_armor,
        } => format!(
            "Deals {:.0} {} damage every second{}.",
            per_tick * stacks,
            school.label(),
            if ignores_armor {
                " (ignores armor)"
            } else {
                ""
            }
        ),
        AuraKind::DamageReduction { pct } => {
            format!("Damage taken reduced by {:.0}%.", pct * 100.0)
        }
        AuraKind::Absorb { remaining } => format!("Absorbs the next {remaining:.0} damage."),
        AuraKind::WeakenedSoul => "Cannot receive another absorb shield.".into(),
        AuraKind::Taunted { .. } => "Forced to attack the taunter.".into(),
        AuraKind::Incapacitated => "Cannot act or move. Any damage breaks it.".into(),
        AuraKind::Silenced => "Cannot cast spells or use special abilities.".into(),
        AuraKind::Edge => format!(
            "{:.0} Edge (of 5): spend it with Deep Lacerate or Reaping Whirl.",
            stacks
        ),
        AuraKind::Opening => "An opening: the reactive Striker can Exploit it.".into(),
        AuraKind::DamageTaken { pct } => format!(
            "Takes {:.0}% more damage ({:.0}% per stack).",
            pct * stacks * 100.0,
            pct * 100.0
        ),
        AuraKind::HealingTaken { pct } => format!(
            "Receives {:.0}% less healing ({:.0}% per stack).",
            pct * stacks * 100.0,
            pct * 100.0
        ),
    };
    let mut lines = vec![effect];
    lines.push(format!(
        "{} of {} left",
        secs(a.remaining.max(0.0)),
        secs(a.duration)
    ));
    if a.max_stacks > 1 {
        lines.push(format!("Stacks: {}/{}", a.stacks, a.max_stacks));
    }
    lines.push(format!(
        "{}{}",
        if a.harmful { "Harmful" } else { "Helpful" },
        if a.dispellable {
            " - dispellable (Purify)"
        } else if a.harmful {
            " - not dispellable"
        } else {
            ""
        }
    ));
    if !source.is_empty() {
        lines.push(format!("From {source}"));
    }
    Tip::new(a.name.to_string(), lines.join("\n"))
}

/// What each item statistic does (RULES.md "Statistics" and "Mitigation").
pub fn stat(name: &str) -> Option<&'static str> {
    Some(match name {
        "stamina" => "Stamina: +10 maximum health per point.",
        "power" => {
            "Power: +1% damage and healing per point (multiplies with the level bonus of +10% per level)."
        }
        "armor" => {
            "Armor: reduces physical damage by armor / (armor + 400 + 85 x attacker level), at most 75%. Bleeds ignore it."
        }
        "resist" => {
            "Resist: reduces magic damage by resist / (resist + 100 + 15 x attacker level), at most 75%."
        }
        "crit" => {
            "Crit: chance for a critical hit (melee x2.0, spells and heals x1.5). -1% per level the target is above you. Periodic effects never crit."
        }
        "hit" => {
            "Hit: subtracted from your miss chance (melee 5% +1% per level above you; spells 4/5/6/17% at +0..+3 levels)."
        }
        "dodge" => "Dodge: chance to avoid a melee attack entirely.",
        "parry" => "Parry: chance to avoid a melee attack from the front.",
        "block" => {
            "Block: chance to block a melee attack from the front, removing the block value from the damage."
        }
        "hp" => "Health. Regenerates 8% per second out of combat.",
        "mana" => {
            "Mana: full regeneration only 5 s after you last spent mana (35% of it while casting continuously); 10% per second out of combat."
        }
        "rage" => {
            "Rage: generated by dealing melee hits and taking damage; decays 2 per second out of combat. Max 100."
        }
        "energy" => "Energy: regenerates 10 per second. Max 100.",
        _ => return None,
    })
}

fn stat_lines(s: &ItemStats) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |v: f32, label: &str, pct: bool| {
        if v != 0.0 {
            out.push(if pct {
                format!("+{v}% {label}")
            } else {
                format!("+{v:.0} {label}")
            });
        }
    };
    push(s.stamina, "stamina", false);
    push(s.power, "power", false);
    push(s.armor, "armor", false);
    push(s.resist, "resist", false);
    push(s.crit, "crit", true);
    push(s.hit, "hit", true);
    push(s.dodge, "dodge", true);
    out
}

fn stat_diff(new: &ItemStats, old: &ItemStats) -> String {
    let mut parts = Vec::new();
    let mut push = |a: f32, b: f32, label: &str, pct: bool| {
        let d = a - b;
        if d.abs() > 0.001 {
            let sign = if d > 0.0 { "+" } else { "" };
            parts.push(if pct {
                format!("{sign}{d:.1}% {label}")
            } else {
                format!("{sign}{d:.0} {label}")
            });
        }
    };
    push(new.stamina, old.stamina, "stamina", false);
    push(new.power, old.power, "power", false);
    push(new.armor, old.armor, "armor", false);
    push(new.resist, old.resist, "resist", false);
    push(new.crit, old.crit, "crit", true);
    push(new.hit, old.hit, "hit", true);
    push(new.dodge, old.dodge, "dodge", true);
    if parts.is_empty() {
        "no change".into()
    } else {
        parts.join(", ")
    }
}

/// Equipment tooltip with the role it suits and a comparison against what the
/// wearer has in that slot (`equipped`), scored for the wearer's role.
#[allow(clippy::too_many_arguments)]
pub fn gear(
    name: &str,
    slot: ItemSlot,
    rarity: Rarity,
    role: RoleAffinity,
    stats: &ItemStats,
    equipped: Option<&Item>,
    wearer_role: Option<Role>,
    extra: &str,
) -> Tip {
    let suits = if role == RoleAffinity::Any {
        "suits any role".to_string()
    } else {
        format!("made for {} roles", role.label())
    };
    let mut lines = vec![format!("{} {} - {suits}", rarity.label(), slot.label())];
    let sl = stat_lines(stats);
    if sl.is_empty() {
        lines.push("No stats.".into());
    } else {
        lines.push(sl.join(", "));
    }
    if let Some(r) = wearer_role {
        if !role.suits(r) {
            lines.push(format!(
                "Not designed for a {} - stats are weighted for {} roles.",
                r.label(),
                role.label()
            ));
        }
        let score = stats.score_for(r);
        match equipped {
            Some(cur) => {
                let old = cur.stats.score_for(r);
                lines.push(format!(
                    "Compared with {}: {}",
                    cur.name,
                    stat_diff(stats, &cur.stats)
                ));
                lines.push(format!(
                    "{} score {score:.0} vs {old:.0} ({})",
                    r.label(),
                    if score > old + 0.5 {
                        "upgrade"
                    } else if score < old - 0.5 {
                        "downgrade"
                    } else {
                        "sidegrade"
                    }
                ));
            }
            None => lines.push(format!(
                "Slot is empty: an upgrade ({} score {score:.0}).",
                r.label()
            )),
        }
    }
    if !extra.is_empty() {
        lines.push(extra.to_string());
    }
    Tip::new(name, lines.join("\n"))
}

pub fn item(it: &Item, equipped: Option<&Item>, wearer_role: Option<Role>, extra: &str) -> Tip {
    let source = if it.source.is_empty() {
        String::new()
    } else {
        format!("Dropped in {}. ", it.source)
    };
    gear(
        &it.name,
        it.slot,
        it.rarity,
        it.role,
        &it.stats,
        equipped,
        wearer_role,
        &format!("{source}{extra}"),
    )
}

/// Companion AI profile, listing the actual parameters it changes.
pub fn profile(p: AiProfile) -> Tip {
    let k = p.params();
    let summary = match p {
        AiProfile::Conservative => {
            "Plays safe: waits for solid tank threat and keeps a wide margin."
        }
        AiProfile::Balanced => "The default trade-off between safety and speed.",
        AiProfile::Aggressive => {
            "Attacks at once and ignores threat margins; reacts slower to mechanics and skips crowd control."
        }
        AiProfile::SupportFocused => "Prioritises dispels, shields and early interrupts.",
        AiProfile::MechanicFocused => {
            "Fast reactions and wide safety margins around mechanics; interrupts early."
        }
    };
    let lines = [
        summary.to_string(),
        format!("Notices new danger zones after {}", secs(k.reaction_time)),
        format!(
            "DPS hold fire above {:.0}% of the tank's threat; wait for {:.0} tank threat",
            k.threat_cap * 100.0,
            k.min_tank_threat
        ),
        format!("Tank defensive below {:.0}% HP", k.defensive_hp * 100.0),
        format!(
            "Healer: Mend below {:.0}%, Renew below {:.0}%, Smite only above {:.0}% mana{}",
            k.mend_threshold * 100.0,
            k.hot_threshold * 100.0,
            k.smite_mana * 100.0,
            if k.dispel_first {
                ", dispels first"
            } else {
                ""
            }
        ),
        format!(
            "Interrupts with {} of the cast left; keeps {:.1} m extra from danger",
            secs(k.interrupt_window),
            k.danger_margin
        ),
        format!(
            "Crowd control: {}. Leader pulls at {:.0}% HP / {:.0}% mana",
            if k.use_cc { "yes" } else { "no" },
            k.ready_hp * 100.0,
            k.ready_mana * 100.0
        ),
        "Familiarity with a dungeon (past clears) speeds reactions further.".to_string(),
    ];
    Tip::new(format!("{} AI", p.label()), lines.join("\n"))
}

pub fn difficulty(d: &Difficulty) -> Tip {
    let mut lines = vec![
        format!("Enemy health x{:.2}, enemy damage x{:.2}", d.hp, d.damage),
        if d.level_bonus > 0 {
            format!(
                "Enemies are {} level(s) higher: you miss more, crit less and they hit harder through armor.",
                d.level_bonus
            )
        } else {
            "Enemies are at the dungeon's normal level.".into()
        },
        format!("Rewards (marks and experience) x{:.2}", d.reward_multiplier),
    ];
    if d.rarity_bonus > 0 {
        lines.push(format!(
            "Loot rarity upgraded by {} step(s).",
            d.rarity_bonus
        ));
    }
    Tip::new(d.name.clone(), lines.join("\n"))
}

pub fn class(c: ClassId) -> Tip {
    let abilities: Vec<&str> = c.abilities().iter().map(|a| a.def().name).collect();
    Tip::new(
        format!("{} ({})", c.label(), c.role().label()),
        format!(
            "{}\nResource: {}\nAbilities: {}",
            c.blurb(),
            c.resource().label(),
            abilities.join(", ")
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::AbilityId;

    #[test]
    fn ability_text_uses_real_numbers() {
        let def = AbilityId::Mend.def();
        let t = ability(&def, 1.0, ResourceKind::Mana);
        assert_eq!(t.title, "Mend");
        assert!(t.body.contains("1.5 s cast"), "{}", t.body);
        assert!(t.body.contains("ally under your mouse"));
        // Power scales the numbers shown.
        let Effect::Heal { amount } = def.effect else {
            panic!()
        };
        let t2 = ability(&def, 2.0, ResourceKind::Mana);
        assert!(
            t2.body.contains(&format!("{:.0}", amount * 2.0)),
            "{}",
            t2.body
        );
    }

    #[test]
    fn every_ability_and_profile_has_a_description() {
        for c in ClassId::ALL {
            for a in c.abilities() {
                let t = ability(&a.def(), 1.0, c.resource());
                assert!(t.body.lines().count() >= 3, "{}", t.title);
            }
            assert!(!class(c).body.is_empty());
        }
        for p in AiProfile::ALL {
            assert!(profile(p).body.contains("Interrupts with"));
        }
        for s in [
            "stamina", "power", "armor", "resist", "crit", "hit", "dodge",
        ] {
            assert!(stat(s).is_some(), "{s}");
        }
    }

    #[test]
    fn gear_tooltip_compares_with_equipped() {
        let mk = |name: &str, power: f32| Item {
            uid: 1,
            base_id: name.into(),
            name: name.into(),
            slot: ItemSlot::MainHand,
            rarity: Rarity::Uncommon,
            role: RoleAffinity::Caster,
            stats: ItemStats {
                power,
                ..Default::default()
            },
            source: String::new(),
        };
        let old = mk("Old Wand", 4.0);
        let new = mk("New Wand", 9.0);
        let t = item(&new, Some(&old), Some(Role::Healer), "");
        assert!(t.body.contains("+5 power"), "{}", t.body);
        assert!(t.body.contains("upgrade"), "{}", t.body);
        let t = item(&new, None, Some(Role::Tank), "");
        assert!(t.body.contains("Not designed for a"), "{}", t.body);
    }
}
