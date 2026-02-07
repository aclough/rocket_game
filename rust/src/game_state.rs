use crate::company::Company;
use crate::engineering_team::WorkEvent;
use crate::time_system::TimeSystem;

// Re-export for backwards compatibility with game_manager imports
pub use crate::company::{CONTRACT_REFRESH_COST, CONTRACTS_TO_SHOW};

/// The overall game state
#[derive(Debug, Clone)]
pub struct GameState {
    /// Current turn/month
    pub turn: u32,
    /// Current game day (advances with actions)
    pub current_day: u32,
    /// Starting year for date display
    pub start_year: u32,
    /// Time system for continuous simulation
    pub time_system: TimeSystem,
    /// The player's company
    pub player_company: Company,
}

impl GameState {
    /// Create a new game with starting conditions
    pub fn new() -> Self {
        Self {
            turn: 1,
            current_day: 1,
            start_year: 2001,
            time_system: TimeSystem::new(),
            player_company: Company::new(),
        }
    }

    // ==========================================
    // Date/Time Management (Legacy API - delegates to time_system)
    // ==========================================

    /// Advance game time by a number of days (legacy API)
    /// For continuous time, use advance_time() instead
    pub fn advance_days(&mut self, days: u32) {
        self.current_day += days;
        self.time_system.current_day = self.current_day;
    }

    /// Get formatted date string (e.g., "Day 45, Year 2001")
    pub fn get_date_string(&self) -> String {
        self.time_system.get_date_string()
    }

    /// Get current year
    pub fn get_current_year(&self) -> u32 {
        self.time_system.get_current_year()
    }

    /// Get day of the current year (1-365)
    pub fn get_day_of_year(&self) -> u32 {
        self.time_system.get_day_of_year()
    }

    // ==========================================
    // Time System Management
    // ==========================================

    /// Advance time by delta_seconds and process work
    /// Returns events that occurred during this time
    pub fn advance_time(&mut self, delta_seconds: f64) -> Vec<WorkEvent> {
        let days_passed = self.time_system.advance(delta_seconds);

        // Keep current_day in sync with time_system
        self.current_day = self.time_system.current_day;

        let mut events = Vec::new();

        // Process each day
        for _ in 0..days_passed {
            let salary_due = self.time_system.check_salary_due();
            let day_events = self.player_company.process_day(salary_due);
            events.extend(day_events);
        }

        events
    }

    /// Toggle time pause state
    pub fn toggle_time_pause(&mut self) {
        self.time_system.toggle_pause();
    }

    /// Check if time is paused
    pub fn is_time_paused(&self) -> bool {
        self.time_system.paused
    }

    /// Set time pause state explicitly
    pub fn set_time_paused(&mut self, paused: bool) {
        self.time_system.set_paused(paused);
    }

