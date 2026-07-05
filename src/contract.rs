use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::balance_config::MarketsConfig;
use crate::calendar::GameDate;

/// Unique identifier for a contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContractId(pub u64);

/// Unique identifier for a market.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct MarketId(pub u64);

/// Status of a contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractStatus {
    Available,
    Accepted,
    Completed,
    Failed { reason: String },
    Expired,
}

/// A contract to deliver a payload to a destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub id: ContractId,
    pub name: String,
    pub destination: String,
    pub payload_kg: f64,
    pub payment: f64,
    pub deadline: GameDate,
    pub status: ContractStatus,
    #[serde(default)]
    pub market_id: MarketId,
}

/// How sensitive a market is to economic cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EconomySensitivity {
    /// Unaffected (government/military).
    None,
    /// Slightly affected.
    Low,
    /// Directly tracks economy.
    Moderate,
    /// Amplified swings.
    High,
}

impl EconomySensitivity {
    /// Apply economy modifier with appropriate sensitivity.
    pub fn apply(&self, economy_modifier: f64) -> f64 {
        match self {
            EconomySensitivity::None => 1.0,
            EconomySensitivity::Low => 1.0 + (economy_modifier - 1.0) * 0.3,
            EconomySensitivity::Moderate => economy_modifier,
            EconomySensitivity::High => 1.0 + (economy_modifier - 1.0) * 1.5,
        }
    }
}

/// A destination within a market that contracts can target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketDestination {
    pub location_id: String,
    pub display_name: String,
    pub min_payload_kg: f64,
    pub max_payload_kg: f64,
    pub rate_per_kg: f64,
    /// Relative weight for random selection among destinations in this market.
    pub weight: f64,
}

/// An active modifier on a market (from events, competition, etc.).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketModifier {
    /// Unique key — checked for duplicates when adding.
    pub id: String,
    /// Human-readable description shown in market info.
    pub description: String,
    /// Multiplier to base volume (1.0 = no change).
    pub volume_mult: f64,
    /// Multiplier to payment rates (1.0 = no change).
    pub rate_mult: f64,
    /// When this modifier expires (None = permanent).
    pub end_date: Option<GameDate>,
}

/// A launch market that generates contracts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Market {
    pub id: MarketId,
    pub name: String,
    pub description: String,
    pub active: bool,
    /// Contracts per month before modifiers.
    pub base_volume: f64,
    pub destinations: Vec<MarketDestination>,
    pub min_reputation: f64,
    pub economy_sensitivity: EconomySensitivity,
    pub name_prefixes: Vec<String>,
    pub modifiers: Vec<MarketModifier>,
}

impl Market {
    /// Effective volume after all modifiers.
    pub fn effective_volume(&self, economy_modifier: f64) -> f64 {
        let econ = self.economy_sensitivity.apply(economy_modifier);
        let mod_mult: f64 = self.modifiers.iter().map(|m| m.volume_mult).product();
        self.base_volume * mod_mult * econ
    }

    /// Effective rate multiplier from all modifiers.
    pub fn rate_multiplier(&self, economy_modifier: f64) -> f64 {
        let econ = self.economy_sensitivity.apply(economy_modifier);
        let mod_mult: f64 = self.modifiers.iter().map(|m| m.rate_mult).product();
        mod_mult * econ
    }

    /// Add a modifier, checking for duplicates by id.
    pub fn add_modifier(&mut self, modifier: MarketModifier) {
        if !self.modifiers.iter().any(|m| m.id == modifier.id) {
            self.modifiers.push(modifier);
        }
    }

    /// Remove expired modifiers.
    pub fn expire_modifiers(&mut self, current_date: GameDate) {
        self.modifiers.retain(|m| {
            m.end_date.map_or(true, |end| current_date < end)
        });
    }
}

