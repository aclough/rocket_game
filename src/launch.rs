use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::contract::ContractId;
use crate::engine::EngineId;
use crate::engine_project::{EngineProject, EngineSource};
use crate::flaw::FlawConsequence;
use crate::rocket::RocketDesign;
use crate::rocket_project::RocketProject;
use crate::third_party::ContractedEngine;

/// Record of a flaw that activated during a launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlawActivation {
    pub flaw_description: String,
    pub consequence: FlawConsequence,
    pub engine_name: String,
}

/// Record of a launch attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchRecord {
    pub launch_date: GameDate,
    pub rocket_name: String,
    pub contract_id: Option<ContractId>,
    pub destination: String,
    pub payload_kg: f64,
    pub outcome: LaunchOutcome,
    pub flaws_activated: Vec<FlawActivation>,
}

/// Outcome of a launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LaunchOutcome {
    Success,
    PartialFailure { reason: String },
    Failure { reason: String },
}

/// Result of simulating a launch, before applying to game state.
pub struct LaunchSimResult {
    pub outcome: LaunchOutcome,
    pub flaws_activated: Vec<FlawActivation>,
    /// Indices of flaws to mark as discovered on engine projects.
    pub engine_flaw_discoveries: Vec<(EngineId, Vec<usize>)>,
    /// Indices of flaws to mark as discovered on rocket projects.
    pub rocket_flaw_discoveries: Vec<usize>,
    /// Indices of flaws to mark as discovered on contracted engines.
    pub contracted_flaw_discoveries: Vec<(EngineSource, Vec<usize>)>,
}

