//! 0.11 "One More Run" experiment (DUNGEON_FUN_0_11.md): the saved,
//! resettable state of the experiment, a summary of what the player did in a
//! run, and the results-screen lines that turn it into reasons to run again
//! (unlocks, personal bests, and what to try next time).
//!
//! The experiment is opt-in (hub Sandbox tab). With it off nothing here runs
//! and the game plays exactly as before.

use crate::class::{StrikerStyle, Technique};
use crate::sim::{RunOutcome, Sim};
use serde::{Deserialize, Serialize};

/// The dungeon the experiment is built around.
pub const EXPERIMENT_DUNGEON: &str = "gloomroot_warrens";

/// Saved with the profile (`Profile::experiment`); old saves load with the
/// defaults (off, nothing unlocked).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Experiment {
    /// The experiment is on: Brood Frenzy, the Flow Striker by default,
    /// Techniques and the run summary.
    pub enabled: bool,
    /// Techniques are unlocked by clearing the Gloomroot Warrens with the
    /// experiment on.
    pub unlocked: bool,
    /// The Technique in use (Flow Striker only); swappable in the hub.
    pub technique: Option<Technique>,
    /// Gloomroot clears with the experiment on.
    pub clears: u32,
    /// Fastest Gloomroot clear with the experiment on (seconds).
    pub best_clear: Option<f32>,
    /// Most Brood Frenzy channels broken in one run.
    pub best_breaks: u32,
}

impl Experiment {
    /// Forget everything earned (unlock, Technique, bests); keep on/off.
    pub fn reset(&mut self) {
        *self = Experiment {
            enabled: self.enabled,
            ..Default::default()
        };
    }

    /// The Technique for the next run, if one is chosen and unlocked.
    pub fn active_technique(&self) -> Option<Technique> {
        if self.enabled && self.unlocked {
            self.technique
        } else {
            None
        }
    }

    /// Record a finished run; returns news for the results screen.
    pub fn record(&mut self, run: &ExperimentRun) -> Vec<String> {
        let mut news = Vec::new();
        if !self.enabled || run.dungeon != EXPERIMENT_DUNGEON {
            return news;
        }
        if run.breaks > self.best_breaks {
            if self.best_breaks > 0 || run.won {
                news.push(format!(
                    "New best: {} Brood Frenzy channel(s) broken in one run",
                    run.breaks
                ));
            }
            self.best_breaks = run.breaks;
        }
        if !run.won {
            return news;
        }
        self.clears += 1;
        match self.best_clear {
            Some(best) if run.duration < best => news.push(format!(
                "New best clear: {} (was {})",
                clock(run.duration),
                clock(best)
            )),
            Some(best) => news.push(format!(
                "Clear time {} (best {})",
                clock(run.duration),
                clock(best)
            )),
            None => news.push(format!("First clear: {}", clock(run.duration))),
        }
        self.best_clear = Some(
            self.best_clear
                .map_or(run.duration, |b| b.min(run.duration)),
        );
        if !self.unlocked {
            self.unlocked = true;
            news.push(
                "TECHNIQUES UNLOCKED: choose Riposte (react) or Patience (plan) on the hub's Sandbox tab - they change how the Flow Striker plays".into(),
            );
        }
        news
    }
}

/// What the player did in one run, for the summary and the records.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExperimentRun {
    pub dungeon: String,
    pub won: bool,
    pub duration: f32,
    pub style: StrikerStyle,
    pub technique: Option<Technique>,
    /// Brood Frenzy channels started, and broken.
    pub channels: u32,
    pub breaks: u32,
    /// Openings that appeared for you, and Seizes/Exploits used.
    pub openings: u32,
    pub seized: u32,
    pub edge_wasted: u32,
    pub kicks: u32,
    /// Ravages (Flow slot 2).
    pub ravages: u32,
}

impl ExperimentRun {
    pub fn from_sim(sim: &Sim) -> ExperimentRun {
        let Some(p) = sim.player else {
            return ExperimentRun::default();
        };
        let m = &sim.metrics.actors[p];
        let style = sim.actors[p].style;
        ExperimentRun {
            dungeon: sim.content.dungeon.id.clone(),
            won: sim.outcome == RunOutcome::Victory,
            duration: sim.ended_at.unwrap_or(sim.time),
            style,
            technique: sim.technique(p),
            channels: sim.metrics.stagger_casts,
            breaks: sim.metrics.staggers,
            openings: m.openings,
            seized: m.exploits,
            edge_wasted: m.edge_wasted,
            kicks: m.interrupts,
            ravages: if style == StrikerStyle::Flow {
                m.ability_uses[1]
            } else {
                0
            },
        }
    }

