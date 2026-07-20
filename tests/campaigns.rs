//! Anchor-customer campaign guards (block-bid redesign).
//!
//! Locks the campaign pipeline end-to-end: `spawn_campaign` draws
//! parameters within the configured `CampaignSpec`, applies the
//! block-buy discount, and opens a sealed solicitation with the hidden
//! per-mission ceiling; `campaign_contract` produces a correlated,
//! numbered series of missions; `GameState` announces a program, takes
//! the player's block bid, awards or lapses it at the deadline, issues
//! the winner's missions pre-accepted on cadence, and retires the
//! campaign; and `MarketsConfig::validate` rejects malformed specs.

use rand::rngs::StdRng;
use rand::SeedableRng;

use rocket_tycoon::balance_config::{BalanceConfig, MarketsConfig};
use rocket_tycoon::calendar::GameDate;
use rocket_tycoon::contract::{
    campaign_contract, initial_markets, spawn_campaign, CampaignSpec,
    CampaignStatus, ContractStatus, Market, MARKET_RIDESHARE,
};
use rocket_tycoon::game_state::{GameSpeed, GameState};

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
        bid_window_days: 20,
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
        // Announcement opens the sealed block-bid window with the same
        // ceiling rule as single solicitations.
        match campaign.status {
            CampaignStatus::Soliciting {
                bid_deadline, budget_ceiling_per_mission, player_bid,
            } => {
                assert_eq!(
                    bid_deadline,
                    current_date.add_days(spec.bid_window_days),
                    "seed {seed_value}: bid deadline should be announcement + window",
                );
                let expected_ceiling =
                    campaign.payment_per_mission * market.budget_tolerance;
                assert!(
                    (budget_ceiling_per_mission - expected_ceiling).abs() < 1e-6,
                    "seed {seed_value}: ceiling {} != reference x budget_tolerance {}",
                    budget_ceiling_per_mission, expected_ceiling,
                );
                assert_eq!(player_bid, None, "seed {seed_value}: bids start empty");
            }
            ref other => panic!("seed {seed_value}: fresh campaign should be Soliciting, got {other:?}"),
        }
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
        bid_window_days: 20,
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

/// A GameState with monthly rideshare campaign announcements rigged on
/// (2 missions, 30-day cadence, 20-day bid window).
fn campaign_world(seed: u64) -> GameState {
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
        bid_window_days: 20,
    });
    GameState::with_balance("Test".into(), seed, config)
}

/// Advance until the first campaign announcement and return its id.
fn advance_to_announcement(gs: &mut GameState) -> rocket_tycoon::contract::CampaignId {
    for _ in 0..70u32 {
        gs.advance_day();
        if let Some(c) = gs.active_campaigns.first() {
            return c.id;
        }
    }
    panic!("no campaign announced within 70 days at spawn chance 1.0");
}

#[test]
fn won_campaign_issues_preaccepted_missions_and_retires() {
    let mut gs = campaign_world(42);
    let cid = advance_to_announcement(&mut gs);

    let (reference, deadline) = {
        let c = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap();
        match c.status {
            CampaignStatus::Soliciting { bid_deadline, .. } =>
                (c.payment_per_mission, bid_deadline),
            ref other => panic!("fresh campaign should be Soliciting, got {other:?}"),
        }
    };

    // Bid exactly the hidden reference: always within the ceiling
    // (budget_tolerance >= 1), so the sole bidder must win.
    let bid = reference;
    gs.place_campaign_bid(cid, bid).expect("bid on a soliciting campaign");

    // Run through the deadline; capture the award.
    let mut awarded = false;
    while gs.date <= deadline {
        for evt in gs.advance_day() {
            if let rocket_tycoon::event::GameEvent::CampaignAwarded { amount, missions, .. } = evt {
                assert_eq!(amount, bid, "award should be at the player's block bid");
                assert_eq!(missions, 2);
                awarded = true;
            }
        }
    }
    // Resolution happens the first day after the deadline.
    if !awarded {
        for evt in gs.advance_day() {
            if let rocket_tycoon::event::GameEvent::CampaignAwarded { amount, .. } = evt {
                assert_eq!(amount, bid);
                awarded = true;
            }
        }
    }
    assert!(awarded, "sole in-budget block bid must win at resolution");
    assert_eq!(gs.speed, GameSpeed::Paused, "winning a program pauses the game");
    {
        let c = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap();
        assert_eq!(c.payment_per_mission, bid, "won price becomes the block price");
        match &c.status {
            CampaignStatus::Won { by_player: true, company } =>
                assert_eq!(company, "Test"),
            other => panic!("campaign should be player-won, got {other:?}"),
        }
    }

    // Mission 1 issues the day of resolution, mission 2 thirty days
    // later; both arrive pre-accepted at the won price and never pass
    // through the open market.
    let mut mission_days: Vec<(String, GameDate)> = Vec::new();
    for _ in 0..40u32 {
        for c in &gs.player_company.active_contracts {
            if c.campaign_id == Some(cid)
                && !mission_days.iter().any(|(n, _)| *n == c.name)
            {
                assert!(
                    matches!(c.status, ContractStatus::Accepted),
                    "campaign missions must arrive pre-accepted",
                );
                assert_eq!(c.payment, bid, "missions must pay the won block price");
                mission_days.push((c.name.clone(), gs.date));
            }
        }
        assert!(
            gs.available_contracts.iter().all(|c| c.campaign_id != Some(cid)),
            "won-campaign missions must never appear as open offers",
        );
        if mission_days.len() == 2 {
            break;
        }
        gs.advance_day();
    }
    assert_eq!(mission_days.len(), 2, "both missions should have issued");
    assert!(mission_days[0].0.contains("Flight 1"));
    assert!(mission_days[1].0.contains("Flight 2"));
    assert_eq!(
        mission_days[0].1.days_until(&mission_days[1].1), 30,
        "missions should follow the program cadence",
    );
    assert!(
        gs.active_campaigns.iter().all(|c| c.id != cid),
        "a campaign with all missions issued should have been retired",
    );
}

