use godot::prelude::*;

use crate::engine::costs;

/// Manages player finances - single source of truth for money
/// This is a Godot Resource so it can be shared between nodes
#[derive(GodotClass)]
#[class(base=Resource)]
pub struct PlayerFinance {
    base: Base<Resource>,
    money: f64,
}

#[godot_api]
impl IResource for PlayerFinance {
    fn init(base: Base<Resource>) -> Self {
        Self {
            base,
            money: costs::STARTING_BUDGET,
        }
    }
}

#[godot_api]
impl PlayerFinance {
    // Note: money_changed signal is emitted by GameManager, not PlayerFinance,
    // to avoid re-entrancy issues with Gd<T>::bind_mut()

    /// Get current money
    #[func]
    pub fn get_money(&self) -> f64 {
        self.money
    }

    /// Set money directly (used for game load/reset)
    /// Note: Does NOT emit signal - caller (GameManager) is responsible for emitting
    #[func]
    pub fn set_money(&mut self, amount: f64) {
        self.money = amount;
    }

    /// Add money (rewards, etc.)
    /// Note: Does NOT emit signal - caller (GameManager) is responsible for emitting
    #[func]
    pub fn add(&mut self, amount: f64) {
        self.money += amount;
    }

    /// Check if player can afford a cost
    #[func]
    pub fn can_afford(&self, amount: f64) -> bool {
        self.money >= amount
    }

    /// Deduct money if affordable, returns true if successful
    /// Note: Does NOT emit signal - caller (GameManager) is responsible for emitting
    #[func]
    pub fn deduct(&mut self, amount: f64) -> bool {
        if self.money >= amount {
            self.money -= amount;
            true
        } else {
            false
        }
    }

    /// Deduct money without checking (for cases where check was already done)
    /// Use with caution - can result in negative balance
    /// Note: Does NOT emit signal - caller (GameManager) is responsible for emitting
    #[func]
    pub fn deduct_unchecked(&mut self, amount: f64) {
        self.money -= amount;
    }

    /// Reset to starting budget (for new game)
    /// Note: Does NOT emit signal - caller (GameManager) is responsible for emitting
    #[func]
    pub fn reset(&mut self) {
        self.money = costs::STARTING_BUDGET;
    }

    // ==========================================
    // Cost constants exposed to GDScript
    // ==========================================

    #[func]
    pub fn get_engine_test_cost(&self) -> f64 {
        costs::ENGINE_TEST_COST
    }

    #[func]
    pub fn get_rocket_test_cost(&self) -> f64 {
        costs::ROCKET_TEST_COST
    }

    #[func]
    pub fn get_flaw_fix_cost(&self) -> f64 {
        costs::FLAW_FIX_COST
    }

    #[func]
    pub fn can_afford_engine_test(&self) -> bool {
        self.can_afford(costs::ENGINE_TEST_COST)
    }

    #[func]
    pub fn can_afford_rocket_test(&self) -> bool {
        self.can_afford(costs::ROCKET_TEST_COST)
    }

    #[func]
    pub fn can_afford_flaw_fix(&self) -> bool {
        self.can_afford(costs::FLAW_FIX_COST)
    }
}