/// Generate contracts for a single market for one month.
pub fn generate_market_contracts(
    market: &Market,
    rng: &mut StdRng,
    next_contract_id: &mut u64,
    current_date: GameDate,
    reputation: f64,
    economy_modifier: f64,
    markets_cfg: &MarketsConfig,
) -> Vec<Contract> {
    if !market.active || reputation < market.min_reputation {
        return Vec::new();
    }

    let effective_volume = market.effective_volume(economy_modifier);
    let count = (effective_volume + rng.gen::<f64>()) as u32;
    let rate_mult = market.rate_multiplier(economy_modifier);

    let mut contracts = Vec::new();
    for _ in 0..count {
        if let Some(c) = generate_single_contract(
            market, rng, next_contract_id, current_date, rate_mult, markets_cfg,
        ) {
            contracts.push(c);
        }
    }
    contracts
}

fn generate_single_contract(
    market: &Market,
    rng: &mut StdRng,
    next_contract_id: &mut u64,
    current_date: GameDate,
    rate_mult: f64,
    markets_cfg: &MarketsConfig,
) -> Option<Contract> {
    if market.destinations.is_empty() || market.name_prefixes.is_empty() {
        return None;
    }

    // Pick destination by weight
    let total_weight: f64 = market.destinations.iter().map(|d| d.weight).sum();
    if total_weight <= 0.0 {
        return None;
    }
    let mut roll = rng.gen::<f64>() * total_weight;
    let mut dest = &market.destinations[0];
    for d in &market.destinations {
        roll -= d.weight;
        if roll <= 0.0 {
            dest = d;
            break;
        }
    }

    let payload_kg = rng.gen_range(dest.min_payload_kg..=dest.max_payload_kg);
    let payload_kg = (payload_kg / 100.0).round() * 100.0;
    let payload_kg = payload_kg.max(dest.min_payload_kg);

    let base_payment = payload_kg * dest.rate_per_kg;
    let variance = rng.gen_range(markets_cfg.payment_variance_min..=markets_cfg.payment_variance_max);
    let payment = (base_payment * variance * rate_mult / 10_000.0).round() * 10_000.0;

    let deadline_days = rng.gen_range(markets_cfg.deadline_min_days..=markets_cfg.deadline_max_days);
    let deadline = current_date.add_days(deadline_days);

    let prefix = &market.name_prefixes[rng.gen_range(0..market.name_prefixes.len())];
    let name = format!("{} to {}", prefix, dest.display_name);

    let id = ContractId(*next_contract_id);
    *next_contract_id += 1;

    Some(Contract {
        id,
        name,
        destination: dest.location_id.clone(),
        payload_kg,
        payment,
        deadline,
        status: ContractStatus::Available,
        market_id: market.id,
    })
}

/// Get the display name for a location ID.
pub fn destination_display_name(location_id: &str) -> &str {
    crate::location::DELTA_V_MAP.location(location_id)
        .map(|l| l.display_name)
        .unwrap_or(location_id)
}

// ==========================================
// Market templates
// ==========================================

pub const MARKET_GEO_COMSATS: MarketId = MarketId(1);
pub const MARKET_GOV_SCIENCE: MarketId = MarketId(2);
pub const MARKET_RIDESHARE: MarketId = MarketId(3);
pub const MARKET_COTS: MarketId = MarketId(4);
pub const MARKET_LEO_CONSTELLATION: MarketId = MarketId(5);
pub const MARKET_MEO_CONSTELLATION: MarketId = MarketId(6);
pub const MARKET_NSSL: MarketId = MarketId(7);
pub const MARKET_EARTH_OBS: MarketId = MarketId(8);

