//! M3 Task 2: the scripted competitor DinoSoar, locked end-to-end.
//!
//! DinoSoar is a real `Company` — real manufacturing, real inventory,
//! real reputation — driven by a margin script instead of a player,
//! competing in the same sealed-bid `resolve_bids` the player does.
//! This file locks: award competition (dino outbids an expensive
//! player, an aggressive player undercuts dino, dino declines when
//! out of stock so the player wins by default), capacity limits
//! (initial_stock caps simultaneous awards), the abstract launch
//! outcome (failure dents reputation and withholds payment, success
//! pays and boosts reputation), the year-1 floor protection that
//! keeps DinoSoar out of the rideshare market, save/load round-trip
//! (including pre-M3 backfill), and the disabled-competitor path.
//!
//! Pattern: most tests inject a hand-built GEO Comsats solicitation
//! directly into `available_contracts` rather than waiting for
//! generated ones, so the bid amounts and deadlines are fully under
//! test control. See `tests/bidding.rs` for the player-only sibling
//! of this file.

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::competitor::ScheduledLaunch;
use rocket_tycoon::contract::{Contract, ContractId, ContractStatus, MARKET_GEO_COMSATS, MARKET_RIDESHARE};
use rocket_tycoon::event::GameEvent;
use rocket_tycoon::game_state::GameState;

/// Build a fresh game under default balance (competitor enabled) at
/// `seed`.
fn fresh_game(seed: u64) -> GameState {
    GameState::with_balance("Test".into(), seed, BalanceConfig::default())
}

/// Inject a GEO Comsats solicitation with a `bid_close` deadline and
/// `ceiling` budget, well inside DinoSoar's capability table (gto,
/// 5,000 kg is comfortably under the 13,500 kg gto cap). Returns the
/// index it lands at (always pushed to the back of an otherwise-empty
/// vec, so 0 unless the caller already pushed others).
fn inject_geo_solicitation(
    gs: &mut GameState,
    id: u64,
    name: &str,
    bid_close: rocket_tycoon::calendar::GameDate,
    ceiling: f64,
) -> usize {
    gs.available_contracts.push(Contract {
        id: ContractId(id),
        name: name.into(),
        destination: "gto".into(),
        payload_kg: 5_000.0,
        payment: 0.0,
        deadline: gs.date.add_days(400),
        status: ContractStatus::Available,
        market_id: MARKET_GEO_COMSATS,
        campaign_id: None,
        bid_deadline: Some(bid_close),
        budget_ceiling: ceiling,
        player_bid: None,
    });
    gs.available_contracts.len() - 1
}

/// Advance days one at a time, collecting every event fired, until
/// `gs.date` exceeds `deadline` (inclusive of the day resolution
/// fires). Panics if resolution doesn't happen within `max_days` — a
/// generous cap so a bug that skips resolution fails loudly instead
/// of looping forever.
fn advance_through(gs: &mut GameState, deadline: rocket_tycoon::calendar::GameDate, max_days: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..max_days {
        all.extend(gs.advance_day());
        if gs.date > deadline {
            return all;
        }
    }
    panic!("resolution did not happen within {max_days} days of deadline {deadline}");
}

/// Rig every rocket flaw on every inventory rocket of DinoSoar's
/// design to a fixed activation chance, so the next abstract launch
/// has a deterministic outcome regardless of which shelf item gets
/// consumed.
fn rig_all_dino_flaws(gs: &mut GameState, activation_chance: f64) {
    let design_id = gs.competitors[0].design_id;
    for rocket in gs.competitors[0].company.manufacturing.inventory.rockets.iter_mut() {
        if rocket.design_id == design_id {
            for flaw in rocket.rocket_flaws.iter_mut() {
                flaw.activation_chance = activation_chance;
            }
        }
    }
}

// ---------------------------------------------------------------
// 1. Dino outbids an expensive player.
// ---------------------------------------------------------------

