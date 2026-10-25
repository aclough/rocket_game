//! Tunable game-balance parameters, gathered into one TOML-loadable
//! struct so the simulation harness can sweep them without recompiling.
//!
//! `BalanceConfig::default()` is the single source of truth for the
//! shipped values — TOML files are partial overrides layered on top
//! (see [`BalanceConfig::load_layered`]). Deliberately excluded:
//! complexity tables (`balance.rs`), physics constants (design
//! decisions, not knobs — 17_3_PHYSICS.md Q2), and UI mechanics. The
//! technology section is seed-entangled: changing it regenerates every
//! world's technologies, not just the odds.

use std::path::Path;

use serde::{Serialize, Deserialize};

use crate::contract::MarketArchetype;
use crate::economy::EconomicCondition;
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
    pub propellant_handling: PropellantHandlingConfig,
    pub flight: FlightConfig,
    pub nozzle: NozzleConfig,
    pub geopolitics: GeopoliticsConfig,
    pub economy: EconomyConfig,
    pub technology: TechnologyConfig,
    pub improvements: ImprovementsConfig,
    pub uprates: UpratesConfig,
    pub third_party: ThirdPartyConfig,
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
    /// Material cost of a retrofit as a fraction of the unit's build
    /// cost: parts swapped, not a vehicle rebuilt.
    pub retrofit_material_fraction: f64,
    /// One-time hiring cost for an engineering team.
    pub engineering_hiring_cost: f64,
    /// Monthly salary for a manufacturing team (~20-25 workers).
    pub manufacturing_monthly_salary: f64,
    /// One-time hiring cost for a manufacturing team.
    pub manufacturing_hiring_cost: f64,
    /// Material cost of a scale-1.0 reference reactor.
    pub reactor_ref_material_cost: f64,
    /// Price per kilogram for each manufacturing resource.
    pub resource_prices: ResourcePrices,
    /// Stage assembly hardware (harness, avionics, umbilicals) is
    /// `assembly_floor_kg + assembly_fraction × structural mass`, priced
    /// through `stage_assembly_bom`: a small avionics core is the same
    /// on any stage, the harness and plumbing grow with it
    /// (24_STRATEGIES.md §2.1). Calibrated so the bot's vehicle pays
    /// what it did under the old flat 500 kg while an Electron-class
    /// stage pays a quarter of that.
    #[serde(default = "default_assembly_floor_kg")]
    pub assembly_floor_kg: f64,
    #[serde(default = "default_assembly_fraction")]
    pub assembly_fraction: f64,
    /// Structural mass above which a stage's harness and avionics
    /// suite stops growing: a 26 t-structure core is not wired like
    /// four 6.5 t boosters. Keeps the incumbent's heavy vehicle near
    /// what it paid under the old flat masses.
    #[serde(default = "default_assembly_cap_kg")]
    pub assembly_cap_kg: f64,
    /// Rocket integration hardware (adapters, fairing hardware, final
    /// wiring) is `integration_floor_kg + integration_fraction × the
    /// vehicle's dry mass`, priced through `rocket_integration_bom`.
    #[serde(default = "default_integration_floor_kg")]
    pub integration_floor_kg: f64,
    #[serde(default = "default_integration_fraction")]
    pub integration_fraction: f64,
    /// Dry mass above which integration hardware stops growing.
    #[serde(default = "default_integration_cap_kg")]
    pub integration_cap_kg: f64,
}

fn default_assembly_floor_kg() -> f64 {
    60.0
}
fn default_assembly_fraction() -> f64 {
    0.12
}
fn default_assembly_cap_kg() -> f64 {
    7_000.0
}
fn default_integration_floor_kg() -> f64 {
    100.0
}
fn default_integration_cap_kg() -> f64 {
    12_000.0
}
fn default_integration_fraction() -> f64 {
    0.06
}

impl CostsConfig {
    /// One team-day of engineering salary, for accruing NRE onto projects.
    pub fn daily_engineering_salary(&self) -> f64 {
        self.engineering_monthly_salary / crate::calendar::DAYS_PER_MONTH_APPROX
    }

    /// One team-day of manufacturing salary, for accruing labor onto orders.
    pub fn daily_manufacturing_salary(&self) -> f64 {
        self.manufacturing_monthly_salary / crate::calendar::DAYS_PER_MONTH_APPROX
    }
}

