use godot::prelude::*;

use crate::contract::{format_money, Destination};
use crate::game_state::{GameState, CONTRACT_REFRESH_COST};
use crate::rocket_designer::RocketDesigner;

/// Godot node for managing overall game state
#[derive(GodotClass)]
#[class(base=Node)]
pub struct GameManager {
    base: Base<Node>,
    state: GameState,
}

#[godot_api]
impl INode for GameManager {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            state: GameState::new(),
        }
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

    // ==========================================
    // Money and Budget
    // ==========================================

    /// Get current money
    #[func]
    pub fn get_money(&self) -> f64 {
        self.state.money
    }

    /// Get money formatted for display (e.g., "$500M")
    #[func]
    pub fn get_money_formatted(&self) -> GString {
        GString::from(format_money(self.state.money).as_str())
    }

    /// Get the current turn number
    #[func]
    pub fn get_turn(&self) -> i32 {
        self.state.turn as i32
    }

    /// Get total launches
    #[func]
    pub fn get_total_launches(&self) -> i32 {
        self.state.total_launches as i32
    }

    /// Get successful launches
    #[func]
    pub fn get_successful_launches(&self) -> i32 {
        self.state.successful_launches as i32
    }

    /// Get success rate as percentage
    #[func]
    pub fn get_success_rate(&self) -> f64 {
        self.state.success_rate()
    }

    /// Check if player is bankrupt
    #[func]
    pub fn is_bankrupt(&self) -> bool {
        self.state.is_bankrupt()
    }

    // ==========================================
    // Contract Management
    // ==========================================

    /// Get the number of available contracts
    #[func]
    pub fn get_contract_count(&self) -> i32 {
        self.state.available_contracts.len() as i32
    }

    /// Get contract ID at index
    #[func]
    pub fn get_contract_id(&self, index: i32) -> i32 {
        self.state
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
            .available_contracts
            .get(index as usize)
            .map(|c| c.destination.required_delta_v())
            .unwrap_or(0.0)
    }

    /// Get contract payload mass at index
    #[func]
    pub fn get_contract_payload(&self, index: i32) -> f64 {
        self.state
            .available_contracts
            .get(index as usize)
            .map(|c| c.payload_mass_kg)
            .unwrap_or(0.0)
    }

    /// Get contract reward at index
    #[func]
    pub fn get_contract_reward(&self, index: i32) -> f64 {
        self.state
            .available_contracts
            .get(index as usize)
            .map(|c| c.reward)
            .unwrap_or(0.0)
    }

    /// Get contract reward formatted at index
    #[func]
    pub fn get_contract_reward_formatted(&self, index: i32) -> GString {
        let reward_str = self.state
            .available_contracts
            .get(index as usize)
            .map(|c| format_money(c.reward))
            .unwrap_or_default();
        GString::from(reward_str.as_str())
    }

    /// Select a contract by ID
    #[func]
    pub fn select_contract(&mut self, contract_id: i32) -> bool {
        if self.state.select_contract(contract_id as u32) {
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
        self.state.can_refresh_contracts()
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
        if self.state.refresh_contracts() {
            let money = self.state.money;
            self.base_mut()
                .emit_signal("money_changed", &[Variant::from(money)]);
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
        self.state.active_contract.is_some()
    }

    /// Get active contract name
    #[func]
    pub fn get_active_contract_name(&self) -> GString {
        GString::from(
            self.state
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
                .active_contract
                .as_ref()
                .map(|c| c.destination.display_name())
                .unwrap_or("")
        )
    }

    /// Get active contract required delta-v
    #[func]
    pub fn get_active_contract_delta_v(&self) -> f64 {
        self.state.get_target_delta_v()
    }

    /// Get active contract payload mass
    #[func]
    pub fn get_active_contract_payload(&self) -> f64 {
        self.state.get_payload_mass()
    }

    /// Get active contract reward
    #[func]
    pub fn get_active_contract_reward(&self) -> f64 {
        self.state
            .active_contract
            .as_ref()
            .map(|c| c.reward)
            .unwrap_or(0.0)
    }

    /// Get active contract reward formatted
    #[func]
    pub fn get_active_contract_reward_formatted(&self) -> GString {
        let reward_str = self.state
            .active_contract
            .as_ref()
            .map(|c| format_money(c.reward))
            .unwrap_or_default();
        GString::from(reward_str.as_str())
    }

    /// Abandon the current contract
    #[func]
    pub fn abandon_contract(&mut self) {
        self.state.abandon_contract();
        self.base_mut().emit_signal("contracts_changed", &[]);
    }

    // ==========================================
    // Mission Completion
    // ==========================================

    /// Complete the current contract (call after successful launch)
    #[func]
    pub fn complete_contract(&mut self) -> f64 {
        let reward = self.state.complete_contract();
        if reward > 0.0 {
            let money = self.state.money;
            self.base_mut()
                .emit_signal("contract_completed", &[Variant::from(reward)]);
            self.base_mut()
                .emit_signal("money_changed", &[Variant::from(money)]);
            self.base_mut().emit_signal("contracts_changed", &[]);
        }
        reward
    }

    /// Record a failed launch
    #[func]
    pub fn fail_contract(&mut self) {
        self.state.fail_contract();
        self.base_mut().emit_signal("contract_failed", &[]);
    }

    /// Pay for the rocket (deduct cost from money)
    #[func]
    pub fn pay_for_rocket(&mut self, cost: f64) -> bool {
        if self.state.money >= cost {
            self.state.money -= cost;
            let money = self.state.money;
            self.base_mut()
                .emit_signal("money_changed", &[Variant::from(money)]);
            true
        } else {
            false
        }
    }

    /// Add money (for testing or cheats)
    #[func]
    pub fn add_money(&mut self, amount: f64) {
        self.state.money += amount;
        let money = self.state.money;
        self.base_mut()
            .emit_signal("money_changed", &[Variant::from(money)]);
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
        self.state.get_saved_design_count() as i32
    }

    /// Get design name at index
    #[func]
    pub fn get_saved_design_name(&self, index: i32) -> GString {
        if index < 0 {
            return GString::from("");
        }
        GString::from(
            self.state
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
            .get_saved_design(index as usize)
            .map(|d| d.total_cost())
            .unwrap_or(0.0)
    }

    /// Get design estimated success rate at index
    #[func]
    pub fn get_saved_design_success_rate(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        self.state
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
            .get_saved_design(index as usize)
            .map(|d| d.get_fixed_flaw_count() as i32)
            .unwrap_or(0)
    }

    /// Save current design to saved list
    #[func]
    pub fn save_current_design(&mut self) -> i32 {
        let index = self.state.save_current_design();
        self.base_mut().emit_signal("designs_changed", &[]);
        index as i32
    }

    /// Save current design with a specific name
    #[func]
    pub fn save_design_as(&mut self, name: GString) -> i32 {
        let index = self.state.save_design_as(&name.to_string());
        self.base_mut().emit_signal("designs_changed", &[]);
        index as i32
    }

    /// Load a saved design into the working design
    #[func]
    pub fn load_design(&mut self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.state.load_design(index as usize);
        if result {
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Update a saved design with the current working design
    #[func]
    pub fn update_saved_design(&mut self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.state.update_saved_design(index as usize);
        if result {
            self.base_mut().emit_signal("designs_changed", &[]);
        }
        result
    }

    /// Delete a saved design
    #[func]
    pub fn delete_saved_design(&mut self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.state.delete_saved_design(index as usize);
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
        let result = self.state.rename_saved_design(index as usize, &new_name.to_string());
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
        match self.state.duplicate_saved_design(index as usize) {
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
        self.state.create_new_design();
    }

    /// Create a new design based on the default template
    #[func]
    pub fn create_default_design(&mut self) {
        self.state.create_default_design();
    }

    /// Get the current working design name
    #[func]
    pub fn get_current_design_name(&self) -> GString {
        GString::from(self.state.rocket_design.name.as_str())
    }

    /// Set the current working design name
    #[func]
    pub fn set_current_design_name(&mut self, name: GString) {
        self.state.rocket_design.name = name.to_string();
    }

    /// Copy design from a RocketDesigner node into the game state
    /// Call this before saving to ensure the game state has the latest design
    #[func]
    pub fn sync_design_from(&mut self, designer: Gd<RocketDesigner>) {
        self.state.rocket_design = designer.bind().get_design_clone();
    }

    /// Copy design from game state to a RocketDesigner node
    /// Call this after loading a design to update the designer
    #[func]
    pub fn sync_design_to(&self, mut designer: Gd<RocketDesigner>) {
        designer.bind_mut().set_design(self.state.rocket_design.clone());
    }

    // ==========================================
    // Game Management
    // ==========================================

    /// Start a new game
    #[func]
    pub fn new_game(&mut self) {
        self.state = GameState::new();
        let money = self.state.money;
        self.base_mut()
            .emit_signal("money_changed", &[Variant::from(money)]);
        self.base_mut().emit_signal("contracts_changed", &[]);
    }

    /// Get a summary of the current game state for saving
    #[func]
    pub fn get_save_summary(&self) -> GString {
        let summary = format!(
            "Turn: {} | Money: {} | Launches: {}/{} ({:.0}%)",
            self.state.turn,
            format_money(self.state.money),
            self.state.successful_launches,
            self.state.total_launches,
            self.state.success_rate()
        );
        GString::from(summary.as_str())
    }
}
