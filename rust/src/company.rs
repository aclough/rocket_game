use crate::contract::{Contract, Destination};
use crate::design_lineage::DesignLineage;
use crate::engine::costs;
use crate::engine_design::{default_engine_lineages, create_engine, EngineDesign, FuelType};
use crate::engineering_team::{team_efficiency, EngineeringTeam, TeamAssignment, TeamType, WorkEvent,
    ENGINEERING_HIRE_COST, MANUFACTURING_HIRE_COST};
use crate::flaw::FlawGenerator;
use crate::launch_site::LaunchSite;
use crate::manufacturing::{Manufacturing, ManufacturingOrderId, ManufacturingOrderType, manufacturing_team_efficiency};

use crate::rocket_design::RocketDesign;

/// Cost to refresh the contract list
pub const CONTRACT_REFRESH_COST: f64 = 10_000_000.0; // $10M

/// Number of contracts to show at once
pub const CONTRACTS_TO_SHOW: usize = 5;

/// A company that designs, builds, and launches rockets.
/// Contains all state specific to a single company (player or AI).
#[derive(Debug, Clone)]
pub struct Company {
    /// Current funds
    pub money: f64,
    /// Player fame/reputation (0.0+)
    pub fame: f64,
    /// Launch site infrastructure
    pub launch_site: LaunchSite,
    /// Next contract ID to assign
    next_contract_id: u32,
    /// Available contracts to choose from
    pub available_contracts: Vec<Contract>,
    /// Currently selected contract (if any)
    pub active_contract: Option<Contract>,
    /// IDs of completed contracts
    pub completed_contracts: Vec<u32>,
    /// IDs of failed contracts
    pub failed_contracts: Vec<u32>,
    /// Rocket design lineages (head + frozen revisions)
    pub rocket_designs: Vec<DesignLineage<RocketDesign>>,
    /// Next design ID to assign
    next_design_id: u32,
    /// Total launches attempted
    pub total_launches: u32,
    /// Successful launches
    pub successful_launches: u32,
    /// Engine design lineages (head + frozen revisions)
    pub engine_designs: Vec<DesignLineage<EngineDesign>>,
    /// Engineering teams that work on designs/engines
    pub teams: Vec<EngineeringTeam>,
    /// Next team ID to assign
    next_team_id: u32,
    /// Flaw generator for creating design flaws
    pub flaw_generator: FlawGenerator,
    /// Manufacturing facilities, orders, and inventory
    pub manufacturing: Manufacturing,
    /// Whether to auto-assign idle manufacturing teams each day
    pub auto_assign_manufacturing: bool,
}

impl Company {
    /// Create a new company with starting conditions
    pub fn new() -> Self {
        let default_design = RocketDesign::default_design();
        let mut company = Self {
            money: costs::STARTING_BUDGET,
            fame: 0.0,
            launch_site: LaunchSite::new(),
            next_contract_id: 1,
            available_contracts: Vec::new(),
            active_contract: None,
            completed_contracts: Vec::new(),
            failed_contracts: Vec::new(),
            rocket_designs: vec![DesignLineage::new("Default Rocket", default_design)],
            next_design_id: 2,
            total_launches: 0,
            successful_launches: 0,
            engine_designs: default_engine_lineages(),
            teams: Vec::new(),
            next_team_id: 1,
            flaw_generator: FlawGenerator::new(),
            manufacturing: Manufacturing::new(),
            auto_assign_manufacturing: false,
        };

        // Generate initial contracts
        company.generate_contracts(CONTRACTS_TO_SHOW);

        // Start with one engineering team (free at game start)
        let starting_team = EngineeringTeam::new(company.next_team_id, TeamType::Engineering);
        company.teams.push(starting_team);
        company.next_team_id += 1;

        company
    }

    // ==========================================
    // Contract Management
    // ==========================================

    /// Generate new contracts
    pub fn generate_contracts(&mut self, count: usize) {
        self.available_contracts =
            Contract::generate_diverse_batch(count, self.next_contract_id);
        self.next_contract_id += count as u32;
    }

    /// Refresh contracts (costs money)
    pub fn refresh_contracts(&mut self) -> bool {
        if self.money < CONTRACT_REFRESH_COST {
            return false;
        }

        self.money -= CONTRACT_REFRESH_COST;
        self.generate_contracts(CONTRACTS_TO_SHOW);
        true
    }

    /// Check if company can afford to refresh contracts
    pub fn can_refresh_contracts(&self) -> bool {
        self.money >= CONTRACT_REFRESH_COST
    }

    /// Select a contract by ID
    pub fn select_contract(&mut self, contract_id: u32) -> bool {
        if let Some(idx) = self
            .available_contracts
            .iter()
            .position(|c| c.id == contract_id)
        {
            self.active_contract = Some(self.available_contracts.remove(idx));
            true
        } else {
            false
        }
    }

    /// Get the currently active contract
    pub fn get_active_contract(&self) -> Option<&Contract> {
        self.active_contract.as_ref()
    }

    /// Get the target delta-v for the current mission
    /// Returns LEO target if no contract is active
    pub fn get_target_delta_v(&self) -> f64 {
        self.active_contract
            .as_ref()
            .map(|c| c.destination.required_delta_v())
            .unwrap_or(Destination::LEO.required_delta_v())
    }

    /// Get the payload mass for the current mission
    pub fn get_payload_mass(&self) -> f64 {
        self.active_contract
            .as_ref()
            .map(|c| c.payload_mass_kg)
            .unwrap_or(self.rocket_designs[0].head().payload_mass_kg)
    }

    /// Called after a successful launch
    /// Deducts the rocket cost, testing costs, and adds the reward
    /// Returns the reward earned and increments turn
    pub fn complete_contract(&mut self, rocket_design_id: usize) -> f64 {
        self.total_launches += 1;
        self.successful_launches += 1;

        // Deduct the rocket cost and testing costs
        let design = self.rocket_designs[rocket_design_id].head();
        let rocket_cost = design.total_cost();
        let testing_cost = design.get_testing_spent();
        self.money -= rocket_cost + testing_cost;

        // Store propellant data before recording (needs immutable borrow first)
        let propellant_loaded = design.propellant_by_fuel_type();
        let propellant_remaining = design.propellant_remaining_by_fuel_type();

        // Record success on both the design version and the lineage
        let head = self.rocket_designs[rocket_design_id].head_mut();
        head.launch_record.record_success();
        head.launch_record.last_propellant_loaded = Some(propellant_loaded);
        head.launch_record.last_propellant_remaining = Some(propellant_remaining);
        self.rocket_designs[rocket_design_id].launch_record.record_success();

        // Reset testing_spent so we don't double-charge if design is reused
        self.rocket_designs[rocket_design_id].head_mut().testing_spent = 0.0;

        if let Some(contract) = self.active_contract.take() {
            let reward = contract.reward;
            self.money += reward;
            self.completed_contracts.push(contract.id);

            // Generate new contracts to replace the completed one
            if self.available_contracts.len() < CONTRACTS_TO_SHOW {
                let needed = CONTRACTS_TO_SHOW - self.available_contracts.len();
                let new_contracts =
                    Contract::generate_batch(needed, self.next_contract_id);
                self.next_contract_id += needed as u32;
                self.available_contracts.extend(new_contracts);
            }

            reward
        } else {
            0.0
        }
    }

