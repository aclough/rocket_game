use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::balance_config::FlawsConfig;

/// Unique identifier for a flaw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlawId(pub u64);

/// When a flaw can trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Default)]
pub enum FlawTrigger {
    /// Rolls once when the stage fires (existing behavior).
    #[default]
    PerFlight,
    /// Rolls every day in flight (endurance flaw).
    PerDay,
}


impl FlawTrigger {
    /// Reference mission duration in days for converting activation_chance to daily rate.
    const REFERENCE_DAYS: f64 = 365.0;
}

/// What happens when a flaw activates during flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlawConsequence {
    /// Fraction of thrust/isp lost (e.g. 0.05 = 5% loss).
    PerformanceDegradation(f64),
    /// The affected engine fails.
    EngineLoss,
    /// The entire stage fails.
    StageLoss,
}

impl FlawConsequence {
    /// Compact wording for engine and rocket flaw lists.
    pub fn short_label(&self) -> String {
        match self {
            FlawConsequence::PerformanceDegradation(frac) =>
                format!("{:.0}% perf loss", frac * 100.0),
            FlawConsequence::EngineLoss => "engine loss".to_string(),
            FlawConsequence::StageLoss => "stage loss".to_string(),
        }
    }

    /// Compact wording for reactor flaw lists, where "engine loss" means
    /// the reactor shuts down and degradation is lost power.
    pub fn reactor_short_label(&self) -> String {
        match self {
            FlawConsequence::PerformanceDegradation(frac) =>
                format!("{:.0}% power loss", frac * 100.0),
            FlawConsequence::EngineLoss => "reactor shutdown".to_string(),
            FlawConsequence::StageLoss => "stage loss".to_string(),
        }
    }
}

impl std::fmt::Display for FlawConsequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlawConsequence::PerformanceDegradation(frac) =>
                write!(f, "{:.0}% performance loss", frac * 100.0),
            FlawConsequence::EngineLoss => write!(f, "engine loss"),
            FlawConsequence::StageLoss => write!(f, "stage loss"),
        }
    }
}

/// Serde default for the per-project `auto_revise` flag. New projects
/// and projects loaded from pre-M5 saves both get it on.
pub fn auto_revise_default() -> bool {
    true
}

/// A latent defect in a design that may activate in flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flaw {
    pub id: FlawId,
    pub description: String,
    pub consequence: FlawConsequence,
    /// Chance per flight (PerFlight) or cumulative over reference duration (PerDay).
    pub activation_chance: f64,
    /// Chance per testing cycle to discover this flaw.
    /// Computed as uniform(0,1)^N * sqrt(activation_chance), N being
    /// `FlawsConfig::flaw_discovery_exponent`.
    pub discovery_probability: f64,
    pub discovered: bool,
    /// When this flaw can trigger.
    #[serde(default)]
    pub trigger: FlawTrigger,
}

impl Flaw {
    /// For PerDay flaws, convert activation_chance to a daily rate.
    /// For PerFlight flaws, returns activation_chance unchanged.
    pub fn daily_rate(&self) -> f64 {
        match self.trigger {
            FlawTrigger::PerFlight => self.activation_chance,
            FlawTrigger::PerDay => {
                // activation_chance = 1 - (1 - daily_rate)^365
                // daily_rate = 1 - (1 - activation_chance)^(1/365)
                1.0 - (1.0 - self.activation_chance).powf(1.0 / FlawTrigger::REFERENCE_DAYS)
            }
        }
    }
}

/// Which kind of design a flaw belongs to. Selects the description
/// pools it is worded from and whether it can be an endurance (`PerDay`)
/// flaw at all — this, with the per-domain complexity that sets the
/// count, is where flaws differ between engines, rockets and reactors.
/// The probability model underneath is shared (`roll_flaw_core`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlawDomain {
    /// An engine; the cycle picks a specialised pool (electric,
    /// nuclear, solar sail) when known.
    Engine(Option<crate::engine::EngineCycle>),
    Rocket,
    Reactor,
}

impl FlawDomain {
    /// Fraction of generated flaws that are `PerDay`. `None` means the
    /// domain has no endurance flaws and draws no trigger roll.
    fn endurance_fraction(self, cfg: &FlawsConfig) -> Option<f64> {
        match self {
            FlawDomain::Engine(_) => None,
            FlawDomain::Rocket => Some(cfg.rocket_endurance_fraction),
            FlawDomain::Reactor => Some(cfg.reactor_endurance_fraction),
        }
    }
}

