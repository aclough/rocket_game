use rand::Rng;
use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::engine::{EngineDesign, EngineCycle, EngineId, PropellantFraction};
use crate::engine_project::{EngineProject, EngineProjectId, EngineDesignStatus, PropellantPreset};
use crate::flaw::{Flaw, FlawConsequence, FlawId};
use crate::propellant::Propellant;
use crate::seed::GameSeed;

/// A third-party engine available for purchase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyEngine {
    pub project: EngineProject,
    pub purchase_cost: f64,
    pub available_from: GameDate,
}

/// Generate the starter third-party engines from the game seed.
///
/// Returns 3 engines:
/// 1. Small solid kick motor
/// 2. Medium kerolox engine (NK-33 analogue)
/// 3. Small hypergolic thruster
pub fn generate_starter_engines(seed: &GameSeed) -> Vec<ThirdPartyEngine> {
    let start = GameDate::default_start();
    let mut rng = seed.world_query("third_party_engines");
    let mut next_flaw_id = 10000u64; // high range to avoid collision with player flaws

    let solid_kick = {
        let mut flaws = Vec::new();
        // Solid motors are simple — 1-2 flaws
        let flaw_count: u32 = rng.gen_range(1..=2);
        for _ in 0..flaw_count {
            flaws.push(generate_third_party_flaw(&mut rng, &mut next_flaw_id));
        }

        let design = EngineDesign {
            id: EngineId(10001),
            name: "KM-15 Kick Motor".into(),
            cycle: EngineCycle::PressureFed,
            thrust_n: 75_000.0,
            mass_kg: 35.0,
            isp_s: 245.0,
            exit_pressure_pa: 30_000.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::SolidMix, mass_fraction: 1.0 },
            ],
        };

        let project = EngineProject {
            project_id: EngineProjectId(10001),
            design,
            preset: PropellantPreset::Solid,
            scale: 1.0,
            status: EngineDesignStatus::Complete,
            flaws,
            revision: 0,
            teams_assigned: 0,
            complexity: 5,
            is_third_party: true,
        };

        ThirdPartyEngine {
            project,
            purchase_cost: 800_000.0,
            available_from: start,
        }
    };

    let nk33_analogue = {
        let mut flaws = Vec::new();
        let flaw_count: u32 = rng.gen_range(2..=4);
        for _ in 0..flaw_count {
            flaws.push(generate_third_party_flaw(&mut rng, &mut next_flaw_id));
        }

        let design = EngineDesign {
            id: EngineId(10002),
            name: "RD-33K".into(),
            cycle: EngineCycle::StagedCombustion,
            thrust_n: 1_680_000.0,
            mass_kg: 1_220.0,
            isp_s: 297.0,
            exit_pressure_pa: 60_000.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.73 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.27 },
            ],
        };

        let project = EngineProject {
            project_id: EngineProjectId(10002),
            design,
            preset: PropellantPreset::Kerolox,
            scale: 1.0,
            status: EngineDesignStatus::Complete,
            flaws,
            revision: 0,
            teams_assigned: 0,
            complexity: 8,
            is_third_party: true,
        };

        ThirdPartyEngine {
            project,
            purchase_cost: 12_000_000.0,
            available_from: start,
        }
    };

    let hypergolic_thruster = {
        let mut flaws = Vec::new();
        let flaw_count: u32 = rng.gen_range(1..=3);
        for _ in 0..flaw_count {
            flaws.push(generate_third_party_flaw(&mut rng, &mut next_flaw_id));
        }

        let design = EngineDesign {
            id: EngineId(10003),
            name: "HT-40".into(),
            cycle: EngineCycle::PressureFed,
            thrust_n: 40_000.0,
            mass_kg: 90.0,
            isp_s: 267.0,
            exit_pressure_pa: 20_000.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::NTO, mass_fraction: 0.57 },
                PropellantFraction { propellant: Propellant::UDMH, mass_fraction: 0.43 },
            ],
        };

        let project = EngineProject {
            project_id: EngineProjectId(10003),
            design,
            preset: PropellantPreset::Hypergolic,
            scale: 1.0,
            status: EngineDesignStatus::Complete,
            flaws,
            revision: 0,
            teams_assigned: 0,
            complexity: 5,
            is_third_party: true,
        };

        ThirdPartyEngine {
            project,
            purchase_cost: 2_500_000.0,
            available_from: start,
        }
    };

    vec![solid_kick, nk33_analogue, hypergolic_thruster]
}