    /// Called after a failed launch
    /// Deducts the rocket cost and testing costs, records the failure
    pub fn fail_contract(&mut self, rocket_design_id: usize) {
        self.total_launches += 1;

        // Deduct the rocket cost and testing costs - failed launches still cost money
        let design = self.rocket_designs[rocket_design_id].head();
        let rocket_cost = design.total_cost();
        let testing_cost = design.get_testing_spent();
        self.money -= rocket_cost + testing_cost;

        // Store propellant data (loaded only; remaining is None on failure)
        let propellant_loaded = design.propellant_by_fuel_type();

        // Record failure on both the design version and the lineage
        let head = self.rocket_designs[rocket_design_id].head_mut();
        head.launch_record.record_failure();
        head.launch_record.last_propellant_loaded = Some(propellant_loaded);
        head.launch_record.last_propellant_remaining = None;
        self.rocket_designs[rocket_design_id].launch_record.record_failure();

        // Reset testing_spent so we don't double-charge on retry
        self.rocket_designs[rocket_design_id].head_mut().testing_spent = 0.0;

        // Don't remove the active contract - player can retry
    }

    /// Abandon the current contract without launching
    /// Returns the contract to the pool
    pub fn abandon_contract(&mut self) {
        if let Some(contract) = self.active_contract.take() {
            self.available_contracts.push(contract);
        }
    }

    // ==========================================
    // Rocket Cost & Budget
    // ==========================================

