//! M1 Task 5: seed-fairness floor.
//!
//! Every seed must offer a viable first year: enough contracts, and
//! enough total contract value, in the payload/destination class a
//! starting company can actually win. A fresh company has reputation
//! 0, so only markets with `min_reputation <= 0` offer it anything,
//! and its first vehicle demonstrably lifts ~500 kg to LEO (see
//! `policy::tests`). These floors exist so that when M2 adds
//! per-seed market variance, no seed can be silently starved.
//!
//! Floors are set at the measured baseline minimum (default balance,
//! 200 seeds, 2026-07, re-measured after M2 volume growth: count
//! min 3 / median 6 / max 11, value min $6.97M / median $28.9M / max
//! $62.6M — the thin seeds are year-1 recessions suppressing
//! rideshare volume). Re-measure with
//! `measure_year1_distribution` (`cargo test --test seed_fairness --
//! --ignored --nocapture`) and update floors alongside any
//! market/balance change. The 10x min-to-max disparity is an M4
//! balance question, not a bug — see ROADMAP.md.

use std::collections::HashSet;

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::contract::Contract;
use rocket_tycoon::game_state::GameState;

/// Simulate one idle year (no company actions) and return every
/// contract that was offered at any point. Contracts expire after
/// 60+ days, so a daily scan of `available_contracts` misses nothing.
fn year1_offered_contracts(seed: u64) -> Vec<Contract> {
    let mut gs = GameState::with_balance("Probe".into(), seed, BalanceConfig::default());
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
        count >= 2,
        "seed {seed}: only {count} achievable year-1 contracts (floor 2, baseline min 2)",
    );
    assert!(
        value >= 5_000_000.0,
        "seed {seed}: achievable year-1 contract value ${value:.0} below $5M floor (baseline min $6.05M)",
    );
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
