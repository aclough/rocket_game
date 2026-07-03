//! Reactor design project — the workflow wrapper around a
//! [`ReactorDesign`]. Mirrors `engine_project::EngineProject` so the
//! Reactors pane can reuse the same status / team-assignment / NRE
//! patterns the Engines pane already has.
//!
//! Phase 1 only needs `Proposed → InDesign → Testing`; flaw discovery
//! and revision are stubbed out and arrive in Phase 3.

use rand::Rng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

use crate::flaw::{self, Flaw, FLAW_REVISION_WORK, TESTING_CYCLE_WORK};
use crate::reactor::{EnrichmentLevel, ReactorDesign, ReactorId};
use crate::technology::TechDeficiencyId;

/// Chance per testing cycle to discover a reactor improvement. Matches
/// the engine improvement discovery rate for parity.
const REACTOR_IMPROVEMENT_DISCOVERY_CHANCE: f64 = 0.08;

/// A potential improvement to a reactor design, discovered during
/// testing and actualized via revision. Reactor-specific counterpart to
/// the engine's `EngineImprovement` (reactors have no Isp/thrust to improve).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactorImprovement {
    pub description: String,
    pub kind: ReactorImprovementKind,
    /// Whether this improvement has been actualized via revision.
    pub actualized: bool,
}

/// What a reactor improvement affects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReactorImprovementKind {
    /// Increase steady output power by this fraction (e.g. 0.02 = +2%).
    Power(f64),
    /// Reduce reactor mass by this fraction (e.g. 0.03 = -3%).
    Mass(f64),
}

impl std::fmt::Display for ReactorImprovementKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReactorImprovementKind::Power(frac) => write!(f, "+{:.0}% power", frac * 100.0),
            ReactorImprovementKind::Mass(frac) => write!(f, "-{:.0}% mass", frac * 100.0),
        }
    }
}

/// Generate a random reactor improvement (Power or Mass).
fn generate_reactor_improvement(rng: &mut StdRng) -> ReactorImprovement {
    let roll: f64 = rng.gen();
    let (kind, description) = if roll < 0.55 {
        let frac = rng.gen_range(0.01..0.04);
        (ReactorImprovementKind::Power(frac), match rng.gen_range(0u32..3) {
            0 => "Higher fuel enrichment margin",
            1 => "Improved neutron reflector geometry",
            _ => "Optimized coolant flow raises output",
        })
    } else {
        let frac = rng.gen_range(0.02..0.06);
        (ReactorImprovementKind::Mass(frac), match rng.gen_range(0u32..3) {
            0 => "Lighter radiation shielding",
            1 => "Compact reactor core design",
            _ => "Reduced radiator support mass",
        })
    };
    ReactorImprovement {
        description: description.to_string(),
        kind,
        actualized: false,
    }
}

/// Reactor design complexity. Fission reactors are a hard engineering
/// domain — fixed at staged-combustion-engine tier for Phase 1. Phase 3
/// can promote this to a function of scale / enrichment.
pub const REACTOR_BASE_COMPLEXITY: u32 = 8;

/// Days of engineering work required to take a reactor through design.
/// Uses the shared `balance::design_work_required` curve so reactor and
/// engine projects feel comparably weighty per team-day.
pub fn reactor_design_work_required(complexity: u32) -> f64 {
    crate::balance::design_work_required(complexity)
}

/// Unique identifier for a reactor project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReactorProjectId(pub u64);

/// Workflow status of a reactor project. Mirrors `EngineDesignStatus`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReactorDesignStatus {
    /// Tentative — created inside the rocket designer but not committed.
    /// No work accrues. Promoted to `InDesign` when the parent rocket
    /// is finalised; deleted if the designer is cancelled.
    Proposed { work_required: f64 },
    InDesign { work_completed: f64, work_required: f64 },
    Testing { work_completed: f64 },
    /// Revising discovered flaws / improvements / tech deficiencies.
    /// Phase 3 wires this; Phase 1 never constructs it.
    Revising {
        remaining_flaw_indices: Vec<usize>,
        remaining_improvement_indices: Vec<usize>,
        remaining_tech_deficiency_ids: Vec<TechDeficiencyId>,
        work_completed: f64,
    },
}

