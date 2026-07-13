//! Tunable game-balance parameters, gathered into one TOML-loadable
//! struct so the simulation harness can sweep them without recompiling.
//!
//! `BalanceConfig::default()` is the single source of truth for the
//! shipped values — TOML files are partial overrides layered on top
//! (see [`BalanceConfig::load_layered`]). Deliberately excluded:
//! complexity tables (`balance.rs`), tech/deficiency generation
//! (seed-entangled), physics constants, and UI mechanics.

use std::path::Path;

use serde::{Serialize, Deserialize};

use crate::contract::MarketArchetype;
use crate::resources::Resource;

/// All tunable balance parameters. Lives on `GameState` (serialized
/// into saves, so a save remembers the balance it was played under).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BalanceConfig {
    pub costs: CostsConfig,
    pub work: WorkConfig,
    pub markets: MarketsConfig,
    pub flaws: FlawsConfig,
    pub reputation: ReputationConfig,
}

impl BalanceConfig {
    /// Build a config by layering TOML files over the compiled-in
    /// defaults. Files are deep-merged in order: later files win, and
    /// any field absent everywhere keeps its default. Arrays (e.g. the
    /// market table) are replaced wholesale, not merged element-wise.
    pub fn load_layered<P: AsRef<Path>>(paths: &[P]) -> Result<Self, String> {
        let default_tree = toml::Value::try_from(BalanceConfig::default())
            .map_err(|e| format!("serializing default balance config: {e}"))?;
        let mut merged = default_tree.clone();
        for path in paths {
            let path = path.as_ref();
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("reading {}: {e}", path.display()))?;
            let overlay: toml::Value = text.parse()
                .map_err(|e| format!("parsing {}: {e}", path.display()))?;
            // Typos in sweep files should fail loudly, not silently no-op.
            check_unknown_keys(&default_tree, &overlay, "")
                .map_err(|key| format!("{}: unknown balance key `{key}`", path.display()))?;
            deep_merge(&mut merged, overlay);
        }
        let config: BalanceConfig = merged.try_into()
            .map_err(|e| format!("invalid balance config: {e}"))?;
        config.markets.validate()?;
        Ok(config)
    }

    /// The full effective config as TOML — the generated reference file
    /// (`--dump-balance`), always in sync with the code defaults.
    pub fn to_toml_string(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|e| format!("serializing balance config: {e}"))
    }
}

/// Verify every table key path in `overlay` exists in the default
/// config tree. Returns the first unknown key path on failure. Array
/// contents are not checked (arrays are replaced wholesale and get
/// validated by the final deserialize).
fn check_unknown_keys(
    default_tree: &toml::Value,
    overlay: &toml::Value,
    path: &str,
) -> Result<(), String> {
    if let (toml::Value::Table(default_table), toml::Value::Table(overlay_table)) =
        (default_tree, overlay)
    {
        for (key, value) in overlay_table {
            let key_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            match default_table.get(key) {
                Some(default_value) => check_unknown_keys(default_value, value, &key_path)?,
                None => return Err(key_path),
            }
        }
    }
    Ok(())
}

