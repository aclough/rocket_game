use std::collections::VecDeque;

use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::contract::{self, Contract, ContractId};
use crate::engine::{EngineCycle, EngineId};
use crate::engine_project::{EngineProject, EngineProjectId, EngineSource, PropellantPreset, WorkEvent};
use crate::event::{EventLog, GameEvent};
use crate::manufacturing::{Manufacturing, ManufacturingOrder, InventoryEngine};
use crate::launch::{self, LaunchRecord, LaunchOutcome};
use crate::reputation::Reputation;
use crate::rocket::RocketDesign;
use crate::rocket_project::{RocketProject, RocketProjectId, RocketWorkEvent};
use crate::seed::GameSeed;
use crate::team::{EngineeringTeam, ManufacturingTeam, TeamId, TEAM_HIRING_COST,
    MANUFACTURING_HIRING_COST};
use crate::third_party::{self, ContractedEngine, ContractedEngineId, ThirdPartyEngine};

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
    pub next_team_id: u64,
    pub next_engine_id: u64,
    pub next_project_id: u64,
    pub next_flaw_id: u64,
    pub next_rocket_project_id: u64,
    pub next_contracted_engine_id: u64,
    pub teams: Vec<EngineeringTeam>,
    pub manufacturing_teams: Vec<ManufacturingTeam>,
    pub engine_projects: Vec<EngineProject>,
    pub rocket_projects: Vec<RocketProject>,
    pub third_party_catalog: Vec<ThirdPartyEngine>,
    pub contracted_engines: Vec<ContractedEngine>,
    pub rocket_designs: Vec<RocketDesign>,
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
}

impl Company {
    pub fn new(name: String, starting_money: f64, seed: &GameSeed) -> Self {
        let catalog = third_party::generate_starter_engines(seed);
        Company {
            name,
            money: starting_money,
            next_team_id: 1,
            next_engine_id: 1,
            next_project_id: 1,
            next_flaw_id: 1,
            next_rocket_project_id: 1,
            next_contracted_engine_id: 1,
            teams: Vec::new(),
            manufacturing_teams: Vec::new(),
            engine_projects: Vec::new(),
            rocket_projects: Vec::new(),
            third_party_catalog: catalog,
            contracted_engines: Vec::new(),
            rocket_designs: Vec::new(),
            manufacturing: Manufacturing::new(),
            notified_manufacturing_idle: false,
            active_contracts: Vec::new(),
            reputation: Reputation::new(),
            launch_history: Vec::new(),
            monthly_financials: VecDeque::new(),
            last_launch_date: None,
        }
    }

    /// Hire a new engineering team. Returns the event if successful.
    pub fn hire_team(&mut self, name: String) -> Option<GameEvent> {
        self.money -= TEAM_HIRING_COST;
        let id = TeamId(self.next_team_id);
        self.next_team_id += 1;
        let team = EngineeringTeam::new(id, name.clone());
        self.teams.push(team);
        Some(GameEvent::TeamHired { name })
    }

    /// Total number of teams.
    pub fn team_count(&self) -> usize {
        self.teams.len()
    }

    /// Number of engineering teams not assigned to any project.
    pub fn unassigned_team_count(&self) -> u32 {
        let assigned: u32 = self.engine_projects.iter()
            .map(|p| p.teams_assigned)
            .sum::<u32>()
            + self.rocket_projects.iter()
                .map(|p| p.teams_assigned)
                .sum::<u32>();
        (self.teams.len() as u32).saturating_sub(assigned)
    }

    /// Number of manufacturing teams not assigned to any order.
    pub fn unassigned_manufacturing_team_count(&self) -> u32 {
        let assigned = self.manufacturing.total_teams_assigned();
        (self.manufacturing_teams.len() as u32).saturating_sub(assigned)
    }

    /// Total monthly salary cost for all teams (engineering + manufacturing).
    pub fn monthly_salary_cost(&self) -> f64 {
        let eng: f64 = self.teams.iter().map(|t| t.monthly_salary).sum();
        let mfg: f64 = self.manufacturing_teams.iter().map(|t| t.monthly_salary).sum();
        eng + mfg
    }

    /// Hire a manufacturing team.
    pub fn hire_manufacturing_team(&mut self, name: String) -> Option<GameEvent> {
        self.money -= MANUFACTURING_HIRING_COST;
        let id = TeamId(self.next_team_id);
        self.next_team_id += 1;
        let team = ManufacturingTeam::new(id, name.clone());
        self.manufacturing_teams.push(team);
        Some(GameEvent::ManufacturingTeamHired { name })
    }

    /// Start a new engine design project. Returns the event if successful.
    pub fn start_engine_project(
        &mut self,
        name: String,
        cycle: EngineCycle,
        preset: PropellantPreset,
        scale: f64,
        use_vacuum_isp: bool,
    ) -> Option<GameEvent> {
        let project_id = EngineProjectId(self.next_project_id);
        let engine_id = EngineId(self.next_engine_id);
        self.next_project_id += 1;
        self.next_engine_id += 1;

        let project = EngineProject::new(
            project_id, engine_id, name.clone(),
            cycle, preset, scale, use_vacuum_isp,
        )?;
        self.engine_projects.push(project);
        Some(GameEvent::EngineDesignStarted { engine_name: name })
    }

