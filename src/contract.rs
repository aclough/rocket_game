use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::balance_config::MarketsConfig;
use crate::calendar::GameDate;
use crate::seed::GameSeed;

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
    /// Compounding annual volume growth rate (0.05 = +5%/year),
    /// drawn per seed at realization.
    #[serde(default)]
    pub annual_growth: f64,
    /// When this market became active; growth compounds from here.
    /// None until activation (and on pre-growth saves).
    #[serde(default)]
    pub activation_date: Option<GameDate>,
    /// Per-market contract deadline window in days from issue;
    /// None falls back to the global `MarketsConfig` window.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub deadline_days: Option<(u32, u32)>,
    /// Multiplier on reputation penalties for failures and expiries
    /// involving this market's contracts (1.0 = baseline; crewed
    /// markets are much less forgiving, government science more so).
    #[serde(default = "default_severity")]
    pub failure_severity: f64,
}

fn default_severity() -> f64 {
    1.0
}

impl Market {
    /// Compounding growth multiplier accumulated since activation
    /// (1.0 before activation or with zero growth).
    pub fn growth_factor(&self, current_date: GameDate) -> f64 {
        match self.activation_date {
            Some(activated) if current_date > activated => {
                let years = activated.days_until(&current_date) as f64 / 365.25;
                (1.0 + self.annual_growth).powf(years)
            }
            _ => 1.0,
        }
    }

    /// Effective volume after growth and all modifiers.
    pub fn effective_volume(&self, economy_modifier: f64, current_date: GameDate) -> f64 {
        let econ = self.economy_sensitivity.apply(economy_modifier);
        let mod_mult: f64 = self.modifiers.iter().map(|m| m.volume_mult).product();
        self.base_volume * self.growth_factor(current_date) * mod_mult * econ
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

    let effective_volume = market.effective_volume(economy_modifier, current_date);
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

    let (deadline_min, deadline_max) = market.deadline_days
        .unwrap_or((markets_cfg.deadline_min_days, markets_cfg.deadline_max_days));
    let deadline_days = rng.gen_range(deadline_min..=deadline_max);
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
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((90, 240)),
            failure_severity: 1.2,
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
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((120, 360)),
            failure_severity: 0.7,
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
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((60, 150)),
            failure_severity: 1.0,
        },
    ]
}

/// Market templates for event-opened markets (created inactive).
pub fn event_market_templates() -> Vec<Market> {
    vec![
        Market {
            id: MARKET_COTS,
            name: "NASA Crew & Cargo".into(),
            description: "ISS resupply and crew rotation under commercial contract. \
                          Crew-adjacent missions: failures end careers".into(),
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
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((90, 270)),
            failure_severity: 2.0,
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
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((60, 180)),
            failure_severity: 1.0,
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
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((90, 210)),
            failure_severity: 1.0,
        },
        Market {
            id: MARKET_NSSL,
            name: "National Security".into(),
            description: "Defense and intelligence satellite launches. \
                          Irreplaceable payloads; failures draw hearings".into(),
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
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((120, 360)),
            failure_severity: 1.5,
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
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((60, 180)),
            failure_severity: 1.0,
        },
    ]
}

// ==========================================
// Market archetypes: seed-perturbed realization (M2)
// ==========================================

/// How and when an event-driven market enters the world mid-game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmergenceSpec {
    /// Inclusive calendar-year window the trigger year is drawn from.
    pub year_range: (u32, u32),
    /// Event-log flavor text when the market opens.
    pub flavor: String,
    /// Modifiers applied to other markets when this one opens.
    pub cross_effects: Vec<CrossEffect>,
}

/// A modifier applied to another market when an emergence fires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossEffect {
    pub target: MarketId,
    pub modifier: MarketModifier,
}