/// Reactor research project. Owns its `ReactorDesign` and carries the
/// same workflow / NRE bookkeeping as `EngineProject`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactorProject {
    pub project_id: ReactorProjectId,
    pub design: ReactorDesign,
    pub status: ReactorDesignStatus,
    pub flaws: Vec<Flaw>,
    pub revision: u32,
    pub teams_assigned: u32,
    pub complexity: u32,
    /// Cumulative engineering salary spent on this project (NRE).
    #[serde(default)]
    pub nre_cost: f64,
    /// Improvements discovered during testing. Pending ones need a
    /// revision to actualize.
    #[serde(default)]
    pub improvements: Vec<ReactorImprovement>,
    /// Cumulative work spent in testing (persists across revisions).
    #[serde(default)]
    pub cumulative_testing_work: f64,
    /// IDs of unsolved tech deficiencies on this reactor (references
    /// Technology.deficiencies). Wired in Phase 3.
    #[serde(default)]
    pub tech_deficiency_ids: Vec<TechDeficiencyId>,
    /// Which technology this reactor uses — always
    /// `Some(TECH_FISSION_REACTOR)` for now.
    #[serde(default)]
    pub technology_id: Option<crate::technology::TechnologyId>,
}

impl ReactorProject {
    /// Start a brand-new reactor project in `InDesign`.
    pub fn new(
        project_id: ReactorProjectId,
        reactor_id: ReactorId,
        name: String,
        scale: f64,
        enrichment: EnrichmentLevel,
    ) -> Self {
        let design = ReactorDesign::new(reactor_id, name, scale, enrichment);
        let complexity = REACTOR_BASE_COMPLEXITY;
        let work_required = reactor_design_work_required(complexity);
        ReactorProject {
            project_id,
            design,
            status: ReactorDesignStatus::InDesign {
                work_completed: 0.0,
                work_required,
            },
            flaws: Vec::new(),
            revision: 0,
            teams_assigned: 0,
            complexity,
            nre_cost: 0.0,
            improvements: Vec::new(),
            cumulative_testing_work: 0.0,
            tech_deficiency_ids: Vec::new(),
            technology_id: Some(crate::technology::TECH_FISSION_REACTOR),
        }
    }

    /// Create a tentative `Proposed` reactor project, used by the
    /// rocket designer for drafts. Promoted to `InDesign` when the
    /// parent rocket is finalised.
    pub fn new_proposed(
        project_id: ReactorProjectId,
        reactor_id: ReactorId,
        name: String,
        scale: f64,
        enrichment: EnrichmentLevel,
    ) -> Self {
        let mut p = Self::new(project_id, reactor_id, name, scale, enrichment);
        let work_required = match p.status {
            ReactorDesignStatus::InDesign { work_required, .. } => work_required,
            _ => unreachable!(),
        };
        p.status = ReactorDesignStatus::Proposed { work_required };
        p
    }

    /// Promote a `Proposed` reactor to `InDesign` with no work
    /// completed. No-op if not Proposed.
    pub fn promote_to_in_design(&mut self) {
        if let ReactorDesignStatus::Proposed { work_required } = self.status {
            self.status = ReactorDesignStatus::InDesign {
                work_completed: 0.0,
                work_required,
            };
        }
    }

    /// Re-derive the design from a fresh (name, scale, enrichment)
    /// triple. Clamps `work_completed` to the new `work_required` so
    /// the player can't appear to have over-completed a now-cheaper
    /// design.
    pub fn apply_edit(&mut self, name: String, scale: f64, enrichment: EnrichmentLevel) {
        self.design.apply_edit(name, scale, enrichment);
        let work_required = reactor_design_work_required(self.complexity);
        match &mut self.status {
            ReactorDesignStatus::Proposed { work_required: wr } => *wr = work_required,
            ReactorDesignStatus::InDesign { work_completed, work_required: wr } => {
                *wr = work_required;
                if *work_completed > *wr {
                    *work_completed = *wr;
                }
            }
            ReactorDesignStatus::Testing { .. } => {
                // Editor shouldn't open on Testing; defensive no-op.
            }
            ReactorDesignStatus::Revising { work_completed, .. } => {
                if *work_completed < 0.0 {
                    *work_completed = 0.0;
                }
            }
        }
    }