    /// Add a team to a project. Returns true if successful.
    pub fn add_team_to_project(&mut self, project_index: usize) -> bool {
        if self.unassigned_team_count() == 0 || project_index >= self.engine_projects.len() {
            return false;
        }
        self.engine_projects[project_index].teams_assigned += 1;
        true
    }

    /// Remove a team from a project. Returns true if successful.
    pub fn remove_team_from_project(&mut self, project_index: usize) -> bool {
        if project_index >= self.engine_projects.len() {
            return false;
        }
        let project = &mut self.engine_projects[project_index];
        if project.teams_assigned == 0 {
            return false;
        }
        project.teams_assigned -= 1;
        true
    }

    /// Start a new rocket design project. Returns the event if successful.
    pub fn start_rocket_project(&mut self, design: RocketDesign) -> Option<GameEvent> {
        let project_id = RocketProjectId(self.next_rocket_project_id);
        self.next_rocket_project_id += 1;
        let name = design.name.clone();
        let project = RocketProject::new(project_id, design);
        self.rocket_projects.push(project);
        Some(GameEvent::RocketDesignStarted { rocket_name: name })
    }

    /// Add an engineering team to a rocket project. Returns true if successful.
    pub fn add_team_to_rocket_project(&mut self, project_index: usize) -> bool {
        if self.unassigned_team_count() == 0 || project_index >= self.rocket_projects.len() {
            return false;
        }
        self.rocket_projects[project_index].teams_assigned += 1;
        true
    }

    /// Remove an engineering team from a rocket project. Returns true if successful.
    pub fn remove_team_from_rocket_project(&mut self, project_index: usize) -> bool {
        if project_index >= self.rocket_projects.len() {
            return false;
        }
        if self.rocket_projects[project_index].teams_assigned == 0 {
            return false;
        }
        self.rocket_projects[project_index].teams_assigned -= 1;
        true
    }

    /// Add a manufacturing team to a manufacturing order. Returns true if successful.
    pub fn add_team_to_manufacturing_order(&mut self, order_index: usize) -> bool {
        let available = self.unassigned_manufacturing_team_count();
        self.manufacturing.add_team_to_order(order_index, available)
    }

    /// Remove a manufacturing team from a manufacturing order. Returns true if successful.
    pub fn remove_team_from_manufacturing_order(&mut self, order_index: usize) -> bool {
        self.manufacturing.remove_team_from_order(order_index)
    }

    /// Order construction of a rocket. Auto-queues engine, stage, and integration orders.
    /// Returns the total material cost and event, or None if the rocket project isn't complete.
    pub fn order_rocket_build(&mut self, rocket_project_index: usize) -> Option<(f64, GameEvent)> {
        if rocket_project_index >= self.rocket_projects.len() {
            return None;
        }
        let rp = &self.rocket_projects[rocket_project_index];
        if !matches!(rp.status, crate::rocket_project::RocketDesignStatus::Testing { .. }) {
            return None;
        }

        let rocket_name = rp.design.name.clone();
        let rocket_project_id = rp.project_id;
        let mut total_cost = 0.0;

        // Queue engine build orders for each engine needed
        for (gi, group) in rp.design.stage_groups.iter().enumerate() {
            for (si, stage) in group.iter().enumerate() {
                let source = self.engine_source_for_id(stage.engine.id);
                for _e in 0..stage.engine_count {
                    match source {
                        Some(EngineSource::PlayerDesign(ep_id)) => {
                            // Find the engine project for manufacturing details
                            if let Some(ep) = self.engine_projects.iter()
                                .find(|ep| ep.project_id == ep_id)
                            {
                                let order_id = self.manufacturing.next_order_id();
                                let order = ManufacturingOrder::new_engine(
                                    order_id,
                                    EngineSource::PlayerDesign(ep_id),
                                    stage.engine.id,
                                    stage.engine.name.clone(),
                                    stage.engine.mass_kg,
                                    ep.complexity,
                                    ep.preset,
                                    0, // TODO: track prior builds per design
                                );
                                total_cost += order.material_cost;
                                self.manufacturing.orders.push(order);
                            }
                        }
                        Some(EngineSource::Contracted(ce_id)) => {
                            // Contracted engine: charge per-unit cost, instant delivery
                            if let Some(ce) = self.contracted_engines.iter()
                                .find(|ce| ce.id == ce_id)
                            {
                                total_cost += ce.purchase_cost_per_unit;
                                let item_id = self.manufacturing.next_inventory_id();
                                self.manufacturing.inventory.engines.push(InventoryEngine {
                                    item_id,
                                    source: EngineSource::Contracted(ce_id),
                                    engine_id: stage.engine.id,
                                    engine_name: stage.engine.name.clone(),
                                });
                            }
                        }
                        None => {}
                    }
                }

                // Queue stage build order
                let order_id = self.manufacturing.next_order_id();
                let stage_name = format!("{} {}-{}", rocket_name, gi, si);
                let order = ManufacturingOrder::new_stage(
                    order_id,
                    rocket_project_id,
                    gi, si,
                    stage_name,
                    stage.structural_mass_kg,
                    0,
                );
                total_cost += order.material_cost;
                self.manufacturing.orders.push(order);
            }
        }

        // Queue integration order
        let total_stages: u32 = rp.design.stage_groups.iter()
            .map(|g| g.len() as u32)
            .sum();
        let order_id = self.manufacturing.next_order_id();
        let design_id = rp.design.id;
        let integration_order = ManufacturingOrder::new_integration(
            order_id,
            rocket_project_id,
            design_id,
            rocket_name.clone(),
            total_stages,
            0,
        );
        total_cost += integration_order.material_cost;
        self.manufacturing.orders.push(integration_order);

        // Deduct costs
        self.money -= total_cost;

        // Reset idle notification since new orders were placed
        self.notified_manufacturing_idle = false;

        Some((total_cost, GameEvent::RocketBuildOrdered {
            rocket_name,
            total_cost,
        }))
    }

