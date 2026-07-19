//! M3 Task 3: the standing-rule auto-bidder, locked end-to-end.
//!
//! `run_bid_rules` (called every `advance_day`) scans unbid
//! solicitations: for each one whose market carries an enabled
//! `BidRule`, it finds capable Testing-status player designs (payload
//! within `BID_PAYLOAD_MARGIN` of the physics cap), takes the
//! cheapest mean-of-last-5 marginal cost among designs with real
//! build-cost history, and — if free stock (inventory minus
//! accepted-unflown minus outstanding bids) covers it — places
//! `cost * (1 + margin)`, rounded to $10k, via the normal
//! `place_bid`/`resolve_bids` pipeline. This file locks: the pricing
//! formula, the mean-of-last-5 windowing, silence when a rule is
//! missing/disabled/costless, the free-stock overcommitment gate
//! (and its release once stock grows), the accepted-unflown
//! reservation, non-interference with manually-placed bids, the
//! award path through `resolve_bids`, save/load round-trip of
//! `bid_rules`, and the `BasicPolicy` / `policy_by_name` margin-syntax
//! wiring that installs these rules for a scripted bot.
//!
//! Pattern: borrow DinoSoar's realized company parts (a Testing
//! rocket project + matching engines + inventory) to give the player
//! company a capable design without waiting on real R&D, and inject
//! solicitations by hand so bid deadlines and budgets are fully under
//! test control. See `tests/bidding.rs` and `tests/competitor_dino.rs`
//! for the sibling patterns this borrows from.

use std::sync::atomic::{AtomicU64, Ordering};

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::calendar::GameDate;
use rocket_tycoon::competitor::realize_dinosoar;
use rocket_tycoon::contract::{
    Contract, ContractId, ContractStatus, MarketId, MARKET_GEO_COMSATS, MARKET_RIDESHARE,
};
use rocket_tycoon::event::GameEvent;
use rocket_tycoon::game_state::{BidRule, GameState};
use rocket_tycoon::policy::policy_by_name;
use rocket_tycoon::rocket_project::max_payload_to;

/// A fresh game with the scripted competitor disabled (so the player
/// is the sole bidder) and DinoSoar's realized "Brontosaur IV"
/// project + engines + 3-rocket inventory grafted onto the player
/// company. Gives the rule engine a real, physics-capable Testing
/// design to bid with, without waiting on R&D.
fn game_with_capable_player(seed: u64) -> GameState {
    let mut balance = BalanceConfig::default();
    balance.competitor.enabled = false;
    let mut gs = GameState::with_balance("Test".into(), seed, balance.clone());
    let dino = realize_dinosoar(&gs.seed, &balance);
    gs.player_company.rocket_projects = dino.company.rocket_projects.clone();
    gs.player_company.engine_projects = dino.company.engine_projects.clone();
    gs.player_company.manufacturing.inventory.rockets =
        dino.company.manufacturing.inventory.rockets.clone();
    gs
}

/// design_id / project_id of the borrowed "Brontosaur IV" project —
/// always `rocket_projects[0]` since it's the only project grafted on.
fn borrowed_design_ids(gs: &GameState) -> (rocket_tycoon::rocket::RocketDesignId, rocket_tycoon::rocket_project::RocketProjectId) {
    let rp = &gs.player_company.rocket_projects[0];
    (rp.design.id, rp.project_id)
}

/// Inject a bare-bones LEO solicitation (500 kg, generous ceiling,
/// bid window closing in 5 days) into `market_id`, under full test
/// control. Returns its index (always the back of the vec).
fn inject_contract(gs: &mut GameState, id: u64, name: &str, market_id: MarketId) -> usize {
    gs.available_contracts.push(Contract {
        id: ContractId(id),
        name: name.into(),
        destination: "leo".into(),
        payload_kg: 500.0,
        payment: 0.0,
        deadline: gs.date.add_days(300),
        status: ContractStatus::Available,
        market_id,
        campaign_id: None,
        bid_deadline: Some(gs.date.add_days(5)),
        budget_ceiling: 50_000_000.0,
        player_bid: None,
    });
    gs.available_contracts.len() - 1
}