    /// Apply one day of work. Returns any work events for the
    /// game-state loop to log. Mirrors `EngineProject::apply_daily_work`:
    /// design completion generates flaws, testing discovers flaws and
    /// improvements, and revision removes flaws / actualizes
    /// improvements / attempts tech-deficiency fixes.
    pub fn apply_daily_work(
        &mut self,
        rng: &mut StdRng,
        next_flaw_id: &mut u64,
    ) -> Vec<ReactorWorkEvent> {
        if self.teams_assigned == 0 {
            return Vec::new();
        }
        let work = crate::team::effective_work_rate(self.teams_assigned);
        let mut events = Vec::new();

        match &mut self.status {
            ReactorDesignStatus::Proposed { .. } => {}
            ReactorDesignStatus::InDesign { work_completed, work_required } => {
                *work_completed += work;
                if *work_completed >= *work_required {
                    // Design complete — generate flaws. Uses the current
                    // complexity (tech-deficiency complexity penalties are
                    // applied afterwards by game_state, matching engines).
                    self.flaws = flaw::generate_reactor_flaws(self.complexity, rng, next_flaw_id);
                    let flaw_count = self.flaws.len() as u32;
                    self.status = ReactorDesignStatus::Testing { work_completed: 0.0 };
                    events.push(ReactorWorkEvent::DesignComplete { flaw_count });
                }
            }
            ReactorDesignStatus::Testing { work_completed } => {
                *work_completed += work;
                self.cumulative_testing_work += work;
                while *work_completed >= TESTING_CYCLE_WORK {
                    *work_completed -= TESTING_CYCLE_WORK;
                    let discovered = flaw::roll_discoveries_with_rng(&mut self.flaws, rng);
                    for idx in discovered {
                        events.push(ReactorWorkEvent::FlawDiscovered {
                            flaw_description: self.flaws[idx].description.clone(),
                        });
                    }
                    // Roll for improvement discovery.
                    if rng.gen::<f64>() < REACTOR_IMPROVEMENT_DISCOVERY_CHANCE {
                        let improvement = generate_reactor_improvement(rng);
                        events.push(ReactorWorkEvent::ImprovementDiscovered {
                            description: format!("{}: {}", improvement.description, improvement.kind),
                        });
                        self.improvements.push(improvement);
                    }
                    events.push(ReactorWorkEvent::TestingCycleComplete);
                }
            }
            ReactorDesignStatus::Revising {
                remaining_flaw_indices,
                remaining_improvement_indices,
                remaining_tech_deficiency_ids,
                work_completed,
            } => {
                *work_completed += work;
                // Process flaws first.
                while *work_completed >= FLAW_REVISION_WORK && !remaining_flaw_indices.is_empty() {
                    *work_completed -= FLAW_REVISION_WORK;
                    let fi = remaining_flaw_indices.remove(0);
                    self.flaws.remove(fi);
                    events.push(ReactorWorkEvent::RevisionComplete);
                    for idx in remaining_flaw_indices.iter_mut() {
                        if *idx > fi {
                            *idx -= 1;
                        }
                    }
                }
                // Then actualize improvements.
                while *work_completed >= FLAW_REVISION_WORK && !remaining_improvement_indices.is_empty() {
                    *work_completed -= FLAW_REVISION_WORK;
                    let ii = remaining_improvement_indices.remove(0);
                    if let Some(imp) = self.improvements.get_mut(ii) {
                        imp.actualized = true;
                        match &imp.kind {
                            ReactorImprovementKind::Power(frac) => {
                                self.design.steady_w *= 1.0 + frac;
                            }
                            ReactorImprovementKind::Mass(frac) => {
                                // Trim the reactor structure (not the
                                // bundled radiator) and keep the
                                // mass_kg = reactor + radiator invariant.
                                let delta = self.design.reactor_mass_kg * frac;
                                self.design.reactor_mass_kg -= delta;
                                self.design.mass_kg -= delta;
                            }
                        }
                        events.push(ReactorWorkEvent::ImprovementActualized {
                            description: format!("{}: {}", imp.description, imp.kind),
                        });
                    }
                }
                // Then attempt tech deficiency fixes (resolved by game_state).
                while *work_completed >= FLAW_REVISION_WORK && !remaining_tech_deficiency_ids.is_empty() {
                    *work_completed -= FLAW_REVISION_WORK;
                    let def_id = remaining_tech_deficiency_ids.remove(0);
                    events.push(ReactorWorkEvent::TechDeficiencyAttempted { deficiency_id: def_id });
                }
                if remaining_flaw_indices.is_empty()
                    && remaining_improvement_indices.is_empty()
                    && remaining_tech_deficiency_ids.is_empty()
                {
                    let leftover = *work_completed;
                    self.status = ReactorDesignStatus::Testing { work_completed: leftover };
                }
            }
        }

        events
    }

