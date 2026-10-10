use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::engine::{EngineDesign, EngineCycle, EngineId};
use crate::engine_project::PropellantPreset;
use rand::Rng;
use crate::flaw::{self, Flaw};
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
    /// What the engine is for. `None` on catalogues from before roles
    /// existed, whose names were fixed.
    #[serde(default)]
    pub role: Option<VendorRole>,
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

/// What a vendor engine is for, which is how the game (and the
/// incumbent) pick one without naming it: names are invented per world
/// (22_FLAWS.md §3.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VendorRole {
    /// A small solid kick motor.
    SolidKickMotor,
    /// A small hypergolic pressure-fed thruster.
    HypergolicThruster,
    /// A big kerolox staged-combustion booster engine (the NK-33 class).
    KeroloxBooster,
    /// A mid-size kerolox gas generator, not in every world.
    KeroloxGasGenerator,
    /// A hydrolox expander upper-stage engine built the way it was in
    /// the 1960s (the RL10 class): heavy, dear, and what the incumbent
    /// flies on top.
    LegacyHydroloxUpper,
}

impl VendorRole {
    pub fn label(self) -> &'static str {
        match self {
            VendorRole::SolidKickMotor => "solid kick motor",
            VendorRole::HypergolicThruster => "hypergolic thruster",
            VendorRole::KeroloxBooster => "kerolox booster",
            VendorRole::KeroloxGasGenerator => "kerolox gas generator",
            VendorRole::LegacyHydroloxUpper => "legacy hydrolox upper stage",
        }
    }
}

/// The vendors of a world: an invented name from the seed, as the
/// catalogue's engines are its products. Two syllables and an ending;
/// the model number carries the vendor's initials.
fn vendor_name(rng: &mut rand::rngs::StdRng) -> String {
    const FIRST: [&str; 14] = ["Vel", "Kor", "Ast", "Dyn", "Orb", "Sol", "Tor", "Nex", "Zen", "Hel", "Mar", "Cal", "Ard", "Pyr"];
    const SECOND: [&str; 10] = ["a", "o", "e", "i", "u", "ar", "en", "ir", "on", "al"];
    const END: [&str; 10] = ["tron", "dyne", "tek", "ion", "ix", "or", "cor", "ex", "ara", "thrust"];
    format!(
        "{}{}{}",
        FIRST[rng.gen_range(0..FIRST.len())],
        SECOND[rng.gen_range(0..SECOND.len())],
        END[rng.gen_range(0..END.len())],
    )
}

/// "Velotron VT-40": the vendor's initial, its next consonant, and a
/// number in the role's range.
fn model_name(vendor: &str, number: u32) -> String {
    let first = vendor.chars().next().unwrap_or('X').to_ascii_uppercase();
    let second = vendor.chars().skip(1)
        .find(|c| c.is_ascii_alphabetic() && !"aeiou".contains(*c))
        .unwrap_or('X')
        .to_ascii_uppercase();
    format!("{vendor} {first}{second}-{number}")
}

/// The starter catalogue, invented per world from the game's own
/// engine baselines: every world gets a solid kick motor, a hypergolic
/// thruster, a kerolox staged-combustion booster and the legacy
/// hydrolox upper stage, and some worlds a mid-size kerolox gas
/// generator too. Sizes, masses and Isp sit in a band around the
/// baseline (a vendor's product is not your design), prices come from
/// the cost model at the vendor's volume and markup, and the legacy
/// engine keeps its 1960s process.
pub fn generate_starter_engines(
    seed: &GameSeed, balance: &crate::balance_config::BalanceConfig,
) -> Vec<ThirdPartyEngine> {
    use crate::engine_project::{engine_baseline, scale_for_headline};
    let mut rng = seed.world_query(WorldQuery::ThirdPartyCatalog);
    let nozzle = &balance.nozzle;
    let start = GameDate::default_start();

    struct RoleTemplate {
        role: VendorRole,
        cycle: EngineCycle,
        preset: PropellantPreset,
        /// Thrust the vendor's product lands in, N.
        thrust: (f64, f64),
        sea_level_bell: bool,
        /// Mass against the game's baseline for that size.
        mass_factor: (f64, f64),
        /// The model number's range.
        numbers: (u32, u32),
    }
    let t = |role, cycle, preset, thrust, sea_level_bell, mass_factor, numbers| RoleTemplate {
        role, cycle, preset, thrust, sea_level_bell, mass_factor, numbers,
    };
    let roles = [
        t(VendorRole::SolidKickMotor, EngineCycle::PressureFed, PropellantPreset::Solid, (50_000.0, 120_000.0), true, (0.9, 1.15), (10, 30)),
        t(VendorRole::HypergolicThruster, EngineCycle::PressureFed, PropellantPreset::Hypergolic, (20_000.0, 60_000.0), false, (0.9, 1.15), (30, 60)),
        t(VendorRole::KeroloxBooster, EngineCycle::StagedCombustion, PropellantPreset::Kerolox, (1_200_000.0, 2_200_000.0), true, (0.95, 1.2), (100, 400)),
        t(VendorRole::KeroloxGasGenerator, EngineCycle::GasGenerator, PropellantPreset::Kerolox, (500_000.0, 1_000_000.0), true, (0.95, 1.2), (60, 90)),
        t(VendorRole::LegacyHydroloxUpper, EngineCycle::Expander, PropellantPreset::Hydrolox, (90_000.0, 130_000.0), false, (1.3, 1.6), (5, 20)),
    ];
    let has_gas_generator = rng.gen::<f64>() < 0.5;

    let mut out = Vec::new();
    for (k, RoleTemplate { role, cycle, preset, thrust, sea_level_bell, mass_factor, numbers }) in roles.into_iter().enumerate() {
        if role == VendorRole::KeroloxGasGenerator && !has_gas_generator {
            continue;
        }
        let vendor = vendor_name(&mut rng);
        let number = rng.gen_range(numbers.0..=numbers.1);
        let target_thrust = rng.gen_range(thrust.0..thrust.1);
        let mass_jitter = rng.gen_range(mass_factor.0..mass_factor.1);
        let isp_jitter = rng.gen_range(0.97..1.02);
        let Some(baseline) = engine_baseline(cycle, preset) else { continue };
        let scale = scale_for_headline(cycle, preset, target_thrust, nozzle).unwrap_or(1.0);
        let name = model_name(&vendor, number);
        let mut design = baseline.design(EngineId(10_001 + k as u64), name, cycle, preset, scale, !sea_level_bell, nozzle);
        design.mass_kg *= mass_jitter;
        design.isp_s *= isp_jitter;
        let complexity = crate::balance::combined_complexity(cycle, &preset.propellants());
        let (build_time_multiplier, legacy, available_from) = match role {
            VendorRole::LegacyHydroloxUpper =>
                (3.0, Some("1960s hand-brazed tube-wall chamber, ×3 build time".to_string()), start),
            VendorRole::KeroloxGasGenerator =>
                (1.0, None, start.add_days(rng.gen_range(0..=3 * 365))),
            _ => (1.0, None, start),
        };
        out.push(ThirdPartyEngine {
            purchase_cost_per_unit: catalogue_price(&design, preset, complexity, build_time_multiplier, balance),
            design,
            preset,
            complexity,
            available_from,
            build_time_multiplier,
            legacy,
            role: Some(role),
        });
    }
    out
}

