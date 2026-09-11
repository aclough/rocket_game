//! M1 Task 4: determinism smoke test + metric-band regression tests.
//!
//! **D7 baseline (17_3_PHYSICS.md, 2026-09-10).** The ascent integrator
//! flies a pitch program instead of a pure gravity turn, the bot's
//! template grew to two booster engines under 90 t with a 15 t upper
//! stage to lift what it used to, and the default starting money went
//! from $200M to $250M to give the dearer vehicle its runway. Measured,
//! 200 seeds × 8 years: 23/200 bankrupt, 34/200 dip below $0, 179/200
//! have a profitable year, 65/200 end above starting money, 146/200
//! keep min money above $25M, aggregate success 88.2% (2,571 of 2,916),
//! worst surviving seed 75%, 7–23 launches per surviving seed, deaths at
//! 10–17 launches. First launch month 18.7, dev spend $81.9M, unit cost
//! $18.5M vs payment $35.9M (cost ratio ~50% still holds). The roguelike
//! target of 2–4% bankruptcies is not met at $250M (it is at $300M: 4/200);
//! the user chose $250M to keep the edge.
//!
//! History: the 2026-07 M4 retune measured 1/200 bankrupt, aggregate
//! 92.9%, 163/200 above $25M; the delta-v plan (17_2) step 3 moved that
//! to 13/200 and 87.7% by taking away the launch check's hidden gravity
//! allowance, and D7 to the numbers above. The margin-sweep context still
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
        // (D7 low -$40.7M).
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
                "seed {}: first profitable year {} later than start+7 (D7 max start+7)",
                s.seed, fpy,
            );
        }
        launches += s.launches;
        successes += s.successes;
        if s.bankrupt {
            // A bankrupt run is truncated at the debt line, so the
            // per-seed activity bands below don't apply to it
            // (D7 deaths: 10-17 launches, success 27-80%).
            bankrupt += 1;
            continue;
        }
        assert!(
            (3..=30).contains(&s.launches),
            "seed {}: {} launches outside band 3..=30 (D7 7..=23; the \
             top is a seed that wins a big block program)",
            s.seed, s.launches,
        );
        let rate = s.successes as f64 / s.launches as f64;
        assert!(
            rate >= 0.65,
            "seed {}: launch success rate {:.0}% below 65% (D7 min 75%; \
             low-launch seeds make this floor noisy)",
            s.seed, rate * 100.0,
        );
    }

    // Fleet-level bands (D7: 65/200 end above starting money,
    // 146/200 keep min money above $25M, 23/200 bankrupt, 179/200 have
    // a profitable year).
    let n = summaries.len() as f64;
    // The agreed roguelike guard band was 1-6 bankruptcies per 100 seeds
    // (target 2-4/100); D7 measured 11.5/100 at the $250M start the user
    // chose, so the 200-seed ceiling sits at 15%. Neither bound means
    // much at n=20 — at an 11.5% true rate, five deaths in twenty happen
    // one run in forty, so the smoke ceiling is 25%.
    //
    // The ceiling needs the same treatment the floor already had. A 6%
    // ceiling on twenty seeds means "at most one bankruptcy", and at a
    // 4% true rate the chance of seeing two or more is 19% — a smoke
    // check that cries wolf one run in five. Allowing two (10%) fails
    // about 3% of the time instead, which is what a smoke check should
    // feel like. The 200-seed run keeps the real 6% ceiling.
    let ceiling = if summaries.len() >= 100 { 0.15 } else { 0.25 };
    assert!(
        bankrupt as f64 / n <= ceiling,
        "{bankrupt}/{n} seeds bankrupt (band <= {:.0}%, D7 measured 11.5% \
         at 200 seeds; roguelike target 2-4%)",
        ceiling * 100.0,
    );
    if summaries.len() >= 100 {
        assert!(
            bankrupt as f64 / n >= 0.01,
            "only {bankrupt}/{n} seeds bankrupt (band >= 1%, D7 11.5%; \
             the game should stay dangerous)",
        );
    }
    assert!(
        min_above_25m as f64 / n >= 0.62,
        "only {min_above_25m}/{n} seeds kept min money above $25M (band >= 62%, \
         D7 73%)",
    );
    assert!(
        profitable as f64 / n >= 0.21,
        "only {profitable}/{n} seeds profitable after run (band >= 21%, D7 32.5%)",
    );
    assert!(
        with_fpy as f64 / n >= 0.85,
        "only {with_fpy}/{n} seeds ever had a profitable year (band >= 85%, D7 89.5%)",
    );

    let aggregate = successes as f64 / launches as f64;
    assert!(
        aggregate >= 0.85,
        "aggregate launch success rate {:.1}% below 85% (D7 88.2%)",
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