/// Recursively merge `overlay` into `base`. Tables merge key-by-key;
/// everything else (scalars, arrays) is replaced by the overlay value.
fn deep_merge(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, value) in overlay_table {
                match base_table.get_mut(&key) {
                    Some(existing) => deep_merge(existing, value),
                    None => { base_table.insert(key, value); }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

// ==========================================
// Costs
// ==========================================

/// Money: starting capital, salaries, facilities, and material prices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CostsConfig {
    /// Starting capital for a new game.
    pub starting_money: f64,
    /// Monthly salary for an engineering team (~8-10 engineers).
    pub engineering_monthly_salary: f64,
    /// One-time hiring cost for an engineering team.
    pub engineering_hiring_cost: f64,
    /// Monthly salary for a manufacturing team (~20-25 workers).
    pub manufacturing_monthly_salary: f64,
    /// One-time hiring cost for a manufacturing team.
    pub manufacturing_hiring_cost: f64,
    /// Cost per unit of manufacturing floor space.
    pub floor_space_cost: f64,
    /// Days to build one floor-space expansion order.
    pub floor_space_build_days: u32,
    /// Floor space units a new company starts with.
    pub starting_floor_space: u32,
    /// Material cost of a scale-1.0 reference reactor.
    pub reactor_ref_material_cost: f64,
    /// Price per kilogram for each manufacturing resource.
    pub resource_prices: ResourcePrices,
}

impl Default for CostsConfig {
    fn default() -> Self {
        CostsConfig {
            starting_money: 200_000_000.0,
            engineering_monthly_salary: 150_000.0,
            engineering_hiring_cost: 150_000.0,
            manufacturing_monthly_salary: 300_000.0,
            manufacturing_hiring_cost: 900_000.0,
            floor_space_cost: 5_000_000.0,
            floor_space_build_days: 30,
            starting_floor_space: 12,
            reactor_ref_material_cost: 30_000_000.0,
            resource_prices: ResourcePrices::default(),
        }
    }
}

/// Price per kilogram in dollars for each resource.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResourcePrices {
    pub aluminium: f64,
    pub steel: f64,
    pub superalloys: f64,
    pub composites: f64,
    pub wiring: f64,
    pub electronics: f64,
    pub plumbing: f64,
    pub solid_propellant: f64,
    /// Highly Enriched Uranium — very expensive, regulated material.
    pub heu: f64,
}

impl Default for ResourcePrices {
    fn default() -> Self {
        ResourcePrices {
            aluminium: 5.0,
            steel: 3.0,
            superalloys: 80.0,
            composites: 50.0,
            wiring: 150.0,
            electronics: 20_000.0,
            plumbing: 1_500.0,
            solid_propellant: 15.0,
            heu: 100_000.0,
        }
    }
}

impl ResourcePrices {
    pub fn price_per_kg(&self, resource: Resource) -> f64 {
        match resource {
            Resource::Aluminium => self.aluminium,
            Resource::Steel => self.steel,
            Resource::Superalloys => self.superalloys,
            Resource::Composites => self.composites,
            Resource::Wiring => self.wiring,
            Resource::Electronics => self.electronics,
            Resource::Plumbing => self.plumbing,
            Resource::SolidPropellant => self.solid_propellant,
            Resource::HEU => self.heu,
        }
    }
}

// ==========================================
// Work / time
// ==========================================

/// Design, build, and testing work formulas (all in team-days).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkConfig {
    /// Base days for engine design at complexity 5.
    pub engine_design_base_days: f64,
    /// Base days for rocket design at complexity 5.
    pub rocket_design_base_days: f64,
    /// Base days to build an engine at complexity 5.
    pub engine_build_base_days: f64,
    /// Base days to build a 10-tonne stage.
    pub stage_build_base_days: f64,
    /// Exponent on (stage mass / 10 t) for stage build work.
    pub stage_build_mass_exponent: f64,
    /// Flat work for rocket integration.
    pub rocket_integration_base_days: f64,
    /// Additional integration work per stage.
    pub rocket_integration_days_per_stage: f64,
    /// Learning-curve exponent: cost multiplier = builds^exponent
    /// (-0.15 ≈ a 90% learning curve).
    pub learning_curve_exponent: f64,
    /// Fraction of a rocket's full design work charged for an
    /// in-flight modification (tankage / power tweak).
    pub rocket_modification_work_fraction: f64,
    /// Work units required to fix one flaw via revision.
    pub flaw_revision_work: f64,
    /// Work units per testing cycle.
    pub testing_cycle_work: f64,
}

impl Default for WorkConfig {
    fn default() -> Self {
        WorkConfig {
            engine_design_base_days: 120.0,
            rocket_design_base_days: 60.0,
            engine_build_base_days: 90.0,
            stage_build_base_days: 60.0,
            stage_build_mass_exponent: 0.75,
            rocket_integration_base_days: 20.0,
            rocket_integration_days_per_stage: 30.0,
            learning_curve_exponent: -0.15,
            rocket_modification_work_fraction: 0.10,
            flaw_revision_work: 30.0,
            testing_cycle_work: 30.0,
        }
    }
}