#[test]
fn dino_outbids_expensive_player() {
    let seed = 101;
    let mut gs = fresh_game(seed);
    assert_eq!(gs.competitors.len(), 1, "seed {seed}: expected exactly one competitor");

    let bid_close = gs.date.add_days(5);
    let idx = inject_geo_solicitation(&mut gs, 9001, "InjectedSat1", bid_close, 300_000_000.0);

    // Recompute DinoSoar's expected bid on a pre-resolution clone,
    // before any days advance (free stock and marginal cost are
    // still at their fresh-game values).
    let contract_clone = gs.available_contracts[idx].clone();
    let expected_bid = gs.competitors[0]
        .compute_bid(&contract_clone, &gs.balance, &gs.seed)
        .expect("seed 101: DinoSoar should be willing to bid on a fresh gto solicitation");

    let placed = gs.place_bid(idx, 290_000_000.0);
    assert!(placed.is_some(), "seed {seed}: player bid should be accepted");

    let events = advance_through(&mut gs, bid_close, 30);

    let award = events.iter().find_map(|e| match e {
        GameEvent::ContractAwardedToCompetitor { contract_name, company, amount, player_bid }
            if contract_name == "InjectedSat1" =>
            Some((company.clone(), *amount, *player_bid)),
        _ => None,
    });
    let (company, amount, player_bid) = award.unwrap_or_else(|| {
        panic!("seed {seed}: expected ContractAwardedToCompetitor for InjectedSat1, got {events:?}")
    });
    assert_eq!(company, "DinoSoar", "seed {seed}: winner should be DinoSoar");
    assert_eq!(
        player_bid,
        Some(290_000_000.0),
        "seed {seed}: the losing player bid should ride along in the event",
    );
    assert_eq!(
        amount, expected_bid,
        "seed {seed}: awarded amount should equal DinoSoar's pre-resolution compute_bid",
    );

    assert!(
        gs.competitors[0].company.active_contracts.iter().any(|c| c.name == "InjectedSat1"),
        "seed {seed}: won contract should sit in DinoSoar's active_contracts",
    );
    assert!(
        !gs.player_company.active_contracts.iter().any(|c| c.name == "InjectedSat1"),
        "seed {seed}: player should not have the contract it lost",
    );
}

// ---------------------------------------------------------------
// 2. Player undercuts Dino and wins.
// ---------------------------------------------------------------

#[test]
fn player_undercuts_dino() {
    let seed = 102;
    let mut gs = fresh_game(seed);

    let bid_close = gs.date.add_days(5);
    let idx = inject_geo_solicitation(&mut gs, 9002, "InjectedSat2", bid_close, 300_000_000.0);

    // margin_min (8.0) x catalog_cost ($9M) = $72M is the cheapest
    // DinoSoar would ever price this at absent jitter; jitter is
    // +/-5%, so its floor in practice is ~$68.4M. $65M safely
    // undercuts that while still comfortably above the bid_floor
    // ($60M) and ceiling.
    let placed = gs.place_bid(idx, 65_000_000.0);
    assert!(placed.is_some(), "seed {seed}: player bid should be accepted");

    let events = advance_through(&mut gs, bid_close, 30);

    let award = events.iter().find_map(|e| match e {
        GameEvent::ContractAwarded { contract_name, amount } if contract_name == "InjectedSat2" =>
            Some(*amount),
        _ => None,
    });
    let amount = award.unwrap_or_else(|| {
        panic!("seed {seed}: expected player ContractAwarded for InjectedSat2, got {events:?}")
    });
    assert_eq!(amount, 65_000_000.0, "seed {seed}: awarded amount should be the player's bid");

    assert!(
        gs.player_company.active_contracts.iter().any(|c| c.name == "InjectedSat2"),
        "seed {seed}: won contract should sit in the player's active_contracts",
    );
    assert!(
        !gs.competitors[0].company.active_contracts.iter().any(|c| c.name == "InjectedSat2"),
        "seed {seed}: DinoSoar should not have the contract it lost",
    );
}

// ---------------------------------------------------------------
// 3. Player wins by default when Dino has no free stock to bid.
// ---------------------------------------------------------------

