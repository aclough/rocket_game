use std::collections::HashSet;

use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::balance;
use crate::balance_config::{BalanceConfig, FlawsConfig};
use crate::flaw::FlawDomain;
use crate::project::{DesignProject, DesignStatus, Designable, Direction, Improvement, ImprovementId, Never, NoSpec, ProjectKind, ProjectRef};
use crate::rocket::RocketDesign;
use crate::technology::TechDeficiencyKind;

/// Unique identifier for a rocket project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct RocketProjectId(pub u64);

/// Status of a rocket design project — the shared [`DesignStatus`].
/// `Proposed` is never constructed for rockets today.
pub type RocketDesignStatus = DesignStatus;

/// A rocket design project with workflow state.
pub type RocketProject = DesignProject<RocketDesign>;

impl Designable for RocketDesign {
    type Id = RocketProjectId;
    fn project_ref(id: Self::Id) -> ProjectRef {
        ProjectRef::Rocket(id)
    }
    type Spec = NoSpec;
    /// Rockets have no improvements: the vehicle is tankage and
    /// structure sized by the player, not a device with a stat curve.
    type ImprovementKind = Never;
    const KIND: ProjectKind = ProjectKind::Rocket;

    fn name(&self) -> &str {
        &self.name
    }

    fn flaw_domain(&self) -> FlawDomain {
        FlawDomain::Rocket
    }

    fn flaw_complexity(&self, _spec: &NoSpec, project_complexity: u32) -> u32 {
        project_complexity
    }

    fn improvement_chance(_cfg: &FlawsConfig) -> Option<f64> {
        None
    }

    fn roll_improvement(&self, _rng: &mut StdRng, _id: ImprovementId) -> Improvement<Never> {
        unreachable!("rocket projects have no improvements: improvement_chance is None")
    }

    fn apply_improvement(&mut self, kind: &Never) {
        match *kind {}
    }

    /// Rockets carry no technology, so no deficiency ever reaches one.
    fn apply_deficiency(&mut self, _kind: &TechDeficiencyKind, _dir: Direction) {}
}

impl RocketProject {
    /// Create a new rocket project from a completed rocket design.
    pub fn new(
        project_id: RocketProjectId,
        design: RocketDesign,
        balance_cfg: &BalanceConfig,
    ) -> Self {
        let (total_stages, unique_engines, max_parallel) = design_stats(&design);
        let complexity = balance::rocket_complexity(total_stages, unique_engines, max_parallel);
        let work_required = balance_cfg.work.rocket_design_work_required(complexity);
        DesignProject::new_in_design(project_id, design, NoSpec {}, complexity, work_required, None)
    }

}