impl WorkConfig {
    /// Work required in days for engine design:
    /// base_days * (complexity / 5).
    pub fn design_work_required(&self, complexity: u32) -> f64 {
        self.engine_design_base_days * (complexity as f64 / 5.0)
    }

    /// Work required in days for rocket design:
    /// base_days * (complexity / 5), shorter base than engines.
    pub fn rocket_design_work_required(&self, complexity: u32) -> f64 {
        self.rocket_design_base_days * (complexity as f64 / 5.0)
    }

    /// Work required in days for engine manufacturing:
    /// base_days * (complexity / 5).
    pub fn engine_build_work(&self, complexity: u32) -> f64 {
        self.engine_build_base_days * (complexity as f64 / 5.0)
    }

    /// Work required in days for stage manufacturing, based on mass.
    pub fn stage_build_work(&self, stage_mass_kg: f64) -> f64 {
        self.stage_build_base_days
            * (stage_mass_kg / 10_000.0_f64).powf(self.stage_build_mass_exponent)
    }

    /// Work required for rocket integration: base + per-stage.
    pub fn rocket_integration_work(&self, total_stages: u32) -> f64 {
        self.rocket_integration_base_days
            + self.rocket_integration_days_per_stage * total_stages as f64
    }

    /// Learning curve cost multiplier for repeated builds: each
    /// doubling of production cuts cost by ~10% at the default exponent.
    pub fn learning_curve_multiplier(&self, total_built: u32) -> f64 {
        if total_built == 0 {
            1.0
        } else {
            (total_built as f64).powf(self.learning_curve_exponent)
        }
    }
}

// ==========================================
// Markets / contracts
// ==========================================

/// Contract-generation parameters plus the market archetype table
/// that the per-seed realization layer draws from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MarketsConfig {
    /// Minimum contract deadline in days from issue.
    pub deadline_min_days: u32,
    /// Maximum contract deadline in days from issue.
    pub deadline_max_days: u32,
    /// Lower bound of the per-contract payment variance multiplier.
    pub payment_variance_min: f64,
    /// Upper bound of the per-contract payment variance multiplier.
    pub payment_variance_max: f64,
    /// Market templates + perturbation specs, realized per seed at
    /// game start (see [`crate::contract::MarketArchetype`]).
    pub archetypes: Vec<MarketArchetype>,
}

impl Default for MarketsConfig {
    fn default() -> Self {
        MarketsConfig {
            deadline_min_days: 60,
            deadline_max_days: 180,
            payment_variance_min: 0.8,
            payment_variance_max: 1.2,
            archetypes: crate::contract::default_archetypes(),
        }
    }
}

impl MarketsConfig {
    /// Structural checks a TOML sweep must not violate. The key rule
    /// is additive-only year-1 variance: markets visible at
    /// reputation 0 from game start form the guaranteed opening
    /// floor, so their per-seed draws may only raise them.
    pub fn validate(&self) -> Result<(), String> {
        let mut keys = std::collections::HashSet::new();
        let mut ids = std::collections::HashSet::new();
        for a in &self.archetypes {
            if !keys.insert(a.key.as_str()) {
                return Err(format!("duplicate market archetype key `{}`", a.key));
            }
            if !ids.insert(a.template.id) {
                return Err(format!(
                    "archetype `{}`: duplicate market id {}", a.key, a.template.id.0,
                ));
            }
            if !(0.0..=1.0).contains(&a.presence_probability) {
                return Err(format!(
                    "archetype `{}`: presence_probability {} outside [0, 1]",
                    a.key, a.presence_probability,
                ));
            }
            for (name, range) in [
                ("volume_mult_range", a.volume_mult_range),
                ("rate_mult_range", a.rate_mult_range),
            ] {
                if range.0 > range.1 || range.0 <= 0.0 {
                    return Err(format!(
                        "archetype `{}`: {} ({}, {}) must be ordered and positive",
                        a.key, name, range.0, range.1,
                    ));
                }
            }
            if !(0.0..1.0).contains(&a.weight_tilt_strength) {
                return Err(format!(
                    "archetype `{}`: weight_tilt_strength {} outside [0, 1)",
                    a.key, a.weight_tilt_strength,
                ));
            }
            if let Some(e) = &a.emergence {
                if e.year_range.0 > e.year_range.1 {
                    return Err(format!(
                        "archetype `{}`: emergence year_range ({}, {}) is reversed",
                        a.key, e.year_range.0, e.year_range.1,
                    ));
                }
            }
            // Additive-only rule for the reputation-0 opening floor.
            if a.template.min_reputation <= 0.0 && a.emergence.is_none() {
                if a.presence_probability < 1.0 {
                    return Err(format!(
                        "archetype `{}`: opening-floor market (min_reputation <= 0, \
                         start-active) must have presence_probability 1.0",
                        a.key,
                    ));
                }
                if a.volume_mult_range.0 < 1.0 || a.rate_mult_range.0 < 1.0 {
                    return Err(format!(
                        "archetype `{}`: opening-floor market (min_reputation <= 0, \
                         start-active) must have multiplier floors >= 1.0 \
                         (additive-only year-1 variance)",
                        a.key,
                    ));
                }
            }
        }
        Ok(())
    }
}

