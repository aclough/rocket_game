//! Teams: hiring engineering and manufacturing teams, and what the
//! roster costs and has free.

use super::*;

impl Company {
    /// Put an engineering team on the roster without charging for it.
    pub(super) fn enroll_team(&mut self, name: String, balance_cfg: &BalanceConfig) {
        let id = TeamId(self.next_team_id);
        self.next_team_id += 1;
        let team = EngineeringTeam::new(id, name, balance_cfg.costs.engineering_monthly_salary);
        self.teams.push(team);
    }

    /// Hire a new engineering team, paying the hiring fee. Returns the
    /// event if successful.
    pub fn hire_team(&mut self, name: String, balance_cfg: &BalanceConfig) -> Option<GameEvent> {
        self.debit(balance_cfg.costs.engineering_hiring_cost);
        self.enroll_team(name.clone(), balance_cfg);
        Some(GameEvent::TeamHired { name })
    }

    /// Total number of teams.
    pub fn team_count(&self) -> usize {
        self.teams.len()
    }

    /// Number of engineering teams not assigned to any project.
    pub fn unassigned_team_count(&self) -> u32 {
        let assigned: u32 = self.projects().map(|p| p.teams_assigned()).sum();
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
        self.debit(balance_cfg.costs.manufacturing_hiring_cost);
        let id = TeamId(self.next_team_id);
        self.next_team_id += 1;
        let team = ManufacturingTeam::new(id, name.clone(), balance_cfg.costs.manufacturing_monthly_salary);
        self.manufacturing_teams.push(team);
        Some(GameEvent::ManufacturingTeamHired { name })
    }
}