/// Advance `gs` day by day (up to `max_days`), collecting every event
/// fired, until `gs.date` passes `deadline`. Panics if that never
/// happens, so a bug that skips resolution fails loudly instead of
/// silently passing an empty-events test.
fn advance_through(gs: &mut GameState, deadline: GameDate, max_days: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..max_days {
        all.extend(gs.advance_day());
        if gs.date > deadline {
            return all;
        }
    }
    panic!("resolution did not happen within {max_days} days of deadline {deadline}");
}

fn temp_save_path(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join("rocket_tycoon_test");
    std::fs::create_dir_all(&dir).unwrap();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("bid_rules_{tag}_{}_{n}.json", std::process::id()))
}

// ---------------------------------------------------------------
// 1. Sanity: the borrowed design is actually capable of LEO.
// ---------------------------------------------------------------

#[test]
fn sanity_borrowed_design_is_capable() {
    let gs = game_with_capable_player(1);
    let design = &gs.player_company.rocket_projects[0].design;
    let payload = max_payload_to(design, "earth_surface", "leo");
    assert!(
        payload > 1000.0,
        "borrowed Brontosaur IV should lift well over 1000 kg to LEO, got {payload:.0} \
         -- if this fails every other test's premise is broken",
    );
}

// ---------------------------------------------------------------
// 2. Rule places a bid at cost * (1 + margin).
// ---------------------------------------------------------------

#[test]
fn rule_places_bid_at_cost_plus_margin() {
    let mut gs = game_with_capable_player(2);
    let (design_id, _) = borrowed_design_ids(&gs);
    gs.player_company.rocket_cost_history.insert(design_id, vec![10_000_000.0]);
    gs.player_company.bid_rules.insert(
        MARKET_RIDESHARE,
        BidRule { enabled: true, margin: 0.5 },
    );
    let idx = inject_contract(&mut gs, 1, "Rideshare A", MARKET_RIDESHARE);

    let events = gs.advance_day();
    let bid_placed = events.iter().find_map(|e| match e {
        GameEvent::BidPlaced { contract_name, amount } if contract_name == "Rideshare A" => {
            Some(*amount)
        }
        _ => None,
    });
    assert_eq!(
        bid_placed,
        Some(15_000_000.0),
        "expected BidPlaced at $15M (cost $10M * 1.5) on the first tick, got events: {events:?}",
    );
    assert_eq!(
        gs.available_contracts[idx].player_bid,
        Some(15_000_000.0),
        "contract's player_bid should be set to the rule's computed bid",
    );
}

// ---------------------------------------------------------------
// 3. Mean of last 5 costs only -- older history doesn't count.
// ---------------------------------------------------------------

#[test]
fn mean_of_last_five_costs() {
    let mut gs = game_with_capable_player(3);
    let (design_id, _) = borrowed_design_ids(&gs);
    // Six entries; mean of the last 5 is exactly $10M (the leading
    // $20M outlier must be excluded).
    gs.player_company.rocket_cost_history.insert(
        design_id,
        vec![20_000_000.0, 10_000_000.0, 10_000_000.0, 10_000_000.0, 10_000_000.0, 10_000_000.0],
    );
    gs.player_company.bid_rules.insert(
        MARKET_RIDESHARE,
        BidRule { enabled: true, margin: 0.5 },
    );
    inject_contract(&mut gs, 1, "Rideshare A", MARKET_RIDESHARE);

    let events = gs.advance_day();
    let bid_placed = events.iter().find_map(|e| match e {
        GameEvent::BidPlaced { amount, .. } => Some(*amount),
        _ => None,
    });
    assert_eq!(
        bid_placed,
        Some(15_000_000.0),
        "mean of the last 5 entries (all $10M) at margin 0.5 should bid $15M; \
         if this is $16.5M the code is wrongly averaging all 6 entries. Got events: {events:?}",
    );
}

// ---------------------------------------------------------------
// 4. Disabled rule and missing rule both stay silent.
// ---------------------------------------------------------------

