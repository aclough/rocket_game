use rand::Rng;
use serde::{Serialize, Deserialize};

use crate::seed::GameSeed;

/// Unique identifier for a technology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TechnologyId(pub u64);

/// Unique identifier for a tech deficiency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TechDeficiencyId(pub u64);

/// What a tech deficiency does to an engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TechDeficiencyKind {
    /// Isp reduced by this fraction (e.g. 0.10 = -10%).
    IspPenalty(f64),
    /// Mass increased by this fraction (e.g. 0.15 = +15%).
    MassPenalty(f64),
    /// Thrust reduced by this fraction.
    ThrustPenalty(f64),
    /// Adds to effective complexity (more flaws, harder testing).
    ComplexityPenalty(u32),
}

impl std::fmt::Display for TechDeficiencyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TechDeficiencyKind::IspPenalty(frac) => write!(f, "-{:.0}% Isp", frac * 100.0),
            TechDeficiencyKind::MassPenalty(frac) => write!(f, "+{:.0}% mass", frac * 100.0),
            TechDeficiencyKind::ThrustPenalty(frac) => write!(f, "-{:.0}% thrust", frac * 100.0),
            TechDeficiencyKind::ComplexityPenalty(n) => write!(f, "+{} complexity", n),
        }
    }
}

/// A deficiency inherent to a technology, determined by the game seed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechDeficiency {
    pub id: TechDeficiencyId,
    pub description: String,
    pub kind: TechDeficiencyKind,
    /// How solvable this deficiency is (0.0 = impossible, 1.0 = easy).
    pub solvability: f64,
    /// Whether any engine in the company has solved this.
    pub solved: bool,
    /// Number of failed revision attempts across all engines.
    pub total_attempts: u32,
}

/// An experimental technology that may have deficiencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Technology {
    pub id: TechnologyId,
    pub name: String,
    pub description: String,
    /// Whether the player can use this tech in engine designs.
    pub unlocked: bool,
    /// Difficulty level: 0 = low risk, 1 = moderate, 2 = high risk.
    pub difficulty: u32,
    /// Deficiencies inherent to this tech (seed-generated).
    pub deficiencies: Vec<TechDeficiency>,
}

impl Technology {
    /// Get unsolved deficiencies.
    pub fn unsolved_deficiencies(&self) -> Vec<&TechDeficiency> {
        self.deficiencies.iter().filter(|d| !d.solved).collect()
    }
}

// ── Well-known technology IDs ──

/// Look up the technology ID for a propellant preset, if it's experimental.
pub fn technology_for_preset(preset: crate::engine_project::PropellantPreset) -> Option<TechnologyId> {
    match preset {
        crate::engine_project::PropellantPreset::Methalox => Some(TECH_METHALOX),
        _ => None,
    }
}

pub const TECH_METHALOX: TechnologyId = TechnologyId(1);
pub const TECH_NUCLEAR_THERMAL: TechnologyId = TechnologyId(2);

/// Generate technologies for a new game.
pub fn generate_technologies(seed: &GameSeed) -> Vec<Technology> {
    vec![
        generate_technology(
            seed,
            TECH_METHALOX,
            "Methalox",
            "Liquid methane/LOX propulsion — promising but unproven in flight",
            true,  // unlocked at start
            0,     // difficulty 0 (low risk)
        ),
        generate_technology(
            seed,
            TECH_NUCLEAR_THERMAL,
            "Nuclear Thermal",
            "Nuclear reactor heating hydrogen propellant — very high Isp but experimental",
            false, // unlocked by event
            2,     // difficulty 2 (high risk)
        ),
    ]
}

fn generate_technology(
    seed: &GameSeed,
    id: TechnologyId,
    name: &str,
    description: &str,
    unlocked: bool,
    difficulty: u32,
) -> Technology {
    let query = format!("tech_{}_deficiencies", id.0);
    let mut rng = seed.world_query(&query);

    let deficiencies = generate_deficiencies(&mut rng, difficulty, id);

    Technology {
        id,
        name: name.to_string(),
        description: description.to_string(),
        unlocked,
        difficulty,
        deficiencies,
    }
}

