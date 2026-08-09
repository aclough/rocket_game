//! The company: everything a rocket firm owns and does that isn't
//! world-state — teams, projects, designs, manufacturing, contracts,
//! reputation, financials, and the standing bid rules. Split out of
//! game_state.rs (M3 hygiene); `game_state` re-exports the public
//! types so existing paths keep working. Both the player and scripted
//! competitors are a `Company`.

use std::collections::{HashMap, VecDeque};

use serde::{Serialize, Deserialize};

use crate::contract::{self, Contract};
use crate::engine::{EngineCycle, EngineId};
use crate::engine_project::{EngineDesignStatus, EngineProject, EngineProjectId, EngineSource, PropellantPreset, WorkEvent};
use crate::calendar::GameDate;
use crate::event::GameEvent;
use crate::manufacturing::{Manufacturing, ManufacturingOrder, ManufacturingOrderType, InventoryEngine};
use crate::launch::LaunchRecord;
use crate::reputation::Reputation;
use crate::rocket::{RocketDesign, RocketDesignId};
use crate::rocket_project::{RocketProject, RocketProjectId, RocketWorkEvent};
use crate::seed::GameSeed;
use crate::balance_config::BalanceConfig;
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

/// Which engineering pool a project lives in. Used by the
/// donor-search helpers to identify a specific project across the
/// three lists (engines / rockets / reactors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectKind {
    Engine(usize),
    Rocket(usize),
    Reactor(usize),
}

/// What a stage build order is waiting on: which engine it needs, and how
/// many. `None` for anything that isn't a stage order, or whose rocket
/// project or engine has since gone away.
///
/// A free function taking slices rather than a `&self` method because both
/// callers iterate `manufacturing.orders` and need the lookup to borrow only
/// the other fields of `Company`.
fn stage_engine_need(
    order_type: &crate::manufacturing::ManufacturingOrderType,
    rocket_projects: &[RocketProject],
    engine_projects: &[EngineProject],
    contracted_engines: &[ContractedEngine],
) -> Option<(EngineSource, u32)> {
    let crate::manufacturing::ManufacturingOrderType::Stage {
        rocket_project_id, group_index, stage_index, ..
    } = order_type else {
        return None;
    };
    let rp = rocket_projects.iter().find(|rp| rp.project_id == *rocket_project_id)?;
    let stage = rp.design.stage_groups.get(*group_index)?.get(*stage_index)?;
    let source = if let Some(ep) = engine_projects.iter()
        .find(|ep| ep.design.id == stage.engine.id)
    {
        EngineSource::PlayerDesign(ep.project_id)
    } else {
        let ce = contracted_engines.iter().find(|ce| ce.design.id == stage.engine.id)?;
        EngineSource::Contracted(ce.id)
    };
    Some((source, stage.engine_count))
}

/// One row of the manufacturing queue as drawn: which order, how deep it
/// sits in the integration → stage → engine tree, and whether it is part
/// of a rush job.
///
/// The Manufacturing tab's selection indexes *this* list, so the draw side
/// and the key handlers have to agree on it — hence one function
/// ([`Company::manufacturing_display_order`]) rather than a sort in each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MfgRow {
    pub order_index: usize,
    pub depth: u8,
    pub rushed: bool,
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
    /// Allocator for `ReactorProjectId`.
    #[serde(default)]
    pub next_reactor_project_id: u64,
    /// Allocator for `ReactorId` (the design's identity, used like
    /// `next_engine_id` for engine designs).
    #[serde(default)]
    pub next_reactor_id: u64,
    pub teams: Vec<EngineeringTeam>,
    pub manufacturing_teams: Vec<ManufacturingTeam>,
    pub engine_projects: Vec<EngineProject>,
    pub rocket_projects: Vec<RocketProject>,
    /// Player-researched reactor designs and their workflow state.
    #[serde(default)]
    pub reactor_projects: Vec<crate::reactor_project::ReactorProject>,
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

/// What one day of R&D produced — see [`Company::tick_daily_research`].
pub struct ResearchTick {
    pub events: Vec<GameEvent>,
    /// Indices into `engine_projects` whose design completed today.
    pub newly_designed_engines: Vec<usize>,
    /// (engine_project_index, deficiency_id) revision attempts.
    pub tech_def_attempts: Vec<(usize, crate::technology::TechDeficiencyId)>,
    /// Indices into `reactor_projects` whose design completed today.
    pub newly_designed_reactors: Vec<usize>,
    /// (reactor_project_index, deficiency_id) revision attempts.
    pub reactor_tech_def_attempts: Vec<(usize, crate::technology::TechDeficiencyId)>,
}

impl Company {
    pub fn new(name: String, starting_money: f64, seed: &GameSeed, balance_cfg: &BalanceConfig) -> Self {
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
            next_reactor_project_id: 1,
            next_reactor_id: 1,
            teams: Vec::new(),
            manufacturing_teams: Vec::new(),
            engine_projects: Vec::new(),
            rocket_projects: Vec::new(),
            reactor_projects: Vec::new(),
            third_party_catalog: catalog,
            contracted_engines: Vec::new(),
            rocket_designs: Vec::new(),
            manufacturing: Manufacturing::new(&balance_cfg.costs),
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
        // Start with one engineering team
        company.hire_team("Team 1".into(), balance_cfg);
        company
    }