    /// The run in a few lines, each with what to try next time. Facts only
    /// come from the run; the advice names a concrete thing to do.
    pub fn coach(&self, unlocked: bool) -> Vec<String> {
        let mut out = Vec::new();
        if self.channels > 0 {
            if self.breaks >= self.channels {
                out.push(format!(
                    "Brood Frenzy: broke {} of {} - she never got her brood.",
                    self.breaks, self.channels
                ));
            } else {
                out.push(format!(
                    "Brood Frenzy: broke {} of {}. Next time: when 'Brood Frenzy in' counts down on her frame, keep 4+ Edge and 25 energy, then Ravage and Seize her - they count triple toward the break.",
                    self.breaks, self.channels
                ));
            }
        }
        if matches!(self.style, StrikerStyle::Flow | StrikerStyle::Reactive) && self.openings > 0 {
            let pct = 100 * self.seized / self.openings.max(1);
            let verb = if self.style == StrikerStyle::Flow {
                "seized"
            } else {
                "exploited"
            };
            if pct < 50 {
                out.push(format!(
                    "Openings: {verb} {} of {} ({pct}%). Watch for the gold marker and the 'Opening:' line; there are more than you can take, so pick the ones next to you.",
                    self.seized, self.openings
                ));
            } else {
                out.push(format!(
                    "Openings: {verb} {} of {} ({pct}%).",
                    self.seized, self.openings
                ));
            }
        }
        if self.style == StrikerStyle::Flow && self.edge_wasted >= 8 {
            out.push(format!(
                "Edge lost at the cap: {}. Ravage before a Seize or a Quick Cut from behind would overflow it.",
                self.edge_wasted
            ));
        }
        match (self.style, self.technique) {
            (StrikerStyle::Flow, Some(Technique::Riposte)) => out.push(format!(
                "Riposte: {} Kick(s) landed. Each one doubles your next Seize within 5 s - casters are your friends.",
                self.kicks
            )),
            (StrikerStyle::Flow, Some(Technique::Patience)) => out.push(
                "Patience: build to 6+ Edge on packs - Ravage then hits every enemy within 8 m and refunds 25 energy.".into(),
            ),
            (StrikerStyle::Flow, None) if unlocked => out.push(
                "Try a Technique next run (Sandbox tab): Riposte rewards Kicks, Patience big Ravages.".into(),
            ),
            (StrikerStyle::Flow, None) => {}
            _ => out.push(
                "This experiment is built around the Striker's C: Flow style (Sandbox tab).".into(),
            ),
        }
        out
    }
}

/// Seconds as m:ss.
pub fn clock(secs: f32) -> String {
    let s = secs.max(0.0).round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(won: bool, duration: f32, breaks: u32) -> ExperimentRun {
        ExperimentRun {
            dungeon: EXPERIMENT_DUNGEON.into(),
            won,
            duration,
            style: StrikerStyle::Flow,
            channels: 3,
            breaks,
            openings: 40,
            seized: 10,
            ..Default::default()
        }
    }

    #[test]
    fn a_first_clear_unlocks_techniques_and_sets_a_best() {
        let mut x = Experiment {
            enabled: true,
            ..Default::default()
        };
        // A wipe unlocks nothing.
        x.record(&run(false, 200.0, 0));
        assert!(!x.unlocked && x.clears == 0);
        let news = x.record(&run(true, 300.0, 1));
        assert!(x.unlocked && x.clears == 1);
        assert_eq!(x.best_clear, Some(300.0));
        assert!(
            news.iter().any(|n| n.starts_with("TECHNIQUES UNLOCKED")),
            "{news:?}"
        );
        assert!(
            news.iter().any(|n| n.starts_with("First clear: 5:00")),
            "{news:?}"
        );
        let news = x.record(&run(true, 280.0, 2));
        assert!(
            news.iter()
                .any(|n| n.contains("New best clear: 4:40 (was 5:00)"))
        );
        assert!(news.iter().any(|n| n.contains("2 Brood Frenzy")));
        assert!(!news.iter().any(|n| n.starts_with("TECHNIQUES")));
        // Other dungeons and a disabled experiment record nothing.
        let mut other = run(true, 10.0, 0);
        other.dungeon = "sunken_vault".into();
        assert!(x.record(&other).is_empty());
        x.enabled = false;
        assert!(x.record(&run(true, 10.0, 3)).is_empty());
    }

    #[test]
    fn reset_forgets_progress_but_keeps_the_switch() {
        let mut x = Experiment {
            enabled: true,
            unlocked: true,
            technique: Some(Technique::Patience),
            clears: 3,
            best_clear: Some(250.0),
            best_breaks: 2,
        };
        assert_eq!(x.active_technique(), Some(Technique::Patience));
        x.reset();
        assert_eq!(
            x,
            Experiment {
                enabled: true,
                ..Default::default()
            }
        );
        assert_eq!(x.active_technique(), None);
    }

    #[test]
    fn coaching_names_something_to_try() {
        let r = run(true, 300.0, 1);
        let lines = r.coach(true);
        assert!(lines[0].starts_with("Brood Frenzy: broke 1 of 3. Next time"));
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("Openings: seized 10 of 40 (25%)"))
        );
        assert!(lines.iter().any(|l| l.starts_with("Try a Technique")));
        let mut all = run(true, 300.0, 3);
        all.technique = Some(Technique::Riposte);
        all.kicks = 4;
        let lines = all.coach(true);
        assert!(lines[0].contains("never got her brood"));
        assert!(lines.iter().any(|l| l.starts_with("Riposte: 4 Kick(s)")));
        let mut classic = run(true, 300.0, 0);
        classic.style = StrikerStyle::Classic;
        assert!(
            classic
                .coach(false)
                .iter()
                .any(|l| l.contains("C: Flow style"))
        );
        assert_eq!(clock(272.4), "4:32");
    }

    #[test]
    fn old_saves_load_with_the_experiment_off() {
        let x: Experiment = serde_json::from_str("{}").unwrap();
        assert_eq!(x, Experiment::default());
        let x: Experiment =
            serde_json::from_str(r#"{"enabled":true,"technique":"riposte"}"#).unwrap();
        assert!(x.enabled && x.technique == Some(Technique::Riposte) && !x.unlocked);
    }
}