    /// Get days until next salary payment
    pub fn days_until_salary(&self) -> u32 {
        self.time_system.days_until_salary()
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::Destination;
    use crate::engine::costs;

    #[test]
    fn test_new_game_state() {
        let state = GameState::new();
        assert_eq!(state.player_company.money, costs::STARTING_BUDGET);
        assert_eq!(state.turn, 1);
        assert_eq!(state.player_company.available_contracts.len(), CONTRACTS_TO_SHOW);
        assert!(state.player_company.active_contract.is_none());
    }

    #[test]
    fn test_select_contract() {
        let mut state = GameState::new();
        let contract_id = state.player_company.available_contracts[0].id;

        assert!(state.player_company.select_contract(contract_id));
        assert!(state.player_company.active_contract.is_some());
        assert_eq!(state.player_company.active_contract.as_ref().unwrap().id, contract_id);
        assert_eq!(state.player_company.available_contracts.len(), CONTRACTS_TO_SHOW - 1);
    }

    #[test]
    fn test_select_invalid_contract() {
        let mut state = GameState::new();
        assert!(!state.player_company.select_contract(99999));
        assert!(state.player_company.active_contract.is_none());
    }

    #[test]
    fn test_complete_contract() {
        let mut state = GameState::new();
        let contract_id = state.player_company.available_contracts[0].id;
        state.player_company.select_contract(contract_id);

        let initial_money = state.player_company.money;
        let rocket_cost = state.player_company.get_rocket_cost();
        let reward = state.player_company.active_contract.as_ref().unwrap().reward;

        let earned = state.player_company.complete_contract();

        assert_eq!(earned, reward);
        // Money = initial - rocket_cost + reward
        assert_eq!(state.player_company.money, initial_money - rocket_cost + reward);
        assert!(state.player_company.active_contract.is_none());
        assert_eq!(state.player_company.completed_contracts.len(), 1);
    }

    #[test]
    fn test_refresh_contracts() {
        let mut state = GameState::new();
        let old_ids: Vec<u32> = state.player_company.available_contracts.iter().map(|c| c.id).collect();

        assert!(state.player_company.refresh_contracts());

        let new_ids: Vec<u32> = state.player_company.available_contracts.iter().map(|c| c.id).collect();
        assert_ne!(old_ids, new_ids);
        assert!(state.player_company.money < costs::STARTING_BUDGET);
    }

    #[test]
    fn test_target_delta_v() {
        let mut state = GameState::new();

        // Default target is LEO
        assert_eq!(state.player_company.get_target_delta_v(), Destination::LEO.required_delta_v());

        // Find a GTO contract and select it
        state.player_company.generate_contracts(20); // Generate more to ensure we get variety
        if let Some(gto_contract) = state
            .player_company
            .available_contracts
            .iter()
            .find(|c| c.destination == Destination::GTO)
        {
            let id = gto_contract.id;
            state.player_company.select_contract(id);
            assert_eq!(state.player_company.get_target_delta_v(), Destination::GTO.required_delta_v());
        }
    }

    #[test]
    fn test_abandon_contract() {
        let mut state = GameState::new();
        let initial_count = state.player_company.available_contracts.len();
        let contract_id = state.player_company.available_contracts[0].id;

        state.player_company.select_contract(contract_id);
        assert_eq!(state.player_company.available_contracts.len(), initial_count - 1);

        state.player_company.abandon_contract();
        assert!(state.player_company.active_contract.is_none());
        assert_eq!(state.player_company.available_contracts.len(), initial_count);
    }

    #[test]
    fn test_success_rate() {
        let mut state = GameState::new();
        assert_eq!(state.player_company.success_rate(), 0.0);

        state.player_company.total_launches = 10;
        state.player_company.successful_launches = 7;
        assert!((state.player_company.success_rate() - 70.0).abs() < 0.001);
    }

    #[test]
    fn test_date_tracking() {
        let mut state = GameState::new();
        assert_eq!(state.current_day, 1);
        assert_eq!(state.start_year, 2001);
        assert_eq!(state.get_date_string(), "Day 1, Year 2001");

        state.advance_days(30);
        assert_eq!(state.current_day, 31);
        assert_eq!(state.get_date_string(), "Day 31, Year 2001");

        // Advance to next year
        state.advance_days(335); // Day 366 = Day 1 of year 2
        assert_eq!(state.current_day, 366);
        assert_eq!(state.get_current_year(), 2002);
        assert_eq!(state.get_day_of_year(), 1);
    }

    #[test]
    fn test_fame_tracking() {
        let mut state = GameState::new();
        assert_eq!(state.player_company.fame, 0.0);
        assert_eq!(state.player_company.get_fame_tier(), 0);
        assert_eq!(state.player_company.get_fame_tier_name(), "Unknown");

        state.player_company.adjust_fame(15.0);
        assert_eq!(state.player_company.fame, 15.0);
        assert_eq!(state.player_company.get_fame_tier(), 1);
        assert_eq!(state.player_company.get_fame_tier_name(), "Newcomer");

        // Fame can't go negative
        state.player_company.adjust_fame(-20.0);
        assert_eq!(state.player_company.fame, 0.0);
    }

    #[test]
    fn test_launch_site_integration() {
        let state = GameState::new();
        assert_eq!(state.player_company.launch_site.pad_level, 1);
        assert_eq!(state.player_company.launch_site.max_launch_mass_kg(), 300_000.0);
    }
}
