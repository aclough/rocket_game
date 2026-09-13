use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::{Serialize, Deserialize};

use crate::contract::{CampaignId, ContractId, MarketId};
use crate::technology::TechnologyId;

/// Every question the game asks its world seed, spelled once. `Display`
/// renders the key that is hashed with the seed; a test pins the
/// rendered strings, because changing one re-rolls that fact in every
/// existing save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldQuery<'a> {
    /// The deficiencies a technology starts with.
    TechDeficiencies(TechnologyId),
    /// Whether a technology unlocks in a given year.
    TechUnlock(TechnologyId, u32),
    /// The n-th economic event; 0 is the opening state.
    EconomyEvent(u32),
    /// Whether event 1 is the dot-com crash.
    EconomyDotCom,
    /// The geopolitical shift rolled at a New Year.
    Geopolitics(u32),
    /// One market's contracts for one month.
    MonthlyContracts { year: u32, month: u32, market: MarketId },
    /// The campaigns announced in one month.
    MonthlyCampaigns { year: u32, month: u32 },
    /// The n-th mission a campaign issues (1-based).
    CampaignIssue(CampaignId, u32),
    /// Whether DinoSoar's launch of a contract fails.
    DinoLaunch(ContractId),
    /// DinoSoar's realized reliability.
    DinoSoar,
    /// DinoSoar's bid jitter on a solicitation.
    DinoBid(ContractId),
    /// DinoSoar's bid jitter on a campaign block bid.
    DinoBlockBid(CampaignId),
    /// A third-party engine's flaws, by engine name.
    ThirdPartyFlaws(&'a str),
    /// A market archetype's presence and shape, by its key.
    MarketArchetype(&'a str),
    /// An ad-hoc question, for tests only.
    Test(&'a str),
}

impl std::fmt::Display for WorldQuery<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorldQuery::TechDeficiencies(id) => write!(f, "tech_{}_deficiencies", id.0),
            WorldQuery::TechUnlock(id, year) => write!(f, "tech_unlock_{}_{}", id.0, year),
            WorldQuery::EconomyEvent(n) => write!(f, "economy_event_{}", n),
            WorldQuery::EconomyDotCom => write!(f, "economy_dot_com"),
            WorldQuery::Geopolitics(year) => write!(f, "geopolitics_{}", year),
            WorldQuery::MonthlyContracts { year, month, market } => {
                write!(f, "contracts_{}_{}_{}", year, month, market.0)
            }
            WorldQuery::MonthlyCampaigns { year, month } => write!(f, "campaigns_{}_{}", year, month),
            WorldQuery::CampaignIssue(id, n) => write!(f, "campaign_issue_{}_{}", id.0, n),
            WorldQuery::DinoLaunch(id) => write!(f, "dino_launch_{}", id.0),
            WorldQuery::DinoSoar => write!(f, "competitor_dinosoar"),
            WorldQuery::DinoBid(id) => write!(f, "dino_bid_{}", id.0),
            WorldQuery::DinoBlockBid(id) => write!(f, "dino_block_bid_{}", id.0),
            WorldQuery::ThirdPartyFlaws(name) => write!(f, "3p_flaws_{}", name),
            WorldQuery::MarketArchetype(key) => write!(f, "{}", key),
            WorldQuery::Test(q) => write!(f, "{}", q),
        }
    }
}

/// Game seed providing deterministic (world) and non-deterministic (contingent) randomness.
///
/// World queries use hash-keyed derivation: the question string is hashed with the seed
/// to produce a per-question sub-RNG. This means the same question always gives the same
/// answer regardless of query order — save-scum-proof and order-independent.
///
/// The contingent RNG is for in-game randomness that doesn't need to be reproducible
/// across query orders (flaw rolls, explosion checks, etc.).
///
/// Serialization: only the seed value is persisted, as `{"seed": N}`
/// (`SeedRepr`), and loading goes through `GameSeed::new`, so a loaded
/// seed's contingent RNG is the one a fresh game with that seed starts
/// with. Contingent randomness is non-deterministic across play by
/// design, so a reload restarting the stream is fine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "SeedRepr", into = "SeedRepr")]
pub struct GameSeed {
    seed: u64,
    pub contingent_rng: StdRng,
}

/// The wire form of a `GameSeed`.
#[derive(Serialize, Deserialize)]
struct SeedRepr {
    seed: u64,
}

impl From<SeedRepr> for GameSeed {
    fn from(repr: SeedRepr) -> Self {
        GameSeed::new(repr.seed)
    }
}

impl From<GameSeed> for SeedRepr {
    fn from(seed: GameSeed) -> Self {
        SeedRepr { seed: seed.seed }
    }
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

