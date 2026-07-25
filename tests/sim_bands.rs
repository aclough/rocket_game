//! M1 Task 4: determinism smoke test + metric-band regression tests.
//!
//! Bands are set around the measured baseline (basic policy, default
//! balance, 200 seeds × 8 years, 2026-07, re-measured after the M4
//! Task 2 cost retune — material prices ×4, DinoSoar margins 3-8×,
//! bot margin cost×2 — which put marginal vehicle cost at ~50% of a
//! winning bid with payments unchanged at real-world prices):
//! 1/200 bankrupt (seed 169), 5/200 dip below $0 mid-run and mostly
//! recover, 4–31 launches per seed, per-seed success ≥ 75%, aggregate
//! success 95.4%, median min money $102M with 183/200 keeping min
//! above $25M, 117/200 end above starting money, 200/200 have a first
//! profitable year (latest start+7). The game is intentionally
//! tighter than pre-M4 (roguelike ruin is possible); Task 5 of the M4
//! plan retargets these bands after the dev-time/risk retune. The
//! margin-sweep context still applies (see policy.rs
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
            s.min_money > -60_000_000.0,
            "seed {}: runaway debt below -$60M (min ${:.0}, baseline low -$38.6M)",
            s.seed, s.min_money,
        );
        if s.min_money > 25_000_000.0 {
            min_above_25m += 1;
        }
        if s.final_money > starting_money {
            profitable += 1;
        }
        assert!(
            (3..=38).contains(&s.launches),
            "seed {}: {} launches outside band 3..=38 (baseline 4..=31; the \
             top is a seed that wins a big block program)",
            s.seed, s.launches,
        );
        let rate = s.successes as f64 / s.launches as f64;
        assert!(
            rate >= 0.70,
            "seed {}: launch success rate {:.0}% below 70% (baseline min 75%; \
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

    // Fleet-level bands (baseline 117/200 end above starting money,
    // 183/200 keep min money above $25M, 1/200 bankrupt, 200/200 have
    // a profitable year).
    let n = summaries.len() as f64;
    assert!(
        bankrupt as f64 / n <= 0.015,
        "{bankrupt}/{n} seeds bankrupt (band <= 1.5%, baseline 0.5%; Task 5 \
         retargets to the 2-4/100 roguelike goal)",
    );
    assert!(
        min_above_25m as f64 / n >= 0.85,
        "only {min_above_25m}/{n} seeds kept min money above $25M (band >= 85%, \
         baseline 91.5%)",
    );
    assert!(
        profitable as f64 / n >= 0.55,
        "only {profitable}/{n} seeds profitable after run (band >= 55%, baseline 58.5%)",
    );
    assert!(
        with_fpy as f64 / n >= 0.95,
        "only {with_fpy}/{n} seeds ever had a profitable year (band >= 95%, baseline 100%)",
    );

    let aggregate = successes as f64 / launches as f64;
    assert!(
        aggregate >= 0.93,
        "aggregate launch success rate {:.1}% below 93% (baseline 95.4%)",
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
