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
    current_saved_design_index: Option<usize>,
    /// Player finances - single source of truth for money
    finance: Gd<PlayerFinance>,
}

#[godot_api]
impl INode for GameManager {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            state: GameState::new(),
            current_saved_design_index: None,
            finance: Gd::from_init_fn(PlayerFinance::init),
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
        let reward = self.state.player_company.complete_contract();
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
        self.state.player_company.fail_contract();
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

    /// Get number of saved designs
    #[func]
    pub fn get_saved_design_count(&self) -> i32 {
        self.state.player_company.get_saved_design_count() as i32
    }

    /// Get design name at index
    #[func]
    pub fn get_saved_design_name(&self, index: i32) -> GString {
        if index < 0 {
            return GString::from("");
        }
        GString::from(
            self.state
                .player_company
                .get_saved_design(index as usize)
                .map(|d| d.name.as_str())
                .unwrap_or("")
        )
    }

    /// Get design stage count at index
    #[func]
    pub fn get_saved_design_stage_count(&self, index: i32) -> i32 {
        if index < 0 {
            return 0;
        }
        self.state
            .player_company
            .get_saved_design(index as usize)
            .map(|d| d.stage_count() as i32)
            .unwrap_or(0)
    }

    /// Get design total delta-v at index
    #[func]
    pub fn get_saved_design_delta_v(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        self.state
            .player_company
            .get_saved_design(index as usize)
            .map(|d| d.total_effective_delta_v())
            .unwrap_or(0.0)
    }

    /// Get design total cost at index
    #[func]
    pub fn get_saved_design_cost(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        self.state
            .player_company
            .get_saved_design(index as usize)
            .map(|d| d.total_cost())
            .unwrap_or(0.0)
    }

    /// Get design total wet mass at index (in kg)
    #[func]
    pub fn get_saved_design_mass(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        self.state
            .player_company
            .get_saved_design(index as usize)
            .map(|d| d.total_wet_mass_kg())
            .unwrap_or(0.0)
    }

    /// Get design estimated success rate at index
    #[func]
    pub fn get_saved_design_success_rate(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        self.state
            .player_company
            .get_saved_design(index as usize)
            .map(|d| d.estimate_success_rate_with_flaws())
            .unwrap_or(0.0)
    }

    /// Check if a saved design has generated flaws
    #[func]
    pub fn saved_design_has_flaws(&self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        self.state
            .player_company
            .get_saved_design(index as usize)
            .map(|d| d.has_flaws_generated())
            .unwrap_or(false)
    }

    /// Get count of discovered flaws for a saved design
    #[func]
    pub fn get_saved_design_discovered_flaw_count(&self, index: i32) -> i32 {
        if index < 0 {
            return 0;
        }
        self.state
            .player_company
            .get_saved_design(index as usize)
            .map(|d| d.get_discovered_flaw_count() as i32)
            .unwrap_or(0)
    }

    /// Get count of fixed flaws for a saved design
    #[func]
    pub fn get_saved_design_fixed_flaw_count(&self, index: i32) -> i32 {
        if index < 0 {
            return 0;
        }
        self.state
            .player_company
            .get_saved_design(index as usize)
            .map(|d| d.get_fixed_flaw_count() as i32)
            .unwrap_or(0)
    }

    /// Get names of discovered (but not fixed) flaws for a saved design
    #[func]
    pub fn get_saved_design_unfixed_flaw_names(&self, index: i32) -> Array<GString> {
        let mut result = Array::new();
        if index < 0 {
            return result;
        }
        if let Some(design) = self.state.player_company.get_saved_design(index as usize) {
            for flaw in &design.active_flaws {
                if flaw.discovered && !flaw.fixed {
                    result.push(&GString::from(flaw.name.as_str()));
                }
            }
        }
        result
    }