    /// Get a deterministic RNG for a specific world question.
    ///
    /// The same question string always produces the same RNG (and thus the same
    /// sequence of random values), regardless of what order questions are asked in.
    ///
    /// ```
    /// use rocket_tycoon::seed::{GameSeed, WorldQuery};
    /// use rand::Rng;
    ///
    /// let seed = GameSeed::new(42);
    /// let mut rng1 = seed.world_query(WorldQuery::Test("lunar_water_abundance"));
    /// let val1: f64 = rng1.gen();
    ///
    /// // Same query, same seed → same answer
    /// let mut rng2 = seed.world_query(WorldQuery::Test("lunar_water_abundance"));
    /// let val2: f64 = rng2.gen();
    /// assert_eq!(val1, val2);
    /// ```
    pub fn world_query(&self, question: WorldQuery<'_>) -> StdRng {
        let sub_seed = self.derive_seed(&question.to_string());
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

    /// The keys are the save's vocabulary: renaming one re-rolls that
    /// fact in every existing world, so each rendered string is pinned.
    #[test]
    fn every_query_renders_its_pinned_key() {
        let cases: Vec<(WorldQuery, &str)> = vec![
            (WorldQuery::TechDeficiencies(TechnologyId(3)), "tech_3_deficiencies"),
            (WorldQuery::TechUnlock(TechnologyId(3), 2004), "tech_unlock_3_2004"),
            (WorldQuery::EconomyEvent(0), "economy_event_0"),
            (WorldQuery::EconomyDotCom, "economy_dot_com"),
            (WorldQuery::Geopolitics(2001), "geopolitics_2001"),
            (WorldQuery::MonthlyContracts { year: 2002, month: 7, market: MarketId(4) }, "contracts_2002_7_4"),
            (WorldQuery::MonthlyCampaigns { year: 2002, month: 7 }, "campaigns_2002_7"),
            (WorldQuery::CampaignIssue(CampaignId(9), 2), "campaign_issue_9_2"),
            (WorldQuery::DinoLaunch(ContractId(11)), "dino_launch_11"),
            (WorldQuery::DinoSoar, "competitor_dinosoar"),
            (WorldQuery::DinoBid(ContractId(11)), "dino_bid_11"),
            (WorldQuery::DinoBlockBid(CampaignId(9)), "dino_block_bid_9"),
            (WorldQuery::ThirdPartyFlaws("RD-33K"), "3p_flaws_RD-33K"),
            (WorldQuery::MarketArchetype("market_cots"), "market_cots"),
            (WorldQuery::Test("anything"), "anything"),
        ];
        for (query, key) in cases {
            assert_eq!(query.to_string(), key, "{query:?}");
        }
    }

    /// The wire form is the bare seed, and a loaded seed's contingent
    /// stream is a fresh game's, not a placeholder.
    #[test]
    fn a_seed_round_trips_with_a_live_contingent_rng() {
        let json = serde_json::to_string(&GameSeed::new(7)).unwrap();
        assert_eq!(json, r#"{"seed":7}"#);
        let mut loaded: GameSeed = serde_json::from_str(&json).unwrap();
        let mut fresh = GameSeed::new(7);
        assert_eq!(loaded.seed(), 7);
        assert_eq!(loaded.contingent_rng.gen::<u64>(), fresh.contingent_rng.gen::<u64>());
    }

    #[test]
    fn test_same_question_same_answer() {
        let seed = GameSeed::new(12345);
        let mut rng1 = seed.world_query(WorldQuery::Test("tech_fusion_difficulty"));
        let mut rng2 = seed.world_query(WorldQuery::Test("tech_fusion_difficulty"));
        let v1: f64 = rng1.gen();
        let v2: f64 = rng2.gen();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_different_questions_different_answers() {
        let seed = GameSeed::new(12345);
        let mut rng1 = seed.world_query(WorldQuery::Test("tech_fusion_difficulty"));
        let mut rng2 = seed.world_query(WorldQuery::Test("lunar_water_abundance"));
        let v1: f64 = rng1.gen();
        let v2: f64 = rng2.gen();
        // Extremely unlikely to be equal
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_order_independence() {
        let seed = GameSeed::new(42);

        // Query A then B
        let mut rng_a1 = seed.world_query(WorldQuery::Test("question_a"));
        let a1: f64 = rng_a1.gen();
        let mut rng_b1 = seed.world_query(WorldQuery::Test("question_b"));
        let b1: f64 = rng_b1.gen();

        // Query B then A
        let mut rng_b2 = seed.world_query(WorldQuery::Test("question_b"));
        let b2: f64 = rng_b2.gen();
        let mut rng_a2 = seed.world_query(WorldQuery::Test("question_a"));
        let a2: f64 = rng_a2.gen();

        assert_eq!(a1, a2);
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_different_seeds_different_worlds() {
        let seed1 = GameSeed::new(100);
        let seed2 = GameSeed::new(200);
        let mut rng1 = seed1.world_query(WorldQuery::Test("lunar_water"));
        let mut rng2 = seed2.world_query(WorldQuery::Test("lunar_water"));
        let v1: f64 = rng1.gen();
        let v2: f64 = rng2.gen();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_contingent_rng_differs_from_world() {
        let mut seed = GameSeed::new(42);
        let v_contingent: f64 = seed.contingent_rng.gen();
        let mut world_rng = seed.world_query(WorldQuery::Test("some_question"));
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
        let mut rng = seed.world_query(WorldQuery::Test("test_variety"));
        let values: Vec<f64> = (0..100).map(|_| rng.gen::<f64>()).collect();
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(min < 0.2, "Should have some low values, min={}", min);
        assert!(max > 0.8, "Should have some high values, max={}", max);
    }
}