    /// Start revising all discovered flaws, pending improvements, and
    /// unsolved tech deficiencies. Testing-only; returns false if not in
    /// Testing or there's nothing to revise.
    pub fn start_revision(&mut self) -> bool {
        if !matches!(self.status, ReactorDesignStatus::Testing { .. }) {
            return false;
        }
        let flaw_indices: Vec<usize> = self.flaws.iter()
            .enumerate()
            .filter(|(_, f)| f.discovered)
            .map(|(i, _)| i)
            .collect();
        let improvement_indices: Vec<usize> = self.improvements.iter()
            .enumerate()
            .filter(|(_, imp)| !imp.actualized)
            .map(|(i, _)| i)
            .collect();
        let tech_def_ids = self.tech_deficiency_ids.clone();
        if flaw_indices.is_empty() && improvement_indices.is_empty() && tech_def_ids.is_empty() {
            return false;
        }
        self.revision += 1;
        self.status = ReactorDesignStatus::Revising {
            remaining_flaw_indices: flaw_indices,
            remaining_improvement_indices: improvement_indices,
            remaining_tech_deficiency_ids: tech_def_ids,
            work_completed: 0.0,
        };
        true
    }

    /// Number of discovered flaws (for the pane display).
    pub fn discovered_flaw_count(&self) -> usize {
        self.flaws.iter().filter(|f| f.discovered).count()
    }

    /// Number of pending (not-yet-actualized) improvements.
    pub fn pending_improvement_count(&self) -> usize {
        self.improvements.iter().filter(|imp| !imp.actualized).count()
    }

    /// Testing level description based on cumulative work in testing.
    /// Mirrors `EngineProject::testing_level`.
    pub fn testing_level(&self) -> &'static str {
        let cycles = (self.cumulative_testing_work / TESTING_CYCLE_WORK) as u32;
        match cycles {
            0 => "Untested",
            1..=2 => "Lightly Tested",
            3..=5 => "Moderately Tested",
            6..=9 => "Well Tested",
            _ => "Thoroughly Tested",
        }
    }
}

/// Events bubbled up from `apply_daily_work` to the game-state loop.
#[derive(Debug, Clone)]
pub enum ReactorWorkEvent {
    DesignComplete { flaw_count: u32 },
    TestingCycleComplete,
    FlawDiscovered { flaw_description: String },
    ImprovementDiscovered { description: String },
    ImprovementActualized { description: String },
    RevisionComplete,
    TechDeficiencyAttempted { deficiency_id: TechDeficiencyId },
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn rng() -> StdRng {
        StdRng::seed_from_u64(7)
    }

    #[test]
    fn new_project_starts_in_design() {
        let p = ReactorProject::new(
            ReactorProjectId(1),
            ReactorId(1),
            "Mk1".into(),
            1.0,
            EnrichmentLevel::Leu,
        );
        match p.status {
            ReactorDesignStatus::InDesign { work_completed, work_required } => {
                assert_eq!(work_completed, 0.0);
                assert!(work_required > 0.0);
            }
            _ => panic!("expected InDesign"),
        }
        assert_eq!(p.teams_assigned, 0);
        assert_eq!(p.technology_id, Some(crate::technology::TECH_FISSION_REACTOR));
    }

    #[test]
    fn proposed_project_accrues_no_work() {
        let mut p = ReactorProject::new_proposed(
            ReactorProjectId(1),
            ReactorId(1),
            "Draft".into(),
            1.0,
            EnrichmentLevel::Leu,
        );
        p.teams_assigned = 2;
        let mut next_flaw = 1u64;
        let events = p.apply_daily_work(&mut rng(), &mut next_flaw);
        assert!(events.is_empty());
        assert!(matches!(p.status, ReactorDesignStatus::Proposed { .. }));
    }

