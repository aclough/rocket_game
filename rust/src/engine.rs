/// Engine types available for rocket design
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineType {
    /// Hydrogen/Oxygen engine - high efficiency, lower thrust
    Hydrolox,
    /// Kerosene/Oxygen engine - lower efficiency, higher thrust
    Kerolox,
}

/// Cost constants for rocket budget system
pub mod costs {
    /// Starting budget in dollars
    pub const STARTING_BUDGET: f64 = 150_000_000.0;

    /// Cost per engine by type (in dollars)
    pub const KEROLOX_ENGINE_COST: f64 = 10_000_000.0;
    pub const HYDROLOX_ENGINE_COST: f64 = 15_000_000.0;

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
    ///
    /// Kerolox (RP-1/LOX):
    /// - RP-1: ~810 kg/m³
    /// - LOX: ~1141 kg/m³
    /// - Typical O:F ratio ~2.56:1 by mass
    /// - Effective combined density: ~1020 kg/m³
    pub const KEROLOX_DENSITY_KG_M3: f64 = 1020.0;

    /// Hydrolox (LH2/LOX):
    /// - LH2: ~71 kg/m³ (extremely low density)
    /// - LOX: ~1141 kg/m³
    /// - Typical O:F ratio ~6:1 by mass
    /// - Effective combined density: ~290 kg/m³
    pub const HYDROLOX_DENSITY_KG_M3: f64 = 290.0;

    /// Tank structural mass as a fraction of propellant mass
    /// This accounts for tank walls, stringers, insulation, plumbing, etc.
    /// Real rockets range from 5-12% depending on propellant type and technology
    /// - Kerolox tanks: typically ~5-7% (denser propellant, smaller tanks)
    /// - Hydrolox tanks: typically ~8-12% (larger tanks for low-density LH2, insulation)
    /// We use a single value for simplicity; could be per-propellant-type later
    pub const TANK_STRUCTURAL_MASS_RATIO: f64 = 0.08;

    /// Structural mass for booster attachment points in kg
    /// Covers radial decouplers, structural adapters, and crossfeed plumbing
    pub const BOOSTER_ATTACHMENT_MASS_KG: f64 = 500.0;

    /// Cost for booster attachment hardware in dollars
    /// Covers radial decouplers, structural integration, and separation systems
    pub const BOOSTER_ATTACHMENT_COST: f64 = 1_000_000.0;
}

impl EngineType {
    /// Returns all available engine types
    pub fn all() -> Vec<EngineType> {
        vec![EngineType::Hydrolox, EngineType::Kerolox]
    }

    /// Convert from integer index (for Godot API)
    pub fn from_index(index: i32) -> Option<EngineType> {
        match index {
            0 => Some(EngineType::Hydrolox),
            1 => Some(EngineType::Kerolox),
            _ => None,
        }
    }

    /// Convert to integer index (for Godot API)
    pub fn to_index(&self) -> i32 {
        match self {
            EngineType::Hydrolox => 0,
            EngineType::Kerolox => 1,
        }
    }

    /// Get the cost per engine for this type in dollars
    pub fn engine_cost(&self) -> f64 {
        match self {
            EngineType::Hydrolox => costs::HYDROLOX_ENGINE_COST,
            EngineType::Kerolox => costs::KEROLOX_ENGINE_COST,
        }
    }

    /// Get the propellant density for this engine type in kg/m³
    pub fn propellant_density(&self) -> f64 {
        match self {
            EngineType::Hydrolox => costs::HYDROLOX_DENSITY_KG_M3,
            EngineType::Kerolox => costs::KEROLOX_DENSITY_KG_M3,
        }
    }

    /// Get the specification for this engine type
    pub fn spec(&self) -> EngineSpec {
        match self {
            EngineType::Hydrolox => EngineSpec {
                engine_type: *self,
                name: "Hydrolox".to_string(),
                mass_kg: 300.0,
                thrust_kn: 100.0,
                exhaust_velocity_ms: 4500.0,
                failure_rate: 0.008, // 0.8%
                // Future-proofing fields
                revision: 1,
                production_count: 0,
                required_tech_level: 0,
                base_cost: costs::HYDROLOX_ENGINE_COST,
            },
            EngineType::Kerolox => EngineSpec {
                engine_type: *self,
                name: "Kerolox".to_string(),
                mass_kg: 450.0,
                thrust_kn: 500.0,  // Reduced from 1000 kN
                exhaust_velocity_ms: 3000.0,
                failure_rate: 0.007, // 0.7%
                // Future-proofing fields
                revision: 1,
                production_count: 0,
                required_tech_level: 0,
                base_cost: costs::KEROLOX_ENGINE_COST,
            },
        }
    }
}

