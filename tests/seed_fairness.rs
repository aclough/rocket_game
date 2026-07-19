//! M1 Task 5: seed-fairness floor.
//!
//! Every seed must offer a viable first year: enough contracts, and
//! enough total contract value, in the payload/destination class a
//! starting company can actually win — its first vehicle demonstrably
//! lifts ~500 kg to LEO (see `policy::tests`). As of M3 every active
//! market is *visible* at reputation 0 (awards, not visibility, are
//! reputation-weighted), so "achievable" is a payload/destination
//! filter, not a visibility one. And since M3 Task 4 the floor is
//! *winnable*, not just offered: a reference-priced bid clears every
//! achievable ceiling, and `assert_year1_reference_bid_wins` proves
//! the award end-to-end with the competitor enabled. These floors
//! exist so no seed can be silently starved.
//!
//! Floors are set at the measured baseline minimum (default balance,
//! 200 seeds, 2026-07, re-measured after M3 Task 1 — per-market
//! world-query streams + deterministic Steady cadence: count min 5 /
//! median 6 / max 7, value min $12.29M / median $27.88M / max
//! $57.44M). The M3 stream changes made the floor nearly
//! deterministic: Steady markets issue contracts by volume
//! accumulation (no count draw to get unlucky on), and each market
//! draws from its own monthly stream, so no other market's volume
//! can shift rideshare's draws. Residual spread comes from the
//! economy cycle and payload/payment draws. Re-measure with
//! `measure_year1_distribution` (`cargo test --test seed_fairness --
//! --ignored --nocapture`) and update floors alongside any
//! market/balance change.

use std::collections::HashSet;

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::contract::Contract;
use rocket_tycoon::game_state::GameState;

/// Simulate one idle year (no company actions) and return every
/// contract that was offered at any point. Contracts expire after
/// 60+ days, so a daily scan of `available_contracts` misses nothing.
fn year1_offered_contracts(seed: u64) -> Vec<Contract> {
    year1_offered_with(seed, BalanceConfig::default())
}

fn year1_offered_with(seed: u64, balance: BalanceConfig) -> Vec<Contract> {
    let mut gs = GameState::with_balance("Probe".into(), seed, balance);
    let start_year = gs.date.year;
    let mut seen = HashSet::new();
    let mut offered = Vec::new();

    while gs.date.year == start_year {
        gs.advance_day();
        for c in &gs.available_contracts {
            if seen.insert(c.id) {
                offered.push(c.clone());
            }
        }
    }
    offered
}

/// The contract class a starting company can plausibly win: reachable
/// destination, payload within the demonstrated starter-vehicle class.
fn achievable(c: &Contract) -> bool {
    (c.destination == "leo" || c.destination == "sso") && c.payload_kg <= 500.0
}

fn assert_year1_floor(seed: u64) {
    let offered = year1_offered_contracts(seed);
    let achievable: Vec<&Contract> = offered.iter().filter(|c| achievable(c)).collect();
    let count = achievable.len();
    let value: f64 = achievable.iter().map(|c| c.payment).sum();

    assert!(
        count >= 4,
        "seed {seed}: only {count} achievable year-1 contracts (floor 4, baseline min 5)",
    );
    assert!(
        value >= 9_000_000.0,
        "seed {seed}: achievable year-1 contract value ${value:.0} below $9M floor (baseline min $12.29M)",
    );

    // Winnable, not just offered (M3 Task 4): a reference-priced bid
    // must clear every achievable solicitation's hidden ceiling. This
    // is structural (ceiling = payment × tolerance, tolerance ≥ 1.0)
    // — asserted so a future pricing change can't silently break it.
    // The other half of winnability — no competitor takes these — is
    // locked economically in tests/competitor_dino.rs (DinoSoar's bid
    // floor prices it out of the small-payload class) and end-to-end
    // by assert_year1_reference_bid_wins below.
    for c in &achievable {
        if c.is_solicitation() {
            assert!(
                c.payment <= c.budget_ceiling,
                "seed {seed}: achievable solicitation {} has reference ${:.0} above \
                 its ceiling ${:.0} — a reference bid could no longer win it",
                c.name, c.payment, c.budget_ceiling,
            );
        }
    }
}

/// End-to-end winnability: in a fresh default world (competitor
/// enabled), bidding the reference payment on the first achievable
/// solicitation must produce an award to the player — not a lapse,
/// not a rejection, not a competitor win.
fn assert_year1_reference_bid_wins(seed: u64) {
    use rocket_tycoon::event::GameEvent;

    let mut gs = GameState::with_balance("Probe".into(), seed, BalanceConfig::default());
    let start_year = gs.date.year;

    // Find the first achievable solicitation.
    let (name, bid_deadline) = loop {
        gs.advance_day();
        assert_eq!(
            gs.date.year, start_year,
            "seed {seed}: no achievable solicitation appeared in year 1",
        );
        let found = gs.available_contracts.iter().enumerate()
            .find(|(_, c)| achievable(c) && c.is_solicitation())
            .map(|(i, c)| (i, c.name.clone(), c.payment, c.bid_deadline.unwrap()));
        if let Some((idx, name, reference, bd)) = found {
            gs.place_bid(idx, reference)
                .expect("placing a reference bid on a solicitation must succeed");
            break (name, bd);
        }
    };

    // Run to resolution and demand the player award.
    let mut won = false;
    while gs.date <= bid_deadline {
        for evt in gs.advance_day() {
            match evt {
                GameEvent::ContractAwarded { contract_name, .. }
                    if contract_name == name => won = true,
                GameEvent::ContractAwardedToCompetitor { contract_name, company, .. }
                    if contract_name == name =>
                    panic!("seed {seed}: {company} took the floor contract {contract_name}"),
                GameEvent::BidRejected { contract_name }
                    if contract_name == name =>
                    panic!("seed {seed}: reference bid on {contract_name} was rejected"),
                _ => {}
            }
        }
    }
    assert!(won, "seed {seed}: reference bid on {name} never resolved to an award");
}