/// A market template plus its per-seed perturbation spec, realized
/// into a concrete `Market` at game start via `world_query(key)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketArchetype {
    /// `world_query` question and `fired_market_events` key; must be
    /// unique across archetypes. Event-market keys predate the
    /// archetype layer and are kept verbatim so existing seeds keep
    /// the same market presence and timing.
    pub key: String,
    /// Chance this market exists at all in a given world (1.0 = always).
    pub presence_probability: f64,
    /// Per-seed multiplier on `base_volume`, drawn uniformly.
    pub volume_mult_range: (f64, f64),
    /// Per-seed multiplier on every destination's `rate_per_kg`.
    pub rate_mult_range: (f64, f64),
    /// Per-seed compounding annual volume growth rate, drawn
    /// uniformly (0.05 = +5%/year from activation).
    #[serde(default = "zero_range")]
    pub annual_growth_range: (f64, f64),
    /// Max fractional jitter on each destination weight (0 = none).
    pub weight_tilt_strength: f64,
    /// At most one present market per group; the earliest trigger
    /// year wins, config order breaks ties.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub exclusive_group: Option<String>,
    /// None = active from game start (when present).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub emergence: Option<EmergenceSpec>,
    pub template: Market,
}

fn zero_range() -> (f64, f64) {
    (0.0, 0.0)
}

/// One archetype realized for a specific world seed.
#[derive(Debug, Clone, PartialEq)]
pub struct RealizedMarket {
    /// Concrete market with all per-seed multipliers baked in — the
    /// only form the rest of the game (and the UI) ever sees.
    pub market: Market,
    /// False if the presence roll failed or an exclusive sibling won.
    pub present: bool,
    /// Year an emergence market opens (None if start-active or absent).
    pub trigger_year: Option<u32>,
}

/// Realize one archetype. The draw order is fixed and additions must
/// go at the end: the presence and trigger-year draws reuse the
/// pre-archetype emergence query stream, so a given seed keeps the
/// market presence and timing it had before this layer existed.
pub fn realize_archetype(seed: &GameSeed, arch: &MarketArchetype) -> RealizedMarket {
    let mut rng = seed.world_query(&arch.key);

    let present = rng.gen::<f64>() < arch.presence_probability;
    let trigger_year = arch.emergence.as_ref().map(|e| {
        rng.gen_range(e.year_range.0..=e.year_range.1)
    });
    let volume_mult = rng.gen_range(arch.volume_mult_range.0..=arch.volume_mult_range.1);
    let rate_mult = rng.gen_range(arch.rate_mult_range.0..=arch.rate_mult_range.1);

    let mut market = arch.template.clone();
    market.base_volume *= volume_mult;
    for dest in &mut market.destinations {
        dest.rate_per_kg *= rate_mult;
        if arch.weight_tilt_strength > 0.0 {
            let tilt = rng.gen_range(-arch.weight_tilt_strength..=arch.weight_tilt_strength);
            dest.weight *= 1.0 + tilt;
        }
    }
    market.annual_growth =
        rng.gen_range(arch.annual_growth_range.0..=arch.annual_growth_range.1);
    if !present {
        market.active = false;
    }
    RealizedMarket {
        market,
        present,
        trigger_year: if present { trigger_year } else { None },
    }
}

/// Realize the full archetype table for a world seed, resolving
/// exclusive groups. Returns one entry per archetype, in order.
pub fn realize_markets(seed: &GameSeed, archetypes: &[MarketArchetype]) -> Vec<RealizedMarket> {
    let mut realized: Vec<RealizedMarket> =
        archetypes.iter().map(|a| realize_archetype(seed, a)).collect();

    let mut groups: Vec<&str> = archetypes.iter()
        .filter_map(|a| a.exclusive_group.as_deref())
        .collect();
    groups.sort_unstable();
    groups.dedup();

    for group in groups {
        let members: Vec<usize> = archetypes.iter().enumerate()
            .filter(|(i, a)| {
                a.exclusive_group.as_deref() == Some(group) && realized[*i].present
            })
            .map(|(i, _)| i)
            .collect();
        // Earliest trigger year wins; start-active members count as
        // year 0; ties go to config order.
        let winner = members.iter()
            .min_by_key(|&&i| (realized[i].trigger_year.unwrap_or(0), i))
            .copied();
        if let Some(winner) = winner {
            for &i in &members {
                if i != winner {
                    realized[i].present = false;
                    realized[i].trigger_year = None;
                    realized[i].market.active = false;
                }
            }
        }
    }
    realized
}