/// Simulate a launch. This does not modify any state — it returns a result
/// that the caller applies.
///
/// The simulation:
/// 1. Rolls activation for each flaw (engine projects + rocket project + contracted engines)
/// 2. Applies consequences to a cloned design
/// 3. Computes delta-v with degraded performance
/// 4. Compares to required delta-v for the destination
pub fn simulate_launch(
    design: &RocketDesign,
    destination: &str,
    payload_kg: f64,
    engine_projects: &[EngineProject],
    rocket_project: &RocketProject,
    contracted_engines: &[ContractedEngine],
    rng: &mut StdRng,
) -> LaunchSimResult {
    let mut activations = Vec::new();
    let mut engine_flaw_discoveries: Vec<(EngineId, Vec<usize>)> = Vec::new();
    let mut rocket_flaw_discoveries: Vec<usize> = Vec::new();
    let mut contracted_flaw_discoveries: Vec<(EngineSource, Vec<usize>)> = Vec::new();

    // Clone the design so we can degrade it
    let mut degraded = design.clone();

    // Roll engine project flaws
    for group in &design.stage_groups {
        for stage in group {
            // Find the engine project for this stage's engine
            if let Some(ep) = engine_projects.iter()
                .find(|ep| ep.design.id == stage.engine.id)
            {
                let mut discovered_indices = Vec::new();
                for (fi, flaw) in ep.flaws.iter().enumerate() {
                    if rng.gen::<f64>() < flaw.activation_chance {
                        activations.push(FlawActivation {
                            flaw_description: flaw.description.clone(),
                            consequence: flaw.consequence.clone(),
                            engine_name: stage.engine.name.clone(),
                        });
                        discovered_indices.push(fi);
                        apply_consequence_to_design(
                            &mut degraded,
                            &flaw.consequence,
                            stage.engine.id,
                        );
                    }
                }
                if !discovered_indices.is_empty() {
                    engine_flaw_discoveries.push((stage.engine.id, discovered_indices));
                }
            }

            // Check contracted engines
            if let Some(ce) = contracted_engines.iter()
                .find(|ce| ce.design.id == stage.engine.id)
            {
                let mut discovered_indices = Vec::new();
                for (fi, flaw) in ce.flaws.iter().enumerate() {
                    if rng.gen::<f64>() < flaw.activation_chance {
                        activations.push(FlawActivation {
                            flaw_description: flaw.description.clone(),
                            consequence: flaw.consequence.clone(),
                            engine_name: stage.engine.name.clone(),
                        });
                        discovered_indices.push(fi);
                        apply_consequence_to_design(
                            &mut degraded,
                            &flaw.consequence,
                            stage.engine.id,
                        );
                    }
                }
                if !discovered_indices.is_empty() {
                    contracted_flaw_discoveries.push((
                        EngineSource::Contracted(ce.id),
                        discovered_indices,
                    ));
                }
            }
        }
    }

    // Roll rocket project flaws — these affect a random stage group
    for (fi, flaw) in rocket_project.flaws.iter().enumerate() {
        if rng.gen::<f64>() < flaw.activation_chance {
            // Pick a random stage group for the consequence
            let group_count = degraded.stage_groups.len();
            if group_count > 0 {
                let gi = rng.gen_range(0..group_count);
                let engine_name = degraded.stage_groups.get(gi)
                    .and_then(|g| g.first())
                    .map(|s| s.engine.name.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                activations.push(FlawActivation {
                    flaw_description: flaw.description.clone(),
                    consequence: flaw.consequence.clone(),
                    engine_name,
                });
                apply_rocket_consequence(&mut degraded, &flaw.consequence, gi);
            }
            rocket_flaw_discoveries.push(fi);
        }
    }

    // Compute degraded delta-v
    let degraded_dv = degraded.total_delta_v(payload_kg);

    // Get required delta-v for destination
    let rocket_mass = design.total_mass_kg();
    let required_dv = crate::location::DELTA_V_MAP
        .shortest_path("earth_surface", destination, rocket_mass)
        .map(|(_, dv)| dv)
        .unwrap_or(f64::INFINITY);

    // Determine outcome
    let outcome = if degraded_dv >= required_dv {
        LaunchOutcome::Success
    } else if degraded_dv >= required_dv * 0.95 {
        let shortfall = ((1.0 - degraded_dv / required_dv) * 100.0).round();
        LaunchOutcome::PartialFailure {
            reason: format!("{}% delta-v shortfall", shortfall),
        }
    } else {
        // Check if it was a stage loss
        let stage_lost = activations.iter().any(|a| matches!(a.consequence, FlawConsequence::StageLoss));
        if stage_lost {
            LaunchOutcome::Failure {
                reason: "Stage loss during flight".to_string(),
            }
        } else {
            let shortfall = ((1.0 - degraded_dv / required_dv) * 100.0).round();
            LaunchOutcome::Failure {
                reason: format!("{}% delta-v shortfall", shortfall),
            }
        }
    };

    LaunchSimResult {
        outcome,
        flaws_activated: activations,
        engine_flaw_discoveries,
        rocket_flaw_discoveries,
        contracted_flaw_discoveries,
    }
}

/// Apply a flaw consequence to a cloned design (engine-level flaw).
fn apply_consequence_to_design(
    design: &mut RocketDesign,
    consequence: &FlawConsequence,
    engine_id: EngineId,
) {
    match consequence {
        FlawConsequence::PerformanceDegradation(frac) => {
            // Reduce thrust and Isp of the affected engine
            for group in &mut design.stage_groups {
                for stage in group {
                    if stage.engine.id == engine_id {
                        stage.engine.thrust_n *= 1.0 - frac;
                        stage.engine.isp_s *= 1.0 - frac;
                    }
                }
            }
        }
        FlawConsequence::EngineLoss => {
            // Remove one engine from the affected stage
            for group in &mut design.stage_groups {
                for stage in group {
                    if stage.engine.id == engine_id && stage.engine_count > 0 {
                        stage.engine_count -= 1;
                        // If engine_count drops to 0, effectively stage is lost
                        // (thrust = 0, no delta-v contribution)
                        return; // only lose one engine per flaw
                    }
                }
            }
        }
        FlawConsequence::StageLoss => {
            // Remove all stages with this engine from their group
            for group in &mut design.stage_groups {
                group.retain(|s| s.engine.id != engine_id);
            }
            // Remove empty groups
            design.stage_groups.retain(|g| !g.is_empty());
        }
    }
}

/// Apply a rocket-level flaw consequence to a specific stage group.
fn apply_rocket_consequence(
    design: &mut RocketDesign,
    consequence: &FlawConsequence,
    group_index: usize,
) {
    if group_index >= design.stage_groups.len() {
        return;
    }
    match consequence {
        FlawConsequence::PerformanceDegradation(frac) => {
            for stage in &mut design.stage_groups[group_index] {
                stage.engine.thrust_n *= 1.0 - frac;
                stage.engine.isp_s *= 1.0 - frac;
            }
        }
        FlawConsequence::EngineLoss => {
            // Lose one engine from the first stage in the group
            if let Some(stage) = design.stage_groups[group_index].first_mut() {
                if stage.engine_count > 0 {
                    stage.engine_count -= 1;
                }
            }
        }
        FlawConsequence::StageLoss => {
            design.stage_groups.remove(group_index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use crate::engine::{EngineDesign, EngineCycle, PropellantFraction};
    use crate::propellant::Propellant;
    use crate::rocket::{RocketDesign, RocketDesignId};
    use crate::stage::{Stage, StageId};
    use crate::engine_project::{EngineProject, EngineProjectId, PropellantPreset};
    use crate::rocket_project::{RocketProject, RocketProjectId};
    use crate::flaw::{Flaw, FlawId};

    fn make_engine(id: u64) -> EngineDesign {
        EngineDesign {
            id: EngineId(id),
            name: format!("TestEngine{}", id),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 1_000_000.0,
            isp_s: 300.0,
            exit_pressure_pa: 100_000.0,
            needs_atmosphere: false,
            mass_kg: 1000.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.6 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.4 },
            ],
        }
    }

    fn make_stage(engine_id: u64) -> Stage {
        Stage {
            id: StageId(engine_id),
            name: format!("S{}", engine_id),
            engine: make_engine(engine_id),
            engine_count: 1,
            propellant_mass_kg: 50_000.0,
            structural_mass_kg: 2_000.0,
            fairing: None,
        }
    }

    fn make_design() -> RocketDesign {
        RocketDesign {
            id: RocketDesignId(1),
            name: "TestRocket".into(),
            stage_groups: vec![
                vec![make_stage(1)],
                vec![make_stage(2)],
            ],
        }
    }

    fn make_engine_project(id: u64, flaws: Vec<Flaw>) -> EngineProject {
        let mut ep = EngineProject::new(
            EngineProjectId(id),
            EngineId(id),
            format!("TestEngine{}", id),
            EngineCycle::GasGenerator,
            PropellantPreset::Kerolox,
            1.0,
            true,
        ).unwrap();
        ep.flaws = flaws;
        ep
    }

    fn make_rocket_project(design: RocketDesign, flaws: Vec<Flaw>) -> RocketProject {
        let mut rp = RocketProject::new(RocketProjectId(1), design);
        rp.flaws = flaws;
        rp
    }

    #[test]
    fn test_launch_no_flaws_success() {
        let design = make_design();
        let ep1 = make_engine_project(1, vec![]);
        let ep2 = make_engine_project(2, vec![]);
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = simulate_launch(
            &design, "leo", 0.0,
            &[ep1, ep2], &rp, &[], &mut rng,
        );

        assert!(matches!(result.outcome, LaunchOutcome::Success));
        assert!(result.flaws_activated.is_empty());
    }

    #[test]
    fn test_launch_with_guaranteed_flaw() {
        let design = make_design();
        let flaw = Flaw {
            id: FlawId(1),
            description: "Turbopump seal failure".into(),
            consequence: FlawConsequence::PerformanceDegradation(0.5),
            activation_chance: 1.0, // guaranteed activation
            discovery_probability: 0.5,
            discovered: false,
        };
        let ep1 = make_engine_project(1, vec![flaw]);
        let ep2 = make_engine_project(2, vec![]);
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = simulate_launch(
            &design, "leo", 0.0,
            &[ep1, ep2], &rp, &[], &mut rng,
        );

        assert_eq!(result.flaws_activated.len(), 1);
        assert_eq!(result.flaws_activated[0].flaw_description, "Turbopump seal failure");
        // Should have discovered the flaw
        assert_eq!(result.engine_flaw_discoveries.len(), 1);
    }

    #[test]
    fn test_launch_stage_loss_causes_failure() {
        let design = make_design();
        let flaw = Flaw {
            id: FlawId(1),
            description: "Structural failure".into(),
            consequence: FlawConsequence::StageLoss,
            activation_chance: 1.0,
            discovery_probability: 0.5,
            discovered: false,
        };
        let ep1 = make_engine_project(1, vec![flaw]);
        let ep2 = make_engine_project(2, vec![]);
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        // With a heavy payload, losing a stage should cause failure
        let result = simulate_launch(
            &design, "gto", 5000.0,
            &[ep1, ep2], &rp, &[], &mut rng,
        );

        // Should be failure or partial failure (not success)
        assert!(!matches!(result.outcome, LaunchOutcome::Success));
    }

    #[test]
    fn test_launch_rocket_flaw_activates() {
        let design = make_design();
        let ep1 = make_engine_project(1, vec![]);
        let ep2 = make_engine_project(2, vec![]);
        let flaw = Flaw {
            id: FlawId(1),
            description: "Separation failure".into(),
            consequence: FlawConsequence::StageLoss,
            activation_chance: 1.0,
            discovery_probability: 0.5,
            discovered: false,
        };
        let rp = make_rocket_project(design.clone(), vec![flaw]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = simulate_launch(
            &design, "leo", 0.0,
            &[ep1, ep2], &rp, &[], &mut rng,
        );

        assert_eq!(result.flaws_activated.len(), 1);
        assert_eq!(result.rocket_flaw_discoveries.len(), 1);
    }

    #[test]
    fn test_zero_activation_chance_never_fires() {
        let design = make_design();
        let flaw = Flaw {
            id: FlawId(1),
            description: "Hidden flaw".into(),
            consequence: FlawConsequence::StageLoss,
            activation_chance: 0.0,
            discovery_probability: 0.5,
            discovered: false,
        };
        let ep1 = make_engine_project(1, vec![flaw]);
        let ep2 = make_engine_project(2, vec![]);
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = simulate_launch(
            &design, "leo", 0.0,
            &[ep1, ep2], &rp, &[], &mut rng,
        );

        assert!(result.flaws_activated.is_empty());
        assert!(matches!(result.outcome, LaunchOutcome::Success));
    }
}
