//! M3 Task 1: sealed-bid award mechanic guards.
//!
//! Locks the bidding pipeline end-to-end: `GameState::place_bid` and
//! `accept_contract` correctly gate solicitations vs. pre-priced
//! contracts, `resolve_bids` (run daily inside `advance_day` once the
//! date passes a contract's `bid_deadline`) awards a sole bid within
//! `budget_ceiling`, rejects one over it, and lets an unbid
//! solicitation lapse silently; bids stay revisable until the
//! deadline; and pre-M3 save data (contracts with no bid fields,
//! markets keyed by the old `min_reputation` name) still loads.

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::calendar::GameDate;
use rocket_tycoon::contract::{
    initial_markets, Contract, ContractId, ContractStatus, Market, MARKET_RIDESHARE,
};
use rocket_tycoon::event::GameEvent;
use rocket_tycoon::game_state::GameState;

/// Advance `gs` day by day (up to `max_days`) until a solicitation
/// appears in `available_contracts`, returning its index. Monthly
/// generation lands on the 1st of each month (first on Feb 1), and
/// GEO Comsats' Steady cadence guarantees at least one contract in
/// month 1, so 40 days is generous headroom.
fn advance_to_first_solicitation(gs: &mut GameState, max_days: u32) -> usize {
    for _ in 0..max_days {
        gs.advance_day();
        if let Some(i) = gs.available_contracts.iter().position(|c| c.is_solicitation()) {
            return i;
        }
    }
    panic!("no solicitation appeared within {max_days} days");
}

#[test]
fn bid_within_ceiling_wins_at_deadline() {
    let mut gs = GameState::with_balance("Test".into(), 1, BalanceConfig::default());
    let idx = advance_to_first_solicitation(&mut gs, 40);

    let name = gs.available_contracts[idx].name.clone();
    let payment = gs.available_contracts[idx].payment;
    let ceiling = gs.available_contracts[idx].budget_ceiling;
    let deadline = gs.available_contracts[idx]
        .bid_deadline
        .expect("solicitation must carry a bid_deadline");
    let destination = gs.available_contracts[idx].destination.clone();
    let payload_kg = gs.available_contracts[idx].payload_kg;

    // budget_tolerance is always >= 1.0, so a reference-priced bid is
    // guaranteed to clear the ceiling.
    assert!(
        payment <= ceiling,
        "seed 1: payment {payment} should be <= budget_ceiling {ceiling}",
    );
    let bid = payment;

    let placed = gs.place_bid(idx, bid);
    assert!(
        matches!(
            placed,
            Some(GameEvent::BidPlaced { ref contract_name, amount })
                if *contract_name == name && amount == bid
        ),
        "expected BidPlaced for `{name}` at {bid}, got {placed:?}",
    );

    let mut awarded: Option<(String, f64)> = None;
    while gs.date <= deadline {
        for e in gs.advance_day() {
            if let GameEvent::ContractAwarded { contract_name, amount } = e {
                if contract_name == name {
                    awarded = Some((contract_name, amount));
                }
            }
        }
    }

    let (awarded_name, awarded_amount) = awarded
        .unwrap_or_else(|| panic!("seed 1: expected ContractAwarded for `{name}` by {deadline}"));
    assert_eq!(awarded_name, name);
    assert_eq!(awarded_amount, bid);

    assert!(
        gs.available_contracts.iter().all(|c| c.name != name),
        "awarded contract `{name}` must be gone from available_contracts",
    );
    let active = gs
        .player_company
        .active_contracts
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("awarded contract `{name}` must be in active_contracts"));
    assert_eq!(active.payment, bid, "active contract payment must equal the winning bid");
    assert!(matches!(active.status, ContractStatus::Accepted));
    assert_eq!(active.campaign_id, None, "market-generated award is not a campaign mission");
    assert_eq!(active.destination, destination, "destination must survive the award unchanged");
    assert_eq!(active.payload_kg, payload_kg, "payload_kg must survive the award unchanged");
}