// ==========================================
// Flaws & risk
// ==========================================

/// Flaw generation and related risk parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlawsConfig {
    /// Stddev of the gaussian flaw count (mean = effective complexity).
    pub count_stddev: f64,
    /// Probability a flaw is a performance degradation.
    pub performance_degradation_weight: f64,
    /// Probability a flaw is an engine/part loss (the remainder after
    /// degradation + engine loss is stage loss).
    pub engine_loss_weight: f64,
    /// Minimum performance-degradation fraction.
    pub degradation_min: f64,
    /// Maximum performance-degradation fraction.
    pub degradation_max: f64,
    /// Fraction of rocket flaws that are PerDay endurance flaws.
    pub rocket_endurance_fraction: f64,
    /// Fraction of reactor flaws that are PerDay endurance flaws.
    pub reactor_endurance_fraction: f64,
    /// Chance per testing cycle to discover an engine improvement.
    pub improvement_discovery_chance: f64,
    /// Chance per testing cycle to discover a reactor improvement.
    pub reactor_improvement_discovery_chance: f64,
    /// Flat probability that a rocket modification introduces a new
    /// undiscovered flaw.
    pub modification_flaw_prob: f64,
}

impl Default for FlawsConfig {
    fn default() -> Self {
        FlawsConfig {
            count_stddev: 1.5,
            performance_degradation_weight: 0.50,
            engine_loss_weight: 0.35,
            degradation_min: 0.03,
            degradation_max: 0.15,
            rocket_endurance_fraction: 0.30,
            reactor_endurance_fraction: 0.30,
            improvement_discovery_chance: 0.08,
            reactor_improvement_discovery_chance: 0.08,
            modification_flaw_prob: 0.10,
        }
    }
}

// ==========================================
// Reputation
// ==========================================

/// Reputation gains, penalties, decay factors, and gates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReputationConfig {
    /// Added to the success factor per successful launch.
    pub success_gain: f64,
    /// Subtracted from the success factor per failed launch.
    pub failure_penalty: f64,
    /// Subtracted from the lost-payload factor when a payload is lost.
    pub lost_payload_penalty: f64,
    /// Subtracted from the success factor on a partial failure.
    pub partial_failure_penalty: f64,
    /// Success factor decay multiplier applied each launch.
    pub success_decay: f64,
    /// Lost-payload factor decay multiplier applied each launch.
    pub lost_payload_decay: f64,
    /// Expiry factor decay multiplier applied each contract launch.
    pub expiry_decay: f64,
    /// Subtracted from the expiry factor per expired accepted contract.
    pub expiry_penalty: f64,
    /// Subtracted from the drought factor per year without a launch.
    pub drought_penalty: f64,
    /// Total reputation required to design a medium-enriched-uranium
    /// reactor. Naval / research-reactor territory.
    pub reactor_meu_min_reputation: f64,
    /// Total reputation required to design a highly-enriched-uranium
    /// reactor. Kilopower / weapons-grade.
    pub reactor_heu_min_reputation: f64,
}