/// Winnable-floor analog of `assert_year1_reference_bid_wins`: across
/// many worlds, bidding a campaign at its hidden reference must always
/// win while the player is the sole bidder (tolerance >= 1 guarantees
/// reference <= ceiling).
#[test]
fn reference_block_bid_wins_across_seeds() {
    for seed in 1..=20u64 {
        let mut gs = campaign_world(seed);
        let cid = advance_to_announcement(&mut gs);
        let (reference, deadline) = {
            let c = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap();
            match c.status {
                CampaignStatus::Soliciting { bid_deadline, .. } =>
                    (c.payment_per_mission, bid_deadline),
                ref other => panic!("seed {seed}: expected Soliciting, got {other:?}"),
            }
        };
        gs.place_campaign_bid(cid, reference).expect("bid lands");
        while gs.date <= deadline.add_days(1) {
            gs.advance_day();
        }
        // The campaign either still exists as Won, or won and already
        // finished issuing (2 missions, 30-day gap — can't happen in a
        // 21-day window, but keep the check honest via contracts).
        let won_live = gs.active_campaigns.iter().any(|c|
            c.id == cid && matches!(c.status, CampaignStatus::Won { by_player: true, .. }));
        let issued = gs.player_company.active_contracts.iter()
            .any(|c| c.campaign_id == Some(cid));
        assert!(
            won_live || issued,
            "seed {seed}: a sole reference block bid must win",
        );
    }
}

