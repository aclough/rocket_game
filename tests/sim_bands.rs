//! M1 Task 4: determinism smoke test + metric-band regression tests.
//!
//! Bands are set around the measured baseline (basic policy, default
//! balance, 200 seeds × 8 years, 2026-07, re-measured after M3
//! Task 3 — the bot now prices blind through the standing-rule
//! engine at its default margin instead of reading the hidden
//! reference payment, keeps 2 rockets on the shelf, and eats real
//! rejections when its bid tops an unseen ceiling): 0/200 bankrupt,
//! 4–22 launches per seed (avg 12.9), per-seed success ≥ 75%,
//! aggregate success 95.2%, min money $71.9M, 193/200 seeds end
//! above starting money, 200/200 have a first profitable year
//! (latest start+6). The launch count fell and the success floor
//! loosened versus the interim reference-price bot — honest blind
//! pricing wastes bid windows on rejections and flies fewer, so
//! early-flight failures weigh more per seed. The 2026-07 margin
//! sweep (see policy.rs DEFAULT_BID_MARGIN) is the context: an
//! uncontested market rewards ever-higher margins, so these bands
//! lock a chosen honest posture, not an optimum. Bands are
//! regression protection around observed reality, not aspirations.
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

    for s in summaries {
        assert!(!s.bankrupt, "seed {}: went bankrupt (final ${:.0})", s.seed, s.final_money);
        assert!(
            s.min_money > 65_000_000.0,
            "seed {}: money dipped below $65M (min ${:.0}, baseline min $71.9M)",
            s.seed, s.min_money,
        );
        if s.final_money > starting_money {
            profitable += 1;
        }
        assert!(
            (3..=28).contains(&s.launches),
            "seed {}: {} launches outside band 3..=28 (baseline 4..=22)",
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
                "seed {}: first profitable year {} later than start+7 (baseline max start+6)",
                s.seed, fpy,
            );
        }
        launches += s.launches;
        successes += s.successes;
    }

    // Fleet-level bands (baseline 193/200 end above starting money,
    // 200/200 have a profitable year).
    let n = summaries.len() as f64;
    assert!(
        profitable as f64 / n >= 0.90,
        "only {profitable}/{n} seeds profitable after run (band >= 90%, baseline 96.5%)",
    );
    assert!(
        with_fpy as f64 / n >= 0.95,
        "only {with_fpy}/{n} seeds ever had a profitable year (band >= 95%, baseline 100%)",
    );

    let aggregate = successes as f64 / launches as f64;
    assert!(
        aggregate >= 0.93,
        "aggregate launch success rate {:.1}% below 93% (baseline 95.2%)",
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