impl Default for ReputationConfig {
    fn default() -> Self {
        ReputationConfig {
            success_gain: 20.0,
            failure_penalty: 20.0,
            lost_payload_penalty: 50.0,
            partial_failure_penalty: 10.0,
            success_decay: 0.8,
            lost_payload_decay: 0.85,
            expiry_decay: 0.8,
            expiry_penalty: 10.0,
            drought_penalty: 10.0,
            reactor_meu_min_reputation: 60.0,
            reactor_heu_min_reputation: 150.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_round_trips_through_toml() {
        let config = BalanceConfig::default();
        let text = config.to_toml_string().expect("serialize");
        let parsed: BalanceConfig = toml::from_str(&text).expect("parse");
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_load_layered_no_files_is_default() {
        let config = BalanceConfig::load_layered::<&str>(&[]).expect("load");
        assert_eq!(config, BalanceConfig::default());
    }

    #[test]
    fn test_partial_toml_only_overrides_named_fields() {
        let mut merged = toml::Value::try_from(BalanceConfig::default()).unwrap();
        let overlay: toml::Value =
            "[costs]\nstarting_money = 50000000.0\n".parse().unwrap();
        deep_merge(&mut merged, overlay);
        let config: BalanceConfig = merged.try_into().unwrap();
        assert_eq!(config.costs.starting_money, 50_000_000.0);
        // Everything else untouched
        let default = BalanceConfig::default();
        assert_eq!(config.costs.engineering_monthly_salary,
            default.costs.engineering_monthly_salary);
        assert_eq!(config.work, default.work);
        assert_eq!(config.markets, default.markets);
    }

    #[test]
    fn test_layered_files_later_wins() {
        let dir = std::env::temp_dir();
        let base = dir.join("rt_balance_test_base.toml");
        let over = dir.join("rt_balance_test_over.toml");
        std::fs::write(&base,
            "[work]\nengine_design_base_days = 100.0\nrocket_design_base_days = 50.0\n").unwrap();
        std::fs::write(&over, "[work]\nengine_design_base_days = 80.0\n").unwrap();
        let config = BalanceConfig::load_layered(&[&base, &over]).expect("load");
        std::fs::remove_file(&base).ok();
        std::fs::remove_file(&over).ok();
        // Later file wins where both set a value
        assert_eq!(config.work.engine_design_base_days, 80.0);
        // Earlier file's other override survives
        assert_eq!(config.work.rocket_design_base_days, 50.0);
        // Untouched fields keep defaults
        assert_eq!(config.work.testing_cycle_work, 30.0);
    }

    #[test]
    fn test_unknown_key_in_file_is_rejected() {
        let path = std::env::temp_dir().join("rt_balance_test_typo.toml");
        std::fs::write(&path, "[costs]\nstartng_money = 1.0\n").unwrap();
        let result = BalanceConfig::load_layered(&[&path]);
        std::fs::remove_file(&path).ok();
        let err = result.expect_err("typo key should be rejected");
        assert!(err.contains("costs.startng_money"), "error should name the key: {err}");
    }

    #[test]
    fn test_work_formulas_match_defaults() {
        let work = WorkConfig::default();
        assert!((work.design_work_required(5) - 120.0).abs() < 0.01);
        assert!((work.design_work_required(9) - 216.0).abs() < 0.01);
        assert!((work.rocket_design_work_required(5) - 60.0).abs() < 0.01);
        assert!((work.engine_build_work(6) - 108.0).abs() < 0.01);
        assert!((work.stage_build_work(10_000.0) - 60.0).abs() < 0.01);
        assert!((work.rocket_integration_work(2) - 80.0).abs() < 0.01);
        assert!((work.learning_curve_multiplier(1) - 1.0).abs() < 0.01);
        assert!(work.learning_curve_multiplier(20) < work.learning_curve_multiplier(10));
    }

    #[test]
    fn test_resource_prices_lookup() {
        let prices = ResourcePrices::default();
        for r in Resource::ALL {
            assert!(prices.price_per_kg(*r) > 0.0);
        }
        assert_eq!(prices.price_per_kg(Resource::Electronics), 20_000.0);
    }
}