/// Create the markets that are active at game start.
pub fn initial_markets() -> Vec<Market> {
    vec![
        Market {
            id: MARKET_GEO_COMSATS,
            name: "GEO Communications".into(),
            description: "Commercial geostationary communications satellites".into(),
            active: true,
            base_volume: 1.5,
            destinations: vec![
                MarketDestination {
                    location_id: "gto".into(), display_name: "GTO".into(),
                    min_payload_kg: 2_000.0, max_payload_kg: 7_000.0,
                    rate_per_kg: 40_000.0, weight: 0.6,
                },
                MarketDestination {
                    location_id: "geo".into(), display_name: "GEO".into(),
                    min_payload_kg: 2_000.0, max_payload_kg: 5_000.0,
                    rate_per_kg: 80_000.0, weight: 0.4,
                },
            ],
            min_reputation: 50.0,
            economy_sensitivity: EconomySensitivity::Moderate,
            name_prefixes: vec!["ComSat".into(), "BroadcastSat".into(), "RelaySat".into()],
            modifiers: Vec::new(),
        },
        Market {
            id: MARKET_GOV_SCIENCE,
            name: "Government Science".into(),
            description: "NASA, ESA and other agency science missions".into(),
            active: true,
            base_volume: 0.3,
            destinations: vec![
                MarketDestination {
                    location_id: "leo".into(), display_name: "LEO".into(),
                    min_payload_kg: 500.0, max_payload_kg: 5_000.0,
                    rate_per_kg: 50_000.0, weight: 0.3,
                },
                MarketDestination {
                    location_id: "sso".into(), display_name: "SSO".into(),
                    min_payload_kg: 500.0, max_payload_kg: 3_000.0,
                    rate_per_kg: 60_000.0, weight: 0.3,
                },
                MarketDestination {
                    location_id: "l1".into(), display_name: "L1".into(),
                    min_payload_kg: 200.0, max_payload_kg: 3_000.0,
                    rate_per_kg: 80_000.0, weight: 0.15,
                },
                MarketDestination {
                    location_id: "l2".into(), display_name: "L2".into(),
                    min_payload_kg: 200.0, max_payload_kg: 3_000.0,
                    rate_per_kg: 80_000.0, weight: 0.15,
                },
                MarketDestination {
                    location_id: "lunar_orbit".into(), display_name: "Lunar Orbit".into(),
                    min_payload_kg: 200.0, max_payload_kg: 2_000.0,
                    rate_per_kg: 120_000.0, weight: 0.1,
                },
            ],
            min_reputation: 40.0,
            economy_sensitivity: EconomySensitivity::Low,
            name_prefixes: vec!["Observatory".into(), "SciSat".into(), "Probe".into(), "WeatherSat".into()],
            modifiers: Vec::new(),
        },
        Market {
            id: MARKET_RIDESHARE,
            name: "Rideshare / Smallsat".into(),
            description: "Universities, startups, and small agencies launching CubeSats and microsats".into(),
            active: true,
            base_volume: 0.5,
            destinations: vec![
                MarketDestination {
                    location_id: "leo".into(), display_name: "LEO".into(),
                    min_payload_kg: 50.0, max_payload_kg: 500.0,
                    rate_per_kg: 15_000.0, weight: 0.6,
                },
                MarketDestination {
                    location_id: "sso".into(), display_name: "SSO".into(),
                    min_payload_kg: 50.0, max_payload_kg: 300.0,
                    rate_per_kg: 30_000.0, weight: 0.4,
                },
            ],
            min_reputation: 0.0,
            economy_sensitivity: EconomySensitivity::Moderate,
            name_prefixes: vec!["CubeSat Bundle".into(), "University Payload".into(), "TechDemo".into()],
            modifiers: Vec::new(),
        },
    ]
}

