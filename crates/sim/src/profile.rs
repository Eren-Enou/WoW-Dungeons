//! Persistent player data: roster of companions, levels, equipment, stash,
//! currency and run history.
//!
//! Saved as pretty JSON. Writes go to a temporary file which is then renamed
//! over the real one so a crash mid-save cannot corrupt the profile. Version 1
//! saves (prototype 0.1) are migrated on load.

use crate::ai::AiProfile;
use crate::class::{ClassId, MAX_LEVEL, Role};
use crate::content::{Content, EnemyRank};
use crate::items::{ItemSlot, ItemStats, Rarity, RoleAffinity};
use crate::rng::Rng;
use crate::sim::{PartyMemberSpec, RunOutcome, Sim};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

pub const PROFILE_VERSION: u32 = 2;
pub const SAVE_DIR_ENV_VAR: &str = "DUNGEON_SAVE_DIR";
/// Companions in a standard 5-person party.
pub const PARTY_COMPANIONS: usize = 4;
/// Group sizes the hub offers: a party or a 10-person raid.
pub const GROUP_SIZES: [u32; 2] = [5, 10];

/// The default roster: three companions of every class.
const DEFAULT_ROSTER: [(&str, &str, ClassId); 12] = [
    ("brannoc", "Brannoc", ClassId::Warden),
    ("ysolde", "Ysolde", ClassId::Warden),
    ("thessaly", "Thessaly", ClassId::Mender),
    ("orrin", "Orrin", ClassId::Mender),
    ("kael", "Kael", ClassId::Striker),
    ("dovic", "Dovic", ClassId::Striker),
    ("mirelle", "Mirelle", ClassId::Arcanist),
    ("senna", "Senna", ClassId::Arcanist),
    ("hale", "Hale", ClassId::Warden),
    ("ivra", "Ivra", ClassId::Mender),
    ("tamsin", "Tamsin", ClassId::Striker),
    ("corvo", "Corvo", ClassId::Arcanist),
];

fn new_companion(id: &str, name: &str, class: ClassId) -> Companion {
    Companion {
        id: id.to_string(),
        name: name.to_string(),
        class,
        equipment: Equipment::default(),
        runs_completed: 0,
        level: 1,
        xp: 0,
        profile: AiProfile::Balanced,
        clears: BTreeMap::new(),
    }
}

fn five() -> u32 {
    5
}
const HISTORY_LEN: usize = 20;

/// [orig] Experience needed to go from `level` to `level + 1`.
pub fn xp_to_next(level: u32) -> u32 {
    250 * level + 50 * level * level
}

