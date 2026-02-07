/// Work required to fix a discovered engine flaw (14 days with 1 team)
pub const ENGINE_FLAW_FIX_WORK: f64 = 14.0;

/// Status of an engine in the refining workflow
#[derive(Debug, Clone, PartialEq)]
pub enum EngineStatus {
    /// Engine has not been submitted for refining yet (future: Designing phase)
    Untested,
    /// Teams are refining the engine and looking for flaws
    Refining {
        /// Work progress (not used for completion, just for tracking)
        progress: f64,
        /// Total work (for reference)
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
            EngineStatus::Refining { .. } => "Refining",
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
            EngineStatus::Refining { .. } => 1.0, // Always show 100% for Refining
            EngineStatus::Fixing { progress, total, .. } => {
                if *total > 0.0 { progress / total } else { 0.0 }
            }
        }
    }

    /// Check if engine is being worked on
    pub fn is_working(&self) -> bool {
        matches!(self, EngineStatus::Refining { .. } | EngineStatus::Fixing { .. })
    }

    /// Start refining this engine
    pub fn start_refining(&mut self) {
        *self = EngineStatus::Refining {
            progress: 0.0,
            total: 30.0, // Reference value
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

    /// Return to Refining after fixing a flaw
    pub fn return_to_refining(&mut self) {
        *self = EngineStatus::Refining {
            progress: 30.0, // Start at 100%
            total: 30.0,
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

    /// Cost per engine by type (in dollars)
    pub const KEROLOX_ENGINE_COST: f64 = 10_000_000.0;
    pub const HYDROLOX_ENGINE_COST: f64 = 15_000_000.0;
    pub const SOLID_ENGINE_COST: f64 = 15_000_000.0;  // Cost per solid motor

    /// Cost per cubic meter of tank volume (in dollars)
    /// Covers tank structure, insulation, plumbing, etc.
    pub const TANK_COST_PER_M3: f64 = 100_000.0;

    /// Fixed overhead cost per stage (in dollars)
    /// Covers separation systems, avionics, structural integration
    pub const STAGE_OVERHEAD_COST: f64 = 5_000_000.0;

    /// Fixed overhead cost per rocket (in dollars)
    /// Covers integration, testing, launch operations
    pub const ROCKET_OVERHEAD_COST: f64 = 10_000_000.0;

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

    /// Structural mass for booster attachment points in kg
    pub const BOOSTER_ATTACHMENT_MASS_KG: f64 = 500.0;

    /// Cost for booster attachment hardware in dollars
    pub const BOOSTER_ATTACHMENT_COST: f64 = 1_000_000.0;
}
