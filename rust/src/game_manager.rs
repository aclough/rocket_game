use godot::prelude::*;

use crate::contract::{format_money, Destination};
use crate::game_state::{GameState, CONTRACT_REFRESH_COST};
use crate::player_finance::PlayerFinance;
use crate::rocket_designer::RocketDesigner;

/// Godot node for managing overall game state
#[derive(GodotClass)]
#[class(base=Node)]
pub struct GameManager {
    base: Base<Node>,
    state: GameState,
    /// Index of the saved design currently being edited (if any)
    current_rocket_design_id: Option<usize>,
    /// Player finances - single source of truth for money
    finance: Gd<PlayerFinance>,
    /// Error message from the last failed manufacturing order
    last_order_error: String,
}

#[godot_api]
impl INode for GameManager {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            state: GameState::new(),
            current_rocket_design_id: None,
            finance: Gd::from_init_fn(PlayerFinance::init),
            last_order_error: String::new(),
        }
    }

    fn ready(&mut self) {
        // Signal forwarding removed - we emit money_changed directly from GameManager
        // to avoid re-entrancy issues with Gd<T>::bind_mut()
    }
}

#[godot_api]
impl GameManager {
    // ==========================================
    // Signals
    // ==========================================

    #[signal]
    fn money_changed(new_amount: f64);

    #[signal]
    fn contracts_changed();

    #[signal]
    fn contract_selected(contract_id: i32);

    #[signal]
    fn contract_completed(reward: f64);

    #[signal]
    fn contract_failed();

    #[signal]
    fn designs_changed();

    #[signal]
    fn date_changed(new_day: i32);

    #[signal]
    fn fame_changed(new_fame: f64);

    #[signal]
    fn time_paused();

    #[signal]
    fn time_resumed();

    #[signal]
    fn work_event_occurred(event_type: GString, data: Dictionary);

    #[signal]
    fn teams_changed();

    #[signal]
    fn manufacturing_changed();

    #[signal]
    fn inventory_changed();

    // ==========================================
    // Money and Budget
    // ==========================================

    /// Emit money_changed signal with current amount
    fn emit_money_changed(&mut self) {
        let amount = self.finance.bind().get_money();
        self.base_mut()
            .emit_signal("money_changed", &[Variant::from(amount)]);
    }

    /// Get the PlayerFinance resource (single source of truth for money)
    #[func]
    pub fn get_finance(&self) -> Gd<PlayerFinance> {
        self.finance.clone()
    }

    /// Sync money from PlayerFinance to GameState (call before GameState operations)
    fn sync_money_to_state(&mut self) {
        self.state.player_company.money = self.finance.bind().get_money();
    }

    /// Sync money from GameState to PlayerFinance (call after GameState operations that modify money)
    fn sync_money_from_state(&mut self) {
        self.finance.bind_mut().set_money(self.state.player_company.money);
    }

    /// Get current money
    #[func]
    pub fn get_money(&self) -> f64 {
        self.finance.bind().get_money()
    }

    /// Get money formatted for display (e.g., "$500M")
    #[func]
    pub fn get_money_formatted(&self) -> GString {
        GString::from(format_money(self.finance.bind().get_money()).as_str())
    }

    /// Get the current turn number
    #[func]
    pub fn get_turn(&self) -> i32 {
        self.state.turn as i32
    }

    /// Get total launches
    #[func]
    pub fn get_total_launches(&self) -> i32 {
        self.state.player_company.total_launches as i32
    }

    /// Get successful launches
    #[func]
    pub fn get_successful_launches(&self) -> i32 {
        self.state.player_company.successful_launches as i32
    }

    /// Get success rate as percentage
    #[func]
    pub fn get_success_rate(&self) -> f64 {
        self.state.player_company.success_rate()
    }

    /// Check if player is bankrupt
    #[func]
    pub fn is_bankrupt(&self) -> bool {
        self.state.player_company.is_bankrupt()
    }

    // ==========================================
    // Contract Management
    // ==========================================

    /// Get the number of available contracts
    #[func]
    pub fn get_contract_count(&self) -> i32 {
        self.state.player_company.available_contracts.len() as i32
    }

    /// Get contract ID at index
    #[func]
    pub fn get_contract_id(&self, index: i32) -> i32 {
        self.state
            .player_company
            .available_contracts
            .get(index as usize)
            .map(|c| c.id as i32)
            .unwrap_or(-1)
    }

    /// Get contract name at index
    #[func]
    pub fn get_contract_name(&self, index: i32) -> GString {
        GString::from(
            self.state
                .player_company
                .available_contracts
                .get(index as usize)
                .map(|c| c.name.as_str())
                .unwrap_or("")
        )
    }

    /// Get contract description at index
    #[func]
    pub fn get_contract_description(&self, index: i32) -> GString {
        GString::from(
            self.state
                .player_company
                .available_contracts
                .get(index as usize)
                .map(|c| c.description.as_str())
                .unwrap_or("")
        )
    }

    /// Get contract destination name at index
    #[func]
    pub fn get_contract_destination(&self, index: i32) -> GString {
        GString::from(
            self.state
                .player_company
                .available_contracts
                .get(index as usize)
                .map(|c| c.destination.display_name())
                .unwrap_or("")
        )
    }

    /// Get contract destination short code at index
    #[func]
    pub fn get_contract_destination_short(&self, index: i32) -> GString {
        GString::from(
            self.state
                .player_company
                .available_contracts
                .get(index as usize)
                .map(|c| c.destination.short_name())
                .unwrap_or("")
        )
    }

    /// Get contract required delta-v at index
    #[func]
    pub fn get_contract_delta_v(&self, index: i32) -> f64 {
        self.state
            .player_company
            .available_contracts
            .get(index as usize)
            .map(|c| c.destination.required_delta_v())
            .unwrap_or(0.0)
    }

    /// Get contract payload mass at index
    #[func]
    pub fn get_contract_payload(&self, index: i32) -> f64 {
        self.state
            .player_company
            .available_contracts
            .get(index as usize)
            .map(|c| c.payload_mass_kg)
            .unwrap_or(0.0)
    }

    /// Get contract reward at index
    #[func]
    pub fn get_contract_reward(&self, index: i32) -> f64 {
        self.state
            .player_company
            .available_contracts
            .get(index as usize)
            .map(|c| c.reward)
            .unwrap_or(0.0)
    }

    /// Get contract reward formatted at index
    #[func]
    pub fn get_contract_reward_formatted(&self, index: i32) -> GString {
        let reward_str = self.state
            .player_company
            .available_contracts
            .get(index as usize)
            .map(|c| format_money(c.reward))
            .unwrap_or_default();
        GString::from(reward_str.as_str())
    }

    /// Select a contract by ID
    #[func]
    pub fn select_contract(&mut self, contract_id: i32) -> bool {
        if self.state.player_company.select_contract(contract_id as u32) {
            self.base_mut()
                .emit_signal("contract_selected", &[Variant::from(contract_id)]);
            self.base_mut()
                .emit_signal("contracts_changed", &[]);
            true
        } else {
            false
        }
    }

    /// Check if we can afford to refresh contracts
    #[func]
    pub fn can_refresh_contracts(&self) -> bool {
        self.state.player_company.can_refresh_contracts()
    }

    /// Get contract refresh cost
    #[func]
    pub fn get_refresh_cost(&self) -> f64 {
        CONTRACT_REFRESH_COST
    }

    /// Get contract refresh cost formatted
    #[func]
    pub fn get_refresh_cost_formatted(&self) -> GString {
        GString::from(format_money(CONTRACT_REFRESH_COST).as_str())
    }

    /// Refresh available contracts (costs money)
    #[func]
    pub fn refresh_contracts(&mut self) -> bool {
        self.sync_money_to_state();
        if self.state.player_company.refresh_contracts() {
            self.sync_money_from_state();
            self.base_mut().emit_signal("contracts_changed", &[]);
            true
        } else {
            false
        }
    }

    // ==========================================
    // Active Contract
    // ==========================================

    /// Check if there's an active contract
    #[func]
    pub fn has_active_contract(&self) -> bool {
        self.state.player_company.active_contract.is_some()
    }

    /// Get active contract name
    #[func]
    pub fn get_active_contract_name(&self) -> GString {
        GString::from(
            self.state
                .player_company
                .active_contract
                .as_ref()
                .map(|c| c.name.as_str())
                .unwrap_or("")
        )
    }

    /// Get active contract destination
    #[func]
    pub fn get_active_contract_destination(&self) -> GString {
        GString::from(
            self.state
                .player_company
                .active_contract
                .as_ref()
                .map(|c| c.destination.display_name())
                .unwrap_or("")
        )
    }

    /// Get active contract required delta-v
    #[func]
    pub fn get_active_contract_delta_v(&self) -> f64 {
        self.state.player_company.get_target_delta_v()
    }

    /// Get active contract payload mass
    #[func]
    pub fn get_active_contract_payload(&self) -> f64 {
        self.state.player_company.get_payload_mass()
    }

    /// Get active contract reward
    #[func]
    pub fn get_active_contract_reward(&self) -> f64 {
        self.state
            .player_company
            .active_contract
            .as_ref()
            .map(|c| c.reward)
            .unwrap_or(0.0)
    }

    /// Get active contract reward formatted
    #[func]
    pub fn get_active_contract_reward_formatted(&self) -> GString {
        let reward_str = self.state
            .player_company
            .active_contract
            .as_ref()
            .map(|c| format_money(c.reward))
            .unwrap_or_default();
        GString::from(reward_str.as_str())
    }

    /// Abandon the current contract
    #[func]
    pub fn abandon_contract(&mut self) {
        self.state.player_company.abandon_contract();
        self.base_mut().emit_signal("contracts_changed", &[]);
    }

    // ==========================================
    // Mission Completion
    // ==========================================

