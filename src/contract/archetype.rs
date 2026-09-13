//! Market archetypes: the seed-perturbed realisation of the event
//! markets (M2) — when each emerges, how strongly, and what it does
//! to its neighbours.

use super::*;
use crate::seed::WorldQuery;

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
    /// Anchor-customer campaign generation for this market (None =
    /// no campaigns).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub campaign: Option<CampaignSpec>,
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
    let mut rng = seed.world_query(WorldQuery::MarketArchetype(&arch.key));

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
///
/// Campaigns run in three markets (campaign redesign Task 4): GEO
/// Comsats carries the early game (always present), COTS and LEO
/// Constellation join when they emerge — block buys are their whole
/// identity. Spawn chances target roughly one announcement in year 1
/// (GEO only) growing to ~1-2/year once the event markets open.
pub fn default_archetypes() -> Vec<MarketArchetype> {
    let base = initial_markets();
    let event = event_market_templates();
    let by_id = |id: MarketId, from: &[Market]| -> Market {
        from.iter().find(|m| m.id == id).expect("template exists").clone()
    };
    let pinned = |key: &str, growth: (f64, f64), campaign: Option<CampaignSpec>,
                  template: Market| MarketArchetype {
        key: key.into(),
        presence_probability: 1.0,
        volume_mult_range: (1.0, 1.0),
        rate_mult_range: (1.0, 1.0),
        annual_growth_range: growth,
        weight_tilt_strength: 0.0,
        exclusive_group: None,
        emergence: None,
        campaign,
        template,
    };

    vec![
        // Mainstays: starting volume/rates literally fixed across
        // seeds; only the growth trajectory varies. GEO is a mature
        // business (flat to gently moving), rideshare rides the
        // smallsat wave (never shrinks: reputation-0 opening floor).
        pinned(
            "market_geo_comsats",
            (-0.02, 0.02),
            Some(CampaignSpec {
                spawn_chance_per_month: 0.03,
                mission_count_range: (3, 5),
                interval_days_range: (60, 120),
                discount_range: (0.10, 0.20),
                program_names: vec![
                    "Meridian Constellation".into(),
                    "EchoRelay Fleet".into(),
                    "PanAm Sat Series".into(),
                    "Orion Direct Broadcast".into(),
                    "Concordia Comms Block".into(),
                ],
                bid_window_days: 40,
            }),
            by_id(MARKET_GEO_COMSATS, &base),
        ),
        MarketArchetype {
            key: "market_gov_science".into(),
            presence_probability: 1.0,
            volume_mult_range: (0.8, 1.3),
            rate_mult_range: (0.9, 1.15),
            annual_growth_range: (-0.01, 0.03),
            weight_tilt_strength: 0.15,
            exclusive_group: None,
            emergence: None,
            campaign: None,
            template: by_id(MARKET_GOV_SCIENCE, &base),
        },
        pinned(
            "market_rideshare",
            (0.02, 0.08),
            None,
            by_id(MARKET_RIDESHARE, &base),
        ),
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
            // Block buys are the whole point of COTS: resupply cycles
            // announced as multi-flight programs.
            campaign: Some(CampaignSpec {
                spawn_chance_per_month: 0.06,
                mission_count_range: (4, 8),
                interval_days_range: (45, 90),
                discount_range: (0.15, 0.25),
                program_names: vec![
                    "Commercial Resupply Cycle A".into(),
                    "Commercial Resupply Cycle B".into(),
                    "Station Logistics Block".into(),
                    "Crew & Cargo Demonstration".into(),
                ],
                bid_window_days: 45,
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
                        ..Default::default()
                    },
                }],
            }),
            // Constellation build-outs arrive as launch blocks.
            campaign: Some(CampaignSpec {
                spawn_chance_per_month: 0.06,
                mission_count_range: (4, 8),
                interval_days_range: (45, 90),
                discount_range: (0.10, 0.20),
                program_names: vec![
                    "Starweave Deployment".into(),
                    "Aurora Broadband Shell".into(),
                    "Loom Constellation Build-out".into(),
                    "Skynet Mesh Phase 1".into(),
                ],
                bid_window_days: 40,
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
                        ..Default::default()
                    },
                }],
            }),
            campaign: None,
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
            campaign: None,
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
            campaign: None,
            template: by_id(MARKET_EARTH_OBS, &event),
        },
    ]
}