    /// Get success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_launches == 0 {
            0.0
        } else {
            (self.successful_launches as f64 / self.total_launches as f64) * 100.0
        }
    }

    /// Check if company is bankrupt (can't afford the cheapest possible rocket)
    pub fn is_bankrupt(&self) -> bool {
        // Minimum rocket cost is roughly: 1 engine + overhead
        // Cheapest engine is Kerolox at $10M + $5M stage + $10M rocket overhead = $25M minimum
        self.money < 25_000_000.0
    }

    // ==========================================
    // Design Management
    // ==========================================

    /// Get the number of rocket designs (lineages)
    pub fn get_rocket_design_count(&self) -> usize {
        self.rocket_designs.len()
    }

    /// Get a rocket design head by index
    pub fn get_rocket_design(&self, index: usize) -> Option<&RocketDesign> {
        self.rocket_designs.get(index).map(|l| l.head())
    }

    /// Get the full lineage at a given index (for lineage-level stats)
    pub fn get_rocket_lineage(&self, index: usize) -> Option<&DesignLineage<RocketDesign>> {
        self.rocket_designs.get(index)
    }

    /// Save a new design as a new lineage
    /// Returns the index of the new lineage
    pub fn save_new_design(&mut self, design: RocketDesign) -> usize {
        let mut d = design;
        if d.name == "Unnamed Rocket" || d.name == "Default Rocket" {
            d.name = format!("Design #{}", self.next_design_id);
            self.next_design_id += 1;
        }
        let name = d.name.clone();
        self.rocket_designs.push(DesignLineage::new(&name, d));
        self.rocket_designs.len() - 1
    }

    /// Save a new design with a specific name as a new lineage
    /// Returns the index of the new lineage
    pub fn save_new_design_as(&mut self, design: RocketDesign, name: &str) -> usize {
        let mut d = design;
        d.name = name.to_string();
        self.rocket_designs.push(DesignLineage::new(name, d));
        self.rocket_designs.len() - 1
    }

    /// Get a clone of a rocket design's head (for loading into designer)
    pub fn load_rocket_design(&self, index: usize) -> Option<RocketDesign> {
        self.rocket_designs.get(index).map(|l| l.head().clone())
    }

    /// Update a rocket design's head with the given design
    pub fn update_rocket_design(&mut self, index: usize, design: RocketDesign) -> bool {
        if let Some(lineage) = self.rocket_designs.get_mut(index) {
            let new_name = design.name.clone();
            *lineage.head_mut() = design;
            lineage.name = new_name.clone();
            lineage.head_mut().name = new_name;
            true
        } else {
            false
        }
    }

    /// Delete a rocket design lineage by index
    /// Prevents deleting the last design
    pub fn delete_rocket_design(&mut self, index: usize) -> bool {
        if index < self.rocket_designs.len() && self.rocket_designs.len() > 1 {
            self.rocket_designs.remove(index);
            true
        } else {
            false
        }
    }

    /// Rename a rocket design lineage
    pub fn rename_rocket_design(&mut self, index: usize, new_name: &str) -> bool {
        if let Some(lineage) = self.rocket_designs.get_mut(index) {
            lineage.name = new_name.to_string();
            lineage.head_mut().name = new_name.to_string();
            true
        } else {
            false
        }
    }

    /// Duplicate a rocket design lineage
    /// Returns the index of the new lineage
    pub fn duplicate_rocket_design(&mut self, index: usize) -> Option<usize> {
        if let Some(lineage) = self.rocket_designs.get(index) {
            let mut new_design = lineage.head().clone();
            new_design.name = format!("{} (Copy)", lineage.name);
            new_design.reset_flaws();
            let new_name = new_design.name.clone();
            self.rocket_designs.push(DesignLineage::new(&new_name, new_design));
            Some(self.rocket_designs.len() - 1)
        } else {
            None
        }
    }

    /// Create a new empty design lineage
    /// Returns the index of the new lineage
    pub fn create_new_design(&mut self) -> usize {
        let mut design = RocketDesign::new();
        design.name = format!("Design #{}", self.next_design_id);
        self.next_design_id += 1;
        let name = design.name.clone();
        self.rocket_designs.push(DesignLineage::new(&name, design));
        self.rocket_designs.len() - 1
    }

    /// Create a new design lineage based on the default template
    /// Returns the index of the new lineage
    pub fn create_default_design(&mut self) -> usize {
        let mut design = RocketDesign::default_design();
        design.name = format!("Design #{}", self.next_design_id);
        self.next_design_id += 1;
        let name = design.name.clone();
        self.rocket_designs.push(DesignLineage::new(&name, design));
        self.rocket_designs.len() - 1
    }

    // ==========================================
    // Engine Design Management
    // ==========================================

    /// Create a new engine design with the given fuel type and scale
    /// Returns the index of the new lineage
    pub fn create_engine_design(&mut self, fuel_type: FuelType, scale: f64) -> usize {
        let engine = create_engine(fuel_type, scale);
        let name = format!("{} Engine #{}", fuel_type.display_name(), self.engine_designs.len() + 1);
        self.engine_designs.push(DesignLineage::new(&name, engine));
        self.engine_designs.len() - 1
    }

    /// Duplicate an engine design lineage
    /// Returns the index of the new lineage
    pub fn duplicate_engine_design(&mut self, index: usize) -> Option<usize> {
        if let Some(lineage) = self.engine_designs.get(index) {
            let mut new_engine = lineage.head().clone();
            // Reset to specification so the copy can be modified
            new_engine.workflow = crate::design_workflow::DesignWorkflow::new();
            let new_name = format!("{} (Copy)", lineage.name);
            self.engine_designs.push(DesignLineage::new(&new_name, new_engine));
            Some(self.engine_designs.len() - 1)
        } else {
            None
        }
    }

    /// Delete an engine design lineage by index
    /// Prevents deleting the last engine
    pub fn delete_engine_design(&mut self, index: usize) -> bool {
        if index < self.engine_designs.len() && self.engine_designs.len() > 1 {
            self.engine_designs.remove(index);
            true
        } else {
            false
        }
    }

    /// Rename an engine design lineage
    pub fn rename_engine_design(&mut self, index: usize, new_name: &str) -> bool {
        if let Some(lineage) = self.engine_designs.get_mut(index) {
            lineage.name = new_name.to_string();
            true
        } else {
            false
        }
    }

    /// Set the scale of an engine design
    pub fn set_engine_design_scale(&mut self, index: usize, scale: f64) -> bool {
        if let Some(lineage) = self.engine_designs.get_mut(index) {
            lineage.head_mut().set_scale(scale)
        } else {
            false
        }
    }

    /// Set the fuel type of an engine design
    pub fn set_engine_design_fuel_type(&mut self, index: usize, fuel_type: FuelType) -> bool {
        if let Some(lineage) = self.engine_designs.get_mut(index) {
            lineage.head_mut().set_fuel_type(fuel_type)
        } else {
            false
        }
    }

    /// Set the engine cycle of an engine design
    pub fn set_engine_design_cycle(&mut self, index: usize, cycle: crate::engine_design::EngineCycle) -> bool {
        if let Some(lineage) = self.engine_designs.get_mut(index) {
            lineage.head_mut().set_cycle(cycle)
        } else {
            false
        }
    }

    // ==========================================
    // Fame Management
    // ==========================================

    /// Adjust fame by a delta (can be positive or negative)
    pub fn adjust_fame(&mut self, delta: f64) {
        self.fame = (self.fame + delta).max(0.0);
    }

    /// Get current fame level as a tier (0-5)
    pub fn get_fame_tier(&self) -> u32 {
        match self.fame as u32 {
            0..=9 => 0,      // Unknown
            10..=29 => 1,    // Newcomer
            30..=59 => 2,    // Established
            60..=99 => 3,    // Renowned
            100..=199 => 4,  // Famous
            _ => 5,          // Legendary
        }
    }

    /// Get fame tier name
    pub fn get_fame_tier_name(&self) -> &'static str {
        match self.get_fame_tier() {
            0 => "Unknown",
            1 => "Newcomer",
            2 => "Established",
            3 => "Renowned",
            4 => "Famous",
            _ => "Legendary",
        }
    }

    // ==========================================
    // Launch Site Management
    // ==========================================

    /// Check if a rocket design can be launched at the current launch site
    pub fn can_launch_rocket_at_site(&self, rocket_design_id: usize) -> bool {
        if let Some(design) = self.get_rocket_design(rocket_design_id) {
            self.launch_site.can_launch_rocket(design.total_wet_mass_kg())
        } else {
            false
        }
    }

    /// Upgrade the launch pad (returns true if successful)
    pub fn upgrade_launch_pad(&mut self) -> bool {
        let cost = self.launch_site.pad_upgrade_cost();
        if cost > 0.0 && self.money >= cost {
            self.money -= cost;
            self.launch_site.upgrade_pad()
        } else {
            false
        }
    }

    // ==========================================
    // Engineering Team Management
    // ==========================================

    /// Hire a new engineering team (costs ENGINEERING_HIRE_COST)
    /// Returns Some(team_id) if affordable, None otherwise
    pub fn hire_engineering_team(&mut self) -> Option<u32> {
        if self.money < ENGINEERING_HIRE_COST {
            return None;
        }
        self.money -= ENGINEERING_HIRE_COST;
        let team = EngineeringTeam::new(self.next_team_id, TeamType::Engineering);
        let id = team.id;
        self.teams.push(team);
        self.next_team_id += 1;
        Some(id)
    }

    /// Hire a new manufacturing team (costs MANUFACTURING_HIRE_COST)
    /// Returns Some(team_id) if affordable, None otherwise
    pub fn hire_manufacturing_team(&mut self) -> Option<u32> {
        if self.money < MANUFACTURING_HIRE_COST {
            return None;
        }
        self.money -= MANUFACTURING_HIRE_COST;
        let team = EngineeringTeam::new(self.next_team_id, TeamType::Manufacturing);
        let id = team.id;
        self.teams.push(team);
        self.next_team_id += 1;
        Some(id)
    }

    /// Fire a team by ID
    /// Returns true if team was found and removed
    pub fn fire_team(&mut self, team_id: u32) -> bool {
        if let Some(idx) = self.teams.iter().position(|t| t.id == team_id) {
            self.teams.remove(idx);
            true
        } else {
            false
        }
    }

    /// Get the number of teams
    pub fn get_team_count(&self) -> usize {
        self.teams.len()
    }

    /// Get a team by ID
    pub fn get_team(&self, team_id: u32) -> Option<&EngineeringTeam> {
        self.teams.iter().find(|t| t.id == team_id)
    }

    /// Get a mutable reference to a team by ID
    pub fn get_team_mut(&mut self, team_id: u32) -> Option<&mut EngineeringTeam> {
        self.teams.iter_mut().find(|t| t.id == team_id)
    }

    /// Assign a team to work on a rocket design (engineering teams only)
    pub fn assign_team_to_design(&mut self, team_id: u32, rocket_design_id: usize) -> bool {
        if rocket_design_id >= self.rocket_designs.len() {
            return false;
        }

        if let Some(team) = self.get_team_mut(team_id) {
            if team.team_type != TeamType::Engineering {
                return false;
            }
            team.assign(TeamAssignment::RocketDesign {
                rocket_design_id,
                work_phase: crate::engineering_team::WorkPhase::Engineering {
                    progress: 0.0,
                    total_work: crate::engineering_team::DETAILED_ENGINEERING_WORK,
                },
            });
        } else {
            return false;
        }

        // Auto-submit from Specification to Engineering when a team is assigned
        let design = self.rocket_designs[rocket_design_id].head_mut();
        if design.workflow.status.can_edit() {
            design.generate_flaws(&mut self.flaw_generator);
            design.submit_to_engineering();
        }

        true
    }

    /// Assign a team to work on an engine design (engineering teams only)
    pub fn assign_team_to_engine(&mut self, team_id: u32, engine_design_id: usize) -> bool {
        if engine_design_id >= self.engine_designs.len() {
            return false;
        }

        if let Some(team) = self.get_team_mut(team_id) {
            if team.team_type != TeamType::Engineering {
                return false;
            }
            team.assign(TeamAssignment::EngineDesign {
                engine_design_id,
                work_phase: crate::engineering_team::WorkPhase::Engineering {
                    progress: 0.0,
                    total_work: crate::engineering_team::DETAILED_ENGINEERING_WORK,
                },
            });
        } else {
            return false;
        }

        // Auto-submit from Specification to Engineering when a team is assigned
        let design = self.engine_designs[engine_design_id].head_mut();
        if design.workflow.status.can_edit() {
            design.submit_to_engineering(&mut self.flaw_generator, engine_design_id);
        }

        true
    }

    /// Unassign a team from its current work
    pub fn unassign_team(&mut self, team_id: u32) -> bool {
        if let Some(team) = self.get_team_mut(team_id) {
            team.unassign();
            true
        } else {
            false
        }
    }

    /// Get IDs of unassigned teams
    pub fn get_unassigned_team_ids(&self) -> Vec<u32> {
        self.teams
            .iter()
            .filter(|t| t.assignment.is_none())
            .map(|t| t.id)
            .collect()
    }

    /// Get teams working on a specific design
    pub fn get_teams_on_design(&self, rocket_design_id: usize) -> Vec<&EngineeringTeam> {
        self.teams
            .iter()
            .filter(|t| {
                matches!(
                    &t.assignment,
                    Some(TeamAssignment::RocketDesign { rocket_design_id: idx, .. }) if *idx == rocket_design_id
                )
            })
            .collect()
    }

    /// Get teams working on a specific engine design
    pub fn get_teams_on_engine(&self, engine_design_id: usize) -> Vec<&EngineeringTeam> {
        self.teams
            .iter()
            .filter(|t| {
                matches!(
                    &t.assignment,
                    Some(TeamAssignment::EngineDesign { engine_design_id: idx, .. }) if *idx == engine_design_id
                )
            })
            .collect()
    }

    /// Calculate total team efficiency for teams on a design
    pub fn get_design_team_efficiency(&self, rocket_design_id: usize) -> f64 {
        let productive_teams: Vec<_> = self
            .get_teams_on_design(rocket_design_id)
            .into_iter()
            .filter(|t| !t.is_ramping_up())
            .collect();
        team_efficiency(productive_teams.len())
    }

    /// Calculate total team efficiency for teams on an engine design
    pub fn get_engine_team_efficiency(&self, engine_design_id: usize) -> f64 {
        let productive_teams: Vec<_> = self
            .get_teams_on_engine(engine_design_id)
            .into_iter()
            .filter(|t| !t.is_ramping_up())
            .collect();
        team_efficiency(productive_teams.len())
    }

    /// Deduct salaries for all teams
    /// Returns total amount deducted
    pub fn deduct_salaries(&mut self) -> f64 {
        let total_salary: f64 = self.teams.iter().map(|t| t.monthly_salary).sum();
        self.money -= total_salary;
        total_salary
    }

    /// Get total monthly salary for all teams
    pub fn get_total_monthly_salary(&self) -> f64 {
        self.teams.iter().map(|t| t.monthly_salary).sum()
    }

    // ==========================================
    // Manufacturing Management
    // ==========================================

    /// Buy floor space (deducts cost, creates construction order).
    /// Returns true if affordable.
    pub fn buy_floor_space(&mut self, units: usize) -> bool {
        let cost = units as f64 * crate::manufacturing::FLOOR_SPACE_COST_PER_UNIT;
        if self.money < cost {
            return false;
        }
        self.money -= cost;
        self.manufacturing.buy_floor_space(units);
        true
    }

    /// Get IDs of engineering teams only
    pub fn get_engineering_team_ids(&self) -> Vec<u32> {
        self.teams.iter()
            .filter(|t| t.team_type == TeamType::Engineering)
            .map(|t| t.id)
            .collect()
    }

    /// Get IDs of manufacturing teams only
    pub fn get_manufacturing_team_ids(&self) -> Vec<u32> {
        self.teams.iter()
            .filter(|t| t.team_type == TeamType::Manufacturing)
            .map(|t| t.id)
            .collect()
    }

    /// Get total monthly salary for engineering teams
    pub fn get_engineering_monthly_salary(&self) -> f64 {
        self.teams.iter()
            .filter(|t| t.team_type == TeamType::Engineering)
            .map(|t| t.monthly_salary)
            .sum()
    }

    /// Get total monthly salary for manufacturing teams
    pub fn get_manufacturing_monthly_salary(&self) -> f64 {
        self.teams.iter()
            .filter(|t| t.team_type == TeamType::Manufacturing)
            .map(|t| t.monthly_salary)
            .sum()
    }

    /// Start an engine manufacturing order.
    /// Requires a frozen revision of the engine design.
    /// Returns Ok((order_id, total_material_cost)) or Err with a reason string.
    pub fn start_engine_order(
        &mut self,
        engine_design_id: usize,
        revision_number: u32,
        quantity: u32,
    ) -> Result<(ManufacturingOrderId, f64), &'static str> {
        let lineage = self.engine_designs.get(engine_design_id)
            .ok_or("Invalid engine design")?;
        let revision = lineage.get_revision(revision_number)
            .ok_or("Invalid revision")?;
        let snapshot = revision.snapshot.snapshot(engine_design_id, &lineage.name);

        // Check floor space before starting
        let space_needed = crate::manufacturing::floor_space_for_engine(snapshot.scale);
        if !self.manufacturing.can_start_engine_order_with_space(space_needed) {
            return Err("Not enough floor space");
        }

        let result = self.manufacturing.start_engine_order(
            engine_design_id,
            revision_number,
            snapshot,
            quantity,
        ).ok_or("Manufacturing order failed")?;

        // Deduct total material cost up front
        let (_, total_material_cost) = result;
        if self.money < total_material_cost {
            // Can't afford — cancel the order we just created
            self.manufacturing.cancel_order(result.0);
            return Err("Not enough funds for materials");
        }
        self.money -= total_material_cost;
        Ok(result)
    }

    /// Start a rocket assembly order.
    /// Requires a frozen revision. If engines are not in inventory, the order
    /// is created with `waiting_for_engines = true` and the caller should
    /// auto-order the missing engines.
    /// Returns Ok((order_id, material_cost, engines_consumed)) or Err with a reason string.
    pub fn start_rocket_order(
        &mut self,
        rocket_design_id: usize,
        revision_number: u32,
    ) -> Result<(ManufacturingOrderId, f64, bool), &'static str> {
        let lineage = self.rocket_designs.get(rocket_design_id)
            .ok_or("Invalid design")?;
        let revision = lineage.get_revision(revision_number)
            .ok_or("Invalid revision")?;
        let design_snapshot = revision.snapshot.clone();

        // Check floor space
        let space_needed = crate::manufacturing::floor_space_for_rocket(&design_snapshot);
        if !self.manufacturing.can_start_rocket_order_with_space(space_needed) {
            return Err("Not enough floor space");
        }

        // Create the order — initially waiting for engines
        let result = self.manufacturing.start_rocket_order(
            rocket_design_id,
            revision_number,
            design_snapshot.clone(),
            true, // waiting_for_engines
        ).ok_or("Manufacturing order failed")?;

        // Deduct material cost
        let (order_id, material_cost) = result;
        if self.money < material_cost {
            self.manufacturing.cancel_order(order_id);
            return Err("Not enough funds for materials");
        }
        self.money -= material_cost;

        // Try to consume engines immediately
        let engines_consumed = if self.manufacturing.consume_engines_for_rocket(&design_snapshot) {
            self.manufacturing.get_order_mut(order_id).unwrap().waiting_for_engines = false;
            true
        } else {
            false
        };

        Ok((order_id, material_cost, engines_consumed))
    }

    /// Auto-order engines needed for a rocket design.
    /// Accounts for engines already in inventory and pending in active orders.
    /// Cuts revisions as needed and starts engine orders for deficit quantities.
    /// Returns total engines ordered, or Err on failure.
    pub fn auto_order_engines_for_rocket(
        &mut self,
        rocket_design_id: usize,
    ) -> Result<u32, &'static str> {
        let design = self.rocket_designs.get(rocket_design_id)
            .ok_or("Invalid design")?
            .head()
            .clone();

        let mut total_ordered: u32 = 0;

        for (engine_design_id, needed) in design.engines_required() {
            let available = self.manufacturing.get_engines_available(engine_design_id);
            let pending = self.manufacturing.engines_pending_for_design(engine_design_id);
            let deficit = (needed as i32) - (available as i32) - (pending as i32);
            if deficit <= 0 {
                continue;
            }

            if engine_design_id >= self.engine_designs.len() {
                return Err("Invalid engine design");
            }

            // Cut a revision for manufacturing
            let rev = self.engine_designs[engine_design_id].cut_revision("auto-mfg");

            // Start the engine order
            match self.start_engine_order(engine_design_id, rev, deficit as u32) {
                Ok(_) => {
                    total_ordered += deficit as u32;
                }
                Err(reason) => {
                    return Err(reason);
                }
            }
        }

        Ok(total_ordered)
    }

    /// Cancel a manufacturing order by ID.
    pub fn cancel_manufacturing_order(&mut self, order_id: ManufacturingOrderId) -> bool {
        self.manufacturing.cancel_order(order_id)
    }

    /// Increase quantity of an existing engine manufacturing order.
    /// Returns the additional cost on success.
    pub fn increase_engine_order(
        &mut self,
        order_id: ManufacturingOrderId,
        quantity_to_add: u32,
    ) -> Result<f64, &'static str> {
        if quantity_to_add == 0 {
            return Err("Quantity must be greater than 0");
        }
        // Check it's an engine order before checking funds
        let cost_per_unit = match self.manufacturing.get_order(order_id) {
            Some(order) if order.is_engine_order() => order.material_cost_per_unit,
            Some(_) => return Err("Not an engine order"),
            None => return Err("Order not found"),
        };
        let additional_cost = cost_per_unit * quantity_to_add as f64;
        if self.money < additional_cost {
            return Err("Insufficient funds");
        }
        self.manufacturing.increase_engine_order_quantity(order_id, quantity_to_add);
        self.money -= additional_cost;
        Ok(additional_cost)
    }

    /// Assign a team to work on a manufacturing order (manufacturing teams only)
    pub fn assign_team_to_manufacturing(&mut self, team_id: u32, order_id: ManufacturingOrderId) -> bool {
        // Verify order exists and is not waiting for engines
        match self.manufacturing.get_order(order_id) {
            Some(order) if order.waiting_for_engines => return false,
            Some(_) => {},
            None => return false,
        }

        if let Some(team) = self.get_team_mut(team_id) {
            if team.team_type != TeamType::Manufacturing {
                return false;
            }
            team.assign(TeamAssignment::Manufacturing { order_id });
            true
        } else {
            false
        }
    }

    /// Get teams working on a specific manufacturing order
    pub fn get_teams_on_order(&self, order_id: ManufacturingOrderId) -> Vec<&EngineeringTeam> {
        self.teams
            .iter()
            .filter(|t| {
                matches!(
                    &t.assignment,
                    Some(TeamAssignment::Manufacturing { order_id: oid }) if *oid == order_id
                )
            })
            .collect()
    }

    /// Calculate total team efficiency for teams on a manufacturing order
    pub fn get_manufacturing_order_efficiency(&self, order_id: ManufacturingOrderId) -> f64 {
        let productive_teams: Vec<_> = self
            .get_teams_on_order(order_id)
            .into_iter()
            .filter(|t| !t.is_ramping_up())
            .collect();
        manufacturing_team_efficiency(productive_teams.len())
    }

    /// Auto-assign idle manufacturing teams across active orders.
    /// Distributes one team at a time, always picking the order with the lowest
    /// teams_on_order / remaining_work ratio (most understaffed relative to work).
    /// Returns the number of teams assigned.
    pub fn auto_assign_manufacturing_teams(&mut self) -> u32 {
        let mut assigned_count: u32 = 0;

        loop {
            // Find the next idle manufacturing team
            let idle_team_id = self.teams.iter()
                .find(|t| t.team_type == TeamType::Manufacturing && t.assignment.is_none())
                .map(|t| t.id);

            let team_id = match idle_team_id {
                Some(id) => id,
                None => break, // No more idle manufacturing teams
            };

            // Find the order with the lowest teams/remaining_work ratio
            let best_order_id = self.manufacturing.active_orders.iter()
                .filter(|o| !o.is_order_complete())
                .filter(|o| !o.waiting_for_engines)
                .filter(|o| o.remaining_work() > 0.0)
                .map(|o| {
                    let teams_on = self.get_teams_on_order(o.id).len() as f64;
                    let remaining = o.remaining_work();
                    let ratio = teams_on / remaining;
                    (o.id, ratio)
                })
                .min_by(|(id_a, ratio_a), (id_b, ratio_b)| {
                    ratio_a.partial_cmp(ratio_b)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(id_a.cmp(id_b))
                })
                .map(|(id, _)| id);

            match best_order_id {
                Some(order_id) => {
                    self.assign_team_to_manufacturing(team_id, order_id);
                    assigned_count += 1;
                }
                None => break, // No eligible orders
            }
        }

        assigned_count
    }

    // ==========================================
    // Day Processing
    // ==========================================

    /// Process a single day of work
    /// salary_due indicates whether salaries should be deducted this day
    /// (determined by TimeSystem which lives in GameState)
    /// Returns events that occurred
    pub fn process_day(&mut self, salary_due: bool) -> Vec<WorkEvent> {
        let mut events = Vec::new();

        // Process team ramp-up
        for team in &mut self.teams {
            let was_ramping = team.is_ramping_up();
            team.process_day();
            if was_ramping && !team.is_ramping_up() {
                events.push(WorkEvent::TeamRampedUp { team_id: team.id });
            }
        }

        // Check for salary payments
        if salary_due {
            let salary_total = self.deduct_salaries();
            if salary_total > 0.0 {
                events.push(WorkEvent::SalaryDeducted {
                    amount: salary_total,
                });
            }
        }

        // Process work on designs
        let design_events = self.process_design_work();
        events.extend(design_events);

        // Process work on engines
        let engine_events = self.process_engine_work();
        events.extend(engine_events);

        // Process manufacturing work
        let manufacturing_events = self.process_manufacturing_work();
        events.extend(manufacturing_events);

        // Process floor space construction
        let completed_units = self.manufacturing.process_construction();
        if completed_units > 0 {
            events.push(WorkEvent::FloorSpaceCompleted { units: completed_units });
        }

        // Auto-assign idle manufacturing teams if toggled on
        if self.auto_assign_manufacturing {
            self.auto_assign_manufacturing_teams();
        }

        events
    }

    /// Shared workflow tick: advance work, discover flaws, auto-start flaw fixing.
    /// Returns events generated during this tick.
    fn process_workflow_tick(
        workflow: &mut crate::design_workflow::DesignWorkflow,
        efficiency: f64,
        design_kind: &'static str,
        design_id: usize,
    ) -> Vec<WorkEvent> {
        use crate::design_workflow::DesignStatus;

        let mut events = Vec::new();

        if !workflow.status.is_working() {
            return events;
        }

        let phase_before = workflow.status.name();
        let is_testing = matches!(workflow.status, DesignStatus::Testing { .. });
        let is_fixing = matches!(workflow.status, DesignStatus::Fixing { .. });

        // Advance work
        let phase_completed = workflow.advance_work(efficiency);

        if phase_completed {
            if is_fixing {
                if let Some(flaw_name) = workflow.complete_flaw_fix() {
                    events.push(WorkEvent::FlawFixed {
                        design_kind,
                        design_id,
                        flaw_name,
                    });
                }
            } else if !is_testing {
                // Engineering phase completed (testing cycle completions are silent)
                events.push(WorkEvent::DesignPhaseComplete {
                    design_kind,
                    design_id,
                    phase_name: phase_before.to_string(),
                });
            }
        }

        // Track cumulative testing work every day
        if is_testing {
            workflow.testing_work_completed += efficiency;
        }

        // Discover flaws only when a testing cycle completes
        if phase_completed && is_testing {
            for flaw_name in workflow.discover_flaws_on_cycle_complete() {
                events.push(WorkEvent::FlawDiscovered {
                    design_kind,
                    design_id,
                    flaw_name,
                });
            }
        }

        // After Testing or completing a fix, check if there are unfixed flaws to work on
        let now_testing = matches!(workflow.status, DesignStatus::Testing { .. });
        if now_testing {
            if let Some(flaw_index) = workflow.get_next_unfixed_flaw() {
                let flaw_name = workflow.active_flaws[flaw_index].name.clone();
                workflow.start_fixing_flaw(flaw_index);
                events.push(WorkEvent::DesignPhaseComplete {
                    design_kind,
                    design_id,
                    phase_name: format!("Started fixing: {}", flaw_name),
                });
            }
        }

        events
    }

    /// Process work progress on all rocket designs
    fn process_design_work(&mut self) -> Vec<WorkEvent> {
        let mut events = Vec::new();

        let design_efficiencies: Vec<(usize, f64)> = (0..self.rocket_designs.len())
            .map(|idx| (idx, self.get_design_team_efficiency(idx)))
            .filter(|(_, eff)| *eff > 0.0)
            .collect();

        for (design_id, efficiency) in design_efficiencies {
            let workflow = &mut self.rocket_designs[design_id].head_mut().workflow;
            events.extend(Self::process_workflow_tick(workflow, efficiency, "rocket", design_id));
        }

        // Auto-unassign teams from completed designs
        self.auto_unassign_completed_designs();

        events
    }

    /// Unassign all teams from designs that are Complete
    fn auto_unassign_completed_designs(&mut self) {
        use crate::design_workflow::DesignStatus;

        let completed_indices: Vec<usize> = self.rocket_designs
            .iter()
            .enumerate()
            .filter(|(_, l)| matches!(l.head().workflow.status, DesignStatus::Complete))
            .map(|(i, _)| i)
            .collect();

        for team in &mut self.teams {
            if let Some(TeamAssignment::RocketDesign { rocket_design_id, .. }) = &team.assignment {
                if completed_indices.contains(rocket_design_id) {
                    team.unassign();
                }
            }
        }
    }

    /// Process work progress on all engine designs
    fn process_engine_work(&mut self) -> Vec<WorkEvent> {
        let mut events = Vec::new();

        let engine_efficiencies: Vec<(usize, f64)> = (0..self.engine_designs.len())
            .map(|idx| (idx, self.get_engine_team_efficiency(idx)))
            .filter(|(_, eff)| *eff > 0.0)
            .collect();

        for (design_id, efficiency) in engine_efficiencies {
            let workflow = &mut self.engine_designs[design_id].head_mut().workflow;
            events.extend(Self::process_workflow_tick(workflow, efficiency, "engine", design_id));
        }

        events
    }

    /// Process work progress on all manufacturing orders
    fn process_manufacturing_work(&mut self) -> Vec<WorkEvent> {
        let mut events = Vec::new();

        // Try to unblock rocket orders waiting for engines
        let unblocked_ids = self.manufacturing.try_unblock_rocket_orders();
        for order_id in unblocked_ids {
            events.push(WorkEvent::RocketOrderUnblocked { order_id });
        }

        // Calculate efficiency for each active order (skip blocked ones)
        let order_efficiencies: Vec<(ManufacturingOrderId, f64)> = self.manufacturing.active_orders
            .iter()
            .filter(|o| !o.waiting_for_engines)
            .map(|o| (o.id, self.get_manufacturing_order_efficiency(o.id)))
            .filter(|(_, eff)| *eff > 0.0)
            .collect();

        // Process work for each order with teams assigned
        for (order_id, efficiency) in order_efficiencies {
            let order = match self.manufacturing.get_order_mut(order_id) {
                Some(o) => o,
                None => continue,
            };

            order.progress += efficiency;

            if order.is_unit_complete() {
                match &mut order.order_type {
                    ManufacturingOrderType::Engine {
                        engine_design_id,
                        revision_number,
                        snapshot,
                        quantity,
                        completed,
                    } => {
                        *completed += 1;
                        let eid = *engine_design_id;
                        let rev = *revision_number;
                        let snap = snapshot.clone();
                        let comp = *completed;
                        let qty = *quantity;
                        let oid = order_id;

                        // Add completed engine to inventory
                        self.manufacturing.add_engine_to_inventory(eid, rev, snap);

                        events.push(WorkEvent::EngineManufactured {
                            engine_design_id: eid,
                            revision_number: rev,
                            order_id: oid,
                        });

                        if comp >= qty {
                            events.push(WorkEvent::ManufacturingOrderComplete {
                                order_id: oid,
                            });
                        } else {
                            // Reset progress for next unit
                            let order = self.manufacturing.get_order_mut(oid).unwrap();
                            order.progress = 0.0;
                        }
                    }
                    ManufacturingOrderType::Rocket {
                        rocket_design_id,
                        revision_number,
                        design_snapshot,
                    } => {
                        let rid = *rocket_design_id;
                        let rev = *revision_number;
                        let snap = design_snapshot.clone();
                        let oid = order_id;

                        // Add completed rocket to inventory
                        self.manufacturing.add_rocket_to_inventory(rid, rev, snap);

                        let serial = self.manufacturing.rocket_inventory.last()
                            .map(|r| r.serial_number)
                            .unwrap_or(0);

                        events.push(WorkEvent::RocketAssembled {
                            rocket_design_id: rid,
                            revision_number: rev,
                            serial_number: serial,
                        });

                        events.push(WorkEvent::ManufacturingOrderComplete {
                            order_id: oid,
                        });
                    }
                }
            }
        }

        // Remove completed orders
        self.manufacturing.active_orders.retain(|o| !o.is_order_complete());

        // Auto-unassign teams from completed orders
        let active_order_ids: Vec<ManufacturingOrderId> = self.manufacturing.active_orders
            .iter()
            .map(|o| o.id)
            .collect();

        for team in &mut self.teams {
            if let Some(TeamAssignment::Manufacturing { order_id }) = &team.assignment {
                if !active_order_ids.contains(order_id) {
                    team.unassign();
                }
            }
        }

        events
    }
}

