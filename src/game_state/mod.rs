use std::collections::HashMap;

use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::contract::{self, Contract};
use crate::flight::Flight;
use crate::event::{EventLog, GameEvent};
use crate::rocket::RocketDesign;
use crate::rocket_project::RocketProjectId;
use crate::seed::GameSeed;
use crate::balance_config::BalanceConfig;

pub use crate::company::{Company, BidRule, MonthlyFinancials};

mod advance;
mod flight_ops;
mod market_ops;

/// Game simulation speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameSpeed {
    Paused,
    Normal,
    Fast,
    VeryFast,
}

impl GameSpeed {
    /// Tick interval in milliseconds for the UI loop.
    pub fn tick_ms(&self) -> u64 {
        match self {
            GameSpeed::Paused => u64::MAX,
            GameSpeed::Normal => 250,
            GameSpeed::Fast => 67,
            GameSpeed::VeryFast => 17,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            GameSpeed::Paused => "Paused",
            GameSpeed::Normal => "Normal",
            GameSpeed::Fast => "Fast",
            GameSpeed::VeryFast => "Very Fast",
        }
    }

    pub fn display_symbol(&self) -> &'static str {
        match self {
            GameSpeed::Paused => "⏸",
            GameSpeed::Normal => "▶",
            GameSpeed::Fast => "▶▶",
            GameSpeed::VeryFast => "▶▶▶",
        }
    }
}


/// Unique identifier for a spacecraft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpacecraftId(pub u64);

/// A persisted rocket at a location (arrived after a flight).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spacecraft {
    pub id: SpacecraftId,
    pub name: String,
    pub rocket: crate::rocket::Rocket,
    pub design: RocketDesign,
    pub location: String,
    #[serde(default)]
    pub rocket_project_id: RocketProjectId,
    /// Payloads still aboard (e.g. CSM in lunar orbit still carrying LEM).
    /// Detached when the player flies the spacecraft and the payload's
    /// `deploy_at` matches a stop on the new mission.
    #[serde(default)]
    pub payloads: Vec<crate::flight::Payload>,
}

impl Spacecraft {
    /// Remaining delta-v with no payload.
    pub fn remaining_delta_v(&self) -> f64 {
        self.rocket.remaining_delta_v(&self.design)
    }
}



const EVENT_LOG_SIZE: usize = 1000;

/// Payload safety factor applied when judging whether a design can
/// carry a contract — don't book payloads within 10% of the physical
/// maximum. Shared by the bid rule engine and `BasicPolicy`.
pub const BID_PAYLOAD_MARGIN: f64 = 0.9;

/// Why a launch manifest couldn't be assembled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// Two picked contracts want different destinations.
    ConflictingDestinations { first: String, second: String },
    /// A picked spacecraft is no longer in inventory.
    SpacecraftMissing,
    /// A picked spacecraft's rocket project no longer exists.
    PayloadProjectMissing,
}

/// Top-level game state.
#[derive(Debug, Serialize, Deserialize)]
pub struct GameState {
    pub date: GameDate,
    pub start_date: GameDate,
    pub player_company: Company,
    pub event_log: EventLog,
    pub seed: GameSeed,
    pub speed: GameSpeed,
    /// Last non-paused speed, for restoring on unpause.
    pub previous_speed: GameSpeed,
    /// Available contracts on the market (not player-owned).
    #[serde(default)]
    pub available_contracts: Vec<Contract>,
    /// Next contract ID counter.
    #[serde(default = "default_next_contract_id")]
    pub next_contract_id: u64,
    /// Flights currently in transit.
    #[serde(default)]
    pub active_flights: Vec<Flight>,
    /// Next flight ID counter.
    #[serde(default = "default_next_flight_id")]
    pub next_flight_id: u64,
    /// Next rocket instance ID counter.
    #[serde(default = "default_next_rocket_id")]
    pub next_rocket_id: u64,
    /// Spacecraft persisted after arrival.
    #[serde(default)]
    pub spacecraft: Vec<Spacecraft>,
    /// Current economic conditions affecting the launch market.
    #[serde(default)]
    pub economy: crate::economy::EconomicState,
    /// Active launch markets that generate contracts.
    #[serde(default = "default_markets")]
    pub markets: Vec<contract::Market>,
    /// Experimental technologies with seed-driven deficiencies.
    #[serde(default)]
    pub technologies: Vec<crate::technology::Technology>,
    /// Tracks which market events have already fired (by event key).
    #[serde(default)]
    pub fired_market_events: Vec<String>,
    /// Scripted competitor companies (M3: DinoSoar). Real `Company`
    /// state driven by a margin script instead of a player.
    #[serde(default)]
    pub competitors: Vec<crate::competitor::Competitor>,
    /// Observed award outcomes, newest last — the player's
    /// price-discovery record (M3 Task 4). Only public information
    /// and the player's own bids; capped so saves stay bounded.
    #[serde(default)]
    pub award_history: Vec<contract::AwardRecord>,
    /// Live anchor-customer campaigns issuing mission contracts.
    #[serde(default)]
    pub active_campaigns: Vec<contract::Campaign>,
    #[serde(default = "default_next_campaign_id")]
    pub next_campaign_id: u64,
    /// Tunable balance parameters this game was created with. Saves
    /// remember their balance; old saves load with defaults.
    #[serde(default)]
    pub balance: crate::balance_config::BalanceConfig,
    /// Max-payload lookups for the bid rule engine, keyed by
    /// (project, revision, destination). Path planning is far too
    /// slow to run per contract per day. Not serialized — rebuilt on
    /// demand; cleared when a design is modified (modifications
    /// change stage_groups without bumping revision).
    #[serde(skip)]
    pub payload_capability_cache: HashMap<(RocketProjectId, u32, String), f64>,
}

