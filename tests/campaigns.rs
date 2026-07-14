//! M2 Task 5: anchor-customer campaigns guards.
//!
//! Locks the campaign pipeline end-to-end: `spawn_campaign` draws
//! parameters within the configured `CampaignSpec` and applies the
//! block-buy discount correctly, `campaign_contract` produces a
//! correlated, numbered series of missions, `GameState` announces and
//! retires campaigns on schedule while a skipped mission never blocks
//! the next, and `MarketsConfig::validate` rejects malformed specs.

use std::collections::{HashMap, HashSet};

use rand::rngs::StdRng;
use rand::SeedableRng;

use rocket_tycoon::balance_config::{BalanceConfig, MarketsConfig};
use rocket_tycoon::calendar::GameDate;
use rocket_tycoon::contract::{
    campaign_contract, initial_markets, spawn_campaign, CampaignSpec, Market,
    MARKET_RIDESHARE,
};
use rocket_tycoon::game_state::GameState;

fn rideshare_market() -> Market {
    initial_markets()
        .into_iter()
        .find(|m| m.id == MARKET_RIDESHARE)
        .expect("rideshare template exists")
}

fn rigged_spec() -> CampaignSpec {
    CampaignSpec {
        spawn_chance_per_month: 1.0,
        mission_count_range: (3, 5),
        interval_days_range: (60, 120),
        discount_range: (0.10, 0.20),
        program_names: vec![
            "Test Constellation".into(),
            "Sample Series".into(),
            "Fixture Fleet".into(),
        ],
    }
}

#[test]
fn spawn_draws_params_within_spec() {
    let market = rideshare_market();
    let spec = rigged_spec();
    let current_date = GameDate::new(2001, 1, 1);

    let min_payload = market
        .destinations
        .iter()
        .map(|d| d.min_payload_kg)
        .fold(f64::INFINITY, f64::min);
    let max_payload = market
        .destinations
        .iter()
        .map(|d| d.max_payload_kg)
        .fold(f64::NEG_INFINITY, f64::max);
    let dest_minimums: Vec<f64> = market.destinations.iter().map(|d| d.min_payload_kg).collect();

    let mut next_campaign_id = 1u64;
    for seed_value in 0..30u64 {
        let mut rng = StdRng::seed_from_u64(seed_value);
        let campaign = spawn_campaign(
            &market, &spec, &mut rng, &mut next_campaign_id, current_date, 1.0,
        )
        .unwrap_or_else(|| panic!("seed {seed_value}: spawn_chance 1.0 must always spawn"));

        assert!(
            campaign.missions_total >= spec.mission_count_range.0
                && campaign.missions_total <= spec.mission_count_range.1,
            "seed {seed_value}: missions_total {} outside {:?}",
            campaign.missions_total, spec.mission_count_range,
        );
        assert!(
            campaign.interval_days >= spec.interval_days_range.0
                && campaign.interval_days <= spec.interval_days_range.1,
            "seed {seed_value}: interval_days {} outside {:?}",
            campaign.interval_days, spec.interval_days_range,
        );
        assert!(
            campaign.payload_kg >= min_payload && campaign.payload_kg <= max_payload,
            "seed {seed_value}: payload_kg {} outside [{min_payload}, {max_payload}]",
            campaign.payload_kg,
        );
        let is_multiple_of_100 = (campaign.payload_kg / 100.0).fract().abs() < 1e-9;
        let is_dest_minimum = dest_minimums.iter().any(|m| (*m - campaign.payload_kg).abs() < 1e-9);
        assert!(
            is_multiple_of_100 || is_dest_minimum,
            "seed {seed_value}: payload_kg {} neither a multiple of 100 nor a destination minimum",
            campaign.payload_kg,
        );
        assert!(
            spec.program_names.contains(&campaign.name),
            "seed {seed_value}: name `{}` not in program pool", campaign.name,
        );
        assert_eq!(campaign.missions_issued, 0);
        assert!(
            campaign.payment_per_mission > 0.0,
            "seed {seed_value}: payment_per_mission must be positive",
        );
    }
}

#[test]
fn block_buy_price_is_discounted() {
    let mut market = rideshare_market();
    market.destinations.truncate(1);
    let dest = market.destinations[0].clone();

    let spec = CampaignSpec {
        spawn_chance_per_month: 1.0,
        mission_count_range: (3, 5),
        interval_days_range: (60, 120),
        discount_range: (0.25, 0.25),
        program_names: vec!["Fixed Discount Program".into()],
    };
    let current_date = GameDate::new(2001, 1, 1);

    let mut next_campaign_id = 1u64;
    for seed_value in 0..10u64 {
        let mut rng = StdRng::seed_from_u64(seed_value);
        let campaign = spawn_campaign(
            &market, &spec, &mut rng, &mut next_campaign_id, current_date, 1.0,
        )
        .unwrap_or_else(|| panic!("seed {seed_value}: spawn_chance 1.0 must always spawn"));

        let expected = (campaign.payload_kg * dest.rate_per_kg * 0.75 / 10_000.0).round() * 10_000.0;
        assert_eq!(
            campaign.payment_per_mission, expected,
            "seed {seed_value}: block-buy price not discounted as expected",
        );
    }
}