    /// Complete the current contract (call after successful launch)
    #[func]
    pub fn complete_contract(&mut self) -> f64 {
        self.sync_money_to_state();
        let design_id = self.current_rocket_design_id.unwrap_or(0);
        let reward = self.state.player_company.complete_contract(design_id);
        if reward > 0.0 {
            self.state.turn += 1;
            self.sync_money_from_state();
            // Launch takes 30 days
            self.advance_time_days(30);
            // Successful launch increases fame (more fame for harder missions)
            let fame_gain = 10.0 + (reward / 10_000_000.0); // Base 10 + scaled by reward
            self.adjust_fame(fame_gain);
            self.base_mut()
                .emit_signal("contract_completed", &[Variant::from(reward)]);
            self.base_mut().emit_signal("contracts_changed", &[]);
        }
        reward
    }

    /// Record a failed launch
    #[func]
    pub fn fail_contract(&mut self) {
        self.sync_money_to_state();
        let design_id = self.current_rocket_design_id.unwrap_or(0);
        self.state.player_company.fail_contract(design_id);
        self.sync_money_from_state();
        // Launch takes 30 days even on failure
        self.advance_time_days(30);
        // Failed launch decreases fame
        self.adjust_fame(-15.0);
        self.base_mut().emit_signal("contract_failed", &[]);
    }

    /// Pay for the rocket (deduct cost from money)
    #[func]
    pub fn pay_for_rocket(&mut self, cost: f64) -> bool {
        let result = self.finance.bind_mut().deduct(cost);
        if result {
            self.emit_money_changed();
        }
        result
    }

    /// Add money (for testing or cheats)
    #[func]
    pub fn add_money(&mut self, amount: f64) {
        self.finance.bind_mut().add(amount);
        self.emit_money_changed();
    }

    // ==========================================
    // Destination Info (Static)
    // ==========================================

    /// Get number of destinations
    #[func]
    pub fn get_destination_count(&self) -> i32 {
        Destination::all().len() as i32
    }

    /// Get destination name at index
    #[func]
    pub fn get_destination_name(&self, index: i32) -> GString {
        GString::from(
            Destination::all()
                .get(index as usize)
                .map(|d| d.display_name())
                .unwrap_or("")
        )
    }

    /// Get destination short code at index
    #[func]
    pub fn get_destination_short_name(&self, index: i32) -> GString {
        GString::from(
            Destination::all()
                .get(index as usize)
                .map(|d| d.short_name())
                .unwrap_or("")
        )
    }

    /// Get destination delta-v at index
    #[func]
    pub fn get_destination_delta_v(&self, index: i32) -> f64 {
        Destination::all()
            .get(index as usize)
            .map(|d| d.required_delta_v())
            .unwrap_or(0.0)
    }

    // ==========================================
    // Design Management
    // ==========================================

    /// Get number of rocket designs (lineages)
    #[func]
    pub fn get_rocket_design_count(&self) -> i32 {
        self.state.player_company.get_rocket_design_count() as i32
    }

    /// Get design name at index
    #[func]
    pub fn get_rocket_design_name(&self, index: i32) -> GString {
        if index < 0 {
            return GString::from("");
        }
        GString::from(
            self.state
                .player_company
                .get_rocket_design(index as usize)
                .map(|d| d.name.as_str())
                .unwrap_or("")
        )
    }

    /// Get design stage count at index
    #[func]
    pub fn get_rocket_design_stage_count(&self, index: i32) -> i32 {
        if index < 0 {
            return 0;
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| d.stage_count() as i32)
            .unwrap_or(0)
    }

