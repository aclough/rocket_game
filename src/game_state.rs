use std::collections::{HashMap, VecDeque};

use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::contract::{self, Contract};
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
            engine_cost_history: HashMap::new(),
            contracted_engine_build_counts: HashMap::new(),
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
        technology_id: Option<crate::technology::TechnologyId>,
    ) -> Option<GameEvent> {
        let project_id = EngineProjectId(self.next_project_id);
        let engine_id = EngineId(self.next_engine_id);
        self.next_project_id += 1;
        self.next_engine_id += 1;

        let mut project = EngineProject::new(
            project_id, engine_id, name.clone(),
            cycle, preset, scale, use_vacuum_isp,
        )?;
        project.technology_id = technology_id;
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
                                    ep.revision,
                                    ep.flaws.clone(),
                                    ep.improvements.iter().filter(|i| i.actualized).cloned().collect(),
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
                                    revision: 0,
                                    flaws: ce.flaws.clone(),
                                    improvements: Vec::new(),
                                });
                                *self.contracted_engine_build_counts.entry(ce_id).or_insert(0) += 1;
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
            rp.revision,
            rp.flaws.clone(),
        );
        total_cost += integration_order.material_cost;
        self.manufacturing.orders.push(integration_order);

        // Increment rocket build count
        *self.rocket_build_counts.entry(design_id).or_insert(0) += 1;

        // Note: rocket_cost_history is populated at integration completion
        // (see advance_day) so the recorded marginal cost includes labor
        // accrued during manufacturing, not just material cost.

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
        let revision = ep.revision;
        let flaws = ep.flaws.clone();
        let improvements: Vec<_> = ep.improvements.iter().filter(|i| i.actualized).cloned().collect();
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
            revision,
            flaws,
            improvements,
        );
        let cost = order.material_cost;
        self.manufacturing.orders.push(order);
        *self.engine_build_counts.entry(ep_id).or_insert(0) += 1;
        // engine_cost_history is populated at engine-build completion so the
        // recorded cost includes labor in addition to materials.
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
                                    // Consume engines from inventory, rolling
                                    // their full build_cost (material + labor)
                                    // into this stage order's material_cost.
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
}

fn default_next_contract_id() -> u64 { 1 }
fn default_next_flight_id() -> u64 { 1 }
fn default_next_rocket_id() -> u64 { 1 }
fn default_markets() -> Vec<contract::Market> {
    let mut markets = contract::initial_markets();
    markets.extend(contract::event_market_templates());
    markets
}

