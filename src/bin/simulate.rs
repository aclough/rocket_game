//! Headless simulation harness — runs the game loop with a scripted
//! policy and no UI, emitting monthly metrics and per-seed summaries.
//!
//! ```text
//! cargo run --bin simulate -- --seed 42 --years 5 --policy none
//!        [--seeds 1..200] [--balance base.toml --balance sweep.toml]
//!        [--dump-balance] [--csv out.csv] [--summary-only]
//! ```

use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::calendar::GameDate;
use rocket_tycoon::event::GameEvent;
use rocket_tycoon::game_state::GameState;
use rocket_tycoon::policy::{policy_by_name, CompanyPolicy, POLICY_NAMES};

const USAGE: &str = "\
Usage: simulate [OPTIONS]

Options:
  --seed N            Run a single seed (default: 42)
  --seeds A..B        Run an inclusive range of seeds (overrides --seed)
  --years Y           Years to simulate per seed (default: 5)
  --policy NAME       Company policy (default: none)
  --balance FILE      Balance TOML override; repeatable, merged in order
  --dump-balance      Print the effective balance TOML and exit
  --csv PATH          Write monthly metric rows to PATH as CSV
  --summary-only      Suppress monthly rows on stdout (summaries still print)
  --help              Show this help
";

struct Args {
    seeds: Vec<u64>,
    years: u32,
    policy: String,
    balance_files: Vec<PathBuf>,
    dump_balance: bool,
    csv: Option<PathBuf>,
    summary_only: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        seeds: vec![42],
        years: 5,
        policy: "none".into(),
        balance_files: Vec::new(),
        dump_balance: false,
        csv: None,
        summary_only: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| {
            it.next().ok_or_else(|| format!("{flag} requires a value"))
        };
        match arg.as_str() {
            "--seed" => {
                let v = value("--seed")?;
                let n = v.parse::<u64>().map_err(|_| format!("bad --seed: {v}"))?;
                args.seeds = vec![n];
            }
            "--seeds" => {
                let v = value("--seeds")?;
                let (a, b) = v.split_once("..")
                    .ok_or_else(|| format!("bad --seeds (want A..B): {v}"))?;
                let a = a.parse::<u64>().map_err(|_| format!("bad --seeds start: {a}"))?;
                let b = b.parse::<u64>().map_err(|_| format!("bad --seeds end: {b}"))?;
                if b < a {
                    return Err(format!("--seeds range is empty: {v}"));
                }
                args.seeds = (a..=b).collect();
            }
            "--years" => {
                let v = value("--years")?;
                args.years = v.parse().map_err(|_| format!("bad --years: {v}"))?;
            }
            "--policy" => args.policy = value("--policy")?,
            "--balance" => args.balance_files.push(PathBuf::from(value("--balance")?)),
            "--dump-balance" => args.dump_balance = true,
            "--csv" => args.csv = Some(PathBuf::from(value("--csv")?)),
            "--summary-only" => args.summary_only = true,
            "--help" | "-h" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}\n\n{USAGE}")),
        }
    }
    Ok(args)
}

/// Cumulative event tallies for one run. Launch attempts/outcomes are
/// tallied from events because `launch_history` only records
/// catastrophic at-launch failures and completed arrivals — vehicles
/// lost mid-flight would otherwise be invisible.
#[derive(Default)]
struct Tally {
    contracts_completed: u32,
    contracts_expired: u32,
    /// At-launch catastrophic failures + flights that departed.
    launch_attempts: u32,
    /// Flights that arrived fully successfully.
    launch_successes: u32,
    /// Vehicles destroyed (at launch or mid-flight) or stranded.
    vehicles_lost: u32,
}

impl Tally {
    fn record_one(&mut self, e: &GameEvent) {
        {
            match e {
                GameEvent::PaymentReceived { .. } => self.contracts_completed += 1,
                GameEvent::ContractExpired { .. } => self.contracts_expired += 1,
                GameEvent::FlightDeparted { .. } => self.launch_attempts += 1,
                GameEvent::LaunchFailure { .. } => {
                    self.launch_attempts += 1;
                    self.vehicles_lost += 1;
                }
                GameEvent::LaunchSuccess { .. } => self.launch_successes += 1,
                GameEvent::SpacecraftLost { .. } => self.vehicles_lost += 1,
                _ => {}
            }
        }
    }
}

const CSV_HEADER: &str = "seed,date,money,reputation,contracts_available,contracts_active,\
contracts_completed,contracts_expired,launches,launch_successes,vehicles_lost,engine_projects,\
rocket_projects,reactor_projects,eng_teams,mfg_teams,rockets_inventory,flights_active";

fn metric_row(seed: u64, gs: &GameState, tally: &Tally) -> String {
    let c = &gs.player_company;
    format!(
        "{seed},{:04}-{:02}-{:02},{:.0},{:.1},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        gs.date.year, gs.date.month, gs.date.day,
        c.money,
        c.reputation.total(),
        gs.available_contracts.len(),
        c.active_contracts.len(),
        tally.contracts_completed,
        tally.contracts_expired,
        tally.launch_attempts,
        tally.launch_successes,
        tally.vehicles_lost,
        c.engine_projects.len(),
        c.rocket_projects.len(),
        c.reactor_projects.len(),
        c.teams.len(),
        c.manufacturing_teams.len(),
        c.manufacturing.inventory.rockets.len(),
        gs.active_flights.len(),
    )
}

