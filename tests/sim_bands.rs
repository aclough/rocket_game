//! M1 Task 4: determinism smoke test + metric-band regression tests.
//!
//! **22 baseline (22_FLAWS.md step 12, 2026-10-10).** Flaws roll at
//! flight events, carry an exposure class, are found faster after a
//! flight, include pad aborts and payload losses, travel with built
//! units and are retrofitted; activation exponent 1.6. Measured, 200
//! seeds × 8 years, basic policy: 19/200 bankrupt, 123/200 keep min
//! money above $25M, 114/200 end above starting money, 184/200 have a
//! profitable year, aggregate success 90.8%, success by launch
//! 36 / 63 / 81 / 97 %. The fleet bands below sit under those with
//! seed-noise room; the per-seed bands are unchanged.
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

/// The fleet-level bands — bankruptcies, seeds keeping $25M, seeds
/// profitable, seeds with a profitable year, aggregate success — were
/// suspended while 22_FLAWS.md remade the flaw model (steps 2–11): each
/// step moved them on purpose, and lifting ceilings step by step had
/// made them meaningless. Step 12 re-baselined them to the new default
/// and switched this back on. Suspend again, rather than lift, when a
/// plan is going to move them for several steps; the figures print as
/// `[fleet]` either way.
const FLEET_BANDS_SUSPENDED: bool = false;

/// Assert a fleet-level band, or, while suspended, print the miss.
fn fleet_band(ok: bool, message: String) {
    if ok {
        return;
    }
    if FLEET_BANDS_SUSPENDED {
        println!("[fleet band suspended] {message}");
    } else {
        panic!("{message}");
    }
}

fn run(seed: u64, years: u32) -> (RunSummary, Vec<String>) {
    run_policy("basic", seed, years)
}

fn run_policy(policy: &str, seed: u64, years: u32) -> (RunSummary, Vec<String>) {
    let balance = BalanceConfig::default();
    let mut policy = policy_by_name(policy).expect("policy exists");
    let mut rows = Vec::new();
    let summary = run_seed(seed, years, &balance, policy.as_mut(), |row| {
        rows.push(row.to_string())
    });
    (summary, rows)
}

/// The fleet figures of a strategy bot, as `assert_bands` counts them,
/// plus its launch count median — printed under its own tag so a
/// sweep can read every strategy's line.
fn fleet_line(tag: &str, summaries: &[RunSummary]) -> String {
    let starting_money = BalanceConfig::default().costs.starting_money;
    let n = summaries.len();
    let bankrupt = summaries.iter().filter(|s| s.bankrupt).count();
    let kept = summaries.iter().filter(|s| s.min_money > 25_000_000.0).count();
    let profitable = summaries.iter().filter(|s| s.final_money > starting_money).count();
    let fpy = summaries.iter().filter(|s| s.first_profitable_year.is_some()).count();
    let launches: usize = summaries.iter().map(|s| s.launches).sum();
    let successes: usize = summaries.iter().map(|s| s.successes).sum();
    let mut counts: Vec<usize> = summaries.iter().map(|s| s.launches).collect();
    counts.sort_unstable();
    let median = counts.get(n / 2).copied().unwrap_or(0);
    format!(
        "[fleet {tag}] bankrupt {bankrupt}/{n}, kept $25M {kept}/{n}, profitable {profitable}/{n}, \
         profitable year {fpy}/{n}, aggregate success {:.1}%, median launches {median}",
        if launches == 0 { 0.0 } else { successes as f64 / launches as f64 * 100.0 },
    )
}

/// The bands a strategy bot must hold (24_STRATEGIES.md §3): it must
/// stay *possible* — few bankruptcies, a profitable year in most
/// worlds, a real launch cadence, a sane success rate — not pay like
/// the medium bot. Thresholds sit under each strategy's first
/// measurement with seed-noise room.
struct StrategyBands {
    policy: &'static str,
    bankrupt_max: f64,
    profitable_year_min: f64,
    median_launches_min: usize,
    aggregate_min: f64,
}