impl Default for CostsConfig {
    fn default() -> Self {
        CostsConfig {
            // $200M until 17_3_PHYSICS.md D7: a vehicle that really lifts
            // 3 t to LEO costs more to develop and to lose than the one the
            // old ascent model allowed, and the extra $50M is the runway
            // that keeps the early flaw-discovery failures survivable
            // (200 seeds: 47/200 bankrupt at $200M, 20/200 at $250M).
            starting_money: 250_000_000.0,
            engineering_monthly_salary: 150_000.0,
            retrofit_material_fraction: 0.05,
            engineering_hiring_cost: 150_000.0,
            manufacturing_monthly_salary: 300_000.0,
            manufacturing_hiring_cost: 900_000.0,
            reactor_ref_material_cost: 30_000_000.0,
            resource_prices: ResourcePrices::default(),
            assembly_floor_kg: default_assembly_floor_kg(),
            assembly_fraction: default_assembly_fraction(),
            assembly_cap_kg: default_assembly_cap_kg(),
            integration_floor_kg: default_integration_floor_kg(),
            integration_fraction: default_integration_fraction(),
            integration_cap_kg: default_integration_cap_kg(),
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
    /// Build work multiplies by this for each complexity point above 5
    /// (21_ENGINES.md step 5): engine cost is labour, and labour is
    /// complexity. Fitted to the survey's middle band — a kerolox gas
    /// generator ~$2k per kN at the 20th unit, kerolox staged combustion
    /// ~$2.5–3k (RD-191, RD-180) — which puts staged combustion at ~2×
    /// a gas generator and full-flow ~3×. Replaced a gentle (cx/5)^1.5
    /// plus hand-set material premiums per cycle. `engine_build_base_days`
    /// (82.7) is set so a complexity-6 engine builds in the same 118
    /// line-days as under the old curve: the bot economy turns on its
    /// kerolox booster's build time, and ±20 % there swings it ±$30M.
    pub engine_build_complexity_factor: f64,
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
    /// Manufacturing work, in team-days, per flaw a retrofit changes on a
    /// built unit (22_FLAWS.md §3.5): bringing a shelved rocket or engine
    /// up to its designs' current flaws. At least one flaw's worth.
    pub retrofit_work_per_flaw: f64,
}

impl Default for WorkConfig {
    fn default() -> Self {
        WorkConfig {
            engine_design_base_days: 120.0,
            rocket_design_base_days: 60.0,
            engine_build_base_days: 82.7,
            engine_design_complexity_exponent: 2.5,
            rocket_design_complexity_exponent: 2.5,
            engine_build_complexity_factor: 1.43,
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
            retrofit_work_per_flaw: 8.0,
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

    /// Work required in days to build an engine:
    /// base_days × factor^(complexity − 5) × (mass / anchor)^mass_exponent
    /// × the propellant's handling multiplier (`PropellantHandlingConfig`).
    pub fn engine_build_work(&self, complexity: u32, engine_mass_kg: f64, handling: f64) -> f64 {
        self.engine_build_base_days
            * self.engine_build_complexity_factor.powi(complexity as i32 - 5)
            * (engine_mass_kg / self.engine_build_mass_anchor_kg)
                .powf(self.engine_build_mass_exponent)
            * handling
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
    /// Payload safety factor applied when judging whether a design can
    /// carry a contract — don't book payloads within (1 − this) of the
    /// physical maximum. Shared by the bid rule engine and `BasicPolicy`.
    pub bid_payload_margin: f64,
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
            bid_payload_margin: 0.9,
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

/// How much longer hydrogen-class hardware takes to build than kerolox
/// hardware of the same complexity and mass (21_ENGINES.md step 5). A
/// multiplier on an engine's build work, and so on its labour cost:
/// the one place cost does not come from complexity alone. The survey
/// has hydrolox boosters that were designed to cost (RS-68) or built
/// in volume (Vulcain 2) at ~$6–12k per kN of thrust against
/// ~$2–3k for kerolox (RD-191, RD-180). Hydrogen's problems factor adds
/// one complexity point (×1.43 build work, and more design work and
/// flaws); carrying the rest as complexity would multiply flaws and
/// design work far past what the history shows (the RL10 and J-2 were
/// not slow programmes), so the rest is this build multiplier.
/// 2.5 × the extra point ≈ 3.6× a kerolox engine's build work, the
/// most the bot economy absorbs with four manufacturing lines (see
/// the step 5 sweep). Everything but hydrolox is 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PropellantHandlingConfig {
    pub kerolox: f64,
    pub methalox: f64,
    /// Deep-cryogenic hydrogen: embrittlement-proof alloys, 20 K seals,
    /// long qualification per unit.
    pub hydrolox: f64,
    pub hypergolic: f64,
    pub solid: f64,
    pub xenon: f64,
    pub photon: f64,
    /// Nuclear thermal: its cost already sits in the HEU and its
    /// complexity.
    pub hydrogen: f64,
}

impl Default for PropellantHandlingConfig {
    fn default() -> Self {
        PropellantHandlingConfig {
            kerolox: 1.0,
            methalox: 1.0,
            hydrolox: 2.5,
            hypergolic: 1.0,
            solid: 1.0,
            xenon: 1.0,
            photon: 1.0,
            hydrogen: 1.0,
        }
    }
}

impl PropellantHandlingConfig {
    /// Build-work multiplier for a propellant preset.
    pub fn multiplier(&self, preset: crate::engine_project::PropellantPreset) -> f64 {
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
    /// Exponent on the uniform draw behind a flaw's activation chance:
    /// `activation = uniform^N`. 2 (mean ⅓, median ¼) skews flaws
    /// toward rarely-firing ones; higher N more so, 1 is flat. 22_FLAWS
    /// step 12 set 1.6 (mean 0.38): with retrofits keeping built units
    /// current, 2.0 left the first flight at 47 %, 1.6 puts it on the
    /// 35 % target.
    pub activation_exponent: f64,
    /// Relative weights of an engine flaw's consequence, in the order
    /// `[degradation, part loss, stage loss, pad abort, payload loss,
    /// power draw]` (22_FLAWS.md §3.3); normalised when drawn, and a
    /// consequence the pool has no words for in a domain is redrawn.
    pub engine_consequence_weights: [f64; 6],
    /// The same for a rocket's flaws. The 2026 launch survey has ground
    /// and range problems at 15 % of early losses, which is where pad
    /// aborts come from.
    pub rocket_consequence_weights: [f64; 6],
    /// The same for a reactor's flaws.
    pub reactor_consequence_weights: [f64; 6],
    /// A pad abort grounds the vehicle for a stand-down drawn uniformly
    /// from `[min, max]` days.
    pub pad_abort_delay_days: [u32; 2],
    /// A power-draw flaw adds a bus load drawn uniformly from
    /// `[min, max]` watts to the flying vehicle.
    pub power_draw_w: [f64; 2],
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
    /// Chance that each change a rocket modification makes — a stage
    /// added, removed, re-engined or given a different engine count;
    /// half for a stage merely resized — introduces a new undiscovered
    /// flaw (22_FLAWS.md §3.8). Falcon 1 flight 3's new engine on an old
    /// stage is one change.
    pub modification_flaw_prob_per_change: f64,
    /// Exponent N in the per-flaw ground-discovery roll for a `Bench`
    /// flaw: discovery = uniform^N * sqrt(activation_chance), with the
    /// uniform draw stored on the flaw (`Flaw::discovery_roll`). 1.0 is
    /// a plain uniform draw; higher N skews discovery low (mean scales
    /// as 1/(N+1)), so test campaigns converge slower and the low tail
    /// becomes a de-facto "never saw it on the stand" class. Flight
    /// activations are always discovered regardless. The M4 Task 3
    /// sweep (2026-07) chose 2.0 for what was then every flaw; 22_FLAWS
    /// step 3 kept it for the bench class and gave the others their
    /// own (§3.1 of that plan); step 7's sweep settled on 2 / 5 / 9
    /// with the flight review at 0.5.
    pub discovery_exponent_bench: f64,
    /// N for an `Integrated` flaw — feed lines, pressurisation,
    /// harnesses: things a full-up stage test or wet dress finds, which
    /// a component stand does not.
    pub discovery_exponent_integrated: f64,
    /// N for an `Analysis` flaw — staging, fairing jettison,
    /// upper-stage ignition, slosh, max-Q structure: found by review
    /// and simulation, slowly, or by flying.
    pub discovery_exponent_analysis: f64,
    /// Share of an engine's flaws drawn from each exposure class,
    /// `[bench, integrated, analysis]`; normalised when used.
    pub engine_exposure_mix: [f64; 3],
    /// The same for a rocket's flaws. The 2026 launch survey has
    /// plumbing on the integrated vehicle as the big early killer and
    /// staging / ignition / guidance second (22_FLAWS.md §1).
    pub rocket_exposure_mix: [f64; 3],
    /// The same for a reactor's flaws.
    pub reactor_exposure_mix: [f64; 3],
    /// The same for a bought-in engine's flaws: you do not own the
    /// drawings, so what you find is mostly in the integration and
    /// in the flying (22_FLAWS.md §3.7).
    pub third_party_exposure_mix: [f64; 3],
    /// How many testing cycles after a flight the flaws that flight
    /// exercised roll discovery at `discovery_exponent_flight_review`
    /// instead of their class's exponent (22_FLAWS.md §3.2). A bonus on
    /// testing the company is already paying for, not an instant
    /// reveal: a company that stops testing after a flight learns
    /// nothing from it.
    pub flight_review_cycles: u32,
    /// N while a flight review is open — below `Bench`, since flight
    /// data beats any stand.
    pub discovery_exponent_flight_review: f64,
    /// Endurance (`PerDay`) flaws join a flight review only when the
    /// flight lasted at least this many days: a LEO delivery does not
    /// review a year's wear.
    pub flight_review_min_days: u32,
}

impl FlawsConfig {
    /// The discovery exponent for a flaw of this exposure.
    pub fn discovery_exponent(&self, exposure: crate::flaw::FlawExposure) -> f64 {
        use crate::flaw::FlawExposure;
        match exposure {
            FlawExposure::Bench => self.discovery_exponent_bench,
            FlawExposure::Integrated => self.discovery_exponent_integrated,
            FlawExposure::Analysis => self.discovery_exponent_analysis,
        }
    }

    /// The consequence weights a domain's flaws are drawn from.
    pub fn consequence_weights(&self, domain: crate::flaw::FlawDomain) -> [f64; 6] {
        use crate::flaw::FlawDomain;
        match domain {
            FlawDomain::Engine(_) => self.engine_consequence_weights,
            FlawDomain::Rocket => self.rocket_consequence_weights,
            FlawDomain::Reactor => self.reactor_consequence_weights,
        }
    }

    /// The exposure mix a domain's flaws are drawn from.
    pub fn exposure_mix(&self, domain: crate::flaw::FlawDomain) -> [f64; 3] {
        use crate::flaw::FlawDomain;
        match domain {
            FlawDomain::Engine(_) => self.engine_exposure_mix,
            FlawDomain::Rocket => self.rocket_exposure_mix,
            FlawDomain::Reactor => self.reactor_exposure_mix,
        }
    }

    /// What `cycles` of bench testing are expected to find of a fresh
    /// design in this domain, weighted by activation chance (the share
    /// of the flight risk removed): the mean over the domain's exposure
    /// mix of 1 − (1 − r^N · √a)^k, with the discovery roll r uniform
    /// and the activation a = u^activation_exponent as `generate_flaws`
    /// draws them. No flight review, no hard-to-test penalty: the
    /// stand's reach on its own. The testing label is cut on this.
    pub fn expected_coverage(&self, domain: crate::flaw::FlawDomain, cycles: u32) -> f64 {
        if cycles == 0 {
            return 0.0;
        }
        let mix = self.exposure_mix(domain);
        let total: f64 = mix.iter().sum();
        if total <= 0.0 {
            return 0.0;
        }
        const GRID: usize = 64;
        let mut found = 0.0;
        let mut weight = 0.0;
        for (share, exposure) in mix.iter().zip(crate::flaw::FlawExposure::ALL) {
            if *share <= 0.0 {
                continue;
            }
            let n = self.discovery_exponent(exposure);
            for i in 0..GRID {
                let u = (i as f64 + 0.5) / GRID as f64;
                let a = u.powf(self.activation_exponent);
                for j in 0..GRID {
                    let r = (j as f64 + 0.5) / GRID as f64;
                    let p = (r.powf(n) * a.sqrt()).min(1.0);
                    let hit = 1.0 - (1.0 - p).powi(cycles as i32);
                    found += share * a * hit;
                    weight += share * a;
                }
            }
        }
        found / weight
    }
}

impl Default for FlawsConfig {
    fn default() -> Self {
        FlawsConfig {
            count_stddev: 1.5,
            activation_exponent: 1.6,
            engine_consequence_weights: [0.45, 0.30, 0.12, 0.08, 0.0, 0.05],
            rocket_consequence_weights: [0.40, 0.25, 0.15, 0.12, 0.08, 0.0],
            reactor_consequence_weights: [0.50, 0.33, 0.12, 0.0, 0.0, 0.05],
            pad_abort_delay_days: [3, 14],
            power_draw_w: [100.0, 1000.0],
            degradation_min: 0.03,
            degradation_max: 0.15,
            rocket_endurance_fraction: 0.30,
            reactor_endurance_fraction: 0.30,
            improvement_discovery_chance: 0.08,
            reactor_improvement_discovery_chance: 0.08,
            improvement_decay: 0.7,
            modification_flaw_prob_per_change: 0.10,
            discovery_exponent_bench: 2.0,
            discovery_exponent_integrated: 5.0,
            discovery_exponent_analysis: 9.0,
            engine_exposure_mix: [0.70, 0.25, 0.05],
            rocket_exposure_mix: [0.15, 0.55, 0.30],
            reactor_exposure_mix: [0.70, 0.25, 0.05],
            third_party_exposure_mix: [0.20, 0.50, 0.30],
            flight_review_cycles: 3,
            discovery_exponent_flight_review: 0.5,
            flight_review_min_days: 30,
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
    /// compensation agreed in the Task 4 plan's Q4c). 12 → 18 in
    /// 21_ENGINES.md step 5: hydrogen's build premium made its booster
    /// engine ~26 % more line-days and its upper engine ~3×; 15 lines
    /// kept its four-year build count but a won campaign's third
    /// mission flew ~day 165, past the cadence test; 18 fly it by ~122.
    /// 18 → 15 in step 8: it now buys its upper-stage engine, and 15
    /// lines give the same four-year build and award counts as before.
    pub production_lines: u32,
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
    /// engine raised measured marginal cost to ~$39M (`probes::dinosoar`);
    /// $39M → $32M in 21_ENGINES.md step 5 (materials floor plus
    /// complexity-driven labour); $32M → $48M in step 8 (its upper
    /// engine bought from the catalogue at its legacy price).
    pub catalog_cost: f64,
    /// Bid = marginal cost × margin. Margin relaxes from margin_max
    /// (one free rocket) toward margin_min as free stock grows.
    /// These are incumbent markups, not thin margins: the parody
    /// incumbent prices at what the market bears, disciplined only by
    /// its own stock pressure. Retuned in the M4 cost pass (build
    /// costs rose ~4×) so its GEO bids stay at the same real-world
    /// prices while the implied markup fell from 8-20× to 3-8×, then
    /// to 2.6-7.2 in Task 4 when the hydrolox premium raised its cost
    /// basis $36M → $39M (bid range stays ~$103-285M), and to 3.18-8.81
    /// in 21_ENGINES.md step 5 when engine cost moved from materials to
    /// complexity and lowered it $39.3M → $32.1M (same bid range), and
    /// to 2.17-6.02 in step 8 when its upper engine became a bought
    /// legacy RL10 analogue (~$16M) and its cost rose to ~$47.9M.
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
            production_lines: 15,
            starting_money: 3_000_000_000.0,
            initial_stock: 3,
            auto_build_target: 4,
            prior_builds: 40,
            catalog_cost: 48_000_000.0,
            margin_min: 2.17,
            margin_max: 6.02,
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

// ==========================================
// Flight (17_3_PHYSICS.md D6)
// ==========================================

/// The cuts that turn a delta-v shortfall into an outcome, at launch
/// and in flight, and what a partial delivery pays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlightConfig {
    /// A launch whose degraded vehicle still has at least this fraction
    /// of the required delta-v is a partial failure; below it, a failure.
    pub partial_failure_cut: f64,
    /// A leg burn that delivers less than this fraction of the leg's
    /// delta-v strands the flight.
    pub short_burn_strand_cut: f64,
    /// After a mid-flight flaw, a vehicle with less than this fraction of
    /// the remaining route's delta-v left is stranded.
    pub remaining_route_strand_cut: f64,
    /// Fraction of the contract payment a partial delivery earns.
    pub partial_payment_fraction: f64,
}

impl Default for FlightConfig {
    fn default() -> Self {
        FlightConfig {
            partial_failure_cut: 0.95,
            short_burn_strand_cut: 0.95,
            remaining_route_strand_cut: 0.5,
            partial_payment_fraction: 0.5,
        }
    }
}

/// Nozzle geometry limits and the flow-separation hazard
/// (19_NOZZLES.md). The physics itself — thrust coefficient by
/// expansion ratio and ambient pressure — is in `nozzle.rs` and is not
/// a knob; these are the design conventions and limits it is applied
/// under.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NozzleConfig {
    /// Exit pressure a sea-level bell is designed to: the Summerfield
    /// separation limit (~0.4 atm) sets how far it may expand.
    pub sea_level_exit_pressure_pa: f64,
    /// How much wider a vacuum bell's exit may be than the same
    /// engine's sea-level bell (diameter ratio): MVac is ~3.6× Merlin,
    /// RVac ~1.8× Raptor. Until stages have a diameter (the EDL and
    /// structure work) this, not a fixed size, bounds the long bell, so
    /// its expansion ratio is this squared times the short bell's.
    pub vacuum_bell_exit_ratio: f64,
    /// Any vacuum bell may reach an exit this wide (m) even when that
    /// is more than `vacuum_bell_exit_ratio` allows: a small or
    /// low-pressure engine is not held to a stubby bell (RL10B-2's
    /// ~2.1 m exit is ε 280, Vinci's ~2.2 m ε 240).
    pub min_vacuum_bell_exit_m: f64,
    /// Cap on a vacuum bell's expansion ratio; past ~250 the Isp gain
    /// has flattened to under 1 % per 50.
    pub max_expansion_ratio: f64,
    /// Mass of bell extension per square metre of added exit area
    /// (MVac ≈ 16, RL10B-2's carbon extendible ≈ 40).
    pub bell_areal_density_kg_m2: f64,
    /// Ambient-to-exit pressure ratio at which flow separation starts
    /// to destroy engines; the sea-level bell sits at 2.5 on the pad.
    pub separation_risk_start_ratio: f64,
    /// Per-engine destruction probability added per unit of pressure
    /// ratio beyond the start.
    pub separation_risk_slope: f64,
}

impl Default for NozzleConfig {
    fn default() -> Self {
        NozzleConfig {
            sea_level_exit_pressure_pa: 0.4 * 101_325.0,
            vacuum_bell_exit_ratio: 3.0,
            min_vacuum_bell_exit_m: 2.0,
            max_expansion_ratio: 250.0,
            bell_areal_density_kg_m2: 20.0,
            separation_risk_start_ratio: 3.0,
            separation_risk_slope: 0.2,
        }
    }
}

// ==========================================
// Geopolitics (17_3_PHYSICS.md D6)
// ==========================================

/// The Great Power War arc: how often it happens, how it escalates,
/// and what it does to the markets while it runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeopoliticsConfig {
    /// Annual chance a war breaks out. At 0.02, fifteen years give a 26%
    /// chance of seeing one at all — deliberately the exception.
    pub war_start_chance: f64,
    /// Annual chance an ongoing war ends. Geometric, so at 0.5 the mean
    /// war runs two years.
    pub war_end_chance: f64,
    /// Annual chance a war escalates to anti-satellite weapons. With the
    /// default end chance, 57% of wars get there.
    pub asat_chance: f64,
    /// Orbits a debris cascade makes unusable. GEO and GTO are far enough
    /// out to be left alone, which is why this is per destination.
    pub debris_orbits: Vec<String>,
    /// What survives in those orbits while the debris is being thrown.
    pub debris_volume_mult: f64,
    /// And what the replacement wave looks like afterwards.
    pub reconstitution_volume_mult: f64,
    /// Wartime demand for reconnaissance launch.
    pub war_nro_volume_mult: f64,
    /// A desperate customer pays more, which is what makes the surge
    /// worth having rather than merely busy.
    pub war_nro_rate_mult: f64,
    /// Wartime urgency lowers the bar: they need lift more than a
    /// spotless record. -20 takes the reputation target from 80 to 60.
    pub war_nro_rep_delta: f64,
}

impl Default for GeopoliticsConfig {
    fn default() -> Self {
        GeopoliticsConfig {
            war_start_chance: 0.02,
            war_end_chance: 0.50,
            asat_chance: 0.40,
            debris_orbits: vec!["leo".into(), "sso".into()],
            debris_volume_mult: 0.2,
            reconstitution_volume_mult: 2.0,
            war_nro_volume_mult: 5.0,
            war_nro_rate_mult: 1.5,
            war_nro_rep_delta: -20.0,
        }
    }
}

// ==========================================
// Economy (17_3_PHYSICS.md D6)
// ==========================================

/// One step of the economic cycle's Markov chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EconomyTransition {
    pub to: EconomicCondition,
    pub chance: f64,
}

/// How one economic condition behaves while it lasts and where it goes
/// next. Transition chances should sum to 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EconomyConditionConfig {
    /// Contract quantity and payment multiplier, drawn uniformly.
    pub modifier_min: f64,
    pub modifier_max: f64,
    /// How long the condition lasts, in months, drawn uniformly.
    pub duration_min_months: u32,
    pub duration_max_months: u32,
    pub transitions: Vec<EconomyTransition>,
}

impl Default for EconomyConditionConfig {
    fn default() -> Self {
        EconomyConfig::default().normal
    }
}

/// The economic cycle: one entry per condition, plus the opening
/// dot-com crash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EconomyConfig {
    pub boom: EconomyConditionConfig,
    pub normal: EconomyConditionConfig,
    pub slowdown: EconomyConditionConfig,
    pub recession: EconomyConditionConfig,
    pub recovery: EconomyConditionConfig,
    /// Chance the first transition of the game is the dot-com crash (a
    /// recession) regardless of the chain.
    pub dot_com_crash_chance: f64,
}

impl EconomyConfig {
    pub fn condition(&self, c: EconomicCondition) -> &EconomyConditionConfig {
        match c {
            EconomicCondition::Boom => &self.boom,
            EconomicCondition::Normal => &self.normal,
            EconomicCondition::Slowdown => &self.slowdown,
            EconomicCondition::Recession => &self.recession,
            EconomicCondition::Recovery => &self.recovery,
        }
    }
}

impl Default for EconomyConfig {
    fn default() -> Self {
        use EconomicCondition::*;
        let t = |to, chance| EconomyTransition { to, chance };
        let cond = |modifier: (f64, f64), months: (u32, u32), transitions: Vec<EconomyTransition>| {
            EconomyConditionConfig {
                modifier_min: modifier.0, modifier_max: modifier.1,
                duration_min_months: months.0, duration_max_months: months.1,
                transitions,
            }
        };
        EconomyConfig {
            boom: cond((1.3, 1.5), (6, 18), vec![t(Normal, 0.45), t(Slowdown, 0.35), t(Recession, 0.15), t(Boom, 0.05)]),
            normal: cond((1.0, 1.0), (12, 36), vec![t(Slowdown, 0.40), t(Normal, 0.25), t(Boom, 0.20), t(Recession, 0.15)]),
            slowdown: cond((0.8, 0.9), (4, 10), vec![t(Recession, 0.45), t(Normal, 0.30), t(Recovery, 0.15), t(Slowdown, 0.10)]),
            recession: cond((0.5, 0.7), (3, 8), vec![t(Recovery, 0.70), t(Slowdown, 0.20), t(Recession, 0.10)]),
            recovery: cond((0.85, 0.95), (8, 24), vec![t(Normal, 0.50), t(Boom, 0.25), t(Slowdown, 0.15), t(Recovery, 0.10)]),
            dot_com_crash_chance: 0.5,
        }
    }
}

// ==========================================
// Technology (17_3_PHYSICS.md D6)
// ==========================================

/// How many deficiencies a technology of one difficulty starts with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeficiencyCount {
    pub min: u32,
    pub max: u32,
}

/// Technology unlocks and the deficiencies a new technology carries.
/// Per-difficulty lists are indexed by difficulty and the last entry
/// covers anything harder. Generation is seed-driven, so changing these
/// changes every world's technologies, not just the odds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TechnologyConfig {
    /// Yearly chance a locked technology becomes available, by difficulty.
    pub unlock_chance_by_difficulty: Vec<f64>,
    /// Deficiencies a new technology carries, by difficulty.
    pub deficiency_count_by_difficulty: Vec<DeficiencyCount>,
    /// Solvability is drawn from `(−d·p) .. (1 − d·p)` and floored at 0,
    /// for difficulty `d` and this penalty `p`.
    pub solvability_penalty_per_difficulty: f64,
    /// A deficiency's magnitude draw `m` runs from `base + d·step` up to
    /// `base + span + d·span_step` for difficulty `d`: the low
    /// end rises gently with difficulty, the high end a lot
    /// (23_DEFICIENCIES.md §2: at the top of difficulty 2 a technology
    /// is essentially unusable until the deficiency is revised out).
    /// Each kind scales the draw by its own factor below.
    pub deficiency_magnitude_base: f64,
    pub deficiency_magnitude_per_difficulty: f64,
    pub deficiency_magnitude_span: f64,
    #[serde(default = "default_magnitude_span_per_difficulty")]
    pub deficiency_magnitude_span_per_difficulty: f64,
    /// Isp lost is `isp_scale × m`: ten per cent off Isp is a different
    /// engine, so Isp takes less of the draw than mass does.
    #[serde(default = "default_isp_scale")]
    pub isp_scale: f64,
    /// Thrust lost is `m`, but never more than this fraction.
    #[serde(default = "default_thrust_cap")]
    pub thrust_cap: f64,
    /// Mass added is `mass_scale × m`.
    #[serde(default = "default_mass_scale")]
    pub mass_scale: f64,
    /// Reactor power lost is `power_scale × m`.
    #[serde(default = "default_power_scale")]
    pub power_scale: f64,
    /// An electric thruster's `PowerDrawPenalty` multiplies its power
    /// per newton by `1 + this × m`: +10..50 % at difficulty 0, up to
    /// double at the top of difficulty 2 (23_DEFICIENCIES.md §3.3).
    #[serde(default = "default_power_draw_scale")]
    pub power_draw_scale: f64,
    /// A `MaterialCostPenalty` multiplies the bill of materials by
    /// `1 + this × m`: ×1.2–2.0 at difficulty 0, up to ×3 at the top of
    /// difficulty 2.
    #[serde(default = "default_material_cost_scale")]
    pub material_cost_scale: f64,
    /// A deficiency another project already solved is this many times
    /// as likely to fall on the next attempt…
    pub solved_elsewhere_multiplier: f64,
    /// …but never more likely than this.
    pub solve_chance_cap: f64,
    /// A `BuildTimePenalty` deficiency multiplies build work by
    /// `1 + this × magnitude`: at the default magnitudes (0.05–0.35) that
    /// is ×1.2–2.4 — room for an RL10-style process on the worst rolls
    /// without every hydrogen engine becoming one.
    #[serde(default = "default_build_time_penalty_scale")]
    pub build_time_penalty_scale: f64,
}

fn default_build_time_penalty_scale() -> f64 {
    4.0
}
fn default_magnitude_span_per_difficulty() -> f64 {
    0.25
}
fn default_isp_scale() -> f64 {
    0.4
}
fn default_thrust_cap() -> f64 {
    0.8
}
fn default_mass_scale() -> f64 {
    1.5
}
fn default_power_scale() -> f64 {
    0.7
}
fn default_power_draw_scale() -> f64 {
    1.0
}
fn default_material_cost_scale() -> f64 {
    2.0
}

impl TechnologyConfig {
    fn by_difficulty<T>(list: &[T], difficulty: u32) -> &T {
        &list[(difficulty as usize).min(list.len() - 1)]
    }
    pub fn unlock_chance(&self, difficulty: u32) -> f64 {
        *Self::by_difficulty(&self.unlock_chance_by_difficulty, difficulty)
    }
    pub fn deficiency_count(&self, difficulty: u32) -> &DeficiencyCount {
        Self::by_difficulty(&self.deficiency_count_by_difficulty, difficulty)
    }
}

impl Default for TechnologyConfig {
    fn default() -> Self {
        TechnologyConfig {
            unlock_chance_by_difficulty: vec![0.0, 0.10, 0.08],
            deficiency_count_by_difficulty: vec![
                DeficiencyCount { min: 0, max: 2 },
                DeficiencyCount { min: 1, max: 3 },
                DeficiencyCount { min: 2, max: 4 },
            ],
            solvability_penalty_per_difficulty: 0.1,
            deficiency_magnitude_base: 0.10,
            deficiency_magnitude_per_difficulty: 0.05,
            deficiency_magnitude_span: 0.40,
            deficiency_magnitude_span_per_difficulty: default_magnitude_span_per_difficulty(),
            isp_scale: default_isp_scale(),
            thrust_cap: default_thrust_cap(),
            mass_scale: default_mass_scale(),
            power_scale: default_power_scale(),
            power_draw_scale: default_power_draw_scale(),
            material_cost_scale: default_material_cost_scale(),
            solved_elsewhere_multiplier: 3.0,
            solve_chance_cap: 0.95,
            build_time_penalty_scale: default_build_time_penalty_scale(),
        }
    }
}

impl TechnologyConfig {
    /// The magnitude draw's range for a difficulty: `(low, high)`.
    pub fn magnitude_range(&self, difficulty: u32) -> (f64, f64) {
        let d = difficulty as f64;
        let low = self.deficiency_magnitude_base + d * self.deficiency_magnitude_per_difficulty;
        let high = self.deficiency_magnitude_base + self.deficiency_magnitude_span
            + d * self.deficiency_magnitude_span_per_difficulty;
        (low, high.max(low + 0.01))
    }
}

// ==========================================
// Improvements (17_3_PHYSICS.md D6)
// ==========================================

/// One kind of improvement a testing cycle can discover: how likely it
/// is relative to the others, and how big it is (a fraction, drawn
/// uniformly from `min..max`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImprovementRoll {
    pub weight: f64,
    pub min: f64,
    pub max: f64,
}