struct RunSummary {
    seed: u64,
    final_money: f64,
    min_money: f64,
    bankrupt: bool,
    launches: usize,
    successes: usize,
    first_profitable_year: Option<u32>,
}

impl RunSummary {
    fn line(&self) -> String {
        let rate = if self.launches > 0 {
            format!("{:.0}%", 100.0 * self.successes as f64 / self.launches as f64)
        } else {
            "-".into()
        };
        let fpy = self.first_profitable_year
            .map(|y| y.to_string())
            .unwrap_or_else(|| "-".into());
        format!(
            "seed {:>5}  final ${:>14.0}  min ${:>14.0}  bankrupt {}  launches {:>3} ({} ok)  first-profitable-year {}",
            self.seed, self.final_money, self.min_money,
            if self.bankrupt { "YES" } else { "no " },
            self.launches, rate, fpy,
        )
    }
}

fn run_seed(
    seed: u64,
    years: u32,
    balance: &BalanceConfig,
    policy: &mut dyn CompanyPolicy,
    mut monthly: impl FnMut(&str),
) -> RunSummary {
    let mut gs = GameState::with_balance("SimCorp".into(), seed, balance.clone());
    let start = gs.date;
    let end = GameDate::new(start.year + years, start.month, start.day);

    let mut tally = Tally::default();
    let mut min_money = gs.player_company.money;
    // Money at each January 1st, for year-over-year profitability.
    let mut jan_money: Vec<(u32, f64)> = vec![(start.year, gs.player_company.money)];

    monthly(&metric_row(seed, &gs, &tally));
    while gs.date < end {
        let log_before = gs.event_log.total_pushed();
        policy.act(&mut gs);
        gs.advance_day();
        // Tally from the event log so policy-initiated events (launches
        // happen during act(), not advance_day) are counted too.
        let new_events = (gs.event_log.total_pushed() - log_before) as usize;
        for (_, e) in gs.event_log.recent(new_events) {
            tally.record_one(e);
        }
        min_money = min_money.min(gs.player_company.money);
        if gs.date.day == 1 {
            monthly(&metric_row(seed, &gs, &tally));
            if gs.date.month == 1 {
                jan_money.push((gs.date.year, gs.player_company.money));
            }
        }
    }

    let first_profitable_year = jan_money.windows(2)
        .find(|w| w[1].1 > w[0].1)
        .map(|w| w[0].0);

    RunSummary {
        seed,
        final_money: gs.player_company.money,
        min_money,
        bankrupt: gs.player_company.money < 0.0,
        launches: tally.launch_attempts as usize,
        successes: tally.launch_successes as usize,
        first_profitable_year,
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let balance = match BalanceConfig::load_layered(&args.balance_files) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    if args.dump_balance {
        match balance.to_toml_string() {
            Ok(toml) => {
                print!("{toml}");
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    // Validate the policy name up front; a FRESH policy instance is
    // created per seed (policies carry run state).
    if policy_by_name(&args.policy).is_none() {
        eprintln!("error: unknown policy `{}` (available: {})",
            args.policy, POLICY_NAMES.join(", "));
        return ExitCode::FAILURE;
    }

    // Monthly rows go to --csv if given, else stdout (unless --summary-only).
    let mut csv_file = match &args.csv {
        Some(path) => match File::create(path) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("error: creating {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let rows_to_stdout = csv_file.is_none() && !args.summary_only;
    let mut wrote_header = false;

    let mut summaries = Vec::new();
    for &seed in &args.seeds {
        let mut policy = policy_by_name(&args.policy).expect("validated above");
        let summary = run_seed(seed, args.years, &balance, policy.as_mut(), |row| {
            if !wrote_header {
                if let Some(f) = csv_file.as_mut() {
                    let _ = writeln!(f, "{CSV_HEADER}");
                } else if rows_to_stdout {
                    println!("{CSV_HEADER}");
                }
                wrote_header = true;
            }
            if let Some(f) = csv_file.as_mut() {
                let _ = writeln!(f, "{row}");
            } else if rows_to_stdout {
                let _ = writeln!(io::stdout(), "{row}");
            }
        });
        let _ = writeln!(io::stdout(), "{}", summary.line());
        summaries.push(summary);
    }

    if summaries.len() > 1 {
        let n = summaries.len() as f64;
        let bankrupt = summaries.iter().filter(|s| s.bankrupt).count();
        let avg_final: f64 = summaries.iter().map(|s| s.final_money).sum::<f64>() / n;
        let profitable = summaries.iter()
            .filter(|s| s.first_profitable_year.is_some())
            .count();
        println!(
            "── {} seeds: avg final ${:.0}, bankrupt {}/{}, ever-profitable {}/{}",
            summaries.len(), avg_final, bankrupt, summaries.len(),
            profitable, summaries.len(),
        );
    }

    io::stdout().flush().ok();
    ExitCode::SUCCESS
}
