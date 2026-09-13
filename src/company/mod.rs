//! The company: everything a rocket firm owns and does that isn't
//! world-state — teams, projects, designs, manufacturing, contracts,
//! reputation, financials, and the standing bid rules. Split out of
//! game_state.rs (M3 hygiene); `game_state` re-exports the public
//! types so existing paths keep working. Both the player and scripted
//! competitors are a `Company`.
//!
//! Split by concern (17_REFACTOR.md E4): `staffing` hires and counts
//! teams, `projects` starts and finds R&D projects and dispatches
//! across the three lists, `retire` takes a design out of service,
//! `production` orders builds and unblocks the queue, `floor` runs the
//! manufacturing floor, `research` ticks the engineering day. The
//! company itself, its cash and the monthly ledger stay here.

mod floor;
mod production;
mod projects;
mod research;
mod retire;
mod staffing;

pub use floor::MfgRow;
pub use research::ResearchTick;
pub use retire::{RetireRefusal, RetirementEffects};

use std::collections::{HashMap, VecDeque};

use serde::{Serialize, Deserialize};

use crate::contract::{self, Contract};
use crate::engine::{EngineCycle, EngineId};
use crate::engine_project::{EngineDesignStatus, EngineProject, EngineProjectId, EngineSource, PropellantPreset};
use crate::calendar::GameDate;
use crate::event::{GameEvent, ProjectEvent};
use crate::manufacturing::{Manufacturing, ManufacturingOrder, ManufacturingOrderType, InventoryEngine};
use crate::launch::LaunchRecord;
use crate::reputation::Reputation;
use crate::rocket::{RocketDesign, RocketDesignId};
use crate::project::{Designable, DesignProject, WorkEvent};

pub use crate::project::{ProjectCore, ProjectKind, ProjectRef, RevisionPlan};

use crate::rocket_project::{RocketProject, RocketProjectId};

use crate::seed::GameSeed;

use crate::balance_config::BalanceConfig;
use crate::id::IdAllocator;

use crate::team::{EngineeringTeam, ManufacturingTeam, TeamId};

use crate::third_party::{self, ContractedEngine, ContractedEngineId, ThirdPartyEngine};

/// Monthly income/expense record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyFinancials {
    pub year: u32,
    pub month: u32,
    pub income: f64,
    pub expenses: f64,
}

/// A player's rocket company.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Company {
    pub name: String,
    pub money: f64,
    pub next_team_id: IdAllocator<TeamId>,
    pub next_engine_id: IdAllocator<EngineId>,
    pub next_project_id: IdAllocator<EngineProjectId>,
    pub next_flaw_id: IdAllocator<crate::flaw::FlawId>,
    pub next_rocket_project_id: IdAllocator<RocketProjectId>,
    pub next_contracted_engine_id: IdAllocator<ContractedEngineId>,
    /// Allocator for `ReactorProjectId`.
    #[serde(default)]
    pub next_reactor_project_id: IdAllocator<crate::reactor_project::ReactorProjectId>,
    /// Allocator for `ReactorId` (the design's identity, used like
    /// `next_engine_id` for engine designs).
    #[serde(default)]
    pub next_reactor_id: IdAllocator<crate::reactor::ReactorId>,
    pub teams: Vec<EngineeringTeam>,
    pub manufacturing_teams: Vec<ManufacturingTeam>,
    pub engine_projects: Vec<EngineProject>,
    pub rocket_projects: Vec<RocketProject>,
    /// Player-researched reactor designs and their workflow state.
    #[serde(default)]
    pub reactor_projects: Vec<crate::reactor_project::ReactorProject>,
    pub third_party_catalog: Vec<ThirdPartyEngine>,
    pub contracted_engines: Vec<ContractedEngine>,
    pub manufacturing: Manufacturing,
    /// Flag to avoid repeatedly pausing when manufacturing is idle.
    #[serde(default)]
    pub notified_manufacturing_idle: bool,
    /// Contracts accepted by the player.
    #[serde(default)]
    pub active_contracts: Vec<Contract>,
    /// Reputation tracker.
    #[serde(default)]
    pub reputation: Reputation,
    /// Launch history.
    #[serde(default)]
    pub launch_history: Vec<LaunchRecord>,
    /// Monthly financial records (rolling 12 months).
    #[serde(default)]
    pub monthly_financials: VecDeque<MonthlyFinancials>,
    /// Date of last launch (for drought tracking).
    #[serde(default)]
    pub last_launch_date: Option<GameDate>,
    /// How many engines have been built per engine project (for learning curve).
    #[serde(default)]
    pub engine_build_counts: HashMap<EngineProjectId, u32>,
    /// How many rockets have been built per design (for learning curve).
    #[serde(default)]
    pub rocket_build_counts: HashMap<RocketDesignId, u32>,
    /// Build cost history per rocket design (for avg/marginal cost).
    /// Each entry is the *total* per-rocket cost (engines + stages + integration)
    /// charged at order time.
    #[serde(default)]
    pub rocket_cost_history: HashMap<RocketDesignId, Vec<f64>>,
    /// Per-engine-project build cost history (player-designed engines only).
    /// Each entry is the material_cost for one built engine, recorded at order
    /// time so the learning curve is reflected.
    #[serde(default)]
    pub engine_cost_history: HashMap<EngineProjectId, Vec<f64>>,
    /// How many engines have been ordered per contracted engine catalog entry.
    #[serde(default)]
    pub contracted_engine_build_counts: HashMap<ContractedEngineId, u32>,
    /// Auto-build targets: maintain at least N rockets in inventory per project.
    #[serde(default)]
    pub auto_build_targets: HashMap<RocketProjectId, u32>,
    /// Rocket projects currently being rushed. A rush job takes the whole
    /// manufacturing floor until the rocket reaches inventory. Several can
    /// run at once; they share the floor the way ordinary orders would.
    #[serde(default)]
    pub rush_projects: std::collections::HashSet<RocketProjectId>,
    /// Standing per-market bid rules (M3 Task 3): while enabled, the
    /// rule engine auto-bids marginal cost × (1 + margin) on that
    /// market's solicitations, gated on free stock.
    #[serde(default)]
    pub bid_rules: HashMap<contract::MarketId, BidRule>,
}