    /// Get names of fixed flaws for a saved design
    #[func]
    pub fn get_saved_design_fixed_flaw_names(&self, index: i32) -> Array<GString> {
        let mut result = Array::new();
        if index < 0 {
            return result;
        }
        if let Some(design) = self.state.player_company.get_saved_design(index as usize) {
            for flaw in &design.active_flaws {
                if flaw.fixed {
                    result.push(&GString::from(flaw.name.as_str()));
                }
            }
        }
        result
    }

    /// Save current design to saved list
    #[func]
    pub fn save_current_design(&mut self) -> i32 {
        let index = self.state.player_company.save_current_design();
        self.base_mut().emit_signal("designs_changed", &[]);
        index as i32
    }

    /// Save current design with a specific name
    #[func]
    pub fn save_design_as(&mut self, name: GString) -> i32 {
        let index = self.state.player_company.save_design_as(&name.to_string());
        self.base_mut().emit_signal("designs_changed", &[]);
        index as i32
    }

    /// Load a saved design into the working design
    #[func]
    pub fn load_design(&mut self, index: i32) -> bool {
        if index < 0 {
            self.current_saved_design_index = None;
            return false;
        }
        let result = self.state.player_company.load_design(index as usize);
        if result {
            // Set budget to current player money
            self.state.player_company.rocket_design.budget = self.finance.bind().get_money();
            self.current_saved_design_index = Some(index as usize);
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Refresh the current working design from the saved design
    /// This re-syncs flaw discoveries and other changes made by engineering teams
    #[func]
    pub fn refresh_current_design(&mut self) -> bool {
        if let Some(index) = self.current_saved_design_index {
            self.state.player_company.load_design(index);
            self.state.player_company.rocket_design.budget = self.finance.bind().get_money();
            true
        } else {
            false
        }
    }

    /// Update a saved design with the current working design
    #[func]
    pub fn update_saved_design(&mut self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.state.player_company.update_saved_design(index as usize);
        if result {
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Update the currently edited saved design with the working design
    /// Call this after launch to save testing_spent reset and flaw changes
    #[func]
    pub fn update_current_saved_design(&mut self) {
        if let Some(index) = self.current_saved_design_index {
            self.state.player_company.update_saved_design(index);
            self.base_mut().emit_signal("designs_changed", &[]);
        }
    }

    /// Delete a saved design
    #[func]
    pub fn delete_saved_design(&mut self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.state.player_company.delete_saved_design(index as usize);
        if result {
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Rename a saved design
    #[func]
    pub fn rename_saved_design(&mut self, index: i32, new_name: GString) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.state.player_company.rename_saved_design(index as usize, &new_name.to_string());
        if result {
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Duplicate a saved design
    #[func]
    pub fn duplicate_saved_design(&mut self, index: i32) -> i32 {
        if index < 0 {
            return -1;
        }
        match self.state.player_company.duplicate_saved_design(index as usize) {
            Some(new_index) => {
                self.base_mut().emit_signal("designs_changed", &[]);
                new_index as i32
            }
            None => -1,
        }
    }

    /// Create a new empty design
    #[func]
    pub fn create_new_design(&mut self) {
        self.state.player_company.create_new_design();
        // Set budget to current player money
        self.state.player_company.rocket_design.budget = self.finance.bind().get_money();
        self.current_saved_design_index = None;
    }

    /// Create a new design based on the default template
    #[func]
    pub fn create_default_design(&mut self) {
        self.state.player_company.create_default_design();
        // Set budget to current player money
        self.state.player_company.rocket_design.budget = self.finance.bind().get_money();
        self.current_saved_design_index = None;
    }

    /// Get the current working design name
    #[func]
    pub fn get_current_design_name(&self) -> GString {
        GString::from(self.state.player_company.rocket_design.name.as_str())
    }

    /// Set the current working design name
    #[func]
    pub fn set_current_design_name(&mut self, name: GString) {
        self.state.player_company.rocket_design.name = name.to_string();
    }

    /// Copy design from a RocketDesigner node into the game state
    /// Call this before saving to ensure the game state has the latest design
    /// Also updates the saved design if one is being edited
    #[func]
    pub fn sync_design_from(&mut self, designer: Gd<RocketDesigner>) {
        self.state.player_company.rocket_design = designer.bind().get_design_clone();

        // Update the saved design if we're editing one
        if let Some(index) = self.current_saved_design_index {
            self.state.player_company.update_saved_design(index);
            self.base_mut().emit_signal("designs_changed", &[]);
        }
    }

    /// Sync engine flaw data from designer back to Company
    /// Call this after testing in the designer to persist flaw discoveries
    #[func]
    pub fn sync_engine_flaws_from_designer(&mut self, designer: Gd<RocketDesigner>) {
        // The designer's get_design_clone() already merges engine flaws into the design.
        // Engine flaws with engine_design_id are restored to the Company's engine_designs
        // when the design is loaded back. For direct sync we extract from the design clone.
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
                        // Move from active to fixed if not already there
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

    /// Save the current design if it hasn't been saved yet
    /// Call this before launching a new (unsaved) design
    /// Returns the index of the saved design
    #[func]
    pub fn ensure_design_saved(&mut self) -> i32 {
        if let Some(index) = self.current_saved_design_index {
            // Already saved, just return the index
            index as i32
        } else {
            // Check if a design with this name already exists
            let current_name = &self.state.player_company.rocket_design.name;
            if let Some(existing_index) = self.state.player_company.saved_designs.iter().position(|d| &d.name == current_name) {
                // Update existing design instead of creating duplicate
                self.state.player_company.update_saved_design(existing_index);
                self.current_saved_design_index = Some(existing_index);
                self.base_mut().emit_signal("designs_changed", &[]);
                existing_index as i32
            } else {
                // Save new design
                let index = self.state.player_company.save_current_design();
                self.current_saved_design_index = Some(index);
                self.base_mut().emit_signal("designs_changed", &[]);
                index as i32
            }
        }
    }

    /// Copy design from game state to a RocketDesigner node
    /// Call this after loading a design to update the designer
    /// Sets the design's budget to the current player money
    /// Also syncs engine data (snapshots + flaws) from Company
    #[func]
    pub fn sync_design_to(&self, mut designer: Gd<RocketDesigner>) {
        let mut design = self.state.player_company.rocket_design.clone();
        design.budget = self.finance.bind().get_money();

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
            .get_saved_design(index as usize)
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
            .get_saved_design(index as usize)
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
            .get_saved_design(index as usize)
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
            .get_saved_design(index as usize)
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
            .get_saved_design(index as usize)
            .map(|d| d.design_status.can_launch())
            .unwrap_or(false)
    }

    /// Get the index of the currently edited design (-1 if none)
    #[func]
    pub fn get_current_design_index(&self) -> i32 {
        self.current_saved_design_index.map(|i| i as i32).unwrap_or(-1)
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

    /// Submit a saved design to engineering
    /// Returns true if successful
    #[func]
    pub fn submit_design_to_engineering(&mut self, index: i32) -> bool {
        if index < 0 || index >= self.state.player_company.saved_designs.len() as i32 {
            return false;
        }
        let result = self.state.player_company.saved_designs[index as usize].submit_to_engineering();
        if result {
            // Generate flaws for the design when submitting to engineering
            let design = &mut self.state.player_company.saved_designs[index as usize];
            design.generate_flaws(&mut self.state.player_company.flaw_generator);
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Reset a saved design back to Specification status
    #[func]
    pub fn reset_design_to_specification(&mut self, index: i32) -> bool {
        if index < 0 || index >= self.state.player_company.saved_designs.len() as i32 {
            return false;
        }
        self.state.player_company.saved_designs[index as usize].reset_to_specification();
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

        // Convert events to Godot dictionaries
        let mut result = Array::new();
        for event in events {
            let dict = self.work_event_to_dict(&event);
            result.push(&dict);

            // Emit signal for each event
            self.emit_work_event(&event);
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
                design_index,
                phase_name,
            } => {
                dict.set("type", "design_phase_complete");
                dict.set("design_index", *design_index as i32);
                dict.set("phase_name", GString::from(phase_name.as_str()));
            }
            WorkEvent::DesignFlawDiscovered {
                design_index,
                flaw_name,
            } => {
                dict.set("type", "design_flaw_discovered");
                dict.set("design_index", *design_index as i32);
                dict.set("flaw_name", GString::from(flaw_name.as_str()));
            }
            WorkEvent::DesignFlawFixed {
                design_index,
                flaw_name,
            } => {
                dict.set("type", "design_flaw_fixed");
                dict.set("design_index", *design_index as i32);
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
            WorkEvent::SalaryDeducted { amount } => {
                dict.set("type", "salary_deducted");
                dict.set("amount", *amount);
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
            crate::engineering_team::WorkEvent::TeamRampedUp { .. } => "team_ramped_up",
            crate::engineering_team::WorkEvent::SalaryDeducted { .. } => "salary_deducted",
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
        self.state.player_company.can_launch_rocket_at_site()
    }

    /// Get propellant storage capacity
    #[func]
    pub fn get_propellant_storage(&self) -> f64 {
        self.state.player_company.launch_site.propellant_storage_kg
    }

    // ==========================================
    // Engineering Team Management
    // ==========================================

    /// Get the number of engineering teams
    #[func]
    pub fn get_team_count(&self) -> i32 {
        self.state.player_company.get_team_count() as i32
    }

    /// Hire a new engineering team
    /// Returns the team ID
    #[func]
    pub fn hire_team(&mut self) -> i32 {
        let id = self.state.player_company.hire_team();
        self.base_mut().emit_signal("teams_changed", &[]);
        id as i32
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
    /// Keys: "type" (string: "none", "design", "engine"), and type-specific data
    #[func]
    pub fn get_team_assignment(&self, team_id: i32) -> Dictionary {
        use crate::engineering_team::TeamAssignment;

        let mut dict = Dictionary::new();

        if let Some(team) = self.state.player_company.get_team(team_id as u32) {
            match &team.assignment {
                None => {
                    dict.set("type", "none");
                }
                Some(TeamAssignment::RocketDesign { design_index, .. }) => {
                    dict.set("type", "design");
                    dict.set("design_index", *design_index as i32);
                }
                Some(TeamAssignment::EngineDesign { engine_design_id, .. }) => {
                    dict.set("type", "engine");
                    dict.set("engine_design_id", *engine_design_id as i32);
                }
            }
        } else {
            dict.set("type", "none");
        }

        dict
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

    /// Submit an engine design to refining (generates flaws if needed)
    #[func]
    pub fn submit_engine_to_refining(&mut self, index: i32) -> bool {
        if index >= 0 && (index as usize) < self.state.player_company.engine_designs.len() {
            let idx = index as usize;
            let flaw_gen = &mut self.state.player_company.flaw_generator;
            let design = self.state.player_company.engine_designs[idx].head_mut();
            let result = design.submit_to_refining(flaw_gen, idx);
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
    /// Also auto-submits the engine to Refining if still Untested
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
                // Auto-submit to refining if still Untested so teams can fix it
                if matches!(design.status, EngineStatus::Untested) {
                    design.submit_to_refining(flaw_gen, idx);
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
    // Game Management
    // ==========================================

    /// Start a new game
    #[func]
    pub fn new_game(&mut self) {
        self.state = GameState::new();
        self.current_saved_design_index = None;
        self.finance.bind_mut().reset();
        self.emit_money_changed();
        self.base_mut().emit_signal("contracts_changed", &[]);
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