    /// Hire a new engineering team. Returns the event if successful.
    pub fn hire_team(&mut self, name: String, balance_cfg: &BalanceConfig) -> Option<GameEvent> {
        self.money -= balance_cfg.costs.engineering_hiring_cost;
        let id = TeamId(self.next_team_id);
        self.next_team_id += 1;
        let team = EngineeringTeam::new(id, name.clone(), balance_cfg.costs.engineering_monthly_salary);
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
                .sum::<u32>()
            + self.reactor_projects.iter()
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
    pub fn hire_manufacturing_team(&mut self, name: String, balance_cfg: &BalanceConfig) -> Option<GameEvent> {
        self.money -= balance_cfg.costs.manufacturing_hiring_cost;
        let id = TeamId(self.next_team_id);
        self.next_team_id += 1;
        let team = ManufacturingTeam::new(id, name.clone(), balance_cfg.costs.manufacturing_monthly_salary);
        self.manufacturing_teams.push(team);
        Some(GameEvent::ManufacturingTeamHired { name })
    }

    /// Order a floor-space expansion and pay for it. Returns the cost.
    pub fn buy_floor_space(&mut self, units: u32, balance_cfg: &BalanceConfig) -> f64 {
        let cost = self.manufacturing.floor_space.order_expansion(units, &balance_cfg.costs);
        self.money -= cost;
        cost
    }

    /// Start a revision on the engine project at `index`. Returns the
    /// (flaw, improvement) counts queued for revision, or None if the
    /// index is invalid or there is nothing to revise / not Testing.
    pub fn start_engine_revision(&mut self, index: usize) -> Option<(usize, usize)> {
        let project = self.engine_projects.get_mut(index)?;
        if !project.start_revision() {
            return None;
        }
        match &project.status {
            EngineDesignStatus::Revising { remaining_flaw_indices, remaining_improvement_indices, .. } =>
                Some((remaining_flaw_indices.len(), remaining_improvement_indices.len())),
            _ => Some((0, 0)),
        }
    }

    /// Start a revision on the rocket project at `index`. Returns the
    /// flaw count queued for revision, or None if invalid / nothing to do.
    pub fn start_rocket_revision(&mut self, index: usize) -> Option<usize> {
        let project = self.rocket_projects.get_mut(index)?;
        if !project.start_revision() {
            return None;
        }
        match &project.status {
            crate::rocket_project::RocketDesignStatus::Revising { remaining_indices, .. } =>
                Some(remaining_indices.len()),
            _ => Some(0),
        }
    }

    /// Start a revision on the reactor project at `index`. Returns the
    /// (flaw, improvement, deficiency) counts queued for revision, or
    /// None if invalid / nothing to do.
    pub fn start_reactor_revision(&mut self, index: usize) -> Option<(usize, usize, usize)> {
        let project = self.reactor_projects.get_mut(index)?;
        if !project.start_revision() {
            return None;
        }
        match &project.status {
            crate::reactor_project::ReactorDesignStatus::Revising {
                remaining_flaw_indices,
                remaining_improvement_indices,
                remaining_tech_deficiency_ids,
                ..
            } => Some((
                remaining_flaw_indices.len(),
                remaining_improvement_indices.len(),
                remaining_tech_deficiency_ids.len(),
            )),
            _ => Some((0, 0, 0)),
        }
    }

    /// Set the auto-build inventory target for a rocket project
    /// (0 removes the target). The project must be in Testing.
    /// Returns false if the project doesn't exist or isn't Testing.
    pub fn set_auto_build_target(&mut self, project_id: RocketProjectId, target: u32) -> bool {
        let Some(project) = self.rocket_projects.iter().find(|p| p.project_id == project_id)
        else {
            return false;
        };
        if !matches!(project.status, crate::rocket_project::RocketDesignStatus::Testing { .. }) {
            return false;
        }
        if target == 0 {
            self.auto_build_targets.remove(&project_id);
        } else {
            self.auto_build_targets.insert(project_id, target);
        }
        true
    }

    /// Cycle the auto-build target for the rocket project at `index`:
    /// 0 → 1 → 2 → 3 → 0. Returns the new target, or None if the
    /// project doesn't exist or isn't in Testing.
    pub fn cycle_auto_build_target(&mut self, index: usize) -> Option<u32> {
        let project_id = self.rocket_projects.get(index)?.project_id;
        let current = self.auto_build_targets.get(&project_id).copied().unwrap_or(0);
        let next = if current >= 3 { 0 } else { current + 1 };
        if self.set_auto_build_target(project_id, next) {
            Some(next)
        } else {
            None
        }
    }

    /// Start a new engine design project. Returns the event if successful.
    #[allow(clippy::too_many_arguments)] // constructor-style, callers read positionally with names at the call site
    pub fn start_engine_project(
        &mut self,
        name: String,
        cycle: EngineCycle,
        preset: PropellantPreset,
        scale: f64,
        technology_id: Option<crate::technology::TechnologyId>,
        balance_cfg: &BalanceConfig,
    ) -> Option<GameEvent> {
        let project_id = EngineProjectId(self.next_project_id);
        let engine_id = EngineId(self.next_engine_id);
        self.next_project_id += 1;
        self.next_engine_id += 1;

        let mut project = EngineProject::new(
            project_id, engine_id, name.clone(),
            cycle, preset, scale, balance_cfg,
        )?;
        project.technology_id = technology_id;
        self.engine_projects.push(project);
        Some(GameEvent::EngineDesignStarted { engine_name: name })
    }

    /// Start a tentative engine design in `Proposed` status. Used by the
    /// rocket designer; the engine doesn't enter the regular project
    /// queue until the parent rocket is finalised. Returns the new
    /// project id on success.
    #[allow(clippy::too_many_arguments)] // constructor-style, callers read positionally with names at the call site
    pub fn start_proposed_engine_project(
        &mut self,
        name: String,
        cycle: EngineCycle,
        preset: PropellantPreset,
        scale: f64,
        technology_id: Option<crate::technology::TechnologyId>,
        balance_cfg: &BalanceConfig,
    ) -> Option<EngineProjectId> {
        let project_id = EngineProjectId(self.next_project_id);
        let engine_id = EngineId(self.next_engine_id);
        self.next_project_id += 1;
        self.next_engine_id += 1;

        let mut project = EngineProject::new_proposed(
            project_id, engine_id, name,
            cycle, preset, scale, balance_cfg,
        )?;
        project.technology_id = technology_id;
        self.engine_projects.push(project);
        Some(project_id)
    }

    /// Iterator over engine projects that should be visible in the
    /// engines pane — everything except `Proposed`, which belongs to an
    /// in-progress rocket designer session.
    pub fn visible_engine_projects(&self) -> impl Iterator<Item = (usize, &EngineProject)> {
        self.engine_projects.iter()
            .enumerate()
            .filter(|(_, ep)| !matches!(ep.status, EngineDesignStatus::Proposed { .. }))
    }

    /// Look up an engine project by id.
    pub fn find_engine_project(&self, id: EngineProjectId) -> Option<&EngineProject> {
        self.engine_projects.iter().find(|ep| ep.project_id == id)
    }

    /// Look up an engine project by id, mutably.
    pub fn find_engine_project_mut(&mut self, id: EngineProjectId) -> Option<&mut EngineProject> {
        self.engine_projects.iter_mut().find(|ep| ep.project_id == id)
    }

    /// Promote a `Proposed` engine project to `InDesign`. Returns the
    /// engine name on success (for logging). No-op if the id isn't found
    /// or the engine isn't in Proposed status.
    pub fn promote_proposed_engine(&mut self, id: EngineProjectId) -> Option<String> {
        let ep = self.engine_projects.iter_mut().find(|ep| ep.project_id == id)?;
        if !matches!(ep.status, EngineDesignStatus::Proposed { .. }) {
            return None;
        }
        ep.promote_to_in_design();
        Some(ep.design.name.clone())
    }

    /// Delete a `Proposed` engine project. Used to clean up when the
    /// rocket designer is cancelled. Silently no-ops if the id is missing
    /// or the project isn't Proposed (defensive — we never want to
    /// accidentally delete real work).
    pub fn delete_proposed_engine(&mut self, id: EngineProjectId) {
        if let Some(pos) = self.engine_projects.iter().position(|ep|
            ep.project_id == id
            && matches!(ep.status, EngineDesignStatus::Proposed { .. }))
        {
            self.engine_projects.remove(pos);
        }
    }

    // ── Reactor project lifecycle (mirrors the engine helpers above) ──

    /// Spawn a `Proposed` reactor project the editor can iterate on
    /// without committing the player to real work. Promoted to
    /// `InDesign` via `promote_proposed_reactor` when the player hits
    /// Done; deleted via `delete_proposed_reactor` on cancel.
    pub fn start_proposed_reactor(
        &mut self,
        name: String,
        scale: f64,
        enrichment: crate::reactor::EnrichmentLevel,
        balance_cfg: &BalanceConfig,
    ) -> crate::reactor_project::ReactorProjectId {
        let project_id = crate::reactor_project::ReactorProjectId(self.next_reactor_project_id);
        let reactor_id = crate::reactor::ReactorId(self.next_reactor_id);
        self.next_reactor_project_id += 1;
        self.next_reactor_id += 1;
        let project = crate::reactor_project::ReactorProject::new_proposed(
            project_id, reactor_id, name, scale, enrichment, balance_cfg,
        );
        self.reactor_projects.push(project);
        project_id
    }

    pub fn find_reactor_project(
        &self,
        id: crate::reactor_project::ReactorProjectId,
    ) -> Option<&crate::reactor_project::ReactorProject> {
        self.reactor_projects.iter().find(|rp| rp.project_id == id)
    }

    pub fn find_reactor_project_mut(
        &mut self,
        id: crate::reactor_project::ReactorProjectId,
    ) -> Option<&mut crate::reactor_project::ReactorProject> {
        self.reactor_projects.iter_mut().find(|rp| rp.project_id == id)
    }

    /// Visible reactor projects (everything not Proposed). Mirrors
    /// `visible_engine_projects`.
    pub fn visible_reactor_projects(
        &self,
    ) -> impl Iterator<Item = (usize, &crate::reactor_project::ReactorProject)> {
        self.reactor_projects.iter().enumerate().filter(|(_, rp)|
            !matches!(rp.status, crate::reactor_project::ReactorDesignStatus::Proposed { .. })
        )
    }

    /// Promote a `Proposed` reactor to `InDesign`. Returns the reactor
    /// name on success (for logging). No-op if the id isn't found or
    /// the reactor isn't Proposed.
    pub fn promote_proposed_reactor(
        &mut self,
        id: crate::reactor_project::ReactorProjectId,
    ) -> Option<String> {
        let rp = self.reactor_projects.iter_mut().find(|rp| rp.project_id == id)?;
        if !matches!(rp.status, crate::reactor_project::ReactorDesignStatus::Proposed { .. }) {
            return None;
        }
        rp.promote_to_in_design();
        Some(rp.design.name.clone())
    }

    /// Delete a `Proposed` reactor. Defensive — silently no-ops on
    /// non-Proposed ids so we never lose real work.
    pub fn delete_proposed_reactor(
        &mut self,
        id: crate::reactor_project::ReactorProjectId,
    ) {
        if let Some(pos) = self.reactor_projects.iter().position(|rp|
            rp.project_id == id
            && matches!(rp.status, crate::reactor_project::ReactorDesignStatus::Proposed { .. }))
        {
            self.reactor_projects.remove(pos);
        }
    }

    /// Reactor projects that are usable in a rocket — anything past
    /// design, i.e. Testing or Revising. (Phase 3 will tighten "usable"
    /// to "no discovered un-revised flaws"; for now any Testing+ design
    /// installs.)
    pub fn installable_reactor_projects(
        &self,
    ) -> impl Iterator<Item = &crate::reactor_project::ReactorProject> {
        self.reactor_projects.iter().filter(|rp| matches!(
            rp.status,
            crate::reactor_project::ReactorDesignStatus::Testing { .. }
            | crate::reactor_project::ReactorDesignStatus::Revising { .. },
        ))
    }

    /// Add a team to the reactor project at `project_index`. True on
    /// success.
    pub fn add_team_to_reactor_project(&mut self, project_index: usize) -> bool {
        if self.unassigned_team_count() == 0 || project_index >= self.reactor_projects.len() {
            return false;
        }
        self.reactor_projects[project_index].teams_assigned += 1;
        true
    }

    /// Remove a team from the reactor project at `project_index`. True
    /// on success.
    pub fn remove_team_from_reactor_project(&mut self, project_index: usize) -> bool {
        if project_index >= self.reactor_projects.len() {
            return false;
        }
        let p = &mut self.reactor_projects[project_index];
        if p.teams_assigned == 0 {
            return false;
        }
        p.teams_assigned -= 1;
        true
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
    pub fn start_rocket_project(&mut self, design: RocketDesign, balance_cfg: &BalanceConfig) -> Option<GameEvent> {
        let project_id = RocketProjectId(self.next_rocket_project_id);
        self.next_rocket_project_id += 1;
        let name = design.name.clone();
        let project = RocketProject::new(project_id, design, balance_cfg);
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

    /// Engines of `source` that are already paid for and still unspoken for:
    /// everything sitting in inventory, plus everything on the manufacturing
    /// line, less what blocked stage orders will claim the moment their
    /// prerequisites are met.
    ///
    /// A stage order that is *not* blocked has already taken its engines out
    /// of inventory (see `try_unblock_manufacturing_orders`), so only the
    /// blocked ones represent a future claim.
    pub fn uncommitted_engines(&self, source: EngineSource) -> usize {
        let in_stock = self.manufacturing.inventory.engine_count(source);
        let on_the_line = self.manufacturing.orders.iter()
            .filter(|o| matches!(
                &o.order_type,
                crate::manufacturing::ManufacturingOrderType::Engine { source: s, .. }
                    if *s == source
            ))
            .count();

        let claimed: usize = self.manufacturing.orders.iter()
            .filter(|o| o.waiting_for_prerequisites)
            .filter_map(|o| match &o.order_type {
                crate::manufacturing::ManufacturingOrderType::Stage {
                    rocket_project_id, group_index, stage_index, ..
                } => Some((rocket_project_id, group_index, stage_index)),
                _ => None,
            })
            .filter_map(|(rp_id, gi, si)| {
                let rp = self.rocket_projects.iter()
                    .find(|rp| rp.project_id == *rp_id)?;
                let stage = rp.design.stage_groups.get(*gi)?.get(*si)?;
                (self.engine_source_for_id(stage.engine.id)? == source)
                    .then_some(stage.engine_count as usize)
            })
            .sum();

        (in_stock + on_the_line).saturating_sub(claimed)
    }

    /// Order construction of a rocket. Auto-queues engine, stage, and integration orders.
    /// Returns the total material cost and event, or None if the rocket project isn't complete.
    pub fn order_rocket_build(&mut self, rocket_project_index: usize, balance_cfg: &BalanceConfig) -> Option<(f64, GameEvent)> {
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

        // Engines the design needs that are already paid for — in stock or on
        // the line — and not promised to some other blocked stage order. An
        // engine the player ordered by hand from the Engines pane is the same
        // engine this rocket needs, so a build tops the stock up rather than
        // duplicating it. Snapshotted before the loop because the loop pushes
        // orders and (for contracted engines) inventory as it goes.
        let mut spare: HashMap<EngineSource, usize> = HashMap::new();
        for group in &rp.design.stage_groups {
            for stage in group {
                if let Some(source) = self.engine_source_for_id(stage.engine.id) {
                    spare.entry(source)
                        .or_insert_with(|| self.uncommitted_engines(source));
                }
            }
        }

        // Queue engine build orders for each engine needed
        for (gi, group) in rp.design.stage_groups.iter().enumerate() {
            for (si, stage) in group.iter().enumerate() {
                let source = self.engine_source_for_id(stage.engine.id);
                // Draw on the existing pool first; only build the shortfall.
                let from_stock = match source {
                    Some(s) => {
                        let pool = spare.get_mut(&s).expect("prefilled above");
                        let take = (*pool).min(stage.engine_count as usize);
                        *pool -= take;
                        take
                    }
                    None => 0,
                };
                for _e in from_stock..stage.engine_count as usize {
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
                                    ep.design.cycle,
                                    engine_prior,
                                    ep.revision,
                                    ep.flaws.clone(),
                                    ep.improvements.iter().filter(|i| i.actualized).cloned().collect(),
                                    balance_cfg,
                                ).for_rocket(rocket_project_id);
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
                    balance_cfg,
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
            balance_cfg,
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
    pub fn order_engine_build(&mut self, engine_project_index: usize, balance_cfg: &BalanceConfig) -> Option<(f64, GameEvent)> {
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
        let cycle = ep.design.cycle;
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
            cycle,
            engine_prior,
            revision,
            flaws,
            improvements,
            balance_cfg,
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
    pub(crate) fn auto_reorder_rockets(&mut self, balance_cfg: &BalanceConfig) -> Vec<GameEvent> {
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
                if let Some((_cost, evt)) = self.order_rocket_build(index, balance_cfg) {
                    events.push(evt);
                }
            }
        }
        events
    }

    /// Try to unblock stage and integration orders that have their prerequisites ready.
    pub fn try_unblock_manufacturing_orders(&mut self) {
        for order in &mut self.manufacturing.orders {
            if !order.waiting_for_prerequisites {
                continue;
            }
            match &order.order_type {
                crate::manufacturing::ManufacturingOrderType::Stage { .. } => {
                    // Stage needs engines for this stage
                    if let Some((source, needed)) = stage_engine_need(
                        &order.order_type, &self.rocket_projects,
                        &self.engine_projects, &self.contracted_engines,
                    ) {
                        let available = self.manufacturing.inventory.engine_count(source);
                        if available >= needed as usize {
                            order.waiting_for_prerequisites = false;
                            // Consume engines from inventory, rolling
                            // their full build_cost (material + labor)
                            // into this stage order's material_cost.
                            for _ in 0..needed {
                                if let Some(eng) = self.manufacturing.inventory.take_engine(source) {
                                    order.material_cost += eng.build_cost;
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
    pub fn contract_third_party(&mut self, catalog_index: usize, current_date: GameDate, seed: &GameSeed, balance_cfg: &BalanceConfig) -> Option<GameEvent> {
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
            &balance_cfg.flaws,
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

    /// The manufacturing queue arranged as a tree: each integration order,
    /// then the stages feeding it, then the engine builds feeding those.
    /// A rush job's subtree leads, as one contiguous block.
    ///
    /// **The nesting is an attribution, not a fact.** Orders record which
    /// rocket they were queued for, never which stage an engine is for —
    /// components pool by source and go to whichever stage unblocks first,
    /// which is what makes rush-job stealing work at all. So engines are
    /// matched to stages greedily, and two identical engines shown under
    /// different stages could swap without anything noticing. Read a row as
    /// "this is what the queue owes that rocket", not "this engine is
    /// bolted to that stage".
    ///
    /// The attribution is load-bearing rather than cosmetic, because it is
    /// also what bounds a rush:
    ///
    /// - **A rush is one build, not a project.** Orders name a rocket
    ///   *project*, so a second build of the same rocket is
    ///   indistinguishable from the one you rushed. Taking the rush
    ///   target's subtree — the first matching integration order and the
    ///   stages it claims — pins the rush to a single build and leaves a
    ///   later order of the same rocket in the ordinary queue.
    /// - **A rush takes the engines it needs and no more.** Every engine of
    ///   a needed source could satisfy a rushed stage, but rushing all of
    ///   them spreads the floor thinner and finishes the rush *later*.
    ///   Claiming exactly `engine_count` per blocked stage keeps the teams
    ///   concentrated on the ones that actually unblock it.
    /// - **A rush claims the most-advanced engines**, so it inherits
    ///   whatever is nearest done and the untouched builds fall through to
    ///   whoever was going to get them.
    ///
    /// Only *blocked* stages claim engines: an unblocked stage has already
    /// taken its engines out of inventory. Anything left over — standalone
    /// builds, or an order whose project has gone — sits at the top level
    /// in queue order.
    pub fn manufacturing_display_order(&self) -> Vec<MfgRow> {
        let orders = &self.manufacturing.orders;
        let mut placed = vec![false; orders.len()];
        let mut rows: Vec<MfgRow> = Vec::with_capacity(orders.len());

        // One integration order per rushed project — the earliest, which is
        // the build that has been going longest and so is nearest done.
        let rush_targets: Vec<usize> = {
            let mut seen: Vec<RocketProjectId> = Vec::new();
            orders.iter().enumerate()
                .filter(|(_, o)| matches!(
                    o.order_type, ManufacturingOrderType::RocketIntegration { .. }))
                .filter(|(_, o)| o.parent_rocket()
                    .is_some_and(|id| self.rush_projects.contains(&id)))
                .filter(|(_, o)| {
                    let id = o.parent_rocket().expect("filtered above");
                    if seen.contains(&id) { false } else { seen.push(id); true }
                })
                .map(|(i, _)| i)
                .collect()
        };

        // Parts already on the shelf, drawn down as orders claim them.
        // A slot filled from inventory wants no build order at all.
        let mut stock: HashMap<EngineSource, usize> = HashMap::new();
        let mut stage_stock: HashMap<(RocketProjectId, usize, usize), usize> = HashMap::new();
        let emit_subtree = |root: usize, rushed: bool,
                            placed: &mut Vec<bool>, rows: &mut Vec<MfgRow>,
                            stock: &mut HashMap<EngineSource, usize>,
                            stage_stock: &mut HashMap<(RocketProjectId, usize, usize), usize>| {
            let ManufacturingOrderType::RocketIntegration { rocket_project_id, .. } =
                &orders[root].order_type else { return };
            placed[root] = true;
            rows.push(MfgRow { order_index: root, depth: 0, rushed });

            // An integration that is no longer waiting has already taken its
            // stages out of inventory, so it needs nothing more and claims
            // nothing — otherwise it reaches across and adopts the *next*
            // build's stages, which are not feeding it and, under a rush,
            // pull teams off the integration that is the only thing left to
            // do. Same rule as blocked-stages-only for engines, one level up.
            if !orders[root].waiting_for_prerequisites {
                return;
            }

            // One build's worth: a build queues exactly one stage order per
            // (group, index), so a slot already filled means the next
            // matching order belongs to a *different* build of the same
            // rocket and must be left for its own integration.
            let mut slots: Vec<(usize, usize)> = Vec::new();
            for (j, stage) in orders.iter().enumerate() {
                if placed[j] {
                    continue;
                }
                let ManufacturingOrderType::Stage {
                    rocket_project_id: rp, group_index, stage_index, ..
                } = &stage.order_type else { continue };
                if rp != rocket_project_id {
                    continue;
                }
                if slots.contains(&(*group_index, *stage_index)) {
                    continue;
                }
                // A slot already filled from inventory needs no order. Left
                // unchecked, an integration still waiting on (say) S1 goes
                // on adopting every fresh S2 that appears, and under a rush
                // builds them one after another for a slot that was
                // satisfied long ago.
                let slot = (*rocket_project_id, *group_index, *stage_index);
                let on_shelf = stage_stock.entry(slot).or_insert_with(
                    || self.manufacturing.inventory
                        .stage_count(*rocket_project_id, *group_index, *stage_index));
                if *on_shelf > 0 {
                    *on_shelf -= 1;
                    slots.push((*group_index, *stage_index));
                    continue;
                }
                slots.push((*group_index, *stage_index));
                placed[j] = true;
                rows.push(MfgRow { order_index: j, depth: 1, rushed });

                if !stage.waiting_for_prerequisites {
                    continue; // already holds its engines
                }
                let Some((source, needs)) = stage_engine_need(
                    &stage.order_type, &self.rocket_projects,
                    &self.engine_projects, &self.contracted_engines,
                ) else { continue };

                // Only the *outstanding* engines belong under this stage.
                // A stage wanting six with four already on the shelf is two
                // builds away, not six, and claiming six would park the
                // floor on engines nobody is waiting for — and, under a
                // rush, spread the teams three times thinner than the two
                // that actually unblock it.
                //
                // Stock is contested between stages needing the same
                // engine, so it is drawn down in the order they are
                // walked. `try_unblock` won't part-consume — a stage takes
                // all its engines at once or none — so this is only ever an
                // estimate of who gets there first.
                let on_hand = stock.entry(source).or_insert_with(
                    || self.manufacturing.inventory.engine_count(source));
                let from_stock = (*on_hand).min(needs as usize);
                *on_hand -= from_stock;
                let wanted = needs as usize - from_stock;

                // Nearest-done first for a rush, so it inherits the work
                // already sunk; queue order otherwise, which keeps the
                // ordinary display stable.
                let mut candidates: Vec<usize> = orders.iter().enumerate()
                    .filter(|(k, _)| !placed[*k])
                    .filter(|(_, e)| matches!(
                        &e.order_type,
                        ManufacturingOrderType::Engine { source: es, .. } if *es == source))
                    .map(|(k, _)| k)
                    .collect();
                if rushed {
                    candidates.sort_by(|a, b| orders[*b].progress()
                        .partial_cmp(&orders[*a].progress())
                        .unwrap_or(std::cmp::Ordering::Equal));
                }
                for k in candidates.into_iter().take(wanted) {
                    placed[k] = true;
                    rows.push(MfgRow { order_index: k, depth: 2, rushed });
                }
            }
        };

        for root in rush_targets {
            emit_subtree(root, true, &mut placed, &mut rows, &mut stock, &mut stage_stock);
        }
        for i in 0..orders.len() {
            if !placed[i] {
                emit_subtree(i, false, &mut placed, &mut rows, &mut stock, &mut stage_stock);
            }
        }
        for (i, done) in placed.iter_mut().enumerate() {
            if !*done {
                *done = true;
                rows.push(MfgRow { order_index: i, depth: 0, rushed: false });
            }
        }
        rows
    }

    /// Which orders the rush is actually pulling teams onto — the rush
    /// subtrees from [`Self::manufacturing_display_order`], so what the
    /// floor does and what the pane draws can't drift apart.
    pub fn rushed_order_flags(&self) -> Vec<bool> {
        let mut flags = vec![false; self.manufacturing.orders.len()];
        for row in self.manufacturing_display_order() {
            flags[row.order_index] = row.rushed;
        }
        flags
    }

    /// Assign manufacturing teams across the actionable orders.
    ///
    /// With no rush job this is the plain "fewest teams wins" round-robin
    /// over every unblocked order, and only idle teams move.
    ///
    /// A rush job preempts: every team is pulled off ordinary work and
    /// split across the rush orders by the same round-robin, because a
    /// deadline means now and waiting for teams to come free could take
    /// weeks. Preempted teams are not remembered — when the rush ends they
    /// fall idle and this function places them again on the next tick.
    pub fn assign_manufacturing_teams(&mut self) {
        let flags = self.rushed_order_flags();
        let rushing: Vec<usize> = self.manufacturing.orders.iter().enumerate()
            .filter(|(_, o)| !o.waiting_for_prerequisites)
            .filter(|(i, _)| flags[*i])
            .map(|(i, _)| i)
            .collect();

        if !rushing.is_empty() {
            // Preempt: strip the floor, then hand it all to the rush.
            for order in &mut self.manufacturing.orders {
                order.teams_assigned = 0;
            }
        }

        loop {
            if self.unassigned_manufacturing_team_count() == 0 {
                break;
            }
            // Ties keep the earliest order, matching the long-standing
            // `min_by_key` behaviour.
            let best = self.manufacturing.orders.iter().enumerate()
                .filter(|(i, o)| {
                    !o.waiting_for_prerequisites
                        && (rushing.is_empty() || rushing.contains(i))
                })
                .min_by_key(|(_, o)| o.teams_assigned)
                .map(|(i, _)| i);
            match best {
                Some(idx) => {
                    let available = self.unassigned_manufacturing_team_count();
                    if !self.manufacturing.add_team_to_order(idx, available) {
                        break;
                    }
                }
                None => break,
            }
        }
    }

    /// Nominal days to build one of these from nothing, with one team on
    /// every part at once.
    ///
    /// This is the critical path, not the total work: engines for a stage
    /// build alongside each other, all stages build alongside each other,
    /// and integration waits for the last of them. One team means a work
    /// rate of exactly 1.0 (`n^0.85` at n=1), so work-days are days.
    ///
    /// Current learning-curve multipliers are folded in, so the number is
    /// what the *next* one would cost rather than what the first did.
    /// Contracted engines contribute nothing: they arrive on order.
    ///
    /// Deliberately ignores how many teams you actually have, floor space,
    /// and anything already in stock — it is a property of the design, for
    /// comparing one against another.
    pub fn nominal_build_days(
        &self, project: &RocketProject, balance: &BalanceConfig,
    ) -> f64 {
        let rocket_prior = *self.rocket_build_counts
            .get(&project.design.id).unwrap_or(&0);
        let rocket_learning = balance.work.learning_curve_multiplier(rocket_prior);

        let mut critical_path = 0.0_f64;
        for group in &project.design.stage_groups {
            for stage in group {
                let engine_days = match self.engine_source_for_id(stage.engine.id) {
                    Some(EngineSource::PlayerDesign(ep_id)) => self.engine_projects.iter()
                        .find(|ep| ep.project_id == ep_id)
                        .map_or(0.0, |ep| {
                            let prior = *self.engine_build_counts
                                .get(&ep_id).unwrap_or(&0);
                            balance.work.engine_build_work(ep.complexity, stage.engine.mass_kg)
                                * balance.work.learning_curve_multiplier(prior)
                        }),
                    // Contracted engines are delivered when ordered.
                    Some(EngineSource::Contracted(_)) | None => 0.0,
                };
                let stage_days = balance.work.stage_build_work(stage.structural_mass_kg)
                    * rocket_learning;
                critical_path = critical_path.max(engine_days + stage_days);
            }
        }

        let total_stages: u32 = project.design.stage_groups.iter()
            .map(|g| g.len() as u32)
            .sum();
        critical_path
            + balance.work.rocket_integration_work(total_stages) * rocket_learning
    }

    /// Drop rush jobs with nothing left to rush.
    ///
    /// The usual retirement is on the `RocketIntegrated` event — the rush
    /// ends the moment its rocket exists. This is the backstop for a rush
    /// declared on a project whose queue empties some other way, so a
    /// stale entry can't sit there waiting to hijack the next build.
    pub fn clear_finished_rush_jobs(&mut self) {
        let inventory = &self.manufacturing;
        self.rush_projects.retain(|id| {
            inventory.orders.iter().any(|o| o.parent_rocket() == Some(*id))
        });
    }

    /// Auto-assign idle engineering teams to the least-staffed project
    /// that can absorb work, mirroring
    /// `assign_manufacturing_teams`.
    ///
    /// An idle engineering team is pure salary burn: there is no state
    /// in which paying a team to do nothing beats putting it on a
    /// project, because testing and revising consume work just as
    /// designing does. The real strategic decision is which project to
    /// *move* a team to, and that stays manual ([+] steals from the
    /// busiest project).
    ///
    /// `Proposed` projects are skipped — they're unfinished rocket-
    /// designer drafts, not committed work.
    pub fn auto_assign_idle_engineering_teams(&mut self) {
        while self.unassigned_team_count() > 0 {
            // Committed projects across the three pools, apportioned by
            // D'Hondt. Only rockets carry a priority; engines and reactors
            // compete at Normal, which is what they did before priorities
            // existed.
            // Least-staffed committed project across the three pools.
            // `min_by_key` keeps the first of equal minima, so ties
            // resolve in a stable pool-then-index order.
            let mut best: Option<(ProjectKind, u32)> = None;
            let mut consider = |kind: ProjectKind, staffed: u32| {
                if best.as_ref().is_none_or(|b| staffed < b.1) {
                    best = Some((kind, staffed));
                }
            };
            for (i, p) in self.engine_projects.iter().enumerate() {
                if matches!(p.status, EngineDesignStatus::Proposed { .. }) { continue; }
                consider(ProjectKind::Engine(i), p.teams_assigned);
            }
            for (i, p) in self.rocket_projects.iter().enumerate() {
                consider(ProjectKind::Rocket(i), p.teams_assigned);
            }
            for (i, p) in self.reactor_projects.iter().enumerate() {
                if matches!(
                    p.status,
                    crate::reactor_project::ReactorDesignStatus::Proposed { .. },
                ) { continue; }
                consider(ProjectKind::Reactor(i), p.teams_assigned);
            }
            let best = best.map(|(kind, _)| kind);

            let assigned = match best {
                Some(ProjectKind::Engine(i)) => self.add_team_to_project(i),
                Some(ProjectKind::Rocket(i)) => self.add_team_to_rocket_project(i),
                Some(ProjectKind::Reactor(i)) => self.add_team_to_reactor_project(i),
                // Nothing to work on — the teams stay idle and the
                // Overview's next-steps panel prompts for a project.
                None => break,
            };
            if !assigned {
                break;
            }
        }
    }

    /// Find the busiest engineering project across the three pools
    /// (engines / rockets / reactors), excluding `exclude`. Returns the
    /// donor's kind, index, and name; caller decrements teams_assigned
    /// and credits the target.
    fn busiest_engineering_donor(
        &self,
        exclude: ProjectKind,
    ) -> Option<(ProjectKind, usize, String)> {
        let mut best: Option<(ProjectKind, usize, u32, String)> = None;
        for (i, ep) in self.engine_projects.iter().enumerate() {
            if ep.teams_assigned == 0 { continue; }
            if matches!(exclude, ProjectKind::Engine(j) if j == i) { continue; }
            if best.as_ref().is_none_or(|b| ep.teams_assigned > b.2) {
                best = Some((ProjectKind::Engine(i), i, ep.teams_assigned, ep.design.name.clone()));
            }
        }
        for (i, rp) in self.rocket_projects.iter().enumerate() {
            if rp.teams_assigned == 0 { continue; }
            if matches!(exclude, ProjectKind::Rocket(j) if j == i) { continue; }
            if best.as_ref().is_none_or(|b| rp.teams_assigned > b.2) {
                best = Some((ProjectKind::Rocket(i), i, rp.teams_assigned, rp.design.name.clone()));
            }
        }
        for (i, rp) in self.reactor_projects.iter().enumerate() {
            if rp.teams_assigned == 0 { continue; }
            if matches!(exclude, ProjectKind::Reactor(j) if j == i) { continue; }
            if best.as_ref().is_none_or(|b| rp.teams_assigned > b.2) {
                best = Some((ProjectKind::Reactor(i), i, rp.teams_assigned, rp.design.name.clone()));
            }
        }
        best.map(|(kind, _, _, name)| (kind, 0, name))
    }

    /// Move one team from `donor` to the project at `(target_kind,
    /// target_index)`. Callers have already confirmed the donor is
    /// valid via `busiest_engineering_donor`.
    fn move_engineering_team(&mut self, donor: ProjectKind, target_kind: ProjectKind) {
        match donor {
            ProjectKind::Engine(i) => self.engine_projects[i].teams_assigned -= 1,
            ProjectKind::Rocket(i) => self.rocket_projects[i].teams_assigned -= 1,
            ProjectKind::Reactor(i) => self.reactor_projects[i].teams_assigned -= 1,
        }
        match target_kind {
            ProjectKind::Engine(i) => self.engine_projects[i].teams_assigned += 1,
            ProjectKind::Rocket(i) => self.rocket_projects[i].teams_assigned += 1,
            ProjectKind::Reactor(i) => self.reactor_projects[i].teams_assigned += 1,
        }
    }

    /// Steal an engineering team from the busiest engineering project
    /// (excluding the target) and assign it to the target engine
    /// project. Returns the donor's display name on success.
    pub fn steal_engineering_team_to_engine_project(&mut self, target: usize) -> Option<String> {
        if target >= self.engine_projects.len() {
            return None;
        }
        let (donor, _, name) = self.busiest_engineering_donor(ProjectKind::Engine(target))?;
        self.move_engineering_team(donor, ProjectKind::Engine(target));
        Some(name)
    }

    /// Steal an engineering team and assign to the target rocket project.
    pub fn steal_engineering_team_to_rocket_project(&mut self, target: usize) -> Option<String> {
        if target >= self.rocket_projects.len() {
            return None;
        }
        let (donor, _, name) = self.busiest_engineering_donor(ProjectKind::Rocket(target))?;
        self.move_engineering_team(donor, ProjectKind::Rocket(target));
        Some(name)
    }

    /// Steal an engineering team and assign to the target reactor
    /// project. Mirrors the engine/rocket variants so the Reactors
    /// pane's `+` key behaves the same as the others.
    pub fn steal_engineering_team_to_reactor_project(&mut self, target: usize) -> Option<String> {
        if target >= self.reactor_projects.len() {
            return None;
        }
        let (donor, _, name) = self.busiest_engineering_donor(ProjectKind::Reactor(target))?;
        self.move_engineering_team(donor, ProjectKind::Reactor(target));
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

    /// One day of R&D across this company's engine / rocket / reactor
    /// project lists: daily work, flaw discovery, revisions, and NRE
    /// accrual. Extracted from `advance_day` (M3 hygiene) so scripted
    /// competitors can eventually run the same loop. Tech-deficiency
    /// *resolution* stays with the game state — it needs the world's
    /// technology table — so the attempt lists ride back in the report.
    pub fn tick_daily_research(
        &mut self,
        rng: &mut rand::rngs::StdRng,
        balance_cfg: &BalanceConfig,
    ) -> ResearchTick {
        let mut events: Vec<GameEvent> = Vec::new();
        let mut newly_designed_engines: Vec<usize> = Vec::new();
        // (engine_project_index, deficiency_id)
        let mut tech_def_attempts: Vec<(usize, crate::technology::TechDeficiencyId)> = Vec::new();
        // Reactor equivalents (mirror the engine tech-deficiency flow).
        let mut newly_designed_reactors: Vec<usize> = Vec::new();
        let mut reactor_tech_def_attempts: Vec<(usize, crate::technology::TechDeficiencyId)> = Vec::new();
        let next_flaw_id = &mut self.next_flaw_id;
        

        for (pi, project) in self.engine_projects.iter_mut().enumerate() {
            let engine_name = project.design.name.clone();
            let work_events = project.apply_daily_work(rng, next_flaw_id, balance_cfg);
            for we in work_events {
                let evt = match we {
                    WorkEvent::DesignComplete => {
                        newly_designed_engines.push(pi);
                        GameEvent::EngineDesignComplete { engine_name: engine_name.clone() }
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
                                    events.push(evt);
            }
        }

        for project in &mut self.rocket_projects {
            let rocket_name = project.design.name.clone();
            let work_events = project.apply_daily_work(rng, next_flaw_id, balance_cfg);
            for we in work_events {
                let evt = match we {
                    RocketWorkEvent::DesignComplete =>
                        GameEvent::RocketDesignComplete { rocket_name: rocket_name.clone() },
                    RocketWorkEvent::TestingCycleComplete => continue,
                    RocketWorkEvent::FlawDiscovered { flaw_description } =>
                        GameEvent::RocketFlawDiscovered { rocket_name: rocket_name.clone(), flaw_description },
                    RocketWorkEvent::RevisionComplete =>
                        GameEvent::RocketRevisionComplete { rocket_name: rocket_name.clone() },
                };
                                    events.push(evt);
            }
        }

        // Reactor projects accrue daily work just like engine projects.
        // Phase 1 only fires DesignComplete; testing/revision events
        // arrive in Phase 3.
        for (pi, project) in self.reactor_projects.iter_mut().enumerate() {
            let reactor_name = project.design.name.clone();
            let work_events = project.apply_daily_work(rng, next_flaw_id, balance_cfg);
            for we in work_events {
                let evt = match we {
                    crate::reactor_project::ReactorWorkEvent::DesignComplete => {
                        newly_designed_reactors.push(pi);
                        GameEvent::ReactorDesignComplete { reactor_name: reactor_name.clone() }
                    }
                    crate::reactor_project::ReactorWorkEvent::TestingCycleComplete => continue,
                    crate::reactor_project::ReactorWorkEvent::FlawDiscovered { flaw_description } =>
                        GameEvent::ReactorFlawDiscovered { reactor_name: reactor_name.clone(), flaw_description },
                    crate::reactor_project::ReactorWorkEvent::ImprovementDiscovered { description } =>
                        GameEvent::ReactorImprovementDiscovered { reactor_name: reactor_name.clone(), description },
                    crate::reactor_project::ReactorWorkEvent::ImprovementActualized { description } =>
                        GameEvent::ReactorImprovementActualized { reactor_name: reactor_name.clone(), description },
                    crate::reactor_project::ReactorWorkEvent::RevisionComplete =>
                        GameEvent::ReactorRevisionComplete { reactor_name: reactor_name.clone() },
                    crate::reactor_project::ReactorWorkEvent::TechDeficiencyAttempted { deficiency_id } => {
                        reactor_tech_def_attempts.push((pi, deficiency_id));
                        continue;
                    }
                };
                                    events.push(evt);
            }
        }

        // Accumulate NRE (engineering salary) on active projects
        let daily_salary = balance_cfg.costs.engineering_monthly_salary / 30.0;
        for project in &mut self.engine_projects {
            if project.teams_assigned > 0 {
                project.nre_cost += project.teams_assigned as f64 * daily_salary;
            }
        }
        for project in &mut self.rocket_projects {
            if project.teams_assigned > 0 {
                project.nre_cost += project.teams_assigned as f64 * daily_salary;
            }
        }
        for project in &mut self.reactor_projects {
            if project.teams_assigned > 0 {
                project.nre_cost += project.teams_assigned as f64 * daily_salary;
            }
        }

        ResearchTick {
            events,
            newly_designed_engines,
            tech_def_attempts,
            newly_designed_reactors,
            reactor_tech_def_attempts,
        }
    }

}