fn assert_strategy_bands(b: &StrategyBands) {
    let summaries: Vec<RunSummary> = (1..=200).map(|seed| run_policy(b.policy, seed, 8).0).collect();
    println!("{}", fleet_line(b.policy, &summaries));
    let n = summaries.len() as f64;
    let bankrupt = summaries.iter().filter(|s| s.bankrupt).count();
    let fpy = summaries.iter().filter(|s| s.first_profitable_year.is_some()).count();
    let launches: usize = summaries.iter().map(|s| s.launches).sum();
    let successes: usize = summaries.iter().map(|s| s.successes).sum();
    let mut counts: Vec<usize> = summaries.iter().map(|s| s.launches).collect();
    counts.sort_unstable();
    let median = counts[counts.len() / 2];
    let p = b.policy;
    fleet_band(bankrupt as f64 / n <= b.bankrupt_max,
        format!("{p}: {bankrupt}/200 bankrupt (band <= {:.0}%)", b.bankrupt_max * 100.0));
    fleet_band(fpy as f64 / n >= b.profitable_year_min,
        format!("{p}: only {fpy}/200 had a profitable year (band >= {:.0}%)", b.profitable_year_min * 100.0));
    fleet_band(median >= b.median_launches_min,
        format!("{p}: median {median} launches in 8 years (band >= {})", b.median_launches_min));
    fleet_band(successes as f64 / launches as f64 >= b.aggregate_min,
        format!("{p}: aggregate success {:.1}% (band >= {:.0}%)", successes as f64 / launches as f64 * 100.0, b.aggregate_min * 100.0));
}

/// The small launcher (24_STRATEGIES.md steps 4–5). Measured after the
/// smallsat market took per-launch floors (200 × 8): bankrupt 0/200,
/// kept $25M 200/200, profitable 0/200, profitable year 132/200,
/// aggregate success 89.5 %, median 13 launches, unit cost $5.6M
/// against $6.7M a flight. A thin business that works — Electron's.
#[test]
#[ignore = "200-seed smallsat bands; run with `cargo test -- --ignored`"]
fn smallsat_bands_200_seeds() {
    assert_strategy_bands(&StrategyBands {
        policy: "smallsat", bankrupt_max: 0.25, profitable_year_min: 0.50, median_launches_min: 10, aggregate_min: 0.80,
    });
}

/// The medium vehicle on methane (step 6): the one bot whose critical
/// path runs through technology deficiencies. Measured 200 × 8:
/// bankrupt 10/200, profitable year 186/200, first flight 27 %,
/// aggregate 92 %, unit cost $15.9M against $30.9M a flight.
#[test]
#[ignore = "200-seed methalox bands; run with `cargo test -- --ignored`"]
fn methalox_bands_200_seeds() {
    assert_strategy_bands(&StrategyBands {
        policy: "methalox", bankrupt_max: 0.15, profitable_year_min: 0.80, median_launches_min: 6, aggregate_min: 0.80,
    });
}