impl Default for Company {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_design::FuelType;
    use crate::engineering_team::TeamType;

    #[test]
    fn test_create_engine_design() {
        let mut company = Company::new();
        let initial_count = company.engine_designs.len();
        let idx = company.create_engine_design(FuelType::Kerolox, 1.5);
        assert_eq!(company.engine_designs.len(), initial_count + 1);
        let snap = company.engine_designs[idx].head().snapshot(idx, &company.engine_designs[idx].name);
        assert_eq!(snap.thrust_kn, 500.0 * 1.5);
    }

    #[test]
    fn test_duplicate_engine_design() {
        let mut company = Company::new();
        let new_idx = company.duplicate_engine_design(0).unwrap();
        assert!(company.engine_designs[new_idx].name.contains("Copy"));
        // Duplicate should be untested and have no flaws
        assert!(company.engine_designs[new_idx].head().can_modify());
        assert!(!company.engine_designs[new_idx].head().workflow.flaws_generated);
    }

    #[test]
    fn test_delete_engine_design() {
        let mut company = Company::new();
        let count = company.engine_designs.len();
        assert!(company.delete_engine_design(0));
        assert_eq!(company.engine_designs.len(), count - 1);
    }

    #[test]
    fn test_cannot_delete_last_engine() {
        let mut company = Company::new();
        // Delete down to 1
        while company.engine_designs.len() > 1 {
            company.delete_engine_design(0);
        }
        assert!(!company.delete_engine_design(0));
        assert_eq!(company.engine_designs.len(), 1);
    }