#[test]
fn missions_are_correlated_and_numbered() {
    let market = rideshare_market();
    let spec = rigged_spec();
    let current_date = GameDate::new(2001, 1, 1);
    let mut next_campaign_id = 1u64;
    let mut rng = StdRng::seed_from_u64(99);
    let campaign = spawn_campaign(
        &market, &spec, &mut rng, &mut next_campaign_id, current_date, 1.0,
    )
    .expect("spawn_chance 1.0 must spawn");

    let deadline_window = (60u32, 150u32);
    let mut next_contract_id = 1u64;
    let mut working = campaign.clone();
    let mut contracts = Vec::new();
    let mut issue_dates = Vec::new();
    for i in 0..3 {
        let issue_date = current_date.add_days(i * 30);
        let c = campaign_contract(&working, deadline_window, &mut rng, &mut next_contract_id, issue_date);
        contracts.push(c);
        issue_dates.push(issue_date);
        working.missions_issued += 1;
    }

    // Shared correlated fields.
    for c in &contracts {
        assert_eq!(c.destination, campaign.destination);
        assert_eq!(c.payload_kg, campaign.payload_kg);
        assert_eq!(c.payment, campaign.payment_per_mission);
        assert_eq!(c.market_id, campaign.market_id);
        assert_eq!(c.campaign_id, Some(campaign.id));
    }

    // Numbered names.
    assert_eq!(
        contracts[0].name,
        format!("{} Flight 1 to {}", campaign.name, campaign.destination_display),
    );
    assert_eq!(
        contracts[1].name,
        format!("{} Flight 2 to {}", campaign.name, campaign.destination_display),
    );
    assert_eq!(
        contracts[2].name,
        format!("{} Flight 3 to {}", campaign.name, campaign.destination_display),
    );

    // Deadlines within the window of their own issue date.
    for (c, issue_date) in contracts.iter().zip(issue_dates.iter()) {
        let span = issue_date.days_until(&c.deadline);
        assert!(
            span >= deadline_window.0 && span <= deadline_window.1,
            "deadline {span} days from issue outside {:?}", deadline_window,
        );
    }
}

#[test]
fn campaigns_flow_end_to_end_and_retire() {
    let mut config = BalanceConfig::default();
    let arch = config
        .markets
        .archetypes
        .iter_mut()
        .find(|a| a.key == "market_rideshare")
        .expect("market_rideshare archetype exists");
    arch.campaign = Some(CampaignSpec {
        spawn_chance_per_month: 1.0,
        mission_count_range: (2, 2),
        interval_days_range: (30, 30),
        discount_range: (0.1, 0.1),
        program_names: vec!["End To End Program".into()],
    });

    let mut gs = GameState::with_balance("Test".into(), 42, config);

    let mut seen_contract_ids: HashSet<u64> = HashSet::new();
    // campaign_id -> (payload_kg, destination, payment, market_id)
    let mut campaign_facts: HashMap<u64, (f64, String, f64, u64)> = HashMap::new();
    // campaign_id -> first-seen day of each mission (in issue order).
    let mut campaign_issue_days: HashMap<u64, Vec<u32>> = HashMap::new();
    let mut saw_any_campaign_contract = false;

    for day in 0..150u32 {
        gs.advance_day();

        for c in &gs.available_contracts {
            let Some(cid) = c.campaign_id else { continue };
            if !seen_contract_ids.insert(c.id.0) {
                continue;
            }
            saw_any_campaign_contract = true;

            let entry = campaign_facts.entry(cid.0).or_insert((
                c.payload_kg,
                c.destination.clone(),
                c.payment,
                c.market_id.0,
            ));
            assert_eq!(entry.0, c.payload_kg, "campaign {}: payload_kg drifted between missions", cid.0);
            assert_eq!(entry.1, c.destination, "campaign {}: destination drifted between missions", cid.0);
            assert_eq!(entry.2, c.payment, "campaign {}: payment drifted between missions", cid.0);
            assert_eq!(entry.3, c.market_id.0, "campaign {}: market_id drifted between missions", cid.0);

            campaign_issue_days.entry(cid.0).or_default().push(day);
        }
        // Never accept any contract: demonstrates skipped missions
        // don't block later ones.
    }

    assert!(saw_any_campaign_contract, "no campaign mission contracts were ever announced");

    for (cid, days) in &campaign_issue_days {
        for w in days.windows(2) {
            assert_eq!(
                w[1] - w[0], 30,
                "campaign {cid}: consecutive missions issued {} days apart, expected 30", w[1] - w[0],
            );
        }
    }

    assert!(
        gs.active_campaigns.iter().all(|c| c.missions_issued < c.missions_total),
        "a campaign with all missions issued should have been retired",
    );
}

#[test]
fn validation_rejects_bad_campaign_specs() {
    fn config_with<F: FnOnce(&mut CampaignSpec)>(mutate: F) -> MarketsConfig {
        let mut config = BalanceConfig::default().markets;
        let arch = config
            .archetypes
            .iter_mut()
            .find(|a| a.key == "market_rideshare")
            .expect("market_rideshare archetype exists");
        let mut spec = arch.campaign.clone().expect("market_rideshare has a campaign spec");
        mutate(&mut spec);
        arch.campaign = Some(spec);
        config
    }

    let bad_spawn_chance = config_with(|s| s.spawn_chance_per_month = 1.5);
    assert!(bad_spawn_chance.validate().is_err(), "spawn_chance_per_month 1.5 should be rejected");

    let bad_interval = config_with(|s| s.interval_days_range = (0, 30));
    assert!(bad_interval.validate().is_err(), "interval_days_range (0, 30) should be rejected");

    let bad_discount = config_with(|s| s.discount_range = (0.5, 0.4));
    assert!(bad_discount.validate().is_err(), "discount_range (0.5, 0.4) should be rejected");

    let bad_names = config_with(|s| s.program_names = Vec::new());
    assert!(bad_names.validate().is_err(), "empty program_names should be rejected");

    // Sanity: the unmutated default config validates cleanly.
    let good = BalanceConfig::default().markets;
    assert!(good.validate().is_ok(), "default config should validate");
}