/// How the third-party catalogue is priced (21_ENGINES.md step 8): what
/// the engine costs its vendor to build — the same cost model as the
/// player's engines, legacy build-time penalty included — at the
/// vendor's production volume, times the vendor's markup (development
/// recovered, support, margin).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThirdPartyConfig {
    /// Units the vendor has built: where its learning curve sits.
    pub vendor_prior_builds: u32,
    /// Price over the vendor's unit cost. 3.0 puts the NK-33 analogue at
    /// ~$7.6M, between the NK-33 surplus sale ($1.1M) and AJ26 (~$10M),
    /// and the RL10 analogue near the RL10C-3 contract (≤ $14.5M).
    pub markup: f64,
}

impl Default for ThirdPartyConfig {
    fn default() -> Self {
        ThirdPartyConfig { vendor_prior_builds: 20, markup: 3.0 }
    }
}

/// Tier-2 improvements: uprates (21_ENGINES.md step 7). Larger than an
/// improvement, and each one risks new hidden flaws — the player sees
/// the risk and chooses whether to accept it. They become discoverable
/// only once an engine has both a testing record and flight heritage
/// (question 5), then roll per testing cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpratesConfig {
    /// Testing cycles an engine must have run (ten: "Well Tested" now
    /// that labels are cut on expected coverage).
    pub min_testing_cycles: u32,
    /// Flights an engine must have flown on the company's rockets.
    pub min_flights: u32,
    /// Chance per testing cycle, once both are met, while no uprate is
    /// already on offer.
    pub chance_per_cycle: f64,
    /// Raise chamber pressure on the existing hardware: thrust up by the
    /// fraction at the same mass, pad Isp up through the nozzle model
    /// (RD-253 → RD-275 → RD-276, Merlin 1C → 1D, RS-68 → 68A).
    pub chamber_pressure: UprateRoll,
    /// Lighter turbomachinery: mass down by the fraction (Merlin 1D,
    /// Raptor 2 → 3, NK-33's single shaft).
    pub turbomachinery: UprateRoll,
    /// Better injection and combustion: Isp up by the fraction.
    pub injector: UprateRoll,
    /// A production-process change (fewer welds, printed parts): build
    /// work divided by the drawn factor (Vulcain 2.1's nozzle, RS-25E).
    pub production: UprateRoll,
}