impl GameState {
    pub fn new(company_name: String, starting_money: f64, seed_value: u64) -> Self {
        let start = GameDate::default_start();
        let mut event_log = EventLog::new(EVENT_LOG_SIZE);
        event_log.push(start, GameEvent::GameStarted);
        let seed = GameSeed::new(seed_value);

        let economy = crate::economy::initial_state(&seed, start);
        let technologies = crate::technology::generate_technologies(&seed);

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
            economy,
            markets: default_markets(),
            fired_market_events: Vec::new(),
            technologies,
        }
    }

    /// Advance the game by one day. Returns events generated this tick.
    pub fn advance_day(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();

        self.date = self.date.next_day();

        // Process daily work on engine and rocket projects
        let mut newly_designed_engines: Vec<usize> = Vec::new();
        // (engine_project_index, deficiency_id)
        let mut tech_def_attempts: Vec<(usize, crate::technology::TechDeficiencyId)> = Vec::new();
        {
            let rng = &mut self.seed.contingent_rng;
            let next_flaw_id = &mut self.player_company.next_flaw_id;

            for (pi, project) in self.player_company.engine_projects.iter_mut().enumerate() {
                let engine_name = project.design.name.clone();
                let work_events = project.apply_daily_work(rng, next_flaw_id);
                for we in work_events {
                    let evt = match we {
                        WorkEvent::DesignComplete { flaw_count } => {
                            newly_designed_engines.push(pi);
                            GameEvent::EngineDesignComplete { engine_name: engine_name.clone(), flaw_count }
                        }
                        WorkEvent::TestingCycleComplete => continue,
                        WorkEvent::FlawDiscovered { flaw_description } =>
                            GameEvent::FlawDiscovered { engine_name: engine_name.clone(), flaw_description },
                        WorkEvent::RevisionComplete =>
                            GameEvent::RevisionComplete { engine_name: engine_name.clone() },
                        WorkEvent::ImprovementDiscovered { description } =>
                            GameEvent::ImprovementDiscovered { engine_name: engine_name.clone(), description },
                        WorkEvent::ImprovementActualized { description } =>
                            GameEvent::ImprovementActualized { engine_name: engine_name.clone(), description },
                        WorkEvent::TechDeficiencyAttempted { deficiency_id } => {
                            tech_def_attempts.push((pi, deficiency_id));
                            continue;
                        }
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

        // Process tech deficiency revision attempts
        for (pi, def_id) in tech_def_attempts {
            let project = &mut self.player_company.engine_projects[pi];
            let tech_id = match project.technology_id {
                Some(id) => id,
                None => continue,
            };
            if let Some(tech) = self.technologies.iter_mut().find(|t| t.id == tech_id) {
                if let Some(def) = tech.deficiencies.iter_mut().find(|d| d.id == def_id) {
                    let already_solved = def.solved;
                    let engine_name = project.design.name.clone();
                    let def_desc = format!("{}: {}", def.description, def.kind);

                    if crate::technology::attempt_solve(def, already_solved, &mut self.seed.contingent_rng) {
                        // Success — remove from engine and restore stats
                        project.tech_deficiency_ids.retain(|id| *id != def_id);
                        match &def.kind {
                            crate::technology::TechDeficiencyKind::IspPenalty(frac) => {
                                project.design.isp_s /= 1.0 - frac;
                            }
                            crate::technology::TechDeficiencyKind::MassPenalty(frac) => {
                                project.design.mass_kg /= 1.0 + frac;
                            }
                            crate::technology::TechDeficiencyKind::ThrustPenalty(frac) => {
                                project.design.thrust_n /= 1.0 - frac;
                            }
                            crate::technology::TechDeficiencyKind::ComplexityPenalty(n) => {
                                project.complexity = project.complexity.saturating_sub(*n);
                            }
                        }
                        let evt = GameEvent::RevisionComplete { engine_name: engine_name.clone() };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    } else {
                        // Failed — report attempt count
                        let hint = crate::technology::failure_hint(def.total_attempts);
                        let msg = if let Some(h) = hint {
                            format!("Failed to resolve {}: {}. {}", engine_name, def_desc, h)
                        } else {
                            format!("Failed to resolve {} deficiency: {}", engine_name, def_desc)
                        };
                        let evt = GameEvent::FlawDiscovered {
                            engine_name,
                            flaw_description: msg,
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    }
                }
            }
        }

        // Apply tech deficiencies to newly completed engine designs
        for pi in newly_designed_engines {
            let project = &mut self.player_company.engine_projects[pi];
            if let Some(tech_id) = project.technology_id {
                if let Some(tech) = self.technologies.iter().find(|t| t.id == tech_id) {
                    let deficiency_ids: Vec<crate::technology::TechDeficiencyId> =
                        tech.deficiencies.iter().map(|d| d.id).collect();
                    // Apply stat penalties from unsolved deficiencies
                    for def in &tech.deficiencies {
                        match &def.kind {
                            crate::technology::TechDeficiencyKind::IspPenalty(frac) => {
                                project.design.isp_s *= 1.0 - frac;
                            }
                            crate::technology::TechDeficiencyKind::MassPenalty(frac) => {
                                project.design.mass_kg *= 1.0 + frac;
                            }
                            crate::technology::TechDeficiencyKind::ThrustPenalty(frac) => {
                                project.design.thrust_n *= 1.0 - frac;
                            }
                            crate::technology::TechDeficiencyKind::ComplexityPenalty(n) => {
                                project.complexity += n;
                            }
                        }
                    }
                    project.tech_deficiency_ids = deficiency_ids;
                    let engine_name = project.design.name.clone();
                    let tech_name = tech.name.clone();
                    let desc: Vec<String> = tech.deficiencies.iter()
                        .map(|d| format!("{}: {}", d.description, d.kind))
                        .collect();
                    if !desc.is_empty() {
                        let evt = GameEvent::TechDeficienciesFound {
                            engine_name: engine_name.clone(),
                            tech_name: tech_name.clone(),
                            deficiencies: desc.join(", "),
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    }
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

            // Advance economy — check if current state has expired
            let prev_condition = self.economy.condition;
            if let Some(new_condition) = crate::economy::advance_economy(
                &mut self.economy, &self.seed, self.date,
            ) {
                // Only fire event if the condition actually changed
                if new_condition != prev_condition {
                    let evt = GameEvent::EconomicShift {
                        condition: new_condition.display_name().to_string(),
                        description: new_condition.flavor_text().to_string(),
                    };
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                    self.speed = GameSpeed::Paused;
                }
            }

            // Expire market modifiers
            for market in &mut self.markets {
                market.expire_modifiers(self.date);
            }

            // Check seed-driven market events
            let market_events = self.check_market_events();
            for evt in market_events {
                self.event_log.push(self.date, evt.clone());
                events.push(evt);
                self.speed = GameSpeed::Paused;
            }

            // Check yearly tech unlock rolls (on January)
            if self.date.month == 1 {
                self.check_tech_unlocks(&mut events);
            }

            // Generate monthly contracts from all active markets
            let rep = self.player_company.reputation.total();
            let query = format!("contracts_{}_{}", self.date.year, self.date.month);
            let mut rng = self.seed.world_query(&query);
            let econ_mod = self.economy.modifier;
            let mut generated = 0u32;
            for market in &self.markets {
                let cs = contract::generate_market_contracts(
                    market, &mut rng, &mut self.next_contract_id,
                    self.date, rep, econ_mod,
                );
                generated += cs.len() as u32;
                self.available_contracts.extend(cs);
            }
            if generated > 0 {
                // Sort by market ID so display order matches selection order
                self.available_contracts.sort_by_key(|c| c.market_id.0);
                let evt = GameEvent::ContractsRefreshed { count: generated };
                self.event_log.push(self.date, evt.clone());
                events.push(evt);
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
                crate::manufacturing::ManufacturingEvent::EngineBuilt {
                    engine_name, source, build_cost, ..
                } => {
                    // Only player-designed engines have a per-project history.
                    if let EngineSource::PlayerDesign(ep_id) = source {
                        self.player_company.engine_cost_history
                            .entry(ep_id)
                            .or_default()
                            .push(build_cost);
                    }
                    GameEvent::EngineBuilt { engine_name }
                }
                crate::manufacturing::ManufacturingEvent::StageBuilt { stage_name, .. } =>
                    GameEvent::StageBuilt { stage_name },
                crate::manufacturing::ManufacturingEvent::RocketIntegrated {
                    rocket_name, design_id, build_cost, ..
                } => {
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

        // Roll endurance flaws for parked spacecraft
        {
            use rand::Rng;
            use crate::flaw::FlawTrigger;
            // Snapshot PerDay flaws from rocket projects
            struct ScFlawRef {
                project_id: RocketProjectId,
                flaw_index: usize,
                daily_rate: f64,
                consequence: crate::flaw::FlawConsequence,
                description: String,
            }
            let mut sc_flaw_table: Vec<ScFlawRef> = Vec::new();
            for rp in &self.player_company.rocket_projects {
                for (fi, flaw) in rp.flaws.iter().enumerate() {
                    if flaw.trigger == FlawTrigger::PerDay {
                        sc_flaw_table.push(ScFlawRef {
                            project_id: rp.project_id,
                            flaw_index: fi,
                            daily_rate: flaw.daily_rate(),
                            consequence: flaw.consequence.clone(),
                            description: flaw.description.clone(),
                        });
                    }
                }
            }
            let mut sc_flaw_discoveries: Vec<(RocketProjectId, usize)> = Vec::new();
            for sc in &mut self.spacecraft {
                for rf in &sc_flaw_table {
                    if rf.project_id != sc.rocket_project_id {
                        continue;
                    }
                    if self.seed.contingent_rng.gen::<f64>() < rf.daily_rate {
                        // Pick a random attached stage
                        let attached: Vec<(usize, usize)> = sc.design.stage_groups.iter()
                            .enumerate()
                            .flat_map(|(gi, group)| {
                                let stage_states = &sc.rocket.stage_states;
                                group.iter().enumerate()
                                    .filter(move |(si, _)| {
                                        stage_states.get(gi)
                                            .and_then(|g| g.get(*si))
                                            .map_or(false, |ss| ss.attached)
                                    })
                                    .map(move |(si, _)| (gi, si))
                            })
                            .collect();
                        if attached.is_empty() { continue; }
                        let (gi, si) = attached[self.seed.contingent_rng.gen_range(0..attached.len())];
                        crate::launch::apply_consequence_to_stage(
                            &mut sc.design, &rf.consequence, gi, si,
                        );
                        let evt = GameEvent::MidFlightFlawActivated {
                            rocket_name: sc.name.clone(),
                            flaw_description: rf.description.clone(),
                            consequence: rf.consequence.to_string(),
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                        sc_flaw_discoveries.push((rf.project_id, rf.flaw_index));
                    }
                }
            }
            // Discover activated flaws on rocket projects
            for (project_id, flaw_index) in &sc_flaw_discoveries {
                if let Some(rp) = self.player_company.rocket_projects.iter_mut()
                    .find(|rp| rp.project_id == *project_id)
                {
                    if *flaw_index < rp.flaws.len() && !rp.flaws[*flaw_index].discovered {
                        rp.flaws[*flaw_index].discovered = true;
                    }
                }
            }
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

    /// Launch a rocket carrying a manifest of payloads.
    /// `rocket_item_id` identifies the InventoryRocket to use as the carrier.
    /// `payloads` is the full manifest — any combination of contract
    /// deliveries, test masses, and nested Spacecraft. The caller is
    /// responsible for already having taken any nested-rocket inventory
    /// items out of inventory and packed them into Spacecraft payloads.
    /// Returns events; on catastrophic failure, also a LaunchRecord. On
    /// success/partial success, the rocket enters transit and resolves on
    /// arrival.
    pub fn launch_rocket(
        &mut self,
        rocket_item_id: crate::manufacturing::InventoryItemId,
        destination: &str,
        payloads: Vec<Payload>,
        persist: bool,
    ) -> Option<(Vec<GameEvent>, Option<LaunchRecord>)> {
        let total_payload_kg: f64 = payloads.iter().map(|p| p.mass_kg()).sum();

        // Take the rocket from inventory
        let inv_rocket = self.player_company.manufacturing.inventory.take_rocket(rocket_item_id)?;

        // Find the rocket project for this rocket
        let rp = self.player_company.rocket_projects.iter()
            .find(|rp| rp.project_id == inv_rocket.rocket_project_id)?;

        // Use snapshotted rocket flaws from the inventory item
        let rocket_flaws = &inv_rocket.rocket_flaws;

        // Simulate flaw activation at launch
        let sim = launch::simulate_launch(
            &rp.design,
            destination,
            total_payload_kg,
            &self.player_company.engine_projects,
            rocket_flaws,
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

        // Catastrophic failure at launch — resolve immediately. The carrier
        // and all nested Spacecraft payloads are destroyed (the `payloads`
        // Vec is dropped here — by user spec, nothing returns to inventory).
        // All on-manifest contracts are forfeited.
        if matches!(sim.outcome, LaunchOutcome::Failure { .. }) {
            let mut contract_id_for_record: Option<crate::contract::ContractId> = None;
            let manifest_contract_ids: Vec<crate::contract::ContractId> = payloads.iter()
                .filter_map(|p| match p {
                    Payload::ContractDelivery { contract_id, .. } => Some(*contract_id),
                    _ => None,
                })
                .collect();
            if let Some(first) = manifest_contract_ids.first() {
                contract_id_for_record = Some(*first);
            }

            self.player_company.reputation.on_launch_failure();

            for cid in &manifest_contract_ids {
                if let Some(ci) = self.player_company.active_contracts.iter()
                    .position(|c| c.id == *cid)
                {
                    self.player_company.active_contracts.remove(ci);
                }
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
                contract_id: contract_id_for_record,
                destination: destination.to_string(),
                payload_kg: total_payload_kg,
                outcome: sim.outcome,
                flaws_activated: sim.flaws_activated,
            };
            self.player_company.launch_history.push(record.clone());
            self.speed = GameSpeed::Paused;
            return Some((events, Some(record)));
        }

        // Success or partial failure — create a flight in transit
        let rocket_mass = sim.degraded_design.total_mass_kg() + total_payload_kg;
        let first_group_thrust = sim.degraded_design.group_thrust_n(0);

        let path = crate::location::DELTA_V_MAP
            .shortest_path_for_rocket(
                "earth_surface", destination, &sim.degraded_design, total_payload_kg,
            );
        let route = match path {
            Some((path, _)) => crate::flight::build_route(&path, rocket_mass, first_group_thrust, false),
            None => vec![],
        };

        let flight_id = FlightId(self.next_flight_id);
        self.next_flight_id += 1;

        // Instantiate a Rocket with per-stage propellant tracking
        let rocket_instance_id = RocketId(self.next_rocket_id);
        self.next_rocket_id += 1;
        let rocket_instance = sim.degraded_design.instantiate(
            rocket_instance_id, "earth_surface", total_payload_kg,
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

    /// Check yearly tech unlock rolls.
    fn check_tech_unlocks(&mut self, events: &mut Vec<GameEvent>) {
        use rand::Rng;
        for tech in &mut self.technologies {
            if tech.unlocked {
                continue;
            }
            let query = format!("tech_unlock_{}_{}", tech.id.0, self.date.year);
            let mut rng = self.seed.world_query(&query);
            let chance = match tech.difficulty {
                0 => 0.0,
                1 => 0.10,
                _ => 0.08,
            };
            if rng.gen::<f64>() < chance {
                tech.unlocked = true;
                let evt = GameEvent::EconomicShift {
                    condition: format!("Technology Available: {}", tech.name),
                    description: tech.description.clone(),
                };
                self.event_log.push(self.date, evt.clone());
                events.push(evt);
                self.speed = GameSpeed::Paused;
            }
        }
    }

    /// Check seed-driven market events and activate/modify markets.
    fn check_market_events(&mut self) -> Vec<GameEvent> {
        use rand::Rng;
        let mut events = Vec::new();

        struct MarketEvent {
            key: &'static str,
            market_id: contract::MarketId,
            probability: f64,
            year_range: (u32, u32),
            flavor: &'static str,
            /// If Some, add a modifier to this other market when this event fires.
            cross_effect: Option<(contract::MarketId, contract::MarketModifier)>,
        }

        // Define potential market events
        let potential_events = [
            MarketEvent {
                key: "market_cots",
                market_id: contract::MARKET_COTS,
                probability: 0.70,
                year_range: (2004, 2008),
                flavor: "NASA announces Commercial Orbital Transportation Services program",
                cross_effect: None,
            },
            MarketEvent {
                key: "market_leo_constellation",
                market_id: contract::MARKET_LEO_CONSTELLATION,
                probability: 0.60,
                year_range: (2008, 2015),
                flavor: "Major LEO broadband constellation announced — GEO market share declining",
                cross_effect: Some((contract::MARKET_GEO_COMSATS, contract::MarketModifier {
                    id: "constellation_competition".into(),
                    description: "LEO constellations taking market share".into(),
                    volume_mult: 0.6,
                    rate_mult: 0.9,
                    end_date: None,
                })),
            },
            MarketEvent {
                key: "market_meo_constellation",
                market_id: contract::MARKET_MEO_CONSTELLATION,
                probability: 0.30,
                year_range: (2008, 2015),
                flavor: "MEO navigation constellation contracts opening up — GEO demand softening",
                cross_effect: Some((contract::MARKET_GEO_COMSATS, contract::MarketModifier {
                    id: "constellation_competition".into(),
                    description: "MEO constellations taking market share".into(),
                    volume_mult: 0.7,
                    rate_mult: 0.95,
                    end_date: None,
                })),
            },
            MarketEvent {
                key: "market_nssl",
                market_id: contract::MARKET_NSSL,
                probability: 0.50,
                year_range: (2010, 2018),
                flavor: "National security space launch program opens to new providers",
                cross_effect: None,
            },
            MarketEvent {
                key: "market_earth_obs",
                market_id: contract::MARKET_EARTH_OBS,
                probability: 0.70,
                year_range: (2005, 2012),
                flavor: "Commercial Earth observation market taking off",
                cross_effect: None,
            },
        ];

        // LEO and MEO constellations are mutually exclusive
        let leo_key = "market_leo_constellation";
        let meo_key = "market_meo_constellation";

        for pe in &potential_events {
            if self.fired_market_events.contains(&pe.key.to_string()) {
                continue;
            }

            // Check mutual exclusivity: skip MEO if LEO already fired and vice versa
            if pe.key == meo_key && self.fired_market_events.contains(&leo_key.to_string()) {
                continue;
            }
            if pe.key == leo_key && self.fired_market_events.contains(&meo_key.to_string()) {
                continue;
            }

            let mut rng = self.seed.world_query(pe.key);
            let triggers = rng.gen::<f64>() < pe.probability;
            let trigger_year = rng.gen_range(pe.year_range.0..=pe.year_range.1);

            if !triggers {
                // Mark as fired so we don't re-check
                self.fired_market_events.push(pe.key.to_string());
                continue;
            }

            if self.date.year >= trigger_year {
                // Activate the market
                if let Some(market) = self.markets.iter_mut().find(|m| m.id == pe.market_id) {
                    market.active = true;
                }
                self.fired_market_events.push(pe.key.to_string());

                // Apply cross-effects
                if let Some((target_id, modifier)) = &pe.cross_effect {
                    if let Some(target) = self.markets.iter_mut().find(|m| m.id == *target_id) {
                        target.add_modifier(modifier.clone());
                    }
                }

                let market_name = self.markets.iter()
                    .find(|m| m.id == pe.market_id)
                    .map(|m| m.name.clone())
                    .unwrap_or_default();

                events.push(GameEvent::EconomicShift {
                    condition: format!("New Market: {}", market_name),
                    description: pe.flavor.to_string(),
                });
            }
        }

        events
    }

    /// Process daily flight advancement. Returns events generated.
    fn advance_flights(&mut self) -> Vec<GameEvent> {
        use rand::Rng;
        use crate::engine::EngineId;
        use crate::flaw::{FlawConsequence, FlawTrigger};
        use crate::engine_project::EngineSource;
        use crate::rocket_project::RocketProjectId;

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

        // Snapshot rocket project PerDay flaws for endurance checking.
        struct RocketFlawRef {
            project_id: RocketProjectId,
            flaw_index: usize,
            daily_rate: f64,
            consequence: FlawConsequence,
            description: String,
        }
        let mut rocket_flaw_table: Vec<RocketFlawRef> = Vec::new();
        for rp in &self.player_company.rocket_projects {
            for (fi, flaw) in rp.flaws.iter().enumerate() {
                if flaw.trigger == FlawTrigger::PerDay {
                    rocket_flaw_table.push(RocketFlawRef {
                        project_id: rp.project_id,
                        flaw_index: fi,
                        daily_rate: flaw.daily_rate(),
                        consequence: flaw.consequence.clone(),
                        description: flaw.description.clone(),
                    });
                }
            }
        }

        // Track flaw discoveries to apply after the flight loop
        let mut flaw_discoveries: Vec<(EngineSource, usize, String)> = Vec::new();
        // Track rocket project flaw discoveries (project_id, flaw_index)
        let mut rocket_flaw_discoveries: Vec<(RocketProjectId, usize)> = Vec::new();

        for (i, flight) in self.active_flights.iter_mut().enumerate() {
            if !matches!(flight.status, FlightStatus::InTransit) {
                continue;
            }

            if flight.leg_days_remaining > 0 {
                flight.leg_days_remaining -= 1;
            }

            // Roll endurance (PerDay) flaws for this flight's rocket project
            for rf in &rocket_flaw_table {
                if rf.project_id != flight.rocket_project_id {
                    continue;
                }
                if self.seed.contingent_rng.gen::<f64>() < rf.daily_rate {
                    // Pick a random attached stage group and stage
                    let attached: Vec<(usize, usize)> = flight.design.stage_groups.iter()
                        .enumerate()
                        .flat_map(|(gi, group)| {
                            let stage_states = &flight.rocket.stage_states;
                            group.iter().enumerate()
                                .filter(move |(si, _)| {
                                    stage_states.get(gi)
                                        .and_then(|g| g.get(*si))
                                        .map_or(false, |ss| ss.attached)
                                })
                                .map(move |(si, _)| (gi, si))
                        })
                        .collect();
                    if attached.is_empty() {
                        continue;
                    }
                    let (gi, si) = attached[self.seed.contingent_rng.gen_range(0..attached.len())];

                    crate::launch::apply_consequence_to_stage(
                        &mut flight.design,
                        &rf.consequence,
                        gi, si,
                    );

                    let evt = GameEvent::MidFlightFlawActivated {
                        rocket_name: flight.rocket_name.clone(),
                        flaw_description: rf.description.clone(),
                        consequence: rf.consequence.to_string(),
                    };
                    events.push(evt);

                    rocket_flaw_discoveries.push((rf.project_id, rf.flaw_index));
                }
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

        // Apply rocket project endurance flaw discoveries
        for (project_id, flaw_index) in &rocket_flaw_discoveries {
            if let Some(rp) = self.player_company.rocket_projects.iter_mut()
                .find(|rp| rp.project_id == *project_id)
            {
                if *flaw_index < rp.flaws.len() && !rp.flaws[*flaw_index].discovered {
                    rp.flaws[*flaw_index].discovered = true;
                    let evt = GameEvent::FlawDiscovered {
                        engine_name: rp.design.name.clone(),
                        flaw_description: rp.flaws[*flaw_index].description.clone(),
                    };
                    events.push(evt);
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

        // Process each payload. Spacecraft payloads marked for this
        // destination are detached and pushed into the fleet; others
        // (contracts/test masses) are completed/discarded as before.
        let mut contract_id_for_record = None;
        let mut deployed_spacecraft: Vec<Payload> = Vec::new();
        let mut remaining_payloads: Vec<Payload> = Vec::new();
        for payload in flight.payloads {
            match payload {
                Payload::ContractDelivery { contract_id, .. } => {
                    contract_id_for_record = Some(contract_id);

                    if let Some(ci) = self.player_company.active_contracts.iter()
                        .position(|c| c.id == contract_id)
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
                    // No payment for test launches.
                }
                Payload::Spacecraft { deploy_at: Some(ref d), .. } if *d == destination => {
                    deployed_spacecraft.push(payload);
                }
                other => {
                    // Spacecraft payload bound for some other waypoint —
                    // not implemented yet (Phase 2). For now keep it on the
                    // arriving rocket as if the carrier were continuing.
                    remaining_payloads.push(other);
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
            destination: destination.clone(),
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
                rocket_project_id: flight.rocket_project_id,
                payloads: remaining_payloads,
            });
        }

        // Detach Spacecraft payloads at this destination into the fleet.
        for payload in deployed_spacecraft {
            if let Payload::Spacecraft {
                design, rocket, nested_payloads, rocket_project_id, name, ..
            } = payload {
                let sc_id = SpacecraftId(self.next_rocket_id);
                self.next_rocket_id += 1;
                let evt = GameEvent::SpacecraftDeployed {
                    spacecraft_name: name.clone(),
                    location: dest_display.to_string(),
                };
                events.push(evt);
                self.spacecraft.push(Spacecraft {
                    id: sc_id,
                    name,
                    rocket,
                    design,
                    location: destination.clone(),
                    rocket_project_id,
                    payloads: nested_payloads,
                });
            }
        }

        events
    }

    /// Send a spacecraft on a new flight to a destination. Any payloads
    /// the spacecraft is still carrying ride along; those whose `deploy_at`
    /// matches the destination will be detached on arrival (via the regular
    /// arrival path).
    pub fn fly_spacecraft(&mut self, spacecraft_index: usize, destination: &str) {
        if spacecraft_index >= self.spacecraft.len() {
            return;
        }
        let mut sc = self.spacecraft.remove(spacecraft_index);
        // Recompute payload mass from current carried payloads (live value
        // may differ from rocket.payload_mass_kg if payloads were detached
        // earlier). Sync the rocket's cached payload mass too so dv math
        // stays correct.
        let payload_mass: f64 = sc.payloads.iter().map(|p| p.mass_kg()).sum();
        sc.rocket.payload_mass_kg = payload_mass;

        let rocket_mass = sc.design.total_mass_kg() + payload_mass;
        let first_group_thrust = sc.design.group_thrust_n(0);
        let low_thrust = sc.rocket.is_current_stage_low_thrust(&sc.design);

        let path = crate::location::DELTA_V_MAP
            .shortest_path_for_rocket(
                &sc.location, destination, &sc.design, payload_mass,
            );
        let route = match path {
            Some((path, _)) => crate::flight::build_route(&path, rocket_mass, first_group_thrust, low_thrust),
            None => {
                // No valid path — put the spacecraft back and abort
                self.spacecraft.insert(spacecraft_index, sc);
                return;
            }
        };
        if route.is_empty() {
            self.spacecraft.insert(spacecraft_index, sc);
            return;
        }

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
            payloads: sc.payloads,
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

    /// Dock spacecraft `small_idx` onto `large_idx`. Both must be at the
    /// same location and refer to different spacecraft. The smaller is
    /// removed from `game.spacecraft` and re-wrapped as a
    /// `Payload::Spacecraft` (with `deploy_at = None`, meaning manual
    /// undock only) on the larger. Returns true on success.
    pub fn dock_spacecraft(&mut self, small_idx: usize, large_idx: usize) -> bool {
        if small_idx == large_idx { return false; }
        let n = self.spacecraft.len();
        if small_idx >= n || large_idx >= n { return false; }
        if self.spacecraft[small_idx].location != self.spacecraft[large_idx].location {
            return false;
        }
        // Remove the smaller first; if its index was below the larger's,
        // the larger's index has shifted down by one.
        let small = self.spacecraft.remove(small_idx);
        let adjusted_large = if small_idx < large_idx { large_idx - 1 } else { large_idx };
        let location = small.location.clone();
        let small_name = small.name.clone();
        let large_name = self.spacecraft[adjusted_large].name.clone();

        let payload = crate::flight::Payload::Spacecraft {
            deploy_at: None,
            design: small.design,
            rocket: small.rocket,
            nested_payloads: small.payloads,
            rocket_project_id: small.rocket_project_id,
            name: small.name,
        };
        self.spacecraft[adjusted_large].payloads.push(payload);

        let evt = GameEvent::SpacecraftDocked {
            small: small_name,
            large: large_name,
            location: crate::contract::destination_display_name(&location).to_string(),
        };
        self.event_log.push(self.date, evt);
        true
    }

    /// Undock the `payload_idx`-th payload of `carrier_idx` and add it to
    /// the fleet at the carrier's location. The payload must be a
    /// `Payload::Spacecraft`. Returns true on success.
    pub fn undock_payload(&mut self, carrier_idx: usize, payload_idx: usize) -> bool {
        if carrier_idx >= self.spacecraft.len() { return false; }
        if payload_idx >= self.spacecraft[carrier_idx].payloads.len() { return false; }
        let is_spacecraft = matches!(
            self.spacecraft[carrier_idx].payloads[payload_idx],
            crate::flight::Payload::Spacecraft { .. },
        );
        if !is_spacecraft { return false; }

        let location = self.spacecraft[carrier_idx].location.clone();
        let carrier_name = self.spacecraft[carrier_idx].name.clone();
        let payload = self.spacecraft[carrier_idx].payloads.remove(payload_idx);
        let crate::flight::Payload::Spacecraft {
            design, rocket, nested_payloads, rocket_project_id, name, ..
        } = payload else {
            return false; // unreachable given the matches! above
        };
        let payload_name = name.clone();

        let sc_id = SpacecraftId(self.next_rocket_id);
        self.next_rocket_id += 1;
        self.spacecraft.push(Spacecraft {
            id: sc_id, name, rocket, design,
            location: location.clone(),
            rocket_project_id,
            payloads: nested_payloads,
        });

        let evt = GameEvent::SpacecraftUndocked {
            payload: payload_name,
            carrier: carrier_name,
            location: crate::contract::destination_display_name(&location).to_string(),
        };
        self.event_log.push(self.date, evt);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flaw::FlawTrigger;

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
            discovered: false, trigger: FlawTrigger::PerFlight,
        };
        let flaw2 = Flaw {
            id: FlawId(2),
            description: "Upper injector erosion".into(),
            consequence: FlawConsequence::PerformanceDegradation(0.01),
            activation_chance: 1.0,
            discovery_probability: 1.0,
            discovered: false, trigger: FlawTrigger::PerFlight,
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
            nre_cost: 0.0, improvements: Vec::new(), cumulative_testing_work: 0.0,
            tech_deficiency_ids: Vec::new(), technology_id: None,
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
            nre_cost: 0.0, improvements: Vec::new(), cumulative_testing_work: 0.0,
            tech_deficiency_ids: Vec::new(), technology_id: None,
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
            &engine_projects, &rp.flaws, &[], &mut rng,
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
            rocket_project_id: RocketProjectId(1),
            payloads: Vec::new(),
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
            &gs.player_company.engine_projects, &rp.flaws, &[], &mut rng,
        );

        // Build route and instantiate rocket
        let rocket_mass = sim.degraded_design.total_mass_kg();
        let thrust = sim.degraded_design.group_thrust_n(0);
        let path = crate::location::DELTA_V_MAP
            .shortest_path("earth_surface", "leo", rocket_mass);
        let route = match path {
            Some((p, _)) => crate::flight::build_route(&p, rocket_mass, thrust, false),
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
            true, None,
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
            true, None,
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
            true, None,
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

    /// Test a three-stage hybrid rocket: chemical stages 1-2 for LEO, ion stage for
    /// transit to NEA, then hypergolic thruster for asteroid surface landing.
    /// Verifies that low-thrust pathfinding routes through low-thrust edges for the
    /// ion stage, and the planner switches to chemical pathfinding after staging.
    #[test]
    fn test_hybrid_ion_chemical_to_asteroid_surface() {
        use crate::engine::{EngineDesign, EngineId, EngineCycle, PropellantFraction};
        use crate::propellant::Propellant;
        use crate::stage::{Stage, StageId};
        use crate::rocket::{RocketDesign, RocketDesignId, RocketId};
        use crate::location::DELTA_V_MAP;

        // Stage 1: big kerolox booster for LEO
        let booster_engine = EngineDesign {
            id: EngineId(201),
            name: "Booster".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 2_000_000.0,
            isp_s: 300.0,
            exit_pressure_pa: 80_000.0,
            needs_atmosphere: false,
            mass_kg: 1500.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.73 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.27 },
            ],
        };
        let stage1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: booster_engine.clone(), engine_count: 3,
            propellant_mass_kg: 200_000.0, structural_mass_kg: 5000.0,
            fairing: None,
        };
        let stage2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: booster_engine.clone(), engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 1000.0,
            fairing: None,
        };

        // Stage 3: ion engine for transit (very high Isp, very low thrust)
        let ion_engine = EngineDesign {
            id: EngineId(202),
            name: "Ion Drive".into(),
            cycle: EngineCycle::ElectricPropulsion,
            thrust_n: 1.0,
            isp_s: 3000.0,
            exit_pressure_pa: 0.0,
            needs_atmosphere: false,
            mass_kg: 50.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::Xenon, mass_fraction: 1.0 },
            ],
        };
        let ion_stage = Stage {
            id: StageId(3), name: "Ion".into(),
            engine: ion_engine.clone(), engine_count: 1,
            propellant_mass_kg: 500.0, structural_mass_kg: 50.0,
            fairing: None,
        };

        // Stage 4: small hypergolic thruster for asteroid landing
        let hyp_engine = EngineDesign {
            id: EngineId(203),
            name: "Lander".into(),
            cycle: EngineCycle::PressureFed,
            thrust_n: 5_000.0,
            isp_s: 280.0,
            exit_pressure_pa: 7_000.0,
            needs_atmosphere: false,
            mass_kg: 20.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::NTO, mass_fraction: 0.57 },
                PropellantFraction { propellant: Propellant::UDMH, mass_fraction: 0.43 },
            ],
        };
        let lander_stage = Stage {
            id: StageId(4), name: "Lander".into(),
            engine: hyp_engine.clone(), engine_count: 1,
            propellant_mass_kg: 100.0, structural_mass_kg: 20.0,
            fairing: None,
        };

        let design = RocketDesign {
            id: RocketDesignId(10),
            name: "Asteroid Explorer".into(),
            stage_groups: vec![
                vec![stage1],   // group 0: booster
                vec![stage2],   // group 1: upper chemical
                vec![ion_stage],    // group 2: ion transit
                vec![lander_stage], // group 3: hypergolic lander
            ],
        };

        // Instantiate at LEO (as if we've already launched)
        let mut rocket = design.instantiate(RocketId(1), "leo", 0.0);

        // Jettison groups 0 and 1 (already used for launch)
        for si in 0..rocket.stage_states[0].len() {
            rocket.jettison_stage(0, si);
        }
        for si in 0..rocket.stage_states[1].len() {
            rocket.jettison_stage(1, si);
        }

        // Now the active stage is group 2 (ion)
        assert!(rocket.is_current_stage_low_thrust(&design),
            "Ion stage should be classified as low-thrust");

        // Ion stage should be able to reach Eros orbit (low-thrust path).
        let remaining_dv = rocket.remaining_delta_v(&design);
        assert!(remaining_dv > 7000.0,
            "Ion stage should have enough dv for Eros transit, got {}", remaining_dv);

        let eros_path = DELTA_V_MAP.shortest_path_constrained(
            "leo", "eros_orbit", 1000.0, true,
        );
        assert!(eros_path.is_some(), "Low-thrust path LEO→Eros orbit should exist");

        // Ion stage should NOT be able to reach Eros surface (Eros gravity
        // is just above the ion-drive landing threshold).
        let surface_path = DELTA_V_MAP.shortest_path_constrained(
            "leo", "eros_surface", 1000.0, true,
        );
        assert!(surface_path.is_none(),
            "Low-thrust should not reach Eros surface");

        // Simulate burning the ion stage along the Eros transit.
        let (_, eros_dv) = eros_path.unwrap();
        let burn_result = rocket.burn_sequential(&design, eros_dv, 0.0);
        assert!(burn_result.dv_achieved > 6000.0,
            "Should burn significant dv for Eros transit, got {}", burn_result.dv_achieved);

        // The chemical lander handles eros_orbit → eros_surface (high-thrust only).
        let chemical_path = DELTA_V_MAP.shortest_path_constrained(
            "eros_orbit", "eros_surface", 200.0, false,
        );
        assert!(chemical_path.is_some(), "Chemical path Eros orbit → surface should exist");
        let (path, _dv) = chemical_path.unwrap();
        assert_eq!(path, vec!["eros_orbit", "eros_surface"]);

        // After ion stage, lander should not be low-thrust.
        assert!(!design.stage_groups[3][0].engine.is_low_thrust(),
            "Lander engine should be high-thrust (chemical)");
    }

    /// Set up a game state with a Testing-status rocket project ready for build.
    fn setup_buildable_rocket(gs: &mut GameState) -> RocketProjectId {
        use crate::rocket_project::{RocketProject, RocketProjectId, RocketDesignStatus};

        let (design, engine_projects) = make_three_stage_design();
        gs.player_company.engine_projects = engine_projects;

        let mut rp = RocketProject::new(RocketProjectId(1), design);
        rp.status = RocketDesignStatus::Testing { work_completed: 100.0 };
        let rp_id = rp.project_id;
        gs.player_company.rocket_projects.push(rp);
        rp_id
    }

    /// Drive the manufacturing pipeline to completion by force-finishing all
    /// orders each day and advancing the game until inventory holds a rocket.
    /// Cap at 30 iterations to avoid infinite loops if something is wrong.
    fn run_manufacturing_to_rocket(gs: &mut GameState) {
        // Hire a manufacturing team so auto-assignment can pick orders up.
        gs.player_company.hire_manufacturing_team("MfgA".into());
        for _ in 0..30 {
            // Force every active order to "almost complete" so the next day's
            // work tick finishes them. We still tick advance_day so the full
            // event-handling pipeline (try_unblock, history pushes) runs.
            for order in &mut gs.player_company.manufacturing.orders {
                if !order.waiting_for_prerequisites && order.teams_assigned > 0 {
                    order.work_completed = order.work_required;
                }
            }
            gs.advance_day();
            if !gs.player_company.manufacturing.inventory.rockets.is_empty()
                && gs.player_company.manufacturing.orders.is_empty()
            {
                break;
            }
        }
    }

    #[test]
    fn test_engine_build_accrues_labor_cost() {
        // Direct manufacturing-layer test: an engine order with a team
        // assigned should accrue per-day labor that exceeds material cost
        // for a multi-day build.
        use crate::manufacturing::{ManufacturingOrder, ManufacturingOrderId};
        use crate::engine_project::PropellantPreset;
        use crate::engine::EngineId;
        use crate::engine_project::EngineSource;

        let mut order = ManufacturingOrder::new_engine(
            ManufacturingOrderId(1),
            EngineSource::PlayerDesign(crate::engine_project::EngineProjectId(1)),
            EngineId(1),
            "Test".into(),
            500.0,
            6,
            PropellantPreset::Kerolox,
            0,
            0,
            Vec::new(),
            Vec::new(),
        );
        let material = order.material_cost;
        order.teams_assigned = 1;

        // Tick 30 days of work — this is roughly one team-month = $300K of labor.
        for _ in 0..30 {
            order.apply_daily_work();
        }
        let expected_month_labor = crate::team::MANUFACTURING_MONTHLY_SALARY;
        assert!((order.labor_cost - expected_month_labor).abs() < 1.0,
            "labor after 30 days should be ≈ one month salary, got {}", order.labor_cost);
        // Material cost should be unchanged by the work loop.
        assert!((order.material_cost - material).abs() < 0.01);
    }

    #[test]
    fn test_rocket_cost_history_includes_full_cost_at_completion() {
        let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
        setup_buildable_rocket(&mut gs);

        gs.player_company.order_rocket_build(0).unwrap();
        run_manufacturing_to_rocket(&mut gs);

        let design_id = gs.player_company.rocket_projects[0].design.id;
        let history = gs.player_company.rocket_cost_history.get(&design_id)
            .expect("rocket cost history should be populated at integration");
        assert_eq!(history.len(), 1);
        // The recorded cost must exceed pure material total — labor must be
        // present. The integration alone is many days × team-day salary.
        let recorded = history[0];
        assert!(recorded > 1_000_000.0,
            "recorded rocket cost should reflect labor too; got {}", recorded);
    }

    #[test]
    fn test_engine_cost_history_populated_on_completion() {
        use crate::engine_project::EngineProjectId;
        let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
        setup_buildable_rocket(&mut gs);

        gs.player_company.order_rocket_build(0).unwrap();
        run_manufacturing_to_rocket(&mut gs);

        // Three-stage design: 4 EP1 engines (3 on S1 + 1 on S2), 1 EP2 (S3).
        let ep1_history = gs.player_company.engine_cost_history
            .get(&EngineProjectId(1)).expect("EP1 history populated");
        assert_eq!(ep1_history.len(), 4);
        let ep2_history = gs.player_company.engine_cost_history
            .get(&EngineProjectId(2)).expect("EP2 history populated");
        assert_eq!(ep2_history.len(), 1);

        // Each entry should include labor — for our setup (single team,
        // engine work_required = 108) labor is on the order of $1M, which
        // dwarfs the materials for a 1500kg engine.
        assert!(ep1_history.iter().all(|&c| c > 100_000.0),
            "engine cost should include labor: history={:?}", ep1_history);
    }

    #[test]
    fn test_contracted_engine_build_count_increments_at_order_time() {
        use crate::engine_project::EngineProjectId;
        let mut gs = GameState::new("Test".into(), 200_000_000.0, 42);

        let date = gs.date;
        let seed = gs.seed.clone();
        let tp_idx = gs.player_company.third_party_catalog.iter()
            .position(|e| e.available_from <= date)
            .expect("at least one starter engine should be available");
        gs.player_company.contract_third_party(tp_idx, date, &seed)
            .expect("contracting should succeed");
        let ce_id = gs.player_company.contracted_engines[0].id;

        let (mut design, engine_projects) = make_three_stage_design();
        gs.player_company.engine_projects = engine_projects;

        let contracted_engine = gs.player_company.contracted_engines[0].design.clone();
        for stage in design.stage_groups[0].iter_mut() {
            stage.engine = contracted_engine.clone();
        }
        let stage1_count = design.stage_groups[0][0].engine_count;

        use crate::rocket_project::{RocketProject, RocketProjectId, RocketDesignStatus};
        let mut rp = RocketProject::new(RocketProjectId(1), design);
        rp.status = RocketDesignStatus::Testing { work_completed: 100.0 };
        gs.player_company.rocket_projects.push(rp);

        // Contracted engines are billed and counted at order time (instant
        // delivery to inventory) — no manufacturing cycle needed.
        gs.player_company.order_rocket_build(0).unwrap();

        let count = *gs.player_company.contracted_engine_build_counts
            .get(&ce_id).unwrap_or(&0);
        assert_eq!(count, stage1_count);
        // Player-designed engine history is populated only after the build
        // pipeline runs — at this point it's still empty.
        assert!(gs.player_company.engine_cost_history
            .get(&EngineProjectId(2)).is_none());
    }

    /// Build a tiny single-stage RocketDesign suitable for use as a
    /// Payload::Spacecraft in arrival/deployment tests.
    fn tiny_payload_spacecraft(
        id: u64, name: &str, deploy_at: &str, nested: Vec<Payload>,
    ) -> Payload {
        use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
        use crate::propellant::Propellant;
        use crate::rocket::{RocketDesign, RocketDesignId, RocketId};
        use crate::stage::{Stage, StageId};
        let engine = EngineDesign {
            id: EngineId(id), name: "TinyEng".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 100_000.0, mass_kg: 100.0, isp_s: 300.0,
            exit_pressure_pa: 70_000.0, needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.7 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.3 },
            ],
        };
        let stage = Stage {
            id: StageId(id), name: format!("S{}", id),
            engine, engine_count: 1,
            propellant_mass_kg: 500.0, structural_mass_kg: 100.0,
            fairing: None,
        };
        let design = RocketDesign {
            id: RocketDesignId(id), name: name.into(),
            stage_groups: vec![vec![stage]],
        };
        let nested_mass: f64 = nested.iter().map(|p| p.mass_kg()).sum();
        let rocket = design.instantiate(RocketId(id), "earth_surface", nested_mass);
        Payload::Spacecraft {
            deploy_at: Some(deploy_at.into()),
            design,
            rocket,
            nested_payloads: nested,
            rocket_project_id: RocketProjectId(id),
            name: name.into(),
        }
    }

    /// Assemble a minimal Flight with the given payloads and run the
    /// arrival path. Used by deployment tests below to skip the full
    /// launch+manufacturing pipeline.
    fn arrive_test_flight(
        gs: &mut GameState, destination: &str, payloads: Vec<Payload>,
    ) -> Vec<crate::event::GameEvent> {
        use crate::flight::{Flight, FlightId, FlightLeg, FlightStatus};
        use crate::rocket::{RocketDesign, RocketDesignId, RocketId};

        // Empty carrier design — arrival logic doesn't care about its dv.
        let design = RocketDesign {
            id: RocketDesignId(999), name: "CarrierStub".into(),
            stage_groups: vec![],
        };
        let rocket = design.instantiate(RocketId(999), "earth_surface", 0.0);
        let flight = Flight {
            id: FlightId(1),
            rocket_name: "Carrier".into(),
            rocket_project_id: RocketProjectId(999),
            design,
            rocket,
            payloads,
            current_location: destination.into(),
            route: vec![FlightLeg {
                from: "earth_surface".into(),
                to: destination.into(),
                delta_v_cost: 0.0, burn_days: 0, coast_days: 0,
                ambient_pressure_pa: 0.0,
            }],
            current_leg: 0,
            leg_days_remaining: 0,
            status: FlightStatus::Arrived,
            flaws_activated: vec![],
            launch_date: gs.date,
            persist: false,
            launch_partial: false,
            flaw_rolled_groups: std::collections::HashSet::new(),
        };
        gs.resolve_arrived_flight(flight)
    }

    #[test]
    fn test_spacecraft_payload_deployed_on_arrival() {
        // Skylab-style: Saturn V drops a station as a Spacecraft at LEO.
        let mut gs = GameState::new("Test".into(), 1_000_000.0, 42);
        let skylab = tiny_payload_spacecraft(1, "Skylab", "leo", vec![]);
        let events = arrive_test_flight(&mut gs, "leo", vec![skylab]);
        assert_eq!(gs.spacecraft.len(), 1, "Skylab should be in fleet");
        let sc = &gs.spacecraft[0];
        assert_eq!(sc.name, "Skylab");
        assert_eq!(sc.location, "leo");
        assert!(sc.payloads.is_empty());
        assert!(events.iter().any(|e| matches!(
            e, crate::event::GameEvent::SpacecraftDeployed { spacecraft_name, .. }
                if spacecraft_name == "Skylab"
        )));
    }

    #[test]
    fn test_csm_carrying_lem_keeps_lem_after_deployment() {
        // Apollo-style: CSM is deployed at lunar_orbit carrying LEM as its
        // own payload. The LEM stays *with* CSM (in CSM.payloads), not
        // separately in the fleet, until CSM later flies somewhere.
        let mut gs = GameState::new("Test".into(), 1_000_000.0, 42);
        let lem = tiny_payload_spacecraft(2, "LEM", "lunar_surface", vec![]);
        let csm = tiny_payload_spacecraft(1, "CSM", "lunar_orbit", vec![lem]);
        arrive_test_flight(&mut gs, "lunar_orbit", vec![csm]);

        assert_eq!(gs.spacecraft.len(), 1, "only CSM in fleet, LEM is its payload");
        let csm_sc = &gs.spacecraft[0];
        assert_eq!(csm_sc.name, "CSM");
        assert_eq!(csm_sc.location, "lunar_orbit");
        assert_eq!(csm_sc.payloads.len(), 1);
        match &csm_sc.payloads[0] {
            Payload::Spacecraft { name, deploy_at, .. } => {
                assert_eq!(name, "LEM");
                assert_eq!(deploy_at.as_deref(), Some("lunar_surface"));
            }
            _ => panic!("expected nested Spacecraft payload"),
        }
    }

    #[test]
    fn test_multiple_payloads_at_same_destination() {
        // Rideshare: a launch carrying two contract deliveries to LEO. The
        // arrival handler must pay both contracts.
        use crate::contract::{Contract, ContractId, ContractStatus};
        use crate::calendar::GameDate;
        let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
        let starting_money = gs.player_company.money;
        let contract_a = Contract {
            id: ContractId(1), name: "A".into(),
            destination: "leo".into(), payload_kg: 100.0, payment: 1_000_000.0,
            deadline: GameDate::new(2099, 1, 1),
            status: ContractStatus::Accepted,
            market_id: Default::default(),
        };
        let contract_b = Contract {
            id: ContractId(2), name: "B".into(),
            destination: "leo".into(), payload_kg: 200.0, payment: 2_000_000.0,
            deadline: GameDate::new(2099, 1, 1),
            status: ContractStatus::Accepted,
            market_id: Default::default(),
        };
        gs.player_company.active_contracts.push(contract_a);
        gs.player_company.active_contracts.push(contract_b);

        let payloads = vec![
            Payload::ContractDelivery { contract_id: ContractId(1), payload_kg: 100.0 },
            Payload::ContractDelivery { contract_id: ContractId(2), payload_kg: 200.0 },
        ];
        arrive_test_flight(&mut gs, "leo", payloads);

        assert_eq!(gs.player_company.active_contracts.len(), 0,
            "both contracts should be completed and removed");
        // Money increased by 3M (1M + 2M from the two contracts).
        let earned = gs.player_company.money - starting_money;
        assert!((earned - 3_000_000.0).abs() < 1.0,
            "expected 3M paid out, got {}", earned);
    }

    /// Push a freshly-built minimal Spacecraft into `gs.spacecraft` at
    /// `location` with the given name. Returns its index.
    fn push_test_spacecraft(gs: &mut GameState, id: u64, name: &str, location: &str) -> usize {
        use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
        use crate::propellant::Propellant;
        use crate::rocket::{RocketDesign, RocketDesignId, RocketId};
        use crate::stage::{Stage, StageId};
        let engine = EngineDesign {
            id: EngineId(id), name: "E".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 1.0, mass_kg: 1.0, isp_s: 100.0,
            exit_pressure_pa: 1.0, needs_atmosphere: false,
            propellant_mix: vec![PropellantFraction {
                propellant: Propellant::LOX, mass_fraction: 1.0,
            }],
        };
        let stage = Stage {
            id: StageId(id), name: "S".into(),
            engine, engine_count: 1,
            propellant_mass_kg: 100.0, structural_mass_kg: 10.0,
            fairing: None,
        };
        let design = RocketDesign {
            id: RocketDesignId(id), name: name.into(),
            stage_groups: vec![vec![stage]],
        };
        let rocket = design.instantiate(RocketId(id), location, 0.0);
        gs.spacecraft.push(Spacecraft {
            id: SpacecraftId(id),
            name: name.into(),
            rocket, design,
            location: location.into(),
            rocket_project_id: RocketProjectId(id),
            payloads: Vec::new(),
        });
        gs.spacecraft.len() - 1
    }

    #[test]
    fn test_dock_combines_two_spacecraft() {
        let mut gs = GameState::new("T".into(), 1.0, 0);
        let csm = push_test_spacecraft(&mut gs, 1, "CSM", "lunar_orbit");
        let lem = push_test_spacecraft(&mut gs, 2, "LEM", "lunar_orbit");
        // Dock LEM onto CSM.
        assert!(gs.dock_spacecraft(lem, csm));
        assert_eq!(gs.spacecraft.len(), 1);
        let carrier = &gs.spacecraft[0];
        assert_eq!(carrier.name, "CSM");
        assert_eq!(carrier.payloads.len(), 1);
        match &carrier.payloads[0] {
            Payload::Spacecraft { name, deploy_at, .. } => {
                assert_eq!(name, "LEM");
                assert!(deploy_at.is_none(), "manual undock only");
            }
            _ => panic!("expected Spacecraft payload"),
        }
    }

    #[test]
    fn test_dock_rejects_different_locations() {
        let mut gs = GameState::new("T".into(), 1.0, 0);
        let a = push_test_spacecraft(&mut gs, 1, "A", "leo");
        let b = push_test_spacecraft(&mut gs, 2, "B", "lunar_orbit");
        assert!(!gs.dock_spacecraft(a, b),
            "dock should refuse cross-location");
        assert_eq!(gs.spacecraft.len(), 2,
            "no spacecraft removed on rejected dock");
    }

    #[test]
    fn test_undock_restores_fleet_member() {
        let mut gs = GameState::new("T".into(), 1.0, 0);
        let csm = push_test_spacecraft(&mut gs, 1, "CSM", "lunar_orbit");
        let lem = push_test_spacecraft(&mut gs, 2, "LEM", "lunar_orbit");
        assert!(gs.dock_spacecraft(lem, csm));
        assert_eq!(gs.spacecraft.len(), 1);
        // Carrier index after dock is 0 (was csm, now alone).
        assert!(gs.undock_payload(0, 0));
        assert_eq!(gs.spacecraft.len(), 2);
        // The undocked LEM should be at the same location.
        let lem_idx = gs.spacecraft.iter()
            .position(|sc| sc.name == "LEM")
            .expect("LEM back in fleet");
        assert_eq!(gs.spacecraft[lem_idx].location, "lunar_orbit");
    }

    #[test]
    fn test_dock_then_fly_keeps_payload_aboard() {
        // After docking with deploy_at = None, flying the carrier should
        // not auto-detach the docked payload.
        let mut gs = GameState::new("T".into(), 1.0, 0);
        let csm = push_test_spacecraft(&mut gs, 1, "CSM", "lunar_orbit");
        let lem = push_test_spacecraft(&mut gs, 2, "LEM", "lunar_orbit");
        gs.dock_spacecraft(lem, csm);

        // Synthesize an arrival of the CSM at earth_escape — the existing
        // arrival path should keep the docked LEM aboard because deploy_at
        // is None (never matches a destination).
        let payloads = std::mem::take(&mut gs.spacecraft[0].payloads);
        let _events = arrive_test_flight(&mut gs, "earth_escape", payloads);
        // arrive_test_flight builds its own carrier, so the docked LEM
        // payload becomes a "remaining_payload" on a non-persisted flight,
        // which means it gets dropped — that's fine for this assertion:
        // we just want to confirm the payload was NOT in deployed_spacecraft.
        let deployed_lem = gs.spacecraft.iter().any(|sc| sc.name == "LEM");
        assert!(!deployed_lem,
            "deploy_at = None should never auto-detach on arrival");
    }

    #[test]
    fn test_undock_with_nested_payloads() {
        // Build a chain: A docked into B, B docked into C. Undock B from C
        // and confirm A is still nested in B.
        let mut gs = GameState::new("T".into(), 1.0, 0);
        let _a = push_test_spacecraft(&mut gs, 1, "A", "leo");
        let _b = push_test_spacecraft(&mut gs, 2, "B", "leo");
        let _c = push_test_spacecraft(&mut gs, 3, "C", "leo");
        // Dock A onto B (indices currently 0, 1, 2; B is at 1, A at 0).
        assert!(gs.dock_spacecraft(0, 1));
        // After: spacecraft = [B(carrying A), C]. Dock B onto C.
        assert!(gs.dock_spacecraft(0, 1));
        // After: spacecraft = [C(carrying B(carrying A))]. Undock B from C.
        assert!(gs.undock_payload(0, 0));
        // Now: C alone in fleet, B in fleet with A nested.
        let b = gs.spacecraft.iter().find(|sc| sc.name == "B")
            .expect("B back in fleet");
        assert_eq!(b.payloads.len(), 1);
        match &b.payloads[0] {
            Payload::Spacecraft { name, .. } => assert_eq!(name, "A"),
            _ => panic!("expected nested A"),
        }
    }

    #[test]
    fn test_save_and_load_with_docked_spacecraft() {
        // Round-trip a docked configuration through save/load.
        use crate::save::{save_game, load_game};
        let mut gs = GameState::new("DockCorp".into(), 1.0, 99);
        let csm = push_test_spacecraft(&mut gs, 1, "CSM", "lunar_orbit");
        let lem = push_test_spacecraft(&mut gs, 2, "LEM", "lunar_orbit");
        gs.dock_spacecraft(lem, csm);

        let path = std::env::temp_dir().join(format!(
            "dock_test_{}.json", std::process::id()));
        save_game(&gs, &path).unwrap();
        let loaded = load_game(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.spacecraft.len(), 1);
        let carrier = &loaded.spacecraft[0];
        assert_eq!(carrier.name, "CSM");
        match &carrier.payloads[0] {
            Payload::Spacecraft { name, deploy_at, .. } => {
                assert_eq!(name, "LEM");
                assert!(deploy_at.is_none(), "deploy_at = None survives round-trip");
            }
            _ => panic!("expected nested Spacecraft payload"),
        }
    }
}