/// [orig] Experience for killing an enemy of `enemy_level` at `level`.
/// Enemies 4+ levels below give a quarter.
pub fn kill_xp(rank: EnemyRank, enemy_level: u32, level: u32) -> u32 {
    let base = match rank {
        EnemyRank::Trash => 20,
        EnemyRank::Elite => 60,
        EnemyRank::Boss => 250,
    } * enemy_level;
    if enemy_level + 4 <= level {
        base / 4
    } else {
        base
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// Unique instance id within this profile.
    pub uid: u64,
    /// Loot table id this item came from.
    pub base_id: String,
    pub name: String,
    pub slot: ItemSlot,
    #[serde(default)]
    pub rarity: Rarity,
    #[serde(default)]
    pub role: RoleAffinity,
    #[serde(default)]
    pub stats: ItemStats,
    #[serde(default)]
    pub source: String,
}

impl Item {
    pub fn describe(&self) -> String {
        format!(
            "[{}] {} ({}, {}{}) {}",
            self.rarity.label(),
            self.name,
            self.slot.label(),
            self.role.label(),
            if self.source.is_empty() {
                String::new()
            } else {
                format!(", from {}", self.source)
            },
            self.stats.describe()
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Equipment {
    pub slots: BTreeMap<ItemSlot, Item>,
}

impl Equipment {
    pub fn stats(&self) -> ItemStats {
        let mut s = ItemStats::default();
        for i in self.slots.values() {
            s.add(&i.stats);
        }
        s
    }

    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.slots.values()
    }

    pub fn get(&self, slot: ItemSlot) -> Option<&Item> {
        self.slots.get(&slot)
    }

    /// Put `item` in its slot, returning whatever was there.
    pub fn put(&mut self, item: Item) -> Option<Item> {
        self.slots.insert(item.slot, item)
    }

    pub fn take(&mut self, slot: ItemSlot) -> Option<Item> {
        self.slots.remove(&slot)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Companion {
    pub id: String,
    pub name: String,
    pub class: ClassId,
    pub equipment: Equipment,
    pub runs_completed: u32,
    #[serde(default = "one")]
    pub level: u32,
    #[serde(default)]
    pub xp: u32,
    #[serde(default)]
    pub profile: AiProfile,
    /// Victories per dungeon id (drives AI familiarity).
    #[serde(default)]
    pub clears: BTreeMap<String, u32>,
}

/// [orig] Familiarity grows with clears of a dungeon: 5 clears = fully familiar.
pub fn familiarity(clears: u32) -> f32 {
    (clears as f32 / 5.0).min(1.0)
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub dungeon: String,
    pub difficulty: String,
    pub outcome: String,
    pub duration: f32,
    pub deaths: u32,
    pub seed: u64,
    pub marks: u32,
    pub xp: u32,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    pub player_name: String,
    pub player_class: ClassId,
    pub player_equipment: Equipment,
    pub player_level: u32,
    pub player_xp: u32,
    /// Vault Marks: currency awarded per run.
    pub marks: u32,
    pub runs_won: u32,
    pub runs_wiped: u32,
    pub next_item_uid: u64,
    /// Unequipped items.
    pub stash: Vec<Item>,
    pub roster: Vec<Companion>,
    /// Companion ids currently recruited into the party (up to 4).
    pub party: Vec<String>,
    pub selected_dungeon: String,
    pub selected_difficulty: String,
    pub history: Vec<RunRecord>,
    /// In-game sandbox knobs applied to every run.
    #[serde(default)]
    pub sandbox: crate::sim::Overrides,
    /// 5 (party) or 10 (raid group), including the player.
    #[serde(default = "five")]
    pub group_size: u32,
    /// Player interface preferences.
    #[serde(default)]
    pub settings: Settings,
}

/// Interface preferences stored with the profile.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Camera eases behind the character while moving.
    pub camera_follow: bool,
    /// Auto-follow responsiveness (1/seconds).
    pub follow_speed: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            camera_follow: true,
            follow_speed: 3.0,
        }
    }
}

/// Who a piece of gear is being equipped on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wearer {
    Player,
    Companion(usize),
}

/// What happened in a run, distilled from the sim for reward purposes.
#[derive(Debug, Clone)]
pub struct RunReport {
    pub dungeon: String,
    pub difficulty: String,
    pub reward_multiplier: f32,
    pub rarity_bonus: u32,
    pub outcome: RunOutcome,
    pub cleared: Vec<String>,
    /// (rank, level) of every enemy killed.
    pub kills: Vec<(EnemyRank, u32)>,
    pub duration: f32,
    pub deaths: u32,
    pub seed: u64,
}

impl RunReport {
    pub fn from_sim(sim: &Sim) -> RunReport {
        let kills = sim
            .events
            .iter()
            .filter_map(|e| match e.kind {
                crate::events::EventKind::Death { actor } => sim.actors[actor]
                    .enemy
                    .as_ref()
                    .map(|es| (es.rank, sim.actors[actor].stats.level)),
                _ => None,
            })
            .collect();
        RunReport {
            dungeon: sim.content.dungeon.id.clone(),
            difficulty: sim.difficulty.id.clone(),
            reward_multiplier: sim.difficulty.reward_multiplier
                * sim.options.overrides.reward_factor(),
            rarity_bonus: sim.difficulty.rarity_bonus,
            outcome: sim.outcome,
            cleared: sim
                .encounters
                .iter()
                .filter(|e| e.cleared)
                .map(|e| e.id.clone())
                .collect(),
            kills,
            duration: sim.ended_at.unwrap_or(sim.time),
            deaths: sim
                .party
                .iter()
                .map(|&p| sim.metrics.actors[p].deaths)
                .sum(),
            seed: sim.seed,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunRewards {
    pub marks: u32,
    pub xp: u32,
    pub items: Vec<Item>,
    /// (name, new level) for everyone who levelled up.
    pub level_ups: Vec<(String, u32)>,
}

/// Computed combat numbers for display in the hub.
#[derive(Debug, Clone)]
pub struct CharacterSheet {
    pub name: String,
    pub class: ClassId,
    pub level: u32,
    pub xp: u32,
    pub xp_next: u32,
    pub max_hp: f32,
    pub resource: crate::class::ResourceKind,
    pub max_resource: f32,
    pub power_pct: f32,
    pub stats: crate::stats::Stats,
    /// Physical damage reduction against an attacker of the same level.
    pub armor_dr: f32,
    pub gear: ItemStats,
    pub profile: AiProfile,
}

#[derive(Debug)]
pub enum ProfileError {
    Io(String),
    Corrupt(String),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileError::Io(m) => write!(f, "save file I/O error: {m}"),
            ProfileError::Corrupt(m) => write!(f, "save file is corrupt: {m}"),
        }
    }
}

impl std::error::Error for ProfileError {}

impl Default for Profile {
    fn default() -> Self {
        Profile::new_default()
    }
}

/// Add XP, levelling up as needed. Returns the new level if it changed.
fn add_xp(level: &mut u32, xp: &mut u32, gained: u32) -> Option<u32> {
    let before = *level;
    *xp += gained;
    while *level < MAX_LEVEL && *xp >= xp_to_next(*level) {
        *xp -= xp_to_next(*level);
        *level += 1;
    }
    if *level == MAX_LEVEL {
        *xp = 0;
    }
    (*level != before).then_some(*level)
}

impl Profile {
    pub fn new_default() -> Self {
        let roster = DEFAULT_ROSTER
            .iter()
            .map(|(id, name, class)| new_companion(id, name, *class))
            .collect();
        let mut p = Profile {
            version: PROFILE_VERSION,
            player_name: "Wayfarer".to_string(),
            player_class: ClassId::Striker,
            player_equipment: Equipment::default(),
            player_level: 1,
            player_xp: 0,
            marks: 0,
            runs_won: 0,
            runs_wiped: 0,
            next_item_uid: 1,
            stash: Vec::new(),
            roster,
            party: Vec::new(),
            selected_dungeon: "sunken_vault".into(),
            selected_difficulty: "normal".into(),
            history: Vec::new(),
            sandbox: crate::sim::Overrides::default(),
            group_size: 5,
            settings: Settings::default(),
        };
        p.auto_fill_party();
        p
    }

    /// Recruit a balanced party around the player's role: one tank, one healer,
    /// the rest damage dealers (ranged and melee alternating).
    pub fn auto_fill_party(&mut self) {
        let player_role = self.player_class.role();
        let max = self.max_companions();
        // One tank and one healer per five players, including the player.
        let per_role = (self.group_size as usize / 5).max(1);
        let mut needed: Vec<Role> = Vec::new();
        for role in [Role::Tank, Role::Healer] {
            let have = usize::from(player_role == role);
            for _ in have..per_role {
                needed.push(role);
            }
        }
        let mut dps_cycle = [Role::RangedDps, Role::MeleeDps].into_iter().cycle();
        while needed.len() < max {
            needed.push(dps_cycle.next().expect("cycle never ends"));
        }
        let mut chosen: Vec<String> = Vec::new();
        for role in needed {
            if let Some(c) = self
                .roster
                .iter()
                .find(|c| c.class.role() == role && !chosen.contains(&c.id))
            {
                chosen.push(c.id.clone());
            }
        }
        for c in &self.roster {
            if chosen.len() >= max {
                break;
            }
            if !chosen.contains(&c.id) {
                chosen.push(c.id.clone());
            }
        }
        self.party = chosen;
    }

    /// Companions allowed by the current group size.
    pub fn max_companions(&self) -> usize {
        (self.group_size.max(2) - 1) as usize
    }

    /// Switch between a 5-person party and a 10-person raid group.
    pub fn set_group_size(&mut self, size: u32) {
        self.group_size = if GROUP_SIZES.contains(&size) { size } else { 5 };
        self.ensure_roster();
        self.auto_fill_party();
    }

    /// Add any default companions missing from an older save.
    pub fn ensure_roster(&mut self) {
        for (id, name, class) in DEFAULT_ROSTER {
            if !self.roster.iter().any(|c| c.id == id) {
                self.roster.push(new_companion(id, name, class));
            }
        }
    }

    /// Toggle a companion in or out of the party. Returns false if the party is full.
    pub fn toggle_recruit(&mut self, roster_index: usize) -> bool {
        let Some(c) = self.roster.get(roster_index) else {
            return false;
        };
        if let Some(pos) = self.party.iter().position(|id| *id == c.id) {
            self.party.remove(pos);
            true
        } else if self.party.len() < self.max_companions() {
            self.party.push(c.id.clone());
            true
        } else {
            false
        }
    }

    pub fn party_companions(&self) -> Vec<&Companion> {
        self.party
            .iter()
            .filter_map(|id| self.roster.iter().find(|c| c.id == *id))
            .collect()
    }

    /// Human-readable warnings about the current composition (not errors: it is a sandbox).
    pub fn party_warnings(&self) -> Vec<String> {
        let mut roles: Vec<Role> = self
            .party_companions()
            .iter()
            .map(|c| c.class.role())
            .collect();
        roles.push(self.player_class.role());
        let mut w = Vec::new();
        if self.party.len() < self.max_companions() {
            w.push(format!(
                "Group has {} of {} companions",
                self.party.len(),
                self.max_companions()
            ));
        }
        if !roles.contains(&Role::Tank) {
            w.push("No tank in party".to_string());
        }
        if !roles.contains(&Role::Healer) {
            w.push("No healer in party".to_string());
        }
        w
    }

    /// Party specs for a run: player first, then recruited companions.
    pub fn party_specs(&self, player_controlled: bool) -> Vec<PartyMemberSpec> {
        self.party_specs_for("", player_controlled)
    }

    /// Party specs with companions' familiarity for `dungeon`.
    pub fn party_specs_for(&self, dungeon: &str, player_controlled: bool) -> Vec<PartyMemberSpec> {
        let mut specs = vec![PartyMemberSpec {
            name: self.player_name.clone(),
            class: self.player_class,
            level: self.player_level,
            gear: self.player_equipment.stats(),
            player_controlled,
            profile: AiProfile::Balanced,
            familiarity: 0.0,
        }];
        for c in self.party_companions() {
            specs.push(PartyMemberSpec {
                name: c.name.clone(),
                class: c.class,
                level: c.level,
                gear: c.equipment.stats(),
                player_controlled: false,
                profile: c.profile,
                familiarity: familiarity(c.clears.get(dungeon).copied().unwrap_or(0)),
            });
        }
        specs
    }

    pub fn wearer_role(&self, who: Wearer) -> Option<Role> {
        match who {
            Wearer::Player => Some(self.player_class.role()),
            Wearer::Companion(i) => self.roster.get(i).map(|c| c.class.role()),
        }
    }

    fn equipment_mut(&mut self, who: Wearer) -> Option<&mut Equipment> {
        match who {
            Wearer::Player => Some(&mut self.player_equipment),
            Wearer::Companion(i) => self.roster.get_mut(i).map(|c| &mut c.equipment),
        }
    }

    pub fn equipment(&self, who: Wearer) -> Option<&Equipment> {
        match who {
            Wearer::Player => Some(&self.player_equipment),
            Wearer::Companion(i) => self.roster.get(i).map(|c| &c.equipment),
        }
    }

    /// Score change if `item` replaced whatever `who` wears in that slot.
    pub fn compare(&self, item: &Item, who: Wearer) -> f32 {
        let Some(role) = self.wearer_role(who) else {
            return 0.0;
        };
        let current = self
            .equipment(who)
            .and_then(|e| e.get(item.slot))
            .map(|i| i.stats.score_for(role))
            .unwrap_or(0.0);
        item.stats.score_for(role) - current
    }

    /// Equip stash item `stash_index` on `who`; any item already in that slot returns to the stash.
    pub fn equip(&mut self, stash_index: usize, who: Wearer) -> Result<(), String> {
        if stash_index >= self.stash.len() {
            return Err("no such item in stash".into());
        }
        if self.equipment_mut(who).is_none() {
            return Err("no such companion".into());
        }
        let item = self.stash.remove(stash_index);
        let eq = self.equipment_mut(who).expect("checked above");
        if let Some(old) = eq.put(item) {
            self.stash.push(old);
        }
        Ok(())
    }

    /// Move an equipped item back to the stash.
    pub fn unequip(&mut self, who: Wearer, slot: ItemSlot) -> bool {
        let Some(item) = self.equipment_mut(who).and_then(|e| e.take(slot)) else {
            return false;
        };
        self.stash.push(item);
        true
    }

    /// Give every recruited companion (and the player) the best stash upgrades for
    /// their role. Returns a description of each change.
    pub fn auto_equip(&mut self) -> Vec<String> {
        let mut changes = Vec::new();
        let mut wearers = vec![Wearer::Player];
        for id in self.party.clone() {
            if let Some(i) = self.roster.iter().position(|c| c.id == id) {
                wearers.push(Wearer::Companion(i));
            }
        }
        for who in wearers {
            let role = self.wearer_role(who).expect("valid wearer");
            loop {
                let best = self
                    .stash
                    .iter()
                    .enumerate()
                    .filter(|(_, it)| it.role.suits(role))
                    .map(|(i, it)| (i, self.compare(it, who)))
                    .filter(|(_, delta)| *delta > 0.5)
                    .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));
                let Some((i, _)) = best else { break };
                let name = self.stash[i].name.clone();
                self.equip(i, who).expect("valid index");
                let who_name = match who {
                    Wearer::Player => self.player_name.clone(),
                    Wearer::Companion(c) => self.roster[c].name.clone(),
                };
                changes.push(format!("{who_name} equips {name}"));
            }
        }
        changes
    }

    fn roll_item(
        &mut self,
        content: &Content,
        source: &str,
        rarity_bonus: u32,
        rng: &mut Rng,
    ) -> Option<Item> {
        let fin = content.encounters[content.final_encounter()].id.clone();
        let pool: Vec<&crate::content::LootEntry> = content
            .loot
            .iter()
            .filter(|l| l.source.as_deref().unwrap_or(fin.as_str()) == source)
            .collect();
        let total: u32 = pool.iter().map(|l| l.weight).sum();
        if total == 0 {
            return None;
        }
        let mut roll = (rng.next_u64() % total as u64) as u32;
        let entry = pool.into_iter().find(|l| {
            if roll < l.weight {
                true
            } else {
                roll -= l.weight;
                false
            }
        })?;
        let rarity = entry.rarity.upgraded(rarity_bonus);
        let scale = rarity.stat_mult() / entry.rarity.stat_mult();
        let item = Item {
            uid: self.next_item_uid,
            base_id: entry.id.clone(),
            name: entry.name.clone(),
            slot: entry.slot,
            rarity,
            role: entry.role,
            stats: if rarity == entry.rarity {
                entry.stats
            } else {
                entry.stats.scaled(scale)
            },
            source: content.dungeon.name.clone(),
        };
        self.next_item_uid += 1;
        Some(item)
    }

    /// Grant rewards for a finished run. Deterministic for a given seed.
    ///
    /// * Every cleared encounter that has loot rolls one item for the stash.
    /// * Experience: kills + the clear bonus (victory only), times the difficulty's
    ///   reward multiplier, for the player and every recruited companion.
    /// * Marks: per clear or per wipe, times the reward multiplier.
    pub fn grant_rewards(&mut self, content: &Content, report: &RunReport) -> RunRewards {
        let victory = report.outcome == RunOutcome::Victory;
        let mult = report.reward_multiplier;
        let base_marks = if victory {
            content.rewards.marks_per_clear
        } else if report.outcome == RunOutcome::InProgress {
            0
        } else {
            content.rewards.marks_per_wipe
        };
        let marks = (base_marks as f32 * mult).round() as u32;
        self.marks += marks;
        match report.outcome {
            RunOutcome::Victory => self.runs_won += 1,
            RunOutcome::InProgress => {}
            _ => self.runs_wiped += 1,
        }
        let mut rng = Rng::new(report.seed ^ 0x10_07);
        let mut items = Vec::new();
        for id in &report.cleared {
            if let Some(item) = self.roll_item(content, id, report.rarity_bonus, &mut rng) {
                self.stash.push(item.clone());
                items.push(item);
            }
        }
        let party_ids = self.party.clone();
        let mut level_ups = Vec::new();
        let xp_for = |level: u32| -> u32 {
            let kills: u32 = report
                .kills
                .iter()
                .map(|(rank, lvl)| kill_xp(*rank, *lvl, level))
                .sum();
            let clear = if victory {
                content.rewards.xp_per_clear
            } else {
                0
            };
            ((kills + clear) as f32 * mult).round() as u32
        };
        let player_xp = xp_for(self.player_level);
        if let Some(l) = add_xp(&mut self.player_level, &mut self.player_xp, player_xp) {
            level_ups.push((self.player_name.clone(), l));
        }
        for c in self.roster.iter_mut().filter(|c| party_ids.contains(&c.id)) {
            if victory {
                c.runs_completed += 1;
                *c.clears.entry(report.dungeon.clone()).or_default() += 1;
            }
            let gained = xp_for(c.level);
            if let Some(l) = add_xp(&mut c.level, &mut c.xp, gained) {
                level_ups.push((c.name.clone(), l));
            }
        }
        self.history.push(RunRecord {
            dungeon: report.dungeon.clone(),
            difficulty: report.difficulty.clone(),
            outcome: format!("{:?}", report.outcome),
            duration: report.duration,
            deaths: report.deaths,
            seed: report.seed,
            marks,
            xp: player_xp,
            items: items.iter().map(|i| i.name.clone()).collect(),
        });
        if self.history.len() > HISTORY_LEN {
            let drop = self.history.len() - HISTORY_LEN;
            self.history.drain(0..drop);
        }
        RunRewards {
            marks,
            xp: player_xp,
            items,
            level_ups,
        }
    }

    /// Combat numbers for the player or a companion (class + level + gear).
    pub fn sheet(&self, who: Wearer) -> Option<CharacterSheet> {
        let (name, class, level, xp, eq, profile) = match who {
            Wearer::Player => (
                self.player_name.clone(),
                self.player_class,
                self.player_level,
                self.player_xp,
                &self.player_equipment,
                AiProfile::Balanced,
            ),
            Wearer::Companion(i) => {
                let c = self.roster.get(i)?;
                (
                    c.name.clone(),
                    c.class,
                    c.level,
                    c.xp,
                    &c.equipment,
                    c.profile,
                )
            }
        };
        let cs = class.stats(level);
        let gear = eq.stats();
        let mut st = cs.stats;
        gear.apply(&mut st);
        Some(CharacterSheet {
            name,
            class,
            level,
            xp,
            xp_next: if level >= MAX_LEVEL {
                0
            } else {
                xp_to_next(level)
            },
            max_hp: (cs.max_hp + gear.stamina * 10.0).round(),
            resource: cs.resource,
            max_resource: cs.max_resource,
            power_pct: (crate::class::level_power(level) * (1.0 + gear.power / 100.0) - 1.0)
                * 100.0,
            armor_dr: crate::stats::armor_reduction(st.armor, level),
            stats: st,
            gear,
            profile,
        })
    }

    /// [orig] Vault Mark price of an item: by rarity, plus its role-neutral stat score.
    pub fn item_price(rarity: Rarity, stats: &ItemStats) -> u32 {
        let base = match rarity {
            Rarity::Common => 6.0,
            Rarity::Uncommon => 12.0,
            Rarity::Rare => 25.0,
            Rarity::Epic => 50.0,
        };
        let score = stats.stamina
            + stats.power * 1.5
            + stats.armor * 0.02
            + stats.resist * 0.2
            + (stats.crit + stats.hit + stats.dodge) * 3.0;
        (base + score * 0.4).round() as u32
    }

    /// Marks returned when salvaging an item (a third of its price, at least 1).
    pub fn salvage_value(item: &Item) -> u32 {
        (Profile::item_price(item.rarity, &item.stats) / 3).max(1)
    }

    /// Items the quartermaster sells: Common and Uncommon loot from every dungeon
    /// whose recommended level is at most one above the player's, plus Rare
    /// items once the player reaches that dungeon's level + 2.
    pub fn vendor_stock<'a>(
        &self,
        dungeons: impl IntoIterator<Item = &'a Content>,
    ) -> Vec<(String, crate::content::LootEntry, u32)> {
        let mut stock = Vec::new();
        for d in dungeons {
            let rec = d.dungeon.recommended_level;
            if rec > self.player_level + 1 {
                continue;
            }
            for l in &d.loot {
                let allowed = match l.rarity {
                    Rarity::Common | Rarity::Uncommon => true,
                    Rarity::Rare => self.player_level >= rec + 2,
                    Rarity::Epic => false,
                };
                if allowed {
                    let price = Profile::item_price(l.rarity, &l.stats);
                    stock.push((d.dungeon.name.clone(), l.clone(), price));
                }
            }
        }
        stock
    }

    /// Buy a vendor item into the stash.
    pub fn buy(
        &mut self,
        source: &str,
        entry: &crate::content::LootEntry,
        price: u32,
    ) -> Result<Item, String> {
        if self.marks < price {
            return Err(format!(
                "Not enough Vault Marks ({} of {price})",
                self.marks
            ));
        }
        self.marks -= price;
        let item = Item {
            uid: self.next_item_uid,
            base_id: entry.id.clone(),
            name: entry.name.clone(),
            slot: entry.slot,
            rarity: entry.rarity,
            role: entry.role,
            stats: entry.stats,
            source: format!("{source} (vendor)"),
        };
        self.next_item_uid += 1;
        self.stash.push(item.clone());
        Ok(item)
    }

    /// Salvage a stash item for marks.
    pub fn salvage(&mut self, stash_index: usize) -> Result<u32, String> {
        if stash_index >= self.stash.len() {
            return Err("no such item in stash".into());
        }
        let item = self.stash.remove(stash_index);
        let value = Profile::salvage_value(&item);
        self.marks += value;
        Ok(value)
    }

    pub fn default_path() -> PathBuf {
        let dir = std::env::var(SAVE_DIR_ENV_VAR).unwrap_or_else(|_| "saves".to_string());
        PathBuf::from(dir).join("profile.json")
    }

    pub fn save(&self, path: &Path) -> Result<(), ProfileError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| ProfileError::Io(e.to_string()))?;
        }
        let json =
            serde_json::to_string_pretty(self).map_err(|e| ProfileError::Io(e.to_string()))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| ProfileError::Io(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| ProfileError::Io(e.to_string()))?;
        Ok(())
    }

    /// Load the profile, or `Ok(None)` if no save exists yet.
    pub fn load(path: &Path) -> Result<Option<Profile>, ProfileError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(ProfileError::Io(e.to_string())),
        };
        Profile::from_json(&text)
            .map(|mut p| {
                p.ensure_roster();
                Some(p)
            })
            .map_err(|e| ProfileError::Corrupt(format!("{}: {e}", path.display())))
    }

    /// Parse a save, migrating version 1 files.
    pub fn from_json(text: &str) -> Result<Profile, String> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        match value.get("version").and_then(|v| v.as_u64()) {
            Some(1) => {
                let v1: v1::Profile = serde_json::from_value(value).map_err(|e| e.to_string())?;
                Ok(v1.migrate())
            }
            Some(2) => serde_json::from_value(value).map_err(|e| e.to_string()),
            other => Err(format!(
                "unsupported profile version {other:?} (expected {PROFILE_VERSION})"
            )),
        }
    }

    /// Load or create a default profile. A corrupt file is moved aside (never deleted)
    /// and the error message is returned alongside the fresh profile.
    pub fn load_or_default(path: &Path) -> (Profile, Option<String>) {
        match Profile::load(path) {
            Ok(Some(p)) => (p, None),
            Ok(None) => (Profile::new_default(), None),
            Err(e) => {
                let backup = path.with_extension("json.corrupt");
                let _ = std::fs::rename(path, &backup);
                (
                    Profile::new_default(),
                    Some(format!("{e}; moved to {}", backup.display())),
                )
            }
        }
    }
}

