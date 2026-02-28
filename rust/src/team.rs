use serde::{Serialize, Deserialize};

/// Monthly salary for an engineering team (~8-10 engineers).
pub const TEAM_MONTHLY_SALARY: f64 = 150_000.0;

/// One-time hiring cost (1x monthly salary).
pub const TEAM_HIRING_COST: f64 = 150_000.0;

/// Unique identifier for an engineering team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TeamId(pub u64);

/// An engineering team that can be assigned to engine projects.
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
            monthly_salary: TEAM_MONTHLY_SALARY,
        }
    }
}

/// Calculate effective work rate for multiple teams on one project.
/// Multiple teams give sqrt(num_teams) work units per day.
pub fn effective_work_rate(num_teams: u32) -> f64 {
    (num_teams as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_team() {
        let team = EngineeringTeam::new(TeamId(1), "Alpha".into());
        assert_eq!(team.id, TeamId(1));
        assert_eq!(team.name, "Alpha");
        assert_eq!(team.monthly_salary, TEAM_MONTHLY_SALARY);
    }

    #[test]
    fn test_effective_work_rate() {
        assert!((effective_work_rate(0) - 0.0).abs() < 0.001);
        assert!((effective_work_rate(1) - 1.0).abs() < 0.001);
        assert!((effective_work_rate(4) - 2.0).abs() < 0.001);
        assert!((effective_work_rate(9) - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_hiring_cost_equals_salary() {
        assert_eq!(TEAM_HIRING_COST, TEAM_MONTHLY_SALARY);
    }
}