/// A standing bid rule for one market. The player (or a policy) sets
/// these once; the daily rule engine does the bidding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BidRule {
    pub enabled: bool,
    /// Markup over the design's marginal cost: bid = cost × (1 + margin).
    pub margin: f64,
}

impl Default for BidRule {
    fn default() -> Self {
        BidRule { enabled: false, margin: 0.25 }
    }
}

impl Company {
    pub fn new(name: String, starting_money: f64, seed: &GameSeed, balance_cfg: &BalanceConfig) -> Self {
        let catalog = third_party::generate_starter_engines(seed);
        let mut company = Company {
            name,
            money: starting_money,
            next_team_id: IdAllocator::starting_at(1),
            next_engine_id: IdAllocator::starting_at(1),
            next_project_id: IdAllocator::starting_at(1),
            next_flaw_id: IdAllocator::starting_at(1),
            next_rocket_project_id: IdAllocator::starting_at(1),
            next_contracted_engine_id: IdAllocator::starting_at(1),
            next_reactor_project_id: IdAllocator::starting_at(1),
            next_reactor_id: IdAllocator::starting_at(1),
            teams: Vec::new(),
            manufacturing_teams: Vec::new(),
            engine_projects: Vec::new(),
            rocket_projects: Vec::new(),
            reactor_projects: Vec::new(),
            third_party_catalog: catalog,
            contracted_engines: Vec::new(),
            manufacturing: Manufacturing::new(),
            notified_manufacturing_idle: false,
            active_contracts: Vec::new(),
            reputation: Reputation::new(),
            launch_history: Vec::new(),
            monthly_financials: VecDeque::new(),
            last_launch_date: None,
            engine_build_counts: HashMap::new(),
            rocket_build_counts: HashMap::new(),
            rocket_cost_history: HashMap::new(),
            engine_cost_history: HashMap::new(),
            contracted_engine_build_counts: HashMap::new(),
            auto_build_targets: HashMap::new(),
            bid_rules: HashMap::new(),
            rush_projects: std::collections::HashSet::new(),
        };
        // The founding team came with the company: no hiring fee for
        // the people who were already there on day one, so the player
        // starts with exactly `starting_money`. Their salary still
        // comes due at the end of the first month like anyone else's.
        company.enroll_team("Team 1".into(), balance_cfg);
        company
    }

    /// Start the ledger row for `date`'s month, unless it is already the
    /// open one. Called as the calendar rolls into a month (before any
    /// spending in it) and on a fresh company; the window keeps twelve.
    pub fn open_month(&mut self, date: GameDate) {
        let open = self.monthly_financials.back()
            .is_some_and(|f| f.year == date.year && f.month == date.month);
        if open {
            return;
        }
        self.monthly_financials.push_back(MonthlyFinancials {
            year: date.year,
            month: date.month,
            income: 0.0,
            expenses: 0.0,
        });
        while self.monthly_financials.len() > 12 {
            self.monthly_financials.pop_front();
        }
    }

    /// Spend `amount`: cash down, this month's expenses up.
    pub fn debit(&mut self, amount: f64) {
        self.money -= amount;
        if let Some(row) = self.monthly_financials.back_mut() {
            row.expenses += amount;
        }
    }

    /// Receive `amount`: cash up, this month's income up.
    pub fn credit(&mut self, amount: f64) {
        self.money += amount;
        if let Some(row) = self.monthly_financials.back_mut() {
            row.income += amount;
        }
    }
}
