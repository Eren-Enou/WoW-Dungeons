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
                max_width: px(1100),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 1.0)),
            children![(Text::new(text), text_font(15.0))],
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
    let mut out = format!(
        "{title} - {}\n\nDuration {:.0}s | encounters cleared {}/{} | party deaths {} | encounter resets {} | seed {}\n",
        r.dungeon_name,
        s.duration,
        s.encounters_cleared,
        s.encounters_total,
        s.party_deaths,
        s.encounter_resets,
        s.seed
    );
    if let Some(t) = s.phase2_at {
        out.push_str(&format!(
            "Boss phase 2 reached at {t:.0}s | Cinder Rings cast {} (party members hit: {})\n",
            s.ring_casts, s.ring_hits
        ));
    }
    out.push_str("\nREWARDS\n");
    out.push_str(&format!("  +{} Vault Marks\n", r.rewards.marks));
    match &r.rewards.item {
        Some(item) => out.push_str(&format!(
            "  Loot: {} - added to your stash (equip it in the hub)\n",
            item.describe()
        )),
        None => out.push_str("  No loot (the boss must be defeated)\n"),
    }
    out.push_str("  Progress saved.\n\nPARTY\n");
    out.push_str(&format!(
        "  {:<10} {:<9} {:>8} {:>7} {:>8} {:>7} {:>8} {:>6}\n",
        "name", "class", "damage", "dps", "healing", "hps", "taken", "deaths"
    ));
    for m in &s.members {
        out.push_str(&format!(
            "  {:<10} {:<9} {:>8.0} {:>7.1} {:>8.0} {:>7.1} {:>8.0} {:>6}\n",
            m.name, m.class, m.damage_done, m.dps, m.healing_done, m.hps, m.damage_taken, m.deaths
        ));
    }
    out.push_str("\nKEY MOMENTS\n");
    let n = r.highlights.len();
    for h in r.highlights.iter().skip(n.saturating_sub(14)) {
        out.push_str(&format!("  {h}\n"));
    }
    if let Some(p) = &r.log_path {
        out.push_str(&format!("\nFull combat log written to {p}\n"));
    }
    out.push_str("\nPress Enter to return to the hub");
    out
}

fn results_input(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<AppState>>) {
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        next.set(AppState::Hub);
    }
}
