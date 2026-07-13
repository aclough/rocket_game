//! M1 Task 4: determinism smoke test + metric-band regression tests.
//!
//! Bands are set around the measured baseline (basic policy, default
//! balance, 200 seeds × 8 years, 2026-07, re-measured after the M2
//! market-archetype layer, volume growth, and contract character
//! axes): 0/200 bankrupt, 23–30 launches per seed, per-seed success
//! ≥ 89%, first profitable year within 5 years of start, min money
//! ≥ $85M, final money ≥ $387M. Bands are
//! regression protection around observed reality, not aspirations —
//! M4 retunes them.
//!
//! When changing balance values or game constants, re-measure with
//! `cargo run --release --bin simulate -- --seeds 1..200 --years 8
//! --policy basic --summary-only` and update these bands in the same
//! change.

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::policy::policy_by_name;
use rocket_tycoon::sim::{run_seed, RunSummary};

fn run(seed: u64, years: u32) -> (RunSummary, Vec<String>) {
    let balance = BalanceConfig::default();
    let mut policy = policy_by_name("basic").expect("basic policy exists");
    let mut rows = Vec::new();
    let summary = run_seed(seed, years, &balance, policy.as_mut(), |row| {
        rows.push(row.to_string())
    });
    (summary, rows)
}

/// Same seed + same policy twice must produce byte-identical monthly
/// metrics. Guards against HashMap-iteration order and wall-clock
/// leaks anywhere in the sim or policy.
#[test]
fn same_seed_same_policy_is_byte_deterministic() {
    let (s1, rows1) = run(42, 4);
    let (s2, rows2) = run(42, 4);
    assert_eq!(rows1, rows2, "monthly metric rows diverged between identical runs");
    assert_eq!(s1.final_money, s2.final_money);
    assert_eq!(s1.launches, s2.launches);
}

fn assert_bands(summaries: &[RunSummary]) {
    let starting_money = BalanceConfig::default().costs.starting_money;
    let mut launches = 0usize;
    let mut successes = 0usize;

    for s in summaries {
        assert!(!s.bankrupt, "seed {}: went bankrupt (final ${:.0})", s.seed, s.final_money);
        assert!(
            s.min_money > 0.0,
            "seed {}: money went negative (min ${:.0})", s.seed, s.min_money,
        );
        assert!(
            s.final_money > starting_money,
            "seed {}: not profitable after run (final ${:.0} <= starting ${:.0})",
            s.seed, s.final_money, starting_money,
        );
        assert!(
            (15..=45).contains(&s.launches),
            "seed {}: {} launches outside band 15..=45 (baseline 23..=30)",
            s.seed, s.launches,
        );
        let rate = s.successes as f64 / s.launches as f64;
        assert!(
            rate >= 0.85,
            "seed {}: launch success rate {:.0}% below 85% (baseline min 93%)",
            s.seed, rate * 100.0,
        );
        let fpy = s.first_profitable_year.unwrap_or_else(|| {
            panic!("seed {}: never had a profitable year", s.seed)
        });
        assert!(
            fpy <= s.start_year + 5,
            "seed {}: first profitable year {} later than start+5 (baseline max start+5)",
            s.seed, fpy,
        );
        launches += s.launches;
        successes += s.successes;
    }

    let aggregate = successes as f64 / launches as f64;
    assert!(
        aggregate >= 0.95,
        "aggregate launch success rate {:.1}% below 95% (baseline 98.6%)",
        aggregate * 100.0,
    );
}

/// Cheap band check that runs in normal `cargo test` (~4s debug).
#[test]
fn metric_bands_20_seeds() {
    let summaries: Vec<RunSummary> = (1..=20).map(|seed| run(seed, 8).0).collect();
    assert_bands(&summaries);
}

/// Full baseline reproduction; run explicitly with
/// `cargo test -- --ignored` (~40s debug, ~4s release).
#[test]
#[ignore = "full 200-seed band check; run with `cargo test -- --ignored`"]
fn metric_bands_200_seeds() {
    let summaries: Vec<RunSummary> = (1..=200).map(|seed| run(seed, 8).0).collect();
    assert_bands(&summaries);
}