/// Extract design statistics for complexity calculation.
fn design_stats(design: &RocketDesign) -> (u32, u32, u32) {
    let total_stages: u32 = design.stage_groups.iter()
        .map(|g| g.len() as u32)
        .sum();

    let mut engine_ids = HashSet::new();
    for group in &design.stage_groups {
        for stage in group {
            engine_ids.insert(stage.engine.id);
        }
    }
    let unique_engines = engine_ids.len() as u32;

    let max_parallel = design.stage_groups.iter()
        .map(|g| g.len() as u32)
        .max()
        .unwrap_or(1);

    (total_stages, unique_engines, max_parallel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flaw::Flaw;
    use crate::project::WorkEvent;
    use crate::engine::*;
    use crate::propellant::Propellant;
    use crate::stage::*;
    use rand::SeedableRng;

    fn test_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    fn bal() -> BalanceConfig {
        BalanceConfig::default()
    }

    fn kerolox_engine(id: u64, thrust: f64, mass: f64, isp: f64) -> EngineDesign {
        EngineDesign {
            id: EngineId(id),
            name: format!("Engine-{}", id),
            cycle: EngineCycle::GasGenerator,
            thrust_n: thrust,
            mass_kg: mass,
            isp_s: isp,
            exit_pressure_pa: 70_000.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
            power_draw_w: 0.0,
        }
    }

    fn simple_two_stage_design() -> RocketDesign {
        let e1 = kerolox_engine(1, 1_000_000.0, 500.0, 280.0);
        let e2 = kerolox_engine(2, 200_000.0, 100.0, 340.0);
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: e1, engine_count: 1,
            propellant_mass_kg: 50_000.0, structural_mass_kg: 3_000.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: e2, engine_count: 1,
            propellant_mass_kg: 10_000.0, structural_mass_kg: 500.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        RocketDesign {
            id: crate::rocket::RocketDesignId(1),
            name: "TestRocket".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        }
    }

    #[test]
    fn test_design_stats() {
        let design = simple_two_stage_design();
        let (total, unique, max_par) = design_stats(&design);
        assert_eq!(total, 2);
        assert_eq!(unique, 2);
        assert_eq!(max_par, 1);
    }

    #[test]
    fn test_new_rocket_project() {
        let design = simple_two_stage_design();
        let proj = RocketProject::new(RocketProjectId(1), design, &bal());
        assert!(matches!(proj.status, RocketDesignStatus::InDesign { .. }));
        assert_eq!(proj.teams_assigned, 0);
        assert!(proj.complexity >= 3 && proj.complexity <= 8);
    }

    #[test]
    fn test_rocket_project_design_completes() {
        let design = simple_two_stage_design();
        let mut proj = RocketProject::new(RocketProjectId(1), design, &bal());
        proj.teams_assigned = 4;
        let mut rng = test_rng();
        let mut next_flaw_id = 0u64;

        let work_needed = match &proj.status {
            RocketDesignStatus::InDesign { work_required, .. } => *work_required,
            _ => panic!("should be InDesign"),
        };

        let mut all_events = Vec::new();
        for _ in 0..(work_needed as u32 + 10) {
            let events = proj.apply_daily_work(&mut rng, &mut next_flaw_id, &bal());
            all_events.extend(events);
        }

        assert!(all_events.iter().any(|e| matches!(e, WorkEvent::DesignComplete)));
        assert!(matches!(proj.status, RocketDesignStatus::Testing { .. }));
    }

    #[test]
    fn test_rocket_project_revision_fixes_all() {
        let design = simple_two_stage_design();
        let mut proj = RocketProject::new(RocketProjectId(1), design, &bal());
        proj.teams_assigned = 4;
        let mut rng = test_rng();
        let mut next_flaw_id = 0u64;

        // Advance to testing
        for _ in 0..200 {
            proj.apply_daily_work(&mut rng, &mut next_flaw_id, &bal());
        }

        // Clear any generated flaws and add controlled test flaws
        proj.flaws.clear();
        proj.flaws.push(Flaw {
            id: crate::flaw::FlawId(900),
            description: "Test flaw 1".into(),
            consequence: crate::flaw::FlawConsequence::StageLoss,
            activation_chance: 0.1,
            discovery_probability: 0.5,
            discovered: true,
            trigger: crate::flaw::FlawTrigger::PerFlight,
        });
        proj.flaws.push(Flaw {
            id: crate::flaw::FlawId(901),
            description: "Test flaw 2".into(),
            consequence: crate::flaw::FlawConsequence::EngineLoss,
            activation_chance: 0.05,
            discovery_probability: 0.3,
            discovered: true,
            trigger: crate::flaw::FlawTrigger::PerFlight,
        });

        assert_eq!(proj.flaws.len(), 2);
        assert_eq!(proj.discovered_flaw_count(), 2);
        assert!(proj.start_revision());

        for _ in 0..50 {
            proj.apply_daily_work(&mut rng, &mut next_flaw_id, &bal());
        }

        assert_eq!(proj.flaws.len(), 0);
        // Revision increments once per revision cycle, not per flaw
        assert_eq!(proj.revision, 1);
        assert!(matches!(proj.status, RocketDesignStatus::Testing { .. }));
    }

    /// The route memos key on this, so it has to move when anything the
    /// planner reads moves — and stay put when nothing does.
    #[test]
    fn design_fingerprint_tracks_what_the_planner_reads() {
        let base = simple_two_stage_design();
        let f = base.fingerprint();
        assert_eq!(f, simple_two_stage_design().fingerprint(), "stable for equal designs");

        let mut renamed = simple_two_stage_design();
        renamed.name = "Something Else".into();
        renamed.stage_groups[0][0].name = "Booster".into();
        assert_eq!(
            renamed.fingerprint(), f,
            "names reach no number the planner uses — renaming must not \
             throw the cache away",
        );

        let mut fuelled = simple_two_stage_design();
        fuelled.stage_groups[0][0].propellant_mass_kg += 1.0;
        assert_ne!(fuelled.fingerprint(), f, "propellant changes the answer");

        let mut heavier = simple_two_stage_design();
        heavier.stage_groups[1][0].structural_mass_kg += 1.0;
        assert_ne!(heavier.fingerprint(), f, "dry mass changes the answer");

        let mut better_engine = simple_two_stage_design();
        better_engine.stage_groups[0][0].engine.isp_s += 1.0;
        assert_ne!(better_engine.fingerprint(), f, "a degraded or improved engine differs");

        let mut more_engines = simple_two_stage_design();
        more_engines.stage_groups[0][0].engine_count += 1;
        assert_ne!(more_engines.fingerprint(), f, "engine count changes thrust");

        let mut powered = simple_two_stage_design();
        powered.stage_groups[1][0].power_sources
            .push(crate::power::PowerSource::new_solar_panel(500.0));
        assert_ne!(
            powered.fingerprint(), f,
            "power kit has mass and decides trip survival",
        );

        let mut staged = simple_two_stage_design();
        staged.stage_groups.pop();
        assert_ne!(staged.fingerprint(), f, "dropping a stage changes the vehicle");
    }
}
