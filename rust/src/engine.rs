use crate::flaw::{Flaw, FlawCategory, FlawGenerator};
use std::collections::HashMap;

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

/// Engine types available for rocket design
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineType {
    /// Hydrogen/Oxygen engine - high efficiency, lower thrust
    Hydrolox,
    /// Kerosene/Oxygen engine - lower efficiency, higher thrust
    Kerolox,
    /// Solid rocket motor - fixed mass ratio, high thrust, cheap
    Solid,
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

    /// Kerolox tank structural mass ratio (~6%)
    /// Denser propellant means smaller tank volume for the same mass
    /// Less insulation needed (RP-1 is storable, LOX is mildly cryogenic)
    pub const KEROLOX_TANK_MASS_RATIO: f64 = 0.06;

    /// Hydrolox tank structural mass ratio (~10%)
    /// Very low density LH2 requires much larger tanks
    /// Extensive insulation needed for deeply cryogenic LH2 (20K)
    /// More complex plumbing and boil-off management
    pub const HYDROLOX_TANK_MASS_RATIO: f64 = 0.10;

    /// Solid motor fixed mass ratio (propellant mass / total mass)
    /// Modern solid motors achieve ~0.88 mass ratio
    /// Player cannot adjust propellant independently - motor is a fixed unit
    pub const SOLID_MASS_RATIO: f64 = 0.88;

    /// Solid motor propellant density in kg/m³
    /// HTPB/AP propellant is quite dense
    pub const SOLID_DENSITY_KG_M3: f64 = 1800.0;

    /// Solid motor "tank" mass ratio (casing mass as fraction of propellant)
    /// For solids this represents the motor casing, not tanks
    pub const SOLID_TANK_MASS_RATIO: f64 = 0.136;  // Derived from mass ratio: (1 - 0.88) / 0.88

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
        vec![EngineType::Hydrolox, EngineType::Kerolox, EngineType::Solid]
    }

    /// Convert from integer index (for Godot API)
    pub fn from_index(index: i32) -> Option<EngineType> {
        match index {
            0 => Some(EngineType::Hydrolox),
            1 => Some(EngineType::Kerolox),
            2 => Some(EngineType::Solid),
            _ => None,
        }
    }

    /// Convert to integer index (for Godot API)
    pub fn to_index(&self) -> i32 {
        match self {
            EngineType::Hydrolox => 0,
            EngineType::Kerolox => 1,
            EngineType::Solid => 2,
        }
    }

    /// Get the cost per engine for this type in dollars
    pub fn engine_cost(&self) -> f64 {
        match self {
            EngineType::Hydrolox => costs::HYDROLOX_ENGINE_COST,
            EngineType::Kerolox => costs::KEROLOX_ENGINE_COST,
            EngineType::Solid => costs::SOLID_ENGINE_COST,
        }
    }

    /// Get the propellant density for this engine type in kg/m³
    pub fn propellant_density(&self) -> f64 {
        match self {
            EngineType::Hydrolox => costs::HYDROLOX_DENSITY_KG_M3,
            EngineType::Kerolox => costs::KEROLOX_DENSITY_KG_M3,
            EngineType::Solid => costs::SOLID_DENSITY_KG_M3,
        }
    }

    /// Get the tank structural mass ratio for this engine type
    /// This is the fraction of propellant mass that the tank structure weighs
    /// For solids, this represents the motor casing
    pub fn tank_mass_ratio(&self) -> f64 {
        match self {
            EngineType::Hydrolox => costs::HYDROLOX_TANK_MASS_RATIO,
            EngineType::Kerolox => costs::KEROLOX_TANK_MASS_RATIO,
            EngineType::Solid => costs::SOLID_TANK_MASS_RATIO,
        }
    }

    /// Get the fixed mass ratio for this engine type, if applicable
    /// Solid motors have a fixed mass ratio; liquid engines return None
    pub fn fixed_mass_ratio(&self) -> Option<f64> {
        match self {
            EngineType::Solid => Some(costs::SOLID_MASS_RATIO),
            _ => None,
        }
    }

    /// Check if this engine type uses solid propellant
    pub fn is_solid(&self) -> bool {
        matches!(self, EngineType::Solid)
    }

    /// Get the flaw category for this engine type
    /// Determines which set of flaw templates to use
    pub fn flaw_category(&self) -> FlawCategory {
        match self {
            EngineType::Solid => FlawCategory::SolidMotor,
            _ => FlawCategory::LiquidEngine,
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
                active_flaws: Vec::new(),
                fixed_flaws: Vec::new(),
                flaws_generated: false,
                status: EngineStatus::Untested,
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
                active_flaws: Vec::new(),
                fixed_flaws: Vec::new(),
                flaws_generated: false,
                status: EngineStatus::Untested,
            },
            EngineType::Solid => EngineSpec {
                engine_type: *self,
                name: "Solid".to_string(),
                mass_kg: 40_000.0,   // Dry mass of one solid motor
                thrust_kn: 8_000.0,  // High thrust
                exhaust_velocity_ms: 2650.0, // ~270s Isp
                revision: 1,
                production_count: 0,
                required_tech_level: 0,
                base_cost: costs::SOLID_ENGINE_COST,
                active_flaws: Vec::new(),
                fixed_flaws: Vec::new(),
                flaws_generated: false,
                status: EngineStatus::Untested,
            },
        }
    }
}

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

    /// Active (unfixed) flaws associated with this engine type
    pub active_flaws: Vec<Flaw>,
    /// Fixed flaws (kept for history/UI display)
    pub fixed_flaws: Vec<Flaw>,
    /// Whether flaws have been generated for this engine
    pub flaws_generated: bool,
    /// Current status in the testing workflow
    pub status: EngineStatus,
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

        // Generate engine flaws for this engine type using the appropriate category
        // Solid motors use SolidMotor flaws, liquid engines use LiquidEngine flaws
        let engine_type_index = self.engine_type.to_index();
        let category = self.engine_type.flaw_category();
        self.active_flaws = generator.generate_engine_flaws_for_type_with_category(engine_type_index, category);
        self.fixed_flaws.clear();
        self.flaws_generated = true;
    }

    /// Get active (unfixed) flaws for this engine
    pub fn get_active_flaws(&self) -> &[Flaw] {
        &self.active_flaws
    }

    /// Get fixed flaws for this engine
    pub fn get_fixed_flaws(&self) -> &[Flaw] {
        &self.fixed_flaws
    }

    /// Get total flaw count (active + fixed)
    pub fn get_flaw_count(&self) -> usize {
        self.active_flaws.len() + self.fixed_flaws.len()
    }

    /// Find a flaw by ID in active flaws
    pub fn get_flaw(&self, id: u32) -> Option<&Flaw> {
        self.active_flaws.iter().find(|f| f.id == id)
    }

    /// Find a flaw by ID (mutable) in active flaws
    pub fn get_flaw_mut(&mut self, id: u32) -> Option<&mut Flaw> {
        self.active_flaws.iter_mut().find(|f| f.id == id)
    }

    /// Fix a flaw by ID - moves it from active_flaws to fixed_flaws
    /// Returns true if the flaw was fixed
    pub fn fix_flaw(&mut self, id: u32) -> bool {
        if let Some(index) = self.active_flaws.iter().position(|f| f.id == id && f.discovered) {
            let mut flaw = self.active_flaws.remove(index);
            flaw.fixed = true;
            self.fixed_flaws.push(flaw);
            return true;
        }
        false
    }

    /// Fix a flaw by index - moves it from active_flaws to fixed_flaws
    /// Returns the flaw name if successful
    pub fn fix_flaw_by_index(&mut self, index: usize) -> Option<String> {
        if index < self.active_flaws.len() && self.active_flaws[index].discovered {
            let mut flaw = self.active_flaws.remove(index);
            let name = flaw.name.clone();
            flaw.fixed = true;
            self.fixed_flaws.push(flaw);
            return Some(name);
        }
        None
    }

    /// Get count of discovered (but not yet fixed) flaws
    pub fn get_discovered_unfixed_count(&self) -> usize {
        self.active_flaws.iter().filter(|f| f.discovered).count()
    }

    /// Get the index of the first discovered but unfixed flaw
    pub fn get_next_unfixed_flaw(&self) -> Option<usize> {
        self.active_flaws.iter().position(|f| f.discovered && !f.fixed)
    }

    /// Get names of discovered (but not fixed) flaws
    pub fn get_unfixed_flaw_names(&self) -> Vec<String> {
        self.active_flaws
            .iter()
            .filter(|f| f.discovered)
            .map(|f| f.name.clone())
            .collect()
    }

    /// Get names of fixed flaws
    pub fn get_fixed_flaw_names(&self) -> Vec<String> {
        self.fixed_flaws.iter().map(|f| f.name.clone()).collect()
    }

    /// Submit engine for refining (generates flaws if needed)
    pub fn submit_to_refining(&mut self, generator: &mut FlawGenerator) -> bool {
        if !matches!(self.status, EngineStatus::Untested) {
            return false;
        }
        if !self.flaws_generated {
            self.generate_flaws(generator);
        }
        self.status.start_refining();
        true
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
        assert_eq!(types.len(), 3);
        assert!(types.contains(&EngineType::Hydrolox));
        assert!(types.contains(&EngineType::Kerolox));
        assert!(types.contains(&EngineType::Solid));
    }

    #[test]
    fn test_engine_index_conversion() {
        assert_eq!(EngineType::from_index(0), Some(EngineType::Hydrolox));
        assert_eq!(EngineType::from_index(1), Some(EngineType::Kerolox));
        assert_eq!(EngineType::from_index(2), Some(EngineType::Solid));
        assert_eq!(EngineType::from_index(3), None);

        assert_eq!(EngineType::Hydrolox.to_index(), 0);
        assert_eq!(EngineType::Kerolox.to_index(), 1);
        assert_eq!(EngineType::Solid.to_index(), 2);
    }

    #[test]
    fn test_solid_spec() {
        let spec = EngineType::Solid.spec();
        assert_eq!(spec.mass_kg, 40_000.0);
        assert_eq!(spec.thrust_kn, 8_000.0);
        assert_eq!(spec.exhaust_velocity_ms, 2650.0);
    }

    #[test]
    fn test_solid_fixed_mass_ratio() {
        assert!(EngineType::Solid.fixed_mass_ratio().is_some());
        assert!(EngineType::Kerolox.fixed_mass_ratio().is_none());
        assert!(EngineType::Hydrolox.fixed_mass_ratio().is_none());

        let ratio = EngineType::Solid.fixed_mass_ratio().unwrap();
        assert!((ratio - 0.88).abs() < 0.01);
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
