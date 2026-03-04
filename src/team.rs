use serde::{Serialize, Deserialize};

/// Monthly salary for an engineering team (~8-10 engineers).
pub const ENGINEERING_MONTHLY_SALARY: f64 = 150_000.0;

/// One-time hiring cost for an engineering team (1x monthly salary).
pub const ENGINEERING_HIRING_COST: f64 = 150_000.0;

/// Monthly salary for a manufacturing team (~20-25 workers).
pub const MANUFACTURING_MONTHLY_SALARY: f64 = 300_000.0;

/// One-time hiring cost for a manufacturing team (3x monthly salary).
pub const MANUFACTURING_HIRING_COST: f64 = 900_000.0;

// Backward-compat aliases used by game_state.rs
pub const TEAM_MONTHLY_SALARY: f64 = ENGINEERING_MONTHLY_SALARY;
pub const TEAM_HIRING_COST: f64 = ENGINEERING_HIRING_COST;

/// Unique identifier for a team (engineering or manufacturing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TeamId(pub u64);

/// An engineering team that can be assigned to engine/rocket design projects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineeringTeam {
    pub id: TeamId,
    pub name: String,
    pub monthly_salary: f64,
}

impl EngineeringTeam {
    pub fn new(id: TeamId, name: String) -> Self {
        EngineeringTeam {
            id,
            name,
            monthly_salary: ENGINEERING_MONTHLY_SALARY,
        }
    }
}

/// A manufacturing team that can be assigned to manufacturing orders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManufacturingTeam {
    pub id: TeamId,
    pub name: String,
    pub monthly_salary: f64,
}

impl ManufacturingTeam {
    pub fn new(id: TeamId, name: String) -> Self {
        ManufacturingTeam {
            id,
            name,
            monthly_salary: MANUFACTURING_MONTHLY_SALARY,
        }
    }
}

/// Calculate effective work rate for multiple engineering teams on one project.
/// Multiple teams give sqrt(num_teams) work units per day.
pub fn effective_work_rate(num_teams: u32) -> f64 {
    (num_teams as f64).sqrt()
}

/// Calculate effective work rate for multiple manufacturing teams on one order.
/// Manufacturing teams scale as n^0.85 (better than engineering's sqrt).
pub fn manufacturing_work_rate(num_teams: u32) -> f64 {
    (num_teams as f64).powf(0.85)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_engineering_team() {
        let team = EngineeringTeam::new(TeamId(1), "Alpha".into());
        assert_eq!(team.id, TeamId(1));
        assert_eq!(team.name, "Alpha");
        assert_eq!(team.monthly_salary, ENGINEERING_MONTHLY_SALARY);
    }

    #[test]
    fn test_new_manufacturing_team() {
        let team = ManufacturingTeam::new(TeamId(1), "Factory A".into());
        assert_eq!(team.id, TeamId(1));
        assert_eq!(team.name, "Factory A");
        assert_eq!(team.monthly_salary, MANUFACTURING_MONTHLY_SALARY);
    }

    #[test]
    fn test_effective_work_rate() {
        assert!((effective_work_rate(0) - 0.0).abs() < 0.001);
        assert!((effective_work_rate(1) - 1.0).abs() < 0.001);
        assert!((effective_work_rate(4) - 2.0).abs() < 0.001);
        assert!((effective_work_rate(9) - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_manufacturing_work_rate() {
        assert!((manufacturing_work_rate(0) - 0.0).abs() < 0.001);
        assert!((manufacturing_work_rate(1) - 1.0).abs() < 0.001);
        // n^0.85 should be higher than sqrt(n) for n>1
        let mfg_4 = manufacturing_work_rate(4);
        let eng_4 = effective_work_rate(4);
        assert!(mfg_4 > eng_4, "Mfg rate {} should exceed eng rate {} at 4 teams", mfg_4, eng_4);
    }

    #[test]
    fn test_manufacturing_costs() {
        assert_eq!(MANUFACTURING_MONTHLY_SALARY, 300_000.0);
        assert_eq!(MANUFACTURING_HIRING_COST, 900_000.0);
        assert_eq!(MANUFACTURING_HIRING_COST, MANUFACTURING_MONTHLY_SALARY * 3.0);
    }

    #[test]
    fn test_hiring_cost_equals_salary() {
        assert_eq!(ENGINEERING_HIRING_COST, ENGINEERING_MONTHLY_SALARY);
    }
}