/// Generate the flaws of a newly completed design.
///
/// The count is drawn from a gaussian centred on `effective_complexity`
/// (stddev `cfg.count_stddev`), floored at zero. Each flaw then rolls
/// its trigger (domains with endurance flaws only), then the shared
/// probability core, then a description from the domain's pool.
pub fn generate_flaws(
    domain: FlawDomain,
    effective_complexity: u32,
    rng: &mut StdRng,
    next_flaw_id: &mut u64,
    cfg: &FlawsConfig,
) -> Vec<Flaw> {
    let mean = effective_complexity as f64;
    let count_f = gaussian_sample(mean, cfg.count_stddev, rng);
    let count = count_f.round().max(0.0) as u32;

    (0..count).map(|_| {
        let id = FlawId(*next_flaw_id);
        *next_flaw_id += 1;
        let trigger = match domain.endurance_fraction(cfg) {
            Some(fraction) if rng.gen::<f64>() < fraction => FlawTrigger::PerDay,
            _ => FlawTrigger::PerFlight,
        };
        generate_single_flaw(domain, id, trigger, rng, cfg)
    }).collect()
}

/// Build one flaw: shared probability core, domain-worded description.
/// `PerDay` flaws read as gradual wear, `PerFlight` ones as events, so
/// what the player is asked to revise sounds like the part it lives on.
pub fn generate_single_flaw(
    domain: FlawDomain, id: FlawId, trigger: FlawTrigger, rng: &mut StdRng, cfg: &FlawsConfig,
) -> Flaw {
    let (consequence, activation_chance, discovery_probability) = roll_flaw_core(rng, cfg);
    let description = FlawPool::for_flaw(domain, trigger).pick(&consequence, rng);
    Flaw {
        id,
        description,
        consequence,
        activation_chance,
        discovery_probability,
        discovered: false,
        trigger,
    }
}

/// Roll the domain-agnostic core of a flaw: its consequence, activation
/// chance, and discovery probability. Shared by engine, rocket, and
/// reactor flaw generation so the probability model stays in one place.
///
/// Consequence weighting: ~50% performance degradation, ~35% engine/part
/// loss, ~15% stage loss. Activation chance is random^2 (skewed low);
/// discovery probability = uniform(0,1)^N * sqrt(activation_chance),
/// where N is `flaw_discovery_exponent` (see FlawsConfig).
fn roll_flaw_core(rng: &mut StdRng, cfg: &FlawsConfig) -> (FlawConsequence, f64, f64) {
    let roll: f64 = rng.gen();
    let consequence = if roll < cfg.performance_degradation_weight {
        let degradation = rng.gen_range(cfg.degradation_min..cfg.degradation_max);
        FlawConsequence::PerformanceDegradation(degradation)
    } else if roll < cfg.performance_degradation_weight + cfg.engine_loss_weight {
        FlawConsequence::EngineLoss
    } else {
        FlawConsequence::StageLoss
    };

    let activation_chance: f64 = rng.gen::<f64>().powi(2);
    let uniform_roll: f64 = rng.gen();
    let discovery_probability =
        uniform_roll.powf(cfg.flaw_discovery_exponent) * activation_chance.sqrt();

    (consequence, activation_chance, discovery_probability)
}

