use crate::balance::{complexity_cost_multiplier, cycle_thrust_multiplier, cycle_ve_multiplier, cycle_mass_multiplier};
use crate::design_workflow::DesignWorkflow;
use crate::engine::costs;
use crate::flaw::{Flaw, FlawCategory, FlawGenerator};

pub const ENGINE_SCALE_MIN: f64 = 0.25;
pub const ENGINE_SCALE_MAX: f64 = 4.0;
pub const ENGINE_SCALE_STEP: f64 = 0.25;

/// Chemical engine fuel types.
/// Future propulsion categories (nuclear pulse, sail) would be a separate enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FuelType {
    Kerolox,
    Hydrolox,
    Solid,
    Methalox,
    Hypergolic,
}

impl FuelType {
    pub fn display_name(&self) -> &'static str {
        match self {
            FuelType::Kerolox => "Kerolox",
            FuelType::Hydrolox => "Hydrolox",
            FuelType::Solid => "Solid",
            FuelType::Methalox => "Methalox",
            FuelType::Hypergolic => "Hypergolic",
        }
    }

    pub fn from_index(i: usize) -> Option<FuelType> {
        match i {
            0 => Some(FuelType::Kerolox),
            1 => Some(FuelType::Hydrolox),
            2 => Some(FuelType::Solid),
            3 => Some(FuelType::Methalox),
            4 => Some(FuelType::Hypergolic),
            _ => None,
        }
    }

    pub fn index(&self) -> usize {
        match self {
            FuelType::Kerolox => 0,
            FuelType::Hydrolox => 1,
            FuelType::Solid => 2,
            FuelType::Methalox => 3,
            FuelType::Hypergolic => 4,
        }
    }
}

/// Engine cycle (pump type) determines how propellants are fed to the combustion chamber.
/// Higher-performance cycles are more complex and expensive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EngineCycle {
    PressureFed,
    GasGenerator,
    Expander,
    StagedCombustion,
    FullFlowStagedCombustion,
}

impl EngineCycle {
    pub fn from_index(i: usize) -> Option<EngineCycle> {
        match i {
            0 => Some(EngineCycle::PressureFed),
            1 => Some(EngineCycle::GasGenerator),
            2 => Some(EngineCycle::Expander),
            3 => Some(EngineCycle::StagedCombustion),
            4 => Some(EngineCycle::FullFlowStagedCombustion),
            _ => None,
        }
    }

    pub fn index(&self) -> usize {
        match self {
            EngineCycle::PressureFed => 0,
            EngineCycle::GasGenerator => 1,
            EngineCycle::Expander => 2,
            EngineCycle::StagedCombustion => 3,
            EngineCycle::FullFlowStagedCombustion => 4,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            EngineCycle::PressureFed => "Pressure-Fed",
            EngineCycle::GasGenerator => "Gas Generator",
            EngineCycle::Expander => "Expander",
            EngineCycle::StagedCombustion => "Staged Combustion",
            EngineCycle::FullFlowStagedCombustion => "Full-Flow Staged",
        }
    }

    /// Whether this cycle uses a turbopump (all except PressureFed)
    pub fn has_turbopump(&self) -> bool {
        !matches!(self, EngineCycle::PressureFed)
    }
}

/// Check if a (fuel, cycle) combination is valid
pub fn is_cycle_compatible(fuel: FuelType, cycle: EngineCycle) -> bool {
    match (fuel, cycle) {
        // PressureFed: all fuels
        (_, EngineCycle::PressureFed) => true,
        // GasGenerator: all liquid fuels
        (FuelType::Kerolox, EngineCycle::GasGenerator) => true,
        (FuelType::Hydrolox, EngineCycle::GasGenerator) => true,
        (FuelType::Methalox, EngineCycle::GasGenerator) => true,
        (FuelType::Hypergolic, EngineCycle::GasGenerator) => true,
        // Expander: Hydrolox and Methalox only
        (FuelType::Hydrolox, EngineCycle::Expander) => true,
        (FuelType::Methalox, EngineCycle::Expander) => true,
        // StagedCombustion: all liquid fuels
        (FuelType::Kerolox, EngineCycle::StagedCombustion) => true,
        (FuelType::Hydrolox, EngineCycle::StagedCombustion) => true,
        (FuelType::Methalox, EngineCycle::StagedCombustion) => true,
        (FuelType::Hypergolic, EngineCycle::StagedCombustion) => true,
        // FullFlow: Kerolox, Hydrolox, Methalox
        (FuelType::Kerolox, EngineCycle::FullFlowStagedCombustion) => true,
        (FuelType::Hydrolox, EngineCycle::FullFlowStagedCombustion) => true,
        (FuelType::Methalox, EngineCycle::FullFlowStagedCombustion) => true,
        // Everything else invalid
        _ => false,
    }
}

/// Get the complexity value for a (fuel, cycle) combination.
/// Returns None for invalid combinations.
pub fn cycle_complexity(fuel: FuelType, cycle: EngineCycle) -> Option<i32> {
    if !is_cycle_compatible(fuel, cycle) {
        return None;
    }
    Some(match (fuel, cycle) {
        (FuelType::Kerolox, EngineCycle::PressureFed) => 4,
        (FuelType::Kerolox, EngineCycle::GasGenerator) => 6,
        (FuelType::Kerolox, EngineCycle::StagedCombustion) => 7,
        (FuelType::Kerolox, EngineCycle::FullFlowStagedCombustion) => 8,

        (FuelType::Hydrolox, EngineCycle::PressureFed) => 5,
        (FuelType::Hydrolox, EngineCycle::GasGenerator) => 6,
        (FuelType::Hydrolox, EngineCycle::Expander) => 7,
        (FuelType::Hydrolox, EngineCycle::StagedCombustion) => 8,
        (FuelType::Hydrolox, EngineCycle::FullFlowStagedCombustion) => 9,

        (FuelType::Methalox, EngineCycle::PressureFed) => 5,
        (FuelType::Methalox, EngineCycle::GasGenerator) => 6,
        (FuelType::Methalox, EngineCycle::Expander) => 7,
        (FuelType::Methalox, EngineCycle::StagedCombustion) => 8,
        (FuelType::Methalox, EngineCycle::FullFlowStagedCombustion) => 9,

        (FuelType::Hypergolic, EngineCycle::PressureFed) => 1,
        (FuelType::Hypergolic, EngineCycle::GasGenerator) => 2,
        (FuelType::Hypergolic, EngineCycle::StagedCombustion) => 4,

        (FuelType::Solid, EngineCycle::PressureFed) => 3,

        _ => unreachable!(), // guarded by is_cycle_compatible check
    })
}

