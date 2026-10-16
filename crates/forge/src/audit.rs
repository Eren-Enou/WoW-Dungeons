//! `dungeon-forge audit` (0.9): generate many layouts of a blueprint and
//! report what every branch is for and whether it delivers it. The 0.8.1
//! branch review was a scratch script; this makes it repeatable and lets
//! tests fail on purposeless or broken branches.

use crate::blueprint::Blueprint;
use crate::build;
use crate::kit::Kit;
use crate::layout::BranchKind;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize)]
pub struct BranchAudit {
    pub id: String,
    pub purpose: String,
    pub from: String,
    pub to: Option<String>,
    /// Rooms in the branch (not counting corridors).
    pub rooms: usize,
    pub delivered: bool,
    /// Walking lengths on the navigation (through routes).
    pub branch_len: Option<f32>,
    pub main_len: Option<f32>,
    pub ratio: Option<f32>,
    /// Flank: degrees off the pack's front.
    pub angle: Option<f32>,
    pub rewards: Vec<String>,
    pub bypasses: Vec<String>,
    pub facts: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutAudit {
    pub seed: u64,
    pub ok: bool,
    pub error: Option<String>,
    pub attempt: u32,
    pub signature: String,
    pub branches: Vec<BranchAudit>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PurposeStats {
    pub count: usize,
    pub delivered: usize,
    pub with_reward: usize,
    pub bypassing_required: usize,
    pub ratio_min: Option<f32>,
    pub ratio_mean: Option<f32>,
    pub ratio_max: Option<f32>,
    pub angle_min: Option<f32>,
    pub angle_mean: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Audit {
    pub blueprint: String,
    pub layouts: usize,
    pub generated: usize,
    pub distinct_shapes: usize,
    pub purposes: BTreeMap<String, PurposeStats>,
    /// Branches with no declared purpose (legacy alternates and empty rooms).
    pub purposeless: usize,
    /// Side branches that end in an empty room nobody asked for.
    pub empty_dead_ends: usize,
    /// Design failures, each naming the seed and branch.
    pub problems: Vec<String>,
    pub per_layout: Vec<LayoutAudit>,
}

impl Audit {
    /// Nothing purposeless, broken or failed.
    pub fn clean(&self) -> bool {
        self.problems.is_empty()
    }
}

/// The main-route turns, branch purposes and shortcut styles of a layout.
pub fn signature(plan: &crate::layout::Plan) -> String {
    let main: String = plan
        .main
        .windows(2)
        .filter_map(|w| {
            plan.corridors
                .iter()
                .find(|c| c.a == w[0] && c.b == w[1])
                .map(|c| format!("{:?}", c.dir))
        })
        .collect();
    let branches: Vec<String> = plan
        .branches
        .iter()
        .map(|b| {
            let at = plan.main.iter().position(|&m| m == b.from).unwrap_or(0);
            format!("{}@{at}", b.kind.label())
        })
        .collect();
    format!("{main} | {}", branches.join(" "))
}

pub fn audit(bp: &Blueprint, kit: &Kit, seeds: impl IntoIterator<Item = u64>) -> Audit {
    let mut per_layout = Vec::new();
    let mut shapes = BTreeSet::new();
    let mut problems = Vec::new();
    let mut purposes: BTreeMap<String, Vec<BranchAudit>> = BTreeMap::new();
    for seed in seeds {
        match build::generate(bp, kit, seed) {
            Ok(g) => {
                let sig = signature(&g.plan);
                shapes.insert(sig.clone());
                let mut branches = Vec::new();
                for (k, (b, m)) in g.plan.branches.iter().zip(&g.measured).enumerate() {
                    let a = BranchAudit {
                        id: format!("b{k}"),
                        purpose: b.kind.label().into(),
                        from: format!("r{} '{}'", b.from, g.plan.rooms[b.from].name),
                        to: b.to.map(|t| format!("r{t} '{}'", g.plan.rooms[t].name)),
                        rooms: b.rooms.len(),
                        delivered: m.delivered,
                        branch_len: m.branch_len,
                        main_len: m.main_len,
                        ratio: m
                            .branch_len
                            .zip(m.main_len)
                            .map(|(bl, ml)| bl / ml.max(1.0)),
                        angle: m.angle,
                        rewards: m.rewards.clone(),
                        bypasses: m.bypasses.clone(),
                        facts: m.facts.clone(),
                        warnings: m.warnings.clone(),
                    };
                    let tag = format!("seed {seed} {} ({})", a.id, a.purpose);
                    if b.kind == BranchKind::Unspecified {
                        problems.push(format!(
                            "{tag}: no declared purpose{}",
                            if b.to.is_none() {
                                " (an empty dead end)"
                            } else if !a.bypasses.is_empty() {
                                " (goes round required encounters, then they must be cleared anyway)"
                            } else {
                                ""
                            }
                        ));
                    } else if !a.delivered && (bp.purposeful() || b.kind != BranchKind::Drop) {
                        problems.push(format!("{tag}: does not deliver its purpose"));
                    }
                    for w in &a.warnings {
                        if b.kind != BranchKind::Unspecified {
                            problems.push(format!("{tag}: {w}"));
                        }
                    }
                    purposes
                        .entry(a.purpose.clone())
                        .or_default()
                        .push(a.clone());
                    branches.push(a);
                }
                per_layout.push(LayoutAudit {
                    seed,
                    ok: true,
                    error: None,
                    attempt: g.plan.attempt,
                    signature: sig,
                    branches,
                });
            }
            Err(e) => {
                let first = e.lines().next().unwrap_or("").to_string();
                problems.push(format!("seed {seed}: could not generate: {first}"));
                per_layout.push(LayoutAudit {
                    seed,
                    ok: false,
                    error: Some(e),
                    attempt: 0,
                    signature: String::new(),
                    branches: Vec::new(),
                });
            }
        }
    }
    let stats = purposes
        .iter()
        .map(|(k, v)| {
            let ratios: Vec<f32> = v.iter().filter_map(|a| a.ratio).collect();
            let angles: Vec<f32> = v.iter().filter_map(|a| a.angle).collect();
            let mean =
                |xs: &[f32]| (!xs.is_empty()).then(|| xs.iter().sum::<f32>() / xs.len() as f32);
            (
                k.clone(),
                PurposeStats {
                    count: v.len(),
                    delivered: v.iter().filter(|a| a.delivered).count(),
                    with_reward: v.iter().filter(|a| !a.rewards.is_empty()).count(),
                    bypassing_required: v.iter().filter(|a| !a.bypasses.is_empty()).count(),
                    ratio_min: ratios.iter().copied().reduce(f32::min),
                    ratio_mean: mean(&ratios),
                    ratio_max: ratios.iter().copied().reduce(f32::max),
                    angle_min: angles.iter().copied().reduce(f32::min),
                    angle_mean: mean(&angles),
                },
            )
        })
        .collect();
    let all: Vec<&BranchAudit> = per_layout.iter().flat_map(|l| &l.branches).collect();
    Audit {
        blueprint: bp.blueprint.id.clone(),
        layouts: per_layout.len(),
        generated: per_layout.iter().filter(|l| l.ok).count(),
        distinct_shapes: shapes.len(),
        purposes: stats,
        purposeless: all.iter().filter(|a| a.purpose == "unspecified").count(),
        empty_dead_ends: all
            .iter()
            .filter(|a| a.purpose == "unspecified" && a.to.is_none())
            .count(),
        problems,
        per_layout,
    }
}

/// A readable summary.
pub fn text(a: &Audit) -> String {
    let mut s = format!(
        "audit of '{}': {}/{} layouts generated, {} distinct shapes\n",
        a.blueprint, a.generated, a.layouts, a.distinct_shapes
    );
    s.push_str(&format!(
        "{:<12} {:>5} {:>9} {:>7} {:>9} {:>22} {:>14}\n",
        "purpose",
        "count",
        "delivered",
        "reward",
        "bypasses",
        "walk vs main min/mean/max",
        "angle min/mean"
    ));
    let f = |v: Option<f32>| v.map(|x| format!("{x:.2}")).unwrap_or_else(|| "-".into());
    let g = |v: Option<f32>| v.map(|x| format!("{x:.0}")).unwrap_or_else(|| "-".into());
    for (k, p) in &a.purposes {
        s.push_str(&format!(
            "{k:<12} {:>5} {:>9} {:>7} {:>9} {:>22} {:>14}\n",
            p.count,
            p.delivered,
            p.with_reward,
            p.bypassing_required,
            format!("{}/{}/{}", f(p.ratio_min), f(p.ratio_mean), f(p.ratio_max)),
            format!("{}/{}", g(p.angle_min), g(p.angle_mean)),
        ));
    }
    s.push_str(&format!(
        "purposeless branches: {} (empty dead ends: {})\n",
        a.purposeless, a.empty_dead_ends
    ));
    if a.problems.is_empty() {
        s.push_str("no problems: every branch declares and delivers a purpose\n");
    } else {
        s.push_str(&format!("{} problem(s):\n", a.problems.len()));
        for p in a.problems.iter().take(40) {
            s.push_str(&format!("  - {p}\n"));
        }
        if a.problems.len() > 40 {
            s.push_str(&format!(
                "  ... and {} more (see --json)\n",
                a.problems.len() - 40
            ));
        }
    }
    s
}