#[test]
fn campaign_status_survives_save_roundtrip() {
    let dir = std::env::temp_dir().join("rocket_tycoon_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("campaign_roundtrip_{}.json", std::process::id()));

    // Soliciting with a sealed bid.
    let mut gs = campaign_world(46);
    let cid = advance_to_announcement(&mut gs);
    let reference = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap()
        .payment_per_mission;
    gs.place_campaign_bid(cid, reference * 1.1).expect("bid lands");
    rocket_tycoon::save::save_game(&gs, &path).expect("save");
    let loaded = rocket_tycoon::save::load_game(&path).expect("load");
    let orig = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap();
    let back = loaded.active_campaigns.iter().find(|c| c.id == cid)
        .expect("campaign survives round-trip");
    assert_eq!(orig, back, "Soliciting campaign (with sealed bid) must round-trip exactly");

    // Won status round-trips too.
    let deadline = match orig.status {
        CampaignStatus::Soliciting { bid_deadline, .. } => bid_deadline,
        ref other => panic!("expected Soliciting, got {other:?}"),
    };
    while gs.date <= deadline.add_days(1) {
        gs.advance_day();
    }
    let orig = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap();
    assert!(matches!(orig.status, CampaignStatus::Won { .. }), "bid within ceiling should win");
    rocket_tycoon::save::save_game(&gs, &path).expect("save");
    let loaded = rocket_tycoon::save::load_game(&path).expect("load");
    let back = loaded.active_campaigns.iter().find(|c| c.id == cid)
        .expect("won campaign survives round-trip");
    assert_eq!(orig, back, "Won campaign must round-trip exactly");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn unbid_campaign_lapses_quietly() {
    let mut gs = campaign_world(43);
    let cid = advance_to_announcement(&mut gs);
    let deadline = match gs.active_campaigns.iter().find(|c| c.id == cid).unwrap().status {
        CampaignStatus::Soliciting { bid_deadline, .. } => bid_deadline,
        ref other => panic!("expected Soliciting, got {other:?}"),
    };

    let mut events = Vec::new();
    while gs.date <= deadline.add_days(1) {
        events.extend(gs.advance_day());
    }
    assert!(
        gs.active_campaigns.iter().all(|c| c.id != cid),
        "an unbid campaign should lapse at its deadline",
    );
    assert!(
        !events.iter().any(|e| matches!(
            e, rocket_tycoon::event::GameEvent::CampaignBidRejected { .. },
        )),
        "lapsing without a bid should not announce a rejection",
    );
    assert!(
        gs.player_company.active_contracts.iter().all(|c| c.campaign_id != Some(cid)),
        "a lapsed campaign must issue no missions",
    );
}

#[test]
fn over_ceiling_block_bid_is_rejected() {
    let mut gs = campaign_world(44);
    let cid = advance_to_announcement(&mut gs);
    let (ceiling, deadline) = match gs.active_campaigns.iter().find(|c| c.id == cid).unwrap().status {
        CampaignStatus::Soliciting { budget_ceiling_per_mission, bid_deadline, .. } =>
            (budget_ceiling_per_mission, bid_deadline),
        ref other => panic!("expected Soliciting, got {other:?}"),
    };

    gs.place_campaign_bid(cid, ceiling * 2.0).expect("over-ceiling bids are accepted sealed");

    let mut rejected = false;
    while gs.date <= deadline.add_days(1) {
        for evt in gs.advance_day() {
            if matches!(evt, rocket_tycoon::event::GameEvent::CampaignBidRejected { .. }) {
                rejected = true;
            }
        }
    }
    assert!(rejected, "an over-ceiling sole bid must be rejected at resolution");
    assert!(
        gs.active_campaigns.iter().all(|c| c.id != cid),
        "a rejected campaign lapses",
    );
    assert!(
        gs.player_company.active_contracts.iter().all(|c| c.campaign_id != Some(cid)),
        "a rejected campaign must issue no missions",
    );
}

#[test]
fn bids_only_land_on_soliciting_campaigns() {
    let mut gs = campaign_world(45);
    let cid = advance_to_announcement(&mut gs);

    assert!(gs.place_campaign_bid(cid, 0.0).is_none(), "non-positive bids refused");
    assert!(
        gs.place_campaign_bid(rocket_tycoon::contract::CampaignId(9999), 1.0).is_none(),
        "unknown campaign refused",
    );

    // Win it, then confirm re-bidding a resolved campaign is refused.
    let deadline = match gs.active_campaigns.iter().find(|c| c.id == cid).unwrap().status {
        CampaignStatus::Soliciting { bid_deadline, .. } => bid_deadline,
        ref other => panic!("expected Soliciting, got {other:?}"),
    };
    let reference = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap().payment_per_mission;
    gs.place_campaign_bid(cid, reference).expect("first bid lands");
    // Revision before the deadline is allowed.
    gs.place_campaign_bid(cid, reference * 0.9).expect("revising a sealed bid is allowed");
    while gs.date <= deadline.add_days(1) {
        gs.advance_day();
        if gs.active_campaigns.iter().any(|c|
            c.id == cid && matches!(c.status, CampaignStatus::Won { .. }))
        {
            break;
        }
    }
    let c = gs.active_campaigns.iter().find(|c| c.id == cid).unwrap();
    assert!(matches!(c.status, CampaignStatus::Won { .. }), "revised bid should still win");
    assert_eq!(c.payment_per_mission, (reference * 0.9), "last revision is the sealed bid");
    assert!(gs.place_campaign_bid(cid, 1.0).is_none(), "resolved campaigns take no bids");
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
        // Defaults ship with campaigns off (M3), so seed a valid spec
        // and then break it.
        let mut spec = rigged_spec();
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

    let bad_window = config_with(|s| s.bid_window_days = 0);
    assert!(bad_window.validate().is_err(), "bid_window_days 0 should be rejected");

    // Sanity: the unmutated default config validates cleanly.
    let good = BalanceConfig::default().markets;
    assert!(good.validate().is_ok(), "default config should validate");
}
