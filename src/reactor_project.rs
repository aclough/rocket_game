//! Reactor design project — the workflow wrapper around a
//! [`ReactorDesign`]. Mirrors `engine_project::EngineProject` so the
//! Reactors pane can reuse the same status / team-assignment / NRE
//! patterns the Engines pane already has.

use rand::Rng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};

use crate::balance_config::{BalanceConfig, FlawsConfig};
use crate::flaw::FlawDomain;
use crate::project::{DesignProject, DesignStatus, Designable, Direction, Improvement, ImprovementId, NoSpec, ProjectKind, ProjectRef};
use crate::reactor::{EnrichmentLevel, ReactorDesign, ReactorId};
use crate::technology::TechDeficiencyKind;

/// A potential improvement to a reactor design, discovered during
/// testing and actualized via revision. Reactor-specific counterpart to
/// the engine's `EngineImprovement` (reactors have no Isp/thrust to improve).
pub type ReactorImprovement = Improvement<ReactorImprovementKind>;

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
fn generate_reactor_improvement(rng: &mut StdRng, id: ImprovementId) -> ReactorImprovement {
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
        id,
        description: description.to_string(),
        kind,
        actualized: false,
    }
}

/// Reactor design complexity. Fission reactors are a hard engineering
/// domain — fixed at staged-combustion-engine tier. Could become a
/// function of scale / enrichment.
pub const REACTOR_BASE_COMPLEXITY: u32 = 8;

/// Days of engineering work required to take a reactor through design.
/// Uses the shared engine design-work curve so reactor and engine
/// projects feel comparably weighty per team-day.
pub fn reactor_design_work_required(complexity: u32, balance_cfg: &BalanceConfig) -> f64 {
    // Reactors have no scale knob; scale 1.0 skips the size term.
    balance_cfg.work.design_work_required(complexity, 1.0)
}

/// Unique identifier for a reactor project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReactorProjectId(pub u64);

/// Workflow status of a reactor project — the shared [`DesignStatus`].
pub type ReactorDesignStatus = DesignStatus;

/// Reactor research project. Owns its `ReactorDesign` and carries the
/// same workflow / NRE bookkeeping as `EngineProject`.
pub type ReactorProject = DesignProject<ReactorDesign>;

impl Designable for ReactorDesign {
    type Id = ReactorProjectId;
    fn project_ref(id: Self::Id) -> ProjectRef {
        ProjectRef::Reactor(id)
    }
    type Spec = NoSpec;
    type ImprovementKind = ReactorImprovementKind;
    const KIND: ProjectKind = ProjectKind::Reactor;

    fn name(&self) -> &str {
        &self.name
    }

    fn flaw_domain(&self) -> FlawDomain {
        FlawDomain::Reactor
    }

    fn flaw_complexity(&self, _spec: &NoSpec, project_complexity: u32) -> u32 {
        project_complexity
    }

    fn improvement_chance(cfg: &FlawsConfig) -> Option<f64> {
        Some(cfg.reactor_improvement_discovery_chance)
    }

    fn roll_improvement(&self, rng: &mut StdRng, id: ImprovementId) -> ReactorImprovement {
        generate_reactor_improvement(rng, id)
    }

    fn apply_improvement(&mut self, kind: &ReactorImprovementKind) {
        match kind {
            ReactorImprovementKind::Power(frac) => self.steady_w *= 1.0 + frac,
            ReactorImprovementKind::Mass(frac) => {
                // Trim the reactor structure (not the bundled radiator)
                // and keep the mass_kg = reactor + radiator invariant.
                let delta = self.reactor_mass_kg * frac;
                self.reactor_mass_kg -= delta;
                self.mass_kg -= delta;
            }
        }
    }

    fn apply_deficiency(&mut self, kind: &TechDeficiencyKind, dir: Direction) {
        ReactorDesign::apply_deficiency(self, kind, dir)
    }
}

impl ReactorProject {
    /// Start a brand-new reactor project in `InDesign`.
    pub fn new(
        project_id: ReactorProjectId,
        reactor_id: ReactorId,
        name: String,
        scale: f64,
        enrichment: EnrichmentLevel,
        balance_cfg: &BalanceConfig,
    ) -> Self {
        let design = ReactorDesign::new(reactor_id, name, scale, enrichment, &balance_cfg.costs);
        let complexity = REACTOR_BASE_COMPLEXITY;
        let work_required = reactor_design_work_required(complexity, balance_cfg);
        DesignProject::new_in_design(
            project_id, design, NoSpec {}, complexity, work_required,
            Some(crate::technology::TECH_FISSION_REACTOR),
        )
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
        balance_cfg: &BalanceConfig,
    ) -> Self {
        let mut p = Self::new(project_id, reactor_id, name, scale, enrichment, balance_cfg);
        let work_required = match p.status {
            ReactorDesignStatus::InDesign { work_required, .. } => work_required,
            _ => unreachable!(),
        };
        p.status = ReactorDesignStatus::Proposed { work_required };
        p
    }