/// One uprate kind: how often it comes up relative to the others, the
/// range its size is drawn from, and its risk — `flaw_rolls` chances of
/// `flaw_chance` each to add a new hidden flaw when it is actualised.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UprateRoll {
    pub weight: f64,
    pub min: f64,
    pub max: f64,
    pub flaw_rolls: u32,
    pub flaw_chance: f64,
}

impl Default for UpratesConfig {
    fn default() -> Self {
        let r = |weight, min, max, flaw_rolls, flaw_chance| UprateRoll { weight, min, max, flaw_rolls, flaw_chance };
        UpratesConfig {
            min_testing_cycles: 10,
            min_flights: 5,
            chance_per_cycle: 0.03,
            chamber_pressure: r(0.25, 0.15, 0.30, 3, 0.25),
            turbomachinery: r(0.25, 0.10, 0.20, 2, 0.25),
            injector: r(0.25, 0.03, 0.06, 2, 0.20),
            production: r(0.25, 1.3, 2.0, 1, 0.25),
        }
    }
}

/// The three improvement kinds an engine family can discover.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineImprovementsConfig {
    pub isp: ImprovementRoll,
    pub mass: ImprovementRoll,
    pub thrust: ImprovementRoll,
}

/// What testing discovers, by engine family and for reactors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImprovementsConfig {
    pub chemical: EngineImprovementsConfig,
    pub electric: EngineImprovementsConfig,
    pub nuclear_thermal: EngineImprovementsConfig,
    pub solar_sail: EngineImprovementsConfig,
    pub reactor_power: ImprovementRoll,
    pub reactor_mass: ImprovementRoll,
}