fn generate_deficiencies(
    rng: &mut rand::rngs::StdRng,
    difficulty: u32,
    tech_id: TechnologyId,
) -> Vec<TechDeficiency> {
    // Difficulty 0: 0-2 deficiencies, solvability 0.0-1.0
    // Difficulty 1: 1-3 deficiencies, solvability max(-0.1..0.9, 0.0)
    // Difficulty 2: 2-4 deficiencies, solvability max(-0.2..0.8, 0.0)
    let (min_count, max_count) = match difficulty {
        0 => (0u32, 2u32),
        1 => (1, 3),
        _ => (2, 4),
    };
    let solvability_offset = -(difficulty as f64) * 0.1;
    let solvability_range = 1.0 - (difficulty as f64) * 0.1;

    let count = rng.gen_range(min_count..=max_count);
    let mut next_id = tech_id.0 * 100; // namespace deficiency IDs by tech

    (0..count).map(|_| {
        let id = TechDeficiencyId(next_id);
        next_id += 1;

        let raw_solvability = solvability_offset + rng.gen::<f64>() * solvability_range;
        let solvability = raw_solvability.max(0.0);

        // Pick deficiency kind and magnitude (higher difficulty = bigger penalties)
        let base_magnitude = 0.05 + difficulty as f64 * 0.05;
        let magnitude = rng.gen_range(base_magnitude..(base_magnitude + 0.15));

        let roll: f64 = rng.gen();
        let (kind, description) = if roll < 0.30 {
            (TechDeficiencyKind::IspPenalty(magnitude), pick_description(rng, &[
                "Incomplete combustion characteristics",
                "Non-optimal mixture ratio range",
                "Nozzle erosion from exhaust products",
            ]))
        } else if roll < 0.55 {
            (TechDeficiencyKind::MassPenalty(magnitude), pick_description(rng, &[
                "Heavier propellant handling systems required",
                "Additional thermal management mass",
                "Reinforced injector design needed",
            ]))
        } else if roll < 0.80 {
            (TechDeficiencyKind::ThrustPenalty(magnitude), pick_description(rng, &[
                "Chamber pressure limitations",
                "Propellant feed instabilities",
                "Reduced injector throughput",
            ]))
        } else {
            let complexity_add = rng.gen_range(1..=2 + difficulty);
            (TechDeficiencyKind::ComplexityPenalty(complexity_add), pick_description(rng, &[
                "Immature manufacturing processes",
                "Poorly understood failure modes",
                "Materials compatibility issues",
            ]))
        };

        TechDeficiency {
            id,
            description,
            kind,
            solvability,
            solved: false,
            total_attempts: 0,
        }
    }).collect()
}

fn pick_description(rng: &mut rand::rngs::StdRng, options: &[&str]) -> String {
    options[rng.gen_range(0..options.len())].to_string()
}

/// Attempt to solve a tech deficiency during revision.
/// Returns true if solved, false if failed.
pub fn attempt_solve(deficiency: &mut TechDeficiency, already_solved_elsewhere: bool, rng: &mut rand::rngs::StdRng) -> bool {
    deficiency.total_attempts += 1;

    let chance = if already_solved_elsewhere {
        (deficiency.solvability * 3.0).min(0.95)
    } else {
        deficiency.solvability
    };

    if rng.gen::<f64>() < chance {
        deficiency.solved = true;
        true
    } else {
        false
    }
}

