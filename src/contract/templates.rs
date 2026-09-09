//! The hand-written markets: the eight that open the game and the
//! event-driven templates the archetypes realise from.

use super::*;

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
            rep_target: 50.0,
            w_cost: 0.6,
            w_rep: 0.4,
            budget_tolerance: 1.2,
            economy_sensitivity: EconomySensitivity::Moderate,
            name_prefixes: vec!["ComSat".into(), "BroadcastSat".into(), "RelaySat".into()],
            modifiers: Vec::new(),
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((90, 240)),
            failure_severity: 1.2,
            cadence: Cadence::Steady,
            volume_accumulator: 0.0,
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
            rep_target: 40.0,
            w_cost: 0.4,
            w_rep: 0.6,
            budget_tolerance: 1.3,
            economy_sensitivity: EconomySensitivity::Low,
            name_prefixes: vec!["Observatory".into(), "SciSat".into(), "Probe".into(), "WeatherSat".into()],
            modifiers: Vec::new(),
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((120, 360)),
            failure_severity: 0.7,
            cadence: Cadence::Steady,
            volume_accumulator: 0.0,
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
            rep_target: -10.0,
            w_cost: 0.8,
            w_rep: 0.2,
            budget_tolerance: 1.15,
            economy_sensitivity: EconomySensitivity::Moderate,
            name_prefixes: vec!["CubeSat Bundle".into(), "University Payload".into(), "TechDemo".into()],
            modifiers: Vec::new(),
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((60, 150)),
            failure_severity: 1.0,
            cadence: Cadence::Steady,
            volume_accumulator: 0.0,
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
            rep_target: 60.0,
            w_cost: 0.5,
            w_rep: 0.5,
            budget_tolerance: 1.25,
            economy_sensitivity: EconomySensitivity::Low,
            name_prefixes: vec!["ISS Resupply".into(), "Station Cargo".into(), "Crew Rotation".into()],
            modifiers: Vec::new(),
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((90, 270)),
            failure_severity: 2.0,
            cadence: Cadence::Steady,
            volume_accumulator: 0.0,
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
            rep_target: 20.0,
            w_cost: 0.8,
            w_rep: 0.2,
            budget_tolerance: 1.15,
            economy_sensitivity: EconomySensitivity::High,
            name_prefixes: vec!["Constellation Batch".into(), "LEO Deploy".into(), "Network Sat".into()],
            modifiers: Vec::new(),
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((60, 180)),
            failure_severity: 1.0,
            cadence: Cadence::Burst { burst_chance: 0.2 },
            volume_accumulator: 0.0,
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
            rep_target: 30.0,
            w_cost: 0.8,
            w_rep: 0.2,
            budget_tolerance: 1.15,
            economy_sensitivity: EconomySensitivity::High,
            name_prefixes: vec!["NavSat Batch".into(), "MEO Deploy".into(), "Constellation Unit".into()],
            modifiers: Vec::new(),
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((90, 210)),
            failure_severity: 1.0,
            cadence: Cadence::Burst { burst_chance: 0.2 },
            volume_accumulator: 0.0,
        },
        Market {
            id: MARKET_NSSL,
            name: "National Reconnaissance".into(),
            description: "Imaging and signals intelligence satellites for the NRO. \
                          Irreplaceable payloads; failures draw hearings".into(),
            active: false,
            base_volume: 0.3,
            destinations: vec![
                // Reconnaissance flies low and polar. The weighting matters
                // beyond flavour: an ASAT exchange suppresses LEO and SSO
                // for everyone else, and this is the customer whose own
                // satellites were the target.
                MarketDestination {
                    location_id: "leo".into(), display_name: "LEO".into(),
                    min_payload_kg: 1_000.0, max_payload_kg: 10_000.0,
                    rate_per_kg: 60_000.0, weight: 0.4,
                },
                MarketDestination {
                    location_id: "sso".into(), display_name: "SSO".into(),
                    min_payload_kg: 1_000.0, max_payload_kg: 5_000.0,
                    rate_per_kg: 70_000.0, weight: 0.35,
                },
                MarketDestination {
                    location_id: "gto".into(), display_name: "GTO".into(),
                    min_payload_kg: 2_000.0, max_payload_kg: 7_000.0,
                    rate_per_kg: 80_000.0, weight: 0.15,
                },
                MarketDestination {
                    location_id: "geo".into(), display_name: "GEO".into(),
                    min_payload_kg: 2_000.0, max_payload_kg: 5_000.0,
                    rate_per_kg: 150_000.0, weight: 0.1,
                },
            ],
            rep_target: 80.0,
            w_cost: 0.35,
            w_rep: 0.65,
            budget_tolerance: 1.4,
            economy_sensitivity: EconomySensitivity::None,
            name_prefixes: vec!["KEYHOLE Follow-on".into(), "Recon Payload".into(), "Classified Mission".into()],
            modifiers: Vec::new(),
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((120, 360)),
            failure_severity: 1.5,
            cadence: Cadence::Lumpy { quiet_chance: 0.5 },
            volume_accumulator: 0.0,
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
            rep_target: 10.0,
            w_cost: 0.75,
            w_rep: 0.25,
            budget_tolerance: 1.15,
            economy_sensitivity: EconomySensitivity::Moderate,
            name_prefixes: vec!["ImagingSat".into(), "RadarSat".into(), "EarthWatch".into()],
            modifiers: Vec::new(),
            annual_growth: 0.0,
            activation_date: None,
            deadline_days: Some((60, 180)),
            failure_severity: 1.0,
            cadence: Cadence::Lumpy { quiet_chance: 0.4 },
            volume_accumulator: 0.0,
        },
    ]
}
