use serde::{Serialize, Deserialize};

// Salaries and hiring costs live in `balance_config::CostsConfig`.

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
    pub fn new(id: TeamId, name: String, monthly_salary: f64) -> Self {
        EngineeringTeam {
            id,
            name,
            monthly_salary,
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
    pub fn new(id: TeamId, name: String, monthly_salary: f64) -> Self {
        ManufacturingTeam {
            id,
            name,
            monthly_salary,
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
    use crate::balance_config::CostsConfig;

    #[test]
    fn test_new_engineering_team() {
        let costs = CostsConfig::default();
        let team = EngineeringTeam::new(TeamId(1), "Alpha".into(), costs.engineering_monthly_salary);
        assert_eq!(team.id, TeamId(1));
        assert_eq!(team.name, "Alpha");
        assert_eq!(team.monthly_salary, costs.engineering_monthly_salary);
    }

    #[test]
    fn test_new_manufacturing_team() {
        let costs = CostsConfig::default();
        let team = ManufacturingTeam::new(TeamId(1), "Factory A".into(), costs.manufacturing_monthly_salary);
        assert_eq!(team.id, TeamId(1));
        assert_eq!(team.name, "Factory A");
        assert_eq!(team.monthly_salary, costs.manufacturing_monthly_salary);
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
    fn test_default_manufacturing_costs() {
        let costs = CostsConfig::default();
        assert_eq!(costs.manufacturing_monthly_salary, 300_000.0);
        assert_eq!(costs.manufacturing_hiring_cost, 900_000.0);
        assert_eq!(costs.manufacturing_hiring_cost, costs.manufacturing_monthly_salary * 3.0);
    }

    #[test]
    fn test_default_hiring_cost_equals_salary() {
        let costs = CostsConfig::default();
        assert_eq!(costs.engineering_hiring_cost, costs.engineering_monthly_salary);
    }
}