/// Message about failed attempts (same regardless of solvability).
pub fn failure_hint(attempts: u32) -> Option<&'static str> {
    match attempts {
        3 => Some("Engineers have made multiple attempts without success"),
        5 => Some("Significant engineering effort invested with no breakthrough"),
        7 => Some("Extensive revision attempts have not resolved this issue"),
        10 => Some("Engineers recommend considering alternative approaches"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_methalox() {
        let seed = GameSeed::new(42);
        let techs = generate_technologies(&seed);
        let methalox = techs.iter().find(|t| t.id == TECH_METHALOX).unwrap();
        assert!(methalox.unlocked);
        assert_eq!(methalox.difficulty, 0);
        assert!(methalox.deficiencies.len() <= 2);
    }

    #[test]
    fn test_generate_nerva() {
        let seed = GameSeed::new(42);
        let techs = generate_technologies(&seed);
        let nerva = techs.iter().find(|t| t.id == TECH_NUCLEAR_THERMAL).unwrap();
        assert!(!nerva.unlocked);
        assert_eq!(nerva.difficulty, 2);
        assert!(nerva.deficiencies.len() >= 2);
    }

    #[test]
    fn test_solvability_clamped_to_zero() {
        for s in 0..100 {
            let seed = GameSeed::new(s);
            let techs = generate_technologies(&seed);
            for tech in &techs {
                for def in &tech.deficiencies {
                    assert!(def.solvability >= 0.0,
                        "Solvability should be >= 0, got {} in {:?}", def.solvability, tech.name);
                }
            }
        }
    }

    #[test]
    fn test_difficulty_2_has_impossible_sometimes() {
        let mut found_impossible = false;
        for s in 0..200 {
            let seed = GameSeed::new(s);
            let techs = generate_technologies(&seed);
            let nerva = techs.iter().find(|t| t.id == TECH_NUCLEAR_THERMAL).unwrap();
            if nerva.deficiencies.iter().any(|d| d.solvability == 0.0) {
                found_impossible = true;
                break;
            }
        }
        assert!(found_impossible, "Difficulty 2 should sometimes produce impossible deficiencies");
    }

    #[test]
    fn test_deterministic() {
        let seed = GameSeed::new(99);
        let t1 = generate_technologies(&seed);
        let t2 = generate_technologies(&seed);
        assert_eq!(t1.len(), t2.len());
        for (a, b) in t1.iter().zip(t2.iter()) {
            assert_eq!(a.deficiencies.len(), b.deficiencies.len());
            for (da, db) in a.deficiencies.iter().zip(b.deficiencies.iter()) {
                assert_eq!(da.solvability, db.solvability);
            }
        }
    }

    #[test]
    fn test_attempt_solve_unsolved() {
        use rand::SeedableRng;
        let seed = GameSeed::new(42);
        let techs = generate_technologies(&seed);
        // Find a deficiency with decent solvability
        let tech = &techs[0]; // methalox
        if let Some(def) = tech.deficiencies.iter().find(|d| d.solvability > 0.5) {
            let mut def = def.clone();
            let mut rng = rand::rngs::StdRng::seed_from_u64(1);
            // Try many times — should eventually solve
            let mut solved = false;
            for _ in 0..20 {
                if attempt_solve(&mut def, false, &mut rng) {
                    solved = true;
                    break;
                }
            }
            assert!(solved, "Should eventually solve a high-solvability deficiency");
        }
    }

    #[test]
    fn test_solved_elsewhere_boosts_chance() {
        use rand::SeedableRng;
        let def = TechDeficiency {
            id: TechDeficiencyId(1),
            description: "test".into(),
            kind: TechDeficiencyKind::IspPenalty(0.1),
            solvability: 0.3,
            solved: false,
            total_attempts: 0,
        };

        let mut successes_without = 0;
        let mut successes_with = 0;
        for s in 0..1000 {
            let mut rng = rand::rngs::StdRng::seed_from_u64(s);
            let mut d1 = def.clone();
            let mut d2 = def.clone();
            if attempt_solve(&mut d1, false, &mut rng) { successes_without += 1; }
            let mut rng2 = rand::rngs::StdRng::seed_from_u64(s);
            if attempt_solve(&mut d2, true, &mut rng2) { successes_with += 1; }
        }
        assert!(successes_with > successes_without * 2,
            "Solved elsewhere should greatly boost success: {} vs {}",
            successes_with, successes_without);
    }
}