    #[test]
    fn promote_moves_proposed_to_in_design() {
        let mut p = ReactorProject::new_proposed(
            ReactorProjectId(1),
            ReactorId(1),
            "Draft".into(),
            1.0,
            EnrichmentLevel::Leu,
        );
        p.promote_to_in_design();
        assert!(matches!(p.status, ReactorDesignStatus::InDesign { .. }));
    }

    #[test]
    fn in_design_accrues_and_transitions_to_testing() {
        let mut p = ReactorProject::new(
            ReactorProjectId(1),
            ReactorId(1),
            "Mk1".into(),
            1.0,
            EnrichmentLevel::Leu,
        );
        p.teams_assigned = 4;
        let mut next_flaw = 1u64;
        let mut saw_complete = false;
        // Hard cap iterations so a runaway loop fails the test rather
        // than the process.
        for _ in 0..10_000 {
            let events = p.apply_daily_work(&mut rng(), &mut next_flaw);
            if events.iter().any(|e| matches!(e, ReactorWorkEvent::DesignComplete { .. })) {
                saw_complete = true;
                break;
            }
        }
        assert!(saw_complete, "design should complete with teams assigned");
        assert!(matches!(p.status, ReactorDesignStatus::Testing { .. }));
    }

    #[test]
    fn design_complete_generates_flaws() {
        // With base complexity 8, a completed design should almost
        // always carry at least one flaw. Sweep a few seeds to be safe.
        let mut saw_flaws = false;
        for seed in 0..20 {
            let mut p = ReactorProject::new(
                ReactorProjectId(1), ReactorId(1), "Mk1".into(), 1.0, EnrichmentLevel::Leu,
            );
            p.teams_assigned = 4;
            let mut r = StdRng::seed_from_u64(seed);
            let mut next_flaw = 1u64;
            for _ in 0..10_000 {
                let events = p.apply_daily_work(&mut r, &mut next_flaw);
                if events.iter().any(|e| matches!(e, ReactorWorkEvent::DesignComplete { .. })) {
                    break;
                }
            }
            if !p.flaws.is_empty() {
                saw_flaws = true;
                break;
            }
        }
        assert!(saw_flaws, "reactor design completion should generate flaws");
    }

    #[test]
    fn testing_discovers_flaws() {
        let mut p = ReactorProject::new(
            ReactorProjectId(1), ReactorId(1), "Mk1".into(), 1.0, EnrichmentLevel::Leu,
        );
        p.teams_assigned = 4;
        let mut r = rng();
        let mut next_flaw = 1u64;
        // Advance to Testing.
        for _ in 0..10_000 {
            let events = p.apply_daily_work(&mut r, &mut next_flaw);
            if events.iter().any(|e| matches!(e, ReactorWorkEvent::DesignComplete { .. })) {
                break;
            }
        }
        // Force a high discovery probability on every flaw so testing
        // surfaces them deterministically.
        for f in &mut p.flaws {
            f.discovery_probability = 0.99;
        }
        let total = p.flaws.len();
        let mut discovered_any = false;
        for _ in 0..200 {
            let events = p.apply_daily_work(&mut r, &mut next_flaw);
            if events.iter().any(|e| matches!(e, ReactorWorkEvent::FlawDiscovered { .. })) {
                discovered_any = true;
            }
            if p.discovered_flaw_count() == total && total > 0 {
                break;
            }
        }
        if total > 0 {
            assert!(discovered_any, "testing should discover forced-visible flaws");
        }
    }