/// Cheap winnability check in normal `cargo test`.
#[test]
fn year1_reference_bid_wins_20_seeds() {
    for seed in 1..=20 {
        assert_year1_reference_bid_wins(seed);
    }
}

/// Full winnability check; run with `cargo test -- --ignored`.
#[test]
#[ignore = "full 200-seed winnability check; run with `cargo test -- --ignored`"]
fn year1_reference_bid_wins_200_seeds() {
    for seed in 1..=200 {
        assert_year1_reference_bid_wins(seed);
    }
}

/// The additive-only property, asserted directly (M2 Task 6,
/// strengthened in M3): the opening-floor markets' year-1 offering
/// must be byte-identical whether or not any other market exists.
/// Per-market world-query streams make this exact — removing every
/// non-opening-floor archetype from the config cannot change a
/// single draw in an opening-floor market's stream. Other markets
/// and variance layers may only ever ADD to the year-1 offering.
fn assert_year1_additive(seed: u64) {
    let full = year1_offered_contracts(seed);

    let mut floor_cfg = BalanceConfig::default();
    floor_cfg.markets.archetypes.retain(|a| {
        a.template.rep_target <= 0.0 && a.emergence.is_none()
    });
    assert!(
        !floor_cfg.markets.archetypes.is_empty(),
        "no opening-floor archetypes in the default config",
    );
    let floor_ids: HashSet<_> = floor_cfg.markets.archetypes.iter()
        .map(|a| a.template.id)
        .collect();
    let baseline = year1_offered_with(seed, floor_cfg);

    // Contract ids shift with other markets present, so compare by
    // content (name, payload, payment, deadlines).
    let key = |c: &Contract| {
        (c.name.clone(), c.payload_kg.to_bits(), c.payment.to_bits(),
         c.deadline, c.bid_deadline)
    };
    let floor_contracts: Vec<_> = full.iter()
        .filter(|c| floor_ids.contains(&c.market_id))
        .map(key)
        .collect();
    let baseline_contracts: Vec<_> = baseline.iter().map(key).collect();
    assert_eq!(
        floor_contracts, baseline_contracts,
        "seed {seed}: the rest of the market table altered the opening-floor \
         year-1 offering instead of only adding to it",
    );

    // And therefore the full offering's value can only be >= the
    // floor-only baseline's.
    let value = |cs: &[Contract]| cs.iter().map(|c| c.payment).sum::<f64>();
    assert!(
        value(&full) >= value(&baseline),
        "seed {seed}: full-config year-1 value below opening-floor baseline",
    );
}

/// Cheap additive-only check in normal `cargo test`.
#[test]
fn year1_additive_only_20_seeds() {
    for seed in 1..=20 {
        assert_year1_additive(seed);
    }
}

/// Full additive-only check; run with `cargo test -- --ignored`.
#[test]
#[ignore = "full 200-seed additive-only check; run with `cargo test -- --ignored`"]
fn year1_additive_only_200_seeds() {
    for seed in 1..=200 {
        assert_year1_additive(seed);
    }
}

/// Cheap floor check in normal `cargo test`.
#[test]
fn year1_floor_20_seeds() {
    for seed in 1..=20 {
        assert_year1_floor(seed);
    }
}

/// Full floor check; run with `cargo test -- --ignored`.
#[test]
#[ignore = "full 200-seed fairness check; run with `cargo test -- --ignored`"]
fn year1_floor_200_seeds() {
    for seed in 1..=200 {
        assert_year1_floor(seed);
    }
}

/// Not an assertion — prints the year-1 distribution so floors can be
/// re-derived after market/balance changes.
/// `cargo test --test seed_fairness -- --ignored --nocapture measure`
#[test]
#[ignore = "measurement helper, not a check"]
fn measure_year1_distribution() {
    let mut counts = Vec::new();
    let mut values = Vec::new();
    for seed in 1..=200 {
        let offered = year1_offered_contracts(seed);
        let ach: Vec<&Contract> = offered.iter().filter(|c| achievable(c)).collect();
        counts.push(ach.len());
        values.push(ach.iter().map(|c| c.payment).sum::<f64>());
        println!(
            "seed {seed:>3}: offered {:>2} total, {:>2} achievable, achievable value ${:.0}",
            offered.len(), ach.len(), ach.iter().map(|c| c.payment).sum::<f64>(),
        );
    }
    counts.sort_unstable();
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "achievable count min {} / median {} / max {}",
        counts[0], counts[counts.len() / 2], counts[counts.len() - 1],
    );
    println!(
        "achievable value min ${:.0} / median ${:.0} / max ${:.0}",
        values[0], values[values.len() / 2], values[values.len() - 1],
    );
}
