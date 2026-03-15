use std::collections::{HashMap, VecDeque};

use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::contract::{self, Contract, ContractId};
use crate::engine::{EngineCycle, EngineId};
use crate::engine_project::{EngineProject, EngineProjectId, EngineSource, PropellantPreset, WorkEvent};
use crate::flight::{Flight, FlightId, FlightStatus, Payload};
use crate::event::{EventLog, GameEvent};
use crate::manufacturing::{Manufacturing, ManufacturingOrder, InventoryEngine};
use crate::launch::{self, LaunchRecord, LaunchOutcome};
use crate::reputation::Reputation;
use crate::rocket::{RocketDesign, RocketDesignId, RocketId};
use crate::rocket_project::{RocketProject, RocketProjectId, RocketWorkEvent};
use crate::seed::GameSeed;
use crate::team::{EngineeringTeam, ManufacturingTeam, TeamId, TEAM_HIRING_COST,
    MANUFACTURING_HIRING_COST, ENGINEERING_MONTHLY_SALARY};
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
}

impl Spacecraft {
    /// Remaining delta-v with no payload.
    pub fn remaining_delta_v(&self) -> f64 {
        self.rocket.remaining_delta_v(&self.design)
    }
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
    /// How many engines have been built per engine project (for learning curve).
    #[serde(default)]
    pub engine_build_counts: HashMap<EngineProjectId, u32>,
    /// How many rockets have been built per design (for learning curve).
    #[serde(default)]
    pub rocket_build_counts: HashMap<RocketDesignId, u32>,
    /// Build cost history per rocket design (for avg/marginal cost).
    #[serde(default)]
    pub rocket_cost_history: HashMap<RocketDesignId, Vec<f64>>,
    /// Auto-build targets: maintain at least N rockets in inventory per project.
    #[serde(default)]
    pub auto_build_targets: HashMap<RocketProjectId, u32>,
}

