/// Engineering team system for continuous time-based work
/// Teams work on rocket designs and engines over time

/// Engineering team monthly salary (~8-10 engineers, $150K/mo)
pub const ENGINEERING_TEAM_SALARY: f64 = 150_000.0;

/// Manufacturing team monthly salary (~20-25 workers + equipment, $300K/mo)
pub const MANUFACTURING_TEAM_SALARY: f64 = 300_000.0;

/// Cost to hire an engineering team (1x monthly salary)
pub const ENGINEERING_HIRE_COST: f64 = ENGINEERING_TEAM_SALARY;

/// Cost to hire a manufacturing team (3x monthly salary)
pub const MANUFACTURING_HIRE_COST: f64 = MANUFACTURING_TEAM_SALARY * 3.0;

/// Type of team — determines what work they can do
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TeamType {
    /// Can do design and testing work (rocket designs, engine designs)
    Engineering,
    /// Can do manufacturing work (build engines, assemble rockets)
    Manufacturing,
}

/// Days for a new team to ramp up before full productivity
pub const RAMP_UP_DAYS: u32 = 7;

/// Work units required for detailed engineering phase
pub const DETAILED_ENGINEERING_WORK: f64 = 30.0;

/// Work units per testing cycle (active work that advances progress)
pub const TESTING_WORK: f64 = 30.0;

/// Represents a team that can work on designs, engines, or manufacturing
#[derive(Debug, Clone)]
pub struct EngineeringTeam {
    /// Unique team identifier
    pub id: u32,
    /// Team name for display
    pub name: String,
    /// Type of team (Engineering or Manufacturing)
    pub team_type: TeamType,
    /// Current assignment (if any)
    pub assignment: Option<TeamAssignment>,
    /// Days remaining in ramp-up period (0 = fully ramped)
    pub ramp_up_days_remaining: u32,
    /// Monthly salary for this team
    pub monthly_salary: f64,
}

impl EngineeringTeam {
    /// Create a new team with the given ID and type
    /// New teams start fully available (not ramping up)
    /// Ramp-up only begins when assigned to work
    pub fn new(id: u32, team_type: TeamType) -> Self {
        let prefix = match team_type {
            TeamType::Engineering => "Eng Team",
            TeamType::Manufacturing => "Mfg Team",
        };
        Self {
            id,
            name: format!("{} {}", prefix, id),
            team_type,
            assignment: None,
            ramp_up_days_remaining: 0,  // Available immediately, ramp-up on assignment
            monthly_salary: match team_type {
                TeamType::Engineering => ENGINEERING_TEAM_SALARY,
                TeamType::Manufacturing => MANUFACTURING_TEAM_SALARY,
            },
        }
    }

    /// Check if team is currently ramping up
    pub fn is_ramping_up(&self) -> bool {
        self.ramp_up_days_remaining > 0
    }

    /// Get productivity multiplier (0.0 during ramp-up, 1.0 after)
    pub fn productivity(&self) -> f64 {
        if self.is_ramping_up() {
            0.0
        } else {
            1.0
        }
    }

    /// Process a day of work, reducing ramp-up time if applicable
    pub fn process_day(&mut self) {
        if self.ramp_up_days_remaining > 0 {
            self.ramp_up_days_remaining -= 1;
        }
    }

    /// Assign team to work on something
    pub fn assign(&mut self, assignment: TeamAssignment) {
        self.assignment = Some(assignment);
        // Reset ramp-up when assigned to new work
        self.ramp_up_days_remaining = RAMP_UP_DAYS;
    }

    /// Unassign team from current work
    pub fn unassign(&mut self) {
        self.assignment = None;
    }
}

/// What a team is currently working on
#[derive(Debug, Clone, PartialEq)]
pub enum TeamAssignment {
    /// Working on a rocket design
    RocketDesign {
        rocket_design_id: usize,
        work_phase: DesignWorkPhase,
    },
    /// Working on an engine design
    EngineDesign {
        engine_design_id: usize,
        work_phase: EngineWorkPhase,
    },
    /// Working on a manufacturing order
    Manufacturing {
        order_id: crate::manufacturing::ManufacturingOrderId,
    },
}

/// Work phases for rocket designs
#[derive(Debug, Clone, PartialEq)]
pub enum DesignWorkPhase {
    /// Detailed engineering work after specification
    DetailedEngineering {
        progress: f64,
        total_work: f64,
    },
    /// Testing phase - looking for and fixing flaws
    Testing {
        progress: f64,
        total_work: f64,
    },
}

/// Work phases for engine types
#[derive(Debug, Clone, PartialEq)]
pub enum EngineWorkPhase {
    /// Testing - looking for flaws
    Testing {
        progress: f64,
        total_work: f64,
    },
}

