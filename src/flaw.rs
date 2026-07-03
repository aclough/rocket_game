use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

/// Unique identifier for a flaw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlawId(pub u64);

/// When a flaw can trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FlawTrigger {
    /// Rolls once when the stage fires (existing behavior).
    PerFlight,
    /// Rolls every day in flight (endurance flaw).
    PerDay,
}

impl Default for FlawTrigger {
    fn default() -> Self { FlawTrigger::PerFlight }
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

/// A flaw in an engine design that may activate during flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flaw {
    pub id: FlawId,
    pub description: String,
    pub consequence: FlawConsequence,
    /// Chance per flight (PerFlight) or cumulative over reference duration (PerDay).
    pub activation_chance: f64,
    /// Chance per testing cycle to discover this flaw.
    /// Computed as uniform(0,1) * sqrt(activation_chance).
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

/// Work units required to fix one flaw via revision.
pub const FLAW_REVISION_WORK: f64 = 30.0;

/// Work units per testing cycle.
pub const TESTING_CYCLE_WORK: f64 = 30.0;

/// Generate flaws for a newly completed engine design.
///
/// `effective_complexity` includes cycle + fuel complexity + problems factor.
/// Flaw count is drawn from a gaussian centered on effective_complexity with stddev ~1.5,
/// converted to a non-negative integer.
pub fn generate_flaws(
    effective_complexity: u32,
    rng: &mut StdRng,
    next_flaw_id: &mut u64,
) -> Vec<Flaw> {
    generate_flaws_for_cycle(effective_complexity, rng, next_flaw_id, None)
}

/// Generate flaws with cycle-specific descriptions.
pub fn generate_flaws_for_cycle(
    effective_complexity: u32,
    rng: &mut StdRng,
    next_flaw_id: &mut u64,
    cycle: Option<crate::engine::EngineCycle>,
) -> Vec<Flaw> {
    let mean = effective_complexity as f64;
    let stddev = 1.5;

    let count_f = gaussian_sample(mean, stddev, rng);
    let count = count_f.round().max(0.0) as u32;

    (0..count).map(|_| {
        let id = FlawId(*next_flaw_id);
        *next_flaw_id += 1;
        generate_single_flaw(id, FlawTrigger::PerFlight, rng, cycle)
    }).collect()
}

/// Generate flaws for a rocket project. ~30% are endurance (PerDay) flaws.
pub fn generate_rocket_flaws(
    effective_complexity: u32,
    rng: &mut StdRng,
    next_flaw_id: &mut u64,
) -> Vec<Flaw> {
    let mean = effective_complexity as f64;
    let stddev = 1.5;
    let count_f = gaussian_sample(mean, stddev, rng);
    let count = count_f.round().max(0.0) as u32;

    (0..count).map(|_| {
        let id = FlawId(*next_flaw_id);
        *next_flaw_id += 1;
        let trigger = if rng.gen::<f64>() < 0.30 {
            FlawTrigger::PerDay
        } else {
            FlawTrigger::PerFlight
        };
        generate_single_flaw(id, trigger, rng, None)
    }).collect()
}

/// Roll the domain-agnostic core of a flaw: its consequence, activation
/// chance, and discovery probability. Shared by engine, rocket, and
/// reactor flaw generation so the probability model stays in one place.
///
/// Consequence weighting: ~50% performance degradation, ~35% engine/part
/// loss, ~15% stage loss. Activation chance is random^2 (skewed low);
/// discovery probability = uniform(0,1) * sqrt(activation_chance).
fn roll_flaw_core(rng: &mut StdRng) -> (FlawConsequence, f64, f64) {
    let roll: f64 = rng.gen();
    let consequence = if roll < 0.50 {
        let degradation = rng.gen_range(0.03..0.15);
        FlawConsequence::PerformanceDegradation(degradation)
    } else if roll < 0.85 {
        FlawConsequence::EngineLoss
    } else {
        FlawConsequence::StageLoss
    };

    let activation_chance: f64 = rng.gen::<f64>().powi(2);
    let uniform_roll: f64 = rng.gen();
    let discovery_probability = uniform_roll * activation_chance.sqrt();

    (consequence, activation_chance, discovery_probability)
}

/// Generate flaws for a newly completed reactor design.
///
/// Mirrors `generate_flaws` (count ~ gaussian around effective
/// complexity) but uses reactor-flavored descriptions and a
/// reactor-appropriate consequence reading (performance degradation =
/// power loss, engine loss = reactor shutdown). All `PerFlight` for v1 —
/// reactor endurance (`PerDay`) flaws are deferred to Phase 3b along
/// with the flight-wiring itself.
pub fn generate_reactor_flaws(
    effective_complexity: u32,
    rng: &mut StdRng,
    next_flaw_id: &mut u64,
) -> Vec<Flaw> {
    let mean = effective_complexity as f64;
    let stddev = 1.5;
    let count_f = gaussian_sample(mean, stddev, rng);
    let count = count_f.round().max(0.0) as u32;

    (0..count).map(|_| {
        let id = FlawId(*next_flaw_id);
        *next_flaw_id += 1;
        generate_single_reactor_flaw(id, rng)
    }).collect()
}

/// Build one reactor flaw. Reuses the shared probability core with a
/// reactor-specific description.
pub fn generate_single_reactor_flaw(id: FlawId, rng: &mut StdRng) -> Flaw {
    let (consequence, activation_chance, discovery_probability) = roll_flaw_core(rng);
    let description = generate_reactor_flaw_description(&consequence, rng);
    Flaw {
        id,
        description,
        consequence,
        activation_chance,
        discovery_probability,
        discovered: false,
        trigger: FlawTrigger::PerFlight,
    }
}

fn generate_reactor_flaw_description(consequence: &FlawConsequence, rng: &mut StdRng) -> String {
    let descriptions = match consequence {
        // Reads as a power-output loss on a reactor.
        FlawConsequence::PerformanceDegradation(_) => &[
            "Coolant loop flow restriction",
            "Radiator fin degradation reduces heat rejection",
            "Control drum drift derates output",
            "Fuel element swelling reduces thermal transfer",
            "Thermoelectric converter efficiency loss",
            "Partial coolant channel blockage",
        ][..],
        // Reads as a reactor shutdown (the "part" is lost, not the stage).
        FlawConsequence::EngineLoss => &[
            "Control drum actuator seizure triggers SCRAM",
            "Coolant pump failure forces reactor shutdown",
            "Fuel element cladding breach",
            "Reactor overheats and trips offline",
            "Neutron poison buildup stalls the core",
            "Primary coolant loop leak",
        ][..],
        FlawConsequence::StageLoss => &[
            "Reactor pressure vessel rupture",
            "Uncontrolled criticality excursion",
            "Radiation shielding structural failure",
            "Coolant flash-boil breaches the stage",
            "Thermal runaway destroys the stage",
            "Reactor debris severs stage structure",
        ][..],
    };

    let idx = rng.gen_range(0..descriptions.len());
    descriptions[idx].to_string()
}

pub fn generate_single_flaw(id: FlawId, trigger: FlawTrigger, rng: &mut StdRng, cycle: Option<crate::engine::EngineCycle>) -> Flaw {
    let (consequence, activation_chance, discovery_probability) = roll_flaw_core(rng);

    let use_electric = matches!(cycle, Some(crate::engine::EngineCycle::ElectricPropulsion));
    let use_nuclear = matches!(cycle, Some(crate::engine::EngineCycle::NuclearThermal));
    let use_solar_sail = matches!(cycle, Some(crate::engine::EngineCycle::SolarSail));

    let description = match trigger {
        FlawTrigger::PerFlight if use_solar_sail =>
            generate_solar_sail_flaw_description(&consequence, rng),
        FlawTrigger::PerFlight if use_electric =>
            generate_electric_flaw_description(&consequence, rng),
        FlawTrigger::PerFlight if use_nuclear =>
            generate_nuclear_flaw_description(&consequence, rng),
        FlawTrigger::PerFlight => generate_flaw_description(&consequence, rng),
        FlawTrigger::PerDay => generate_endurance_flaw_description(&consequence, rng),
    };

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

fn generate_flaw_description(consequence: &FlawConsequence, rng: &mut StdRng) -> String {
    let descriptions = match consequence {
        FlawConsequence::PerformanceDegradation(_) => &[
            "Turbopump seal leak",
            "Injector pattern inefficiency",
            "Nozzle cooling channel restriction",
            "Valve response lag",
            "Combustion instability at partial throttle",
            "Propellant feed pressure oscillation",
        ][..],
        FlawConsequence::EngineLoss => &[
            "Turbopump bearing fatigue",
            "Combustion chamber hot spot",
            "Igniter reliability issue",
            "Oxidizer-rich preburner instability",
            "Thermal stress cracking in nozzle",
            "Main injector face erosion",
        ][..],
        FlawConsequence::StageLoss => &[
            "Propellant feed line vibration failure",
            "Stage separation bolt stress fracture",
            "Thrust structure resonance mode",
            "Ullage gas contamination risk",
            "Inter-stage electrical harness fault",
            "Catastrophic combustion instability",
        ][..],
    };

    let idx = rng.gen_range(0..descriptions.len());
    descriptions[idx].to_string()
}

fn generate_endurance_flaw_description(consequence: &FlawConsequence, rng: &mut StdRng) -> String {
    let descriptions = match consequence {
        FlawConsequence::PerformanceDegradation(_) => &[
            "Thermal cycling degradation",
            "Sensor drift accumulation",
            "Propellant line seal wear",
            "Attitude control thruster fouling",
            "Radiator coating degradation",
            "Reaction wheel bearing wear",
        ][..],
        FlawConsequence::EngineLoss => &[
            "Turbopump bearing wear",
            "Igniter electrode erosion",
            "Fuel valve seat degradation",
            "Oxidizer seal embrittlement",
            "Engine controller memory corruption",
            "Regenerative cooling tube fatigue",
        ][..],
        FlawConsequence::StageLoss => &[
            "Avionics thermal failure",
            "Battery capacity degradation",
            "Structural fatigue crack propagation",
            "Guidance computer memory fault",
            "Wiring harness insulation breakdown",
            "Pressurization system leak",
        ][..],
    };

    let idx = rng.gen_range(0..descriptions.len());
    descriptions[idx].to_string()
}

fn generate_electric_flaw_description(consequence: &FlawConsequence, rng: &mut StdRng) -> String {
    let descriptions = match consequence {
        FlawConsequence::PerformanceDegradation(_) => &[
            "Ion grid erosion rate higher than expected",
            "Beam neutralizer current drift",
            "Discharge chamber magnetic field asymmetry",
            "Xenon flow controller calibration offset",
            "Thruster plume divergence angle excessive",
            "Power processing unit efficiency loss",
        ][..],
        FlawConsequence::EngineLoss => &[
            "Grid short circuit from sputtered material",
            "Cathode heater element failure",
            "Xenon isolator valve seizure",
            "High-voltage breakdown in PPU",
            "Discharge chamber wall sputter-through",
            "Neutralizer keeper electrode erosion",
        ][..],
        FlawConsequence::StageLoss => &[
            "Xenon tank pressure regulator failure",
            "Solar array connection arc fault",
            "Thruster gimbal mechanism binding",
            "Power bus overcurrent shutdown",
            "Propellant management unit leak",
            "Electromagnetic interference with avionics",
        ][..],
    };

    let idx = rng.gen_range(0..descriptions.len());
    descriptions[idx].to_string()
}

fn generate_nuclear_flaw_description(consequence: &FlawConsequence, rng: &mut StdRng) -> String {
    let descriptions = match consequence {
        FlawConsequence::PerformanceDegradation(_) => &[
            "Fuel element hydrogen corrosion",
            "Reactor power distribution imbalance",
            "Turbopump hydrogen bearing wear",
            "Nozzle skirt hydrogen embrittlement",
            "Moderator element swelling",
            "Reflector drum actuator lag",
        ][..],
        FlawConsequence::EngineLoss => &[
            "Fuel element mid-section break",
            "Control drum servo mechanism failure",
            "Reactor thermal runaway risk",
            "Hydrogen leak in reactor pressure vessel",
            "Neutron poison buildup in fuel elements",
            "Turbopump seal failure from radiation damage",
        ][..],
        FlawConsequence::StageLoss => &[
            "Radiation shielding structural failure",
            "Reactor SCRAM system false trigger",
            "Hydrogen tank embrittlement fracture",
            "Reactor coolant channel blockage",
            "Uncontrolled criticality excursion risk",
            "Nozzle detachment from thermal cycling",
        ][..],
    };

    let idx = rng.gen_range(0..descriptions.len());
    descriptions[idx].to_string()
}

fn generate_solar_sail_flaw_description(consequence: &FlawConsequence, rng: &mut StdRng) -> String {
    let descriptions = match consequence {
        FlawConsequence::PerformanceDegradation(_) => &[
            "Sail reflectivity degradation",
            "Micrometeorite puncture damage",
            "Sail deployment mechanism binding",
            "Attitude control vane misalignment",
            "Sail surface wrinkling",
            "Solar radiation pressure modeling error",
        ][..],
        FlawConsequence::EngineLoss => &[
            "Sail boom structural failure",
            "Complete sail deployment failure",
            "Sail tearing from thermal stress",
            "Attitude control system failure",
            "Sail furling mechanism jam",
            "Boom hinge seizure",
        ][..],
        FlawConsequence::StageLoss => &[
            "Sail catastrophic tear propagation",
            "Boom collapse from impact",
            "Sail jettison mechanism malfunction",
            "Thermal deformation beyond recovery",
            "Complete attitude loss from sail asymmetry",
            "Sail connection point failure",
        ][..],
    };

    let idx = rng.gen_range(0..descriptions.len());
    descriptions[idx].to_string()
}

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

    #[test]
    fn test_generate_flaws_count_near_complexity() {
        // Run many times and check average is near effective_complexity
        let mut total = 0u32;
        let trials = 1000;
        for seed in 0..trials {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut next_id = 0u64;
            let flaws = generate_flaws(7, &mut rng, &mut next_id);
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
            let flaws = generate_flaws(2, &mut rng, &mut next_id);
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
        let flaws = generate_flaws(6, &mut rng, &mut next_id);
        for (i, flaw) in flaws.iter().enumerate() {
            assert_eq!(flaw.id, FlawId(10 + i as u64));
        }
        assert_eq!(next_id, 10 + flaws.len() as u64);
    }

    #[test]
    fn test_flaws_start_undiscovered() {
        let mut rng = test_rng();
        let mut next_id = 0u64;
        let flaws = generate_flaws(8, &mut rng, &mut next_id);
        for flaw in &flaws {
            assert!(!flaw.discovered);
        }
    }

    #[test]
    fn test_activation_chance_in_range() {
        let mut rng = test_rng();
        let mut next_id = 0u64;
        let flaws = generate_flaws(9, &mut rng, &mut next_id);
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
        let flaws = generate_flaws(100, &mut rng, &mut next_id);
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
        let flaws = generate_flaws(9, &mut rng, &mut next_id);
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
        let mut flaws = generate_flaws(8, &mut rng, &mut next_id);

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
        let flaws = generate_rocket_flaws(10, &mut rng, &mut next_id);
        let per_day_count = flaws.iter().filter(|f| f.trigger == FlawTrigger::PerDay).count();
        // With 30% chance and ~10 flaws, expect ~3 PerDay (allow 0-8 for randomness)
        assert!(per_day_count > 0, "Should have some PerDay flaws");
        assert!(per_day_count < flaws.len(), "Should have some PerFlight flaws too");
    }

    #[test]
    fn test_reactor_flaws_all_per_flight() {
        let mut rng = test_rng();
        let mut next_id = 0u64;
        let flaws = generate_reactor_flaws(10, &mut rng, &mut next_id);
        for flaw in &flaws {
            assert_eq!(flaw.trigger, FlawTrigger::PerFlight,
                "Reactor flaws should all be PerFlight in v1");
        }
    }

    #[test]
    fn test_reactor_flaws_count_near_complexity() {
        let mut total = 0u32;
        let trials = 1000;
        for seed in 0..trials {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut next_id = 0u64;
            let flaws = generate_reactor_flaws(8, &mut rng, &mut next_id);
            total += flaws.len() as u32;
        }
        let avg = total as f64 / trials as f64;
        assert!((avg - 8.0).abs() < 1.0, "Average reactor flaw count {} should be near 8", avg);
    }

    #[test]
    fn test_reactor_flaws_ids_sequential_and_undiscovered() {
        let mut rng = test_rng();
        let mut next_id = 5u64;
        let flaws = generate_reactor_flaws(9, &mut rng, &mut next_id);
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
        let flaws = generate_flaws(10, &mut rng, &mut next_id);
        for flaw in &flaws {
            assert_eq!(flaw.trigger, FlawTrigger::PerFlight,
                "Engine flaws should all be PerFlight");
        }
    }
}