/// Market templates for event-opened markets (created inactive).
pub fn event_market_templates() -> Vec<Market> {
    vec![
        Market {
            id: MARKET_COTS,
            name: "NASA Crew & Cargo".into(),
            description: "ISS resupply and crew rotation under commercial contract".into(),
            active: false,
            base_volume: 0.5,
            destinations: vec![
                MarketDestination {
                    location_id: "leo".into(), display_name: "LEO".into(),
                    min_payload_kg: 2_000.0, max_payload_kg: 6_000.0,
                    rate_per_kg: 40_000.0, weight: 1.0,
                },
            ],
            min_reputation: 60.0,
            economy_sensitivity: EconomySensitivity::Low,
            name_prefixes: vec!["ISS Resupply".into(), "Station Cargo".into(), "Crew Rotation".into()],
            modifiers: Vec::new(),
        },
        Market {
            id: MARKET_LEO_CONSTELLATION,
            name: "LEO Constellation".into(),
            description: "Broadband internet constellation deployment".into(),
            active: false,
            base_volume: 1.0,
            destinations: vec![
                MarketDestination {
                    location_id: "leo".into(), display_name: "LEO".into(),
                    min_payload_kg: 500.0, max_payload_kg: 5_000.0,
                    rate_per_kg: 15_000.0, weight: 0.6,
                },
                MarketDestination {
                    location_id: "sso".into(), display_name: "SSO".into(),
                    min_payload_kg: 500.0, max_payload_kg: 3_000.0,
                    rate_per_kg: 20_000.0, weight: 0.4,
                },
            ],
            min_reputation: 20.0,
            economy_sensitivity: EconomySensitivity::High,
            name_prefixes: vec!["Constellation Batch".into(), "LEO Deploy".into(), "Network Sat".into()],
            modifiers: Vec::new(),
        },
        Market {
            id: MARKET_MEO_CONSTELLATION,
            name: "MEO Constellation".into(),
            description: "Navigation and communications constellation in medium Earth orbit".into(),
            active: false,
            base_volume: 0.7,
            destinations: vec![
                MarketDestination {
                    location_id: "meo".into(), display_name: "MEO".into(),
                    min_payload_kg: 500.0, max_payload_kg: 3_000.0,
                    rate_per_kg: 25_000.0, weight: 1.0,
                },
            ],
            min_reputation: 30.0,
            economy_sensitivity: EconomySensitivity::High,
            name_prefixes: vec!["NavSat Batch".into(), "MEO Deploy".into(), "Constellation Unit".into()],
            modifiers: Vec::new(),
        },
        Market {
            id: MARKET_NSSL,
            name: "National Security".into(),
            description: "Defense and intelligence satellite launches".into(),
            active: false,
            base_volume: 0.3,
            destinations: vec![
                MarketDestination {
                    location_id: "leo".into(), display_name: "LEO".into(),
                    min_payload_kg: 1_000.0, max_payload_kg: 10_000.0,
                    rate_per_kg: 60_000.0, weight: 0.3,
                },
                MarketDestination {
                    location_id: "gto".into(), display_name: "GTO".into(),
                    min_payload_kg: 2_000.0, max_payload_kg: 7_000.0,
                    rate_per_kg: 80_000.0, weight: 0.25,
                },
                MarketDestination {
                    location_id: "geo".into(), display_name: "GEO".into(),
                    min_payload_kg: 2_000.0, max_payload_kg: 5_000.0,
                    rate_per_kg: 150_000.0, weight: 0.2,
                },
                MarketDestination {
                    location_id: "sso".into(), display_name: "SSO".into(),
                    min_payload_kg: 1_000.0, max_payload_kg: 5_000.0,
                    rate_per_kg: 70_000.0, weight: 0.25,
                },
            ],
            min_reputation: 80.0,
            economy_sensitivity: EconomySensitivity::None,
            name_prefixes: vec!["NatSec Payload".into(), "Defense Sat".into(), "Classified Mission".into()],
            modifiers: Vec::new(),
        },
        Market {
            id: MARKET_EARTH_OBS,
            name: "Earth Observation".into(),
            description: "Imaging, radar, and environmental monitoring satellites".into(),
            active: false,
            base_volume: 0.5,
            destinations: vec![
                MarketDestination {
                    location_id: "leo".into(), display_name: "LEO".into(),
                    min_payload_kg: 100.0, max_payload_kg: 1_000.0,
                    rate_per_kg: 25_000.0, weight: 0.4,
                },
                MarketDestination {
                    location_id: "sso".into(), display_name: "SSO".into(),
                    min_payload_kg: 100.0, max_payload_kg: 800.0,
                    rate_per_kg: 35_000.0, weight: 0.6,
                },
            ],
            min_reputation: 10.0,
            economy_sensitivity: EconomySensitivity::Moderate,
            name_prefixes: vec!["ImagingSat".into(), "RadarSat".into(), "EarthWatch".into()],
            modifiers: Vec::new(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn make_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    fn mcfg() -> MarketsConfig {
        MarketsConfig::default()
    }

    #[test]
    fn test_initial_markets_count() {
        let markets = initial_markets();
        assert_eq!(markets.len(), 3);
        assert!(markets.iter().all(|m| m.active));
    }

    #[test]
    fn test_event_markets_inactive() {
        let markets = event_market_templates();
        assert!(markets.iter().all(|m| !m.active));
    }

    #[test]
    fn test_generate_contracts_respects_reputation() {
        let markets = initial_markets();
        let mut rng = make_rng();
        let date = GameDate::new(2001, 1, 1);
        let mut next_id = 1u64;

        // Rideshare (rep 0) should work, GEO (rep 50) should not
        let rideshare = markets.iter().find(|m| m.id == MARKET_RIDESHARE).unwrap();
        let geo = markets.iter().find(|m| m.id == MARKET_GEO_COMSATS).unwrap();

        let cs = generate_market_contracts(rideshare, &mut rng, &mut next_id, date, 0.0, 1.0, &mcfg());
        // May or may not generate (volume 0.5 + random), but shouldn't error
        assert!(cs.iter().all(|c| c.market_id == MARKET_RIDESHARE));

        let cs = generate_market_contracts(geo, &mut rng, &mut next_id, date, 10.0, 1.0, &mcfg());
        assert!(cs.is_empty(), "GEO should require rep 50");
    }

    #[test]
    fn test_economy_sensitivity() {
        assert_eq!(EconomySensitivity::None.apply(0.5), 1.0);
        assert_eq!(EconomySensitivity::None.apply(1.5), 1.0);

        let low = EconomySensitivity::Low.apply(0.5);
        assert!(low > 0.8 && low < 0.9, "Low sensitivity at 0.5x should be ~0.85, got {}", low);

        assert_eq!(EconomySensitivity::Moderate.apply(0.5), 0.5);
        assert_eq!(EconomySensitivity::Moderate.apply(1.5), 1.5);

        let high = EconomySensitivity::High.apply(0.5);
        assert!(high < 0.3, "High sensitivity at 0.5x should be ~0.25, got {}", high);
    }

    #[test]
    fn test_market_modifier_dedup() {
        let mut market = initial_markets().remove(0);
        market.add_modifier(MarketModifier {
            id: "test".into(), description: "Test".into(),
            volume_mult: 0.5, rate_mult: 1.0, end_date: None,
        });
        market.add_modifier(MarketModifier {
            id: "test".into(), description: "Test duplicate".into(),
            volume_mult: 0.3, rate_mult: 1.0, end_date: None,
        });
        assert_eq!(market.modifiers.len(), 1, "Should deduplicate by id");
    }

    #[test]
    fn test_modifier_affects_volume() {
        let mut market = initial_markets().remove(0); // GEO, base_volume 1.5
        let vol_before = market.effective_volume(1.0);
        market.add_modifier(MarketModifier {
            id: "test".into(), description: "Test".into(),
            volume_mult: 0.5, rate_mult: 1.0, end_date: None,
        });
        let vol_after = market.effective_volume(1.0);
        assert!((vol_after - vol_before * 0.5).abs() < 0.01);
    }

    #[test]
    fn test_expire_modifiers() {
        let mut market = initial_markets().remove(0);
        market.add_modifier(MarketModifier {
            id: "temp".into(), description: "Temp".into(),
            volume_mult: 0.5, rate_mult: 1.0,
            end_date: Some(GameDate::new(2005, 1, 1)),
        });
        market.add_modifier(MarketModifier {
            id: "perm".into(), description: "Perm".into(),
            volume_mult: 0.8, rate_mult: 1.0, end_date: None,
        });
        market.expire_modifiers(GameDate::new(2006, 1, 1));
        assert_eq!(market.modifiers.len(), 1);
        assert_eq!(market.modifiers[0].id, "perm");
    }

    #[test]
    fn test_contract_has_market_id() {
        let market = &initial_markets()[2]; // Rideshare
        let mut rng = make_rng();
        let mut next_id = 1u64;
        let cs = generate_market_contracts(market, &mut rng, &mut next_id, GameDate::new(2001, 1, 1), 0.0, 1.0, &mcfg());
        for c in &cs {
            assert_eq!(c.market_id, MARKET_RIDESHARE);
        }
    }

    #[test]
    fn test_inactive_market_generates_nothing() {
        let market = &event_market_templates()[0]; // COTS, inactive
        let mut rng = make_rng();
        let mut next_id = 1u64;
        let cs = generate_market_contracts(market, &mut rng, &mut next_id, GameDate::new(2001, 1, 1), 200.0, 1.0, &mcfg());
        assert!(cs.is_empty());
    }
}