/// The words a flaw is described with: six per consequence, drawn
/// uniformly. One pool per (domain, trigger) — and per engine cycle for
/// the cycles whose failure modes are nothing like a chemical engine's.
///
/// Every entry names a *property of the design*, not an event: a flaw is
/// latent, built in and carried until revised, and the trigger decides
/// when it bites. Endurance (`PerDay`) pools read as wear and drift; the
/// rest as things that go wrong during a burn.
#[derive(Debug, Clone, Copy)]
pub struct FlawPool {
    /// `PerformanceDegradation`: the part works worse.
    pub degradation: [&'static str; 6],
    /// `EngineLoss`: the part is lost (an engine shuts down, a reactor
    /// SCRAMs) but the stage survives.
    pub part_loss: [&'static str; 6],
    /// `StageLoss`: the whole stage is lost.
    pub stage_loss: [&'static str; 6],
}

impl FlawPool {
    /// One description for `consequence`, drawn uniformly.
    fn pick(&self, consequence: &FlawConsequence, rng: &mut StdRng) -> String {
        let descriptions = match consequence {
            FlawConsequence::PerformanceDegradation(_) => &self.degradation,
            FlawConsequence::EngineLoss => &self.part_loss,
            FlawConsequence::StageLoss => &self.stage_loss,
        };
        descriptions[rng.gen_range(0..descriptions.len())].to_string()
    }

    /// The pool a flaw of `domain` with `trigger` is worded from.
    fn for_flaw(domain: FlawDomain, trigger: FlawTrigger) -> &'static FlawPool {
        use crate::engine::EngineCycle;
        match (domain, trigger) {
            (FlawDomain::Rocket, FlawTrigger::PerDay) => &ROCKET_ENDURANCE_FLAWS,
            (FlawDomain::Rocket, FlawTrigger::PerFlight) => &ROCKET_FLAWS,
            (FlawDomain::Reactor, FlawTrigger::PerDay) => &REACTOR_ENDURANCE_FLAWS,
            (FlawDomain::Reactor, FlawTrigger::PerFlight) => &REACTOR_FLAWS,
            (FlawDomain::Engine(Some(EngineCycle::SolarSail)), FlawTrigger::PerFlight) => &SOLAR_SAIL_FLAWS,
            (FlawDomain::Engine(Some(EngineCycle::ElectricPropulsion)), FlawTrigger::PerFlight) => &ELECTRIC_FLAWS,
            (FlawDomain::Engine(Some(EngineCycle::NuclearThermal)), FlawTrigger::PerFlight) => &NUCLEAR_FLAWS,
            (FlawDomain::Engine(_), FlawTrigger::PerFlight) => &ENGINE_FLAWS,
            (FlawDomain::Engine(_), FlawTrigger::PerDay) => &ENGINE_ENDURANCE_FLAWS,
        }
    }
}

/// Chemical engines during a burn.
pub const ENGINE_FLAWS: FlawPool = FlawPool {
    degradation: [
        "Turbopump seal leak",
        "Injector pattern inefficiency",
        "Nozzle cooling channel restriction",
        "Valve response lag",
        "Combustion instability at partial throttle",
        "Propellant feed pressure oscillation",
    ],
    part_loss: [
        "Turbopump bearing fatigue",
        "Combustion chamber hot spot",
        "Igniter reliability issue",
        "Oxidizer-rich preburner instability",
        "Thermal stress cracking in nozzle",
        "Main injector face erosion",
    ],
    stage_loss: [
        "Propellant feed line vibration failure",
        "Stage separation bolt stress fracture",
        "Thrust structure resonance mode",
        "Ullage gas contamination risk",
        "Inter-stage electrical harness fault",
        "Catastrophic combustion instability",
    ],
};

/// Engines and their plumbing over a long mission.
pub const ENGINE_ENDURANCE_FLAWS: FlawPool = FlawPool {
    degradation: [
        "Thermal cycling degradation",
        "Sensor drift accumulation",
        "Propellant line seal wear",
        "Attitude control thruster fouling",
        "Radiator coating degradation",
        "Reaction wheel bearing wear",
    ],
    part_loss: [
        "Turbopump bearing wear",
        "Igniter electrode erosion",
        "Fuel valve seat degradation",
        "Oxidizer seal embrittlement",
        "Engine controller memory corruption",
        "Regenerative cooling tube fatigue",
    ],
    stage_loss: [
        "Avionics thermal failure",
        "Battery capacity degradation",
        "Structural fatigue crack propagation",
        "Guidance computer memory fault",
        "Wiring harness insulation breakdown",
        "Pressurization system leak",
    ],
};

/// Ion and Hall thrusters: grids, cathodes, xenon feed, the PPU.
pub const ELECTRIC_FLAWS: FlawPool = FlawPool {
    degradation: [
        "Ion grid erosion rate higher than expected",
        "Beam neutralizer current drift",
        "Discharge chamber magnetic field asymmetry",
        "Xenon flow controller calibration offset",
        "Thruster plume divergence angle excessive",
        "Power processing unit efficiency loss",
    ],
    part_loss: [
        "Grid short circuit from sputtered material",
        "Cathode heater element failure",
        "Xenon isolator valve seizure",
        "High-voltage breakdown in PPU",
        "Discharge chamber wall sputter-through",
        "Neutralizer keeper electrode erosion",
    ],
    stage_loss: [
        "Xenon tank pressure regulator failure",
        "Solar array connection arc fault",
        "Thruster gimbal mechanism binding",
        "Power bus overcurrent shutdown",
        "Propellant management unit leak",
        "Electromagnetic interference with avionics",
    ],
};

/// Nuclear-thermal engines: fuel elements, drums, hydrogen everywhere.
pub const NUCLEAR_FLAWS: FlawPool = FlawPool {
    degradation: [
        "Fuel element hydrogen corrosion",
        "Reactor power distribution imbalance",
        "Turbopump hydrogen bearing wear",
        "Nozzle skirt hydrogen embrittlement",
        "Moderator element swelling",
        "Reflector drum actuator lag",
    ],
    part_loss: [
        "Fuel element mid-section break",
        "Control drum servo mechanism failure",
        "Reactor thermal runaway risk",
        "Hydrogen leak in reactor pressure vessel",
        "Neutron poison buildup in fuel elements",
        "Turbopump seal failure from radiation damage",
    ],
    stage_loss: [
        "Radiation shielding structural failure",
        "Reactor SCRAM system false trigger",
        "Hydrogen tank embrittlement fracture",
        "Reactor coolant channel blockage",
        "Uncontrolled criticality excursion risk",
        "Nozzle detachment from thermal cycling",
    ],
};

/// Solar sails: film, booms, deployment.
pub const SOLAR_SAIL_FLAWS: FlawPool = FlawPool {
    degradation: [
        "Sail reflectivity degradation",
        "Micrometeorite puncture damage",
        "Sail deployment mechanism binding",
        "Attitude control vane misalignment",
        "Sail surface wrinkling",
        "Solar radiation pressure modeling error",
    ],
    part_loss: [
        "Sail boom structural failure",
        "Complete sail deployment failure",
        "Sail tearing from thermal stress",
        "Attitude control system failure",
        "Sail furling mechanism jam",
        "Boom hinge seizure",
    ],
    stage_loss: [
        "Sail catastrophic tear propagation",
        "Boom collapse from impact",
        "Sail jettison mechanism malfunction",
        "Thermal deformation beyond recovery",
        "Complete attitude loss from sail asymmetry",
        "Sail connection point failure",
    ],
};

/// The vehicle rather than its engines: tankage, structure, separation,
/// avionics, plumbing. An engine project already owns injectors and
/// turbopumps, so a rocket project that borrowed those words read as if
/// you were revising somebody else's work.
pub const ROCKET_FLAWS: FlawPool = FlawPool {
    degradation: [
        "Tank baffle slosh damping insufficient",
        "Aerodynamic fairing drag higher than modelled",
        "Guidance loop overcorrects in high winds",
        "Stage mass over budget after assembly",
        "Thrust vector alignment out of tolerance",
        "Residual propellant trapped at tank sump",
    ],
    part_loss: [
        "Propellant feed starves the outboard engine",
        "Engine bay overheats without purge flow",
        "Gimbal actuator mount flexes under load",
        "Pogo suppressor undersized for this stage",
        "Engine mount bolt preload inconsistent",
        "Feed line collapses under transient pressure",
    ],
    stage_loss: [
        "Interstage buckles under max-Q loading",
        "Separation pyrotechnics fire out of sequence",
        "Common bulkhead weld porosity",
        "Payload fairing fails to jettison cleanly",
        "Tank pressurisation regulator runs away",
        "Flight computer resets during staging transient",
    ],
};

/// The airframe and its systems wearing and drifting in transit —
/// "no micrometeoroid shielding", not "pitting weakened a tank wall".
pub const ROCKET_ENDURANCE_FLAWS: FlawPool = FlawPool {
    degradation: [
        "Tank insulation degrades, boiloff climbs",
        "Star tracker alignment drifts with thermal cycling",
        "Attitude control propellant leaks past a seat",
        "Solar array hinge stiffens, pointing lags",
        "Thermal coating erodes under UV",
        "Reaction wheel imbalance grows with hours",
    ],
    part_loss: [
        "Restart accumulator loses pressure over days",
        "Engine bay heater fails, propellant lines chill",
        "Ullage motor propellant slowly vents",
        "Gimbal actuator lubricant migrates in vacuum",
        "Feed line bellows fatigues on each thermal cycle",
        "Engine controller watchdog trips intermittently",
    ],
    stage_loss: [
        "No micrometeoroid shielding over the tank wall",
        "Battery cell imbalance goes uncorrected",
        "Harness insulation embrittles and shorts",
        "Pressurant slowly leaks past a check valve",
        "Structural adhesive creeps under sustained load",
        "Flight computer accumulates uncorrected bit flips",
    ],
};

/// Reactors at power. Degradation reads as lost output; part loss as a
/// shutdown (the reactor trips, not the stage).
pub const REACTOR_FLAWS: FlawPool = FlawPool {
    degradation: [
        "Coolant loop flow restriction",
        "Radiator fin degradation reduces heat rejection",
        "Control drum drift derates output",
        "Fuel element swelling reduces thermal transfer",
        "Thermoelectric converter efficiency loss",
        "Partial coolant channel blockage",
    ],
    part_loss: [
        "Control drum actuator seizure triggers SCRAM",
        "Coolant pump failure forces reactor shutdown",
        "Fuel element cladding breach",
        "Reactor overheats and trips offline",
        "Neutron poison buildup stalls the core",
        "Primary coolant loop leak",
    ],
    stage_loss: [
        "Reactor pressure vessel rupture",
        "Uncontrolled criticality excursion",
        "Radiation shielding structural failure",
        "Coolant flash-boil breaches the stage",
        "Thermal runaway destroys the stage",
        "Reactor debris severs stage structure",
    ],
};

/// Reactors over a long mission: burnup, fouling, embrittlement, creep.
pub const REACTOR_ENDURANCE_FLAWS: FlawPool = FlawPool {
    degradation: [
        "Radiator coating erosion degrades heat rejection",
        "Fuel burnup lowers reactivity over time",
        "Neutron embrittlement of core structure",
        "Coolant loop fouling accumulates",
        "Thermoelectric junction degradation",
        "Control drum bearing wear derates output",
    ],
    part_loss: [
        "Fuel cladding creep-ruptures after prolonged heat",
        "Coolant pump bearing wears out and seizes",
        "Cumulative xenon poisoning stalls the core",
        "Control-drum actuator fails from thermal cycling",
        "Primary loop develops a slow coolant leak",
        "Reactor trips offline on degraded shielding sensors",
    ],
    stage_loss: [
        "Coolant embrittlement leads to pressure-vessel failure",
        "Long-term radiation damage collapses the structure",
        "Cumulative thermal fatigue cracks the reactor mount",
        "Shielding degradation triggers a runaway excursion",
        "Radiator manifold fatigue ruptures the coolant loop",
        "Structural creep severs the stage under load",
    ],
};

/// Roll for flaw discovery during a testing cycle.
/// Returns indices of newly discovered flaws.
pub fn roll_discoveries_with_rng(flaws: &mut [Flaw], rng: &mut StdRng) -> Vec<usize> {
    let mut discovered = Vec::new();
    for (i, flaw) in flaws.iter_mut().enumerate() {
        if !flaw.discovered {
            let roll: f64 = rng.gen();
            if roll < flaw.discovery_probability {
                flaw.discovered = true;
                discovered.push(i);
            }
        }
    }
    discovered
}

/// Sample from a gaussian distribution using Box-Muller transform.
fn gaussian_sample(mean: f64, stddev: f64, rng: &mut StdRng) -> f64 {
    let u1: f64 = rng.gen();
    let u2: f64 = rng.gen();
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    mean + stddev * z
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn test_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    fn cfg() -> FlawsConfig {
        FlawsConfig::default()
    }

    #[test]
    fn test_generate_flaws_count_near_complexity() {
        // Run many times and check average is near effective_complexity
        let mut total = 0u32;
        let trials = 1000;
        for seed in 0..trials {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut next_id = 0u64;
            let flaws = generate_flaws(FlawDomain::Engine(None), 7, &mut rng, &mut next_id, &cfg());
            total += flaws.len() as u32;
        }
        let avg = total as f64 / trials as f64;
        // Should be close to 7 (±1 is fine for 1000 trials)
        assert!((avg - 7.0).abs() < 1.0, "Average flaw count {} should be near 7", avg);
    }

    #[test]
    fn test_generate_flaws_can_be_zero() {
        // With low complexity, some runs should produce zero flaws
        let mut found_zero = false;
        for seed in 0..1000 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut next_id = 0u64;
            let flaws = generate_flaws(FlawDomain::Engine(None), 2, &mut rng, &mut next_id, &cfg());
            if flaws.is_empty() {
                found_zero = true;
                break;
            }
        }
        assert!(found_zero, "Should sometimes generate zero flaws at low complexity");
    }

    #[test]
    fn test_flaw_ids_are_sequential() {
        let mut rng = test_rng();
        let mut next_id = 10u64;
        let flaws = generate_flaws(FlawDomain::Engine(None), 6, &mut rng, &mut next_id, &cfg());
        for (i, flaw) in flaws.iter().enumerate() {
            assert_eq!(flaw.id, FlawId(10 + i as u64));
        }
        assert_eq!(next_id, 10 + flaws.len() as u64);
    }

    #[test]
    fn test_flaws_start_undiscovered() {
        let mut rng = test_rng();
        let mut next_id = 0u64;
        let flaws = generate_flaws(FlawDomain::Engine(None), 8, &mut rng, &mut next_id, &cfg());
        for flaw in &flaws {
            assert!(!flaw.discovered);
        }
    }

    #[test]
    fn test_activation_chance_in_range() {
        let mut rng = test_rng();
        let mut next_id = 0u64;
        let flaws = generate_flaws(FlawDomain::Engine(None), 9, &mut rng, &mut next_id, &cfg());
        for flaw in &flaws {
            assert!(flaw.activation_chance >= 0.0, "activation_chance should be non-negative");
            assert!(flaw.activation_chance <= 1.0, "activation_chance should be <= 1");
        }
    }

    #[test]
    fn test_activation_chance_skewed_low() {
        // With random^2, most values should be below 0.5
        let mut rng = test_rng();
        let mut next_id = 0u64;
        let flaws = generate_flaws(FlawDomain::Engine(None), 100, &mut rng, &mut next_id, &cfg());
        let below_half = flaws.iter().filter(|f| f.activation_chance < 0.5).count();
        assert!(
            below_half as f64 / flaws.len() as f64 > 0.6,
            "Most activation chances should be below 0.5 (got {}/{})",
            below_half, flaws.len(),
        );
    }

    #[test]
    fn test_discovery_probability_bounded_by_sqrt_activation() {
        let mut rng = test_rng();
        let mut next_id = 0u64;
        let flaws = generate_flaws(FlawDomain::Engine(None), 9, &mut rng, &mut next_id, &cfg());
        for flaw in &flaws {
            assert!(
                flaw.discovery_probability <= flaw.activation_chance.sqrt() + 0.001,
                "discovery {} should be <= sqrt(activation {}) = {}",
                flaw.discovery_probability,
                flaw.activation_chance,
                flaw.activation_chance.sqrt()
            );
        }
    }

    #[test]
    fn test_roll_discoveries() {
        let mut rng = test_rng();
        let mut next_id = 0u64;
        let mut flaws = generate_flaws(FlawDomain::Engine(None), 8, &mut rng, &mut next_id, &cfg());

        // Force high discovery probability on first flaw for testing
        if !flaws.is_empty() {
            flaws[0].discovery_probability = 0.99;
        }

        // Roll many times — should eventually discover the high-probability flaw
        let mut discovered_first = false;
        for seed in 0..100 {
            let mut roll_rng = StdRng::seed_from_u64(seed + 1000);
            let newly = roll_discoveries_with_rng(&mut flaws, &mut roll_rng);
            if newly.contains(&0) {
                discovered_first = true;
                break;
            }
            // Reset for next attempt
            if !discovered_first {
                flaws[0].discovered = false;
            }
        }
        assert!(discovered_first, "Should discover a flaw with 0.99 probability");
    }

    #[test]
    fn test_consequence_display() {
        assert_eq!(
            FlawConsequence::PerformanceDegradation(0.05).to_string(),
            "5% performance loss"
        );
        assert_eq!(FlawConsequence::EngineLoss.to_string(), "engine loss");
        assert_eq!(FlawConsequence::StageLoss.to_string(), "stage loss");
    }

    #[test]
    fn test_gaussian_distribution() {
        let mut rng = test_rng();
        let samples: Vec<f64> = (0..10000).map(|_| gaussian_sample(7.0, 1.5, &mut rng)).collect();
        let mean: f64 = samples.iter().sum::<f64>() / samples.len() as f64;
        assert!((mean - 7.0).abs() < 0.1, "Gaussian mean {} should be near 7.0", mean);
    }

    #[test]
    fn test_daily_rate_per_flight_unchanged() {
        let flaw = Flaw {
            id: FlawId(1), description: "test".into(),
            consequence: FlawConsequence::EngineLoss,
            activation_chance: 0.5,
            discovery_probability: 0.3,
            discovered: false,
            trigger: FlawTrigger::PerFlight,
        };
        assert_eq!(flaw.daily_rate(), 0.5);
    }

    #[test]
    fn test_daily_rate_per_day_conversion() {
        let flaw = Flaw {
            id: FlawId(1), description: "test".into(),
            consequence: FlawConsequence::EngineLoss,
            activation_chance: 0.30,
            discovery_probability: 0.3,
            discovered: false,
            trigger: FlawTrigger::PerDay,
        };
        let rate = flaw.daily_rate();
        // 1 - (1 - 0.30)^(1/365) ≈ 0.000977
        assert!(rate > 0.0009 && rate < 0.0011,
            "Daily rate should be ~0.097%/day, got {}", rate);

        // Verify: cumulative over 365 days should recover ~0.30
        let cumulative = 1.0 - (1.0 - rate).powi(365);
        assert!((cumulative - 0.30).abs() < 0.001,
            "Cumulative over 365 days should be ~0.30, got {}", cumulative);
    }

    #[test]
    fn test_rocket_flaws_have_per_day() {
        let mut rng = test_rng();
        let mut next_id = 0u64;
        let flaws = generate_flaws(FlawDomain::Rocket, 10, &mut rng, &mut next_id, &cfg());
        let per_day_count = flaws.iter().filter(|f| f.trigger == FlawTrigger::PerDay).count();
        // With 30% chance and ~10 flaws, expect ~3 PerDay (allow 0-8 for randomness)
        assert!(per_day_count > 0, "Should have some PerDay flaws");
        assert!(per_day_count < flaws.len(), "Should have some PerFlight flaws too");
    }

    #[test]
    fn test_reactor_flaws_have_endurance_mix() {
        // Reactors run continuously, so ~30% of their flaws are PerDay
        // endurance flaws and the rest PerFlight. Aggregate over seeds so
        // the statistical split is reliable.
        let mut per_day = 0usize;
        let mut per_flight = 0usize;
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut next_id = 0u64;
            for flaw in generate_flaws(FlawDomain::Reactor, 10, &mut rng, &mut next_id, &cfg()) {
                match flaw.trigger {
                    FlawTrigger::PerDay => per_day += 1,
                    FlawTrigger::PerFlight => per_flight += 1,
                }
            }
        }
        assert!(per_day > 0, "reactors should have some PerDay endurance flaws");
        assert!(per_flight > 0, "reactors should have some PerFlight flaws too");
        // Roughly 30% endurance — allow a wide band for randomness.
        let frac = per_day as f64 / (per_day + per_flight) as f64;
        assert!(frac > 0.15 && frac < 0.45, "endurance fraction {} should be ~0.30", frac);
    }

