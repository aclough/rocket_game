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

    /// Get the default specification for this engine type (without flaws).
    /// For flaw-aware operations, use EngineRegistry instead.
    pub fn spec(&self) -> EngineSpec {
        match self {
            EngineType::Hydrolox => EngineSpec {
                engine_type: *self,
                name: "Hydrolox".to_string(),
                mass_kg: 300.0,
                thrust_kn: 100.0,
                exhaust_velocity_ms: 4500.0,
                revision: 1,
                production_count: 0,
                required_tech_level: 0,
                base_cost: costs::HYDROLOX_ENGINE_COST,
                flaws: Vec::new(),
                flaws_generated: false,
            },
            EngineType::Kerolox => EngineSpec {
                engine_type: *self,
                name: "Kerolox".to_string(),
                mass_kg: 450.0,
                thrust_kn: 500.0,
                exhaust_velocity_ms: 3000.0,
                revision: 1,
                production_count: 0,
                required_tech_level: 0,
                base_cost: costs::KEROLOX_ENGINE_COST,
                flaws: Vec::new(),
                flaws_generated: false,
            },
        }
    }
}

use crate::flaw::{Flaw, FlawGenerator};
use std::collections::HashMap;

/// Specification for a rocket engine
/// Note: Engine failures are handled through the flaw system stored on EngineSpec.
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

    // Future-proofing fields (from Rocket Tycoon 1.0 vision)

    /// Revision number - higher means more refined design
    pub revision: u32,
    /// Number of engines produced - affects manufacturing efficiency
    pub production_count: u32,
    /// Technology level required to build this engine
    pub required_tech_level: u32,
    /// Base cost per engine in dollars (for future economy system)
    pub base_cost: f64,

    // Flaw system fields

    /// Flaws associated with this engine type
    pub flaws: Vec<Flaw>,
    /// Whether flaws have been generated for this engine
    pub flaws_generated: bool,
}

impl EngineSpec {
    /// Calculate total thrust for multiple engines in kN
    pub fn total_thrust_kn(&self, engine_count: u32) -> f64 {
        self.thrust_kn * engine_count as f64
    }

    /// Calculate total engine mass for multiple engines in kg
    pub fn total_mass_kg(&self, engine_count: u32) -> f64 {
        self.mass_kg * engine_count as f64
    }

    /// Generate flaws for this engine if not already generated
    pub fn generate_flaws(&mut self, generator: &mut FlawGenerator) {
        if self.flaws_generated {
            return;
        }

        // Generate engine flaws for this engine type
        // Fixed count per engine type (not scaled by usage)
        let engine_type_index = self.engine_type.to_index();
        self.flaws = generator.generate_engine_flaws_for_type(engine_type_index);
        self.flaws_generated = true;
    }

    /// Get active (unfixed) flaws for this engine
    pub fn active_flaws(&self) -> Vec<&Flaw> {
        self.flaws.iter().filter(|f| f.is_active()).collect()
    }

    /// Find a flaw by ID
    pub fn get_flaw(&self, id: u32) -> Option<&Flaw> {
        self.flaws.iter().find(|f| f.id == id)
    }

    /// Find a flaw by ID (mutable)
    pub fn get_flaw_mut(&mut self, id: u32) -> Option<&mut Flaw> {
        self.flaws.iter_mut().find(|f| f.id == id)
    }
}

/// Registry of engine specifications with their flaws.
/// Engine flaws are shared across all designs using the same engine type.
#[derive(Debug, Clone)]
pub struct EngineRegistry {
    specs: HashMap<EngineType, EngineSpec>,
    flaw_generator: FlawGenerator,
}

impl EngineRegistry {
    /// Create a new registry with default engine specs
    pub fn new() -> Self {
        let mut specs = HashMap::new();
        for engine_type in EngineType::all() {
            specs.insert(engine_type, engine_type.spec());
        }
        Self {
            specs,
            flaw_generator: FlawGenerator::new(),
        }
    }

    /// Get an engine spec (generates flaws if not already done)
    pub fn get(&mut self, engine_type: EngineType) -> &EngineSpec {
        // Ensure flaws are generated
        if let Some(spec) = self.specs.get_mut(&engine_type) {
            if !spec.flaws_generated {
                spec.generate_flaws(&mut self.flaw_generator);
            }
        }
        self.specs.get(&engine_type).unwrap()
    }

    /// Get an engine spec mutably (generates flaws if not already done)
    pub fn get_mut(&mut self, engine_type: EngineType) -> &mut EngineSpec {
        let spec = self.specs.get_mut(&engine_type).unwrap();
        if !spec.flaws_generated {
            let generator = &mut self.flaw_generator;
            spec.generate_flaws(generator);
        }
        self.specs.get_mut(&engine_type).unwrap()
    }

    /// Get an engine spec without generating flaws (for read-only access to stats)
    pub fn get_spec_readonly(&self, engine_type: EngineType) -> &EngineSpec {
        self.specs.get(&engine_type).unwrap()
    }

    /// Get all engine types in the registry
    pub fn engine_types(&self) -> Vec<EngineType> {
        self.specs.keys().cloned().collect()
    }

    /// Get the flaw generator (for generating design flaws)
    pub fn flaw_generator_mut(&mut self) -> &mut FlawGenerator {
        &mut self.flaw_generator
    }
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self::new()
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
    }

    #[test]
    fn test_kerolox_spec() {
        let spec = EngineType::Kerolox.spec();
        assert_eq!(spec.mass_kg, 450.0);
        assert_eq!(spec.thrust_kn, 500.0);
        assert_eq!(spec.exhaust_velocity_ms, 3000.0);
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