/// Prototype 0.1 save format, kept only for migration.
mod v1 {
    use super::*;

    #[derive(Deserialize)]
    pub struct Item {
        pub uid: u64,
        pub base_id: String,
        pub name: String,
        pub slot: ItemSlot,
        pub power: u32,
        pub stamina: u32,
    }

    #[derive(Deserialize, Default)]
    pub struct Equipment {
        pub weapon: Option<Item>,
        pub armor: Option<Item>,
    }

    #[derive(Deserialize)]
    pub struct Companion {
        pub id: String,
        pub name: String,
        pub class: ClassId,
        pub equipment: Equipment,
        pub runs_completed: u32,
    }

    #[derive(Deserialize)]
    pub struct Profile {
        pub player_name: String,
        pub player_class: ClassId,
        pub player_equipment: Equipment,
        pub marks: u32,
        pub runs_won: u32,
        pub runs_wiped: u32,
        pub next_item_uid: u64,
        pub stash: Vec<Item>,
        pub roster: Vec<Companion>,
        pub party: Vec<String>,
    }

    fn item(i: Item) -> super::Item {
        super::Item {
            uid: i.uid,
            base_id: i.base_id,
            name: i.name,
            slot: i.slot,
            rarity: Rarity::Uncommon,
            role: RoleAffinity::Any,
            stats: ItemStats {
                power: i.power as f32,
                stamina: i.stamina as f32,
                ..Default::default()
            },
            source: "The Sunken Vault".into(),
        }
    }