#[test]
fn disabled_rule_and_missing_rule_stay_silent() {
    let mut gs = game_with_capable_player(4);
    let (design_id, _) = borrowed_design_ids(&gs);
    gs.player_company.rocket_cost_history.insert(design_id, vec![10_000_000.0]);
    // MARKET_RIDESHARE: explicitly disabled.
    gs.player_company.bid_rules.insert(
        MARKET_RIDESHARE,
        BidRule { enabled: false, margin: 0.5 },
    );
    // MARKET_GEO_COMSATS: no rule entry at all.
    let idx_a = inject_contract(&mut gs, 1, "Rideshare A", MARKET_RIDESHARE);
    let idx_b = inject_contract(&mut gs, 2, "Comsat A", MARKET_GEO_COMSATS);

    let mut all_events = Vec::new();
    for _ in 0..4 {
        all_events.extend(gs.advance_day());
    }

    assert!(
        !all_events.iter().any(|e| matches!(e, GameEvent::BidPlaced { .. })),
        "no BidPlaced should fire for a disabled rule or a market with no rule, got: {all_events:?}",
    );
    assert_eq!(
        gs.available_contracts[idx_a].player_bid, None,
        "disabled-rule contract should stay unbid",
    );
    assert_eq!(
        gs.available_contracts[idx_b].player_bid, None,
        "no-rule contract should stay unbid",
    );
}

// ---------------------------------------------------------------
// 5. No cost history -> no bid, even with a capable design + enabled rule.
// ---------------------------------------------------------------

#[test]
fn no_cost_history_no_bid() {
    let mut gs = game_with_capable_player(5);
    // Deliberately do NOT populate rocket_cost_history.
    gs.player_company.bid_rules.insert(
        MARKET_RIDESHARE,
        BidRule { enabled: true, margin: 0.5 },
    );
    let idx = inject_contract(&mut gs, 1, "Rideshare A", MARKET_RIDESHARE);

    let events = gs.advance_day();
    assert!(
        !events.iter().any(|e| matches!(e, GameEvent::BidPlaced { .. })),
        "no build-cost history means no cost basis, so no bid should fire: {events:?}",
    );
    assert_eq!(
        gs.available_contracts[idx].player_bid, None,
        "contract should remain unbid without cost history",
    );
}

// ---------------------------------------------------------------
// 6. Free-stock gate blocks overcommitment, then releases once stock grows.
// ---------------------------------------------------------------

#[test]
fn gate_blocks_overcommitment() {
    let mut gs = game_with_capable_player(6);
    let (design_id, _) = borrowed_design_ids(&gs);
    gs.player_company.rocket_cost_history.insert(design_id, vec![10_000_000.0]);
    gs.player_company.bid_rules.insert(
        MARKET_RIDESHARE,
        BidRule { enabled: true, margin: 0.5 },
    );
    // Truncate the borrowed 3-rocket shelf down to exactly 1.
    gs.player_company.manufacturing.inventory.rockets.truncate(1);
    assert_eq!(gs.player_company.manufacturing.inventory.rockets.len(), 1);

    let idx_first = inject_contract(&mut gs, 1, "Rideshare A", MARKET_RIDESHARE);
    let idx_second = inject_contract(&mut gs, 2, "Rideshare B", MARKET_RIDESHARE);

    let events = gs.advance_day();
    let bids: Vec<_> = events.iter().filter_map(|e| match e {
        GameEvent::BidPlaced { contract_name, amount } => Some((contract_name.clone(), *amount)),
        _ => None,
    }).collect();
    assert_eq!(
        bids.len(), 1,
        "with only 1 rocket in stock, exactly one bid should fire; got {bids:?}",
    );
    assert_eq!(
        gs.available_contracts[idx_first].player_bid, Some(15_000_000.0),
        "the first contract in list order should be the one that gets bid",
    );
    assert_eq!(
        gs.available_contracts[idx_second].player_bid, None,
        "the second contract should stay unbid while stock is exhausted",
    );

    // Push a second rocket onto the shelf and advance another day.
    let item_id = gs.player_company.manufacturing.next_inventory_id();
    let mut extra = gs.player_company.manufacturing.inventory.rockets[0].clone();
    extra.item_id = item_id;
    gs.player_company.manufacturing.inventory.rockets.push(extra);

    let events2 = gs.advance_day();
    let bid_second = events2.iter().find_map(|e| match e {
        GameEvent::BidPlaced { contract_name, amount } if contract_name == "Rideshare B" => {
            Some(*amount)
        }
        _ => None,
    });
    assert_eq!(
        bid_second, Some(15_000_000.0),
        "once stock grows to 2, the previously-blocked second contract should get bid: {events2:?}",
    );
}