/// Events generated during work processing
#[derive(Debug, Clone)]
pub enum WorkEvent {
    /// A design phase has completed
    DesignPhaseComplete {
        rocket_design_id: usize,
        phase_name: String,
    },
    /// A design flaw was discovered during testing
    DesignFlawDiscovered {
        rocket_design_id: usize,
        flaw_name: String,
    },
    /// A design flaw was fixed
    DesignFlawFixed {
        rocket_design_id: usize,
        flaw_name: String,
    },
    /// An engine flaw was discovered during testing
    EngineFlawDiscovered {
        engine_design_id: usize,
        flaw_name: String,
    },
    /// An engine flaw was fixed during revamp
    EngineFlawFixed {
        engine_design_id: usize,
        flaw_name: String,
    },
    /// An engine was manufactured (one unit of an engine order completed)
    EngineManufactured {
        engine_design_id: usize,
        revision_number: u32,
        order_id: crate::manufacturing::ManufacturingOrderId,
    },
    /// A rocket was assembled
    RocketAssembled {
        rocket_design_id: usize,
        revision_number: u32,
        serial_number: u32,
    },
    /// A manufacturing order is fully complete (all units built)
    ManufacturingOrderComplete {
        order_id: crate::manufacturing::ManufacturingOrderId,
    },
    /// A team finished ramping up
    TeamRampedUp {
        team_id: u32,
    },
    /// Salaries were deducted
    SalaryDeducted {
        amount: f64,
    },
    /// Floor space construction completed
    FloorSpaceCompleted {
        units: usize,
    },
    /// A rocket order was unblocked (engines arrived and were consumed)
    RocketOrderUnblocked {
        order_id: crate::manufacturing::ManufacturingOrderId,
    },
}

/// Calculate total efficiency for multiple teams working on the same thing
/// Returns the effective number of "full teams" worth of work
/// Uses power law: efficiency = team_count^0.75
pub fn team_efficiency(team_count: usize) -> f64 {
    if team_count == 0 {
        0.0
    } else {
        (team_count as f64).powf(0.75)
    }
}

/// Calculate marginal efficiency for the nth team (1-indexed)
/// Used for determining if adding another team is worth it
/// Returns the additional efficiency gained by adding the nth team
pub fn marginal_team_efficiency(team_number: usize) -> f64 {
    if team_number == 0 {
        0.0
    } else {
        team_efficiency(team_number) - team_efficiency(team_number - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_engineering_team() {
        let team = EngineeringTeam::new(1, TeamType::Engineering);
        assert_eq!(team.id, 1);
        assert_eq!(team.name, "Eng Team 1");
        assert_eq!(team.team_type, TeamType::Engineering);
        assert!(team.assignment.is_none());
        assert_eq!(team.ramp_up_days_remaining, 0);
        assert!(!team.is_ramping_up());
        assert_eq!(team.productivity(), 1.0);
    }

    #[test]
    fn test_new_manufacturing_team() {
        let team = EngineeringTeam::new(2, TeamType::Manufacturing);
        assert_eq!(team.id, 2);
        assert_eq!(team.name, "Mfg Team 2");
        assert_eq!(team.team_type, TeamType::Manufacturing);
    }

    #[test]
    fn test_ramp_up() {
        let mut team = EngineeringTeam::new(1, TeamType::Engineering);
        // Starts fully available
        assert_eq!(team.productivity(), 1.0);
        assert!(!team.is_ramping_up());

        // Assignment triggers ramp-up
        team.assign(TeamAssignment::RocketDesign {
            rocket_design_id: 0,
            work_phase: DesignWorkPhase::DetailedEngineering { progress: 0.0, total_work: 30.0 },
        });
        assert!(team.is_ramping_up());
        assert_eq!(team.productivity(), 0.0);

        // Process days until ramp-up complete
        for _ in 0..RAMP_UP_DAYS {
            team.process_day();
        }

        assert!(!team.is_ramping_up());
        assert_eq!(team.productivity(), 1.0);
    }

    #[test]
    fn test_team_efficiency() {
        // Uses power law: n^0.75
        assert_eq!(team_efficiency(0), 0.0);
        assert_eq!(team_efficiency(1), 1.0);
        // 2^0.75 ≈ 1.6818
        assert!((team_efficiency(2) - 1.6818).abs() < 0.001);
        // 3^0.75 ≈ 2.2795
        assert!((team_efficiency(3) - 2.2795).abs() < 0.001);
        // 4^0.75 ≈ 2.8284
        assert!((team_efficiency(4) - 2.8284).abs() < 0.001);
        // 5^0.75 ≈ 3.3437
        assert!((team_efficiency(5) - 3.3437).abs() < 0.001);
    }

    #[test]
    fn test_assignment_resets_ramp_up() {
        let mut team = EngineeringTeam::new(1, TeamType::Engineering);

        // Fully ramp up
        for _ in 0..RAMP_UP_DAYS {
            team.process_day();
        }
        assert!(!team.is_ramping_up());

        // Assign to new work
        team.assign(TeamAssignment::RocketDesign {
            rocket_design_id: 0,
            work_phase: DesignWorkPhase::DetailedEngineering {
                progress: 0.0,
                total_work: DETAILED_ENGINEERING_WORK,
            },
        });

        // Should be ramping up again
        assert!(team.is_ramping_up());
        assert_eq!(team.ramp_up_days_remaining, RAMP_UP_DAYS);
    }
}
