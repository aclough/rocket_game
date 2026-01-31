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
        // Connect PlayerFinance money_changed signal to forward through GameManager
        let mut finance = self.finance.clone();
        let callable = self.base().callable("_on_finance_money_changed");
        finance.connect("money_changed", &callable);
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

    /// Callback to forward money_changed signal from PlayerFinance
    #[func]
    fn _on_finance_money_changed(&mut self, new_amount: f64) {
        self.base_mut().emit_signal("money_changed", &[Variant::from(new_amount)]);
    }

    /// Get the PlayerFinance resource (single source of truth for money)
    #[func]
    pub fn get_finance(&self) -> Gd<PlayerFinance> {
        self.finance.clone()
    }

    /// Sync money from PlayerFinance to GameState (call before GameState operations)
    fn sync_money_to_state(&mut self) {
        self.state.money = self.finance.bind().get_money();
    }

    /// Sync money from GameState to PlayerFinance (call after GameState operations that modify money)
    fn sync_money_from_state(&mut self) {
        self.finance.bind_mut().set_money(self.state.money);
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
        self.sync_money_to_state();
        if self.state.refresh_contracts() {
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
        self.sync_money_to_state();
        let reward = self.state.complete_contract();
        if reward > 0.0 {
            self.sync_money_from_state();
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
        self.state.fail_contract();
        self.sync_money_from_state();
        self.base_mut().emit_signal("contract_failed", &[]);
    }

    /// Pay for the rocket (deduct cost from money)
    #[func]
    pub fn pay_for_rocket(&mut self, cost: f64) -> bool {
        self.finance.bind_mut().deduct(cost)
    }

    /// Add money (for testing or cheats)
    #[func]
    pub fn add_money(&mut self, amount: f64) {
        self.finance.bind_mut().add(amount);
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
            self.current_saved_design_index = None;
            return false;
        }
        let result = self.state.load_design(index as usize);
        if result {
            // Set budget to current player money
            self.state.rocket_design.budget = self.finance.bind().get_money();
            self.current_saved_design_index = Some(index as usize);
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

    /// Update the currently edited saved design with the working design
    /// Call this after launch to save testing_spent reset and flaw changes
    #[func]
    pub fn update_current_saved_design(&mut self) {
        if let Some(index) = self.current_saved_design_index {
            self.state.update_saved_design(index);
            self.base_mut().emit_signal("designs_changed", &[]);
        }
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
        // Set budget to current player money
        self.state.rocket_design.budget = self.finance.bind().get_money();
        self.current_saved_design_index = None;
    }

    /// Create a new design based on the default template
    #[func]
    pub fn create_default_design(&mut self) {
        self.state.create_default_design();
        // Set budget to current player money
        self.state.rocket_design.budget = self.finance.bind().get_money();
        self.current_saved_design_index = None;
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
    /// Also updates the saved design if one is being edited
    #[func]
    pub fn sync_design_from(&mut self, designer: Gd<RocketDesigner>) {
        self.state.rocket_design = designer.bind().get_design_clone();

        // Update the saved design if we're editing one
        if let Some(index) = self.current_saved_design_index {
            self.state.update_saved_design(index);
            self.base_mut().emit_signal("designs_changed", &[]);
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
            let current_name = &self.state.rocket_design.name;
            if let Some(existing_index) = self.state.saved_designs.iter().position(|d| &d.name == current_name) {
                // Update existing design instead of creating duplicate
                self.state.update_saved_design(existing_index);
                self.current_saved_design_index = Some(existing_index);
                self.base_mut().emit_signal("designs_changed", &[]);
                existing_index as i32
            } else {
                // Save new design
                let index = self.state.save_current_design();
                self.current_saved_design_index = Some(index);
                self.base_mut().emit_signal("designs_changed", &[]);
                index as i32
            }
        }
    }

    /// Copy design from game state to a RocketDesigner node
    /// Call this after loading a design to update the designer
    /// Sets the design's budget to the current player money
    #[func]
    pub fn sync_design_to(&self, mut designer: Gd<RocketDesigner>) {
        let mut design = self.state.rocket_design.clone();
        design.budget = self.finance.bind().get_money();
        designer.bind_mut().set_design(design);
        designer.bind_mut().set_finance(self.finance.clone());
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
        self.base_mut().emit_signal("contracts_changed", &[]);
    }

    /// Get a summary of the current game state for saving
    #[func]
    pub fn get_save_summary(&self) -> GString {
        let summary = format!(
            "Turn: {} | Money: {} | Launches: {}/{} ({:.0}%)",
            self.state.turn,
            format_money(self.finance.bind().get_money()),
            self.state.successful_launches,
            self.state.total_launches,
            self.state.success_rate()
        );
        GString::from(summary.as_str())
    }
}