// ---------------------------------------------------------------
// 7. An accepted-but-unflown contract reserves stock too.
// ---------------------------------------------------------------

#[test]
fn accepted_unflown_contract_reserves_stock() {
    let mut gs = game_with_capable_player(7);
    let (design_id, _project_id) = borrowed_design_ids(&gs);
    gs.player_company.rocket_cost_history.insert(design_id, vec![10_000_000.0]);
    gs.player_company.bid_rules.insert(
        MARKET_RIDESHARE,
        BidRule { enabled: true, margin: 0.5 },
    );
    gs.player_company.manufacturing.inventory.rockets.truncate(1);

    // A previously-awarded contract sitting in active_contracts with
    // status Accepted, no matching flight in transit -- reserves the
    // one rocket in stock just like an outstanding bid would.
    gs.player_company.active_contracts.push(Contract {
        id: ContractId(999),
        name: "Already Awarded".into(),
        destination: "leo".into(),
        payload_kg: 500.0,
        payment: 12_000_000.0,
        deadline: gs.date.add_days(300),
        status: ContractStatus::Accepted,
        market_id: MARKET_RIDESHARE,
        campaign_id: None,
        bid_deadline: None,
        budget_ceiling: 0.0,
        player_bid: None,
    });
    let idx = inject_contract(&mut gs, 1, "Rideshare A", MARKET_RIDESHARE);

    let events = gs.advance_day();
    assert!(
        !events.iter().any(|e| matches!(e, GameEvent::BidPlaced { .. })),
        "the one rocket in stock is already reserved by the accepted-unflown contract, \
         so no bid should fire: {events:?}",
    );
    assert_eq!(
        gs.available_contracts[idx].player_bid, None,
        "solicitation should stay unbid while stock is fully reserved",
    );
}

// ---------------------------------------------------------------
// 8. Manual bid is never overridden by the rule engine.
// ---------------------------------------------------------------

#[test]
fn manual_bid_not_overridden() {
    let mut gs = game_with_capable_player(8);
    let (design_id, _) = borrowed_design_ids(&gs);
    gs.player_company.rocket_cost_history.insert(design_id, vec![10_000_000.0]);
    gs.player_company.bid_rules.insert(
        MARKET_RIDESHARE,
        BidRule { enabled: true, margin: 0.5 },
    );
    let idx = inject_contract(&mut gs, 1, "Rideshare A", MARKET_RIDESHARE);

    let manual = gs.place_bid(idx, 42_000_000.0);
    assert!(
        matches!(manual, Some(GameEvent::BidPlaced { amount, .. }) if amount == 42_000_000.0),
        "manual place_bid should succeed with the manual amount: {manual:?}",
    );

    let mut all_events = Vec::new();
    for _ in 0..4 {
        all_events.extend(gs.advance_day());
    }

    assert_eq!(
        gs.available_contracts[idx].player_bid,
        Some(42_000_000.0),
        "manual bid should never be overwritten by the rule engine",
    );
    let bid_placed_events: Vec<_> = all_events.iter().filter_map(|e| match e {
        GameEvent::BidPlaced { contract_name, amount } if contract_name == "Rideshare A" => {
            Some(*amount)
        }
        _ => None,
    }).collect();
    assert!(
        bid_placed_events.is_empty(),
        "the rule engine must skip a contract that already carries a bid -- no BidPlaced \
         should fire for it during advance_day (the manual one was placed directly via \
         place_bid, outside advance_day): got {bid_placed_events:?}",
    );
}

// ---------------------------------------------------------------
// 9. Rule bid wins the award end-to-end.
// ---------------------------------------------------------------