/// Get the default cycle for a fuel type
pub fn default_cycle(fuel: FuelType) -> EngineCycle {
    match fuel {
        FuelType::Kerolox => EngineCycle::GasGenerator,
        FuelType::Hydrolox => EngineCycle::Expander,
        FuelType::Methalox => EngineCycle::GasGenerator,
        FuelType::Hypergolic => EngineCycle::PressureFed,
        FuelType::Solid => EngineCycle::PressureFed,
    }
}

/// Get the list of valid cycles for a fuel type (in index order)
pub fn valid_cycles_for_fuel(fuel: FuelType) -> Vec<EngineCycle> {
    let all = [
        EngineCycle::PressureFed,
        EngineCycle::GasGenerator,
        EngineCycle::Expander,
        EngineCycle::StagedCombustion,
        EngineCycle::FullFlowStagedCombustion,
    ];
    all.iter().copied().filter(|c| is_cycle_compatible(fuel, *c)).collect()
}

/// Get the engine components for a (fuel, cycle) combination
pub fn components_for(fuel: FuelType, cycle: EngineCycle) -> Vec<EngineComponent> {
    match fuel {
        FuelType::Solid => vec![EngineComponent::SolidMotor],
        FuelType::Kerolox => {
            if cycle.has_turbopump() {
                vec![EngineComponent::Kerolox, EngineComponent::Turbopump]
            } else {
                vec![EngineComponent::Kerolox]
            }
        }
        FuelType::Hydrolox => {
            if cycle.has_turbopump() {
                vec![EngineComponent::Hydrolox, EngineComponent::Turbopump]
            } else {
                vec![EngineComponent::Hydrolox]
            }
        }
        FuelType::Methalox => {
            if cycle.has_turbopump() {
                vec![EngineComponent::Methalox, EngineComponent::Turbopump]
            } else {
                vec![EngineComponent::Methalox]
            }
        }
        FuelType::Hypergolic => {
            if cycle.has_turbopump() {
                vec![EngineComponent::Hypergolic, EngineComponent::Turbopump]
            } else {
                vec![EngineComponent::Hypergolic]
            }
        }
    }
}

/// Components that make up an engine design.
/// Propellant chemistry determines base performance; Turbopump is required for turbopump-fed cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineComponent {
    /// RP-1/LOX propellant
    Kerolox,
    /// LH2/LOX propellant
    Hydrolox,
    /// HTPB/AP solid propellant
    SolidMotor,
    /// Required for turbopump-fed liquid engines
    Turbopump,
    /// CH4/LOX propellant
    Methalox,
    /// NTO/UDMH storable propellant
    Hypergolic,
}

/// A designable engine: composed of components at a scale, with derived stats and hidden flaws.
#[derive(Debug, Clone)]
pub struct EngineDesign {
    pub components: Vec<EngineComponent>,
    pub scale: f64,
    /// Complexity level (derived from fuel type + cycle).
    /// Higher = better performance, higher cost/build time, more flaws.
    pub complexity: i32,
    /// Engine cycle (pump type) — determines complexity
    pub cycle: EngineCycle,
    /// Unified workflow state (status, flaws, testing progress)
    pub workflow: DesignWorkflow,
}

/// Lightweight stats cache stored on RocketStage.
/// Avoids passing the full EngineDesign around.
#[derive(Debug, Clone)]
pub struct EngineDesignSnapshot {
    pub engine_design_id: usize,
    pub name: String,
    pub fuel_type: FuelType,
    pub cycle: EngineCycle,
    pub scale: f64,
    pub complexity: i32,
    pub mass_kg: f64,
    pub thrust_kn: f64,
    pub exhaust_velocity_ms: f64,
    pub base_cost: f64,
    pub propellant_density: f64,
    pub tank_mass_ratio: f64,
    pub is_solid: bool,
    pub fixed_mass_ratio: Option<f64>,
    pub flaw_category: FlawCategory,
}

impl EngineDesign {
    /// Whether this engine can be modified (only when in Specification)
    pub fn can_modify(&self) -> bool {
        self.workflow.status.can_edit()
    }

    /// Get the fuel type from current components
    pub fn fuel_type(&self) -> FuelType {
        if self.components.contains(&EngineComponent::SolidMotor) {
            FuelType::Solid
        } else if self.components.contains(&EngineComponent::Methalox) {
            FuelType::Methalox
        } else if self.components.contains(&EngineComponent::Hypergolic) {
            FuelType::Hypergolic
        } else if self.components.contains(&EngineComponent::Hydrolox) {
            FuelType::Hydrolox
        } else {
            FuelType::Kerolox
        }
    }

