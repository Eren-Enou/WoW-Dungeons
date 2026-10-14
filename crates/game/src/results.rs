//! Results screen: outcome, per-member metrics, rewards and combat highlights.

use crate::common::{AppState, LastResult, text_font};
use bevy::prelude::*;
use dungeon_sim::RunOutcome;

pub struct ResultsPlugin;

impl Plugin for ResultsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Results), spawn_results)
            .add_systems(Update, results_input.run_if(in_state(AppState::Results)));
    }
}

fn spawn_results(mut commands: Commands, result: Option<Res<LastResult>>) {
    let scoped = DespawnOnExit(AppState::Results);
    let text = match result {
        Some(r) => format_results(&r),
        None => "No result recorded.".to_string(),
    };
    commands.spawn((
        Node {
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.05, 0.96)),
        scoped,
        children![(
            Node {
                padding: UiRect::all(px(20)),
                max_width: px(1200),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 1.0)),
            children![(Text::new(text), text_font(14.0))],
        )],
    ));
}

fn format_results(r: &LastResult) -> String {
    let s = &r.summary;
    let title = match s.outcome {
        RunOutcome::Victory => "VICTORY",
        RunOutcome::Wipe => "WIPE",
        RunOutcome::Timeout => "TIME OUT",
        RunOutcome::Abandoned => "ABANDONED",
        RunOutcome::InProgress => "IN PROGRESS",
    };
    let mut lines = vec![
        format!("{title} - {} [{}]", r.dungeon_name, r.difficulty_name),
        String::new(),
        format!(
            "Duration {:.0}s | encounters cleared {}/{} | enemies killed {} | party deaths {} | resets {} | seed {}",
            s.duration,
            s.encounters_cleared,
            s.encounters_total,
            s.enemies_killed,
            s.party_deaths,
            s.encounter_resets,
            s.seed
        ),
        format!(
            "Interrupts {}/{} | threat failures {} | avoidable damage taken {:.0} | crowd control {} (broken {}) | dispels {} | overheal {:.0}%",
            s.interrupted_casts,
            s.interruptible_casts,
            s.threat_failures,
            s.avoidable_damage,
            s.cc_applied,
            s.cc_broken,
            s.dispels,
            s.overheal_pct
        ),
    ];
    if let Some(t) = s.phase2_at {
        lines.push(format!(
            "Final boss phase 2 at {t:.0}s | ground effects cast {} (party hits {})",
            s.ring_casts, s.ring_hits
        ));
    }
    lines.push(String::new());
    lines.push("REWARDS".into());
    lines.push(format!(
        "  +{} Vault Marks   +{} experience",
        r.rewards.marks, r.rewards.xp
    ));
    for (name, level) in &r.rewards.level_ups {
        lines.push(format!("  LEVEL UP: {name} reaches level {level}!"));
    }
    if r.rewards.items.is_empty() {
        lines.push(
            "  No loot (bosses and elites drop items when their encounter is cleared)".into(),
        );
    }
    for item in &r.rewards.items {
        lines.push(format!("  Loot: {}", item.describe()));
    }
    lines.push("  Progress saved. Equip new items in the hub (Character tab).".into());
    lines.push(String::new());
    lines.push("PARTY".into());
    lines.push(format!(
        "  {:<10} {:<9} {:>8} {:>7} {:>8} {:>7} {:>8} {:>9} {:>5} {:>6}",
        "name", "class", "damage", "dps", "healing", "hps", "taken", "avoidable", "kicks", "deaths"
    ));
    for m in &s.members {
        lines.push(format!(
            "  {:<10} {:<9} {:>8.0} {:>7.1} {:>8.0} {:>7.1} {:>8.0} {:>9.0} {:>5} {:>6}",
            m.name,
            m.class,
            m.damage_done,
            m.dps,
            m.healing_done + m.absorb_done,
            m.hps,
            m.damage_taken,
            m.avoidable_taken,
            m.interrupts,
            m.deaths
        ));
    }
    lines.push(String::new());
    lines.push("KEY MOMENTS".into());
    let n = r.highlights.len();
    for h in r.highlights.iter().skip(n.saturating_sub(12)) {
        lines.push(format!("  {h}"));
    }
    if let Some(p) = &r.log_path {
        lines.push(String::new());
        lines.push(format!("Full combat log written to {p}"));
    }
    lines.push(String::new());
    lines.push("Press Enter to return to the hub".into());
    lines.join("\n")
}

fn results_input(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        next.set(AppState::Hub);
    }
}