    fn equipment(e: Equipment) -> super::Equipment {
        let mut out = super::Equipment::default();
        for i in e.weapon.into_iter().chain(e.armor) {
            out.put(item(i));
        }
        out
    }

    impl Profile {
        pub fn migrate(self) -> super::Profile {
            let mut p = super::Profile::new_default();
            p.player_name = self.player_name;
            p.player_class = self.player_class;
            p.player_equipment = equipment(self.player_equipment);
            p.marks = self.marks;
            p.runs_won = self.runs_won;
            p.runs_wiped = self.runs_wiped;
            p.next_item_uid = self.next_item_uid;
            p.stash = self.stash.into_iter().map(item).collect();
            p.roster = self
                .roster
                .into_iter()
                .map(|c| super::Companion {
                    id: c.id,
                    name: c.name,
                    class: c.class,
                    equipment: equipment(c.equipment),
                    runs_completed: c.runs_completed,
                    level: 1,
                    xp: 0,
                    profile: AiProfile::Balanced,
                    clears: BTreeMap::new(),
                })
                .collect();
            p.party = self.party;
            p
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dungeon_sandbox_test_{name}_{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn victory_report(content: &Content, seed: u64) -> RunReport {
        RunReport {
            dungeon: content.dungeon.id.clone(),
            difficulty: "normal".into(),
            reward_multiplier: 1.0,
            rarity_bonus: 0,
            outcome: RunOutcome::Victory,
            cleared: content.encounters.iter().map(|e| e.id.clone()).collect(),
            kills: vec![(EnemyRank::Trash, 1), (EnemyRank::Boss, 3)],
            duration: 100.0,
            deaths: 0,
            seed,
        }
    }

    #[test]
    fn default_party_is_balanced_for_each_player_class() {
        for class in ClassId::ALL {
            let mut p = Profile::new_default();
            p.player_class = class;
            p.auto_fill_party();
            assert_eq!(p.party.len(), 4);
            assert!(
                p.party_warnings().is_empty(),
                "{class:?}: {:?}",
                p.party_warnings()
            );
        }
    }

    #[test]
    fn reward_survives_save_and_reload() {
        let content = Content::builtin();
        let path = temp_dir("reward").join("profile.json");
        let mut p = Profile::new_default();
        let rewards = p.grant_rewards(&content, &victory_report(&content, 99));
        assert!(!rewards.items.is_empty(), "victory drops loot");
        p.save(&path).unwrap();
        let loaded = Profile::load(&path).unwrap().expect("file exists");
        assert_eq!(loaded.marks, content.rewards.marks_per_clear);
        assert_eq!(loaded.runs_won, 1);
        for item in &rewards.items {
            assert!(loaded.stash.contains(item));
        }
        assert_eq!(loaded, p);
        assert_eq!(loaded.history.len(), 1);
    }

    #[test]
    fn each_cleared_loot_source_drops_one_item() {
        let content = Content::builtin();
        let mut p = Profile::new_default();
        let r = p.grant_rewards(&content, &victory_report(&content, 5));
        // Sunken Vault: the brute and the final boss have loot tables.
        assert_eq!(r.items.len(), 2);
    }

    #[test]
    fn wipes_give_no_clear_loot_but_keep_kill_xp() {
        let content = Content::builtin();
        let mut p = Profile::new_default();
        let mut report = victory_report(&content, 5);
        report.outcome = RunOutcome::Wipe;
        report.cleared.clear();
        let r = p.grant_rewards(&content, &report);
        assert!(r.items.is_empty());
        assert_eq!(r.marks, content.rewards.marks_per_wipe);
        assert_eq!(r.xp, 20 + 750);
        assert_eq!(p.runs_wiped, 1);
    }

    #[test]
    fn xp_levels_up_and_caps() {
        let (mut level, mut xp) = (1, 0);
        assert_eq!(add_xp(&mut level, &mut xp, 299), None);
        assert_eq!(add_xp(&mut level, &mut xp, 1), Some(2));
        assert_eq!(xp, 0);
        add_xp(&mut level, &mut xp, 1_000_000);
        assert_eq!(level, MAX_LEVEL);
        assert_eq!(xp, 0);
    }

    #[test]
    fn heroic_upgrades_rarity_and_scales_stats() {
        let content = Content::builtin();
        let mut p = Profile::new_default();
        let mut report = victory_report(&content, 11);
        report.rarity_bonus = 1;
        report.reward_multiplier = 2.0;
        let r = p.grant_rewards(&content, &report);
        for item in &r.items {
            let base = content.loot.iter().find(|l| l.id == item.base_id).unwrap();
            assert!(item.rarity > base.rarity || base.rarity == Rarity::Epic);
        }
        assert_eq!(r.marks, 2 * content.rewards.marks_per_clear);
    }

    #[test]
    fn equip_swaps_with_stash_and_auto_equip_upgrades() {
        let content = Content::builtin();
        let mut p = Profile::new_default();
        for seed in 1..6 {
            p.grant_rewards(&content, &victory_report(&content, seed));
        }
        let n = p.stash.len();
        let slot = p.stash[0].slot;
        p.equip(0, Wearer::Player).unwrap();
        assert!(p.player_equipment.get(slot).is_some());
        assert_eq!(p.stash.len(), n - 1);
        let changes = p.auto_equip();
        assert!(!changes.is_empty(), "some companion should find an upgrade");
        // Every equipped item suits its wearer's role.
        for c in p.party_companions() {
            for it in c.equipment.items() {
                assert!(it.role.suits(c.class.role()), "{} on {}", it.name, c.name);
            }
        }
    }

    #[test]
    fn v1_saves_are_migrated() {
        let v1 = r#"{
          "version": 1, "player_name": "Old", "player_class": "warden",
          "player_equipment": { "weapon": { "uid": 1, "base_id": "x", "name": "Old Blade", "slot": "weapon", "power": 8, "stamina": 0 }, "armor": null },
          "marks": 30, "runs_won": 3, "runs_wiped": 1, "next_item_uid": 4,
          "stash": [ { "uid": 2, "base_id": "y", "name": "Old Mail", "slot": "armor", "power": 0, "stamina": 30 } ],
          "roster": [ { "id": "kael", "name": "Kael", "class": "striker", "equipment": { "weapon": null, "armor": null }, "runs_completed": 3 } ],
          "party": ["kael"]
        }"#;
        let p = Profile::from_json(v1).expect("migrates");
        assert_eq!(p.version, PROFILE_VERSION);
        assert_eq!(p.player_name, "Old");
        assert_eq!(p.marks, 30);
        assert_eq!(
            p.player_equipment
                .get(ItemSlot::MainHand)
                .unwrap()
                .stats
                .power,
            8.0
        );
        assert_eq!(p.stash[0].slot, ItemSlot::Chest);
        assert_eq!(p.roster[0].level, 1);
    }

    #[test]
    fn vendor_sells_unlocked_items_and_salvage_refunds_a_third() {
        let vault = Content::builtin();
        let warrens = Content::builtin_warrens();
        let mut p = Profile::new_default();
        // Level 1: only the Vault's common/uncommon items are for sale.
        let stock = p.vendor_stock([&vault, &warrens]);
        assert!(!stock.is_empty());
        assert!(stock.iter().all(|(src, l, _)| src == "The Sunken Vault"
            && matches!(l.rarity, Rarity::Common | Rarity::Uncommon)));
        p.player_level = 5;
        let stock = p.vendor_stock([&vault, &warrens]);
        assert!(
            stock
                .iter()
                .any(|(src, _, _)| src == "The Gloomroot Warrens")
        );
        assert!(stock.iter().all(|(_, l, _)| l.rarity != Rarity::Epic));
        let (src, entry, price) = stock[0].clone();
        assert!(p.buy(&src, &entry, price).is_err(), "no marks yet");
        p.marks = price + 5;
        let item = p.buy(&src, &entry, price).unwrap();
        assert_eq!(p.marks, 5);
        assert!(p.stash.contains(&item));
        let idx = p.stash.iter().position(|i| *i == item).unwrap();
        let back = p.salvage(idx).unwrap();
        assert_eq!(back, (price / 3).max(1));
        assert_eq!(p.marks, 5 + back);
        assert!(p.stash.is_empty());
    }

    #[test]
    fn corrupt_save_is_reported_and_backed_up() {
        let dir = temp_dir("corrupt");
        let path = dir.join("profile.json");
        std::fs::write(&path, "{ not json").unwrap();
        let (p, err) = Profile::load_or_default(&path);
        assert!(err.is_some());
        assert_eq!(p.runs_won, 0);
        assert!(dir.join("profile.json.corrupt").exists());
    }
}
