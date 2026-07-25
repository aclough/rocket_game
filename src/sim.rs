//! Shared headless-run harness used by the `simulate` binary and the
//! integration tests in `tests/`, so both measure the game identically.

use crate::balance_config::BalanceConfig;
use crate::calendar::GameDate;
use crate::event::GameEvent;
use crate::game_state::GameState;
use crate::policy::CompanyPolicy;

/// Cumulative event tallies for one run. Launch attempts/outcomes are
/// tallied from events because `launch_history` only records
/// catastrophic at-launch failures and completed arrivals — vehicles
/// lost mid-flight would otherwise be invisible.
#[derive(Default)]
pub struct Tally {
    pub contracts_completed: u32,
    pub contracts_expired: u32,
    /// At-launch catastrophic failures + flights that departed.
    pub launch_attempts: u32,
    /// Flights that arrived fully successfully.
    pub launch_successes: u32,
    /// Vehicles destroyed (at launch or mid-flight) or stranded.
    pub vehicles_lost: u32,
    /// Sum of all contract payments received.
    pub payment_total: f64,
}

impl Tally {
    pub fn record_one(&mut self, e: &GameEvent) {
        match e {
            GameEvent::PaymentReceived { amount, .. } => {
                self.contracts_completed += 1;
                self.payment_total += amount;
            }
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

pub const CSV_HEADER: &str = "seed,date,money,reputation,contracts_available,contracts_active,\
contracts_completed,contracts_expired,launches,launch_successes,vehicles_lost,engine_projects,\
rocket_projects,reactor_projects,eng_teams,mfg_teams,rockets_inventory,flights_active";

pub fn metric_row(seed: u64, gs: &GameState, tally: &Tally) -> String {
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

pub struct RunSummary {
    pub seed: u64,
    pub start_year: u32,
    /// Money at run end — for a bankrupt run, money on the day the
    /// harness terminated it (just below [`SIM_DEBT_LIMIT`]).
    pub final_money: f64,
    pub min_money: f64,
    /// True when the run was terminated at [`SIM_DEBT_LIMIT`], or
    /// (rarely) survived to the end still below $0.
    pub bankrupt: bool,
    pub launches: usize,
    pub successes: usize,
    pub first_profitable_year: Option<u32>,
    /// Date of the first launch attempt (None if the run never launched).
    pub first_launch_date: Option<GameDate>,
    /// starting_money − balance on the first launch's day: the program
    /// cost to first flight (pre-launch revenue is zero, so this is pure
    /// spend).
    pub dev_spend_at_first_launch: Option<f64>,
    /// Undiscovered flaws across all player designs (engine + rocket
    /// projects and contracted engines) on the first launch's day — the
    /// hidden risk the company flew with.
    pub undiscovered_flaws_at_first_launch: Option<u32>,
    /// Mean unit build cost over every rocket the player completed
    /// (from `rocket_cost_history`, the same source bidding uses).
    pub mean_rocket_unit_cost: Option<f64>,
    /// Mean payment across completed contracts.
    pub mean_contract_payment: Option<f64>,
}

impl RunSummary {
    pub fn line(&self) -> String {
        let rate = if self.launches > 0 {
            format!("{:.0}%", 100.0 * self.successes as f64 / self.launches as f64)
        } else {
            "-".into()
        };
        let fpy = self.first_profitable_year
            .map(|y| y.to_string())
            .unwrap_or_else(|| "-".into());
        let m = |v: Option<f64>| v.map(|x| format!("${:.1}M", x / 1e6))
            .unwrap_or_else(|| "-".into());
        let first = match self.first_launch_date {
            Some(d) => format!(
                "{:04}-{:02}-{:02} (spend {}, {} hidden flaws)",
                d.year, d.month, d.day,
                m(self.dev_spend_at_first_launch),
                self.undiscovered_flaws_at_first_launch.unwrap_or(0),
            ),
            None => "-".into(),
        };
        format!(
            "seed {:>5}  final ${:>14.0}  min ${:>14.0}  bankrupt {}  launches {:>3} ({} ok)  first-profitable-year {}  first-launch {}  unit-cost {}  payment {}",
            self.seed, self.final_money, self.min_money,
            if self.bankrupt { "YES" } else { "no " },
            self.launches, rate, fpy,
            first,
            m(self.mean_rocket_unit_cost),
            m(self.mean_contract_payment),
        )
    }
}

/// Undiscovered flaws across every player design: engine projects,
/// rocket projects, and contracted (bought-in) engines.
fn undiscovered_flaw_count(c: &crate::company::Company) -> u32 {
    c.engine_projects.iter().flat_map(|p| p.flaws.iter())
        .chain(c.rocket_projects.iter().flat_map(|p| p.flaws.iter()))
        .chain(c.contracted_engines.iter().flat_map(|e| e.flaws.iter()))
        .filter(|f| !f.discovered)
        .count() as u32
}

/// Debt at which the harness terminates a run and marks it bankrupt.
/// The game itself has no death — bankruptcy/firm death arrive with
/// loans in a later milestone — but for balance measurement a company
/// $30M in the red is finished: letting it keep playing on unlimited
/// free debt hides real deaths (M4 Task 5 found seeds grinding from
/// -$97M back to positive on salary credit that nobody extended).
pub const SIM_DEBT_LIMIT: f64 = -30_000_000.0;

/// Simulate one seed for `years` under `policy`, calling `monthly`
/// with a metric row on day 1 of every month (plus the starting day).
/// The run ends early — bankrupt — if money falls below
/// [`SIM_DEBT_LIMIT`].
pub fn run_seed(
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
    let start_money = gs.player_company.money;
    let mut min_money = start_money;
    // Money at each January 1st, for year-over-year profitability.
    let mut jan_money: Vec<(u32, f64)> = vec![(start.year, start_money)];
    let mut first_launch: Option<(GameDate, f64, u32)> = None;

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
        if first_launch.is_none() && tally.launch_attempts > 0 {
            first_launch = Some((
                gs.date,
                start_money - gs.player_company.money,
                undiscovered_flaw_count(&gs.player_company),
            ));
        }
        min_money = min_money.min(gs.player_company.money);
        if gs.player_company.money < SIM_DEBT_LIMIT {
            monthly(&metric_row(seed, &gs, &tally));
            break;
        }
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

    let unit_costs: Vec<f64> = gs.player_company.rocket_cost_history
        .values()
        .flatten()
        .copied()
        .collect();
    let mean_rocket_unit_cost = (!unit_costs.is_empty())
        .then(|| unit_costs.iter().sum::<f64>() / unit_costs.len() as f64);
    let mean_contract_payment = (tally.contracts_completed > 0)
        .then(|| tally.payment_total / tally.contracts_completed as f64);

    RunSummary {
        seed,
        start_year: start.year,
        final_money: gs.player_company.money,
        min_money,
        bankrupt: gs.player_company.money < 0.0,
        launches: tally.launch_attempts as usize,
        successes: tally.launch_successes as usize,
        first_profitable_year,
        first_launch_date: first_launch.map(|(d, _, _)| d),
        dev_spend_at_first_launch: first_launch.map(|(_, s, _)| s),
        undiscovered_flaws_at_first_launch: first_launch.map(|(_, _, f)| f),
        mean_rocket_unit_cost,
        mean_contract_payment,
    }
}