#[test]
fn player_wins_when_dino_declines() {
    let seed = 103;
    let mut gs = fresh_game(seed);

    // Reserve every rocket DinoSoar has on the shelf with fake
    // far-future scheduled launches against dummy contract ids, until
    // free_stock() hits zero.
    let mut dummy_id = 80_000u64;
    while gs.competitors[0].free_stock() > 0 {
        gs.competitors[0].scheduled_launches.push(ScheduledLaunch {
            contract_id: ContractId(dummy_id),
            launch_date: gs.date.add_days(400),
        });
        dummy_id += 1;
    }
    assert_eq!(gs.competitors[0].free_stock(), 0, "seed {seed}: setup should exhaust free stock");

    let bid_close = gs.date.add_days(5);
    let idx = inject_geo_solicitation(&mut gs, 9003, "InjectedSat3", bid_close, 300_000_000.0);
    // Expensive but within ceiling — would lose to Dino if Dino could
    // bid at all, so this only proves the point if Dino is genuinely
    // out of the running.
    let placed = gs.place_bid(idx, 290_000_000.0);
    assert!(placed.is_some(), "seed {seed}: player bid should be accepted");

    let contract_clone = gs.available_contracts[idx].clone();
    assert_eq!(
        gs.competitors[0].compute_bid(&contract_clone, &gs.balance, &gs.seed),
        None,
        "seed {seed}: DinoSoar should decline with zero free stock",
    );

    let events = advance_through(&mut gs, bid_close, 30);
    let award = events.iter().find_map(|e| match e {
        GameEvent::ContractAwarded { contract_name, amount } if contract_name == "InjectedSat3" =>
            Some(*amount),
        _ => None,
    });
    let amount = award.unwrap_or_else(|| {
        panic!("seed {seed}: expected player ContractAwarded for InjectedSat3, got {events:?}")
    });
    assert_eq!(amount, 290_000_000.0, "seed {seed}: awarded amount should be the player's bid");
}

// ---------------------------------------------------------------
// 4. Capacity limits simultaneous awards to initial_stock.
// ---------------------------------------------------------------

#[test]
fn capacity_limits_awards() {
    let seed = 104;
    let mut gs = fresh_game(seed);
    let initial_stock = gs.balance.competitor.initial_stock;
    assert_eq!(
        gs.competitors[0].free_stock(),
        initial_stock,
        "seed {seed}: fresh game should start at initial_stock free",
    );

    let bid_close = gs.date.add_days(5);
    for i in 0..6u64 {
        inject_geo_solicitation(
            &mut gs,
            9100 + i,
            &format!("Capacity{i}"),
            bid_close,
            300_000_000.0,
        );
    }
    // No player bids at all in this test.

    let events = advance_through(&mut gs, bid_close, 30);
    let awards: Vec<(String, Option<f64>)> = events.iter().filter_map(|e| match e {
        GameEvent::ContractAwardedToCompetitor { contract_name, player_bid, .. }
            if contract_name.starts_with("Capacity") =>
            Some((contract_name.clone(), *player_bid)),
        _ => None,
    }).collect();

    assert_eq!(
        awards.len(),
        initial_stock as usize,
        "seed {seed}: exactly initial_stock ({initial_stock}) awards should fire, got {awards:?}",
    );
    for (_, bid) in &awards {
        assert!(bid.is_none(), "seed {seed}: player never bid on any Capacity contract");
    }

    // The other 3 (6 - initial_stock, assuming initial_stock == 3)
    // lapse silently: gone from available_contracts, no award event.
    let lapsed_count = 6 - awards.len();
    assert_eq!(
        lapsed_count,
        (6 - initial_stock as usize),
        "seed {seed}: remaining Capacity contracts should lapse without an award",
    );
    assert!(
        gs.available_contracts.iter().all(|c| !c.name.starts_with("Capacity")),
        "seed {seed}: all Capacity contracts (won or lapsed) should be gone from available_contracts",
    );
    assert_eq!(
        gs.competitors[0].company.active_contracts.iter()
            .filter(|c| c.name.starts_with("Capacity"))
            .count(),
        initial_stock as usize,
        "seed {seed}: DinoSoar's active_contracts should hold exactly initial_stock Capacity awards",
    );
}

// ---------------------------------------------------------------
// 5. A rigged failure dents reputation and withholds payment.
// ---------------------------------------------------------------