    #[test]
    fn test_reactor_flaws_count_near_complexity() {
        let mut total = 0u32;
        let trials = 1000;
        for seed in 0..trials {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut next_id = 0u64;
            let flaws = generate_flaws(FlawDomain::Reactor, 8, &mut rng, &mut next_id, &cfg());
            total += flaws.len() as u32;
        }
        let avg = total as f64 / trials as f64;
        assert!((avg - 8.0).abs() < 1.0, "Average reactor flaw count {} should be near 8", avg);
    }

    #[test]
    fn test_reactor_flaws_ids_sequential_and_undiscovered() {
        let mut rng = test_rng();
        let mut next_id = 5u64;
        let flaws = generate_flaws(FlawDomain::Reactor, 9, &mut rng, &mut next_id, &cfg());
        for (i, flaw) in flaws.iter().enumerate() {
            assert_eq!(flaw.id, FlawId(5 + i as u64));
            assert!(!flaw.discovered);
        }
        assert_eq!(next_id, 5 + flaws.len() as u64);
    }

    #[test]
    fn test_engine_flaws_all_per_flight() {
        let mut rng = test_rng();
        let mut next_id = 0u64;
        let flaws = generate_flaws(FlawDomain::Engine(None), 10, &mut rng, &mut next_id, &cfg());
        for flaw in &flaws {
            assert_eq!(flaw.trigger, FlawTrigger::PerFlight,
                "Engine flaws should all be PerFlight");
        }
    }
}