/// Specification for a rocket engine
#[derive(Debug, Clone)]
pub struct EngineSpec {
    /// The type of engine
    pub engine_type: EngineType,
    /// Human-readable name
    pub name: String,
    /// Mass of the engine in kilograms
    pub mass_kg: f64,
    /// Thrust in kilonewtons
    pub thrust_kn: f64,
    /// Exhaust velocity in meters per second (equivalent to Isp * g0)
    pub exhaust_velocity_ms: f64,
    /// Probability of failure per ignition (0.0 to 1.0)
    pub failure_rate: f64,

    // Future-proofing fields (from Rocket Tycoon 1.0 vision)

    /// Revision number - higher means more refined design
    pub revision: u32,
    /// Number of engines produced - affects manufacturing efficiency
    pub production_count: u32,
    /// Technology level required to build this engine
    pub required_tech_level: u32,
    /// Base cost per engine in dollars (for future economy system)
    pub base_cost: f64,
}

impl EngineSpec {
    /// Calculate success rate for a stage with multiple engines
    /// All engines must ignite successfully
    pub fn stage_success_rate(&self, engine_count: u32) -> f64 {
        let single_success = 1.0 - self.failure_rate;
        single_success.powi(engine_count as i32)
    }

    /// Calculate failure rate for a stage with multiple engines
    pub fn stage_failure_rate(&self, engine_count: u32) -> f64 {
        1.0 - self.stage_success_rate(engine_count)
    }

    /// Calculate total thrust for multiple engines in kN
    pub fn total_thrust_kn(&self, engine_count: u32) -> f64 {
        self.thrust_kn * engine_count as f64
    }

    /// Calculate total engine mass for multiple engines in kg
    pub fn total_mass_kg(&self, engine_count: u32) -> f64 {
        self.mass_kg * engine_count as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_types() {
        let types = EngineType::all();
        assert_eq!(types.len(), 2);
        assert!(types.contains(&EngineType::Hydrolox));
        assert!(types.contains(&EngineType::Kerolox));
    }

    #[test]
    fn test_engine_index_conversion() {
        assert_eq!(EngineType::from_index(0), Some(EngineType::Hydrolox));
        assert_eq!(EngineType::from_index(1), Some(EngineType::Kerolox));
        assert_eq!(EngineType::from_index(2), None);

        assert_eq!(EngineType::Hydrolox.to_index(), 0);
        assert_eq!(EngineType::Kerolox.to_index(), 1);
    }

    #[test]
    fn test_hydrolox_spec() {
        let spec = EngineType::Hydrolox.spec();
        assert_eq!(spec.mass_kg, 300.0);
        assert_eq!(spec.thrust_kn, 100.0);
        assert_eq!(spec.exhaust_velocity_ms, 4500.0);
        assert_eq!(spec.failure_rate, 0.008);
    }

    #[test]
    fn test_kerolox_spec() {
        let spec = EngineType::Kerolox.spec();
        assert_eq!(spec.mass_kg, 450.0);
        assert_eq!(spec.thrust_kn, 500.0);
        assert_eq!(spec.exhaust_velocity_ms, 3000.0);
        assert_eq!(spec.failure_rate, 0.007);
    }

    #[test]
    fn test_stage_failure_rate() {
        let spec = EngineType::Kerolox.spec();

        // Single engine: 0.7% failure
        let single = spec.stage_failure_rate(1);
        assert!((single - 0.007).abs() < 0.0001);

        // Three engines: 1 - (0.993)^3 = ~2.08%
        let triple = spec.stage_failure_rate(3);
        let expected = 1.0 - 0.993_f64.powi(3);
        assert!((triple - expected).abs() < 0.0001);
    }

    #[test]
    fn test_total_thrust() {
        let spec = EngineType::Kerolox.spec();
        assert_eq!(spec.total_thrust_kn(3), 1500.0); // 3 × 500 kN
    }

    #[test]
    fn test_total_mass() {
        let spec = EngineType::Hydrolox.spec();
        assert_eq!(spec.total_mass_kg(5), 1500.0);
    }
}
