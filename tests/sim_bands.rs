//! M1 Task 4: determinism smoke test + metric-band regression tests.
//!
//! Bands are set around the measured baseline (basic policy, default
//! balance, 200 seeds × 8 years, 2026-07, re-measured after the M4
//! Task 3 dev-time/risk retune — design work × (complexity/5)^2.5,
//! engine build ^1.5, flaw ground discovery uniform^2 × sqrt(act),
//! and at most one rocket-destroying flaw discovered per launch —
//! on top of the Task 2 cost retune):
//! 4/200 bankrupt (seeds 45/58/88/165 — the 2-4/100 roguelike band),
//! 9/200 survivors dip below $0 and recover (worst -$89.7M), 4-25
//! launches per seed, per-seed success ≥ 69%, aggregate success
//! 92.6%, median survivor min money $88M with 161/200 keeping min
//! above $25M, 77/200 end above starting money, 199/200 have a first
//! profitable year (latest start+7). First launch averages month
//! 17.5 with ~8 undiscovered flaws aboard; dev spend to first launch
//! ~$69M. Task 5 of the M4 plan does the final difficulty
//! calibration. The margin-sweep context still applies (see policy.rs
//! DEFAULT_BID_MARGIN): the uncontested small-payload market rewards
//! higher margins, so these bands lock a chosen honest posture, not
//! an optimum. Bands are regression protection around observed
//! reality, not aspirations.
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
    let mut profitable = 0usize;
    let mut with_fpy = 0usize;
    let mut bankrupt = 0usize;
    let mut min_above_25m = 0usize;

    for s in summaries {
        if s.bankrupt {
            bankrupt += 1;
        }
        assert!(
            s.min_money > -120_000_000.0,
            "seed {}: runaway debt below -$120M (min ${:.0}, baseline low -$99.2M)",
            s.seed, s.min_money,
        );
        if s.min_money > 25_000_000.0 {
            min_above_25m += 1;
        }
        if s.final_money > starting_money {
            profitable += 1;
        }
        assert!(
            (3..=30).contains(&s.launches),
            "seed {}: {} launches outside band 3..=30 (baseline 4..=25; the \
             top is a seed that wins a big block program)",
            s.seed, s.launches,
        );
        let rate = s.successes as f64 / s.launches as f64;
        assert!(
            rate >= 0.65,
            "seed {}: launch success rate {:.0}% below 65% (baseline min 69%; \
             low-launch seeds make this floor noisy)",
            s.seed, rate * 100.0,
        );
        if let Some(fpy) = s.first_profitable_year {
            with_fpy += 1;
            assert!(
                fpy <= s.start_year + 7,
                "seed {}: first profitable year {} later than start+7 (baseline max start+7)",
                s.seed, fpy,
            );
        }
        launches += s.launches;
        successes += s.successes;
    }

    // Fleet-level bands (baseline 77/200 end above starting money,
    // 161/200 keep min money above $25M, 4/200 bankrupt, 199/200 have
    // a profitable year).
    let n = summaries.len() as f64;
    assert!(
        bankrupt as f64 / n <= 0.035,
        "{bankrupt}/{n} seeds bankrupt (band <= 3.5%, baseline 2.0% — inside \
         the agreed 2-4/100 roguelike goal; Task 5 does final calibration)",
    );
    assert!(
        min_above_25m as f64 / n >= 0.72,
        "only {min_above_25m}/{n} seeds kept min money above $25M (band >= 72%, \
         baseline 80.5%)",
    );
    assert!(
        profitable as f64 / n >= 0.30,
        "only {profitable}/{n} seeds profitable after run (band >= 30%, baseline 38.5%)",
    );
    assert!(
        with_fpy as f64 / n >= 0.95,
        "only {with_fpy}/{n} seeds ever had a profitable year (band >= 95%, baseline 99.5%)",
    );

    let aggregate = successes as f64 / launches as f64;
    assert!(
        aggregate >= 0.90,
        "aggregate launch success rate {:.1}% below 90% (baseline 92.6%)",
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