    #[test]
    fn test_rename_engine_design() {
        let mut company = Company::new();
        assert!(company.rename_engine_design(0, "Custom Name"));
        assert_eq!(company.engine_designs[0].name, "Custom Name");
    }

    #[test]
    fn test_set_engine_design_scale() {
        let mut company = Company::new();
        assert!(company.set_engine_design_scale(0, 2.5));
        assert_eq!(company.engine_designs[0].head().scale, 2.5);
    }

    #[test]
    fn test_set_engine_design_fuel_type() {
        let mut company = Company::new();
        assert!(company.set_engine_design_fuel_type(0, FuelType::Solid));
        assert_eq!(company.engine_designs[0].head().fuel_type(), FuelType::Solid);
    }

    #[test]
    fn test_starting_team_is_engineering() {
        let company = Company::new();
        assert_eq!(company.teams.len(), 1);
        assert_eq!(company.teams[0].team_type, TeamType::Engineering);
    }

    #[test]
    fn test_hire_engineering_team() {
        let mut company = Company::new();
        let initial_money = company.money;
        let result = company.hire_engineering_team();
        assert!(result.is_some());
        assert_eq!(company.teams.len(), 2);
        assert_eq!(company.teams[1].team_type, TeamType::Engineering);
        assert!((company.money - (initial_money - ENGINEERING_HIRE_COST)).abs() < 1.0);
    }