/// Generate flaws for a third-party engine using scaled-down complexity.
/// Third-party engines are mature designs, so they use complexity/8 (min 1).
pub fn generate_third_party_flaws(
    complexity: u32,
    seed: &GameSeed,
    engine_name: &str,
    features: &[flaw::Feature],
    next_flaw_id: &mut crate::id::IdAllocator<crate::flaw::FlawId>,
    flaws_cfg: &crate::balance_config::FlawsConfig,
) -> Vec<Flaw> {
    let effective = (complexity / 8).max(1);
    let mut rng = seed.world_query(WorldQuery::ThirdPartyFlaws(engine_name));
    // Not your drawings: what you find is mostly in the integration and
    // the flying, so the engine mix gives way to the vendor mix.
    let mut cfg = flaws_cfg.clone();
    cfg.engine_exposure_mix = flaws_cfg.third_party_exposure_mix;
    flaw::generate_flaws(flaw::FlawDomain::Engine(None), effective, 0, features, &mut rng, next_flaw_id, &cfg)
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
            assert!(e.purchase_cost_per_unit > 0.0);
            if e.role != Some(VendorRole::KeroloxGasGenerator) {
                assert_eq!(e.available_from, start);
            }
        }
        let by_role = |role: VendorRole| engines.iter().find(|e| e.role == Some(role)).expect("role present");
        assert_eq!(by_role(VendorRole::SolidKickMotor).preset, PropellantPreset::Solid);
        assert_eq!(by_role(VendorRole::KeroloxBooster).preset, PropellantPreset::Kerolox);
        assert_eq!(by_role(VendorRole::HypergolicThruster).preset, PropellantPreset::Hypergolic);
        let legacy = by_role(VendorRole::LegacyHydroloxUpper);
        assert!(legacy.legacy.is_some());
        assert!(legacy.design.name.contains('-'), "a model name: {}", legacy.design.name);
    }

    #[test]
    fn test_third_party_flaw_scaling() {
        let seed = GameSeed::new(42);
        let mut next_flaw_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(10000);

        // Complexity 8 -> effective 1, should produce few flaws on average
        let mut total_flaws = 0;
        for i in 0..100 {
            let seed_i = GameSeed::new(i);
            let flaws = generate_third_party_flaws(8, &seed_i, "test", &[], &mut next_flaw_id, &crate::balance_config::FlawsConfig::default());
            total_flaws += flaws.len();
        }
        let avg = total_flaws as f64 / 100.0;
        assert!(avg < 3.0, "Avg flaw count {} should be low for complexity 8 (effective 1)", avg);

        // Complexity 5 -> effective 1 (5/8 = 0, clamped to 1)
        let flaws = generate_third_party_flaws(5, &seed, "solid", &[], &mut next_flaw_id, &crate::balance_config::FlawsConfig::default());
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
        let f1 = generate_third_party_flaws(8, &seed, "RD-33K", &[], &mut id1, &crate::balance_config::FlawsConfig::default());
        let f2 = generate_third_party_flaws(8, &seed, "RD-33K", &[], &mut id2, &crate::balance_config::FlawsConfig::default());
        assert_eq!(f1.len(), f2.len());
    }
}
