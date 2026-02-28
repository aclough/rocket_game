use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::engine::{EngineCycle, EngineId};
use crate::engine_project::{EngineProject, EngineProjectId, PropellantPreset, WorkEvent};
use crate::event::{EventLog, GameEvent};
use crate::rocket::RocketDesign;
use crate::seed::GameSeed;
use crate::team::{EngineeringTeam, TeamId, TEAM_HIRING_COST};
use crate::third_party::{self, ThirdPartyEngine};

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

/// A player's rocket company.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Company {
    pub name: String,
    pub money: f64,
    pub next_team_id: u64,
    pub next_engine_id: u64,
    pub next_project_id: u64,
    pub next_flaw_id: u64,
    pub teams: Vec<EngineeringTeam>,
    pub engine_projects: Vec<EngineProject>,
    pub third_party_catalog: Vec<ThirdPartyEngine>,
    pub rocket_designs: Vec<RocketDesign>,
}

impl Company {
    pub fn new(name: String, starting_money: f64, seed: &GameSeed) -> Self {
        let catalog = third_party::generate_starter_engines(seed);
        Company {
            name,
            money: starting_money,
            next_team_id: 1,
            next_engine_id: 1,
            next_project_id: 1,
            next_flaw_id: 1,
            teams: Vec::new(),
            engine_projects: Vec::new(),
            third_party_catalog: catalog,
            rocket_designs: Vec::new(),
        }
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

    /// Number of teams not assigned to any project.
    pub fn unassigned_team_count(&self) -> u32 {
        let assigned: u32 = self.engine_projects.iter()
            .map(|p| p.teams_assigned)
            .sum();
        (self.teams.len() as u32).saturating_sub(assigned)
    }

    /// Total monthly salary cost for all teams.
    pub fn monthly_salary_cost(&self) -> f64 {
        self.teams.iter().map(|t| t.monthly_salary).sum()
    }

    /// Start a new engine design project. Returns the event if successful.
    pub fn start_engine_project(
        &mut self,
        name: String,
        cycle: EngineCycle,
        preset: PropellantPreset,
        scale: f64,
        use_vacuum_isp: bool,
    ) -> Option<GameEvent> {
        let project_id = EngineProjectId(self.next_project_id);
        let engine_id = EngineId(self.next_engine_id);
        self.next_project_id += 1;
        self.next_engine_id += 1;

        let project = EngineProject::new(
            project_id, engine_id, name.clone(),
            cycle, preset, scale, use_vacuum_isp,
        )?;
        self.engine_projects.push(project);
        Some(GameEvent::EngineDesignStarted { engine_name: name })
    }

    /// Add a team to a project. Returns true if successful.
    pub fn add_team_to_project(&mut self, project_index: usize) -> bool {
        if self.unassigned_team_count() == 0 || project_index >= self.engine_projects.len() {
            return false;
        }
        let project = &mut self.engine_projects[project_index];
        if project.is_third_party {
            return false; // can't assign teams to third-party engines
        }
        project.teams_assigned += 1;
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

    /// Purchase a third-party engine from the catalog.
    pub fn purchase_third_party(&mut self, catalog_index: usize, current_date: GameDate) -> Option<GameEvent> {
        if catalog_index >= self.third_party_catalog.len() {
            return None;
        }
        let entry = &self.third_party_catalog[catalog_index];
        if current_date < entry.available_from {
            return None;
        }
        let cost = entry.purchase_cost;
        let mut project = entry.project.clone();
        // Give it a new project ID in the player's space
        project.project_id = EngineProjectId(self.next_project_id);
        self.next_project_id += 1;
        let name = project.design.name.clone();

        self.money -= cost;
        self.engine_projects.push(project);
        Some(GameEvent::EnginePurchased { engine_name: name, cost })
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
}

impl GameState {
    pub fn new(company_name: String, starting_money: f64, seed_value: u64) -> Self {
        let start = GameDate::default_start();
        let mut event_log = EventLog::new(EVENT_LOG_SIZE);
        event_log.push(start, GameEvent::GameStarted);
        let seed = GameSeed::new(seed_value);

        GameState {
            date: start,
            start_date: start,
            player_company: Company::new(company_name, starting_money, &seed),
            event_log,
            seed,
            speed: GameSpeed::Paused,
            previous_speed: GameSpeed::Normal,
        }
    }

    /// Advance the game by one day. Returns events generated this tick.
    pub fn advance_day(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();

        self.date = self.date.next_day();

        // Process daily work on engine projects
        let rng = &mut self.seed.contingent_rng;
        let next_flaw_id = &mut self.player_company.next_flaw_id;
        for project in &mut self.player_company.engine_projects {
            let engine_name = project.design.name.clone();
            let work_events = project.apply_daily_work(rng, next_flaw_id);
            for we in work_events {
                let evt = match we {
                    WorkEvent::DesignComplete { flaw_count } =>
                        GameEvent::EngineDesignComplete { engine_name: engine_name.clone(), flaw_count },
                    WorkEvent::TestingCycleComplete => continue,
                    WorkEvent::FlawDiscovered { flaw_description } =>
                        GameEvent::FlawDiscovered { engine_name: engine_name.clone(), flaw_description },
                    WorkEvent::RevisionComplete =>
                        GameEvent::RevisionComplete { engine_name: engine_name.clone() },
                };
                self.event_log.push(self.date, evt.clone());
                events.push(evt);
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
        }

        // Future: process manufacturing, flights, research, contracts

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_game_state() {
        let gs = GameState::new("SpaceCorp".into(), 200_000_000.0, 42);
        assert_eq!(gs.date, GameDate::default_start());
        assert_eq!(gs.player_company.name, "SpaceCorp");
        assert_eq!(gs.player_company.money, 200_000_000.0);
        assert_eq!(gs.speed, GameSpeed::Paused);
        assert_eq!(gs.elapsed_days(), 0);
        // Should have GameStarted event
        assert_eq!(gs.event_log.len(), 1);
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
        let recent = gs.event_log.recent(3);
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
        assert_eq!(gs.player_company.team_count(), 0);
        gs.player_company.hire_team("Alpha".into());
        assert_eq!(gs.player_company.team_count(), 1);
        assert_eq!(gs.player_company.money, 1_000_000.0 - 150_000.0);
    }

    #[test]
    fn test_salary_deduction() {
        let mut gs = GameState::new("Test".into(), 1_000_000.0, 1);
        gs.player_company.hire_team("Alpha".into());

        // Advance to Feb 1 (31 days)
        for _ in 0..31 {
            gs.advance_day();
        }
        // Should have paid 1 month salary
        let expected = 1_000_000.0 - 150_000.0 - 150_000.0; // hiring + first month
        assert!((gs.player_company.money - expected).abs() < 0.01);
    }

    #[test]
    fn test_negative_money_allowed() {
        let mut gs = GameState::new("Test".into(), 100_000.0, 1);
        gs.player_company.hire_team("Alpha".into()); // -150K, now -50K
        assert!(gs.player_company.money < 0.0);
        // Should still work, just go negative
        for _ in 0..31 {
            gs.advance_day();
        }
        // Should have deducted another salary on top
        assert!(gs.player_company.money < -50_000.0);
    }

    #[test]
    fn test_start_engine_project() {
        let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
        let evt = gs.player_company.start_engine_project(
            "Kestrel".into(),
            crate::engine::EngineCycle::GasGenerator,
            crate::engine_project::PropellantPreset::Kerolox,
            1.0,
            true,
        );
        assert!(evt.is_some());
        assert_eq!(gs.player_company.engine_projects.len(), 1);
    }

    #[test]
    fn test_team_assignment() {
        let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
        gs.player_company.hire_team("Alpha".into());
        gs.player_company.start_engine_project(
            "Kestrel".into(),
            crate::engine::EngineCycle::GasGenerator,
            crate::engine_project::PropellantPreset::Kerolox,
            1.0,
            true,
        );

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
    fn test_cant_assign_to_third_party() {
        let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
        gs.player_company.hire_team("Alpha".into());
        // Purchase a third-party engine first
        let date = gs.date;
        gs.player_company.purchase_third_party(0, date);
        // Find the third-party project
        let tp_idx = gs.player_company.engine_projects.iter()
            .position(|p| p.is_third_party)
            .unwrap();
        assert!(!gs.player_company.add_team_to_project(tp_idx));
    }

    #[test]
    fn test_third_party_catalog() {
        let gs = GameState::new("Test".into(), 200_000_000.0, 42);
        assert_eq!(gs.player_company.third_party_catalog.len(), 3);
    }

    #[test]
    fn test_purchase_third_party() {
        let mut gs = GameState::new("Test".into(), 200_000_000.0, 42);
        let initial_money = gs.player_company.money;
        let date = gs.date;
        let cost = gs.player_company.third_party_catalog[0].purchase_cost;

        let evt = gs.player_company.purchase_third_party(0, date);
        assert!(evt.is_some());
        assert_eq!(gs.player_company.engine_projects.len(), 1);
        assert!((gs.player_company.money - (initial_money - cost)).abs() < 0.01);
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
            true,
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
}
