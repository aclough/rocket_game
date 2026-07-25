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
    pub competitor: CompetitorConfig,
    pub engine_materials: EngineMaterialsConfig,
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
        config.competitor.validate()?;
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
        // Aerospace-grade prices: raw commodity cost plus the machining,
        // inspection, and traceability that flight hardware carries.
        // Retuned ×4 in the M4 cost pass so marginal vehicle cost lands
        // near 40-60% of a winning bid (payments stay at real-world
        // launch prices).
        ResourcePrices {
            aluminium: 20.0,
            steel: 12.0,
            superalloys: 320.0,
            composites: 200.0,
            wiring: 600.0,
            electronics: 80_000.0,
            plumbing: 6_000.0,
            solid_propellant: 60.0,
            heu: 400_000.0,
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
    /// Exponent on (complexity / 5) for engine (and reactor) design
    /// work. 1.0 is linear; above 1.0 a complexity-5 engine is
    /// unchanged while high-complexity cycles stretch superlinearly
    /// (real staged-combustion programs ran 8-9 years). The M4 Task 3
    /// sweep (2026-07) chose 2.5: the BasicPolicy first launch moves
    /// from month ~13 to ~18, a GG kerolox booster (complexity 6)
    /// costs 1.6x its old design work, and a staged-combustion
    /// hydrolox monster (effective 9) costs 4.3x — a multi-year
    /// program, which is where the real 8-9-year outliers live.
    pub engine_design_complexity_exponent: f64,
    /// Exponent on (complexity / 5) for rocket design work.
    pub rocket_design_complexity_exponent: f64,
    /// Exponent on (complexity / 5) for engine build work. Kept
    /// gentler than the design exponent (1.5 vs 2.5): dev-time realism
    /// lives in design, while a steep build exponent mostly starves
    /// DinoSoar's production line (its complexity-12 booster engine
    /// would build 3.7x slower at 2.5, killing its bid readiness and
    /// campaign cadence; at 1.5 it is 1.55x, which the line absorbs).
    pub engine_build_complexity_exponent: f64,
    /// Exponent on (engine mass / anchor) for engine build work — M4
    /// Task 4c. Removes the free amortization where flat per-engine
    /// labor made 4x-scale engines the cheapest per kN: a big engine
    /// now takes more line-days to build, a small one fewer.
    pub engine_build_mass_exponent: f64,
    /// Mass anchor (kg) for the engine build mass factor — the starter
    /// kerolox gas-generator's mass, so the early-game pace is
    /// unchanged by the exponent.
    pub engine_build_mass_anchor_kg: f64,
    /// Exponent on engine scale for design work — M4 Task 4c. A
    /// 4x-scale engine is a genuinely bigger dev program (the F-1
    /// story: size was itself the problem), ~1.7x the work at the
    /// default 0.4; a 0.25x engine is ~0.6x.
    pub engine_design_scale_exponent: f64,
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
            engine_design_complexity_exponent: 2.5,
            rocket_design_complexity_exponent: 2.5,
            engine_build_complexity_exponent: 1.5,
            engine_build_mass_exponent: 0.6,
            engine_build_mass_anchor_kg: 1150.0,
            engine_design_scale_exponent: 0.4,
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
    /// base_days * (complexity / 5)^exponent * scale^scale_exponent.
    /// Reactors (no scale knob) pass scale 1.0.
    pub fn design_work_required(&self, complexity: u32, scale: f64) -> f64 {
        self.engine_design_base_days
            * (complexity as f64 / 5.0).powf(self.engine_design_complexity_exponent)
            * scale.powf(self.engine_design_scale_exponent)
    }

    /// Work required in days for rocket design:
    /// base_days * (complexity / 5)^exponent, shorter base than engines.
    pub fn rocket_design_work_required(&self, complexity: u32) -> f64 {
        self.rocket_design_base_days
            * (complexity as f64 / 5.0).powf(self.rocket_design_complexity_exponent)
    }

    /// Work required in days for engine manufacturing:
    /// base_days * (complexity / 5)^cx_exponent * (mass / anchor)^mass_exponent.
    pub fn engine_build_work(&self, complexity: u32, engine_mass_kg: f64) -> f64 {
        self.engine_build_base_days
            * (complexity as f64 / 5.0).powf(self.engine_build_complexity_exponent)
            * (engine_mass_kg / self.engine_build_mass_anchor_kg)
                .powf(self.engine_build_mass_exponent)
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
    /// Days from a solicitation's issue to its bid deadline (global —
    /// deliberately not a per-market character axis).
    pub bid_window_days: u32,
    /// Width of the "near target" band in the logistic reputation
    /// factor used by award scoring (see `contract::rep_factor`).
    pub rep_scale: f64,
    /// Campaign clause: extra reputation hit for missing a won
    /// program's mission, as a multiplier on the normal expiry hit
    /// (2.0 = the miss costs the normal hit plus twice it again).
    #[serde(default = "default_campaign_miss_rep_penalty")]
    pub campaign_miss_rep_penalty: f64,
    /// Campaign clause: missed missions before the customer cancels
    /// the remainder of the program.
    #[serde(default = "default_campaign_max_misses")]
    pub campaign_max_misses: u32,
    /// Campaign clause: the one-time reputation hit when a program is
    /// cancelled, as a multiplier on the normal expiry hit.
    #[serde(default = "default_campaign_cancel_rep_penalty")]
    pub campaign_cancel_rep_penalty: f64,
    /// Market templates + perturbation specs, realized per seed at
    /// game start (see [`crate::contract::MarketArchetype`]).
    pub archetypes: Vec<MarketArchetype>,
}

fn default_campaign_miss_rep_penalty() -> f64 { 2.0 }
fn default_campaign_max_misses() -> u32 { 2 }
fn default_campaign_cancel_rep_penalty() -> f64 { 4.0 }

impl Default for MarketsConfig {
    fn default() -> Self {
        MarketsConfig {
            deadline_min_days: 60,
            deadline_max_days: 180,
            payment_variance_min: 0.8,
            payment_variance_max: 1.2,
            bid_window_days: 30,
            rep_scale: 10.0,
            campaign_miss_rep_penalty: default_campaign_miss_rep_penalty(),
            campaign_max_misses: default_campaign_max_misses(),
            campaign_cancel_rep_penalty: default_campaign_cancel_rep_penalty(),
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
        if self.bid_window_days < 1 {
            return Err("bid_window_days must be >= 1".into());
        }
        if self.rep_scale <= 0.0 {
            return Err(format!("rep_scale {} must be positive", self.rep_scale));
        }
        if self.campaign_miss_rep_penalty < 0.0 || self.campaign_cancel_rep_penalty < 0.0 {
            return Err("campaign miss/cancel rep penalties must be >= 0".into());
        }
        if self.campaign_max_misses < 1 {
            return Err("campaign_max_misses must be >= 1".into());
        }
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
            let growth = a.annual_growth_range;
            if growth.0 > growth.1 || growth.0 <= -1.0 {
                return Err(format!(
                    "archetype `{}`: annual_growth_range ({}, {}) must be ordered and > -1.0",
                    a.key, growth.0, growth.1,
                ));
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
            if let Some((lo, hi)) = a.template.deadline_days {
                if lo < 1 || lo > hi {
                    return Err(format!(
                        "archetype `{}`: deadline_days ({lo}, {hi}) must be ordered and >= 1",
                        a.key,
                    ));
                }
            }
            if a.template.failure_severity < 0.0 {
                return Err(format!(
                    "archetype `{}`: failure_severity {} must be >= 0",
                    a.key, a.template.failure_severity,
                ));
            }
            if a.template.w_cost < 0.0 || a.template.w_rep < 0.0
                || a.template.w_cost + a.template.w_rep <= 0.0
            {
                return Err(format!(
                    "archetype `{}`: award weights (w_cost {}, w_rep {}) must be \
                     non-negative and not both zero",
                    a.key, a.template.w_cost, a.template.w_rep,
                ));
            }
            if a.template.budget_tolerance < 1.0 {
                return Err(format!(
                    "archetype `{}`: budget_tolerance {} must be >= 1.0 — a \
                     reference-priced bid must always fit the customer's budget",
                    a.key, a.template.budget_tolerance,
                ));
            }
            if let Some(c) = &a.campaign {
                if !(0.0..=1.0).contains(&c.spawn_chance_per_month) {
                    return Err(format!(
                        "archetype `{}`: campaign spawn_chance_per_month {} outside [0, 1]",
                        a.key, c.spawn_chance_per_month,
                    ));
                }
                if c.mission_count_range.0 < 1 || c.mission_count_range.0 > c.mission_count_range.1 {
                    return Err(format!(
                        "archetype `{}`: campaign mission_count_range ({}, {}) must be ordered and >= 1",
                        a.key, c.mission_count_range.0, c.mission_count_range.1,
                    ));
                }
                if c.interval_days_range.0 < 1 || c.interval_days_range.0 > c.interval_days_range.1 {
                    return Err(format!(
                        "archetype `{}`: campaign interval_days_range ({}, {}) must be ordered and >= 1",
                        a.key, c.interval_days_range.0, c.interval_days_range.1,
                    ));
                }
                if !(0.0..1.0).contains(&c.discount_range.0)
                    || !(0.0..1.0).contains(&c.discount_range.1)
                    || c.discount_range.0 > c.discount_range.1
                {
                    return Err(format!(
                        "archetype `{}`: campaign discount_range ({}, {}) must be ordered within [0, 1)",
                        a.key, c.discount_range.0, c.discount_range.1,
                    ));
                }
                if c.program_names.is_empty() {
                    return Err(format!(
                        "archetype `{}`: campaign program_names must not be empty",
                        a.key,
                    ));
                }
                if c.bid_window_days < 1 {
                    return Err(format!(
                        "archetype `{}`: campaign bid_window_days must be >= 1",
                        a.key,
                    ));
                }
            }
            match a.template.cadence {
                crate::contract::Cadence::Steady => {}
                crate::contract::Cadence::Lumpy { quiet_chance } => {
                    if !(0.0..1.0).contains(&quiet_chance) {
                        return Err(format!(
                            "archetype `{}`: Lumpy quiet_chance {} outside [0, 1)",
                            a.key, quiet_chance,
                        ));
                    }
                }
                crate::contract::Cadence::Burst { burst_chance } => {
                    if !(burst_chance > 0.0 && burst_chance <= 1.0) {
                        return Err(format!(
                            "archetype `{}`: Burst burst_chance {} outside (0, 1]",
                            a.key, burst_chance,
                        ));
                    }
                }
            }
            // Additive-only rule for the reputation-0 opening floor.
            if a.template.rep_target <= 0.0 && a.emergence.is_none() {
                if a.presence_probability < 1.0 {
                    return Err(format!(
                        "archetype `{}`: opening-floor market (rep_target <= 0, \
                         start-active) must have presence_probability 1.0",
                        a.key,
                    ));
                }
                if a.volume_mult_range.0 < 1.0 || a.rate_mult_range.0 < 1.0 {
                    return Err(format!(
                        "archetype `{}`: opening-floor market (rep_target <= 0, \
                         start-active) must have multiplier floors >= 1.0 \
                         (additive-only year-1 variance)",
                        a.key,
                    ));
                }
                if a.annual_growth_range.0 < 0.0 {
                    return Err(format!(
                        "archetype `{}`: opening-floor market (rep_target <= 0, \
                         start-active) must have annual_growth_range floor >= 0 \
                         (the floor may only rise)",
                        a.key,
                    ));
                }
                if a.template.cadence != crate::contract::Cadence::Steady {
                    return Err(format!(
                        "archetype `{}`: opening-floor market (rep_target <= 0, \
                         start-active) must have Steady cadence — lumpy/burst \
                         variance can starve a seed's first year even at \
                         conserved volume",
                        a.key,
                    ));
                }
            }
        }
        Ok(())
    }
}

// ==========================================
// Engine material premiums
// ==========================================

/// Multipliers on engine BOM material cost by propellant preset and by
/// cycle — M4 Task 4a/4b. The BOM fractions alone can't express the
/// real cost dividers (superalloys are only $320/kg, so fraction shifts
/// cap out around 1.7x): hydrogen hardware really costs ~20x kerolox
/// per engine (RL10 vs Merlin), and top cycles carry a precision
/// premium. The two multipliers stack:
/// materials = BOM $/kg x mass x preset_mult x cycle_mult.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineMaterialsConfig {
    /// Kerolox — the anchor, calibrated in the M4 Task 2 pass.
    pub kerolox: f64,
    /// Methalox hardware is kerolox-like (Raptor's cheapness is
    /// production rate, which the learning curve models).
    pub methalox: f64,
    /// Deep-cryo hydrogen hardware premium. Benchmarked against
    /// RS-68 — a hydrolox booster *designed to cost* runs ~$6.4k/kN,
    /// ~5x a Merlin — not against RL10, whose famous price is mostly
    /// a hand-brazed 1960s process at ~1 engine/month (the learning
    /// curve's territory, not the BOM's). At 3.0 a hydrolox GG lands
    /// ~3x kerolox per kN and an expander upper ~6x.
    pub hydrolox: f64,
    /// Storable hypergolics: simple, room-temperature hardware.
    pub hypergolic: f64,
    /// Solid motors: cheap cases, propellant already in the BOM.
    pub solid: f64,
    /// Electric propulsion BOM is already electronics-heavy.
    pub xenon: f64,
    /// Solar sail BOM is already film/structure.
    pub photon: f64,
    /// Nuclear-thermal BOM already carries the HEU premium.
    pub hydrogen: f64,
    /// Pressure-fed: the simplest possible plumbing.
    pub pressure_fed: f64,
    /// Gas generator — the cycle anchor.
    pub gas_generator: f64,
    /// Expander: brazed regen nozzles, tight tolerances.
    pub expander: f64,
    /// Staged combustion: high-pressure preburner hardware.
    pub staged_combustion: f64,
    /// Full-flow staged combustion: two preburners, hottest turbines.
    pub full_flow: f64,
    /// Non-chemical cycles: premiums live in their preset/BOM instead.
    pub nuclear_thermal: f64,
    pub electric_propulsion: f64,
    pub solar_sail: f64,
}

impl Default for EngineMaterialsConfig {
    fn default() -> Self {
        EngineMaterialsConfig {
            kerolox: 1.0,
            methalox: 1.0,
            hydrolox: 3.0,
            hypergolic: 1.0,
            solid: 1.0,
            xenon: 1.0,
            photon: 1.0,
            hydrogen: 1.0,
            pressure_fed: 0.8,
            gas_generator: 1.0,
            expander: 1.3,
            staged_combustion: 1.6,
            full_flow: 2.0,
            nuclear_thermal: 1.0,
            electric_propulsion: 1.0,
            solar_sail: 1.0,
        }
    }
}

impl EngineMaterialsConfig {
    /// Material multiplier for a propellant preset.
    pub fn preset_multiplier(&self, preset: crate::engine_project::PropellantPreset) -> f64 {
        use crate::engine_project::PropellantPreset as P;
        match preset {
            P::Kerolox => self.kerolox,
            P::Methalox => self.methalox,
            P::Hydrolox => self.hydrolox,
            P::Hypergolic => self.hypergolic,
            P::Solid => self.solid,
            P::Xenon => self.xenon,
            P::Photon => self.photon,
            P::Hydrogen => self.hydrogen,
        }
    }

    /// Material multiplier for an engine cycle.
    pub fn cycle_multiplier(&self, cycle: crate::engine::EngineCycle) -> f64 {
        use crate::engine::EngineCycle as C;
        match cycle {
            C::PressureFed => self.pressure_fed,
            C::GasGenerator => self.gas_generator,
            C::Expander => self.expander,
            C::StagedCombustion => self.staged_combustion,
            C::FullFlow => self.full_flow,
            C::NuclearThermal => self.nuclear_thermal,
            C::ElectricPropulsion => self.electric_propulsion,
            C::SolarSail => self.solar_sail,
        }
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
    /// Per-improvement decay on the discovery chance — M4 Task 4e:
    /// effective chance = base x decay^(improvements already found on
    /// that design). The first few +thrust/+isp tweaks come easily,
    /// then the design's low-hanging fruit runs out (the TODO "engine
    /// improvements get harder quickly the more there are"). Applies
    /// to engines and reactors alike.
    pub improvement_decay: f64,
    /// Flat probability that a rocket modification introduces a new
    /// undiscovered flaw.
    pub modification_flaw_prob: f64,
    /// Exponent N in the per-flaw ground-discovery roll:
    /// discovery_probability = uniform^N * sqrt(activation_chance).
    /// 1.0 is the original uniform draw; higher N skews discovery
    /// probabilities low (mean scales as 1/(N+1)), so test campaigns
    /// converge slower and the low tail becomes a de-facto
    /// "never saw it on the stand" class. Flight activations are
    /// always discovered regardless. The M4 Task 3 sweep (2026-07)
    /// chose 2.0: mean discovery drops to 1/3 of the sqrt cap (from
    /// 1/2), leaving ~8 undiscovered flaws at the BasicPolicy's first
    /// launch (was ~7) and putting 200-seed bankruptcies in the
    /// 2-4/100 roguelike band together with the work exponents.
    pub flaw_discovery_exponent: f64,
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
            improvement_decay: 0.7,
            modification_flaw_prob: 0.10,
            flaw_discovery_exponent: 2.0,
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

// ==========================================
// Competitor (M3: DinoSoar)
// ==========================================

/// One destination the competitor's catalog vehicle can serve, and
/// the heaviest payload it will take there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DestinationCapability {
    pub location_id: String,
    pub max_payload_kg: f64,
}

/// Script parameters for the scripted competitor (DinoSoar). The
/// company itself is a real `Company`; these knobs shape its size,
/// pricing rule, and seeded reliability. `production_lines` is the
/// intended sweep knob for difficulty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompetitorConfig {
    /// Master switch; off = no competitor is realized.
    pub enabled: bool,
    pub name: String,
    /// Manufacturing teams hired at realization — the capacity knob.
    /// 8 → 12 in M4 Task 4: the engine-build mass exponent makes the
    /// 6.6 t booster engine ~2.9x the line-days, and 12 is the minimum
    /// that keeps a won campaign's 3-mission cadence on schedule (the
    /// compensation agreed in the Task 4 plan's Q4c).
    pub production_lines: u32,
    /// Floor space units at realization (must comfortably fit
    /// simultaneous stage + integration orders for the catalog vehicle).
    pub floor_space: u32,
    pub starting_money: f64,
    /// Integrated rockets on the shelf at game start.
    pub initial_stock: u32,
    /// Auto-build inventory target (real `auto_build_targets` entry).
    pub auto_build_target: u32,
    /// Prior builds credited at realization: a mature incumbent starts
    /// well down the learning curve.
    pub prior_builds: u32,
    /// Marginal-cost estimate used for pricing until the real
    /// manufacturing pipeline has produced cost history, and the book
    /// value of initial stock. Keep near the real build cost (the
    /// pricing basis switches to actual cost history after the first
    /// build; a large gap makes prices jump). $36M → $39M in M4
    /// Task 4: the hydrolox material premium on its 6.6 t booster
    /// engine raised measured marginal cost to ~$39M (dino_probe).
    pub catalog_cost: f64,
    /// Bid = marginal cost × margin. Margin relaxes from margin_max
    /// (one free rocket) toward margin_min as free stock grows.
    /// These are incumbent markups, not thin margins: the parody
    /// incumbent prices at what the market bears, disciplined only by
    /// its own stock pressure. Retuned in the M4 cost pass (build
    /// costs rose ~4×) so its GEO bids stay at the same real-world
    /// prices while the implied markup fell from 8-20× to 3-8×, then
    /// to 2.6-7.2 in Task 4 when the hydrolox premium raised its cost
    /// basis $36M → $39M (bid range stays ~$103-285M).
    pub margin_min: f64,
    pub margin_max: f64,
    /// Absolute lowest bid the script will ever place — the safety
    /// knob that keeps it out of small-contract price wars.
    pub bid_floor: f64,
    /// Symmetric per-contract price noise (0.05 = ±5%), seeded per
    /// contract from the world seed.
    pub bid_jitter: f64,
    /// Discount on the margin (not the bid) for campaign block bids
    /// (0.10 = 10% keener): an incumbent prices guaranteed volume
    /// slightly below its one-off rate. The bid floor still applies.
    #[serde(default = "default_block_discount")]
    pub block_discount: f64,
    /// Days between an award and the scripted launch (clamped to the
    /// contract deadline).
    pub launch_lead_days: u32,
    /// Per-flight loss-of-vehicle chance = failure_base +
    /// failure_spread × u^failure_skew, u uniform in [0,1) from the
    /// world seed. High skew keeps most worlds near failure_base
    /// (~99%+ reliable) and makes a ~95% DinoSoar rare.
    pub failure_base: f64,
    pub failure_spread: f64,
    pub failure_skew: f64,
    /// Destinations served and per-destination payload limits.
    pub capability: Vec<DestinationCapability>,
}

fn default_block_discount() -> f64 {
    0.10
}

impl Default for CompetitorConfig {
    fn default() -> Self {
        let cap = |location_id: &str, max_payload_kg: f64| DestinationCapability {
            location_id: location_id.into(), max_payload_kg,
        };
        CompetitorConfig {
            enabled: true,
            name: "DinoSoar".into(),
            production_lines: 12,
            floor_space: 40,
            starting_money: 3_000_000_000.0,
            initial_stock: 3,
            auto_build_target: 4,
            prior_builds: 40,
            catalog_cost: 39_000_000.0,
            margin_min: 2.6,
            margin_max: 7.2,
            bid_floor: 60_000_000.0,
            bid_jitter: 0.05,
            block_discount: 0.10,
            launch_lead_days: 30,
            failure_base: 0.003,
            failure_spread: 0.047,
            failure_skew: 8.0,
            capability: vec![
                cap("leo", 26_000.0),
                cap("sso", 20_000.0),
                cap("gto", 13_500.0),
                cap("geo", 6_600.0),
                cap("meo", 10_000.0),
                cap("l1", 9_000.0),
                cap("l2", 9_000.0),
                cap("lunar_orbit", 9_000.0),
            ],
        }
    }
}

impl CompetitorConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        if self.production_lines == 0 {
            return Err("competitor.production_lines must be >= 1 when enabled".into());
        }
        if self.margin_min < 1.0 || self.margin_max < self.margin_min {
            return Err(format!(
                "competitor margins must satisfy 1.0 <= margin_min <= margin_max \
                 (got {} / {})", self.margin_min, self.margin_max,
            ));
        }
        if self.catalog_cost <= 0.0 || self.bid_floor < 0.0 {
            return Err("competitor.catalog_cost must be > 0 and bid_floor >= 0".into());
        }
        if !(0.0..0.5).contains(&self.bid_jitter) {
            return Err(format!("competitor.bid_jitter {} outside [0, 0.5)", self.bid_jitter));
        }
        if !(0.0..1.0).contains(&self.block_discount) {
            return Err(format!(
                "competitor.block_discount {} outside [0, 1)", self.block_discount,
            ));
        }
        let max_fail = self.failure_base + self.failure_spread;
        if self.failure_base < 0.0 || self.failure_spread < 0.0 || max_fail > 1.0 {
            return Err(format!(
                "competitor failure rate range [{}, {}] must sit inside [0, 1]",
                self.failure_base, max_fail,
            ));
        }
        if self.failure_skew < 1.0 {
            return Err("competitor.failure_skew must be >= 1.0 (higher = rarer bad seeds)".into());
        }
        if self.capability.is_empty() {
            return Err("competitor.capability must list at least one destination when enabled".into());
        }
        Ok(())
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
        // Complexity 5 at scale 1 is the anchor: exponents leave it unchanged.
        assert!((work.design_work_required(5, 1.0) - 120.0).abs() < 0.01);
        // 120 * (9/5)^2.5 ≈ 521.6 — superlinear stretch at the top end.
        assert!((work.design_work_required(9, 1.0) - 521.63).abs() < 0.01);
        // Scale term: 4x engine ≈ 1.74x the dev work (4^0.4).
        assert!((work.design_work_required(5, 4.0) - 120.0 * 4.0_f64.powf(0.4)).abs() < 0.01);
        assert!((work.rocket_design_work_required(5) - 60.0).abs() < 0.01);
        // 90 * (6/5)^1.5 ≈ 118.3 at the 1150 kg mass anchor — build
        // scales gentler than design.
        assert!((work.engine_build_work(6, 1150.0) - 118.31).abs() < 0.01);
        // Mass term: a 2x-mass engine is 2^0.6 ≈ 1.52x the line-days.
        assert!((work.engine_build_work(6, 2300.0) - 118.31 * 2.0_f64.powf(0.6)).abs() < 0.1);
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
        assert_eq!(prices.price_per_kg(Resource::Electronics), 80_000.0);
    }
}