    /// Set fuel type. If current cycle is compatible with the new fuel, keep it;
    /// otherwise switch to the default cycle for the new fuel.
    /// Complexity is always derived from (fuel, cycle).
    /// Returns false if not modifiable.
    pub fn set_fuel_type(&mut self, fuel: FuelType) -> bool {
        if !self.can_modify() {
            return false;
        }
        // Keep current cycle if compatible, otherwise use default
        let new_cycle = if is_cycle_compatible(fuel, self.cycle) {
            self.cycle
        } else {
            default_cycle(fuel)
        };
        self.cycle = new_cycle;
        self.components = components_for(fuel, new_cycle);
        self.complexity = cycle_complexity(fuel, new_cycle).unwrap();
        true
    }

    /// Set the engine cycle. Returns false if not modifiable or incompatible.
    pub fn set_cycle(&mut self, cycle: EngineCycle) -> bool {
        if !self.can_modify() {
            return false;
        }
        let fuel = self.fuel_type();
        if !is_cycle_compatible(fuel, cycle) {
            return false;
        }
        self.cycle = cycle;
        self.components = components_for(fuel, cycle);
        self.complexity = cycle_complexity(fuel, cycle).unwrap();
        true
    }

    /// Set scale (clamped to bounds). Returns false if not modifiable.
    pub fn set_scale(&mut self, scale: f64) -> bool {
        if !self.can_modify() {
            return false;
        }
        self.scale = scale.clamp(ENGINE_SCALE_MIN, ENGINE_SCALE_MAX);
        true
    }

    /// Derive a snapshot of stats from this design's components and scale.
    pub fn snapshot(&self, id: usize, name: &str) -> EngineDesignSnapshot {
        let is_solid = self.components.contains(&EngineComponent::SolidMotor);

        let fuel_type = self.fuel_type();
        let (base_mass, base_thrust, ve, density, tank_ratio, fixed_mass_ratio, flaw_category) =
            match fuel_type {
                FuelType::Solid => (
                    40_000.0,
                    8_000.0,
                    2650.0,
                    costs::SOLID_DENSITY_KG_M3,
                    costs::SOLID_TANK_MASS_RATIO,
                    Some(costs::SOLID_MASS_RATIO),
                    FlawCategory::SolidMotor,
                ),
                FuelType::Hydrolox => (
                    300.0,
                    100.0,
                    4500.0,
                    costs::HYDROLOX_DENSITY_KG_M3,
                    costs::HYDROLOX_TANK_MASS_RATIO,
                    None,
                    FlawCategory::LiquidEngine,
                ),
                FuelType::Methalox => (
                    400.0,
                    400.0,
                    3300.0,
                    costs::METHALOX_DENSITY_KG_M3,
                    costs::METHALOX_TANK_MASS_RATIO,
                    None,
                    FlawCategory::LiquidEngine,
                ),
                FuelType::Hypergolic => (
                    200.0,
                    50.0,
                    2800.0,
                    costs::HYPERGOLIC_DENSITY_KG_M3,
                    costs::HYPERGOLIC_TANK_MASS_RATIO,
                    None,
                    FlawCategory::LiquidEngine,
                ),
                FuelType::Kerolox => (
                    450.0,
                    500.0,
                    3000.0,
                    costs::KEROLOX_DENSITY_KG_M3,
                    costs::KEROLOX_TANK_MASS_RATIO,
                    None,
                    FlawCategory::LiquidEngine,
                ),
            };

        let mass_kg = base_mass * self.scale * cycle_mass_multiplier(self.cycle);
        let exhaust_velocity_ms = ve * cycle_ve_multiplier(self.cycle);
        let raw_cost = crate::resources::engine_resource_cost(fuel_type, mass_kg);
        let base_cost = raw_cost * complexity_cost_multiplier(self.complexity);

        EngineDesignSnapshot {
            engine_design_id: id,
            name: name.to_string(),
            fuel_type,
            cycle: self.cycle,
            scale: self.scale,
            complexity: self.complexity,
            mass_kg,
            thrust_kn: base_thrust * self.scale * cycle_thrust_multiplier(self.cycle),
            exhaust_velocity_ms,
            base_cost,
            propellant_density: density,
            tank_mass_ratio: tank_ratio,
            is_solid,
            fixed_mass_ratio,
            flaw_category,
        }
    }

    // ==========================================
    // Flaw Management (delegates to workflow)
    // ==========================================

    /// Generate flaws for this engine if not already generated
    pub fn generate_flaws(&mut self, generator: &mut FlawGenerator, engine_design_id: usize) {
        if self.workflow.flaws_generated {
            return;
        }

        let category = if self.components.contains(&EngineComponent::SolidMotor) {
            FlawCategory::SolidMotor
        } else {
            FlawCategory::LiquidEngine
        };
        let fuel_type = self.fuel_type();
        self.workflow.active_flaws = generator.generate_engine_flaws_with_complexity(
            engine_design_id,
            category,
            fuel_type,
            self.scale,
            self.complexity,
        );
        self.workflow.fixed_flaws.clear();
        self.workflow.flaws_generated = true;
    }

    /// Get active (unfixed) flaws
    pub fn get_active_flaws(&self) -> &[Flaw] {
        &self.workflow.active_flaws
    }

    /// Get fixed flaws
    pub fn get_fixed_flaws(&self) -> &[Flaw] {
        &self.workflow.fixed_flaws
    }

    /// Get total flaw count (active + fixed)
    pub fn get_flaw_count(&self) -> usize {
        self.workflow.active_flaws.len() + self.workflow.fixed_flaws.len()
    }

    /// Find a flaw by ID in active flaws
    pub fn get_flaw(&self, id: u32) -> Option<&Flaw> {
        self.workflow.active_flaws.iter().find(|f| f.id == id)
    }

    /// Find a flaw by ID (mutable) in active flaws
    pub fn get_flaw_mut(&mut self, id: u32) -> Option<&mut Flaw> {
        self.workflow.active_flaws.iter_mut().find(|f| f.id == id)
    }