#[test]
fn rule_bid_wins_award_end_to_end() {
    let mut gs = game_with_capable_player(9);
    let (design_id, _) = borrowed_design_ids(&gs);
    gs.player_company.rocket_cost_history.insert(design_id, vec![10_000_000.0]);
    gs.player_company.bid_rules.insert(
        MARKET_RIDESHARE,
        BidRule { enabled: true, margin: 0.5 },
    );
    let idx = inject_contract(&mut gs, 1, "Rideshare A", MARKET_RIDESHARE);
    let deadline = gs.available_contracts[idx].bid_deadline.unwrap();

    let events = advance_through(&mut gs, deadline, 30);

    let awarded = events.iter().find_map(|e| match e {
        GameEvent::ContractAwarded { contract_name, amount } if contract_name == "Rideshare A" => {
            Some(*amount)
        }
        _ => None,
    });
    assert_eq!(
        awarded,
        Some(15_000_000.0),
        "expected ContractAwarded at $15M for the sole rule-placed bid: {events:?}",
    );
    let active = gs.player_company.active_contracts.iter()
        .find(|c| c.name == "Rideshare A")
        .expect("awarded contract should sit in active_contracts");
    assert_eq!(
        active.payment, 15_000_000.0,
        "active contract's payment should equal the winning rule bid",
    );
}

// ---------------------------------------------------------------
// 10. bid_rules survive a save/load round-trip.
// ---------------------------------------------------------------

#[test]
fn rules_survive_save_load() {
    let mut gs = game_with_capable_player(10);
    gs.player_company.bid_rules.insert(
        MARKET_RIDESHARE,
        BidRule { enabled: true, margin: 0.5 },
    );
    gs.player_company.bid_rules.insert(
        MARKET_GEO_COMSATS,
        BidRule { enabled: false, margin: 1.25 },
    );

    let path = temp_save_path("roundtrip");
    rocket_tycoon::save::save_game(&gs, &path).expect("save should succeed");
    let loaded = rocket_tycoon::save::load_game(&path).expect("load should succeed");

    assert_eq!(
        loaded.player_company.bid_rules, gs.player_company.bid_rules,
        "bid_rules should round-trip exactly through save/load",
    );
    assert_eq!(
        loaded.player_company.bid_rules.get(&MARKET_RIDESHARE),
        Some(&BidRule { enabled: true, margin: 0.5 }),
        "MARKET_RIDESHARE rule should round-trip exactly",
    );
    assert_eq!(
        loaded.player_company.bid_rules.get(&MARKET_GEO_COMSATS),
        Some(&BidRule { enabled: false, margin: 1.25 }),
        "MARKET_GEO_COMSATS rule should round-trip exactly",
    );

    std::fs::remove_file(&path).ok();
}

// ---------------------------------------------------------------
// 11. policy_by_name margin syntax.
// ---------------------------------------------------------------

#[test]
fn policy_by_name_margin_syntax() {
    assert!(
        policy_by_name("basic:0.15").is_some(),
        "basic:0.15 should parse to a policy",
    );
    assert!(
        policy_by_name("basic:abc").is_none(),
        "basic:abc should fail to parse as a margin",
    );
    assert!(
        policy_by_name("basic:-1").is_none(),
        "basic:-1 is outside the accepted margin range and should be rejected",
    );
    assert!(
        policy_by_name("basic").is_some(),
        "plain basic should still resolve to a policy",
    );
}

// ---------------------------------------------------------------
// 12. BasicPolicy installs standing rules for every market.
// ---------------------------------------------------------------

#[test]
fn basic_policy_installs_rules() {
    let mut gs = GameState::with_balance("Test".into(), 12, BalanceConfig::default());
    let mut policy = policy_by_name("basic").expect("basic policy should resolve");
    policy.act(&mut gs);

    assert!(
        !gs.markets.is_empty(),
        "sanity: default game should have at least one market",
    );
    for market in &gs.markets {
        let rule = gs.player_company.bid_rules.get(&market.id).unwrap_or_else(|| {
            panic!("BasicPolicy should install a bid rule for market {:?}", market.id)
        });
        assert!(
            rule.enabled,
            "BasicPolicy's installed rule for market {:?} should be enabled",
            market.id,
        );
    }
}
