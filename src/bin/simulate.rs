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
use rocket_tycoon::policy::{policy_by_name, POLICY_NAMES};
use rocket_tycoon::sim::{run_seed, CSV_HEADER};

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