/// The default archetype table: the eight pre-M2 markets plus the
/// emergence data that used to live in `GameState::check_market_events`.
/// The two mainstays (Rideshare, GEO Comsats) are pinned at exactly
/// (1.0, 1.0) — identical in every world; everything else varies per
/// seed. Perturbation ranges are first-guess values for M4 to tune.
pub fn default_archetypes() -> Vec<MarketArchetype> {
    let base = initial_markets();
    let event = event_market_templates();
    let by_id = |id: MarketId, from: &[Market]| -> Market {
        from.iter().find(|m| m.id == id).expect("template exists").clone()
    };
    let pinned = |key: &str, growth: (f64, f64), template: Market| MarketArchetype {
        key: key.into(),
        presence_probability: 1.0,
        volume_mult_range: (1.0, 1.0),
        rate_mult_range: (1.0, 1.0),
        annual_growth_range: growth,
        weight_tilt_strength: 0.0,
        exclusive_group: None,
        emergence: None,
        template,
    };

    vec![
        // Mainstays: starting volume/rates literally fixed across
        // seeds; only the growth trajectory varies. GEO is a mature
        // business (flat to gently moving), rideshare rides the
        // smallsat wave (never shrinks: reputation-0 opening floor).
        pinned("market_geo_comsats", (-0.02, 0.02), by_id(MARKET_GEO_COMSATS, &base)),
        MarketArchetype {
            key: "market_gov_science".into(),
            presence_probability: 1.0,
            volume_mult_range: (0.8, 1.3),
            rate_mult_range: (0.9, 1.15),
            annual_growth_range: (-0.01, 0.03),
            weight_tilt_strength: 0.15,
            exclusive_group: None,
            emergence: None,
            template: by_id(MARKET_GOV_SCIENCE, &base),
        },
        pinned("market_rideshare", (0.02, 0.08), by_id(MARKET_RIDESHARE, &base)),
        MarketArchetype {
            key: "market_cots".into(),
            presence_probability: 0.70,
            volume_mult_range: (0.8, 1.3),
            rate_mult_range: (0.9, 1.1),
            annual_growth_range: (0.0, 0.05),
            weight_tilt_strength: 0.0,
            exclusive_group: None,
            emergence: Some(EmergenceSpec {
                year_range: (2004, 2008),
                flavor: "NASA announces Commercial Orbital Transportation Services program".into(),
                cross_effects: Vec::new(),
            }),
            template: by_id(MARKET_COTS, &event),
        },
        MarketArchetype {
            key: "market_leo_constellation".into(),
            presence_probability: 0.60,
            volume_mult_range: (0.7, 1.5),
            rate_mult_range: (0.85, 1.15),
            annual_growth_range: (0.04, 0.12),
            weight_tilt_strength: 0.1,
            exclusive_group: Some("constellation".into()),
            emergence: Some(EmergenceSpec {
                year_range: (2008, 2015),
                flavor: "Major LEO broadband constellation announced — GEO market share declining".into(),
                cross_effects: vec![CrossEffect {
                    target: MARKET_GEO_COMSATS,
                    modifier: MarketModifier {
                        id: "constellation_competition".into(),
                        description: "LEO constellations taking market share".into(),
                        volume_mult: 0.6,
                        rate_mult: 0.9,
                        end_date: None,
                    },
                }],
            }),
            template: by_id(MARKET_LEO_CONSTELLATION, &event),
        },
        MarketArchetype {
            key: "market_meo_constellation".into(),
            presence_probability: 0.30,
            volume_mult_range: (0.7, 1.4),
            rate_mult_range: (0.9, 1.1),
            annual_growth_range: (0.03, 0.10),
            weight_tilt_strength: 0.0,
            exclusive_group: Some("constellation".into()),
            emergence: Some(EmergenceSpec {
                year_range: (2008, 2015),
                flavor: "MEO navigation constellation contracts opening up — GEO demand softening".into(),
                cross_effects: vec![CrossEffect {
                    target: MARKET_GEO_COMSATS,
                    modifier: MarketModifier {
                        id: "constellation_competition".into(),
                        description: "MEO constellations taking market share".into(),
                        volume_mult: 0.7,
                        rate_mult: 0.95,
                        end_date: None,
                    },
                }],
            }),
            template: by_id(MARKET_MEO_CONSTELLATION, &event),
        },
        MarketArchetype {
            key: "market_nssl".into(),
            presence_probability: 0.50,
            volume_mult_range: (0.8, 1.2),
            rate_mult_range: (0.9, 1.2),
            annual_growth_range: (0.0, 0.04),
            weight_tilt_strength: 0.1,
            exclusive_group: None,
            emergence: Some(EmergenceSpec {
                year_range: (2010, 2018),
                flavor: "National security space launch program opens to new providers".into(),
                cross_effects: Vec::new(),
            }),
            template: by_id(MARKET_NSSL, &event),
        },
        MarketArchetype {
            key: "market_earth_obs".into(),
            presence_probability: 0.70,
            volume_mult_range: (0.7, 1.4),
            rate_mult_range: (0.85, 1.15),
            annual_growth_range: (0.02, 0.10),
            weight_tilt_strength: 0.2,
            exclusive_group: None,
            emergence: Some(EmergenceSpec {
                year_range: (2005, 2012),
                flavor: "Commercial Earth observation market taking off".into(),
                cross_effects: Vec::new(),
            }),
            template: by_id(MARKET_EARTH_OBS, &event),
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
        let date = GameDate::new(2001, 1, 1);
        let vol_before = market.effective_volume(1.0, date);
        market.add_modifier(MarketModifier {
            id: "test".into(), description: "Test".into(),
            volume_mult: 0.5, rate_mult: 1.0, end_date: None,
        });
        let vol_after = market.effective_volume(1.0, date);
        assert!((vol_after - vol_before * 0.5).abs() < 0.01);
    }

    #[test]
    fn test_growth_compounds_from_activation() {
        let mut market = initial_markets().remove(0);
        market.annual_growth = 0.10;

        // No activation date -> no growth.
        assert_eq!(market.growth_factor(GameDate::new(2005, 1, 1)), 1.0);

        market.activation_date = Some(GameDate::new(2001, 1, 1));
        // Before/at activation -> no growth.
        assert_eq!(market.growth_factor(GameDate::new(2000, 6, 1)), 1.0);
        assert_eq!(market.growth_factor(GameDate::new(2001, 1, 1)), 1.0);
        // Two years on -> ~1.1^2, within leap-day slop.
        let two_years = market.growth_factor(GameDate::new(2003, 1, 1));
        assert!(
            (two_years - 1.21).abs() < 0.01,
            "expected ~1.21 growth factor after 2 years at 10%, got {two_years}",
        );
        // Growth feeds effective_volume.
        let base = market.effective_volume(1.0, GameDate::new(2001, 1, 1));
        let grown = market.effective_volume(1.0, GameDate::new(2003, 1, 1));
        assert!((grown / base - two_years).abs() < 1e-9);
    }

    #[test]
    fn test_negative_growth_shrinks() {
        let mut market = initial_markets().remove(0);
        market.annual_growth = -0.02;
        market.activation_date = Some(GameDate::new(2001, 1, 1));
        let factor = market.growth_factor(GameDate::new(2011, 1, 1));
        assert!(
            factor < 1.0 && factor > 0.7,
            "expected mild decade-long decline at -2%/yr, got {factor}",
        );
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