impl Company {
    pub fn new(name: String, starting_money: f64, seed: &GameSeed) -> Self {
        let catalog = third_party::generate_starter_engines(seed);
        let mut company = Company {
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
            engine_build_counts: HashMap::new(),
            rocket_build_counts: HashMap::new(),
            rocket_cost_history: HashMap::new(),
            auto_build_targets: HashMap::new(),
        };
        // Start with one engineering team
        company.hire_team("Team 1".into());
        company
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
        let design_id = rp.design.id;
        let mut total_cost = 0.0;

        // Get current build count for this rocket design (for learning curve)
        let rocket_prior = *self.rocket_build_counts.get(&design_id).unwrap_or(&0);

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
                                let engine_prior = *self.engine_build_counts.get(&ep_id).unwrap_or(&0);
                                let order_id = self.manufacturing.next_order_id();
                                let order = ManufacturingOrder::new_engine(
                                    order_id,
                                    EngineSource::PlayerDesign(ep_id),
                                    stage.engine.id,
                                    stage.engine.name.clone(),
                                    stage.engine.mass_kg,
                                    ep.complexity,
                                    ep.preset,
                                    engine_prior,
                                );
                                total_cost += order.material_cost;
                                self.manufacturing.orders.push(order);
                                *self.engine_build_counts.entry(ep_id).or_insert(0) += 1;
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
                                    build_cost: ce.purchase_cost_per_unit,
                                });
                            }
                        }
                        None => {}
                    }
                }

                // Queue stage build order
                let order_id = self.manufacturing.next_order_id();
                let stage_label = if group.len() == 1 {
                    format!("{}", gi + 1)
                } else {
                    let suffix = (b'a' + si as u8) as char;
                    format!("{}{}", gi + 1, suffix)
                };
                let stage_name = format!("{} S{}", rocket_name, stage_label);
                let order = ManufacturingOrder::new_stage(
                    order_id,
                    rocket_project_id,
                    gi, si,
                    stage_name,
                    stage.structural_mass_kg,
                    rocket_prior,
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
        let integration_order = ManufacturingOrder::new_integration(
            order_id,
            rocket_project_id,
            design_id,
            rocket_name.clone(),
            total_stages,
            rocket_prior,
        );
        total_cost += integration_order.material_cost;
        self.manufacturing.orders.push(integration_order);

        // Increment rocket build count
        *self.rocket_build_counts.entry(design_id).or_insert(0) += 1;

        // Deduct costs
        self.money -= total_cost;

        // Reset idle notification since new orders were placed
        self.notified_manufacturing_idle = false;

        Some((total_cost, GameEvent::RocketBuildOrdered {
            rocket_name,
            total_cost,
        }))
    }

    /// Order a standalone engine build for a player-designed engine project.
    pub fn order_engine_build(&mut self, engine_project_index: usize) -> Option<(f64, GameEvent)> {
        if engine_project_index >= self.engine_projects.len() {
            return None;
        }
        let ep = &self.engine_projects[engine_project_index];
        if !matches!(ep.status, crate::engine_project::EngineDesignStatus::Testing { .. }) {
            return None;
        }

        let engine_name = ep.design.name.clone();
        let ep_id = ep.project_id;
        let engine_id = ep.design.id;
        let mass_kg = ep.design.mass_kg;
        let complexity = ep.complexity;
        let preset = ep.preset;
        let engine_prior = *self.engine_build_counts.get(&ep_id).unwrap_or(&0);

        let order_id = self.manufacturing.next_order_id();
        let order = ManufacturingOrder::new_engine(
            order_id,
            EngineSource::PlayerDesign(ep_id),
            engine_id,
            engine_name.clone(),
            mass_kg,
            complexity,
            preset,
            engine_prior,
        );
        let cost = order.material_cost;
        self.manufacturing.orders.push(order);
        *self.engine_build_counts.entry(ep_id).or_insert(0) += 1;
        self.money -= cost;
        self.notified_manufacturing_idle = false;

        Some((cost, GameEvent::EngineBuildOrdered { engine_name }))
    }

    /// Automatically order rocket builds to maintain auto_build_targets inventory levels.
    fn auto_reorder_rockets(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let targets: Vec<(RocketProjectId, u32)> = self.auto_build_targets.iter()
            .map(|(&pid, &count)| (pid, count))
            .collect();

        for (project_id, min_count) in targets {
            // Find the project index
            let index = match self.rocket_projects.iter().position(|rp| rp.project_id == project_id) {
                Some(i) => i,
                None => continue,
            };
            // Only auto-build for projects in Testing status
            if !matches!(self.rocket_projects[index].status, crate::rocket_project::RocketDesignStatus::Testing { .. }) {
                continue;
            }
            let current = self.manufacturing.inventory.rocket_count(project_id) as u32
                + self.manufacturing.pending_integration_orders(project_id);
            for _ in current..min_count {
                if let Some((_cost, evt)) = self.order_rocket_build(index) {
                    events.push(evt);
                }
            }
        }
        events
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
                                    // Consume engines from inventory, accumulating their build cost
                                    for _ in 0..stage.engine_count {
                                        if let Some(eng) = self.manufacturing.inventory.take_engine(source) {
                                            order.material_cost += eng.build_cost;
                                        }
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
                            // Consume stages from inventory, accumulating their build cost
                            for (gi, group) in rp.design.stage_groups.iter().enumerate() {
                                for (si, _stage) in group.iter().enumerate() {
                                    if let Some(stg) = self.manufacturing.inventory.take_stage(*rocket_project_id, gi, si) {
                                        order.material_cost += stg.build_cost;
                                    }
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
}

fn default_next_contract_id() -> u64 { 1 }
fn default_next_flight_id() -> u64 { 1 }
fn default_next_rocket_id() -> u64 { 1 }

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
            active_flights: Vec::new(),
            next_flight_id: 1,
            next_rocket_id: 1,
            spacecraft: Vec::new(),
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

            // Accumulate NRE (engineering salary) on active projects
            let daily_salary = ENGINEERING_MONTHLY_SALARY / 30.0;
            for project in &mut self.player_company.engine_projects {
                if project.teams_assigned > 0 {
                    project.nre_cost += project.teams_assigned as f64 * daily_salary;
                }
            }
            for project in &mut self.player_company.rocket_projects {
                if project.teams_assigned > 0 {
                    project.nre_cost += project.teams_assigned as f64 * daily_salary;
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
                crate::manufacturing::ManufacturingEvent::RocketIntegrated { rocket_name, design_id, build_cost, .. } => {
                    self.player_company.rocket_cost_history
                        .entry(design_id)
                        .or_default()
                        .push(build_cost);
                    GameEvent::RocketIntegrated { rocket_name }
                }
                crate::manufacturing::ManufacturingEvent::FloorSpaceComplete { units } =>
                    GameEvent::FloorSpaceComplete { units },
            };
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }

        // Try to unblock manufacturing orders that now have prerequisites
        self.player_company.try_unblock_manufacturing_orders();

        // Auto-reorder rockets to maintain inventory targets
        let auto_events = self.player_company.auto_reorder_rockets();
        for evt in auto_events {
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }

        // Auto-assign idle manufacturing teams to least-staffed orders
        self.player_company.auto_assign_idle_manufacturing_teams();

        // Advance flights in transit
        let flight_events = self.advance_flights();
        for evt in flight_events {
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }

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
    /// Returns the events generated. On catastrophic failure, also returns a LaunchRecord.
    /// On success/partial success, the rocket enters transit and resolves on arrival.
    pub fn launch_rocket(
        &mut self,
        rocket_item_id: crate::manufacturing::InventoryItemId,
        contract_index: Option<usize>,
        destination: &str,
        payload_kg: f64,
        persist: bool,
    ) -> Option<(Vec<GameEvent>, Option<LaunchRecord>)> {
        // Take the rocket from inventory
        let inv_rocket = self.player_company.manufacturing.inventory.take_rocket(rocket_item_id)?;

        // Find the rocket project for this rocket
        let rp = self.player_company.rocket_projects.iter()
            .find(|rp| rp.project_id == inv_rocket.rocket_project_id)?;

        // Simulate flaw activation at launch
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

        // Update launch tracking
        self.player_company.last_launch_date = Some(self.date);

        // Catastrophic failure at launch — resolve immediately
        if matches!(sim.outcome, LaunchOutcome::Failure { .. }) {
            let contract_id = contract_index.and_then(|ci| {
                self.player_company.active_contracts.get(ci).map(|c| c.id)
            });

            self.player_company.reputation.on_launch_failure();

            if let Some(ci) = contract_index {
                self.player_company.active_contracts.remove(ci);
            }

            let reason = match &sim.outcome {
                LaunchOutcome::Failure { reason } => reason.clone(),
                _ => unreachable!(),
            };
            let evt = GameEvent::LaunchFailure {
                rocket_name: inv_rocket.rocket_name.clone(),
                reason: reason.clone(),
            };
            self.event_log.push(self.date, evt.clone());
            events.push(evt);

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
            self.speed = GameSpeed::Paused;
            return Some((events, Some(record)));
        }

        // Success or partial failure — create a flight in transit
        let rocket_mass = sim.degraded_design.total_mass_kg() + payload_kg;
        let first_group_thrust = sim.degraded_design.group_thrust_n(0);

        let path = crate::location::DELTA_V_MAP
            .shortest_path("earth_surface", destination, rocket_mass);
        let route = match path {
            Some((path, _)) => crate::flight::build_route(&path, rocket_mass, first_group_thrust),
            None => vec![],
        };

        // Build payloads
        let payloads = if let Some(ci) = contract_index {
            let contract_id = self.player_company.active_contracts[ci].id;
            vec![Payload::ContractDelivery { contract_id, payload_kg }]
        } else {
            vec![Payload::TestMass { mass_kg: payload_kg }]
        };

        let flight_id = FlightId(self.next_flight_id);
        self.next_flight_id += 1;

        // Instantiate a Rocket with per-stage propellant tracking
        let rocket_instance_id = RocketId(self.next_rocket_id);
        self.next_rocket_id += 1;
        let rocket_instance = sim.degraded_design.instantiate(
            rocket_instance_id, "earth_surface", payload_kg,
        );

        let leg_days = route.first().map(|l| l.total_days()).unwrap_or(0);

        let dest_display = crate::contract::destination_display_name(destination);

        let flight = Flight {
            id: flight_id,
            rocket_name: inv_rocket.rocket_name.clone(),
            rocket_project_id: inv_rocket.rocket_project_id,
            design: sim.degraded_design,
            rocket: rocket_instance,
            payloads,
            current_location: "earth_surface".to_string(),
            route,
            current_leg: 0,
            leg_days_remaining: leg_days,
            status: FlightStatus::InTransit,
            flaws_activated: sim.flaws_activated,
            launch_date: self.date,
            persist,
            launch_partial: matches!(sim.outcome, LaunchOutcome::PartialFailure { .. }),
            flaw_rolled_groups: sim.flaw_rolled_groups,
        };

        self.active_flights.push(flight);

        let evt = GameEvent::FlightDeparted {
            rocket_name: inv_rocket.rocket_name,
            destination: dest_display.to_string(),
        };
        self.event_log.push(self.date, evt.clone());
        events.push(evt);

        self.speed = GameSpeed::Paused;

        Some((events, None))
    }

    /// Process daily flight advancement. Returns events generated.
    fn advance_flights(&mut self) -> Vec<GameEvent> {
        use rand::Rng;
        use crate::engine::EngineId;
        use crate::flaw::FlawConsequence;
        use crate::engine_project::EngineSource;

        let mut events = Vec::new();
        let mut arrived_indices = Vec::new();
        let mut stranded_indices = Vec::new();

        // Snapshot engine flaws keyed by engine_id for lookup during flight iteration.
        // Each entry: (engine_id, engine_name, flaw_index_in_project, flaw_data, source)
        struct FlawRef {
            engine_id: EngineId,
            engine_name: String,
            activation_chance: f64,
            consequence: FlawConsequence,
            description: String,
            source: EngineSource,
            flaw_index: usize,
        }
        let mut flaw_table: Vec<FlawRef> = Vec::new();
        for ep in &self.player_company.engine_projects {
            let source = EngineSource::PlayerDesign(ep.project_id);
            for (fi, flaw) in ep.flaws.iter().enumerate() {
                flaw_table.push(FlawRef {
                    engine_id: ep.design.id,
                    engine_name: ep.design.name.clone(),
                    activation_chance: flaw.activation_chance,
                    consequence: flaw.consequence.clone(),
                    description: flaw.description.clone(),
                    source,
                    flaw_index: fi,
                });
            }
        }
        for ce in &self.player_company.contracted_engines {
            let source = EngineSource::Contracted(ce.id);
            for (fi, flaw) in ce.flaws.iter().enumerate() {
                flaw_table.push(FlawRef {
                    engine_id: ce.design.id,
                    engine_name: ce.design.name.clone(),
                    activation_chance: flaw.activation_chance,
                    consequence: flaw.consequence.clone(),
                    description: flaw.description.clone(),
                    source,
                    flaw_index: fi,
                });
            }
        }

        // Track flaw discoveries to apply after the flight loop
        let mut flaw_discoveries: Vec<(EngineSource, usize, String)> = Vec::new();

        for (i, flight) in self.active_flights.iter_mut().enumerate() {
            if !matches!(flight.status, FlightStatus::InTransit) {
                continue;
            }

            if flight.leg_days_remaining > 0 {
                flight.leg_days_remaining -= 1;
            }

            if flight.leg_days_remaining == 0 {
                // Leg complete — consume propellant for this leg
                if let Some(leg) = flight.route.get(flight.current_leg) {
                    let dv_cost = leg.delta_v_cost;
                    let ambient = leg.ambient_pressure_pa;
                    let burn_result = flight.rocket.burn_sequential(&flight.design, dv_cost, ambient);

                    flight.current_location = leg.to.clone();
                    flight.rocket.location = leg.to.clone();

                    // Check overexpansion destruction for atmospheric legs.
                    // Only the first burned group is at sea level; upper groups
                    // fire at high altitude. Also skip groups already checked at launch.
                    if ambient > 0.0 {
                        let first_burned = burn_result.groups_burned.first().copied();
                        for &gi in &burn_result.groups_burned {
                            // Only the first burned group faces atmospheric pressure
                            if Some(gi) != first_burned {
                                continue;
                            }
                            if flight.flaw_rolled_groups.contains(&gi) {
                                continue; // already checked during launch sim
                            }
                            if let Some(group) = flight.design.stage_groups.get_mut(gi) {
                                for stage in group.iter_mut() {
                                    let risk = stage.engine.overexpansion_destruction_risk(ambient);
                                    if risk <= 0.0 { continue; }
                                    let mut engines_lost = 0u32;
                                    for _ in 0..stage.engine_count {
                                        if self.seed.contingent_rng.gen::<f64>() < risk {
                                            engines_lost += 1;
                                        }
                                    }
                                    if engines_lost > 0 {
                                        if engines_lost >= stage.engine_count {
                                            stage.engine_count = 0;
                                            stage.engine.thrust_n = 0.0;
                                            stage.engine.isp_s = 0.0;
                                            stage.propellant_mass_kg = 0.0;
                                        } else {
                                            stage.engine_count -= engines_lost;
                                        }
                                        let evt = GameEvent::MidFlightFlawActivated {
                                            rocket_name: flight.rocket_name.clone(),
                                            flaw_description: format!(
                                                "{} engine(s) destroyed by flow separation",
                                                engines_lost,
                                            ),
                                            consequence: "Engine destruction".to_string(),
                                        };
                                        events.push(evt);
                                    }
                                }
                            }
                        }
                    }

                    // Roll mid-flight flaws for groups that burned propellant
                    // (must happen before stranding check — stage was used even if burn fell short)
                    // Filter to groups not yet rolled for flaws
                    let new_burned: Vec<usize> = burn_result.groups_burned.iter()
                        .copied()
                        .filter(|gi| !flight.flaw_rolled_groups.contains(gi))
                        .collect();
                    if !new_burned.is_empty() {
                        for &gi in &new_burned {
                            flight.flaw_rolled_groups.insert(gi);
                        }
                        // Collect (group_index, stage_index, engine_id, engine_count) from newly-burned stages
                        let mut burned_stages: Vec<(usize, usize, EngineId, u32)> = Vec::new();
                        for &gi in &new_burned {
                            if let Some(group) = flight.design.stage_groups.get(gi) {
                                for (si, stage) in group.iter().enumerate() {
                                    burned_stages.push((gi, si, stage.engine.id, stage.engine_count));
                                }
                            }
                        }

                        // Roll flaws for each engine used in burned groups
                        for &(gi, si, engine_id, engine_count) in &burned_stages {
                            for flaw_ref in &flaw_table {
                                if flaw_ref.engine_id != engine_id {
                                    continue;
                                }
                                let effective_p = 1.0 - (1.0 - flaw_ref.activation_chance)
                                    .powi(engine_count as i32);
                                if self.seed.contingent_rng.gen::<f64>() < effective_p {
                                    flight.flaws_activated.push(crate::launch::FlawActivation {
                                        flaw_description: flaw_ref.description.clone(),
                                        consequence: flaw_ref.consequence.clone(),
                                        engine_name: flaw_ref.engine_name.clone(),
                                    });

                                    // Apply consequence to the stage that has the flaw
                                    crate::launch::apply_consequence_to_stage(
                                        &mut flight.design,
                                        &flaw_ref.consequence,
                                        gi,
                                        si,
                                    );

                                    let evt = GameEvent::MidFlightFlawActivated {
                                        rocket_name: flight.rocket_name.clone(),
                                        flaw_description: flaw_ref.description.clone(),
                                        consequence: flaw_ref.consequence.to_string(),
                                    };
                                    events.push(evt);

                                    flaw_discoveries.push((
                                        flaw_ref.source,
                                        flaw_ref.flaw_index,
                                        flaw_ref.engine_name.clone(),
                                    ));
                                }
                            }
                        }

                        // After flaw application, recheck remaining dv for stranding
                        let remaining_dv = flight.rocket.remaining_delta_v(&flight.design);
                        let remaining_route_dv: f64 = flight.route.iter()
                            .skip(flight.current_leg + 1)
                            .map(|leg| leg.delta_v_cost)
                            .sum();
                        if remaining_route_dv > 0.0 && remaining_dv < remaining_route_dv * 0.5 {
                            flight.status = FlightStatus::Stranded;
                            stranded_indices.push(i);
                            continue;
                        }
                    }

                    // Check if burn fell significantly short — strand the flight
                    if burn_result.dv_achieved < dv_cost * 0.95 {
                        flight.status = FlightStatus::Stranded;
                        stranded_indices.push(i);
                        continue;
                    }
                }

                // Advance to next leg
                flight.current_leg += 1;
                if flight.current_leg < flight.route.len() {
                    flight.leg_days_remaining = flight.route[flight.current_leg].total_days();
                } else {
                    // All legs complete
                    flight.status = FlightStatus::Arrived;
                    arrived_indices.push(i);
                }
            }
        }

        // Apply flaw discoveries to engine/rocket projects
        for (source, flaw_index, _engine_name) in &flaw_discoveries {
            match source {
                EngineSource::PlayerDesign(project_id) => {
                    if let Some(ep) = self.player_company.engine_projects.iter_mut()
                        .find(|ep| ep.project_id == *project_id)
                    {
                        if *flaw_index < ep.flaws.len() && !ep.flaws[*flaw_index].discovered {
                            ep.flaws[*flaw_index].discovered = true;
                            let evt = GameEvent::FlawDiscovered {
                                engine_name: ep.design.name.clone(),
                                flaw_description: ep.flaws[*flaw_index].description.clone(),
                            };
                            events.push(evt);
                        }
                    }
                }
                EngineSource::Contracted(ce_id) => {
                    if let Some(ce) = self.player_company.contracted_engines.iter_mut()
                        .find(|ce| ce.id == *ce_id)
                    {
                        if *flaw_index < ce.flaws.len() {
                            ce.flaws[*flaw_index].discovered = true;
                        }
                    }
                }
            }
        }

        // Resolve stranded and arrived flights (process in reverse to preserve indices)
        // Combine and sort in reverse order so we can remove safely
        let mut remove_indices: Vec<(usize, bool)> = Vec::new(); // (index, is_arrived)
        for &i in &stranded_indices {
            remove_indices.push((i, false));
        }
        for &i in &arrived_indices {
            remove_indices.push((i, true));
        }
        remove_indices.sort_by(|a, b| b.0.cmp(&a.0));

        for (i, is_arrived) in remove_indices {
            let flight = self.active_flights.remove(i);
            if is_arrived {
                let arrival_events = self.resolve_arrived_flight(flight);
                events.extend(arrival_events);
            } else {
                let location = crate::contract::destination_display_name(&flight.current_location);
                let evt = GameEvent::SpacecraftStranded {
                    rocket_name: flight.rocket_name.clone(),
                    location: location.to_string(),
                };
                events.push(evt);
            }
        }

        events
    }

    /// Resolve a flight that has arrived at its destination.
    fn resolve_arrived_flight(&mut self, flight: Flight) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let destination = flight.destination().to_string();
        let dest_display = crate::contract::destination_display_name(&destination);
        let total_payload_kg = flight.total_payload_kg();

        let evt = GameEvent::FlightArrived {
            rocket_name: flight.rocket_name.clone(),
            destination: dest_display.to_string(),
        };
        events.push(evt);

        // Determine outcome based on launch sim result (stored in flight)
        let is_partial = flight.launch_partial;

        if is_partial {
            self.player_company.reputation.on_launch_partial_failure();
        } else {
            self.player_company.reputation.on_launch_success();
        }

        // Process each payload
        let mut contract_id_for_record = None;
        for payload in &flight.payloads {
            match payload {
                Payload::ContractDelivery { contract_id, .. } => {
                    contract_id_for_record = Some(*contract_id);

                    // Find and remove the contract, pay the player
                    if let Some(ci) = self.player_company.active_contracts.iter()
                        .position(|c| c.id == *contract_id)
                    {
                        let contract = &self.player_company.active_contracts[ci];
                        let payment = if is_partial {
                            contract.payment * 0.5
                        } else {
                            contract.payment
                        };
                        let contract_name = contract.name.clone();
                        self.player_company.money += payment;
                        self.record_income(payment);
                        self.player_company.reputation.on_contract_launch();

                        let pay_evt = GameEvent::PaymentReceived {
                            amount: payment,
                            contract_name,
                        };
                        events.push(pay_evt);

                        self.player_company.active_contracts.remove(ci);
                    }
                }
                Payload::TestMass { .. } => {
                    // No payment for test launches
                }
            }
        }

        // Generate outcome event
        let outcome = if is_partial {
            let reason = flight.flaws_activated.first()
                .map(|f| f.flaw_description.clone())
                .unwrap_or_else(|| "degraded performance".to_string());
            let evt = GameEvent::LaunchPartialFailure {
                rocket_name: flight.rocket_name.clone(),
                reason: reason.clone(),
            };
            events.push(evt);
            LaunchOutcome::PartialFailure { reason }
        } else {
            let evt = GameEvent::LaunchSuccess {
                rocket_name: flight.rocket_name.clone(),
                destination: dest_display.to_string(),
            };
            events.push(evt);
            LaunchOutcome::Success
        };

        // Persist as spacecraft if requested
        let persist = flight.persist;
        let rocket_instance = flight.rocket;
        let design_clone = flight.design;
        let rocket_name = flight.rocket_name;
        let dest_for_spacecraft = destination.clone();

        let record = LaunchRecord {
            launch_date: flight.launch_date,
            rocket_name: rocket_name.clone(),
            contract_id: contract_id_for_record,
            destination,
            payload_kg: total_payload_kg,
            outcome,
            flaws_activated: flight.flaws_activated,
        };
        self.player_company.launch_history.push(record);

        if persist {
            let sc_id = SpacecraftId(self.next_rocket_id);
            self.next_rocket_id += 1;
            self.spacecraft.push(Spacecraft {
                id: sc_id,
                name: rocket_name,
                rocket: rocket_instance,
                design: design_clone,
                location: dest_for_spacecraft,
            });
        }

        events
    }

    /// Send a spacecraft on a new flight to a destination.
    pub fn fly_spacecraft(&mut self, spacecraft_index: usize, destination: &str) {
        if spacecraft_index >= self.spacecraft.len() {
            return;
        }
        let sc = self.spacecraft.remove(spacecraft_index);
        let rocket_mass = sc.design.total_mass_kg() + sc.rocket.payload_mass_kg;
        let first_group_thrust = sc.design.group_thrust_n(0);

        let path = crate::location::DELTA_V_MAP
            .shortest_path(&sc.location, destination, rocket_mass);
        let route = match path {
            Some((path, _)) => crate::flight::build_route(&path, rocket_mass, first_group_thrust),
            None => vec![],
        };

        let flight_id = FlightId(self.next_flight_id);
        self.next_flight_id += 1;

        let leg_days = route.first().map(|l| l.total_days()).unwrap_or(0);
        let dest_display = crate::contract::destination_display_name(destination);

        let flight = Flight {
            id: flight_id,
            rocket_name: sc.name.clone(),
            rocket_project_id: crate::rocket_project::RocketProjectId(0), // no project for spacecraft flights
            design: sc.design,
            rocket: sc.rocket,
            payloads: vec![],
            current_location: sc.location,
            route,
            current_leg: 0,
            leg_days_remaining: leg_days,
            status: FlightStatus::InTransit,
            flaws_activated: vec![],
            launch_date: self.date,
            persist: true, // spacecraft flights always persist
            launch_partial: false,
            flaw_rolled_groups: std::collections::HashSet::new(),
        };

        self.active_flights.push(flight);

        let evt = GameEvent::FlightDeparted {
            rocket_name: sc.name,
            destination: dest_display.to_string(),
        };
        self.event_log.push(self.date, evt);
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
        // Starting money minus one engineering team hiring cost ($150K)
        assert_eq!(gs.player_company.money, 200_000_000.0 - TEAM_HIRING_COST);
        assert_eq!(gs.speed, GameSpeed::Paused);
        assert_eq!(gs.elapsed_days(), 0);
        // Should have GameStarted event
        assert_eq!(gs.event_log.len(), 1);
        // Should start with 1 engineering team
        assert_eq!(gs.player_company.team_count(), 1);
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
        let recent = gs.event_log.recent(10);
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
        // Starts with 1 team (from Company::new)
        assert_eq!(gs.player_company.team_count(), 1);
        gs.player_company.hire_team("Alpha".into());
        assert_eq!(gs.player_company.team_count(), 2);
        // Starting money minus 2 hiring costs (initial team + Alpha)
        assert_eq!(gs.player_company.money, 1_000_000.0 - 2.0 * TEAM_HIRING_COST);
    }

    /// Build a 3-stage rocket design with two different engines.
    /// Stages 1 & 2 use engine_id=1, stage 3 uses engine_id=2.
    /// With 0 payload, stages 1+2 provide enough dv for LEO; stage 3 provides dv for LEO→GTO.
    fn make_three_stage_design() -> (RocketDesign, Vec<crate::engine_project::EngineProject>) {
        use crate::engine::{EngineDesign, EngineId, EngineCycle, PropellantFraction};
        use crate::propellant::Propellant;
        use crate::stage::{Stage, StageId};
        use crate::flaw::{Flaw, FlawId, FlawConsequence};
        use crate::engine_project::{EngineProject, EngineProjectId, EngineDesignStatus, PropellantPreset};

        let engine1 = EngineDesign {
            id: EngineId(101),
            name: "Lifter".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 2_000_000.0,
            isp_s: 300.0,
            exit_pressure_pa: 100_000.0,
            needs_atmosphere: false,
            mass_kg: 1500.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.6 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.4 },
            ],
        };

        let engine2 = EngineDesign {
            id: EngineId(102),
            name: "Upper".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 100_000.0,
            isp_s: 350.0,
            exit_pressure_pa: 100_000.0,
            needs_atmosphere: false,
            mass_kg: 200.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.6 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.4 },
            ],
        };

        let stage1 = Stage {
            id: StageId(1),
            name: "S1".into(),
            engine: engine1.clone(),
            engine_count: 3,
            propellant_mass_kg: 200_000.0,
            structural_mass_kg: 5000.0,
            fairing: None,
        };
        let stage2 = Stage {
            id: StageId(2),
            name: "S2".into(),
            engine: engine1.clone(),
            engine_count: 1,
            propellant_mass_kg: 30_000.0,
            structural_mass_kg: 1000.0,
            fairing: None,
        };
        // Stage 3 sized so that LEO→GTO (2440 m/s) + GTO→GEO (1500 m/s) = 3940 m/s
        // exceeds its dv, ensuring it gets exhausted and jettisoned mid-flight.
        // With 1000 kg prop, 300 dry, 200 engine = 500 dry, ve=3433: dv ≈ 3433*ln(1500/500) = 3773 m/s
        let stage3 = Stage {
            id: StageId(3),
            name: "S3".into(),
            engine: engine2.clone(),
            engine_count: 1,
            propellant_mass_kg: 1000.0,
            structural_mass_kg: 300.0,
            fairing: None,
        };

        let design = RocketDesign {
            id: crate::rocket::RocketDesignId(1),
            name: "TestThreeStage".into(),
            stage_groups: vec![
                vec![stage1],
                vec![stage2],
                vec![stage3],
            ],
        };

        // Engine projects with guaranteed flaws
        let flaw1 = Flaw {
            id: FlawId(1),
            description: "Lifter turbopump vibration".into(),
            consequence: FlawConsequence::PerformanceDegradation(0.01),
            activation_chance: 1.0,
            discovery_probability: 1.0,
            discovered: false,
        };
        let flaw2 = Flaw {
            id: FlawId(2),
            description: "Upper injector erosion".into(),
            consequence: FlawConsequence::PerformanceDegradation(0.01),
            activation_chance: 1.0,
            discovery_probability: 1.0,
            discovered: false,
        };

        let ep1 = EngineProject {
            project_id: EngineProjectId(1),
            design: engine1,
            preset: PropellantPreset::Kerolox,
            scale: 1.0,
            status: EngineDesignStatus::Testing {
                work_completed: 100.0,
            },
            flaws: vec![flaw1],
            revision: 0,
            teams_assigned: 0,
            complexity: 6,
            nre_cost: 0.0,
        };
        let ep2 = EngineProject {
            project_id: EngineProjectId(2),
            design: engine2,
            preset: PropellantPreset::Kerolox,
            scale: 1.0,
            status: EngineDesignStatus::Testing {
                work_completed: 100.0,
            },
            flaws: vec![flaw2],
            revision: 0,
            teams_assigned: 0,
            complexity: 6,
            nre_cost: 0.0,
        };

        (design, vec![ep1, ep2])
    }

    #[test]
    fn test_flaw_scoping_by_stage_usage() {
        use rand::SeedableRng;
        use crate::engine::EngineId;
        use crate::rocket_project::{RocketProject, RocketProjectId};

        let (design, engine_projects) = make_three_stage_design();

        // Verify stages 1+2 can reach LEO with 0 payload
        let dv_12 = {
            let two_stage = RocketDesign {
                id: design.id,
                name: design.name.clone(),
                stage_groups: vec![
                    design.stage_groups[0].clone(),
                    design.stage_groups[1].clone(),
                ],
            };
            two_stage.total_delta_v(0.0)
        };
        let total_dv = design.total_delta_v(0.0);
        assert!(dv_12 > 9400.0,
            "Stages 1+2 should provide enough dv for LEO, got {:.0}", dv_12);
        assert!(total_dv > dv_12 + 2000.0,
            "Stage 3 should add significant dv, got total {:.0} vs 1+2={:.0}", total_dv, dv_12);

        // --- Part 1: Launch to LEO, only stages 1+2 flaws should fire ---
        let rp = RocketProject::new(RocketProjectId(1), design.clone());
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        let sim = crate::launch::simulate_launch(
            &design, "leo", 0.0,
            &engine_projects, &rp, &[], &mut rng,
        );

        assert!(matches!(sim.outcome, crate::launch::LaunchOutcome::Success),
            "Launch to LEO should succeed, got {:?}", sim.outcome);
        // Only group 0 (stage 1) flaws should fire at launch.
        // Stage 2 (group 1) and stage 3 (group 2) flaws are deferred to mid-flight.
        assert_eq!(sim.flaws_activated.len(), 1,
            "Only group 0 flaw should fire at launch, got {:?}", sim.flaws_activated);
        assert_eq!(sim.flaws_activated[0].flaw_description, "Lifter turbopump vibration");
        assert_eq!(sim.flaw_rolled_groups.len(), 1);
        assert!(sim.flaw_rolled_groups.contains(&0));

        // --- Part 2: Create a spacecraft at LEO and fly to GTO ---
        let mut gs = GameState::new("Test".into(), 200_000_000.0, 42);
        gs.player_company.engine_projects = engine_projects;
        // Reset flaw discovery for the fly phase
        for ep in &mut gs.player_company.engine_projects {
            for flaw in &mut ep.flaws {
                flaw.discovered = false;
            }
        }

        // Instantiate the rocket from the degraded design (as launch_rocket would)
        let rocket = sim.degraded_design.instantiate(
            crate::rocket::RocketId(1), "leo", 0.0,
        );

        // Simulate that stages 1+2 are jettisoned (as they would be after LEO insertion)
        let mut rocket = rocket;
        for si in 0..rocket.stage_states[0].len() {
            rocket.jettison_stage(0, si);
        }
        for si in 0..rocket.stage_states[1].len() {
            rocket.jettison_stage(1, si);
        }

        // Verify we're on stage 3 (group index 2)
        let current_group = (0..sim.degraded_design.stage_groups.len())
            .find(|&gi| rocket.stage_states.get(gi)
                .map(|ss| ss.iter().any(|s| s.attached))
                .unwrap_or(false));
        assert_eq!(current_group, Some(2), "Should be on stage 3 (group index 2)");

        // Add as spacecraft
        let sc = Spacecraft {
            id: SpacecraftId(1),
            name: "TestCraft".into(),
            rocket,
            design: sim.degraded_design,
            location: "leo".into(),
        };
        gs.spacecraft.push(sc);

        // Fly spacecraft to GEO (LEO→GTO→GEO, 3940 m/s total, exceeds stage 3 dv)
        // Stage 3 will be exhausted and jettisoned mid-flight, triggering flaw roll.
        gs.fly_spacecraft(0, "geo");
        assert_eq!(gs.active_flights.len(), 1, "Should have one active flight");
        assert!(gs.spacecraft.is_empty(), "Spacecraft should be consumed");

        // Advance days until the flight completes (arrives or strands)
        for _ in 0..30 {
            gs.advance_day();
            if gs.active_flights.is_empty() {
                break;
            }
        }

        // Flight should have completed (stranded after stage exhaustion is OK)
        assert!(gs.active_flights.is_empty(),
            "Flight should have completed, still have {} active", gs.active_flights.len());

        // Check that the Upper engine flaw was discovered mid-flight
        let ep2 = gs.player_company.engine_projects.iter()
            .find(|ep| ep.design.id == EngineId(102))
            .expect("Should find engine project 102");
        assert!(ep2.flaws[0].discovered,
            "Upper engine flaw should be discovered after stage 3 jettison");

        // Check the event log for the mid-flight flaw activation
        let flaw_events: Vec<_> = gs.event_log.iter()
            .filter(|(_, e)| matches!(e, GameEvent::MidFlightFlawActivated { .. }))
            .collect();
        assert_eq!(flaw_events.len(), 1,
            "Should have exactly one mid-flight flaw event, got {}", flaw_events.len());
    }

    #[test]
    fn test_spacecraft_has_remaining_dv_after_leo_launch() {
        use crate::rocket_project::{RocketProject, RocketProjectId};

        let (design, engine_projects) = make_three_stage_design();

        let mut gs = GameState::new("Test".into(), 200_000_000.0, 42);
        gs.player_company.engine_projects = engine_projects;

        // Simulate launch to get degraded design
        let rp = RocketProject::new(RocketProjectId(1), design.clone());
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(99);
        let sim = crate::launch::simulate_launch(
            &design, "leo", 0.0,
            &gs.player_company.engine_projects, &rp, &[], &mut rng,
        );

        // Build route and instantiate rocket
        let rocket_mass = sim.degraded_design.total_mass_kg();
        let thrust = sim.degraded_design.group_thrust_n(0);
        let path = crate::location::DELTA_V_MAP
            .shortest_path("earth_surface", "leo", rocket_mass);
        let route = match path {
            Some((p, _)) => crate::flight::build_route(&p, rocket_mass, thrust),
            None => vec![],
        };
        let rocket = sim.degraded_design.instantiate(
            crate::rocket::RocketId(1), "earth_surface", 0.0,
        );
        let leg_days = route.first().map(|l| l.total_days()).unwrap_or(0);

        let flight = crate::flight::Flight {
            id: crate::flight::FlightId(1),
            rocket_name: "TestRocket".into(),
            rocket_project_id: RocketProjectId(1),
            design: sim.degraded_design,
            rocket,
            payloads: vec![],
            current_location: "earth_surface".into(),
            route,
            current_leg: 0,
            leg_days_remaining: leg_days,
            status: crate::flight::FlightStatus::InTransit,
            flaws_activated: sim.flaws_activated,
            launch_date: gs.date,
            persist: true,
            launch_partial: false,
            flaw_rolled_groups: sim.flaw_rolled_groups,
        };

        gs.active_flights.push(flight);

        // Advance days until flight arrives
        for _ in 0..10 {
            gs.advance_day();
            if gs.active_flights.is_empty() { break; }
        }

        assert!(gs.active_flights.is_empty(), "Flight should have arrived");
        assert_eq!(gs.spacecraft.len(), 1, "Should have a spacecraft");

        let sc = &gs.spacecraft[0];
        let remaining = sc.remaining_delta_v();
        assert!(remaining > 1000.0,
            "Spacecraft should have significant remaining dv, got {:.0}", remaining);
    }

    #[test]
    fn test_salary_deduction() {
        let mut gs = GameState::new("Test".into(), 1_000_000.0, 1);
        gs.player_company.hire_team("Alpha".into());
        // Now has 2 teams (1 initial + Alpha), paid 2 hiring costs

        // Advance to Feb 1 (31 days)
        for _ in 0..31 {
            gs.advance_day();
        }
        // Should have paid 2 hiring costs + 2 team salaries for 1 month
        let expected = 1_000_000.0 - 2.0 * TEAM_HIRING_COST - 2.0 * ENGINEERING_MONTHLY_SALARY;
        assert!((gs.player_company.money - expected).abs() < 0.01);
    }

    #[test]
    fn test_negative_money_allowed() {
        let mut gs = GameState::new("Test".into(), 100_000.0, 1);
        // Starts with 1 team (hiring cost $150K), money = 100K - 150K = -50K
        assert!(gs.player_company.money < 0.0);
        gs.player_company.hire_team("Alpha".into()); // another -150K
        assert!(gs.player_company.money < -150_000.0);
        // Should still work, just go negative
        for _ in 0..31 {
            gs.advance_day();
        }
        // Should have deducted 2 salaries on top
        assert!(gs.player_company.money < -200_000.0);
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
        // Starts with 1 team, hire another
        gs.player_company.hire_team("Alpha".into());
        gs.player_company.start_engine_project(
            "Kestrel".into(),
            crate::engine::EngineCycle::GasGenerator,
            crate::engine_project::PropellantPreset::Kerolox,
            1.0,
            true,
        );

        assert_eq!(gs.player_company.unassigned_team_count(), 2);
        assert!(gs.player_company.add_team_to_project(0));
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
