use crate::contract::{Contract, Destination};
use crate::design_lineage::DesignLineage;
use crate::engine::costs;
use crate::engine_design::{default_engine_lineages, create_engine, EngineDesign, FuelType};
use crate::engineering_team::{team_efficiency, EngineeringTeam, TeamAssignment, WorkEvent};
use crate::flaw::FlawGenerator;
use crate::launch_site::LaunchSite;
use crate::rocket_design::RocketDesign;
use rand::Rng;

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
        };

        // Generate initial contracts
        company.generate_contracts(CONTRACTS_TO_SHOW);

        // Start with one engineering team
        company.hire_team();

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
            let name = lineage.head().name.clone();
            *lineage.head_mut() = design;
            lineage.head_mut().name = name;
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
            // Reset to untested so the copy can be modified
            new_engine.status = crate::engine::EngineStatus::Untested;
            new_engine.active_flaws.clear();
            new_engine.fixed_flaws.clear();
            new_engine.flaws_generated = false;
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

    /// Hire a new engineering team
    /// Returns the team ID
    pub fn hire_team(&mut self) -> u32 {
        let team = EngineeringTeam::new(self.next_team_id);
        let id = team.id;
        self.teams.push(team);
        self.next_team_id += 1;
        id
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

    /// Assign a team to work on a rocket design
    pub fn assign_team_to_design(&mut self, team_id: u32, rocket_design_id: usize) -> bool {
        if rocket_design_id >= self.rocket_designs.len() {
            return false;
        }

        if let Some(team) = self.get_team_mut(team_id) {
            team.assign(TeamAssignment::RocketDesign {
                rocket_design_id,
                work_phase: crate::engineering_team::DesignWorkPhase::DetailedEngineering {
                    progress: 0.0,
                    total_work: crate::engineering_team::DETAILED_ENGINEERING_WORK,
                },
            });
            true
        } else {
            false
        }
    }

    /// Assign a team to work on an engine design
    pub fn assign_team_to_engine(&mut self, team_id: u32, engine_design_id: usize) -> bool {
        if let Some(team) = self.get_team_mut(team_id) {
            team.assign(TeamAssignment::EngineDesign {
                engine_design_id,
                work_phase: crate::engineering_team::EngineWorkPhase::Refining {
                    progress: 0.0,
                    total_work: crate::engineering_team::ENGINE_REFINING_WORK,
                },
            });
            true
        } else {
            false
        }
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

        events
    }

    /// Process work progress on all designs
    fn process_design_work(&mut self) -> Vec<WorkEvent> {
        use crate::rocket_design::DesignStatus;

        let mut events = Vec::new();

        // Calculate efficiency for each design being worked on
        let design_efficiencies: Vec<(usize, f64)> = (0..self.rocket_designs.len())
            .map(|idx| (idx, self.get_design_team_efficiency(idx)))
            .filter(|(_, eff)| *eff > 0.0)
            .collect();

        // Process work for each design with teams assigned
        for (rocket_design_id, efficiency) in design_efficiencies {
            let design = self.rocket_designs[rocket_design_id].head_mut();

            // Skip designs not in a work phase
            if !design.design_status.is_working() {
                continue;
            }

            let phase_before = design.design_status.name();
            let is_refining = matches!(design.design_status, DesignStatus::Refining { .. });
            let is_fixing = matches!(design.design_status, DesignStatus::Fixing { .. });

            // Advance work
            let phase_completed = design.advance_work(efficiency);

            if phase_completed {
                if is_fixing {
                    // Fixing complete - mark flaw as fixed and return to Refining
                    if let Some(flaw_name) = design.complete_flaw_fix() {
                        events.push(WorkEvent::DesignFlawFixed {
                            rocket_design_id,
                            flaw_name,
                        });
                    }
                } else {
                    // Engineering phase completed
                    events.push(WorkEvent::DesignPhaseComplete {
                        rocket_design_id,
                        phase_name: phase_before.to_string(),
                    });
                }
            }

            // Only discover flaws during Refining (not during Fixing)
            if is_refining {
                // Check each undiscovered flaw using its individual discovery probability
                // Divide by 30 to convert from per-test to per-day probability (roughly monthly)
                let mut rng = rand::thread_rng();
                for flaw in design.active_flaws.iter_mut() {
                    if !flaw.discovered && !flaw.fixed {
                        let daily_discovery_chance = flaw.discovery_probability() / 30.0;
                        let roll = rng.gen::<f64>();
                        if roll < daily_discovery_chance {
                            flaw.discovered = true;
                            events.push(WorkEvent::DesignFlawDiscovered {
                                rocket_design_id,
                                flaw_name: flaw.name.clone(),
                            });
                        }
                    }
                }
            }

            // After Refining or completing a fix, check if there are unfixed flaws to work on
            let now_refining = matches!(design.design_status, DesignStatus::Refining { .. });
            if now_refining {
                if let Some(flaw_index) = design.get_next_unfixed_flaw() {
                    let flaw_name = design.active_flaws[flaw_index].name.clone();
                    design.start_fixing_flaw(flaw_index);
                    events.push(WorkEvent::DesignPhaseComplete {
                        rocket_design_id,
                        phase_name: format!("Started fixing: {}", flaw_name),
                    });
                }
            }
        }

        // Auto-unassign teams from completed designs
        self.auto_unassign_completed_designs();

        events
    }

    /// Unassign all teams from designs that are Complete
    fn auto_unassign_completed_designs(&mut self) {
        use crate::rocket_design::DesignStatus;

        // Collect completed design indices
        let completed_indices: Vec<usize> = self.rocket_designs
            .iter()
            .enumerate()
            .filter(|(_, l)| matches!(l.head().design_status, DesignStatus::Complete))
            .map(|(i, _)| i)
            .collect();

        // Unassign teams working on completed designs
        for team in &mut self.teams {
            if let Some(TeamAssignment::RocketDesign { rocket_design_id, .. }) = &team.assignment {
                if completed_indices.contains(rocket_design_id) {
                    team.unassign();
                }
            }
        }
    }

    /// Process work progress on all engines
    fn process_engine_work(&mut self) -> Vec<WorkEvent> {
        use crate::engine::EngineStatus;

        let mut events = Vec::new();

        // Get all engine design indices that have teams working on them
        let engine_efficiencies: Vec<(usize, f64)> = (0..self.engine_designs.len())
            .map(|idx| (idx, self.get_engine_team_efficiency(idx)))
            .filter(|(_, eff)| *eff > 0.0)
            .collect();

        // Process work for each engine with teams assigned
        for (engine_design_id, efficiency) in engine_efficiencies {
            let design = self.engine_designs[engine_design_id].head_mut();

            // Skip engines not in a work phase
            if !design.status.is_working() {
                continue;
            }

            let is_refining = matches!(design.status, EngineStatus::Refining { .. });
            let is_fixing = matches!(design.status, EngineStatus::Fixing { .. });

            // Handle Fixing phase
            if is_fixing {
                if let EngineStatus::Fixing { flaw_index, progress, total, .. } = &mut design.status {
                    *progress += efficiency;
                    if *progress >= *total {
                        let flaw_index_copy = *flaw_index;
                        // Fix complete - mark flaw as fixed and return to Refining
                        if let Some(flaw_name) = design.fix_flaw_by_index(flaw_index_copy) {
                            events.push(WorkEvent::EngineFlawFixed {
                                engine_design_id,
                                flaw_name,
                            });
                        }
                        design.status.return_to_refining();
                    }
                }
            }

            // Handle Refining phase - discover flaws
            if is_refining {
                // Check each undiscovered flaw using its individual discovery probability
                // Divide by 30 to convert from per-test to per-day probability
                let mut rng = rand::thread_rng();
                for flaw in design.active_flaws.iter_mut() {
                    if !flaw.discovered && !flaw.fixed {
                        let daily_discovery_chance = flaw.discovery_probability() / 30.0;
                        let roll = rng.gen::<f64>();
                        if roll < daily_discovery_chance {
                            flaw.discovered = true;
                            events.push(WorkEvent::EngineFlawDiscovered {
                                engine_design_id,
                                flaw_name: flaw.name.clone(),
                            });
                        }
                    }
                }
            }

            // After Refining or completing a fix, check if there are unfixed flaws to work on
            let now_refining = matches!(design.status, EngineStatus::Refining { .. });
            if now_refining {
                if let Some(flaw_index) = design.get_next_unfixed_flaw() {
                    let flaw_name = design.active_flaws[flaw_index].name.clone();
                    design.status.start_fixing(flaw_name.clone(), flaw_index);
                    events.push(WorkEvent::DesignPhaseComplete {
                        rocket_design_id: engine_design_id,
                        phase_name: format!("Started fixing: {}", flaw_name),
                    });
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
        assert!(!company.engine_designs[new_idx].head().flaws_generated);
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
}
