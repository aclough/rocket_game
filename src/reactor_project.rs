//! Reactor design project — the workflow wrapper around a
//! [`ReactorDesign`]. Mirrors `engine_project::EngineProject` so the
//! Reactors pane can reuse the same status / team-assignment / NRE
//! patterns the Engines pane already has.
//!
//! Phase 1 only needs `Proposed → InDesign → Testing`; flaw discovery
//! and revision are stubbed out and arrive in Phase 3.

use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

use crate::flaw::Flaw;
use crate::reactor::{EnrichmentLevel, ReactorDesign, ReactorId};
use crate::technology::TechDeficiencyId;

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
    /// game-state loop to log. Phase 1 only fires `DesignComplete`;
    /// `Testing` accrues quietly with no flaw discovery until Phase 3.
    pub fn apply_daily_work(
        &mut self,
        _rng: &mut StdRng,
        _next_flaw_id: &mut u64,
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
                    self.status = ReactorDesignStatus::Testing { work_completed: 0.0 };
                    events.push(ReactorWorkEvent::DesignComplete { flaw_count: 0 });
                }
            }
            ReactorDesignStatus::Testing { work_completed } => {
                *work_completed += work;
                self.cumulative_testing_work += work;
                // Phase 3 — flaw discovery rolls live here.
            }
            ReactorDesignStatus::Revising { .. } => {
                // Phase 3 — revision flow lives here.
            }
        }

        events
    }
}

/// Events bubbled up from `apply_daily_work` to the game-state loop.
#[derive(Debug, Clone)]
pub enum ReactorWorkEvent {
    DesignComplete { flaw_count: u32 },
    TestingCycleComplete,
    FlawDiscovered { flaw_description: String },
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