    /// Re-derive the design from a fresh (name, scale, enrichment)
    /// triple. Progress is clamped to the new `work_required` so the
    /// player can't appear to have over-completed a now-cheaper design.
    pub fn apply_edit(&mut self, name: String, scale: f64, enrichment: EnrichmentLevel, balance_cfg: &BalanceConfig) {
        self.design.apply_edit(name, scale, enrichment, &balance_cfg.costs);
        let work_required = reactor_design_work_required(self.complexity, balance_cfg);
        self.clamp_work_after_edit(work_required);
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flaw::Flaw;
    use crate::project::WorkEvent;
    use rand::SeedableRng;

    fn rng() -> StdRng {
        StdRng::seed_from_u64(7)
    }

    fn bal() -> BalanceConfig {
        BalanceConfig::default()
    }

    #[test]
    fn new_project_starts_in_design() {
        let p = ReactorProject::new(
            ReactorProjectId(1),
            ReactorId(1),
            "Mk1".into(),
            1.0,
            EnrichmentLevel::Leu,
            &bal(),
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
            &bal(),
        );
        p.teams_assigned = 2;
        let mut next_flaw = 1u64;
        let events = p.apply_daily_work(&mut rng(), &mut next_flaw, &bal());
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
            &bal(),
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
            &bal(),
        );
        p.teams_assigned = 4;
        let mut next_flaw = 1u64;
        let mut saw_complete = false;
        // Hard cap iterations so a runaway loop fails the test rather
        // than the process.
        for _ in 0..10_000 {
            let events = p.apply_daily_work(&mut rng(), &mut next_flaw, &bal());
            if events.iter().any(|e| matches!(e, WorkEvent::DesignComplete)) {
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
                &bal(),
            );
            p.teams_assigned = 4;
            let mut r = StdRng::seed_from_u64(seed);
            let mut next_flaw = 1u64;
            for _ in 0..10_000 {
                let events = p.apply_daily_work(&mut r, &mut next_flaw, &bal());
                if events.iter().any(|e| matches!(e, WorkEvent::DesignComplete)) {
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
            &bal(),
        );
        p.teams_assigned = 4;
        let mut r = rng();
        let mut next_flaw = 1u64;
        // Advance to Testing.
        for _ in 0..10_000 {
            let events = p.apply_daily_work(&mut r, &mut next_flaw, &bal());
            if events.iter().any(|e| matches!(e, WorkEvent::DesignComplete)) {
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
            let events = p.apply_daily_work(&mut r, &mut next_flaw, &bal());
            if events.iter().any(|e| matches!(e, WorkEvent::FlawDiscovered { .. })) {
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
            &bal(),
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
            p.apply_daily_work(&mut r, &mut next_flaw, &bal());
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
            &bal(),
        );
        p.status = ReactorDesignStatus::Testing { work_completed: 0.0 };
        let before_w = p.design.steady_w;
        p.improvements.push(ReactorImprovement {
            id: ImprovementId(0),
            description: "Optimized coolant flow raises output".into(),
            kind: ReactorImprovementKind::Power(0.05),
            actualized: false,
        });
        p.teams_assigned = 4;
        assert!(p.start_revision());
        let mut r = rng();
        let mut next_flaw = 1u64;
        for _ in 0..50 {
            p.apply_daily_work(&mut r, &mut next_flaw, &bal());
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
            &bal(),
        );
        p.status = ReactorDesignStatus::Testing { work_completed: 0.0 };
        p.improvements.push(ReactorImprovement {
            id: ImprovementId(0),
            description: "Lighter radiation shielding".into(),
            kind: ReactorImprovementKind::Mass(0.10),
            actualized: false,
        });
        p.teams_assigned = 4;
        p.start_revision();
        let mut r = rng();
        let mut next_flaw = 1u64;
        for _ in 0..50 {
            p.apply_daily_work(&mut r, &mut next_flaw, &bal());
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
            &bal(),
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
            &bal(),
        );
        if let ReactorDesignStatus::InDesign { work_completed, work_required } = &mut p.status {
            *work_completed = *work_required;
        }
        // Re-edit doesn't reduce work_required (complexity is constant
        // for Phase 1) but the clamp logic should still leave us
        // ≤ work_required.
        p.apply_edit("Renamed".into(), 1.0, EnrichmentLevel::Leu, &bal());
        if let ReactorDesignStatus::InDesign { work_completed, work_required } = p.status {
            assert!(work_completed <= work_required);
        } else {
            panic!("status should still be InDesign");
        }
    }
}