    #[test]
    fn test_hire_manufacturing_team() {
        let mut company = Company::new();
        let initial_money = company.money;
        let result = company.hire_manufacturing_team();
        assert!(result.is_some());
        assert_eq!(company.teams.len(), 2);
        assert_eq!(company.teams[1].team_type, TeamType::Manufacturing);
        assert!((company.money - (initial_money - MANUFACTURING_HIRE_COST)).abs() < 1.0);
    }

    #[test]
    fn test_hiring_costs() {
        assert!((ENGINEERING_HIRE_COST - 150_000.0).abs() < 1.0);
        // Manufacturing hire = 3× manufacturing salary ($300K) = $900K
        assert!((MANUFACTURING_HIRE_COST - 900_000.0).abs() < 1.0);
    }

    #[test]
    fn test_engineering_team_cannot_do_manufacturing() {
        let mut company = Company::new();
        // Starting team is engineering
        let eng_team_id = company.teams[0].id;

        // Hire a manufacturing team and start an order so we have an order to assign to
        company.hire_manufacturing_team();

        // Create a simple engine order (need a frozen revision)
        let idx = company.engine_designs.len() - 1;
        company.engine_designs[idx].cut_revision("v1");
        let order_result = company.start_engine_order(idx, 1, 1);
        if let Ok((order_id, _)) = order_result {
            // Engineering team should fail to be assigned to manufacturing
            assert!(!company.assign_team_to_manufacturing(eng_team_id, order_id));
        }
    }