#[test]
fn dino_failure_dents_reputation() {
    let seed = 105;
    let mut gs = fresh_game(seed);
    assert_eq!(
        gs.competitors[0].company.reputation.total(),
        0.0,
        "seed {seed}: DinoSoar reputation should start at zero",
    );

    let bid_close = gs.date.add_days(5);
    inject_geo_solicitation(&mut gs, 9200, "FailSat", bid_close, 300_000_000.0);
    // No player bid: DinoSoar wins unopposed.

    let award_events = advance_through(&mut gs, bid_close, 30);
    let payment = award_events.iter().find_map(|e| match e {
        GameEvent::ContractAwardedToCompetitor { contract_name, amount, .. } if contract_name == "FailSat" =>
            Some(*amount),
        _ => None,
    }).unwrap_or_else(|| panic!("seed {seed}: expected DinoSoar to win FailSat, got {award_events:?}"));

    let money_before_award = gs.competitors[0].company.money;

    // Rig every flaw to certain activation, re-applying each day in
    // case manufacturing adds fresh (unrigged) inventory in the
    // meantime — first-match-in-vector-order launch selection favors
    // the original shelf stock, but this keeps the test honest either
    // way.
    let launch_lead = gs.balance.competitor.launch_lead_days;
    let mut success_seen = false;
    let mut failure_seen = false;
    for _ in 0..(launch_lead + 15) {
        rig_all_dino_flaws(&mut gs, 1.0);
        let events = gs.advance_day();
        for e in &events {
            if let GameEvent::CompetitorLaunch { company, contract_name, success } = e {
                if contract_name == "FailSat" {
                    assert_eq!(company, "DinoSoar", "seed {seed}: launch company mismatch");
                    if *success { success_seen = true } else { failure_seen = true }
                }
            }
        }
        if success_seen || failure_seen {
            break;
        }
    }
    assert!(
        failure_seen && !success_seen,
        "seed {seed}: expected a rigged CompetitorLaunch failure for FailSat (success_seen={success_seen})",
    );

    assert!(
        gs.competitors[0].company.reputation.total() < 0.0,
        "seed {seed}: DinoSoar reputation should be negative after a failure, got {}",
        gs.competitors[0].company.reputation.total(),
    );
    // No other income exists for a competitor besides contract
    // payments, so on failure money can only have gone sideways
    // (manufacturing/salary costs) or stayed flat relative to the
    // award, never up by the payment amount.
    assert!(
        gs.competitors[0].company.money < money_before_award + payment,
        "seed {seed}: money should not have increased by the withheld payment",
    );
}

// ---------------------------------------------------------------
// 6. A rigged success pays and boosts reputation.
// ---------------------------------------------------------------

#[test]
fn dino_success_pays_and_boosts_rep() {
    let seed = 106;
    let mut gs = fresh_game(seed);

    let bid_close = gs.date.add_days(5);
    inject_geo_solicitation(&mut gs, 9300, "WinSat", bid_close, 300_000_000.0);

    let award_events = advance_through(&mut gs, bid_close, 30);
    let payment = award_events.iter().find_map(|e| match e {
        GameEvent::ContractAwardedToCompetitor { contract_name, amount, .. } if contract_name == "WinSat" =>
            Some(*amount),
        _ => None,
    }).unwrap_or_else(|| panic!("seed {seed}: expected DinoSoar to win WinSat, got {award_events:?}"));

    let launch_lead = gs.balance.competitor.launch_lead_days;
    let mut success_seen = false;
    let mut failure_seen = false;
    let mut money_before_launch_day = gs.competitors[0].company.money;
    for _ in 0..(launch_lead + 15) {
        rig_all_dino_flaws(&mut gs, 0.0);
        // Snapshot right before the tick that may contain the launch,
        // so a same-day salary/order cost doesn't get folded into a
        // prior day's snapshot by mistake.
        money_before_launch_day = gs.competitors[0].company.money;
        let day_is_month_start = gs.date.add_days(1).is_first_of_month();
        let events = gs.advance_day();
        for e in &events {
            if let GameEvent::CompetitorLaunch { company, contract_name, success } = e {
                if contract_name == "WinSat" {
                    assert_eq!(company, "DinoSoar", "seed {seed}: launch company mismatch");
                    if *success { success_seen = true } else { failure_seen = true }
                    assert!(
                        !day_is_month_start,
                        "seed {seed}: launch landed on the 1st of a month (salary noise) \
                         — adjust bid_close so launch_lead_days doesn't land there",
                    );
                }
            }
        }
        if success_seen || failure_seen {
            break;
        }
    }
    assert!(
        success_seen && !failure_seen,
        "seed {seed}: expected a rigged CompetitorLaunch success for WinSat (failure_seen={failure_seen})",
    );

    assert!(
        gs.competitors[0].company.reputation.total() > 0.0,
        "seed {seed}: DinoSoar reputation should be positive after a success, got {}",
        gs.competitors[0].company.reputation.total(),
    );

    let delta = gs.competitors[0].company.money - money_before_launch_day;
    // Upper bound is exact: besides the contract payment, nothing
    // adds money to a competitor on a non-month-start day. Lower
    // bound leaves generous headroom for that same day's own
    // manufacturing order/material costs (a single rocket order tops
    // out far below this contract's payment).
    assert!(
        delta <= payment,
        "seed {seed}: same-day delta {delta} should never exceed the payment {payment}",
    );
    assert!(
        delta >= payment - 50_000_000.0,
        "seed {seed}: same-day delta {delta} should be close to the payment {payment} \
         (allowing headroom for same-day manufacturing costs)",
    );
}

