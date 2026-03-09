use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

/// Unique identifier for a flaw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlawId(pub u64);

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
    /// Chance per flight that this flaw triggers.
    pub activation_chance: f64,
    /// Chance per testing cycle to discover this flaw.
    /// Computed as uniform(0,1) * sqrt(activation_chance).
    pub discovery_probability: f64,
    pub discovered: bool,
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
    let mean = effective_complexity as f64;
    let stddev = 1.5;

    // Box-Muller transform for gaussian
    let count_f = gaussian_sample(mean, stddev, rng);
    let count = count_f.round().max(0.0) as u32;

    (0..count).map(|_| {
        let id = FlawId(*next_flaw_id);
        *next_flaw_id += 1;
        generate_single_flaw(id, rng)
    }).collect()
}

fn generate_single_flaw(id: FlawId, rng: &mut StdRng) -> Flaw {
    // Pick consequence type: weighted random
    // ~50% performance degradation, ~35% engine loss, ~15% stage loss
    let roll: f64 = rng.gen();
    let consequence = if roll < 0.50 {
        // Performance degradation: 3-15% loss
        let degradation = rng.gen_range(0.03..0.15);
        FlawConsequence::PerformanceDegradation(degradation)
    } else if roll < 0.85 {
        FlawConsequence::EngineLoss
    } else {
        FlawConsequence::StageLoss
    };

    // Activation chance: random^2, skewed toward low values (mean ~0.33)
    let activation_chance: f64 = rng.gen::<f64>().powi(2);

    // Discovery probability = uniform(0,1) * sqrt(activation_chance)
    let uniform_roll: f64 = rng.gen();
    let discovery_probability = uniform_roll * activation_chance.sqrt();

    let description = generate_flaw_description(&consequence, rng);

    Flaw {
        id,
        description,
        consequence,
        activation_chance,
        discovery_probability,
        discovered: false,
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
}
