use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::engine::{EngineDesign, Propulsion, EngineCycle, EngineId, PropellantFraction};
use crate::engine_project::PropellantPreset;
use crate::flaw::{self, Flaw};
use crate::propellant::Propellant;
use crate::seed::{GameSeed, WorldQuery};

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
    /// The legacy process that makes it dear, if any (from the catalogue).
    #[serde(default)]
    pub legacy: Option<String>,
}

/// A third-party engine available in the catalog for contracting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyEngine {
    pub design: EngineDesign,
    pub preset: PropellantPreset,
    pub complexity: u32,
    pub purchase_cost_per_unit: f64,
    pub available_from: GameDate,
    /// Multiplier on the vendor's build work from a legacy process — a
    /// design laid out when its technology was young and never
    /// re-engineered (21_ENGINES.md §1.1: the RL10 is a 1960 engine).
    /// Fixed: a vendor's legacy is not something a buyer's revision
    /// can solve.
    #[serde(default = "crate::project::unit_multiplier")]
    pub build_time_multiplier: f64,
    /// What that legacy is, for the catalogue.
    #[serde(default)]
    pub legacy: Option<String>,
}

/// What a catalogue engine sells for: the vendor's unit cost under the
/// same model as the player's engines (materials plus build work at the
/// line salary, legacy penalty included) at the vendor's volume, times
/// its markup (`ThirdPartyConfig`).
pub fn catalogue_price(
    design: &EngineDesign, preset: PropellantPreset, complexity: u32, build_time_multiplier: f64,
    balance: &crate::balance_config::BalanceConfig,
) -> f64 {
    let materials = crate::resources::engine_material_cost(preset, design.mass_kg, &balance.costs.resource_prices);
    let work = balance.work.engine_build_work(complexity, design.mass_kg, balance.propellant_handling.multiplier(preset))
        * build_time_multiplier;
    let unit = (materials + work * balance.costs.daily_manufacturing_salary())
        * balance.work.learning_curve_multiplier(balance.third_party.vendor_prior_builds);
    unit * balance.third_party.markup
}

/// Name of the legacy hydrolox upper-stage engine in the catalogue — the
/// RL10 analogue DinoSoar buys for its upper stage.
pub const DACTYL_NAME: &str = "DL-10 Dactyl";

/// The starter third-party engines, priced from the cost model:
/// 1. Small solid kick motor
/// 2. Medium kerolox engine (NK-33 analogue)
/// 3. Small hypergolic thruster
/// 4. Legacy hydrolox upper-stage engine (RL10 analogue), dear for its
///    1960s process (21_ENGINES.md step 8)
pub fn generate_starter_engines(
    _seed: &GameSeed, balance: &crate::balance_config::BalanceConfig,
) -> Vec<ThirdPartyEngine> {
    let start = GameDate::default_start();
    let entry = |design: EngineDesign, preset: PropellantPreset, build_time_multiplier: f64, legacy: Option<&str>| {
        let complexity = crate::balance::combined_complexity(design.cycle, &preset.propellants());
        ThirdPartyEngine {
            purchase_cost_per_unit: catalogue_price(&design, preset, complexity, build_time_multiplier, balance),
            design,
            preset,
            complexity,
            available_from: start,
            build_time_multiplier,
            legacy: legacy.map(str::to_string),
        }
    };

    vec![
        entry(EngineDesign {
            id: EngineId(10001),
            name: "KM-15 Kick Motor".into(),
            cycle: EngineCycle::PressureFed,
            thrust_n: 75_000.0,
            mass_kg: 35.0,
            isp_s: 245.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::SolidMix, mass_fraction: 1.0 },
            ],
            propulsion: Propulsion::nozzle(6_000_000.0, 10.56, 1.2, true),
        }, PropellantPreset::Solid, 1.0, None),
        entry(EngineDesign {
            id: EngineId(10002),
            name: "RD-33K".into(),
            cycle: EngineCycle::StagedCombustion,
            thrust_n: 1_680_000.0,
            mass_kg: 1_220.0,
            isp_s: 297.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.73 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.27 },
            ],
            propulsion: Propulsion::nozzle(25_000_000.0, 28.61, 1.2, true),
        }, PropellantPreset::Kerolox, 1.0, None),
        entry(EngineDesign {
            id: EngineId(10003),
            name: "HT-40".into(),
            cycle: EngineCycle::PressureFed,
            thrust_n: 40_000.0,
            mass_kg: 90.0,
            isp_s: 267.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::NTO, mass_fraction: 0.57 },
                PropellantFraction { propellant: Propellant::UDMH, mass_fraction: 0.43 },
            ],
            propulsion: Propulsion::nozzle(1_000_000.0, 15.59, 1.2, false),
        }, PropellantPreset::Hypergolic, 1.0, None),
        // RL10A-4-class: 110 kN, ~465 s, heavy for its thrust, and built
        // by hand the way it was in 1962.
        entry(EngineDesign {
            id: EngineId(10004),
            name: DACTYL_NAME.into(),
            cycle: EngineCycle::Expander,
            thrust_n: 110_000.0,
            mass_kg: 300.0,
            isp_s: 462.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.83 },
                PropellantFraction { propellant: Propellant::LH2, mass_fraction: 0.17 },
            ],
            propulsion: Propulsion::nozzle(4_500_000.0, 65.85, 1.2, false),
        }, PropellantPreset::Hydrolox, 3.0, Some("1960s hand-brazed tube-wall chamber, ×3 build time")),
    ]
}

/// Generate flaws for a third-party engine using scaled-down complexity.
/// Third-party engines are mature designs, so they use complexity/8 (min 1).
pub fn generate_third_party_flaws(
    complexity: u32,
    seed: &GameSeed,
    engine_name: &str,
    next_flaw_id: &mut crate::id::IdAllocator<crate::flaw::FlawId>,
    flaws_cfg: &crate::balance_config::FlawsConfig,
) -> Vec<Flaw> {
    let effective = (complexity / 8).max(1);
    let mut rng = seed.world_query(WorldQuery::ThirdPartyFlaws(engine_name));
    flaw::generate_flaws(flaw::FlawDomain::Engine(None), effective, 0, &mut rng, next_flaw_id, flaws_cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_starter_engines() {
        let engines = generate_starter_engines(&GameSeed::new(42), &crate::balance_config::BalanceConfig::default());
        assert_eq!(engines.len(), 4);

        let start = GameDate::default_start();
        for e in &engines {
            assert_eq!(e.available_from, start);
            assert!(e.purchase_cost_per_unit > 0.0);
        }

        assert_eq!(engines[0].preset, PropellantPreset::Solid);
        assert_eq!(engines[1].preset, PropellantPreset::Kerolox);
        assert_eq!(engines[2].preset, PropellantPreset::Hypergolic);
        assert_eq!(engines[3].design.name, DACTYL_NAME);
        assert!(engines[3].legacy.is_some());
    }

    #[test]
    fn test_third_party_flaw_scaling() {
        let seed = GameSeed::new(42);
        let mut next_flaw_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(10000);

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
        let mut id1 = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(10000);
        let mut id2 = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(10000);
        let f1 = generate_third_party_flaws(8, &seed, "RD-33K", &mut id1, &crate::balance_config::FlawsConfig::default());
        let f2 = generate_third_party_flaws(8, &seed, "RD-33K", &mut id2, &crate::balance_config::FlawsConfig::default());
        assert_eq!(f1.len(), f2.len());
    }
}