// ---------------------------------------------------------------
// 7. Year-1 floor protection: DinoSoar never wins rideshare.
// ---------------------------------------------------------------

fn assert_no_rideshare_award(gs: &GameState, seed: u64, day: u32) {
    assert!(
        gs.competitors[0].company.active_contracts.iter().all(|c| c.market_id != MARKET_RIDESHARE),
        "seed {seed}, day {day}: DinoSoar should never hold a rideshare-market contract \
         (bid floor should keep it priced out of the small-payload market in year 1)",
    );
}

#[test]
fn dino_never_wins_year1_rideshare_20_seeds() {
    for seed in 1..=20u64 {
        let mut gs = fresh_game(seed);
        for day in 0..365u32 {
            gs.advance_day();
            assert_no_rideshare_award(&gs, seed, day);
        }
    }
}

#[test]
#[ignore = "full 200-seed check; run with `cargo test -- --ignored`"]
fn dino_never_wins_year1_rideshare_200_seeds() {
    for seed in 1..=200u64 {
        let mut gs = fresh_game(seed);
        for day in 0..365u32 {
            gs.advance_day();
            assert_no_rideshare_award(&gs, seed, day);
        }
    }
}

// ---------------------------------------------------------------
// 8. Save/load round-trip, including pre-M3 backfill.
// ---------------------------------------------------------------

fn temp_save_path(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join("rocket_tycoon_test");
    std::fs::create_dir_all(&dir).unwrap();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("competitor_dino_{tag}_{}_{n}.json", std::process::id()))
}

#[test]
fn competitor_survives_save_load() {
    let seed = 108;
    let mut gs = fresh_game(seed);
    for _ in 0..40 {
        gs.advance_day();
    }

    let before = &gs.competitors[0];
    let before_failure_rate = before.failure_rate;
    let before_money = before.company.money;
    let before_rocket_count = before.company.manufacturing.inventory.rockets.len();
    let before_scheduled = before.scheduled_launches.len();

    let path = temp_save_path("roundtrip");
    rocket_tycoon::save::save_game(&gs, &path).expect("seed 108: save should succeed");
    let loaded = rocket_tycoon::save::load_game(&path).expect("seed 108: load should succeed");

    assert_eq!(loaded.competitors.len(), 1, "seed {seed}: exactly one competitor after load");
    let after = &loaded.competitors[0];
    assert_eq!(after.failure_rate, before_failure_rate, "seed {seed}: failure_rate should survive round-trip");
    assert_eq!(after.company.money, before_money, "seed {seed}: money should survive round-trip");
    assert_eq!(
        after.company.manufacturing.inventory.rockets.len(),
        before_rocket_count,
        "seed {seed}: inventory rocket count should survive round-trip",
    );
    assert_eq!(
        after.scheduled_launches.len(),
        before_scheduled,
        "seed {seed}: scheduled_launches count should survive round-trip",
    );

    // Backfill: a save with no competitors (pre-M3, or one where we
    // manually clear it) gets DinoSoar realized fresh on load.
    let mut backfill_source = loaded;
    backfill_source.competitors.clear();
    let path2 = temp_save_path("backfill");
    rocket_tycoon::save::save_game(&backfill_source, &path2).expect("seed 108: second save should succeed");
    let reloaded = rocket_tycoon::save::load_game(&path2).expect("seed 108: second load should succeed");
    assert_eq!(
        reloaded.competitors.len(),
        1,
        "seed {seed}: pre-M3-shaped save (empty competitors) should get DinoSoar backfilled on load",
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&path2);
}

// ---------------------------------------------------------------
// 9. Disabled competitor never appears.
// ---------------------------------------------------------------

#[test]
fn disabled_competitor_never_appears() {
    let seed = 109;
    let mut balance = BalanceConfig::default();
    balance.competitor.enabled = false;
    let mut gs = GameState::with_balance("Test".into(), seed, balance);

    assert!(gs.competitors.is_empty(), "seed {seed}: no competitor should be realized when disabled");

    for day in 0..40 {
        gs.advance_day();
        assert!(
            gs.competitors.is_empty(),
            "seed {seed}, day {day}: disabled competitor should never appear during play",
        );
    }
}
