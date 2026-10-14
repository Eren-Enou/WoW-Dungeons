//! Persistent player data: roster of companions, equipment, stash and currency.
//!
//! Saved as pretty JSON. Writes go to a temporary file which is then renamed
//! over the real one so a crash mid-save cannot corrupt the profile.

use crate::class::{ClassId, Role};
use crate::content::Content;
use crate::rng::Rng;
use crate::sim::{PartyMemberSpec, RunOutcome};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

pub const PROFILE_VERSION: u32 = 1;
pub const SAVE_DIR_ENV_VAR: &str = "DUNGEON_SAVE_DIR";
pub const PARTY_COMPANIONS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemSlot {
    Weapon,
    Armor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Unique instance id within this profile.
    pub uid: u64,
    /// Loot table id this item came from.
    pub base_id: String,
    pub name: String,
    pub slot: ItemSlot,
    pub power: u32,
    pub stamina: u32,
}

impl Item {
    pub fn describe(&self) -> String {
        let mut stats = Vec::new();
        if self.power > 0 {
            stats.push(format!("+{} power", self.power));
        }
        if self.stamina > 0 {
            stats.push(format!("+{} stamina", self.stamina));
        }
        format!("{} ({:?}: {})", self.name, self.slot, stats.join(", "))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Equipment {
    pub weapon: Option<Item>,
    pub armor: Option<Item>,
}

impl Equipment {
    pub fn power(&self) -> u32 {
        self.items().map(|i| i.power).sum()
    }

    pub fn stamina(&self) -> u32 {
        self.items().map(|i| i.stamina).sum()
    }

    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.weapon.iter().chain(self.armor.iter())
    }

    pub fn slot_mut(&mut self, slot: ItemSlot) -> &mut Option<Item> {
        match slot {
            ItemSlot::Weapon => &mut self.weapon,
            ItemSlot::Armor => &mut self.armor,
        }
    }

    pub fn slot(&self, slot: ItemSlot) -> &Option<Item> {
        match slot {
            ItemSlot::Weapon => &self.weapon,
            ItemSlot::Armor => &self.armor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Companion {
    pub id: String,
    pub name: String,
    pub class: ClassId,
    pub equipment: Equipment,
    pub runs_completed: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    pub player_name: String,
    pub player_class: ClassId,
    pub player_equipment: Equipment,
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
}

/// Who a piece of gear is being equipped on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wearer {
    Player,
    Companion(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunRewards {
    pub marks: u32,
    pub item: Option<Item>,
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

impl Profile {
    pub fn new_default() -> Self {
        let roster = [
            ("brannoc", "Brannoc", ClassId::Warden),
            ("ysolde", "Ysolde", ClassId::Warden),
            ("thessaly", "Thessaly", ClassId::Mender),
            ("orrin", "Orrin", ClassId::Mender),
            ("kael", "Kael", ClassId::Striker),
            ("dovic", "Dovic", ClassId::Striker),
            ("mirelle", "Mirelle", ClassId::Arcanist),
            ("senna", "Senna", ClassId::Arcanist),
        ]
        .into_iter()
        .map(|(id, name, class)| Companion {
            id: id.to_string(),
            name: name.to_string(),
            class,
            equipment: Equipment::default(),
            runs_completed: 0,
        })
        .collect();
        let mut p = Profile {
            version: PROFILE_VERSION,
            player_name: "Wayfarer".to_string(),
            player_class: ClassId::Striker,
            player_equipment: Equipment::default(),
            marks: 0,
            runs_won: 0,
            runs_wiped: 0,
            next_item_uid: 1,
            stash: Vec::new(),
            roster,
            party: Vec::new(),
        };
        p.auto_fill_party();
        p
    }

    /// Recruit a balanced party around the player's role: one tank, one healer,
    /// the rest damage dealers (melee and ranged alternating).
    pub fn auto_fill_party(&mut self) {
        let player_role = self.player_class.role();
        let mut needed: Vec<Role> = Vec::new();
        if player_role != Role::Tank {
            needed.push(Role::Tank);
        }
        if player_role != Role::Healer {
            needed.push(Role::Healer);
        }
        let mut dps_cycle = [Role::RangedDps, Role::MeleeDps].into_iter().cycle();
        while needed.len() < PARTY_COMPANIONS {
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
        // Fill any gaps with whoever is left.
        for c in &self.roster {
            if chosen.len() >= PARTY_COMPANIONS {
                break;
            }
            if !chosen.contains(&c.id) {
                chosen.push(c.id.clone());
            }
        }
        self.party = chosen;
    }

    /// Toggle a companion in or out of the party. Returns false if the party is full.
    pub fn toggle_recruit(&mut self, roster_index: usize) -> bool {
        let Some(c) = self.roster.get(roster_index) else {
            return false;
        };
        if let Some(pos) = self.party.iter().position(|id| *id == c.id) {
            self.party.remove(pos);
            true
        } else if self.party.len() < PARTY_COMPANIONS {
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
        if self.party.len() < PARTY_COMPANIONS {
            w.push(format!(
                "Party has {} of {} companions",
                self.party.len(),
                PARTY_COMPANIONS
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
        let mut specs = vec![PartyMemberSpec {
            name: self.player_name.clone(),
            class: self.player_class,
            power: self.player_equipment.power(),
            stamina: self.player_equipment.stamina(),
            player_controlled,
        }];
        for c in self.party_companions() {
            specs.push(PartyMemberSpec {
                name: c.name.clone(),
                class: c.class,
                power: c.equipment.power(),
                stamina: c.equipment.stamina(),
                player_controlled: false,
            });
        }
        specs
    }

    fn equipment_mut(&mut self, who: Wearer) -> Option<&mut Equipment> {
        match who {
            Wearer::Player => Some(&mut self.player_equipment),
            Wearer::Companion(i) => self.roster.get_mut(i).map(|c| &mut c.equipment),
        }
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
        let slot = item.slot;
        let eq = self.equipment_mut(who).expect("checked above");
        let old = eq.slot_mut(slot).replace(item);
        if let Some(old) = old {
            self.stash.push(old);
        }
        Ok(())
    }

    /// Roll and grant rewards for a finished run. Deterministic for a given seed.
    pub fn grant_rewards(
        &mut self,
        content: &Content,
        outcome: RunOutcome,
        seed: u64,
    ) -> RunRewards {
        match outcome {
            RunOutcome::Victory => {
                self.runs_won += 1;
                self.marks += content.rewards.marks_per_clear;
                let party_ids = self.party.clone();
                for c in self.roster.iter_mut().filter(|c| party_ids.contains(&c.id)) {
                    c.runs_completed += 1;
                }
                let mut rng = Rng::new(seed ^ 0x10_07);
                let total: u32 = content.loot.iter().map(|l| l.weight).sum();
                let mut roll = (rng.next_u64() % total as u64) as u32;
                let entry = content
                    .loot
                    .iter()
                    .find(|l| {
                        if roll < l.weight {
                            true
                        } else {
                            roll -= l.weight;
                            false
                        }
                    })
                    .expect("weights sum to total");
                let item = Item {
                    uid: self.next_item_uid,
                    base_id: entry.id.clone(),
                    name: entry.name.clone(),
                    slot: entry.slot,
                    power: entry.power,
                    stamina: entry.stamina,
                };
                self.next_item_uid += 1;
                self.stash.push(item.clone());
                RunRewards {
                    marks: content.rewards.marks_per_clear,
                    item: Some(item),
                }
            }
            RunOutcome::Wipe | RunOutcome::Timeout | RunOutcome::Abandoned => {
                self.runs_wiped += 1;
                self.marks += content.rewards.marks_per_wipe;
                RunRewards {
                    marks: content.rewards.marks_per_wipe,
                    item: None,
                }
            }
            RunOutcome::InProgress => RunRewards {
                marks: 0,
                item: None,
            },
        }
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
        let p: Profile = serde_json::from_str(&text)
            .map_err(|e| ProfileError::Corrupt(format!("{}: {e}", path.display())))?;
        if p.version != PROFILE_VERSION {
            return Err(ProfileError::Corrupt(format!(
                "unsupported profile version {} (expected {PROFILE_VERSION})",
                p.version
            )));
        }
        Ok(Some(p))
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
        let rewards = p.grant_rewards(&content, RunOutcome::Victory, 99);
        let item = rewards.item.expect("victory drops an item");
        p.save(&path).unwrap();

        let loaded = Profile::load(&path).unwrap().expect("file exists");
        assert_eq!(loaded.marks, content.rewards.marks_per_clear);
        assert_eq!(loaded.runs_won, 1);
        assert!(loaded.stash.contains(&item));
        assert_eq!(loaded, p);
    }

    #[test]
    fn equip_swaps_with_stash() {
        let content = Content::builtin();
        let mut p = Profile::new_default();
        p.grant_rewards(&content, RunOutcome::Victory, 1);
        p.grant_rewards(&content, RunOutcome::Victory, 2);
        let first_slot = p.stash[0].slot;
        p.equip(0, Wearer::Player).unwrap();
        assert!(p.player_equipment.slot(first_slot).is_some());
        assert_eq!(p.stash.len(), 1);
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