    #[test]
    fn test_manufacturing_team_cannot_do_design() {
        let mut company = Company::new();
        company.hire_manufacturing_team();
        let mfg_team_id = company.teams[1].id;

        // Manufacturing team should fail to be assigned to rocket design
        assert!(!company.assign_team_to_design(mfg_team_id, 0));

        // Manufacturing team should fail to be assigned to engine design
        assert!(!company.assign_team_to_engine(mfg_team_id, 0));
    }

    #[test]
    fn test_team_type_queries() {
        let mut company = Company::new();
        // Start with 1 engineering team
        assert_eq!(company.get_engineering_team_ids().len(), 1);
        assert_eq!(company.get_manufacturing_team_ids().len(), 0);

        // Hire a manufacturing team
        company.hire_manufacturing_team();
        assert_eq!(company.get_engineering_team_ids().len(), 1);
        assert_eq!(company.get_manufacturing_team_ids().len(), 1);

        // Hire another engineering team
        company.hire_engineering_team();
        assert_eq!(company.get_engineering_team_ids().len(), 2);
        assert_eq!(company.get_manufacturing_team_ids().len(), 1);
    }

    #[test]
    fn test_buy_floor_space() {
        let mut company = Company::new();
        let initial_space = company.manufacturing.floor_space_total;
        let initial_money = company.money;

        assert!(company.buy_floor_space(3));
        // Money deducted
        let expected_cost = 3.0 * crate::manufacturing::FLOOR_SPACE_COST_PER_UNIT;
        assert!((company.money - (initial_money - expected_cost)).abs() < 1.0);
        // Space not yet added (under construction)
        assert_eq!(company.manufacturing.floor_space_total, initial_space);
        assert_eq!(company.manufacturing.floor_space_constructing(), 3);
    }

    #[test]
    fn test_floor_space_completes_in_process_day() {
        let mut company = Company::new();
        company.buy_floor_space(2);
        let initial_space = company.manufacturing.floor_space_total;

        // Process 29 days - no completion
        for _ in 0..29 {
            let events = company.process_day(false);
            assert!(!events.iter().any(|e| matches!(e, WorkEvent::FloorSpaceCompleted { .. })));
        }

        // Day 30 - should complete
        let events = company.process_day(false);
        let floor_event = events.iter().find(|e| matches!(e, WorkEvent::FloorSpaceCompleted { .. }));
        assert!(floor_event.is_some());
        if let Some(WorkEvent::FloorSpaceCompleted { units }) = floor_event {
            assert_eq!(*units, 2);
        }
        assert_eq!(company.manufacturing.floor_space_total, initial_space + 2);
    }

    #[test]
    fn test_update_rocket_design_propagates_name() {
        let mut company = Company::new();
        let original_name = company.rocket_designs[0].name.clone();
        assert_eq!(original_name, "Default Rocket");

        // Load the design, change its name, and update
        let mut design = company.load_rocket_design(0).unwrap();
        design.name = "My Custom Rocket".to_string();
        assert!(company.update_rocket_design(0, design));

        // Both lineage and head should have the new name
        assert_eq!(company.rocket_designs[0].name, "My Custom Rocket");
        assert_eq!(company.rocket_designs[0].head().name, "My Custom Rocket");
    }

    // ==========================================
    // Auto-Assign Manufacturing Teams Tests
    // ==========================================

    #[test]
    fn test_auto_assign_no_idle_teams() {
        let mut company = Company::new();
        // No manufacturing teams at all
        let assigned = company.auto_assign_manufacturing_teams();
        assert_eq!(assigned, 0);
    }

    #[test]
    fn test_auto_assign_no_orders() {
        let mut company = Company::new();
        company.hire_manufacturing_team();
        // No manufacturing orders
        let assigned = company.auto_assign_manufacturing_teams();
        assert_eq!(assigned, 0);
    }

