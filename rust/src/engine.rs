/// Work required to fix a discovered engine flaw (14 days with 1 team)
pub const ENGINE_FLAW_FIX_WORK: f64 = 14.0;

/// Status of an engine in the testing workflow
#[derive(Debug, Clone, PartialEq)]
pub enum EngineStatus {
    /// Engine has not been submitted for testing yet (future: Designing phase)
    Untested,
    /// Teams are testing the engine and looking for flaws
    Testing {
        /// Work progress (0.0 to total)
        progress: f64,
        /// Total work required per testing cycle
        total: f64,
    },
    /// Teams are fixing a discovered flaw
    Fixing {
        /// Name of the flaw being fixed
        flaw_name: String,
        /// Index of the flaw in the flaws list
        flaw_index: usize,
        /// Work progress (0.0 to total)
        progress: f64,
        /// Total work required
        total: f64,
    },
}

impl Default for EngineStatus {
    fn default() -> Self {
        EngineStatus::Untested
    }
}

impl EngineStatus {
    /// Get the base status name for display
    pub fn name(&self) -> &'static str {
        match self {
            EngineStatus::Untested => "Untested",
            EngineStatus::Testing { .. } => "Testing",
            EngineStatus::Fixing { .. } => "Fixing",
        }
    }

    /// Get the full status string for display (includes flaw name if Fixing)
    pub fn display_name(&self) -> String {
        match self {
            EngineStatus::Fixing { flaw_name, .. } => format!("Fixing: {}", flaw_name),
            other => other.name().to_string(),
        }
    }

    /// Get progress as a fraction (0.0 to 1.0)
    pub fn progress_fraction(&self) -> f64 {
        match self {
            EngineStatus::Untested => 0.0,
            EngineStatus::Testing { progress, total } => {
                if *total > 0.0 { progress / total } else { 0.0 }
            }
            EngineStatus::Fixing { progress, total, .. } => {
                if *total > 0.0 { progress / total } else { 0.0 }
            }
        }
    }

    /// Check if engine is being worked on
    pub fn is_working(&self) -> bool {
        matches!(self, EngineStatus::Testing { .. } | EngineStatus::Fixing { .. })
    }

    /// Start testing this engine
    pub fn start_testing(&mut self) {
        *self = EngineStatus::Testing {
            progress: 0.0,
            total: crate::engineering_team::TESTING_WORK,
        };
    }

    /// Start fixing a flaw
    pub fn start_fixing(&mut self, flaw_name: String, flaw_index: usize) {
        *self = EngineStatus::Fixing {
            flaw_name,
            flaw_index,
            progress: 0.0,
            total: ENGINE_FLAW_FIX_WORK,
        };
    }

    /// Return to Testing after fixing a flaw (reset progress for new cycle)
    pub fn return_to_testing(&mut self) {
        *self = EngineStatus::Testing {
            progress: 0.0,
            total: crate::engineering_team::TESTING_WORK,
        };
    }
}

/// Cost constants for rocket budget system
pub mod costs {
    /// Standard gravity at Earth's surface in m/s²
    pub const G0: f64 = 9.81;

    /// Starting budget in dollars
    pub const STARTING_BUDGET: f64 = 500_000_000.0;

    /// Cost per engine test in dollars
    pub const ENGINE_TEST_COST: f64 = 1_000_000.0;

    /// Cost per rocket test in dollars
    pub const ROCKET_TEST_COST: f64 = 2_000_000.0;

    /// Cost to fix a discovered flaw in dollars
    pub const FLAW_FIX_COST: f64 = 5_000_000.0;

    /// Propellant densities in kg/m³
    /// These are effective combined densities accounting for mixture ratios
    pub const KEROLOX_DENSITY_KG_M3: f64 = 1020.0;
    pub const HYDROLOX_DENSITY_KG_M3: f64 = 290.0;

    /// Tank structural mass as a fraction of propellant mass
    pub const KEROLOX_TANK_MASS_RATIO: f64 = 0.06;
    pub const HYDROLOX_TANK_MASS_RATIO: f64 = 0.10;

    /// Solid motor fixed mass ratio (propellant mass / total mass)
    pub const SOLID_MASS_RATIO: f64 = 0.88;

    /// Solid motor propellant density in kg/m³
    pub const SOLID_DENSITY_KG_M3: f64 = 1800.0;

    /// Solid motor "tank" mass ratio (casing mass as fraction of propellant)
    pub const SOLID_TANK_MASS_RATIO: f64 = 0.136;

    /// Methalox (CH4/LOX) effective combined density in kg/m³
    pub const METHALOX_DENSITY_KG_M3: f64 = 830.0;

    /// Methalox tank structural mass as a fraction of propellant mass
    pub const METHALOX_TANK_MASS_RATIO: f64 = 0.07;

    /// Hypergolic (NTO/UDMH) effective combined density in kg/m³
    pub const HYPERGOLIC_DENSITY_KG_M3: f64 = 1200.0;

    /// Hypergolic tank structural mass as a fraction of propellant mass
    pub const HYPERGOLIC_TANK_MASS_RATIO: f64 = 0.05;

    /// Structural mass for booster attachment points in kg
    pub const BOOSTER_ATTACHMENT_MASS_KG: f64 = 500.0;

    /// Cost for booster attachment hardware in dollars
    pub const BOOSTER_ATTACHMENT_COST: f64 = 1_000_000.0;
}