    /// Fix a flaw by ID - moves it from active_flaws to fixed_flaws
    pub fn fix_flaw(&mut self, id: u32) -> bool {
        if let Some(index) = self.workflow.active_flaws.iter().position(|f| f.id == id && f.discovered) {
            let mut flaw = self.workflow.active_flaws.remove(index);
            flaw.fixed = true;
            self.workflow.fixed_flaws.push(flaw);
            return true;
        }
        false
    }

    /// Get count of discovered (but not yet fixed) flaws
    pub fn get_discovered_unfixed_count(&self) -> usize {
        self.workflow.get_discovered_unfixed_count()
    }

    /// Get the index of the first discovered but unfixed flaw
    pub fn get_next_unfixed_flaw(&self) -> Option<usize> {
        self.workflow.get_next_unfixed_flaw()
    }

    /// Get names of discovered (but not fixed) flaws
    pub fn get_unfixed_flaw_names(&self) -> Vec<String> {
        self.workflow.get_unfixed_flaw_names()
    }

    /// Get names of fixed flaws
    pub fn get_fixed_flaw_names(&self) -> Vec<String> {
        self.workflow.get_fixed_flaw_names()
    }

    /// Submit engine to engineering (generates flaws and transitions to Engineering phase)
    pub fn submit_to_engineering(&mut self, generator: &mut FlawGenerator, engine_design_id: usize) -> bool {
        if !self.workflow.status.can_edit() {
            return false;
        }
        if !self.workflow.flaws_generated {
            self.generate_flaws(generator, engine_design_id);
        }
        self.workflow.submit_to_engineering()
    }
}

// ==========================================
// Engine Creation Functions
// ==========================================

/// Create an engine design with the given fuel type and scale.
/// Uses the default cycle for the fuel type; complexity derived from (fuel, cycle).
pub fn create_engine(fuel: FuelType, scale: f64) -> EngineDesign {
    let cycle = default_cycle(fuel);
    let complexity = cycle_complexity(fuel, cycle).unwrap();
    EngineDesign {
        components: components_for(fuel, cycle),
        scale: scale.clamp(ENGINE_SCALE_MIN, ENGINE_SCALE_MAX),
        complexity,
        cycle,
        workflow: DesignWorkflow::new(),
    }
}

// ==========================================
// Default Engine Creation Functions
// ==========================================

/// Create the default Kerolox engine design
pub fn default_kerolox() -> EngineDesign {
    create_engine(FuelType::Kerolox, 1.0)
}

pub fn default_hydrolox() -> EngineDesign {
    create_engine(FuelType::Hydrolox, 1.0)
}

pub fn default_solid() -> EngineDesign {
    create_engine(FuelType::Solid, 1.0)
}

/// Create the 3 default engine design lineages.
/// Index order matches old EngineType: 0=Hydrolox, 1=Kerolox, 2=Solid
pub fn default_engine_lineages() -> Vec<crate::design_lineage::DesignLineage<EngineDesign>> {
    use crate::design_lineage::DesignLineage;

    vec![
        DesignLineage::new("Hydrolox", default_hydrolox()),
        DesignLineage::new("Kerolox", default_kerolox()),
        DesignLineage::new("Solid", default_solid()),
    ]
}

