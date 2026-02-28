use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::{Serialize, Deserialize};

/// Game seed providing deterministic (world) and non-deterministic (contingent) randomness.
///
/// World queries use hash-keyed derivation: the question string is hashed with the seed
/// to produce a per-question sub-RNG. This means the same question always gives the same
/// answer regardless of query order — save-scum-proof and order-independent.
///
/// The contingent RNG is for in-game randomness that doesn't need to be reproducible
/// across query orders (flaw rolls, explosion checks, etc.).
///
/// Serialization: only the seed value is persisted. The contingent RNG is recreated on
/// load. This is fine since contingent randomness is non-deterministic by design.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSeed {
    seed: u64,
    #[serde(skip, default = "default_contingent_rng")]
    pub contingent_rng: StdRng,
}

fn default_contingent_rng() -> StdRng {
    // Placeholder — overridden by the Deserialize impl's post-processing
    StdRng::seed_from_u64(0)
}

impl GameSeed {
    /// Create a new game seed. The contingent RNG gets a derived but different seed.
    pub fn new(seed: u64) -> Self {
        let contingent_rng = StdRng::seed_from_u64(seed.wrapping_add(1));
        GameSeed { seed, contingent_rng }
    }

    /// The raw seed value (for display/save).
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Re-initialize the contingent RNG after deserialization.
    /// Called automatically by save/load; world queries are unaffected (hash-derived).
    pub fn fix_after_load(&mut self) {
        self.contingent_rng = StdRng::seed_from_u64(self.seed.wrapping_add(1));
    }

    /// Get a deterministic RNG for a specific world question.
    ///
    /// The same question string always produces the same RNG (and thus the same
    /// sequence of random values), regardless of what order questions are asked in.
    ///
    /// ```
    /// use rocket_tycoon::seed::GameSeed;
    /// use rand::Rng;
    ///
    /// let seed = GameSeed::new(42);
    /// let mut rng1 = seed.world_query("lunar_water_abundance");
    /// let val1: f64 = rng1.gen();
    ///
    /// // Same query, same seed → same answer
    /// let mut rng2 = seed.world_query("lunar_water_abundance");
    /// let val2: f64 = rng2.gen();
    /// assert_eq!(val1, val2);
    /// ```
    pub fn world_query(&self, question: &str) -> StdRng {
        let sub_seed = self.derive_seed(question);
        StdRng::seed_from_u64(sub_seed)
    }

    /// Derive a deterministic sub-seed from the world seed and a question string.
    fn derive_seed(&self, question: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.seed.hash(&mut hasher);
        question.hash(&mut hasher);
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn test_same_question_same_answer() {
        let seed = GameSeed::new(12345);
        let mut rng1 = seed.world_query("tech_fusion_difficulty");
        let mut rng2 = seed.world_query("tech_fusion_difficulty");
        let v1: f64 = rng1.gen();
        let v2: f64 = rng2.gen();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_different_questions_different_answers() {
        let seed = GameSeed::new(12345);
        let mut rng1 = seed.world_query("tech_fusion_difficulty");
        let mut rng2 = seed.world_query("lunar_water_abundance");
        let v1: f64 = rng1.gen();
        let v2: f64 = rng2.gen();
        // Extremely unlikely to be equal
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_order_independence() {
        let seed = GameSeed::new(42);

        // Query A then B
        let mut rng_a1 = seed.world_query("question_a");
        let a1: f64 = rng_a1.gen();
        let mut rng_b1 = seed.world_query("question_b");
        let b1: f64 = rng_b1.gen();

        // Query B then A
        let mut rng_b2 = seed.world_query("question_b");
        let b2: f64 = rng_b2.gen();
        let mut rng_a2 = seed.world_query("question_a");
        let a2: f64 = rng_a2.gen();

        assert_eq!(a1, a2);
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_different_seeds_different_worlds() {
        let seed1 = GameSeed::new(100);
        let seed2 = GameSeed::new(200);
        let mut rng1 = seed1.world_query("lunar_water");
        let mut rng2 = seed2.world_query("lunar_water");
        let v1: f64 = rng1.gen();
        let v2: f64 = rng2.gen();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_contingent_rng_differs_from_world() {
        let mut seed = GameSeed::new(42);
        let v_contingent: f64 = seed.contingent_rng.gen();
        let mut world_rng = seed.world_query("some_question");
        let v_world: f64 = world_rng.gen();
        assert_ne!(v_contingent, v_world);
    }

    #[test]
    fn test_seed_value_preserved() {
        let seed = GameSeed::new(99999);
        assert_eq!(seed.seed(), 99999);
    }

    #[test]
    fn test_world_query_produces_variety() {
        // A single query should produce a full range of values
        let seed = GameSeed::new(42);
        let mut rng = seed.world_query("test_variety");
        let values: Vec<f64> = (0..100).map(|_| rng.gen::<f64>()).collect();
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(min < 0.2, "Should have some low values, min={}", min);
        assert!(max > 0.8, "Should have some high values, max={}", max);
    }
}