fn generate_third_party_flaw(rng: &mut impl Rng, next_flaw_id: &mut u64) -> Flaw {
    let id = FlawId(*next_flaw_id);
    *next_flaw_id += 1;

    let roll: f64 = rng.gen();
    let (consequence, activation_range) = if roll < 0.50 {
        let degradation = rng.gen_range(0.03..0.15);
        (FlawConsequence::PerformanceDegradation(degradation), (0.05, 0.40))
    } else if roll < 0.85 {
        (FlawConsequence::EngineLoss, (0.02, 0.25))
    } else {
        (FlawConsequence::StageLoss, (0.01, 0.15))
    };

    let activation_chance: f64 = rng.gen_range(activation_range.0..activation_range.1);
    // Third-party flaws have same discovery formula but only discoverable through flight
    let uniform_roll: f64 = rng.gen();
    let discovery_probability = uniform_roll * activation_chance.sqrt();

    let descriptions = [
        "Manufacturing variance",
        "Aging component degradation",
        "Undocumented operating limit",
        "Propellant compatibility issue",
        "Thermal cycling weakness",
        "Vibration resonance mode",
    ];
    let desc_idx = rng.gen_range(0..descriptions.len());

    Flaw {
        id,
        description: descriptions[desc_idx].to_string(),
        consequence,
        activation_chance,
        discovery_probability,
        discovered: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_starter_engines() {
        let seed = GameSeed::new(42);
        let engines = generate_starter_engines(&seed);
        assert_eq!(engines.len(), 3);

        // All available from start
        let start = GameDate::default_start();
        for e in &engines {
            assert_eq!(e.available_from, start);
            assert!(e.purchase_cost > 0.0);
            assert!(e.project.is_third_party);
            assert!(matches!(e.project.status, EngineDesignStatus::Complete));
        }

        // Verify types
        assert_eq!(engines[0].project.preset, PropellantPreset::Solid);
        assert_eq!(engines[1].project.preset, PropellantPreset::Kerolox);
        assert_eq!(engines[2].project.preset, PropellantPreset::Hypergolic);
    }

    #[test]
    fn test_starter_engines_deterministic() {
        let seed = GameSeed::new(42);
        let e1 = generate_starter_engines(&seed);
        let e2 = generate_starter_engines(&seed);

        for (a, b) in e1.iter().zip(e2.iter()) {
            assert_eq!(a.project.flaws.len(), b.project.flaws.len());
            assert_eq!(a.purchase_cost, b.purchase_cost);
        }
    }

    #[test]
    fn test_starter_engines_have_flaws() {
        let seed = GameSeed::new(42);
        let engines = generate_starter_engines(&seed);
        for e in &engines {
            assert!(!e.project.flaws.is_empty(), "{} should have flaws", e.project.design.name);
        }
    }

    #[test]
    fn test_third_party_flaws_undiscovered() {
        let seed = GameSeed::new(42);
        let engines = generate_starter_engines(&seed);
        for e in &engines {
            for flaw in &e.project.flaws {
                assert!(!flaw.discovered);
            }
        }
    }

    #[test]
    fn test_different_seeds_different_flaws() {
        let e1 = generate_starter_engines(&GameSeed::new(1));
        let e2 = generate_starter_engines(&GameSeed::new(2));
        // Flaw counts should differ for at least one engine
        let differ = e1.iter().zip(e2.iter())
            .any(|(a, b)| a.project.flaws.len() != b.project.flaws.len());
        assert!(differ, "Different seeds should produce different flaw counts");
    }
}