#[cfg(test)]
mod description_pool_tests {
    use super::*;
    use rand::SeedableRng;

    fn cfg() -> FlawsConfig {
        crate::balance_config::BalanceConfig::default().flaws
    }

    /// Every description a rocket project can produce, over enough rolls
    /// to reach every entry in both pools.
    fn all_rocket_descriptions() -> Vec<String> {
        let mut rng = StdRng::seed_from_u64(7);
        let cfg = cfg();
        let mut out = Vec::new();
        for i in 0..2_000u64 {
            let trigger = if i % 2 == 0 { FlawTrigger::PerFlight } else { FlawTrigger::PerDay };
            out.push(generate_single_flaw(FlawDomain::Rocket, FlawId(i), trigger, &mut rng, &cfg).description);
        }
        out
    }

    fn all_engine_descriptions() -> Vec<String> {
        let mut rng = StdRng::seed_from_u64(7);
        let cfg = cfg();
        let mut out = Vec::new();
        for i in 0..2_000u64 {
            let trigger = if i % 2 == 0 { FlawTrigger::PerFlight } else { FlawTrigger::PerDay };
            out.push(generate_single_flaw(FlawDomain::Engine(None), FlawId(i), trigger, &mut rng, &cfg).description);
        }
        out
    }

    /// A rocket project owns tankage, structure, separation and avionics;
    /// an engine project owns injectors and turbopumps. Revising a rocket
    /// used to hand you "Injector pattern inefficiency", which reads as
    /// somebody else's work.
    #[test]
    fn rocket_flaws_never_borrow_engine_internals() {
        let rocket = all_rocket_descriptions();
        let engine = all_engine_descriptions();

        for d in &rocket {
            assert!(!engine.contains(d),
                "rocket flaw {d:?} is also in the engine pool");
        }
        // And the obvious engine-internal words stay out of it.
        for word in ["Injector", "injector", "Turbopump", "turbopump",
                     "Combustion chamber", "preburner", "Nozzle"] {
            assert!(!rocket.iter().any(|d| d.contains(word)),
                "a rocket flaw mentions {word:?}");
        }
    }