#[test]
fn bid_over_ceiling_is_rejected() {
    let mut gs = GameState::with_balance("Test".into(), 2, BalanceConfig::default());
    let idx = advance_to_first_solicitation(&mut gs, 40);

    let name = gs.available_contracts[idx].name.clone();
    let ceiling = gs.available_contracts[idx].budget_ceiling;
    let deadline = gs.available_contracts[idx]
        .bid_deadline
        .expect("solicitation must carry a bid_deadline");

    let bid = ceiling * 2.0;
    let placed = gs.place_bid(idx, bid);
    assert!(placed.is_some(), "an over-ceiling bid is still a valid bid to place");

    let mut rejected = false;
    let mut awarded = false;
    while gs.date <= deadline {
        for e in gs.advance_day() {
            match e {
                GameEvent::BidRejected { contract_name } if contract_name == name => rejected = true,
                GameEvent::ContractAwarded { contract_name, .. } if contract_name == name => awarded = true,
                _ => {}
            }
        }
    }

    assert!(rejected, "seed 2: expected BidRejected for `{name}` (bid {bid} > ceiling {ceiling})");
    assert!(!awarded, "an over-ceiling bid must never be awarded");

    assert!(
        gs.available_contracts.iter().all(|c| c.name != name),
        "rejected contract `{name}` must be gone from available_contracts",
    );
    assert!(
        gs.player_company.active_contracts.iter().all(|c| c.name != name),
        "rejected contract `{name}` must never reach active_contracts",
    );
}

#[test]
fn unbid_solicitations_lapse_silently() {
    let mut gs = GameState::with_balance("Test".into(), 3, BalanceConfig::default());

    let mut all_events = Vec::new();
    let mut month1_solicitation_ids: Vec<u64> = Vec::new();
    let mut captured_month1 = false;

    for _ in 0..70u32 {
        let events = gs.advance_day();
        all_events.extend(events);

        if !captured_month1 && gs.date.year == 2001 && gs.date.month == 2 && gs.date.day == 1 {
            month1_solicitation_ids = gs
                .available_contracts
                .iter()
                .filter(|c| c.is_solicitation())
                .map(|c| c.id.0)
                .collect();
            captured_month1 = true;
        }
    }

    assert!(captured_month1, "test must observe Feb 1, 2001 to snapshot month-1 solicitations");
    assert!(
        !month1_solicitation_ids.is_empty(),
        "seed 3: expected some solicitations issued on Feb 1",
    );

    for e in &all_events {
        assert!(
            !matches!(e, GameEvent::ContractAwarded { .. }),
            "no bids were ever placed: ContractAwarded must never fire, got {e:?}",
        );
        assert!(
            !matches!(e, GameEvent::BidRejected { .. }),
            "no bids were ever placed: BidRejected must never fire, got {e:?}",
        );
    }

    assert!(
        gs.player_company.active_contracts.is_empty(),
        "with no bids placed, nothing should have been awarded into active_contracts",
    );

    // Delivery-deadline windows (60-360 days) are all longer than the
    // 30-day bid window, so a month-1 solicitation's disappearance by
    // day 70 can only be the silent bid lapse, not delivery expiry.
    for id in &month1_solicitation_ids {
        assert!(
            gs.available_contracts.iter().all(|c| c.id.0 != *id),
            "month-1 solicitation id {id} should have lapsed (30-day bid window) by day 70",
        );
    }
}

#[test]
fn accept_refuses_solicitations_and_bid_refuses_prepriced() {
    let mut gs = GameState::with_balance("Test".into(), 4, BalanceConfig::default());
    let idx = advance_to_first_solicitation(&mut gs, 40);

    assert!(
        gs.accept_contract(idx).is_none(),
        "accept_contract must refuse a solicitation",
    );
    assert!(
        gs.place_bid(idx, -5.0).is_none(),
        "place_bid must refuse a negative bid",
    );
    assert!(
        gs.place_bid(idx, 0.0).is_none(),
        "place_bid must refuse a zero bid",
    );

    // Construct a pre-priced contract (bid_deadline: None) directly,
    // mimicking a campaign mission / pre-M3 contract.
    gs.available_contracts.push(Contract {
        id: ContractId(9999),
        name: "Legacy Pre-Priced Contract".into(),
        destination: "leo".into(),
        payload_kg: 500.0,
        payment: 2_000_000.0,
        deadline: gs.date.add_days(90),
        status: ContractStatus::Available,
        market_id: MARKET_RIDESHARE,
        campaign_id: None,
        bid_deadline: None,
        budget_ceiling: 0.0,
        player_bid: None,
    });
    let pre_priced_idx = gs.available_contracts.len() - 1;

    assert!(
        gs.place_bid(pre_priced_idx, 1_000_000.0).is_none(),
        "place_bid must refuse a pre-priced (bid_deadline: None) contract",
    );

    let evt = gs.accept_contract(pre_priced_idx);
    assert!(
        matches!(
            evt,
            Some(GameEvent::ContractAccepted { ref contract_name })
                if contract_name == "Legacy Pre-Priced Contract"
        ),
        "accept_contract must accept a pre-priced contract, got {evt:?}",
    );
    assert!(
        gs.player_company
            .active_contracts
            .iter()
            .any(|c| c.name == "Legacy Pre-Priced Contract"),
        "accepted pre-priced contract must land in active_contracts",
    );
}

