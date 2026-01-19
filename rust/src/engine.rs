/// Engine types available for rocket design
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineType {
    /// Hydrogen/Oxygen engine - high efficiency, lower thrust
    Hydrolox,
    /// Kerosene/Oxygen engine - lower efficiency, higher thrust
    Kerolox,
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
                base_cost: 0.0, // Not implemented yet
            },
            EngineType::Kerolox => EngineSpec {
                engine_type: *self,
                name: "Kerolox".to_string(),
                mass_kg: 450.0,
                thrust_kn: 1000.0,
                exhaust_velocity_ms: 3000.0,
                failure_rate: 0.007, // 0.7%
                // Future-proofing fields
                revision: 1,
                production_count: 0,
                required_tech_level: 0,
                base_cost: 0.0,
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
        assert_eq!(spec.thrust_kn, 1000.0);
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
        assert_eq!(spec.total_thrust_kn(3), 3000.0);
    }

    #[test]
    fn test_total_mass() {
        let spec = EngineType::Hydrolox.spec();
        assert_eq!(spec.total_mass_kg(5), 1500.0);
    }
}