    /// Both triggers draw from the rocket pools — the endurance half is
    /// the one a player sees most, since it is what strands a craft.
    #[test]
    fn both_rocket_triggers_have_their_own_pool() {
        let mut rng = StdRng::seed_from_u64(11);
        let cfg = cfg();
        let per_flight: Vec<String> = (0..300u64)
            .map(|i| generate_single_flaw(FlawDomain::Rocket, 
                FlawId(i), FlawTrigger::PerFlight, &mut rng, &cfg).description)
            .collect();
        let per_day: Vec<String> = (0..300u64)
            .map(|i| generate_single_flaw(FlawDomain::Rocket, 
                FlawId(i), FlawTrigger::PerDay, &mut rng, &cfg).description)
            .collect();

        assert!(per_flight.iter().any(|d| d.contains("Interstage")
            || d.contains("separation") || d.contains("Separation")),
            "per-flight flaws are staging and burn events");
        assert!(per_day.iter().any(|d| d.contains("boiloff")
            || d.contains("drifts") || d.contains("erodes")),
            "per-day flaws are wear and drift");
        assert!(per_flight.iter().all(|d| !per_day.contains(d)),
            "the two pools stay distinct");
    }

    /// Only the description changed: the probability model is still shared
    /// with engine and reactor flaws.
    #[test]
    fn rocket_flaws_keep_the_shared_probability_core() {
        let cfg = cfg();
        let mut a = StdRng::seed_from_u64(3);
        let mut b = StdRng::seed_from_u64(3);
        let rocket = generate_single_flaw(FlawDomain::Rocket, FlawId(1), FlawTrigger::PerFlight, &mut a, &cfg);
        let engine = generate_single_flaw(FlawDomain::Engine(None), FlawId(1), FlawTrigger::PerFlight, &mut b, &cfg);

        // FlawConsequence has no PartialEq; its Display is faithful enough.
        assert_eq!(rocket.consequence.to_string(), engine.consequence.to_string());
        assert_eq!(rocket.activation_chance, engine.activation_chance);
        assert_eq!(rocket.discovery_probability, engine.discovery_probability);
        assert_ne!(rocket.description, engine.description, "but the words differ");
    }
}