    #[test]
    fn test_auto_assign_one_order_two_teams() {
        let mut company = Company::new();
        company.hire_manufacturing_team();
        company.hire_manufacturing_team();

        // Create an engine order (need a frozen revision)
        let idx = company.engine_designs.len() - 1;
        company.engine_designs[idx].cut_revision("v1");
        let rev = company.engine_designs[idx].revisions.len() as u32;
        let result = company.start_engine_order(idx, rev, 3);
        assert!(result.is_ok(), "Failed to start engine order: {:?}", result);

        let assigned = company.auto_assign_manufacturing_teams();
        assert_eq!(assigned, 2);

        // Both teams should be on the order
        let order_id = company.manufacturing.active_orders[0].id;
        let teams_on = company.get_teams_on_order(order_id);
        assert_eq!(teams_on.len(), 2);
    }

    #[test]
    fn test_auto_assign_two_orders_distributed() {
        let mut company = Company::new();
        company.hire_manufacturing_team();
        company.hire_manufacturing_team();

        // Create two engine orders
        let idx = company.engine_designs.len() - 1;
        company.engine_designs[idx].cut_revision("v1");
        let rev = company.engine_designs[idx].revisions.len() as u32;

        let result1 = company.start_engine_order(idx, rev, 3);
        assert!(result1.is_ok());
        let result2 = company.start_engine_order(idx, rev, 3);
        assert!(result2.is_ok());

        let assigned = company.auto_assign_manufacturing_teams();
        assert_eq!(assigned, 2);

        // Each order should get one team (both have same remaining work)
        let order1_id = company.manufacturing.active_orders[0].id;
        let order2_id = company.manufacturing.active_orders[1].id;
        assert_eq!(company.get_teams_on_order(order1_id).len(), 1);
        assert_eq!(company.get_teams_on_order(order2_id).len(), 1);
    }

    // ==========================================
    // Queued Rocket Order Tests
    // ==========================================

    /// Helper: set up a company with a frozen rocket revision and engine revisions
    fn company_with_rocket_revision() -> (Company, usize, u32) {
        let mut company = Company::new();
        // Cut revisions for the default engine designs so we can order them
        for i in 0..company.engine_designs.len() {
            company.engine_designs[i].cut_revision("v1");
        }
        // Cut a revision for the default rocket design
        company.rocket_designs[0].cut_revision("v1");
        let rev = company.rocket_designs[0].revisions.len() as u32;
        (company, 0, rev)
    }

    #[test]
    fn test_start_rocket_order_without_engines_queues() {
        let (mut company, design_id, rev) = company_with_rocket_revision();

        // No engines in inventory — order should succeed with waiting_for_engines = true
        let result = company.start_rocket_order(design_id, rev);
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        let (order_id, _cost, engines_consumed) = result.unwrap();
        assert!(!engines_consumed);

        let order = company.manufacturing.get_order(order_id).unwrap();
        assert!(order.waiting_for_engines);
    }

    #[test]
    fn test_start_rocket_order_with_engines_consumes() {
        let (mut company, design_id, rev) = company_with_rocket_revision();

        // Stock engines: 5 kerolox (id=1) + 1 hydrolox (id=0)
        let kerolox_snap = crate::engine_design::default_snapshot(1);
        let hydrolox_snap = crate::engine_design::default_snapshot(0);
        for _ in 0..5 {
            company.manufacturing.add_engine_to_inventory(1, 1, kerolox_snap.clone());
        }
        company.manufacturing.add_engine_to_inventory(0, 1, hydrolox_snap.clone());

        let result = company.start_rocket_order(design_id, rev);
        assert!(result.is_ok());
        let (order_id, _cost, engines_consumed) = result.unwrap();
        assert!(engines_consumed);

        let order = company.manufacturing.get_order(order_id).unwrap();
        assert!(!order.waiting_for_engines);

        // Engines should be consumed
        assert_eq!(company.manufacturing.get_engines_available(1), 0);
        assert_eq!(company.manufacturing.get_engines_available(0), 0);
    }

    #[test]
    fn test_assign_team_to_blocked_order_fails() {
        let (mut company, design_id, rev) = company_with_rocket_revision();
        company.hire_manufacturing_team();
        let mfg_team_id = company.get_manufacturing_team_ids()[0];

        // Create a blocked rocket order
        let (order_id, _, _) = company.start_rocket_order(design_id, rev).unwrap();

        // Should not be able to assign team
        assert!(!company.assign_team_to_manufacturing(mfg_team_id, order_id));
    }

    #[test]
    fn test_auto_assign_skips_blocked_orders() {
        let (mut company, design_id, rev) = company_with_rocket_revision();
        company.hire_manufacturing_team();

        // Create a blocked rocket order
        let (order_id, _, _) = company.start_rocket_order(design_id, rev).unwrap();

        // Auto-assign should not assign to blocked order
        let assigned = company.auto_assign_manufacturing_teams();
        assert_eq!(assigned, 0);
        assert_eq!(company.get_teams_on_order(order_id).len(), 0);
    }

    #[test]
    fn test_engine_manufactured_unblocks_rocket() {
        let (mut company, design_id, rev) = company_with_rocket_revision();

        // Create a blocked rocket order
        let (order_id, _, engines_consumed) = company.start_rocket_order(design_id, rev).unwrap();
        assert!(!engines_consumed);
        assert!(company.manufacturing.get_order(order_id).unwrap().waiting_for_engines);

        // Add engines to inventory (simulating completed manufacturing)
        let kerolox_snap = crate::engine_design::default_snapshot(1);
        let hydrolox_snap = crate::engine_design::default_snapshot(0);
        for _ in 0..5 {
            company.manufacturing.add_engine_to_inventory(1, 1, kerolox_snap.clone());
        }
        company.manufacturing.add_engine_to_inventory(0, 1, hydrolox_snap.clone());

        // process_manufacturing_work should unblock the order
        let events = company.process_manufacturing_work();
        let unblock_events: Vec<_> = events.iter()
            .filter(|e| matches!(e, WorkEvent::RocketOrderUnblocked { .. }))
            .collect();
        assert_eq!(unblock_events.len(), 1);

        // Order should now be unblocked
        assert!(!company.manufacturing.get_order(order_id).unwrap().waiting_for_engines);

        // Engines should be consumed
        assert_eq!(company.manufacturing.get_engines_available(1), 0);
        assert_eq!(company.manufacturing.get_engines_available(0), 0);
    }

    #[test]
    fn test_increase_engine_order_deducts_cost() {
        let mut company = Company::new();
        // Start an engine order
        let rev = company.engine_designs[1].cut_revision("mfg");
        let (order_id, initial_cost) = company.start_engine_order(1, rev, 1).unwrap();
        let money_after_start = company.money;

        let cost_per_unit = initial_cost; // quantity=1
        let result = company.increase_engine_order(order_id, 3);
        assert!(result.is_ok());
        let additional_cost = result.unwrap();
        assert!((additional_cost - cost_per_unit * 3.0).abs() < 0.01);
        assert!((company.money - (money_after_start - additional_cost)).abs() < 0.01);
    }

    #[test]
    fn test_increase_engine_order_insufficient_funds() {
        let mut company = Company::new();
        let rev = company.engine_designs[1].cut_revision("mfg");
        let (order_id, _) = company.start_engine_order(1, rev, 1).unwrap();

        // Drain funds
        company.money = 0.0;

        let result = company.increase_engine_order(order_id, 1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Insufficient funds");
    }
}