/// The solid-boosted vehicle (step 6): the one bot that builds and
/// flies solids. Measured 200 × 8: bankrupt 2/200, profitable year
/// 156/200, median 9 launches, aggregate 76.6 % — its first two
/// flights succeed 28 % and 35 % of the time against the medium bot's
/// 36 % and 64 %, which is a flaw-model question about solids worth
/// its own look, not a band to hide; unit cost $13.9M against $27.4M
/// a flight.
#[test]
#[ignore = "200-seed solid bands; run with `cargo test -- --ignored`"]
fn solid_bands_200_seeds() {
    assert_strategy_bands(&StrategyBands {
        policy: "solid", bankrupt_max: 0.15, profitable_year_min: 0.65, median_launches_min: 4, aggregate_min: 0.70,
    });
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
        // The top was 30 until 22_FLAWS step 3: a seed that fails more
        // re-flies more, and one hit 31.
        assert!(
            (3..=35).contains(&s.launches),
            "seed {}: {} launches outside band 3..=35 (D7 7..=23; the \
             top is a seed that wins a big block program, or re-flies failures)",
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
    let aggregate_now = successes as f64 / launches as f64;
    println!(
        "[fleet] bankrupt {bankrupt}/{n}, kept $25M {min_above_25m}/{n}, profitable {profitable}/{n}, \
         profitable year {with_fpy}/{n}, aggregate success {:.1}%",
        aggregate_now * 100.0,
    );
    // The agreed roguelike guard band was 1-6 bankruptcies per 100 seeds
    // (target 2-4/100); D7 measured 11.5/100 at the $250M start the user
    // chose, so the 200-seed ceiling sat at 15%; since 21_2 step 2
    // (2026-10-04) the bot runs at 14-16% on these seeds and ~11% on
    // fresh ones, so the ceiling is 20% to stop seed noise tripping it.
    // Neither bound means
    // much at n=20 — at an 11.5% true rate, five deaths in twenty happen
    // one run in forty, so the smoke ceiling is 25%.
    //
    // The ceiling needs the same treatment the floor already had. A 6%
    // ceiling on twenty seeds means "at most one bankruptcy", and at a
    // 4% true rate the chance of seeing two or more is 19% — a smoke
    // check that cries wolf one run in five. Allowing two (10%) fails
    // about 3% of the time instead, which is what a smoke check should
    // feel like. The 200-seed run keeps the real 6% ceiling.
    //
    // 22 baseline: 19/200 (9.5%). The ceiling leaves seed-noise room;
    // the smoke run gets more (five in twenty happens one run in forty
    // at this rate).
    let ceiling = if summaries.len() >= 100 { 0.15 } else { 0.25 };
    fleet_band(
        bankrupt as f64 / n <= ceiling,
        format!(
            "{bankrupt}/{n} seeds bankrupt (band <= {:.0}%, D7 measured 11.5% \
             at 200 seeds; roguelike target 2-4%)",
            ceiling * 100.0,
        ),
    );
    // 24_STRATEGIES.md steps 3/3b (lighter structure and small-engine
    // pumps) took the basic bot from 2/200 to 1/200 bankrupt; the floor
    // is 0.5 % until the slack is spent on the flaws retune the plan
    // reserves it for (Q2 there) — the game should stay dangerous.
    if summaries.len() >= 100 {
        fleet_band(
            bankrupt as f64 / n >= 0.005,
            format!(
                "only {bankrupt}/{n} seeds bankrupt (band >= 0.5%, D7 11.5%; \
                 the game should stay dangerous)",
            ),
        );
    }
    // 22 baseline: 123/200 keep $25M, 114/200 profitable, 184/200 with a
    // profitable year; floors sit ~6 points under, the smoke run lower.
    let cash_floor = if summaries.len() >= 100 { 0.55 } else { 0.40 };
    fleet_band(
        min_above_25m as f64 / n >= cash_floor,
        format!(
            "only {min_above_25m}/{n} seeds kept min money above $25M (band >= {:.0}%, \
             22 baseline 61.5%, D7 73%)",
            cash_floor * 100.0,
        ),
    );
    let profitable_floor = if summaries.len() >= 100 { 0.45 } else { 0.30 };
    fleet_band(
        profitable as f64 / n >= profitable_floor,
        format!("only {profitable}/{n} seeds profitable after run (band >= {:.0}%, 22 baseline 57%, D7 32.5%)",
            profitable_floor * 100.0),
    );
    let fpy_floor = if summaries.len() >= 100 { 0.85 } else { 0.70 };
    fleet_band(
        with_fpy as f64 / n >= fpy_floor,
        format!("only {with_fpy}/{n} seeds ever had a profitable year (band >= {:.0}%, 22 baseline 92%, D7 89.5%)",
            fpy_floor * 100.0),
    );

    let aggregate = successes as f64 / launches as f64;
    fleet_band(
        aggregate >= 0.85,
        format!("aggregate launch success rate {:.1}% below 85% (22 baseline 90.8%, D7 88.2%)", aggregate * 100.0),
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
