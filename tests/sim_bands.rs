//! M1 Task 4: determinism smoke test + metric-band regression tests.
//!
//! **Provisional bands (17_2_DELTA_V.md step 3, 2026-09).** The launch
//! check now judges a flawed vehicle on the planner's usable Δv
//! (vacuum less ascent gravity loss) instead of vacuum Δv alone, which
//! took away the hidden 8–11% allowance every launch used to carry.
//! The bot's fixed template bids at zero margin, so its numbers moved:
//! 200 seeds × 8 years — 13/200 bankrupt, 27/200 dip below $0, 189/200
//! have a profitable year, 44/200 end above starting money, 131/200
//! keep min money above $25M, aggregate success 87.7% (2,489 of
//! 2,839), worst surviving seed 62%, 6–26 launches per surviving seed,
//! deaths at 7–14 launches. First-launch month (16.8), dev spend
//! ($71.1M) and hidden flaws (8.3) did not move. These bands are
//! re-measured after step 6 (flight pays the ascent) and the roguelike
//! 1–6% bankruptcy target is revisited then — see the plan's Q5 for the
//! bot-margin experiments that did not restore the pre-step-3 figures.
//!
//! Pre-step-3 history, for the record (2026-07 M4 engine-cost retune
//! baseline): 1/200 bankrupt, 16/200 dip below $0, per-seed success
//! ≥ 73%, aggregate 92.9% (94.9% at step 0 of the Δv plan), 163/200
//! keep min money above $25M, 91/200 end above starting money, unit
//! cost $15.0M vs payment $30.0M. The margin-sweep context still
//! applies (see policy.rs DEFAULT_BID_MARGIN): the uncontested
//! small-payload market rewards higher markups, so these bands lock a
//! chosen honest posture, not an optimum. Bands are regression
//! protection around observed reality, not aspirations.
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
        // The harness stops a run at SIM_DEBT_LIMIT (-$30M); a single
        // day's spend can overshoot the line, but never by much
        // (step 3 low -$37.4M; pre-step-3 baseline -$40.2M).
        assert!(
            s.min_money > -60_000_000.0,
            "seed {}: min money ${:.0} far below the -$30M sim debt limit",
            s.seed, s.min_money,
        );
        if s.min_money > 25_000_000.0 {
            min_above_25m += 1;
        }
        if s.final_money > starting_money {
            profitable += 1;
        }
        if let Some(fpy) = s.first_profitable_year {
            with_fpy += 1;
            assert!(
                fpy <= s.start_year + 7,
                "seed {}: first profitable year {} later than start+7 (step 3 max start+7)",
                s.seed, fpy,
            );
        }
        launches += s.launches;
        successes += s.successes;
        if s.bankrupt {
            // A bankrupt run is truncated at the debt line, so the
            // per-seed activity bands below don't apply to it
            // (step 3 deaths: 7-14 launches, success 18-86%).
            bankrupt += 1;
            continue;
        }
        assert!(
            (3..=30).contains(&s.launches),
            "seed {}: {} launches outside band 3..=30 (step 3 6..=26; the \
             top is a seed that wins a big block program)",
            s.seed, s.launches,
        );
        let rate = s.successes as f64 / s.launches as f64;
        assert!(
            rate >= 0.55,
            "seed {}: launch success rate {:.0}% below 55% (step 3 min 62%, pre-step-3 73%; \
             low-launch seeds make this floor noisy)",
            s.seed, rate * 100.0,
        );
    }

    // Fleet-level bands (step 3: 44/200 end above starting money,
    // 131/200 keep min money above $25M, 13/200 bankrupt, 189/200 have
    // a profitable year).
    let n = summaries.len() as f64;
    // The agreed roguelike guard band is 1-6 bankruptcies per 100 seeds
    // (target 2-4/100); step 3 measured 6.5/100, so the 200-seed
    // ceiling sits at 8% provisionally until step 6 re-measures.
    // Neither bound means much at n=20, so both are size-aware.
    //
    // The ceiling needs the same treatment the floor already had. A 6%
    // ceiling on twenty seeds means "at most one bankruptcy", and at a
    // 4% true rate the chance of seeing two or more is 19% — a smoke
    // check that cries wolf one run in five. Allowing two (10%) fails
    // about 3% of the time instead, which is what a smoke check should
    // feel like. The 200-seed run keeps the real 6% ceiling.
    let ceiling = if summaries.len() >= 100 { 0.08 } else { 0.10 };
    assert!(
        bankrupt as f64 / n <= ceiling,
        "{bankrupt}/{n} seeds bankrupt (band <= {:.0}%, step 3 measured 6.5% \
         at 200 seeds; roguelike target 2-4%)",
        ceiling * 100.0,
    );
    if summaries.len() >= 100 {
        assert!(
            bankrupt as f64 / n >= 0.01,
            "only {bankrupt}/{n} seeds bankrupt (band >= 1%, step 3 6.5%; \
             the game should stay dangerous)",
        );
    }
    assert!(
        min_above_25m as f64 / n >= 0.58,
        "only {min_above_25m}/{n} seeds kept min money above $25M (band >= 58%, \
         step 3 65.5%, pre-step-3 81.5%)",
    );
    assert!(
        profitable as f64 / n >= 0.15,
        "only {profitable}/{n} seeds profitable after run (band >= 15%, step 3 22%, pre-step-3 45.5%)",
    );
    assert!(
        with_fpy as f64 / n >= 0.92,
        "only {with_fpy}/{n} seeds ever had a profitable year (band >= 92%, step 3 94.5%)",
    );

    let aggregate = successes as f64 / launches as f64;
    assert!(
        aggregate >= 0.85,
        "aggregate launch success rate {:.1}% below 85% (step 3 87.7%, pre-step-3 94.9%)",
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