    /// Try to unblock stage and integration orders that have their prerequisites ready.
    pub fn try_unblock_manufacturing_orders(&mut self) {
        // Helper: find engine source by engine id (inline to avoid borrow issues)
        let find_source = |engine_id: EngineId, engine_projects: &[EngineProject], contracted_engines: &[ContractedEngine]| -> Option<EngineSource> {
            if let Some(ep) = engine_projects.iter().find(|ep| ep.design.id == engine_id) {
                return Some(EngineSource::PlayerDesign(ep.project_id));
            }
            if let Some(ce) = contracted_engines.iter().find(|ce| ce.design.id == engine_id) {
                return Some(EngineSource::Contracted(ce.id));
            }
            None
        };

        for order in &mut self.manufacturing.orders {
            if !order.waiting_for_prerequisites {
                continue;
            }
            match &order.order_type {
                crate::manufacturing::ManufacturingOrderType::Stage {
                    rocket_project_id, group_index, stage_index, ..
                } => {
                    // Stage needs engines for this stage
                    if let Some(rp) = self.rocket_projects.iter()
                        .find(|rp| rp.project_id == *rocket_project_id)
                    {
                        if let Some(stage) = rp.design.stage_groups
                            .get(*group_index)
                            .and_then(|g| g.get(*stage_index))
                        {
                            // Find engine source
                            if let Some(source) = find_source(stage.engine.id, &self.engine_projects, &self.contracted_engines) {
                                let available = self.manufacturing.inventory.engine_count(source);
                                if available >= stage.engine_count as usize {
                                    order.waiting_for_prerequisites = false;
                                    // Consume engines from inventory
                                    for _ in 0..stage.engine_count {
                                        self.manufacturing.inventory.take_engine(source);
                                    }
                                }
                            }
                        }
                    }
                }
                crate::manufacturing::ManufacturingOrderType::RocketIntegration {
                    rocket_project_id, ..
                } => {
                    // Integration needs all stages
                    if let Some(rp) = self.rocket_projects.iter()
                        .find(|rp| rp.project_id == *rocket_project_id)
                    {
                        let all_stages_ready = rp.design.stage_groups.iter().enumerate().all(|(gi, group)| {
                            group.iter().enumerate().all(|(si, _stage)| {
                                self.manufacturing.inventory.stage_count(*rocket_project_id, gi, si) >= 1
                            })
                        });
                        if all_stages_ready {
                            order.waiting_for_prerequisites = false;
                            // Consume stages from inventory
                            for (gi, group) in rp.design.stage_groups.iter().enumerate() {
                                for (si, _stage) in group.iter().enumerate() {
                                    self.manufacturing.inventory.take_stage(*rocket_project_id, gi, si);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Contract a third-party engine from the catalog.
    /// No upfront cost — per-unit cost is charged when building rockets.
    pub fn contract_third_party(&mut self, catalog_index: usize, current_date: GameDate, seed: &GameSeed) -> Option<GameEvent> {
        if catalog_index >= self.third_party_catalog.len() {
            return None;
        }
        let entry = &self.third_party_catalog[catalog_index];
        if current_date < entry.available_from {
            return None;
        }

        let id = ContractedEngineId(self.next_contracted_engine_id);
        self.next_contracted_engine_id += 1;
        let name = entry.design.name.clone();

        let flaws = third_party::generate_third_party_flaws(
            entry.complexity,
            seed,
            &name,
            &mut self.next_flaw_id,
        );

        let contracted = ContractedEngine {
            id,
            design: entry.design.clone(),
            preset: entry.preset,
            purchase_cost_per_unit: entry.purchase_cost_per_unit,
            flaws,
            complexity: entry.complexity,
        };
        self.contracted_engines.push(contracted);
        Some(GameEvent::EngineContracted { engine_name: name })
    }

    /// Whether any manufacturing order is actionable (not waiting for prerequisites).
    pub fn has_actionable_manufacturing_orders(&self) -> bool {
        self.manufacturing.orders.iter().any(|o| !o.waiting_for_prerequisites)
    }

    /// Auto-assign idle manufacturing teams to the order with the fewest teams.
    pub fn auto_assign_idle_manufacturing_teams(&mut self) {
        loop {
            if self.unassigned_manufacturing_team_count() == 0 {
                break;
            }
            // Find the non-waiting order with the fewest teams assigned
            let best = self.manufacturing.orders.iter().enumerate()
                .filter(|(_, o)| !o.waiting_for_prerequisites)
                .min_by_key(|(_, o)| o.teams_assigned)
                .map(|(i, _)| i);
            match best {
                Some(idx) => {
                    let available = self.unassigned_manufacturing_team_count();
                    self.manufacturing.add_team_to_order(idx, available);
                }
                None => break,
            }
        }
    }

    /// Steal an engineering team from the busiest project and assign to the target engine project.
    /// Returns the name of the project stolen from, or None if no team can be stolen.
    pub fn steal_engineering_team_to_engine_project(&mut self, target: usize) -> Option<String> {
        if target >= self.engine_projects.len() {
            return None;
        }
        // Find the engine or rocket project with the most teams assigned (>0, not target engine project)
        let mut best_source: Option<(&str, u32)> = None;
        let mut best_kind: Option<(bool, usize)> = None; // (is_engine, index)

        for (i, ep) in self.engine_projects.iter().enumerate() {
            if i == target || ep.teams_assigned == 0 { continue; }
            if best_source.is_none() || ep.teams_assigned > best_source.unwrap().1 {
                best_source = Some((&ep.design.name, ep.teams_assigned));
                best_kind = Some((true, i));
            }
        }
        for (i, rp) in self.rocket_projects.iter().enumerate() {
            if rp.teams_assigned == 0 { continue; }
            if best_source.is_none() || rp.teams_assigned > best_source.unwrap().1 {
                best_source = Some((&rp.design.name, rp.teams_assigned));
                best_kind = Some((false, i));
            }
        }

        let (is_engine, idx) = best_kind?;
        let name = if is_engine {
            let n = self.engine_projects[idx].design.name.clone();
            self.engine_projects[idx].teams_assigned -= 1;
            n
        } else {
            let n = self.rocket_projects[idx].design.name.clone();
            self.rocket_projects[idx].teams_assigned -= 1;
            n
        };
        self.engine_projects[target].teams_assigned += 1;
        Some(name)
    }

    /// Steal an engineering team from the busiest project and assign to the target rocket project.
    pub fn steal_engineering_team_to_rocket_project(&mut self, target: usize) -> Option<String> {
        if target >= self.rocket_projects.len() {
            return None;
        }
        let mut best_source: Option<(&str, u32)> = None;
        let mut best_kind: Option<(bool, usize)> = None;

        for (i, ep) in self.engine_projects.iter().enumerate() {
            if ep.teams_assigned == 0 { continue; }
            if best_source.is_none() || ep.teams_assigned > best_source.unwrap().1 {
                best_source = Some((&ep.design.name, ep.teams_assigned));
                best_kind = Some((true, i));
            }
        }
        for (i, rp) in self.rocket_projects.iter().enumerate() {
            if i == target || rp.teams_assigned == 0 { continue; }
            if best_source.is_none() || rp.teams_assigned > best_source.unwrap().1 {
                best_source = Some((&rp.design.name, rp.teams_assigned));
                best_kind = Some((false, i));
            }
        }

        let (is_engine, idx) = best_kind?;
        let name = if is_engine {
            let n = self.engine_projects[idx].design.name.clone();
            self.engine_projects[idx].teams_assigned -= 1;
            n
        } else {
            let n = self.rocket_projects[idx].design.name.clone();
            self.rocket_projects[idx].teams_assigned -= 1;
            n
        };
        self.rocket_projects[target].teams_assigned += 1;
        Some(name)
    }

    /// Steal a manufacturing team from the busiest order and assign to the target order.
    pub fn steal_manufacturing_team_to_order(&mut self, target: usize) -> Option<String> {
        if target >= self.manufacturing.orders.len() {
            return None;
        }
        if self.manufacturing.orders[target].waiting_for_prerequisites {
            return None;
        }
        // Find non-waiting order with most teams (>0, not target)
        let best = self.manufacturing.orders.iter().enumerate()
            .filter(|(i, o)| *i != target && !o.waiting_for_prerequisites && o.teams_assigned > 0)
            .max_by_key(|(_, o)| o.teams_assigned)
            .map(|(i, o)| (i, o.order_type.display_name()));

        let (idx, name) = best?;
        self.manufacturing.orders[idx].teams_assigned -= 1;
        self.manufacturing.orders[target].teams_assigned += 1;
        Some(name)
    }

    /// Look up the EngineSource for an engine by its EngineId.
    pub fn engine_source_for_id(&self, engine_id: EngineId) -> Option<EngineSource> {
        // Check player engine projects first
        if let Some(ep) = self.engine_projects.iter()
            .find(|ep| ep.design.id == engine_id)
        {
            return Some(EngineSource::PlayerDesign(ep.project_id));
        }
        // Check contracted engines
        if let Some(ce) = self.contracted_engines.iter()
            .find(|ce| ce.design.id == engine_id)
        {
            return Some(EngineSource::Contracted(ce.id));
        }
        None
    }
}

const EVENT_LOG_SIZE: usize = 1000;

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
}

fn default_next_contract_id() -> u64 { 1 }

impl GameState {
    pub fn new(company_name: String, starting_money: f64, seed_value: u64) -> Self {
        let start = GameDate::default_start();
        let mut event_log = EventLog::new(EVENT_LOG_SIZE);
        event_log.push(start, GameEvent::GameStarted);
        let seed = GameSeed::new(seed_value);

        GameState {
            date: start,
            start_date: start,
            player_company: Company::new(company_name, starting_money, &seed),
            event_log,
            seed,
            speed: GameSpeed::Paused,
            previous_speed: GameSpeed::Normal,
            available_contracts: Vec::new(),
            next_contract_id: 1,
        }
    }

    /// Advance the game by one day. Returns events generated this tick.
    pub fn advance_day(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();

        self.date = self.date.next_day();

        // Process daily work on engine and rocket projects
        {
            let rng = &mut self.seed.contingent_rng;
            let next_flaw_id = &mut self.player_company.next_flaw_id;

            for project in &mut self.player_company.engine_projects {
                let engine_name = project.design.name.clone();
                let work_events = project.apply_daily_work(rng, next_flaw_id);
                for we in work_events {
                    let evt = match we {
                        WorkEvent::DesignComplete { flaw_count } =>
                            GameEvent::EngineDesignComplete { engine_name: engine_name.clone(), flaw_count },
                        WorkEvent::TestingCycleComplete => continue,
                        WorkEvent::FlawDiscovered { flaw_description } =>
                            GameEvent::FlawDiscovered { engine_name: engine_name.clone(), flaw_description },
                        WorkEvent::RevisionComplete =>
                            GameEvent::RevisionComplete { engine_name: engine_name.clone() },
                    };
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                }
            }

            for project in &mut self.player_company.rocket_projects {
                let rocket_name = project.design.name.clone();
                let work_events = project.apply_daily_work(rng, next_flaw_id);
                for we in work_events {
                    let evt = match we {
                        RocketWorkEvent::DesignComplete { flaw_count } =>
                            GameEvent::RocketDesignComplete { rocket_name: rocket_name.clone(), flaw_count },
                        RocketWorkEvent::TestingCycleComplete => continue,
                        RocketWorkEvent::FlawDiscovered { flaw_description } =>
                            GameEvent::RocketFlawDiscovered { rocket_name: rocket_name.clone(), flaw_description },
                        RocketWorkEvent::RevisionComplete =>
                            GameEvent::RocketRevisionComplete { rocket_name: rocket_name.clone() },
                    };
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                }
            }
        }

        if self.date.is_first_of_month() {
            let evt = GameEvent::MonthStart;
            self.event_log.push(self.date, evt.clone());
            events.push(evt);

            // Deduct salaries
            let salary = self.player_company.monthly_salary_cost();
            if salary > 0.0 {
                self.player_company.money -= salary;
                // Track expense
                self.record_expense(salary);
                let evt = GameEvent::SalariesPaid { amount: salary };
                self.event_log.push(self.date, evt.clone());
                events.push(evt);

                if self.player_company.money < 0.0 {
                    let evt = GameEvent::InsufficientFunds {
                        shortfall: -self.player_company.money,
                    };
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                }
            }

            // Generate monthly contract
            let rep = self.player_company.reputation.total();
            let query = format!("contracts_{}_{}", self.date.year, self.date.month);
            let mut rng = self.seed.world_query(&query);
            let contract_id = ContractId(self.next_contract_id);
            self.next_contract_id += 1;
            if let Some(c) = contract::generate_monthly_contract(
                &mut rng, contract_id, self.date, rep,
            ) {
                let evt = GameEvent::ContractsRefreshed { count: 1 };
                self.event_log.push(self.date, evt.clone());
                events.push(evt);
                self.available_contracts.push(c);
            }

            // Start new month in financials
            self.ensure_current_month_financials();
        }

        // Expire contracts past deadline
        self.expire_contracts(&mut events);

        // Track launch drought (yearly check)
        if self.date.is_first_of_month() && self.date.month == 1 && self.date.day == 1 {
            if let Some(last) = self.player_company.last_launch_date {
                let days_since = last.days_until(&self.date);
                if days_since >= 365 {
                    self.player_company.reputation.on_year_without_launch();
                }
            } else if self.date != self.start_date {
                // Never launched and at least a year has passed
                let days_since_start = self.start_date.days_until(&self.date);
                if days_since_start >= 365 {
                    self.player_company.reputation.on_year_without_launch();
                }
            }
        }

        // Process manufacturing
        let mfg_events = self.player_company.manufacturing.advance_day();
        for me in mfg_events {
            let evt = match me {
                crate::manufacturing::ManufacturingEvent::EngineBuilt { engine_name, .. } =>
                    GameEvent::EngineBuilt { engine_name },
                crate::manufacturing::ManufacturingEvent::StageBuilt { stage_name, .. } =>
                    GameEvent::StageBuilt { stage_name },
                crate::manufacturing::ManufacturingEvent::RocketIntegrated { rocket_name, .. } =>
                    GameEvent::RocketIntegrated { rocket_name },
                crate::manufacturing::ManufacturingEvent::FloorSpaceComplete { units } =>
                    GameEvent::FloorSpaceComplete { units },
            };
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }

        // Try to unblock manufacturing orders that now have prerequisites
        self.player_company.try_unblock_manufacturing_orders();

        // Auto-assign idle manufacturing teams to least-staffed orders
        self.player_company.auto_assign_idle_manufacturing_teams();

        // Pause on transition to idle manufacturing
        if !self.player_company.manufacturing_teams.is_empty()
            && !self.player_company.has_actionable_manufacturing_orders()
            && !self.player_company.notified_manufacturing_idle
        {
            self.speed = GameSpeed::Paused;
            self.player_company.notified_manufacturing_idle = true;
            let evt = GameEvent::ManufacturingIdle;
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }
        if self.player_company.has_actionable_manufacturing_orders() {
            self.player_company.notified_manufacturing_idle = false;
        }

        events
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
    fn ensure_current_month_financials(&mut self) {
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
    fn record_expense(&mut self, amount: f64) {
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
    fn record_income(&mut self, amount: f64) {
        self.ensure_current_month_financials();
        let year = self.date.year;
        let month = self.date.month;
        if let Some(f) = self.player_company.monthly_financials.iter_mut()
            .find(|f| f.year == year && f.month == month)
        {
            f.income += amount;
        }
    }

    /// Expire contracts past their deadline and update reputation.
    fn expire_contracts(&mut self, events: &mut Vec<GameEvent>) {
        // Check available contracts
        let mut expired_available = Vec::new();
        for (i, c) in self.available_contracts.iter().enumerate() {
            if self.date > c.deadline {
                expired_available.push(i);
            }
        }
        for i in expired_available.into_iter().rev() {
            self.available_contracts.remove(i);
        }

        // Check accepted contracts on the company
        let mut expired_accepted = Vec::new();
        for (i, c) in self.player_company.active_contracts.iter().enumerate() {
            if self.date > c.deadline {
                expired_accepted.push((i, c.name.clone()));
            }
        }
        for (i, name) in expired_accepted.into_iter().rev() {
            self.player_company.active_contracts.remove(i);
            self.player_company.reputation.on_contract_expired();
            let evt = GameEvent::ContractExpired { contract_name: name };
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }
    }

    /// Accept a contract from the available market.
    pub fn accept_contract(&mut self, index: usize) -> Option<GameEvent> {
        if index >= self.available_contracts.len() {
            return None;
        }
        let mut c = self.available_contracts.remove(index);
        let name = c.name.clone();
        c.status = contract::ContractStatus::Accepted;
        self.player_company.active_contracts.push(c);
        let evt = GameEvent::ContractAccepted { contract_name: name };
        self.event_log.push(self.date, evt.clone());
        Some(evt)
    }

    /// Launch a rocket for a contract (or test launch if contract_index is None).
    /// `rocket_item_id` identifies the InventoryRocket to consume.
    /// `contract_index` is the index into player_company.active_contracts (None for test launch).
    /// Returns the events generated and the launch record.
    pub fn launch_rocket(
        &mut self,
        rocket_item_id: crate::manufacturing::InventoryItemId,
        contract_index: Option<usize>,
        destination: &str,
        payload_kg: f64,
    ) -> Option<(Vec<GameEvent>, LaunchRecord)> {
        // Take the rocket from inventory
        let inv_rocket = self.player_company.manufacturing.inventory.take_rocket(rocket_item_id)?;

        // Find the rocket project for this rocket
        let rp = self.player_company.rocket_projects.iter()
            .find(|rp| rp.project_id == inv_rocket.rocket_project_id)?;

        // Simulate the launch
        let sim = launch::simulate_launch(
            &rp.design,
            destination,
            payload_kg,
            &self.player_company.engine_projects,
            rp,
            &self.player_company.contracted_engines,
            &mut self.seed.contingent_rng,
        );

        let mut events = Vec::new();

        // Mark activated flaws as discovered on engine projects
        for (engine_id, indices) in &sim.engine_flaw_discoveries {
            if let Some(ep) = self.player_company.engine_projects.iter_mut()
                .find(|ep| ep.design.id == *engine_id)
            {
                for &idx in indices {
                    if idx < ep.flaws.len() {
                        ep.flaws[idx].discovered = true;
                        let evt = GameEvent::FlawDiscovered {
                            engine_name: ep.design.name.clone(),
                            flaw_description: ep.flaws[idx].description.clone(),
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    }
                }
            }
        }

        // Mark activated flaws as discovered on contracted engines
        for (source, indices) in &sim.contracted_flaw_discoveries {
            if let EngineSource::Contracted(ce_id) = source {
                if let Some(ce) = self.player_company.contracted_engines.iter_mut()
                    .find(|ce| ce.id == *ce_id)
                {
                    for &idx in indices {
                        if idx < ce.flaws.len() {
                            ce.flaws[idx].discovered = true;
                        }
                    }
                }
            }
        }

        // Mark activated flaws as discovered on rocket project
        {
            // Re-find as mutable
            if let Some(rp_mut) = self.player_company.rocket_projects.iter_mut()
                .find(|rp| rp.project_id == inv_rocket.rocket_project_id)
            {
                for &idx in &sim.rocket_flaw_discoveries {
                    if idx < rp_mut.flaws.len() {
                        rp_mut.flaws[idx].discovered = true;
                        let evt = GameEvent::RocketFlawDiscovered {
                            rocket_name: rp_mut.design.name.clone(),
                            flaw_description: rp_mut.flaws[idx].description.clone(),
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    }
                }
            }
        }

        // Determine contract info
        let contract_id = contract_index.and_then(|ci| {
            self.player_company.active_contracts.get(ci).map(|c| c.id)
        });
        let contract_name = contract_index.and_then(|ci| {
            self.player_company.active_contracts.get(ci).map(|c| c.name.clone())
        });

        // Apply results based on outcome
        let launch_evt = match &sim.outcome {
            LaunchOutcome::Success => {
                self.player_company.reputation.on_launch_success();

                if let Some(ci) = contract_index {
                    let payment = self.player_company.active_contracts[ci].payment;
                    self.player_company.money += payment;
                    self.record_income(payment);
                    self.player_company.reputation.on_contract_launch();

                    let pay_evt = GameEvent::PaymentReceived {
                        amount: payment,
                        contract_name: contract_name.clone().unwrap_or_default(),
                    };
                    self.event_log.push(self.date, pay_evt.clone());
                    events.push(pay_evt);

                    // Mark contract completed and remove
                    self.player_company.active_contracts.remove(ci);
                }

                GameEvent::LaunchSuccess {
                    rocket_name: inv_rocket.rocket_name.clone(),
                    destination: destination.to_string(),
                }
            }
            LaunchOutcome::PartialFailure { reason } => {
                self.player_company.reputation.on_launch_partial_failure();

                if let Some(ci) = contract_index {
                    // 50% payment
                    let payment = self.player_company.active_contracts[ci].payment * 0.5;
                    self.player_company.money += payment;
                    self.record_income(payment);
                    self.player_company.reputation.on_contract_launch();

                    let pay_evt = GameEvent::PaymentReceived {
                        amount: payment,
                        contract_name: contract_name.clone().unwrap_or_default(),
                    };
                    self.event_log.push(self.date, pay_evt.clone());
                    events.push(pay_evt);

                    self.player_company.active_contracts.remove(ci);
                }

                GameEvent::LaunchPartialFailure {
                    rocket_name: inv_rocket.rocket_name.clone(),
                    reason: reason.clone(),
                }
            }
            LaunchOutcome::Failure { reason } => {
                self.player_company.reputation.on_launch_failure();

                if let Some(ci) = contract_index {
                    // No payment, contract failed
                    self.player_company.active_contracts.remove(ci);
                }

                GameEvent::LaunchFailure {
                    rocket_name: inv_rocket.rocket_name.clone(),
                    reason: reason.clone(),
                }
            }
        };

        self.event_log.push(self.date, launch_evt.clone());
        events.push(launch_evt);

        // Update launch tracking
        self.player_company.last_launch_date = Some(self.date);

        let record = LaunchRecord {
            launch_date: self.date,
            rocket_name: inv_rocket.rocket_name,
            contract_id,
            destination: destination.to_string(),
            payload_kg,
            outcome: sim.outcome,
            flaws_activated: sim.flaws_activated,
        };
        self.player_company.launch_history.push(record.clone());

        // Pause the game for the player to see the result
        self.speed = GameSpeed::Paused;

        Some((events, record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_game_state() {
        let gs = GameState::new("SpaceCorp".into(), 200_000_000.0, 42);
        assert_eq!(gs.date, GameDate::default_start());
        assert_eq!(gs.player_company.name, "SpaceCorp");
        assert_eq!(gs.player_company.money, 200_000_000.0);
        assert_eq!(gs.speed, GameSpeed::Paused);
        assert_eq!(gs.elapsed_days(), 0);
        // Should have GameStarted event
        assert_eq!(gs.event_log.len(), 1);
    }

    #[test]
    fn test_advance_day() {
        let mut gs = GameState::new("Test".into(), 100.0, 1);
        let events = gs.advance_day();
        assert_eq!(gs.date, GameDate::new(2001, 1, 2));
        assert_eq!(gs.elapsed_days(), 1);
        // Normal day should produce no events (DayAdvanced no longer logged)
        assert!(events.is_empty());
    }

    #[test]
    fn test_advance_to_new_month() {
        let mut gs = GameState::new("Test".into(), 100.0, 1);
        // Advance 31 days to get to Feb 1
        for _ in 0..31 {
            gs.advance_day();
        }
        assert_eq!(gs.date, GameDate::new(2001, 2, 1));
        // Last tick should have produced MonthStart
        let recent = gs.event_log.recent(3);
        assert!(recent.iter().any(|(_, e)| matches!(e, GameEvent::MonthStart)));
    }

    #[test]
    fn test_toggle_pause() {
        let mut gs = GameState::new("Test".into(), 100.0, 1);
        assert_eq!(gs.speed, GameSpeed::Paused);

        gs.toggle_pause();
        assert_eq!(gs.speed, GameSpeed::Normal);

        gs.toggle_pause();
        assert_eq!(gs.speed, GameSpeed::Paused);

        // Should remember Normal
        gs.toggle_pause();
        assert_eq!(gs.speed, GameSpeed::Normal);
    }

    #[test]
    fn test_toggle_pause_remembers_speed() {
        let mut gs = GameState::new("Test".into(), 100.0, 1);
        gs.set_speed(GameSpeed::VeryFast);
        assert_eq!(gs.speed, GameSpeed::VeryFast);

        gs.toggle_pause();
        assert_eq!(gs.speed, GameSpeed::Paused);

        // Should restore VeryFast, not Normal
        gs.toggle_pause();
        assert_eq!(gs.speed, GameSpeed::VeryFast);
    }

    #[test]
    fn test_set_speed() {
        let mut gs = GameState::new("Test".into(), 100.0, 1);
        gs.set_speed(GameSpeed::Fast);
        assert_eq!(gs.speed, GameSpeed::Fast);
        gs.set_speed(GameSpeed::VeryFast);
        assert_eq!(gs.speed, GameSpeed::VeryFast);
    }

    #[test]
    fn test_speed_tick_ms() {
        assert!(GameSpeed::Normal.tick_ms() > GameSpeed::Fast.tick_ms());
        assert!(GameSpeed::Fast.tick_ms() > GameSpeed::VeryFast.tick_ms());
    }

    #[test]
    fn test_elapsed_days_after_year() {
        let mut gs = GameState::new("Test".into(), 100.0, 1);
        for _ in 0..365 {
            gs.advance_day();
        }
        assert_eq!(gs.elapsed_days(), 365);
        assert_eq!(gs.date, GameDate::new(2002, 1, 1));
    }

    #[test]
    fn test_hire_team() {
        let mut gs = GameState::new("Test".into(), 1_000_000.0, 1);
        assert_eq!(gs.player_company.team_count(), 0);
        gs.player_company.hire_team("Alpha".into());
        assert_eq!(gs.player_company.team_count(), 1);
        assert_eq!(gs.player_company.money, 1_000_000.0 - 150_000.0);
    }

    #[test]
    fn test_salary_deduction() {
        let mut gs = GameState::new("Test".into(), 1_000_000.0, 1);
        gs.player_company.hire_team("Alpha".into());

        // Advance to Feb 1 (31 days)
        for _ in 0..31 {
            gs.advance_day();
        }
        // Should have paid 1 month salary
        let expected = 1_000_000.0 - 150_000.0 - 150_000.0; // hiring + first month
        assert!((gs.player_company.money - expected).abs() < 0.01);
    }

    #[test]
    fn test_negative_money_allowed() {
        let mut gs = GameState::new("Test".into(), 100_000.0, 1);
        gs.player_company.hire_team("Alpha".into()); // -150K, now -50K
        assert!(gs.player_company.money < 0.0);
        // Should still work, just go negative
        for _ in 0..31 {
            gs.advance_day();
        }
        // Should have deducted another salary on top
        assert!(gs.player_company.money < -50_000.0);
    }

    #[test]
    fn test_start_engine_project() {
        let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
        let evt = gs.player_company.start_engine_project(
            "Kestrel".into(),
            crate::engine::EngineCycle::GasGenerator,
            crate::engine_project::PropellantPreset::Kerolox,
            1.0,
            true,
        );
        assert!(evt.is_some());
        assert_eq!(gs.player_company.engine_projects.len(), 1);
    }

    #[test]
    fn test_team_assignment() {
        let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
        gs.player_company.hire_team("Alpha".into());
        gs.player_company.start_engine_project(
            "Kestrel".into(),
            crate::engine::EngineCycle::GasGenerator,
            crate::engine_project::PropellantPreset::Kerolox,
            1.0,
            true,
        );

        assert_eq!(gs.player_company.unassigned_team_count(), 1);
        assert!(gs.player_company.add_team_to_project(0));
        assert_eq!(gs.player_company.unassigned_team_count(), 0);

        // Can't assign more than available
        assert!(!gs.player_company.add_team_to_project(0));

        // Can remove
        assert!(gs.player_company.remove_team_from_project(0));
        assert_eq!(gs.player_company.unassigned_team_count(), 1);
    }

    #[test]
    fn test_third_party_catalog() {
        let gs = GameState::new("Test".into(), 200_000_000.0, 42);
        assert_eq!(gs.player_company.third_party_catalog.len(), 3);
    }

    #[test]
    fn test_contract_third_party() {
        let mut gs = GameState::new("Test".into(), 200_000_000.0, 42);
        let initial_money = gs.player_company.money;
        let date = gs.date;
        let seed = gs.seed.clone();

        let evt = gs.player_company.contract_third_party(0, date, &seed);
        assert!(evt.is_some());
        assert_eq!(gs.player_company.contracted_engines.len(), 1);
        // No money deducted for contracting
        assert!((gs.player_company.money - initial_money).abs() < 0.01);
        // Engine should not be added to engine_projects
        assert_eq!(gs.player_company.engine_projects.len(), 0);
    }

    #[test]
    fn test_design_work_progresses() {
        let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
        gs.player_company.hire_team("Alpha".into());
        gs.player_company.start_engine_project(
            "Kestrel".into(),
            crate::engine::EngineCycle::GasGenerator,
            crate::engine_project::PropellantPreset::Kerolox,
            1.0,
            true,
        );
        gs.player_company.add_team_to_project(0);

        // Advance 10 days
        for _ in 0..10 {
            gs.advance_day();
        }

        // Check work progressed
        match &gs.player_company.engine_projects[0].status {
            crate::engine_project::EngineDesignStatus::InDesign { work_completed, .. } => {
                assert!(*work_completed > 9.0, "Should have ~10 work units after 10 days with 1 team");
            }
            _ => {} // might have completed if work_required was low enough (unlikely for complexity 6)
        }
    }
}
