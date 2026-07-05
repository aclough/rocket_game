use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::engine::{EngineDesign, EngineCycle, EngineId, PropellantFraction};
use crate::engine_project::PropellantPreset;
use crate::flaw::{self, Flaw};
use crate::propellant::Propellant;
use crate::seed::GameSeed;

/// Unique identifier for a contracted third-party engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContractedEngineId(pub u64);

/// A contracted third-party engine available for use in rocket builds.
/// Per-unit cost is charged when building rockets that use this engine.
/// No manufacturing work or team assignment required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractedEngine {
    pub id: ContractedEngineId,
    pub design: EngineDesign,
    pub preset: PropellantPreset,
    pub purchase_cost_per_unit: f64,
    pub flaws: Vec<Flaw>,
    pub complexity: u32,
}

/// A third-party engine available in the catalog for contracting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyEngine {
    pub design: EngineDesign,
    pub preset: PropellantPreset,
    pub complexity: u32,
    pub purchase_cost_per_unit: f64,
    pub available_from: GameDate,
}

/// Generate the starter third-party engines from the game seed.
///
/// Returns 3 engines:
/// 1. Small solid kick motor
/// 2. Medium kerolox engine (NK-33 analogue)
/// 3. Small hypergolic thruster
pub fn generate_starter_engines(_seed: &GameSeed) -> Vec<ThirdPartyEngine> {
    let start = GameDate::default_start();

    vec![
        ThirdPartyEngine {
            design: EngineDesign {
                id: EngineId(10001),
                name: "KM-15 Kick Motor".into(),
                cycle: EngineCycle::PressureFed,
                thrust_n: 75_000.0,
                mass_kg: 35.0,
                isp_s: 245.0,
                exit_pressure_pa: 70_000.0, // sea-level optimized SRM
                needs_atmosphere: false,
                propellant_mix: vec![
                    PropellantFraction { propellant: Propellant::SolidMix, mass_fraction: 1.0 },
                ],
                power_draw_w: 0.0,
            },
            preset: PropellantPreset::Solid,
            complexity: 5,
            purchase_cost_per_unit: 800_000.0,
            available_from: start,
        },
        ThirdPartyEngine {
            design: EngineDesign {
                id: EngineId(10002),
                name: "RD-33K".into(),
                cycle: EngineCycle::StagedCombustion,
                thrust_n: 1_680_000.0,
                mass_kg: 1_220.0,
                isp_s: 297.0,
                exit_pressure_pa: 80_000.0, // sea-level optimized kerolox
                needs_atmosphere: false,
                propellant_mix: vec![
                    PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.73 },
                    PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.27 },
                ],
                power_draw_w: 0.0,
            },
            preset: PropellantPreset::Kerolox,
            complexity: 8,
            purchase_cost_per_unit: 12_000_000.0,
            available_from: start,
        },
        ThirdPartyEngine {
            design: EngineDesign {
                id: EngineId(10003),
                name: "HT-40".into(),
                cycle: EngineCycle::PressureFed,
                thrust_n: 40_000.0,
                mass_kg: 90.0,
                isp_s: 267.0,
                exit_pressure_pa: 7_000.0, // vacuum-optimized hypergolic
                needs_atmosphere: false,
                propellant_mix: vec![
                    PropellantFraction { propellant: Propellant::NTO, mass_fraction: 0.57 },
                    PropellantFraction { propellant: Propellant::UDMH, mass_fraction: 0.43 },
                ],
                power_draw_w: 0.0,
            },
            preset: PropellantPreset::Hypergolic,
            complexity: 5,
            purchase_cost_per_unit: 2_500_000.0,
            available_from: start,
        },
    ]
}

/// Generate flaws for a third-party engine using scaled-down complexity.
/// Third-party engines are mature designs, so they use complexity/8 (min 1).
pub fn generate_third_party_flaws(
    complexity: u32,
    seed: &GameSeed,
    engine_name: &str,
    next_flaw_id: &mut u64,
    flaws_cfg: &crate::balance_config::FlawsConfig,
) -> Vec<Flaw> {
    let effective = (complexity / 8).max(1);
    let mut rng = seed.world_query(&format!("3p_flaws_{}", engine_name));
    flaw::generate_flaws(effective, &mut rng, next_flaw_id, flaws_cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_starter_engines() {
        let engines = generate_starter_engines(&GameSeed::new(42));
        assert_eq!(engines.len(), 3);

        let start = GameDate::default_start();
        for e in &engines {
            assert_eq!(e.available_from, start);
            assert!(e.purchase_cost_per_unit > 0.0);
        }

        assert_eq!(engines[0].preset, PropellantPreset::Solid);
        assert_eq!(engines[1].preset, PropellantPreset::Kerolox);
        assert_eq!(engines[2].preset, PropellantPreset::Hypergolic);
    }

    #[test]
    fn test_third_party_flaw_scaling() {
        let seed = GameSeed::new(42);
        let mut next_flaw_id = 10000u64;

        // Complexity 8 -> effective 1, should produce few flaws on average
        let mut total_flaws = 0;
        for i in 0..100 {
            let seed_i = GameSeed::new(i);
            let flaws = generate_third_party_flaws(8, &seed_i, "test", &mut next_flaw_id, &crate::balance_config::FlawsConfig::default());
            total_flaws += flaws.len();
        }
        let avg = total_flaws as f64 / 100.0;
        assert!(avg < 3.0, "Avg flaw count {} should be low for complexity 8 (effective 1)", avg);

        // Complexity 5 -> effective 1 (5/8 = 0, clamped to 1)
        let flaws = generate_third_party_flaws(5, &seed, "solid", &mut next_flaw_id, &crate::balance_config::FlawsConfig::default());
        // Just check they're generated, exact count is random
        for flaw in &flaws {
            assert!(!flaw.discovered);
        }
    }

    #[test]
    fn test_third_party_flaws_deterministic() {
        let seed = GameSeed::new(42);
        let mut id1 = 10000u64;
        let mut id2 = 10000u64;
        let f1 = generate_third_party_flaws(8, &seed, "RD-33K", &mut id1, &crate::balance_config::FlawsConfig::default());
        let f2 = generate_third_party_flaws(8, &seed, "RD-33K", &mut id2, &crate::balance_config::FlawsConfig::default());
        assert_eq!(f1.len(), f2.len());
    }
}