fn default_next_contract_id() -> u64 { 1 }
fn default_next_campaign_id() -> u64 { 1 }
fn default_next_flight_id() -> u64 { 1 }
fn default_next_rocket_id() -> u64 { 1 }
fn default_markets() -> Vec<contract::Market> {
    // Fallback for saves predating the markets field: unperturbed
    // templates (no seed available in a serde default).
    contract::default_archetypes().into_iter().map(|a| a.template).collect()
}

impl GameState {
    pub fn new(company_name: String, starting_money: f64, seed_value: u64) -> Self {
        Self::with_balance_and_money(
            company_name, starting_money, seed_value, BalanceConfig::default(),
        )
    }

    /// Create a game with a custom balance config; starting money comes
    /// from the config. Used by the game binary and the sim harness.
    pub fn with_balance(company_name: String, seed_value: u64, balance: BalanceConfig) -> Self {
        let starting_money = balance.costs.starting_money;
        Self::with_balance_and_money(company_name, starting_money, seed_value, balance)
    }

    fn with_balance_and_money(
        company_name: String,
        starting_money: f64,
        seed_value: u64,
        balance: BalanceConfig,
    ) -> Self {
        let start = GameDate::default_start();
        let mut event_log = EventLog::new(EVENT_LOG_SIZE);
        event_log.push(start, GameEvent::GameStarted);
        let seed = GameSeed::new(seed_value);

        let economy = crate::economy::initial_state(&seed, start);
        let technologies = crate::technology::generate_technologies(&seed);

        // Realize the archetype table for this world: presence rolls,
        // volume/rate multipliers, growth rates, and weight tilts
        // baked in. Absent and not-yet-emerged markets ride along
        // inactive. Start-active markets begin their growth clock now.
        let markets: Vec<contract::Market> =
            contract::realize_markets(&seed, &balance.markets.archetypes)
                .into_iter()
                .map(|r| {
                    let mut m = r.market;
                    if m.active {
                        m.activation_date = Some(start);
                    }
                    m
                })
                .collect();

        let competitors = if balance.competitor.enabled {
            vec![crate::competitor::realize_dinosoar(&seed, &balance)]
        } else {
            Vec::new()
        };

        GameState {
            date: start,
            start_date: start,
            player_company: Company::new(company_name, starting_money, &seed, &balance),
            event_log,
            seed,
            speed: GameSpeed::Paused,
            previous_speed: GameSpeed::Normal,
            available_contracts: Vec::new(),
            next_contract_id: 1,
            active_flights: Vec::new(),
            next_flight_id: 1,
            next_rocket_id: 1,
            spacecraft: Vec::new(),
            economy,
            markets,
            fired_market_events: Vec::new(),
            competitors,
            award_history: Vec::new(),
            active_campaigns: Vec::new(),
            next_campaign_id: 1,
            technologies,
            balance,
            payload_capability_cache: HashMap::new(),
        }
    }

    /// Resolve a flight's owning company to the real `Company`. Today
    /// every flight is player-owned (competitor launches are
    /// abstract); this is the seam the flight loop resolves through so
    /// competitor flights can become real without touching the loop's
    /// company accesses again.
    pub fn company_mut(&mut self, company: crate::flight::CompanyRef) -> &mut Company {
        match company {
            crate::flight::CompanyRef::Player => &mut self.player_company,
            crate::flight::CompanyRef::Competitor(ci) => &mut self.competitors[ci].company,
        }
    }

