use rand::Rng;
use rand_chacha::ChaCha8Rng;
use rand::SeedableRng;

/// A deterministic seed for generating consistent world facts.
///
/// The world seed determines fixed truths about a playthrough —
/// how much water is on the Moon, whether space tourism is viable,
/// how hard a particular technology is, which narrative events exist.
///
/// Query it with a string key and you always get the same answer
/// for a given seed, regardless of player actions or save/load.
#[derive(Debug, Clone)]
pub struct WorldSeed {
    seed: u64,
}

impl WorldSeed {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// Create a WorldSeed with a random seed
    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        Self { seed: rng.gen() }
    }

    /// Get the raw seed value (for serialization)
    pub fn raw_seed(&self) -> u64 {
        self.seed
    }

    /// Query a world fact as a value in [0.0, 1.0).
    ///
    /// The same seed + question always produces the same value.
    ///
    /// # Example
    /// ```
    /// use rocket_tycoon::world_seed::WorldSeed;
    /// let seed = WorldSeed::new(12345);
    /// let water = seed.query("moon_south_pole_water");
    /// // Interpret: 0.0-0.3 = dry, 0.3-0.7 = moderate, 0.7-1.0 = abundant
    /// ```
    pub fn query(&self, question: &str) -> f64 {
        let mut rng = self.query_rng(question);
        rng.gen::<f64>()
    }

    /// Get a deterministic RNG stream for a topic that needs multiple values.
    ///
    /// Use this when a single f64 isn't enough — e.g., generating a full
    /// demand curve or a set of narrative event parameters.
    pub fn query_rng(&self, topic: &str) -> ChaCha8Rng {
        let question_hash = fnv1a(topic.as_bytes());
        let mut seed_bytes = [0u8; 32];
        seed_bytes[..8].copy_from_slice(&self.seed.to_le_bytes());
        seed_bytes[8..16].copy_from_slice(&question_hash.to_le_bytes());
        ChaCha8Rng::from_seed(seed_bytes)
    }
}

/// FNV-1a hash — simple, stable, and deterministic across platforms/versions.
/// We use this instead of std's DefaultHasher because DefaultHasher is not
/// guaranteed stable across Rust versions.
fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_query() {
        let seed = WorldSeed::new(42);
        let a = seed.query("moon_south_pole_water");
        let b = seed.query("moon_south_pole_water");
        assert_eq!(a, b);
    }

    #[test]
    fn test_different_questions_differ() {
        let seed = WorldSeed::new(42);
        let water = seed.query("moon_south_pole_water");
        let tourism = seed.query("space_tourism_demand");
        assert_ne!(water, tourism);
    }

    #[test]
    fn test_different_seeds_differ() {
        let a = WorldSeed::new(42);
        let b = WorldSeed::new(43);
        let qa = a.query("moon_south_pole_water");
        let qb = b.query("moon_south_pole_water");
        assert_ne!(qa, qb);
    }

    #[test]
    fn test_query_range() {
        let seed = WorldSeed::new(12345);
        // Check many queries are all in [0, 1)
        let questions = [
            "moon_water", "space_tourism", "nuclear_lightbulb_difficulty",
            "helium3_value", "solar_power_viability", "fullflow_difficulty",
        ];
        for q in &questions {
            let v = seed.query(q);
            assert!(v >= 0.0 && v < 1.0, "{q} produced {v}");
        }
    }

    #[test]
    fn test_query_rng_deterministic() {
        let seed = WorldSeed::new(99);
        let mut rng1 = seed.query_rng("event_parameters");
        let mut rng2 = seed.query_rng("event_parameters");
        let vals1: Vec<f64> = (0..10).map(|_| rng1.gen()).collect();
        let vals2: Vec<f64> = (0..10).map(|_| rng2.gen()).collect();
        assert_eq!(vals1, vals2);
    }

    #[test]
    fn test_raw_seed_roundtrip() {
        let seed = WorldSeed::new(777);
        let raw = seed.raw_seed();
        let restored = WorldSeed::new(raw);
        assert_eq!(
            seed.query("test_question"),
            restored.query("test_question")
        );
    }
}