impl Default for ImprovementsConfig {
    fn default() -> Self {
        let r = |weight, min, max| ImprovementRoll { weight, min, max };
        let family = |isp, mass, thrust| EngineImprovementsConfig { isp, mass, thrust };
        ImprovementsConfig {
            chemical: family(r(0.40, 0.01, 0.04), r(0.30, 0.02, 0.06), r(0.30, 0.01, 0.04)),
            electric: family(r(0.40, 0.01, 0.04), r(0.30, 0.02, 0.06), r(0.30, 0.01, 0.04)),
            nuclear_thermal: family(r(0.40, 0.01, 0.04), r(0.30, 0.02, 0.06), r(0.30, 0.01, 0.04)),
            solar_sail: family(r(0.0, 0.0, 0.0), r(0.50, 0.02, 0.06), r(0.50, 0.02, 0.05)),
            reactor_power: r(0.55, 0.01, 0.04),
            reactor_mass: r(0.45, 0.02, 0.06),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The testing label's yardstick (TODO "how tested is Thoroughly
    /// Tested"): the stand finds about half an engine's flight risk in
    /// ten cycles and a third of a rocket's, and flattens from there —
    /// flight finds the rest.
    #[test]
    fn expected_coverage_matches_the_probe() {
        use crate::flaw::FlawDomain;
        let cfg = FlawsConfig::default();
        let engine = |k| cfg.expected_coverage(FlawDomain::Engine(None), k);
        let rocket = |k| cfg.expected_coverage(FlawDomain::Rocket, k);
        assert_eq!(engine(0), 0.0);
        assert!((engine(10) - 0.58).abs() < 0.04, "engine 10 cycles {}", engine(10));
        assert!((rocket(10) - 0.37).abs() < 0.04, "rocket 10 cycles {}", rocket(10));
        assert!(engine(30) < 0.75 && rocket(30) < 0.52, "the stand saturates: {} {}", engine(30), rocket(30));
        for k in 1..30 {
            assert!(engine(k + 1) > engine(k) && rocket(k + 1) > rocket(k));
        }
    }

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
        // 82.7 × 1.43 ≈ 118 line-days for complexity 6 at the 1150 kg
        // anchor — the old curve's figure, kept; each further point
        // multiplies by 1.43; handling multiplies through.
        let cx6 = 82.7 * 1.43;
        assert!((work.engine_build_work(6, 1150.0, 1.0) - cx6).abs() < 1e-9);
        assert!((cx6 - 118.3).abs() < 0.1, "complexity 6 keeps its old build time");
        assert!((work.engine_build_work(8, 1150.0, 1.0) - 82.7 * 1.43_f64.powi(3)).abs() < 1e-9);
        assert!((work.engine_build_work(6, 1150.0, 2.5) - cx6 * 2.5).abs() < 1e-9);
        // Mass term: a 2x-mass engine is 2^0.6 ≈ 1.52x the line-days.
        assert!((work.engine_build_work(6, 2300.0, 1.0) - cx6 * 2.0_f64.powf(0.6)).abs() < 1e-9);
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