#[test]
fn bids_are_revisable_until_deadline() {
    let mut gs = GameState::with_balance("Test".into(), 5, BalanceConfig::default());
    let idx = advance_to_first_solicitation(&mut gs, 40);

    let name = gs.available_contracts[idx].name.clone();
    let ceiling = gs.available_contracts[idx].budget_ceiling;
    let deadline = gs.available_contracts[idx]
        .bid_deadline
        .expect("solicitation must carry a bid_deadline");

    let first_bid = ceiling * 0.5;
    let second_bid = ceiling * 0.7;
    assert!(first_bid < second_bid && second_bid <= ceiling);

    gs.place_bid(idx, first_bid);
    assert_eq!(
        gs.available_contracts[idx].player_bid,
        Some(first_bid),
        "player_bid must reflect the first placed bid",
    );

    gs.place_bid(idx, second_bid);
    assert_eq!(
        gs.available_contracts[idx].player_bid,
        Some(second_bid),
        "player_bid must reflect the most recent (revised) bid",
    );

    let mut awarded_amount = None;
    while gs.date <= deadline {
        for e in gs.advance_day() {
            if let GameEvent::ContractAwarded { contract_name, amount } = e {
                if contract_name == name {
                    awarded_amount = Some(amount);
                }
            }
        }
    }

    assert_eq!(
        awarded_amount,
        Some(second_bid),
        "seed 5: award must pay the last bid placed, not an earlier revision",
    );
}

#[test]
fn legacy_contract_json_loads_and_accepts() {
    // Mimics a pre-M3 save: only the fields that existed before the
    // M3 bid_deadline/budget_ceiling/player_bid fields were added.
    let json = r#"{
        "id": 42,
        "name": "Legacy ComSat Delivery",
        "destination": "gto",
        "payload_kg": 3000.0,
        "payment": 45000000.0,
        "deadline": {"year": 2001, "month": 6, "day": 1},
        "status": "Available",
        "market_id": 1
    }"#;

    let contract: Contract = serde_json::from_str(json)
        .expect("pre-M3 Contract JSON (no bid fields) must still deserialize");

    assert_eq!(contract.id.0, 42);
    assert_eq!(contract.name, "Legacy ComSat Delivery");
    assert_eq!(contract.destination, "gto");
    assert_eq!(contract.payload_kg, 3000.0);
    assert_eq!(contract.payment, 45_000_000.0);
    assert_eq!(contract.deadline, GameDate::new(2001, 6, 1));
    assert!(matches!(contract.status, ContractStatus::Available));
    assert_eq!(contract.market_id.0, 1);

    assert!(
        contract.bid_deadline.is_none(),
        "legacy contract must default bid_deadline to None",
    );
    assert!(
        !contract.is_solicitation(),
        "a legacy (bid_deadline: None) contract must not be treated as a solicitation",
    );
    assert_eq!(
        contract.budget_ceiling, 0.0,
        "legacy contract must default budget_ceiling to 0.0",
    );
    assert!(contract.player_bid.is_none());

    // And it flows through the pre-priced accept path like any other
    // bid_deadline: None contract.
    let mut gs = GameState::with_balance("Test".into(), 6, BalanceConfig::default());
    gs.available_contracts.push(contract);
    let idx = gs.available_contracts.len() - 1;

    assert!(
        gs.place_bid(idx, 1_000_000.0).is_none(),
        "legacy pre-priced contract must refuse bids",
    );
    let evt = gs.accept_contract(idx);
    assert!(
        matches!(
            evt,
            Some(GameEvent::ContractAccepted { ref contract_name })
                if contract_name == "Legacy ComSat Delivery"
        ),
        "legacy pre-priced contract must still accept directly, got {evt:?}",
    );
}

#[test]
fn legacy_market_min_reputation_alias_loads() {
    let market = initial_markets().remove(0); // GEO Comsats, rep_target 50.0
    assert_eq!(market.rep_target, 50.0, "test assumption: GEO's rep_target is 50.0");

    let mut value = serde_json::to_value(&market).expect("Market must serialize to JSON");
    let obj = value.as_object_mut().expect("Market must serialize to a JSON object");
    let rep = obj
        .remove("rep_target")
        .expect("serialized Market must have a rep_target key");
    obj.insert("min_reputation".to_string(), rep);

    let reloaded: Market = serde_json::from_value(value)
        .expect("Market JSON keyed by the legacy `min_reputation` name must still deserialize");

    assert_eq!(
        reloaded.rep_target, 50.0,
        "the `min_reputation` alias must populate rep_target",
    );
    assert_eq!(reloaded.id, market.id);
    assert_eq!(reloaded.name, market.name);
}