    #[test]
    fn revision_removes_discovered_flaws_and_returns_to_testing() {
        let mut p = ReactorProject::new(
            ReactorProjectId(1), ReactorId(1), "Mk1".into(), 1.0, EnrichmentLevel::Leu,
        );
        // Put it into Testing with one discovered flaw.
        p.status = ReactorDesignStatus::Testing { work_completed: 0.0 };
        p.flaws.push(Flaw {
            id: crate::flaw::FlawId(1),
            description: "Coolant loop flow restriction".into(),
            consequence: crate::flaw::FlawConsequence::PerformanceDegradation(0.05),
            activation_chance: 0.1,
            discovery_probability: 0.5,
            discovered: true,
            trigger: crate::flaw::FlawTrigger::PerFlight,
        });
        p.teams_assigned = 4;

        assert!(p.start_revision());
        assert!(matches!(p.status, ReactorDesignStatus::Revising { .. }));

        let mut r = rng();
        let mut next_flaw = 2u64;
        for _ in 0..50 {
            p.apply_daily_work(&mut r, &mut next_flaw);
            if matches!(p.status, ReactorDesignStatus::Testing { .. }) {
                break;
            }
        }
        assert!(p.flaws.is_empty(), "discovered flaw should be removed");
        assert_eq!(p.revision, 1);
        assert!(matches!(p.status, ReactorDesignStatus::Testing { .. }));
    }

    #[test]
    fn improvement_actualization_boosts_power() {
        let mut p = ReactorProject::new(
            ReactorProjectId(1), ReactorId(1), "Mk1".into(), 1.0, EnrichmentLevel::Leu,
        );
        p.status = ReactorDesignStatus::Testing { work_completed: 0.0 };
        let before_w = p.design.steady_w;
        p.improvements.push(ReactorImprovement {
            description: "Optimized coolant flow raises output".into(),
            kind: ReactorImprovementKind::Power(0.05),
            actualized: false,
        });
        p.teams_assigned = 4;
        assert!(p.start_revision());
        let mut r = rng();
        let mut next_flaw = 1u64;
        for _ in 0..50 {
            p.apply_daily_work(&mut r, &mut next_flaw);
            if matches!(p.status, ReactorDesignStatus::Testing { .. }) {
                break;
            }
        }
        assert!(p.improvements[0].actualized);
        assert!((p.design.steady_w - before_w * 1.05).abs() < 1.0);
    }

    #[test]
    fn mass_improvement_preserves_total_mass_invariant() {
        let mut p = ReactorProject::new(
            ReactorProjectId(1), ReactorId(1), "Mk1".into(), 1.0, EnrichmentLevel::Leu,
        );
        p.status = ReactorDesignStatus::Testing { work_completed: 0.0 };
        p.improvements.push(ReactorImprovement {
            description: "Lighter radiation shielding".into(),
            kind: ReactorImprovementKind::Mass(0.10),
            actualized: false,
        });
        p.teams_assigned = 4;
        p.start_revision();
        let mut r = rng();
        let mut next_flaw = 1u64;
        for _ in 0..50 {
            p.apply_daily_work(&mut r, &mut next_flaw);
            if matches!(p.status, ReactorDesignStatus::Testing { .. }) {
                break;
            }
        }
        // mass_kg must still equal reactor_mass_kg + radiator mass.
        let expected = p.design.reactor_mass_kg + p.design.radiator.mass_kg;
        assert!((p.design.mass_kg - expected).abs() < 1e-6);
    }

    #[test]
    fn start_revision_noop_when_nothing_to_do() {
        let mut p = ReactorProject::new(
            ReactorProjectId(1), ReactorId(1), "Mk1".into(), 1.0, EnrichmentLevel::Leu,
        );
        p.status = ReactorDesignStatus::Testing { work_completed: 0.0 };
        // No discovered flaws, no improvements, no deficiencies.
        assert!(!p.start_revision());
        assert!(matches!(p.status, ReactorDesignStatus::Testing { .. }));
    }

    #[test]
    fn apply_edit_clamps_overrun_work() {
        let mut p = ReactorProject::new(
            ReactorProjectId(1),
            ReactorId(1),
            "Mk1".into(),
            10.0,
            EnrichmentLevel::Heu,
        );
        if let ReactorDesignStatus::InDesign { work_completed, work_required } = &mut p.status {
            *work_completed = *work_required;
        }
        // Re-edit doesn't reduce work_required (complexity is constant
        // for Phase 1) but the clamp logic should still leave us
        // ≤ work_required.
        p.apply_edit("Renamed".into(), 1.0, EnrichmentLevel::Leu);
        if let ReactorDesignStatus::InDesign { work_completed, work_required } = p.status {
            assert!(work_completed <= work_required);
        } else {
            panic!("status should still be InDesign");
        }
    }
}