    /// Apply a modification (tankage / power tweak) to an existing
    /// rocket project. Replaces the design's stage_groups, transitions
    /// status back to `InDesign` with `MODIFICATION_WORK_FRACTION` of
    /// the project's original work_required, and rolls a flat chance to
    /// introduce one new undiscovered flaw. Caller is responsible for
    /// only invoking this when the project's status is `InDesign` or
    /// `Testing`; Revising is rejected. Returns Some(event) on success.
    pub fn apply_rocket_modification(
        &mut self,
        project_id: crate::rocket_project::RocketProjectId,
        new_stage_groups: Vec<Vec<crate::stage::Stage>>,
    ) -> Option<GameEvent> {
        use crate::rocket_project::RocketDesignStatus;
        use rand::Rng;

        let project = self.player_company.rocket_projects.iter_mut()
            .find(|p| p.project_id == project_id)?;
        if matches!(project.status, RocketDesignStatus::Revising { .. }) {
            return None;
        }
        let work_required = self.balance.work.rocket_design_work_required(project.complexity)
            * self.balance.work.rocket_modification_work_fraction;
        project.design.stage_groups = new_stage_groups;
        // The design's performance changed under the same revision —
        // drop every cached capability figure.
        self.payload_capability_cache.clear();
        project.status = RocketDesignStatus::InDesign {
            work_completed: 0.0,
            work_required,
        };

        // Roll for a new undiscovered flaw. Uses the per-flight trigger
        // distribution from the engine flaw generator (it's the same
        // schema for rocket flaws; existing rocket flaws are generated
        // the same way via gaussian_sample over complexity).
        let new_flaw = self.seed.contingent_rng.gen::<f64>()
            < self.balance.flaws.modification_flaw_prob;
        if new_flaw {
            let id = crate::flaw::FlawId(self.player_company.next_flaw_id);
            self.player_company.next_flaw_id += 1;
            let trigger = if self.seed.contingent_rng.gen::<f64>()
                < self.balance.flaws.rocket_endurance_fraction
            {
                crate::flaw::FlawTrigger::PerDay
            } else {
                crate::flaw::FlawTrigger::PerFlight
            };
            let flaw = crate::flaw::generate_single_flaw(
                id, trigger, &mut self.seed.contingent_rng, None, &self.balance.flaws,
            );
            // Re-borrow project (it was released across the rng calls).
            let project = self.player_company.rocket_projects.iter_mut()
                .find(|p| p.project_id == project_id)?;
            project.flaws.push(flaw);
        }
        let project = self.player_company.rocket_projects.iter()
            .find(|p| p.project_id == project_id)?;
        Some(GameEvent::RocketDesignModified {
            rocket_name: project.design.name.clone(),
            new_flaw,
        })
    }

    /// Days elapsed since the game started.
    pub fn elapsed_days(&self) -> u32 {
        self.start_date.days_until(&self.date)
    }

    /// Toggle between paused and the last non-paused speed.
    pub fn toggle_pause(&mut self) {
        if self.speed == GameSpeed::Paused {
            self.speed = self.previous_speed;
        } else {
            self.previous_speed = self.speed;
            self.speed = GameSpeed::Paused;
        }
    }

    /// Set speed (also updates previous_speed so pause toggle remembers it).
    pub fn set_speed(&mut self, speed: GameSpeed) {
        if speed != GameSpeed::Paused {
            self.previous_speed = speed;
        }
        self.speed = speed;
    }

    /// Ensure the current month has an entry in the financials buffer.
    pub(super) fn ensure_current_month_financials(&mut self) {
        let year = self.date.year;
        let month = self.date.month;
        let already = self.player_company.monthly_financials.iter()
            .any(|f| f.year == year && f.month == month);
        if !already {
            self.player_company.monthly_financials.push_back(MonthlyFinancials {
                year,
                month,
                income: 0.0,
                expenses: 0.0,
            });
            // Keep rolling 12-month window
            while self.player_company.monthly_financials.len() > 12 {
                self.player_company.monthly_financials.pop_front();
            }
        }
    }

    /// Record an expense in the current month's financials.
    pub(super) fn record_expense(&mut self, amount: f64) {
        self.ensure_current_month_financials();
        let year = self.date.year;
        let month = self.date.month;
        if let Some(f) = self.player_company.monthly_financials.iter_mut()
            .find(|f| f.year == year && f.month == month)
        {
            f.expenses += amount;
        }
    }

    /// Record income in the current month's financials.
    pub(super) fn record_income(&mut self, amount: f64) {
        self.ensure_current_month_financials();
        let year = self.date.year;
        let month = self.date.month;
        if let Some(f) = self.player_company.monthly_financials.iter_mut()
            .find(|f| f.year == year && f.month == month)
        {
            f.income += amount;
        }
    }

}

#[cfg(test)]
mod tests;