    /// Get design total delta-v at index
    #[func]
    pub fn get_rocket_design_delta_v(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| d.total_effective_delta_v())
            .unwrap_or(0.0)
    }

    /// Get design total cost at index
    #[func]
    pub fn get_rocket_design_cost(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| d.total_cost())
            .unwrap_or(0.0)
    }

    /// Get design total wet mass at index (in kg)
    #[func]
    pub fn get_rocket_design_mass(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| d.total_wet_mass_kg())
            .unwrap_or(0.0)
    }

    /// Get design testing level at index (0-4)
    #[func]
    pub fn get_rocket_design_testing_level(&self, index: i32) -> i32 {
        if index < 0 {
            return 0;
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| {
                use crate::flaw::rocket_testing_level;
                use std::collections::HashSet;

                let stage_count = d.stages.len();
                if stage_count == 0 {
                    return 0;
                }
                let unique_fuel_types = {
                    let fuel_types: HashSet<usize> = d.stages.iter()
                        .map(|s| s.engine_snapshot().fuel_type.index())
                        .collect();
                    fuel_types.len()
                };
                let total_engines: u32 = d.stages.iter().map(|s| s.engine_count).sum();
                rocket_testing_level(stage_count, unique_fuel_types, total_engines, d.testing_work_completed).to_index()
            })
            .unwrap_or(0)
    }

    /// Get design testing level name at index
    #[func]
    pub fn get_rocket_design_testing_level_name(&self, index: i32) -> GString {
        use crate::flaw::TestingLevel;
        GString::from(TestingLevel::from_index(self.get_rocket_design_testing_level(index)).name())
    }

    /// Get engine testing level at index (0-4)
    #[func]
    pub fn get_engine_testing_level(&self, index: i32) -> i32 {
        if index < 0 {
            return 0;
        }
        self.state
            .player_company
            .engine_designs
            .get(index as usize)
            .map(|l| {
                use crate::flaw::engine_testing_level;
                let d = l.head();
                engine_testing_level(d.fuel_type(), d.scale, d.testing_work_completed).to_index()
            })
            .unwrap_or(0)
    }

    /// Get engine testing level name at index
    #[func]
    pub fn get_engine_testing_level_name(&self, index: i32) -> GString {
        use crate::flaw::TestingLevel;
        GString::from(TestingLevel::from_index(self.get_engine_testing_level(index)).name())
    }

    /// Check if a rocket design has generated flaws
    #[func]
    pub fn rocket_design_has_flaws(&self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| d.has_flaws_generated())
            .unwrap_or(false)
    }

    /// Get count of discovered flaws for a rocket design
    #[func]
    pub fn get_rocket_design_discovered_flaw_count(&self, index: i32) -> i32 {
        if index < 0 {
            return 0;
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| d.get_discovered_flaw_count() as i32)
            .unwrap_or(0)
    }

    /// Get count of fixed flaws for a rocket design
    #[func]
    pub fn get_rocket_design_fixed_flaw_count(&self, index: i32) -> i32 {
        if index < 0 {
            return 0;
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| d.get_fixed_flaw_count() as i32)
            .unwrap_or(0)
    }

    /// Get names of discovered (but not fixed) flaws for a rocket design
    #[func]
    pub fn get_rocket_design_unfixed_flaw_names(&self, index: i32) -> Array<GString> {
        let mut result = Array::new();
        if index < 0 {
            return result;
        }
        if let Some(design) = self.state.player_company.get_rocket_design(index as usize) {
            for flaw in &design.active_flaws {
                if flaw.discovered && !flaw.fixed {
                    result.push(&GString::from(flaw.name.as_str()));
                }
            }
        }
        result
    }

    /// Get names of fixed flaws for a rocket design
    #[func]
    pub fn get_rocket_design_fixed_flaw_names(&self, index: i32) -> Array<GString> {
        let mut result = Array::new();
        if index < 0 {
            return result;
        }
        if let Some(design) = self.state.player_company.get_rocket_design(index as usize) {
            for flaw in &design.active_flaws {
                if flaw.fixed {
                    result.push(&GString::from(flaw.name.as_str()));
                }
            }
        }
        result
    }

    /// Save the designer's current design as a new lineage
    /// Returns the index of the new lineage
    #[func]
    pub fn save_current_design(&mut self, designer: Gd<RocketDesigner>) -> i32 {
        let design = designer.bind().get_design_clone();
        let index = self.state.player_company.save_new_design(design);
        self.current_rocket_design_id = Some(index);
        self.base_mut().emit_signal("designs_changed", &[]);
        index as i32
    }

    /// Save the designer's current design as a new lineage with a specific name
    /// Returns the index of the new lineage
    #[func]
    pub fn save_design_as(&mut self, designer: Gd<RocketDesigner>, name: GString) -> i32 {
        let design = designer.bind().get_design_clone();
        let index = self.state.player_company.save_new_design_as(design, &name.to_string());
        self.current_rocket_design_id = Some(index);
        self.base_mut().emit_signal("designs_changed", &[]);
        index as i32
    }

    /// Load a rocket design into the designer
    #[func]
    pub fn load_rocket_design(&mut self, index: i32) -> bool {
        if index < 0 {
            self.current_rocket_design_id = None;
            return false;
        }
        if self.state.player_company.load_rocket_design(index as usize).is_some() {
            self.current_rocket_design_id = Some(index as usize);
            self.base_mut().emit_signal("designs_changed", &[]);
            true
        } else {
            false
        }
    }

    /// Refresh the current design - no-op in lineage model
    /// (designer re-syncs from lineage head via sync_design_to)
    #[func]
    pub fn refresh_current_design(&mut self) -> bool {
        self.current_rocket_design_id.is_some()
    }

    /// Update a rocket design's lineage head with the designer's current state
    #[func]
    pub fn update_rocket_design(&mut self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        // This is called by GDScript - the designer pushes its state here
        // The actual update happens via sync_design_from
        self.base_mut().emit_signal("designs_changed", &[]);
        true
    }

    /// Update the currently active rocket design from the designer
    /// Call this after launch to save testing_spent reset and flaw changes
    #[func]
    pub fn update_current_rocket_design(&mut self) {
        // In the lineage model, the lineage head is already updated via sync_design_from
        self.base_mut().emit_signal("designs_changed", &[]);
    }

    /// Delete a rocket design lineage
    #[func]
    pub fn delete_rocket_design(&mut self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.state.player_company.delete_rocket_design(index as usize);
        if result {
            // Invalidate current_rocket_design_id if it was the deleted one or shifted
            if let Some(current) = self.current_rocket_design_id {
                if current == index as usize {
                    self.current_rocket_design_id = None;
                } else if current > index as usize {
                    self.current_rocket_design_id = Some(current - 1);
                }
            }
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Rename a rocket design lineage
    #[func]
    pub fn rename_rocket_design(&mut self, index: i32, new_name: GString) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.state.player_company.rename_rocket_design(index as usize, &new_name.to_string());
        if result {
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Duplicate a rocket design lineage
    #[func]
    pub fn duplicate_rocket_design(&mut self, index: i32) -> i32 {
        if index < 0 {
            return -1;
        }
        match self.state.player_company.duplicate_rocket_design(index as usize) {
            Some(new_index) => {
                self.base_mut().emit_signal("designs_changed", &[]);
                new_index as i32
            }
            None => -1,
        }
    }

    /// Create a new empty design lineage and set it as active
    #[func]
    pub fn create_new_design(&mut self) -> i32 {
        let index = self.state.player_company.create_new_design();
        self.current_rocket_design_id = Some(index);
        index as i32
    }

    /// Create a new design lineage based on the default template and set it as active
    #[func]
    pub fn create_default_design(&mut self) -> i32 {
        let index = self.state.player_company.create_default_design();
        self.current_rocket_design_id = Some(index);
        index as i32
    }

    /// Get the current working design name (from active lineage head)
    #[func]
    pub fn get_current_design_name(&self) -> GString {
        if let Some(id) = self.current_rocket_design_id {
            self.state.player_company.get_rocket_design(id)
                .map(|d| GString::from(d.name.as_str()))
                .unwrap_or_default()
        } else {
            GString::from("")
        }
    }

    /// Set the current working design name (on active lineage head)
    #[func]
    pub fn set_current_design_name(&mut self, name: GString) {
        if let Some(id) = self.current_rocket_design_id {
            self.state.player_company.rename_rocket_design(id, &name.to_string());
        }
    }

    /// Copy design from a RocketDesigner node into the lineage head
    /// Also updates the active lineage if one is being edited
    #[func]
    pub fn sync_design_from(&mut self, designer: Gd<RocketDesigner>) {
        let design = designer.bind().get_design_clone();

        // Update the active lineage head
        if let Some(index) = self.current_rocket_design_id {
            self.state.player_company.update_rocket_design(index, design);
            // If the design is Testing and has a discovered unfixed flaw, start fixing it
            let design = self.state.player_company.rocket_designs[index].head_mut();
            if matches!(design.design_status, crate::rocket_design::DesignStatus::Testing { .. }) {
                if let Some(flaw_index) = design.get_next_unfixed_flaw() {
                    design.start_fixing_flaw(flaw_index);
                }
            }
            self.base_mut().emit_signal("designs_changed", &[]);
        }
    }

    /// Sync engine flaw data from designer back to Company
    /// Call this after testing in the designer to persist flaw discoveries
    #[func]
    pub fn sync_engine_flaws_from_designer(&mut self, designer: Gd<RocketDesigner>) {
        let design = designer.bind().get_design_clone();
        for flaw in &design.active_flaws {
            if flaw.flaw_type == crate::flaw::FlawType::Engine {
                if let Some(idx) = flaw.engine_design_id {
                    if idx < self.state.player_company.engine_designs.len() {
                        let engine_design = self.state.player_company.engine_designs[idx].head_mut();
                        if let Some(existing) = engine_design.active_flaws.iter_mut().find(|f| f.id == flaw.id) {
                            existing.discovered = flaw.discovered;
                        }
                    }
                }
            }
        }
        for flaw in &design.fixed_flaws {
            if flaw.flaw_type == crate::flaw::FlawType::Engine {
                if let Some(idx) = flaw.engine_design_id {
                    if idx < self.state.player_company.engine_designs.len() {
                        let engine_design = self.state.player_company.engine_designs[idx].head_mut();
                        if let Some(pos) = engine_design.active_flaws.iter().position(|f| f.id == flaw.id) {
                            let mut f = engine_design.active_flaws.remove(pos);
                            f.fixed = true;
                            engine_design.fixed_flaws.push(f);
                        }
                    }
                }
            }
        }
    }

    /// Ensure the current design is saved as a lineage
    /// If already associated with a lineage, returns that index
    /// Otherwise creates a new lineage from the designer's state
    #[func]
    pub fn ensure_design_saved(&mut self, designer: Gd<RocketDesigner>) -> i32 {
        if let Some(index) = self.current_rocket_design_id {
            // Already associated with a lineage - sync from designer first
            let design = designer.bind().get_design_clone();
            self.state.player_company.update_rocket_design(index, design);
            index as i32
        } else {
            // Check if a lineage with this name already exists
            let design = designer.bind().get_design_clone();
            let current_name = design.name.clone();
            if let Some(existing_index) = self.state.player_company.rocket_designs.iter().position(|l| l.head().name == current_name) {
                self.state.player_company.update_rocket_design(existing_index, design);
                self.current_rocket_design_id = Some(existing_index);
                self.base_mut().emit_signal("designs_changed", &[]);
                existing_index as i32
            } else {
                let index = self.state.player_company.save_new_design(design);
                self.current_rocket_design_id = Some(index);
                self.base_mut().emit_signal("designs_changed", &[]);
                index as i32
            }
        }
    }

    /// Copy design from lineage head to a RocketDesigner node
    /// Call this after loading a design to update the designer
    /// Sets the design's budget to the current player money
    /// Also syncs engine data (snapshots + flaws) from Company
    #[func]
    pub fn sync_design_to(&self, mut designer: Gd<RocketDesigner>) {
        let design = if let Some(id) = self.current_rocket_design_id {
            self.state.player_company.load_rocket_design(id)
        } else {
            None
        };
        let mut design = design.unwrap_or_else(|| self.state.player_company.rocket_designs[0].head().clone());
        design.budget = self.finance.bind().get_money();

        // Set targets from active contract so the designer shows correct mission requirements
        if self.state.player_company.active_contract.is_some() {
            design.target_delta_v = self.state.player_company.get_target_delta_v();
            design.payload_mass_kg = self.state.player_company.get_payload_mass();
        }

        // Sync engine data from Company to Designer
        let snapshots: Vec<_> = self.state.player_company.engine_designs
            .iter()
            .enumerate()
            .map(|(i, lineage)| lineage.head().snapshot(i, &lineage.name))
            .collect();
        let flaws: Vec<_> = self.state.player_company.engine_designs
            .iter()
            .map(|lineage| {
                let head = lineage.head();
                (head.active_flaws.clone(), head.fixed_flaws.clone())
            })
            .collect();
        designer.bind_mut().sync_engine_data(snapshots, flaws);

        designer.bind_mut().set_design(design);
        designer.bind_mut().set_finance(self.finance.clone());
    }

    /// Sync only engine data (snapshots + flaws) from Company to a RocketDesigner
    /// without resetting the rocket design. Use when engines change while user is editing.
    #[func]
    pub fn sync_engines_to_designer(&self, mut designer: Gd<RocketDesigner>) {
        let snapshots: Vec<_> = self.state.player_company.engine_designs
            .iter()
            .enumerate()
            .map(|(i, lineage)| lineage.head().snapshot(i, &lineage.name))
            .collect();
        let flaws: Vec<_> = self.state.player_company.engine_designs
            .iter()
            .map(|lineage| {
                let head = lineage.head();
                (head.active_flaws.clone(), head.fixed_flaws.clone())
            })
            .collect();
        designer.bind_mut().sync_engine_data(snapshots, flaws);
    }

    // ==========================================
    // Design Status Management
    // ==========================================

    /// Get the status name of a saved design (includes flaw name if Fixing)
    #[func]
    pub fn get_design_status(&self, index: i32) -> GString {
        if index < 0 {
            return GString::from("");
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| GString::from(d.design_status.display_name().as_str()))
            .unwrap_or_default()
    }

    /// Get the base status name of a saved design (without flaw name)
    #[func]
    pub fn get_design_status_base(&self, index: i32) -> GString {
        if index < 0 {
            return GString::from("");
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| GString::from(d.design_status.name()))
            .unwrap_or_default()
    }

    /// Get the work progress of a saved design (0.0 to 1.0)
    #[func]
    pub fn get_design_progress(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| d.design_status.progress_fraction())
            .unwrap_or(0.0)
    }

    /// Check if a saved design can be edited
    #[func]
    pub fn can_edit_design(&self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| d.design_status.can_edit())
            .unwrap_or(false)
    }

    /// Check if a saved design can be launched
    #[func]
    pub fn can_launch_design(&self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        self.state
            .player_company
            .get_rocket_design(index as usize)
            .map(|d| d.design_status.can_launch())
            .unwrap_or(false)
    }

    /// Get the index of the currently edited design (-1 if none)
    #[func]
    pub fn get_current_design_index(&self) -> i32 {
        self.current_rocket_design_id.map(|i| i as i32).unwrap_or(-1)
    }

    /// Get the status of the currently edited design
    #[func]
    pub fn get_current_design_status(&self) -> GString {
        self.get_design_status(self.get_current_design_index())
    }

    /// Check if the current design can be submitted to engineering
    #[func]
    pub fn can_submit_current_to_engineering(&self) -> bool {
        let index = self.get_current_design_index();
        if index < 0 {
            return false;
        }
        self.get_design_status(index) == GString::from("Specification")
    }

    /// Submit the current design to engineering
    #[func]
    pub fn submit_current_to_engineering(&mut self) -> bool {
        let index = self.get_current_design_index();
        if index < 0 {
            return false;
        }
        self.submit_design_to_engineering(index)
    }

    /// Submit a rocket design to engineering
    /// Returns true if successful
    #[func]
    pub fn submit_design_to_engineering(&mut self, index: i32) -> bool {
        if index < 0 || index >= self.state.player_company.rocket_designs.len() as i32 {
            return false;
        }
        let idx = index as usize;
        let result = self.state.player_company.rocket_designs[idx].head_mut().submit_to_engineering();
        if result {
            let flaw_gen = &mut self.state.player_company.flaw_generator;
            let design = self.state.player_company.rocket_designs[idx].head_mut();
            design.generate_flaws(flaw_gen);
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Reset a rocket design back to Specification status
    #[func]
    pub fn reset_design_to_specification(&mut self, index: i32) -> bool {
        if index < 0 || index >= self.state.player_company.rocket_designs.len() as i32 {
            return false;
        }
        self.state.player_company.rocket_designs[index as usize].head_mut().reset_to_specification();
        self.base_mut().emit_signal("designs_changed", &[]);
        true
    }

    // ==========================================
    // Date/Time Management
    // ==========================================

    /// Get current day number
    #[func]
    pub fn get_current_day(&self) -> i32 {
        self.state.current_day as i32
    }

    /// Get formatted date string (e.g., "Day 45, Year 2001")
    #[func]
    pub fn get_date_formatted(&self) -> GString {
        GString::from(self.state.get_date_string().as_str())
    }

    /// Get current year
    #[func]
    pub fn get_current_year(&self) -> i32 {
        self.state.get_current_year() as i32
    }

    /// Advance game time by a number of days and emit signal (legacy API)
    fn advance_time_days(&mut self, days: u32) {
        self.state.advance_days(days);
        let new_day = self.state.current_day as i32;
        self.base_mut()
            .emit_signal("date_changed", &[Variant::from(new_day)]);
    }

    // ==========================================
    // Continuous Time System
    // ==========================================

    /// Advance time by delta_seconds (called from _process)
    /// Returns an array of work event dictionaries
    #[func]
    pub fn advance_time(&mut self, delta_seconds: f64) -> Array<Dictionary> {
        let events = self.state.advance_time(delta_seconds);

        // Emit date_changed if day changed
        let new_day = self.state.current_day as i32;
        self.base_mut()
            .emit_signal("date_changed", &[Variant::from(new_day)]);

        // Sync money from state in case salaries were deducted
        self.sync_money_from_state();

        // Check if salary was deducted and emit money_changed
        let had_salary_event = events
            .iter()
            .any(|e| matches!(e, crate::engineering_team::WorkEvent::SalaryDeducted { .. }));
        if had_salary_event {
            self.emit_money_changed();
        }

        // Check for manufacturing events to emit appropriate signals
        let has_manufacturing_event = events.iter().any(|e| {
            matches!(
                e,
                crate::engineering_team::WorkEvent::EngineManufactured { .. }
                    | crate::engineering_team::WorkEvent::RocketAssembled { .. }
                    | crate::engineering_team::WorkEvent::ManufacturingOrderComplete { .. }
                    | crate::engineering_team::WorkEvent::FloorSpaceCompleted { .. }
            )
        });
        let has_inventory_event = events.iter().any(|e| {
            matches!(
                e,
                crate::engineering_team::WorkEvent::EngineManufactured { .. }
                    | crate::engineering_team::WorkEvent::RocketAssembled { .. }
            )
        });

        // Convert events to Godot dictionaries
        let mut result = Array::new();
        for event in events {
            let dict = self.work_event_to_dict(&event);
            result.push(&dict);

            // Emit signal for each event
            self.emit_work_event(&event);
        }

        // Emit aggregate signals after processing all events
        if has_manufacturing_event {
            self.base_mut()
                .emit_signal("manufacturing_changed", &[]);
        }
        if has_inventory_event {
            self.base_mut()
                .emit_signal("inventory_changed", &[]);
            // Inventory changes also affect money (material costs already deducted at order start)
            self.emit_money_changed();
        }

        result
    }

    /// Toggle time pause state
    #[func]
    pub fn toggle_time_pause(&mut self) {
        self.state.toggle_time_pause();
        if self.state.is_time_paused() {
            self.base_mut().emit_signal("time_paused", &[]);
        } else {
            self.base_mut().emit_signal("time_resumed", &[]);
        }
    }

    /// Check if time is paused
    #[func]
    pub fn is_time_paused(&self) -> bool {
        self.state.is_time_paused()
    }

    /// Set time pause state explicitly
    #[func]
    pub fn set_time_paused(&mut self, paused: bool) {
        let was_paused = self.state.is_time_paused();
        self.state.set_time_paused(paused);
        if paused != was_paused {
            if paused {
                self.base_mut().emit_signal("time_paused", &[]);
            } else {
                self.base_mut().emit_signal("time_resumed", &[]);
            }
        }
    }

    /// Get days until next salary payment
    #[func]
    pub fn days_until_salary(&self) -> i32 {
        self.state.days_until_salary() as i32
    }

    /// Convert a WorkEvent to a Godot Dictionary
    fn work_event_to_dict(&self, event: &crate::engineering_team::WorkEvent) -> Dictionary {
        use crate::engineering_team::WorkEvent;

        let mut dict = Dictionary::new();
        match event {
            WorkEvent::DesignPhaseComplete {
                rocket_design_id,
                phase_name,
            } => {
                dict.set("type", "design_phase_complete");
                dict.set("design_index", *rocket_design_id as i32);
                dict.set("phase_name", GString::from(phase_name.as_str()));
            }
            WorkEvent::DesignFlawDiscovered {
                rocket_design_id,
                flaw_name,
            } => {
                dict.set("type", "design_flaw_discovered");
                dict.set("design_index", *rocket_design_id as i32);
                dict.set("flaw_name", GString::from(flaw_name.as_str()));
            }
            WorkEvent::DesignFlawFixed {
                rocket_design_id,
                flaw_name,
            } => {
                dict.set("type", "design_flaw_fixed");
                dict.set("design_index", *rocket_design_id as i32);
                dict.set("flaw_name", GString::from(flaw_name.as_str()));
            }
            WorkEvent::EngineFlawDiscovered {
                engine_design_id,
                flaw_name,
            } => {
                dict.set("type", "engine_flaw_discovered");
                dict.set("engine_design_id", *engine_design_id as i32);
                dict.set("flaw_name", GString::from(flaw_name.as_str()));
            }
            WorkEvent::EngineFlawFixed {
                engine_design_id,
                flaw_name,
            } => {
                dict.set("type", "engine_flaw_fixed");
                dict.set("engine_design_id", *engine_design_id as i32);
                dict.set("flaw_name", GString::from(flaw_name.as_str()));
            }
            WorkEvent::TeamRampedUp { team_id } => {
                dict.set("type", "team_ramped_up");
                dict.set("team_id", *team_id as i32);
            }
            WorkEvent::EngineManufactured {
                engine_design_id,
                revision_number,
                order_id,
            } => {
                dict.set("type", "engine_manufactured");
                dict.set("engine_design_id", *engine_design_id as i32);
                dict.set("revision_number", *revision_number as i32);
                dict.set("order_id", *order_id as i32);
            }
            WorkEvent::RocketAssembled {
                rocket_design_id,
                revision_number,
                serial_number,
            } => {
                dict.set("type", "rocket_assembled");
                dict.set("design_index", *rocket_design_id as i32);
                dict.set("revision_number", *revision_number as i32);
                dict.set("serial_number", *serial_number as i32);
            }
            WorkEvent::ManufacturingOrderComplete { order_id } => {
                dict.set("type", "manufacturing_order_complete");
                dict.set("order_id", *order_id as i32);
            }
            WorkEvent::SalaryDeducted { amount } => {
                dict.set("type", "salary_deducted");
                dict.set("amount", *amount);
            }
            WorkEvent::FloorSpaceCompleted { units } => {
                dict.set("type", "floor_space_completed");
                dict.set("units", *units as i32);
            }
        }
        dict
    }

    /// Emit a signal for a work event
    fn emit_work_event(&mut self, event: &crate::engineering_team::WorkEvent) {
        let dict = self.work_event_to_dict(event);
        let event_type = match event {
            crate::engineering_team::WorkEvent::DesignPhaseComplete { .. } => {
                "design_phase_complete"
            }
            crate::engineering_team::WorkEvent::DesignFlawDiscovered { .. } => {
                "design_flaw_discovered"
            }
            crate::engineering_team::WorkEvent::DesignFlawFixed { .. } => "design_flaw_fixed",
            crate::engineering_team::WorkEvent::EngineFlawDiscovered { .. } => {
                "engine_flaw_discovered"
            }
            crate::engineering_team::WorkEvent::EngineFlawFixed { .. } => "engine_flaw_fixed",
            crate::engineering_team::WorkEvent::EngineManufactured { .. } => {
                "engine_manufactured"
            }
            crate::engineering_team::WorkEvent::RocketAssembled { .. } => "rocket_assembled",
            crate::engineering_team::WorkEvent::ManufacturingOrderComplete { .. } => {
                "manufacturing_order_complete"
            }
            crate::engineering_team::WorkEvent::TeamRampedUp { .. } => "team_ramped_up",
            crate::engineering_team::WorkEvent::SalaryDeducted { .. } => "salary_deducted",
            crate::engineering_team::WorkEvent::FloorSpaceCompleted { .. } => {
                "floor_space_completed"
            }
        };
        self.base_mut().emit_signal(
            "work_event_occurred",
            &[
                Variant::from(GString::from(event_type)),
                Variant::from(dict),
            ],
        );
    }

    // ==========================================
    // Fame Management
    // ==========================================

    /// Get current fame value
    #[func]
    pub fn get_fame(&self) -> f64 {
        self.state.player_company.fame
    }

    /// Get fame formatted as integer for display
    #[func]
    pub fn get_fame_formatted(&self) -> GString {
        GString::from(format!("{:.0}", self.state.player_company.fame).as_str())
    }

    /// Get fame tier (0-5)
    #[func]
    pub fn get_fame_tier(&self) -> i32 {
        self.state.player_company.get_fame_tier() as i32
    }

    /// Get fame tier name (Unknown, Newcomer, Established, etc.)
    #[func]
    pub fn get_fame_tier_name(&self) -> GString {
        GString::from(self.state.player_company.get_fame_tier_name())
    }

    /// Adjust fame and emit signal
    fn adjust_fame(&mut self, delta: f64) {
        self.state.player_company.adjust_fame(delta);
        let new_fame = self.state.player_company.fame;
        self.base_mut()
            .emit_signal("fame_changed", &[Variant::from(new_fame)]);
    }

    // ==========================================
    // Launch Site Management
    // ==========================================

    /// Get current pad level (1-5)
    #[func]
    pub fn get_pad_level(&self) -> i32 {
        self.state.player_company.launch_site.pad_level as i32
    }

    /// Get pad level name
    #[func]
    pub fn get_pad_level_name(&self) -> GString {
        GString::from(self.state.player_company.launch_site.pad_level_name())
    }

    /// Get maximum launch mass for current pad
    #[func]
    pub fn get_max_launch_mass(&self) -> f64 {
        self.state.player_company.launch_site.max_launch_mass_kg()
    }

    /// Get maximum launch mass formatted (e.g., "200t")
    #[func]
    pub fn get_max_launch_mass_formatted(&self) -> GString {
        let mass = self.state.player_company.launch_site.max_launch_mass_kg();
        if mass >= 1_000_000.0 {
            GString::from(format!("{:.1}kt", mass / 1_000_000.0).as_str())
        } else {
            GString::from(format!("{:.0}t", mass / 1000.0).as_str())
        }
    }

    /// Get cost to upgrade pad
    #[func]
    pub fn get_pad_upgrade_cost(&self) -> f64 {
        self.state.player_company.launch_site.pad_upgrade_cost()
    }

    /// Get pad upgrade cost formatted
    #[func]
    pub fn get_pad_upgrade_cost_formatted(&self) -> GString {
        let cost = self.state.player_company.launch_site.pad_upgrade_cost();
        if cost > 0.0 {
            GString::from(format_money(cost).as_str())
        } else {
            GString::from("Max Level")
        }
    }

    /// Check if pad can be upgraded
    #[func]
    pub fn can_upgrade_pad(&self) -> bool {
        let cost = self.state.player_company.launch_site.pad_upgrade_cost();
        cost > 0.0 && self.finance.bind().get_money() >= cost
    }

    /// Upgrade the launch pad (deducts cost)
    #[func]
    pub fn upgrade_pad(&mut self) -> bool {
        let cost = self.state.player_company.launch_site.pad_upgrade_cost();
        if cost > 0.0 && self.finance.bind_mut().deduct(cost) {
            let result = self.state.player_company.launch_site.upgrade_pad();
            self.emit_money_changed();
            result
        } else {
            false
        }
    }

    /// Check if current rocket can be launched at this site
    #[func]
    pub fn can_launch_current_rocket(&self) -> bool {
        let design_id = self.current_rocket_design_id.unwrap_or(0);
        self.state.player_company.can_launch_rocket_at_site(design_id)
    }

    /// Get propellant storage capacity
    #[func]
    pub fn get_propellant_storage(&self) -> f64 {
        self.state.player_company.launch_site.propellant_storage_kg
    }

    // ==========================================
    // Engineering Team Management
    // ==========================================

    /// Get the number of all teams
    #[func]
    pub fn get_team_count(&self) -> i32 {
        self.state.player_company.get_team_count() as i32
    }

    /// Hire a new engineering team (deducts hire cost)
    /// Returns the team ID, or -1 if can't afford
    #[func]
    pub fn hire_engineering_team(&mut self) -> i32 {
        self.sync_money_to_state();
        match self.state.player_company.hire_engineering_team() {
            Some(id) => {
                self.sync_money_from_state();
                self.emit_money_changed();
                self.base_mut().emit_signal("teams_changed", &[]);
                id as i32
            }
            None => -1,
        }
    }

    /// Hire a new manufacturing team (deducts hire cost)
    /// Returns the team ID, or -1 if can't afford
    #[func]
    pub fn hire_manufacturing_team(&mut self) -> i32 {
        self.sync_money_to_state();
        match self.state.player_company.hire_manufacturing_team() {
            Some(id) => {
                self.sync_money_from_state();
                self.emit_money_changed();
                self.base_mut().emit_signal("teams_changed", &[]);
                id as i32
            }
            None => -1,
        }
    }

    /// Fire a team by ID
    /// Returns true if team was found and removed
    #[func]
    pub fn fire_team(&mut self, team_id: i32) -> bool {
        let result = self.state.player_company.fire_team(team_id as u32);
        if result {
            self.base_mut().emit_signal("teams_changed", &[]);
        }
        result
    }

    /// Get team name by ID
    #[func]
    pub fn get_team_name(&self, team_id: i32) -> GString {
        self.state
            .player_company
            .get_team(team_id as u32)
            .map(|t| GString::from(t.name.as_str()))
            .unwrap_or_default()
    }

    /// Check if a team is ramping up
    #[func]
    pub fn is_team_ramping_up(&self, team_id: i32) -> bool {
        self.state
            .player_company
            .get_team(team_id as u32)
            .map(|t| t.is_ramping_up())
            .unwrap_or(false)
    }

    /// Get team's ramp-up days remaining
    #[func]
    pub fn get_team_ramp_up_days(&self, team_id: i32) -> i32 {
        self.state
            .player_company
            .get_team(team_id as u32)
            .map(|t| t.ramp_up_days_remaining as i32)
            .unwrap_or(0)
    }

    /// Assign a team to work on a design
    #[func]
    pub fn assign_team_to_design(&mut self, team_id: i32, design_index: i32) -> bool {
        if design_index < 0 {
            return false;
        }
        let result = self.state.player_company.assign_team_to_design(team_id as u32, design_index as usize);
        if result {
            self.base_mut().emit_signal("teams_changed", &[]);
        }
        result
    }

    /// Assign a team to work on an engine design
    #[func]
    pub fn assign_team_to_engine(&mut self, team_id: i32, engine_design_id: i32) -> bool {
        if engine_design_id < 0 {
            return false;
        }
        let result = self.state.player_company.assign_team_to_engine(team_id as u32, engine_design_id as usize);
        if result {
            self.base_mut().emit_signal("teams_changed", &[]);
        }
        result
    }

    /// Unassign a team from its current work
    #[func]
    pub fn unassign_team(&mut self, team_id: i32) -> bool {
        let result = self.state.player_company.unassign_team(team_id as u32);
        if result {
            self.base_mut().emit_signal("teams_changed", &[]);
        }
        result
    }

    /// Get IDs of unassigned teams
    #[func]
    pub fn get_unassigned_team_ids(&self) -> Array<i32> {
        let mut result = Array::new();
        for id in self.state.player_company.get_unassigned_team_ids() {
            result.push(id as i32);
        }
        result
    }

    /// Get number of teams working on a design
    #[func]
    pub fn get_teams_on_design_count(&self, design_index: i32) -> i32 {
        if design_index < 0 {
            return 0;
        }
        self.state.player_company.get_teams_on_design(design_index as usize).len() as i32
    }

    /// Get total monthly salary for all teams
    #[func]
    pub fn get_total_monthly_salary(&self) -> f64 {
        self.state.player_company.get_total_monthly_salary()
    }

    /// Get total monthly salary formatted
    #[func]
    pub fn get_total_monthly_salary_formatted(&self) -> GString {
        GString::from(format_money(self.state.player_company.get_total_monthly_salary()).as_str())
    }

    /// Get all team IDs
    #[func]
    pub fn get_all_team_ids(&self) -> Array<i32> {
        let mut result = Array::new();
        for team in &self.state.player_company.teams {
            result.push(team.id as i32);
        }
        result
    }

    /// Check if a team is assigned to anything
    #[func]
    pub fn is_team_assigned(&self, team_id: i32) -> bool {
        self.state
            .player_company
            .get_team(team_id as u32)
            .map(|t| t.assignment.is_some())
            .unwrap_or(false)
    }

    /// Get what a team is assigned to (returns a dictionary)
    /// Keys: "type" (string: "none", "design", "engine", "manufacturing"),
    ///        "team_type" (string: "engineering" or "manufacturing"), and type-specific data
    #[func]
    pub fn get_team_assignment(&self, team_id: i32) -> Dictionary {
        use crate::engineering_team::{TeamAssignment, TeamType};

        let mut dict = Dictionary::new();

        if let Some(team) = self.state.player_company.get_team(team_id as u32) {
            dict.set("team_type", match team.team_type {
                TeamType::Engineering => "engineering",
                TeamType::Manufacturing => "manufacturing",
            });
            match &team.assignment {
                None => {
                    dict.set("type", "none");
                }
                Some(TeamAssignment::RocketDesign { rocket_design_id, .. }) => {
                    dict.set("type", "design");
                    dict.set("design_index", *rocket_design_id as i32);
                }
                Some(TeamAssignment::EngineDesign { engine_design_id, .. }) => {
                    dict.set("type", "engine");
                    dict.set("engine_design_id", *engine_design_id as i32);
                }
                Some(TeamAssignment::Manufacturing { order_id }) => {
                    dict.set("type", "manufacturing");
                    dict.set("order_id", *order_id as i32);
                }
            }
        } else {
            dict.set("type", "none");
        }

        dict
    }

    /// Get IDs of engineering teams only
    #[func]
    pub fn get_engineering_team_ids(&self) -> Array<i32> {
        let mut result = Array::new();
        for id in self.state.player_company.get_engineering_team_ids() {
            result.push(id as i32);
        }
        result
    }

    /// Get IDs of manufacturing teams only
    #[func]
    pub fn get_manufacturing_team_ids(&self) -> Array<i32> {
        let mut result = Array::new();
        for id in self.state.player_company.get_manufacturing_team_ids() {
            result.push(id as i32);
        }
        result
    }

    /// Get team type as string ("engineering" or "manufacturing")
    #[func]
    pub fn get_team_type(&self, team_id: i32) -> GString {
        use crate::engineering_team::TeamType;
        self.state.player_company.get_team(team_id as u32)
            .map(|t| match t.team_type {
                TeamType::Engineering => GString::from("engineering"),
                TeamType::Manufacturing => GString::from("manufacturing"),
            })
            .unwrap_or_default()
    }

    /// Get total monthly salary for engineering teams
    #[func]
    pub fn get_engineering_monthly_salary(&self) -> f64 {
        self.state.player_company.get_engineering_monthly_salary()
    }

    /// Get total monthly salary for manufacturing teams
    #[func]
    pub fn get_manufacturing_monthly_salary(&self) -> f64 {
        self.state.player_company.get_manufacturing_monthly_salary()
    }

    // ==========================================
    // Engine Design CRUD
    // ==========================================

    /// Create a new engine design with the given fuel type and scale
    /// Returns the index of the new lineage
    #[func]
    pub fn create_engine_design(&mut self, fuel_type: i32, scale: f64) -> i32 {
        let ft = crate::engine_design::FuelType::from_index(fuel_type as usize).unwrap_or(crate::engine_design::FuelType::Kerolox);
        let idx = self.state.player_company.create_engine_design(ft, scale);
        self.base_mut().emit_signal("designs_changed", &[]);
        idx as i32
    }

    /// Duplicate an engine design lineage
    /// Returns the new index or -1 on failure
    #[func]
    pub fn duplicate_engine_design(&mut self, index: i32) -> i32 {
        if index < 0 {
            return -1;
        }
        match self.state.player_company.duplicate_engine_design(index as usize) {
            Some(new_index) => {
                self.base_mut().emit_signal("designs_changed", &[]);
                new_index as i32
            }
            None => -1,
        }
    }

    /// Delete an engine design lineage
    #[func]
    pub fn delete_engine_design(&mut self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.state.player_company.delete_engine_design(index as usize);
        if result {
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Rename an engine design lineage
    #[func]
    pub fn rename_engine_design(&mut self, index: i32, new_name: GString) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.state.player_company.rename_engine_design(index as usize, &new_name.to_string());
        if result {
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    // ==========================================
    // Engine Design Modification
    // ==========================================

    /// Set the scale of an engine design
    #[func]
    pub fn set_engine_design_scale(&mut self, index: i32, scale: f64) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.state.player_company.set_engine_design_scale(index as usize, scale);
        if result {
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Set the fuel type of an engine design
    #[func]
    pub fn set_engine_design_fuel_type(&mut self, index: i32, fuel_type: i32) -> bool {
        if index < 0 {
            return false;
        }
        let ft = match crate::engine_design::FuelType::from_index(fuel_type as usize) {
            Some(ft) => ft,
            None => return false,
        };
        let result = self.state.player_company.set_engine_design_fuel_type(index as usize, ft);
        if result {
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Check if an engine design can be modified (only when Untested)
    #[func]
    pub fn can_modify_engine_design(&self, index: i32) -> bool {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            self.state.player_company.engine_designs[index as usize].head().can_modify()
        } else {
            false
        }
    }

    // ==========================================
    // Engine Design Stat Queries
    // ==========================================

    /// Get the scale of an engine design
    #[func]
    pub fn get_engine_design_scale(&self, index: i32) -> f64 {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            self.state.player_company.engine_designs[index as usize].head().scale
        } else {
            1.0
        }
    }

    /// Get the fuel type index of an engine design (0=Kerolox, 1=Hydrolox, 2=Solid)
    #[func]
    pub fn get_engine_design_fuel_type(&self, index: i32) -> i32 {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            self.state.player_company.engine_designs[index as usize].head().fuel_type().index() as i32
        } else {
            0
        }
    }

    /// Get the fuel type display name of an engine design
    #[func]
    pub fn get_engine_design_fuel_type_name(&self, index: i32) -> GString {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            GString::from(self.state.player_company.engine_designs[index as usize].head().fuel_type().display_name())
        } else {
            GString::from("")
        }
    }

    /// Get the thrust of an engine design (kN)
    #[func]
    pub fn get_engine_design_thrust(&self, index: i32) -> f64 {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let lineage = &self.state.player_company.engine_designs[index as usize];
            lineage.head().snapshot(index as usize, &lineage.name).thrust_kn
        } else {
            0.0
        }
    }

    /// Get the exhaust velocity of an engine design (m/s)
    #[func]
    pub fn get_engine_design_exhaust_velocity(&self, index: i32) -> f64 {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let lineage = &self.state.player_company.engine_designs[index as usize];
            lineage.head().snapshot(index as usize, &lineage.name).exhaust_velocity_ms
        } else {
            0.0
        }
    }

    /// Get the mass of an engine design (kg)
    #[func]
    pub fn get_engine_design_mass(&self, index: i32) -> f64 {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let lineage = &self.state.player_company.engine_designs[index as usize];
            lineage.head().snapshot(index as usize, &lineage.name).mass_kg
        } else {
            0.0
        }
    }

    /// Get the cost of an engine design ($)
    #[func]
    pub fn get_engine_design_cost(&self, index: i32) -> f64 {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let lineage = &self.state.player_company.engine_designs[index as usize];
            lineage.head().snapshot(index as usize, &lineage.name).base_cost
        } else {
            0.0
        }
    }

    // ==========================================
    // Engine Management (for Research UI)
    // ==========================================

    /// Get the number of engine designs
    #[func]
    pub fn get_engine_type_count(&self) -> i32 {
        self.state.player_company.engine_designs.len() as i32
    }

    /// Get the name of an engine design
    #[func]
    pub fn get_engine_type_name(&self, index: i32) -> GString {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            GString::from(self.state.player_company.engine_designs[index as usize].name.as_str())
        } else {
            GString::from("")
        }
    }

    /// Get the status of an engine design (includes flaw name if Fixing)
    #[func]
    pub fn get_engine_status(&self, index: i32) -> GString {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let design = self.state.player_company.engine_designs[index as usize].head();
            GString::from(design.status.display_name().as_str())
        } else {
            GString::from("")
        }
    }

    /// Get the base status of an engine design (without flaw name)
    #[func]
    pub fn get_engine_status_base(&self, index: i32) -> GString {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let design = self.state.player_company.engine_designs[index as usize].head();
            GString::from(design.status.name())
        } else {
            GString::from("")
        }
    }

    /// Get the progress of an engine design (0.0 to 1.0)
    #[func]
    pub fn get_engine_progress(&self, index: i32) -> f64 {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let design = self.state.player_company.engine_designs[index as usize].head();
            design.status.progress_fraction()
        } else {
            0.0
        }
    }

    /// Get names of discovered (unfixed) flaws for an engine design
    #[func]
    pub fn get_engine_unfixed_flaw_names(&self, index: i32) -> Array<GString> {
        let mut result = Array::new();
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let design = self.state.player_company.engine_designs[index as usize].head();
            for name in design.get_unfixed_flaw_names() {
                result.push(&GString::from(name.as_str()));
            }
        }
        result
    }

    /// Get names of fixed flaws for an engine design
    #[func]
    pub fn get_engine_fixed_flaw_names(&self, index: i32) -> Array<GString> {
        let mut result = Array::new();
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let design = self.state.player_company.engine_designs[index as usize].head();
            for name in design.get_fixed_flaw_names() {
                result.push(&GString::from(name.as_str()));
            }
        }
        result
    }

    /// Submit an engine design to testing (generates flaws if needed)
    #[func]
    pub fn submit_engine_to_testing(&mut self, index: i32) -> bool {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let idx = index as usize;
            let flaw_gen = &mut self.state.player_company.flaw_generator;
            let design = self.state.player_company.engine_designs[idx].head_mut();
            let result = design.submit_to_testing(flaw_gen, idx);
            if result {
                self.base_mut().emit_signal("designs_changed", &[]);
            }
            result
        } else {
            false
        }
    }

    /// Discover an engine flaw by ID in the game state's engine designs
    /// Called when a launch failure reveals an engine flaw
    /// Also auto-submits the engine to Testing if still Untested
    /// Returns the flaw name if found and newly discovered
    #[func]
    pub fn discover_engine_flaw_by_id(&mut self, flaw_id: i32) -> GString {
        use crate::engine::EngineStatus;

        if flaw_id < 0 {
            return GString::from("");
        }

        for idx in 0..self.state.player_company.engine_designs.len() {
            let flaw_gen = &mut self.state.player_company.flaw_generator;
            let design = self.state.player_company.engine_designs[idx].head_mut();

            // Check if this flaw belongs to this engine design
            let mut found_flaw_name = None;
            for flaw in design.active_flaws.iter_mut() {
                if flaw.id == flaw_id as u32 && !flaw.discovered {
                    flaw.discovered = true;
                    found_flaw_name = Some(flaw.name.clone());
                    break;
                }
            }

            if let Some(name) = found_flaw_name {
                // Auto-submit to testing if still Untested
                if matches!(design.status, EngineStatus::Untested) {
                    design.submit_to_testing(flaw_gen, idx);
                }
                // Transition to Fixing if currently Testing so teams work on the flaw
                if matches!(design.status, EngineStatus::Testing { .. }) {
                    if let Some(flaw_index) = design.get_next_unfixed_flaw() {
                        let flaw_name = design.active_flaws[flaw_index].name.clone();
                        design.status.start_fixing(flaw_name, flaw_index);
                    }
                }
                self.base_mut().emit_signal("designs_changed", &[]);
                return GString::from(name.as_str());
            }
        }

        GString::from("")
    }

    /// Get number of teams working on an engine design
    #[func]
    pub fn get_teams_on_engine_count(&self, engine_design_id: i32) -> i32 {
        if engine_design_id < 0 {
            return 0;
        }
        self.state.player_company.get_teams_on_engine(engine_design_id as usize).len() as i32
    }

    // ==========================================
    // Manufacturing Management
    // ==========================================

    /// Get total floor space
    #[func]
    pub fn get_floor_space_total(&self) -> i32 {
        self.state.player_company.manufacturing.floor_space_total as i32
    }

    /// Get floor space currently in use by active orders
    #[func]
    pub fn get_floor_space_in_use(&self) -> i32 {
        self.state.player_company.manufacturing.floor_space_in_use() as i32
    }

    /// Get floor space available for new orders
    #[func]
    pub fn get_floor_space_available(&self) -> i32 {
        self.state.player_company.manufacturing.floor_space_available() as i32
    }

    /// Get floor space currently under construction
    #[func]
    pub fn get_floor_space_under_construction(&self) -> i32 {
        self.state.player_company.manufacturing.floor_space_constructing() as i32
    }

    /// Get cost per unit of floor space
    #[func]
    pub fn get_floor_space_cost_per_unit(&self) -> f64 {
        crate::manufacturing::FLOOR_SPACE_COST_PER_UNIT
    }

    /// Buy floor space (deducts cost, starts construction)
    #[func]
    pub fn buy_floor_space(&mut self, units: i32) -> bool {
        if units <= 0 {
            return false;
        }
        self.sync_money_to_state();
        let result = self.state.player_company.buy_floor_space(units as usize);
        if result {
            self.sync_money_from_state();
            self.emit_money_changed();
            self.base_mut().emit_signal("manufacturing_changed", &[]);
        }
        result
    }

    /// Start an engine manufacturing order.
    /// Returns order_id (>0) on success, -1 on failure.
    /// On failure, call get_last_order_error() for the reason.
    #[func]
    pub fn start_engine_order(&mut self, engine_design_id: i32, revision_number: i32, quantity: i32) -> i32 {
        if engine_design_id < 0 || revision_number < 0 || quantity <= 0 {
            self.last_order_error = "Invalid parameters".to_string();
            return -1;
        }
        self.sync_money_to_state();
        match self.state.player_company.start_engine_order(
            engine_design_id as usize,
            revision_number as u32,
            quantity as u32,
        ) {
            Ok((order_id, _)) => {
                self.last_order_error.clear();
                self.sync_money_from_state();
                self.emit_money_changed();
                self.base_mut().emit_signal("manufacturing_changed", &[]);
                order_id as i32
            }
            Err(reason) => {
                self.last_order_error = reason.to_string();
                self.sync_money_from_state();
                -1
            }
        }
    }

    /// Start a rocket assembly order.
    /// Returns order_id (>0) on success, -1 on failure.
    /// On failure, call get_last_order_error() for the reason.
    #[func]
    pub fn start_rocket_order(&mut self, rocket_design_id: i32, revision_number: i32) -> i32 {
        if rocket_design_id < 0 || revision_number < 0 {
            self.last_order_error = "Invalid design or revision".to_string();
            return -1;
        }
        self.sync_money_to_state();
        match self.state.player_company.start_rocket_order(
            rocket_design_id as usize,
            revision_number as u32,
        ) {
            Ok((order_id, _)) => {
                self.last_order_error.clear();
                self.sync_money_from_state();
                self.emit_money_changed();
                self.base_mut().emit_signal("manufacturing_changed", &[]);
                self.base_mut().emit_signal("inventory_changed", &[]);
                order_id as i32
            }
            Err(reason) => {
                self.last_order_error = reason.to_string();
                self.sync_money_from_state();
                -1
            }
        }
    }

    /// Get the error message from the last failed order attempt
    #[func]
    pub fn get_last_order_error(&self) -> GString {
        GString::from(self.last_order_error.as_str())
    }

    /// Cancel a manufacturing order
    #[func]
    pub fn cancel_manufacturing_order(&mut self, order_id: i32) -> bool {
        if order_id < 0 {
            return false;
        }
        let result = self.state.player_company.cancel_manufacturing_order(order_id as u32);
        if result {
            self.base_mut().emit_signal("manufacturing_changed", &[]);
        }
        result
    }

    /// Get number of active manufacturing orders
    #[func]
    pub fn get_active_order_count(&self) -> i32 {
        self.state.player_company.manufacturing.active_orders.len() as i32
    }

    /// Get info about an active order as a dictionary
    #[func]
    pub fn get_order_info(&self, order_id: i32) -> Dictionary {
        let mut dict = Dictionary::new();
        if order_id < 0 {
            return dict;
        }
        if let Some(order) = self.state.player_company.manufacturing.get_order(order_id as u32) {
            dict.set("id", order.id as i32);
            dict.set("display_name", GString::from(order.display_name().as_str()));
            dict.set("progress", order.progress_fraction());
            dict.set("is_engine", order.is_engine_order());
            dict.set("total_work", order.total_work);
            dict.set("current_progress", order.progress);

            match &order.order_type {
                crate::manufacturing::ManufacturingOrderType::Engine { engine_design_id, quantity, completed, .. } => {
                    dict.set("engine_design_id", *engine_design_id as i32);
                    dict.set("quantity", *quantity as i32);
                    dict.set("completed", *completed as i32);
                }
                crate::manufacturing::ManufacturingOrderType::Rocket { rocket_design_id, .. } => {
                    dict.set("rocket_design_id", *rocket_design_id as i32);
                }
            }
        }
        dict
    }

    /// Get all active order IDs
    #[func]
    pub fn get_active_order_ids(&self) -> Array<i32> {
        let mut result = Array::new();
        for order in &self.state.player_company.manufacturing.active_orders {
            result.push(order.id as i32);
        }
        result
    }

    /// Get engine inventory as an array of dictionaries
    #[func]
    pub fn get_engine_inventory(&self) -> Array<Dictionary> {
        let mut result = Array::new();
        for entry in &self.state.player_company.manufacturing.engine_inventory {
            let mut dict = Dictionary::new();
            dict.set("engine_design_id", entry.engine_design_id as i32);
            dict.set("revision_number", entry.revision_number as i32);
            dict.set("name", GString::from(entry.snapshot.name.as_str()));
            dict.set("quantity", entry.quantity as i32);
            result.push(&dict);
        }
        result
    }

    /// Get rocket inventory as an array of dictionaries
    #[func]
    pub fn get_rocket_inventory(&self) -> Array<Dictionary> {
        let mut result = Array::new();
        for entry in &self.state.player_company.manufacturing.rocket_inventory {
            let mut dict = Dictionary::new();
            dict.set("rocket_design_id", entry.rocket_design_id as i32);
            dict.set("revision_number", entry.revision_number as i32);
            dict.set("serial_number", entry.serial_number as i32);
            dict.set("name", GString::from(entry.design_snapshot.name.as_str()));
            dict.set("mass_kg", entry.design_snapshot.total_wet_mass_kg());
            result.push(&dict);
        }
        result
    }

    /// Get the number of engines available for a given design ID
    #[func]
    pub fn get_engines_available_for_design(&self, engine_design_id: i32) -> i32 {
        if engine_design_id < 0 {
            return 0;
        }
        self.state.player_company.manufacturing.get_engines_available(engine_design_id as usize) as i32
    }

    /// Get engine material cost for a given engine design
    #[func]
    pub fn get_engine_material_cost(&self, index: i32) -> f64 {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let lineage = &self.state.player_company.engine_designs[index as usize];
            let snap = lineage.head().snapshot(index as usize, &lineage.name);
            crate::manufacturing::engine_material_cost(&snap)
        } else {
            0.0
        }
    }

    /// Get engine build work (team-days) for a given engine design
    #[func]
    pub fn get_engine_build_days(&self, index: i32) -> f64 {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let lineage = &self.state.player_company.engine_designs[index as usize];
            let snap = lineage.head().snapshot(index as usize, &lineage.name);
            crate::manufacturing::engine_build_work(&snap)
        } else {
            0.0
        }
    }

    /// Get total material cost for a rocket design (stages + integration, no engines)
    #[func]
    pub fn get_rocket_material_cost(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        self.state.player_company.get_rocket_design(index as usize)
            .map(|d| d.total_material_cost())
            .unwrap_or(0.0)
    }

    /// Get total assembly work (team-days) for a rocket design
    #[func]
    pub fn get_rocket_assembly_days(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        self.state.player_company.get_rocket_design(index as usize)
            .map(|d| d.total_assembly_work())
            .unwrap_or(0.0)
    }

    /// Get engines required for a rocket design as an array of dictionaries
    /// Each dict has: engine_design_id, count
    #[func]
    pub fn get_engines_required_for_rocket(&self, index: i32) -> Array<Dictionary> {
        let mut result = Array::new();
        if index < 0 {
            return result;
        }
        if let Some(design) = self.state.player_company.get_rocket_design(index as usize) {
            for (engine_design_id, count) in design.engines_required() {
                let mut dict = Dictionary::new();
                dict.set("engine_design_id", engine_design_id as i32);
                dict.set("count", count as i32);
                // Include engine name if available
                if engine_design_id < self.state.player_company.engine_designs.len() {
                    let name = &self.state.player_company.engine_designs[engine_design_id].name;
                    dict.set("name", GString::from(name.as_str()));
                }
                result.push(&dict);
            }
        }
        result
    }

    /// Get missing engines for a rocket design as an array of dictionaries
    /// Each dict has: engine_design_id, name, needed, available, deficit
    #[func]
    pub fn get_missing_engines_for_rocket(&self, index: i32) -> Array<Dictionary> {
        let mut result = Array::new();
        if index < 0 {
            return result;
        }
        if let Some(design) = self.state.player_company.get_rocket_design(index as usize) {
            for (engine_design_id, needed) in design.engines_required() {
                let available = self.state.player_company.manufacturing.get_engines_available(engine_design_id);
                let deficit = (needed as i32) - (available as i32);
                if deficit > 0 {
                    let mut dict = Dictionary::new();
                    dict.set("engine_design_id", engine_design_id as i32);
                    if engine_design_id < self.state.player_company.engine_designs.len() {
                        dict.set("name", GString::from(self.state.player_company.engine_designs[engine_design_id].name.as_str()));
                    } else {
                        dict.set("name", GString::from("Unknown"));
                    }
                    dict.set("needed", needed as i32);
                    dict.set("available", available as i32);
                    dict.set("deficit", deficit);
                    result.push(&dict);
                }
            }
        }
        result
    }

    /// Auto-order engines needed for a rocket design.
    /// Cuts revisions as needed and starts engine orders for deficit quantities.
    /// Returns total engines ordered, or -1 on failure.
    #[func]
    pub fn auto_order_engines_for_rocket(&mut self, index: i32) -> i32 {
        if index < 0 {
            return -1;
        }
        let design = match self.state.player_company.get_rocket_design(index as usize) {
            Some(d) => d.clone(),
            None => return -1,
        };

        let mut total_ordered: i32 = 0;

        for (engine_design_id, needed) in design.engines_required() {
            let available = self.state.player_company.manufacturing.get_engines_available(engine_design_id);
            let deficit = (needed as i32) - (available as i32);
            if deficit <= 0 {
                continue;
            }

            if engine_design_id >= self.state.player_company.engine_designs.len() {
                return -1;
            }

            // Cut a revision for manufacturing
            let rev = self.state.player_company.engine_designs[engine_design_id]
                .cut_revision("auto-mfg");

            // Start the engine order
            self.sync_money_to_state();
            match self.state.player_company.start_engine_order(
                engine_design_id,
                rev,
                deficit as u32,
            ) {
                Ok(_) => {
                    self.sync_money_from_state();
                    self.emit_money_changed();
                    total_ordered += deficit;
                }
                Err(reason) => {
                    self.last_order_error = reason.to_string();
                    self.sync_money_from_state();
                    return -1;
                }
            }
        }

        if total_ordered > 0 {
            self.base_mut().emit_signal("manufacturing_changed", &[]);
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        total_ordered
    }

    /// Assign a team to work on a manufacturing order
    #[func]
    pub fn assign_team_to_manufacturing(&mut self, team_id: i32, order_id: i32) -> bool {
        if team_id < 0 || order_id < 0 {
            return false;
        }
        let result = self.state.player_company.assign_team_to_manufacturing(team_id as u32, order_id as u32);
        if result {
            self.base_mut().emit_signal("teams_changed", &[]);
        }
        result
    }

    /// Get number of teams working on a manufacturing order
    #[func]
    pub fn get_teams_on_order_count(&self, order_id: i32) -> i32 {
        if order_id < 0 {
            return 0;
        }
        self.state.player_company.get_teams_on_order(order_id as u32).len() as i32
    }

    /// Auto-assign idle manufacturing teams across active orders.
    /// Returns the number of teams assigned.
    #[func]
    pub fn auto_assign_manufacturing_teams(&mut self) -> i32 {
        let assigned = self.state.player_company.auto_assign_manufacturing_teams();
        if assigned > 0 {
            self.base_mut().emit_signal("manufacturing_changed", &[]);
        }
        assigned as i32
    }

    #[func]
    pub fn get_auto_assign_manufacturing(&self) -> bool {
        self.state.player_company.auto_assign_manufacturing
    }

    #[func]
    pub fn set_auto_assign_manufacturing(&mut self, enabled: bool) {
        self.state.player_company.auto_assign_manufacturing = enabled;
        if enabled {
            // Immediately assign any idle teams when toggled on
            let assigned = self.state.player_company.auto_assign_manufacturing_teams();
            if assigned > 0 {
                self.base_mut().emit_signal("manufacturing_changed", &[]);
            }
        }
    }

    /// Cut a revision for an engine design and return the revision number
    #[func]
    pub fn cut_engine_revision(&mut self, index: i32, label: GString) -> i32 {
        if index < 0 || (index as usize) >= self.state.player_company.engine_designs.len() {
            return -1;
        }
        let rev = self.state.player_company.engine_designs[index as usize]
            .cut_revision(&label.to_string());
        self.base_mut().emit_signal("designs_changed", &[]);
        rev as i32
    }

    /// Cut a revision for a rocket design and return the revision number
    #[func]
    pub fn cut_rocket_revision(&mut self, index: i32, label: GString) -> i32 {
        if index < 0 || (index as usize) >= self.state.player_company.rocket_designs.len() {
            return -1;
        }
        let rev = self.state.player_company.rocket_designs[index as usize]
            .cut_revision(&label.to_string());
        self.base_mut().emit_signal("designs_changed", &[]);
        rev as i32
    }

    /// Check if a rocket design has engines available for manufacturing
    #[func]
    pub fn has_engines_for_rocket(&self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        if let Some(design) = self.state.player_company.get_rocket_design(index as usize) {
            self.state.player_company.manufacturing.has_engines_for_rocket(design)
        } else {
            false
        }
    }

    /// Consume a rocket from inventory for launch (by serial number)
    /// Returns true if successful
    #[func]
    pub fn consume_rocket_for_launch(&mut self, serial_number: i32) -> bool {
        if serial_number < 0 {
            return false;
        }
        let result = self.state.player_company.manufacturing.consume_rocket(serial_number as u32);
        if result.is_some() {
            self.base_mut().emit_signal("inventory_changed", &[]);
            true
        } else {
            false
        }
    }

    /// Check if there's a manufactured rocket in inventory matching the current design
    #[func]
    pub fn has_rocket_for_current_design(&self) -> bool {
        let design_id = match self.current_rocket_design_id {
            Some(id) => id,
            None => return false,
        };
        self.state.player_company.manufacturing.rocket_inventory
            .iter()
            .any(|entry| entry.rocket_design_id == design_id)
    }

    /// Consume a manufactured rocket matching the current design for launch
    /// Returns true if a rocket was found and consumed
    #[func]
    pub fn consume_rocket_for_current_design(&mut self) -> bool {
        let design_id = match self.current_rocket_design_id {
            Some(id) => id,
            None => return false,
        };
        // Find the first rocket matching this design
        let serial = self.state.player_company.manufacturing.rocket_inventory
            .iter()
            .find(|entry| entry.rocket_design_id == design_id)
            .map(|entry| entry.serial_number);
        if let Some(serial_number) = serial {
            let result = self.state.player_company.manufacturing.consume_rocket(serial_number);
            if result.is_some() {
                self.base_mut().emit_signal("inventory_changed", &[]);
                return true;
            }
        }
        false
    }

    // ==========================================
    // Game Management
    // ==========================================

    /// Start a new game
    #[func]
    pub fn new_game(&mut self) {
        self.state = GameState::new();
        self.current_rocket_design_id = None;
        self.finance.bind_mut().reset();
        self.emit_money_changed();
        self.base_mut().emit_signal("contracts_changed", &[]);
    }

    // ==========================================
    // Resource System (for Finance tab and cost display)
    // ==========================================

    /// Number of resource types
    #[func]
    pub fn get_resource_count(&self) -> i32 {
        crate::resources::RESOURCE_COUNT as i32
    }

    /// Get resource display name by index
    #[func]
    pub fn get_resource_name(&self, index: i32) -> GString {
        crate::resources::Resource::from_index(index as usize)
            .map(|r| r.display_name())
            .unwrap_or("Unknown")
            .into()
    }

    /// Get resource price per kg by index
    #[func]
    pub fn get_resource_price(&self, index: i32) -> f64 {
        crate::resources::Resource::from_index(index as usize)
            .map(|r| r.price_per_kg())
            .unwrap_or(0.0)
    }

    /// Get cost to hire an engineering team
    #[func]
    pub fn get_engineering_hire_cost(&self) -> f64 {
        crate::engineering_team::ENGINEERING_HIRE_COST
    }

    /// Get cost to hire a manufacturing team
    #[func]
    pub fn get_manufacturing_hire_cost(&self) -> f64 {
        crate::engineering_team::MANUFACTURING_HIRE_COST
    }

    /// Get engineering team monthly salary
    #[func]
    pub fn get_engineering_team_salary(&self) -> f64 {
        crate::engineering_team::ENGINEERING_TEAM_SALARY
    }

    /// Get manufacturing team monthly salary
    #[func]
    pub fn get_manufacturing_team_salary(&self) -> f64 {
        crate::engineering_team::MANUFACTURING_TEAM_SALARY
    }

    /// Get pad upgrade costs for all levels (indices 0-3 = upgrades to levels 2-5)
    #[func]
    pub fn get_pad_upgrade_costs(&self) -> Array<f64> {
        let mut result = Array::new();
        result.push(50_000_000.0);
        result.push(150_000_000.0);
        result.push(400_000_000.0);
        result.push(1_000_000_000.0);
        result
    }

    /// Get a summary of the current game state for saving
    #[func]
    pub fn get_save_summary(&self) -> GString {
        let summary = format!(
            "Turn: {} | Money: {} | Launches: {}/{} ({:.0}%)",
            self.state.turn,
            format_money(self.finance.bind().get_money()),
            self.state.player_company.successful_launches,
            self.state.player_company.total_launches,
            self.state.player_company.success_rate()
        );
        GString::from(summary.as_str())
    }
}