/// Create an EngineDesignSnapshot for a default engine by index.
/// Useful for test helpers and default design creation.
pub fn default_snapshot(index: usize) -> EngineDesignSnapshot {
    let lineages = default_engine_lineages();
    lineages[index].head().snapshot(index, &lineages[index].name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kerolox_snapshot() {
        let design = default_kerolox();
        let snap = design.snapshot(1, "Kerolox");
        assert_eq!(snap.mass_kg, 450.0);
        assert_eq!(snap.thrust_kn, 500.0);
        assert_eq!(snap.exhaust_velocity_ms, 3000.0);
        assert_eq!(snap.fuel_type, FuelType::Kerolox);
        // base_cost is resource-based: ~$158K for 450 kg kerolox
        let expected_cost = crate::resources::engine_resource_cost(FuelType::Kerolox, 450.0);
        assert!((snap.base_cost - expected_cost).abs() < 1.0);
        assert_eq!(snap.propellant_density, costs::KEROLOX_DENSITY_KG_M3);
        assert_eq!(snap.tank_mass_ratio, costs::KEROLOX_TANK_MASS_RATIO);
        assert!(!snap.is_solid);
        assert!(snap.fixed_mass_ratio.is_none());
        assert_eq!(snap.flaw_category, FlawCategory::LiquidEngine);
    }

    #[test]
    fn test_hydrolox_snapshot() {
        let design = default_hydrolox();
        let snap = design.snapshot(0, "Hydrolox");
        // Default Hydrolox cycle = Expander: mass*0.9, thrust*0.8, ve*1.04
        assert!((snap.mass_kg - 270.0).abs() < 0.1); // 300 * 0.9
        assert!((snap.thrust_kn - 80.0).abs() < 0.1); // 100 * 0.8
        assert!((snap.exhaust_velocity_ms - 4680.0).abs() < 0.1); // 4500 * 1.04
        assert_eq!(snap.fuel_type, FuelType::Hydrolox);
        // cost = resource_cost(270) * complexity_cost(7/6)^2
        let expected_cost = crate::resources::engine_resource_cost(FuelType::Hydrolox, 270.0)
            * (7.0_f64 / 6.0).powi(2);
        assert!((snap.base_cost - expected_cost).abs() < 1.0);
        assert!(!snap.is_solid);
    }

    #[test]
    fn test_solid_snapshot() {
        let design = default_solid();
        let snap = design.snapshot(2, "Solid");
        // Default Solid cycle = PressureFed: mass*0.7, thrust*0.6, ve*0.92
        assert!((snap.mass_kg - 28_000.0).abs() < 0.1); // 40000 * 0.7
        assert!((snap.thrust_kn - 4_800.0).abs() < 0.1); // 8000 * 0.6
        assert!((snap.exhaust_velocity_ms - 2438.0).abs() < 0.1); // 2650 * 0.92
        assert_eq!(snap.fuel_type, FuelType::Solid);
        // cost = resource_cost(28000) * complexity_cost(3/6)^2 = resource_cost * 0.25
        let expected_cost = crate::resources::engine_resource_cost(FuelType::Solid, 28_000.0)
            * (3.0_f64 / 6.0).powi(2);
        assert!((snap.base_cost - expected_cost).abs() < 1.0);
        assert!(snap.is_solid);
        assert!((snap.fixed_mass_ratio.unwrap() - 0.88).abs() < 0.01);
        assert_eq!(snap.flaw_category, FlawCategory::SolidMotor);
    }

    #[test]
    fn test_methalox_snapshot() {
        let design = create_engine(FuelType::Methalox, 1.0);
        let snap = design.snapshot(3, "Methalox");
        // Default Methalox cycle = GasGenerator (all cycle multipliers = 1.0)
        // complexity = 6, baseline = 6: cost multiplier = 1.0
        assert!((snap.mass_kg - 400.0).abs() < 0.1); // 400 * 1.0
        assert!((snap.thrust_kn - 400.0).abs() < 0.1); // 400 * 1.0
        assert!((snap.exhaust_velocity_ms - 3300.0).abs() < 0.1); // 3300 * 1.0
        assert_eq!(snap.fuel_type, FuelType::Methalox);
        assert_eq!(snap.propellant_density, costs::METHALOX_DENSITY_KG_M3);
        assert_eq!(snap.tank_mass_ratio, costs::METHALOX_TANK_MASS_RATIO);
        assert!(!snap.is_solid);
        assert!(snap.fixed_mass_ratio.is_none());
        assert_eq!(snap.flaw_category, FlawCategory::LiquidEngine);
    }

    #[test]
    fn test_hypergolic_snapshot() {
        let design = create_engine(FuelType::Hypergolic, 1.0);
        let snap = design.snapshot(4, "Hypergolic");
        // Default Hypergolic cycle = PressureFed: mass*0.7, thrust*0.6, ve*0.92
        // complexity = 1, baseline = 6: cost multiplier = (1/6)^2 ≈ 0.028
        assert!((snap.mass_kg - 140.0).abs() < 0.1); // 200 * 0.7
        assert!((snap.thrust_kn - 30.0).abs() < 0.1); // 50 * 0.6
        assert!((snap.exhaust_velocity_ms - 2576.0).abs() < 0.1); // 2800 * 0.92
        assert_eq!(snap.fuel_type, FuelType::Hypergolic);
        assert_eq!(snap.propellant_density, costs::HYPERGOLIC_DENSITY_KG_M3);
        assert_eq!(snap.tank_mass_ratio, costs::HYPERGOLIC_TANK_MASS_RATIO);
        assert!(!snap.is_solid);
        assert!(snap.fixed_mass_ratio.is_none());
        assert_eq!(snap.flaw_category, FlawCategory::LiquidEngine);
    }

    #[test]
    fn test_scale_linearity() {
        let mut design = default_kerolox();
        design.scale = 2.0;
        let snap = design.snapshot(0, "Scaled");

        // Mass and thrust scale linearly
        assert_eq!(snap.mass_kg, 900.0);
        assert_eq!(snap.thrust_kn, 1000.0);
        // Cost scales linearly with mass (resource BOMs are mass-proportional)
        let expected_cost = crate::resources::engine_resource_cost(FuelType::Kerolox, 900.0);
        assert!((snap.base_cost - expected_cost).abs() < 1.0);

        // These don't scale
        assert_eq!(snap.exhaust_velocity_ms, 3000.0);
        assert_eq!(snap.propellant_density, costs::KEROLOX_DENSITY_KG_M3);
        assert_eq!(snap.tank_mass_ratio, costs::KEROLOX_TANK_MASS_RATIO);
    }

    #[test]
    fn test_default_lineages_order() {
        let lineages = default_engine_lineages();
        assert_eq!(lineages.len(), 3);
        assert_eq!(lineages[0].name, "Hydrolox");
        assert_eq!(lineages[1].name, "Kerolox");
        assert_eq!(lineages[2].name, "Solid");
    }

    #[test]
    fn test_default_snapshot_helper() {
        let snap0 = default_snapshot(0);
        assert_eq!(snap0.name, "Hydrolox");
        assert_eq!(snap0.engine_design_id, 0);

        let snap1 = default_snapshot(1);
        assert_eq!(snap1.name, "Kerolox");

        let snap2 = default_snapshot(2);
        assert_eq!(snap2.name, "Solid");
        assert!(snap2.is_solid);
    }

    #[test]
    fn test_generate_flaws() {
        let mut design = default_kerolox();
        let mut gen = FlawGenerator::new();
        design.generate_flaws(&mut gen, 1);
        assert!(design.workflow.flaws_generated);
        assert!(design.workflow.active_flaws.len() >= 3);

        // Calling again is a no-op
        let count = design.workflow.active_flaws.len();
        design.generate_flaws(&mut gen, 1);
        assert_eq!(design.workflow.active_flaws.len(), count);
    }

    #[test]
    fn test_fix_flaw_by_id() {
        let mut design = default_kerolox();
        let mut gen = FlawGenerator::new();
        design.generate_flaws(&mut gen, 1);

        // Discover first flaw
        design.workflow.active_flaws[0].discovered = true;
        let id = design.workflow.active_flaws[0].id;

        assert!(design.fix_flaw(id));
        assert_eq!(design.workflow.fixed_flaws.len(), 1);
        assert!(design.workflow.fixed_flaws[0].fixed);
    }

    #[test]
    fn test_submit_to_engineering() {
        use crate::design_workflow::DesignStatus;
        let mut design = default_kerolox();
        let mut gen = FlawGenerator::new();

        assert!(design.submit_to_engineering(&mut gen, 1));
        assert!(matches!(design.workflow.status, DesignStatus::Engineering { .. }));
        assert!(design.workflow.flaws_generated);

        // Can't submit again
        assert!(!design.submit_to_engineering(&mut gen, 1));
    }

    #[test]
    fn test_fuel_type_roundtrip() {
        let design = default_kerolox();
        assert_eq!(design.fuel_type(), FuelType::Kerolox);

        let design = default_hydrolox();
        assert_eq!(design.fuel_type(), FuelType::Hydrolox);

        let design = default_solid();
        assert_eq!(design.fuel_type(), FuelType::Solid);
    }

    #[test]
    fn test_set_fuel_type() {
        let mut design = default_kerolox();
        // Kerolox default cycle = GasGenerator
        assert_eq!(design.cycle, EngineCycle::GasGenerator);
        assert!(design.set_fuel_type(FuelType::Hydrolox));
        assert_eq!(design.fuel_type(), FuelType::Hydrolox);
        // GasGenerator is compatible with Hydrolox, so cycle stays
        assert_eq!(design.cycle, EngineCycle::GasGenerator);
        // Complexity for (Hydrolox, GasGenerator) = 6
        assert_eq!(design.complexity, 6);
    }

    #[test]
    fn test_set_fuel_type_switches_incompatible_cycle() {
        let mut design = default_kerolox();
        // Set to FullFlow first
        design.set_cycle(EngineCycle::FullFlowStagedCombustion);
        assert_eq!(design.cycle, EngineCycle::FullFlowStagedCombustion);

        // Switch to Hypergolic - FullFlow is not compatible
        assert!(design.set_fuel_type(FuelType::Hypergolic));
        assert_eq!(design.fuel_type(), FuelType::Hypergolic);
        // Should fall back to default cycle for Hypergolic = PressureFed
        assert_eq!(design.cycle, EngineCycle::PressureFed);
        assert_eq!(design.complexity, 1);
    }

    #[test]
    fn test_set_fuel_type_blocked_when_not_untested() {
        let mut design = default_kerolox();
        let mut gen = FlawGenerator::new();
        design.submit_to_engineering(&mut gen, 0);

        assert!(!design.set_fuel_type(FuelType::Solid));
        // Should still be kerolox
        assert_eq!(design.fuel_type(), FuelType::Kerolox);
    }

    #[test]
    fn test_can_modify() {
        let mut design = default_kerolox();
        assert!(design.can_modify());

        let mut gen = FlawGenerator::new();
        design.submit_to_engineering(&mut gen, 0);
        assert!(!design.can_modify());
    }

    #[test]
    fn test_set_scale() {
        let mut design = default_kerolox();
        assert!(design.set_scale(2.0));
        assert_eq!(design.scale, 2.0);

        // Clamped to bounds
        assert!(design.set_scale(0.1));
        assert_eq!(design.scale, ENGINE_SCALE_MIN);

        assert!(design.set_scale(10.0));
        assert_eq!(design.scale, ENGINE_SCALE_MAX);
    }

    #[test]
    fn test_set_scale_blocked_when_not_untested() {
        let mut design = default_kerolox();
        let mut gen = FlawGenerator::new();
        design.submit_to_engineering(&mut gen, 0);

        assert!(!design.set_scale(2.0));
        assert_eq!(design.scale, 1.0);
    }

    #[test]
    fn test_create_engine() {
        let engine = create_engine(FuelType::Hydrolox, 2.0);
        assert_eq!(engine.fuel_type(), FuelType::Hydrolox);
        assert_eq!(engine.scale, 2.0);
        assert!(engine.can_modify());
        assert_eq!(engine.cycle, EngineCycle::Expander);
    }

    #[test]
    fn test_fuel_type_index_roundtrip() {
        for i in 0..5 {
            let ft = FuelType::from_index(i).unwrap();
            assert_eq!(ft.index(), i);
        }
        assert!(FuelType::from_index(5).is_none());
    }

    // ==========================================
    // Cycle Tests
    // ==========================================

    #[test]
    fn test_default_cycles() {
        assert_eq!(default_cycle(FuelType::Kerolox), EngineCycle::GasGenerator);
        assert_eq!(default_cycle(FuelType::Hydrolox), EngineCycle::Expander);
        assert_eq!(default_cycle(FuelType::Methalox), EngineCycle::GasGenerator);
        assert_eq!(default_cycle(FuelType::Hypergolic), EngineCycle::PressureFed);
        assert_eq!(default_cycle(FuelType::Solid), EngineCycle::PressureFed);
    }

    #[test]
    fn test_default_complexity_from_cycle() {
        let kerolox = default_kerolox();
        assert_eq!(kerolox.complexity, 6); // GasGenerator
        assert_eq!(kerolox.cycle, EngineCycle::GasGenerator);

        let hydrolox = default_hydrolox();
        assert_eq!(hydrolox.complexity, 7); // Expander
        assert_eq!(hydrolox.cycle, EngineCycle::Expander);

        let solid = default_solid();
        assert_eq!(solid.complexity, 3); // PressureFed
        assert_eq!(solid.cycle, EngineCycle::PressureFed);
    }

    #[test]
    fn test_create_engine_complexity_from_cycle() {
        let e = create_engine(FuelType::Kerolox, 1.0);
        assert_eq!(e.complexity, 6);
        assert_eq!(e.cycle, EngineCycle::GasGenerator);

        let e = create_engine(FuelType::Hydrolox, 1.0);
        assert_eq!(e.complexity, 7);
        assert_eq!(e.cycle, EngineCycle::Expander);

        let e = create_engine(FuelType::Solid, 1.0);
        assert_eq!(e.complexity, 3);
        assert_eq!(e.cycle, EngineCycle::PressureFed);

        let e = create_engine(FuelType::Methalox, 1.0);
        assert_eq!(e.complexity, 6);
        assert_eq!(e.cycle, EngineCycle::GasGenerator);

        let e = create_engine(FuelType::Hypergolic, 1.0);
        assert_eq!(e.complexity, 1);
        assert_eq!(e.cycle, EngineCycle::PressureFed);
    }

    #[test]
    fn test_set_cycle() {
        let mut design = default_kerolox();
        assert!(design.set_cycle(EngineCycle::StagedCombustion));
        assert_eq!(design.cycle, EngineCycle::StagedCombustion);
        assert_eq!(design.complexity, 7);

        assert!(design.set_cycle(EngineCycle::FullFlowStagedCombustion));
        assert_eq!(design.complexity, 8);

        assert!(design.set_cycle(EngineCycle::PressureFed));
        assert_eq!(design.complexity, 4);
    }

    #[test]
    fn test_set_cycle_incompatible() {
        let mut design = default_kerolox();
        // Expander is not compatible with Kerolox
        assert!(!design.set_cycle(EngineCycle::Expander));
        assert_eq!(design.cycle, EngineCycle::GasGenerator); // unchanged
    }

    #[test]
    fn test_set_cycle_blocked_when_not_untested() {
        let mut design = default_kerolox();
        let mut gen = FlawGenerator::new();
        design.submit_to_engineering(&mut gen, 0);

        assert!(!design.set_cycle(EngineCycle::StagedCombustion));
        assert_eq!(design.cycle, EngineCycle::GasGenerator);
    }

    #[test]
    fn test_cycle_compatibility_matrix() {
        // PressureFed: all fuels
        for fuel in [FuelType::Kerolox, FuelType::Hydrolox, FuelType::Solid, FuelType::Methalox, FuelType::Hypergolic] {
            assert!(is_cycle_compatible(fuel, EngineCycle::PressureFed), "PressureFed should work with {:?}", fuel);
        }

        // GasGenerator: all liquid fuels, not solid
        assert!(is_cycle_compatible(FuelType::Kerolox, EngineCycle::GasGenerator));
        assert!(is_cycle_compatible(FuelType::Hydrolox, EngineCycle::GasGenerator));
        assert!(is_cycle_compatible(FuelType::Methalox, EngineCycle::GasGenerator));
        assert!(is_cycle_compatible(FuelType::Hypergolic, EngineCycle::GasGenerator));
        assert!(!is_cycle_compatible(FuelType::Solid, EngineCycle::GasGenerator));

        // Expander: Hydrolox and Methalox only
        assert!(!is_cycle_compatible(FuelType::Kerolox, EngineCycle::Expander));
        assert!(is_cycle_compatible(FuelType::Hydrolox, EngineCycle::Expander));
        assert!(is_cycle_compatible(FuelType::Methalox, EngineCycle::Expander));
        assert!(!is_cycle_compatible(FuelType::Hypergolic, EngineCycle::Expander));
        assert!(!is_cycle_compatible(FuelType::Solid, EngineCycle::Expander));

        // StagedCombustion: all liquid, not solid
        assert!(is_cycle_compatible(FuelType::Kerolox, EngineCycle::StagedCombustion));
        assert!(is_cycle_compatible(FuelType::Hydrolox, EngineCycle::StagedCombustion));
        assert!(is_cycle_compatible(FuelType::Methalox, EngineCycle::StagedCombustion));
        assert!(is_cycle_compatible(FuelType::Hypergolic, EngineCycle::StagedCombustion));
        assert!(!is_cycle_compatible(FuelType::Solid, EngineCycle::StagedCombustion));

        // FullFlow: Kerolox, Hydrolox, Methalox
        assert!(is_cycle_compatible(FuelType::Kerolox, EngineCycle::FullFlowStagedCombustion));
        assert!(is_cycle_compatible(FuelType::Hydrolox, EngineCycle::FullFlowStagedCombustion));
        assert!(is_cycle_compatible(FuelType::Methalox, EngineCycle::FullFlowStagedCombustion));
        assert!(!is_cycle_compatible(FuelType::Hypergolic, EngineCycle::FullFlowStagedCombustion));
        assert!(!is_cycle_compatible(FuelType::Solid, EngineCycle::FullFlowStagedCombustion));
    }

    #[test]
    fn test_valid_cycles_for_fuel() {
        let kerolox_cycles = valid_cycles_for_fuel(FuelType::Kerolox);
        assert_eq!(kerolox_cycles.len(), 4); // PressureFed, GasGen, Staged, FullFlow
        assert!(!kerolox_cycles.contains(&EngineCycle::Expander));

        let hydrolox_cycles = valid_cycles_for_fuel(FuelType::Hydrolox);
        assert_eq!(hydrolox_cycles.len(), 5); // All 5

        let solid_cycles = valid_cycles_for_fuel(FuelType::Solid);
        assert_eq!(solid_cycles.len(), 1); // PressureFed only
        assert_eq!(solid_cycles[0], EngineCycle::PressureFed);

        let hypergolic_cycles = valid_cycles_for_fuel(FuelType::Hypergolic);
        assert_eq!(hypergolic_cycles.len(), 3); // PressureFed, GasGen, Staged
    }

    #[test]
    fn test_cycle_index_roundtrip() {
        for i in 0..5 {
            let cycle = EngineCycle::from_index(i).unwrap();
            assert_eq!(cycle.index(), i);
        }
        assert!(EngineCycle::from_index(5).is_none());
    }

    #[test]
    fn test_cycle_complexity_values() {
        // Spot-check the table
        assert_eq!(cycle_complexity(FuelType::Kerolox, EngineCycle::PressureFed), Some(4));
        assert_eq!(cycle_complexity(FuelType::Kerolox, EngineCycle::GasGenerator), Some(6));
        assert_eq!(cycle_complexity(FuelType::Kerolox, EngineCycle::FullFlowStagedCombustion), Some(8));
        assert_eq!(cycle_complexity(FuelType::Hydrolox, EngineCycle::Expander), Some(7));
        assert_eq!(cycle_complexity(FuelType::Solid, EngineCycle::PressureFed), Some(3));
        assert_eq!(cycle_complexity(FuelType::Hypergolic, EngineCycle::PressureFed), Some(1));

        // Invalid combos return None
        assert_eq!(cycle_complexity(FuelType::Kerolox, EngineCycle::Expander), None);
        assert_eq!(cycle_complexity(FuelType::Solid, EngineCycle::GasGenerator), None);
    }

    #[test]
    fn test_snapshot_includes_cycle() {
        let design = default_kerolox();
        let snap = design.snapshot(0, "Test");
        assert_eq!(snap.cycle, EngineCycle::GasGenerator);
        assert_eq!(snap.complexity, 6);
    }

    #[test]
    fn test_components_for_pressure_fed_no_turbopump() {
        let comps = components_for(FuelType::Kerolox, EngineCycle::PressureFed);
        assert!(!comps.contains(&EngineComponent::Turbopump));
        assert!(comps.contains(&EngineComponent::Kerolox));

        let comps = components_for(FuelType::Kerolox, EngineCycle::GasGenerator);
        assert!(comps.contains(&EngineComponent::Turbopump));
    }

    // ==========================================
    // Cycle Performance Tests
    // ==========================================

    #[test]
    fn test_full_flow_cycle_effects_on_kerolox() {
        // Default Kerolox (GasGenerator) vs FullFlow
        let gg_snap = default_kerolox().snapshot(0, "GG");

        let mut design_ff = default_kerolox();
        design_ff.set_cycle(EngineCycle::FullFlowStagedCombustion);
        let ff_snap = design_ff.snapshot(0, "FF");

        // FullFlow: thrust*1.3, mass*1.3, ve*1.08
        assert!((ff_snap.mass_kg - 450.0 * 1.3).abs() < 0.1);
        assert!((ff_snap.thrust_kn - 500.0 * 1.3).abs() < 0.1);
        assert!((ff_snap.exhaust_velocity_ms - 3000.0 * 1.08).abs() < 0.1);

        // Heavier, more thrust, better VE than GasGenerator
        assert!(ff_snap.mass_kg > gg_snap.mass_kg);
        assert!(ff_snap.thrust_kn > gg_snap.thrust_kn);
        assert!(ff_snap.exhaust_velocity_ms > gg_snap.exhaust_velocity_ms);
    }

    #[test]
    fn test_pressure_fed_cycle_effects_on_kerolox() {
        let gg_snap = default_kerolox().snapshot(0, "GG");

        let mut design_pf = default_kerolox();
        design_pf.set_cycle(EngineCycle::PressureFed);
        let pf_snap = design_pf.snapshot(0, "PF");

        // PressureFed: thrust*0.6, mass*0.7, ve*0.92
        assert!((pf_snap.mass_kg - 450.0 * 0.7).abs() < 0.1);
        assert!((pf_snap.thrust_kn - 500.0 * 0.6).abs() < 0.1);
        assert!((pf_snap.exhaust_velocity_ms - 3000.0 * 0.92).abs() < 0.1);

        // Lighter, less thrust, worse VE than GasGenerator
        assert!(pf_snap.mass_kg < gg_snap.mass_kg);
        assert!(pf_snap.thrust_kn < gg_snap.thrust_kn);
        assert!(pf_snap.exhaust_velocity_ms < gg_snap.exhaust_velocity_ms);
    }

    #[test]
    fn test_complexity_cost_in_snapshot() {
        // GasGenerator (complexity=6): cost = raw * (6/6)^2 = raw * 1.0
        let gg_snap = default_kerolox().snapshot(0, "GG");
        let gg_raw = crate::resources::engine_resource_cost(FuelType::Kerolox, 450.0);
        assert!((gg_snap.base_cost - gg_raw).abs() < 1.0);

        // FullFlow (complexity=8): cost = raw_ff * (8/6)^2
        // raw_ff is for mass 450*1.3=585kg
        let mut design_ff = default_kerolox();
        design_ff.set_cycle(EngineCycle::FullFlowStagedCombustion);
        let ff_snap = design_ff.snapshot(0, "FF");
        let ff_raw = crate::resources::engine_resource_cost(FuelType::Kerolox, 585.0);
        let expected_ff = ff_raw * (8.0_f64 / 6.0).powi(2);
        assert!((ff_snap.base_cost - expected_ff).abs() < 1.0);

        // PressureFed (complexity=4): cost = raw_pf * (4/6)^2
        // raw_pf is for mass 450*0.7=315kg
        let mut design_pf = default_kerolox();
        design_pf.set_cycle(EngineCycle::PressureFed);
        let pf_snap = design_pf.snapshot(0, "PF");
        let pf_raw = crate::resources::engine_resource_cost(FuelType::Kerolox, 315.0);
        let expected_pf = pf_raw * (4.0_f64 / 6.0).powi(2);
        assert!((pf_snap.base_cost - expected_pf).abs() < 1.0);

        // FullFlow more expensive than GasGenerator, PressureFed cheaper
        assert!(ff_snap.base_cost > gg_snap.base_cost);
        assert!(pf_snap.base_cost < gg_snap.base_cost);
    }
}
