use crate::engine::{costs, EngineStatus};
use crate::flaw::{Flaw, FlawCategory, FlawGenerator};

pub const ENGINE_SCALE_MIN: f64 = 0.25;
pub const ENGINE_SCALE_MAX: f64 = 4.0;
pub const ENGINE_SCALE_STEP: f64 = 0.25;

/// Chemical engine fuel types.
/// Future propulsion categories (nuclear pulse, sail) would be a separate enum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FuelType {
    Kerolox,
    Hydrolox,
    Solid,
}

impl FuelType {
    pub fn components(&self) -> Vec<EngineComponent> {
        match self {
            FuelType::Kerolox => vec![EngineComponent::Kerolox, EngineComponent::Turbopump],
            FuelType::Hydrolox => vec![EngineComponent::Hydrolox, EngineComponent::Turbopump],
            FuelType::Solid => vec![EngineComponent::SolidMotor],
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            FuelType::Kerolox => "Kerolox",
            FuelType::Hydrolox => "Hydrolox",
            FuelType::Solid => "Solid",
        }
    }

    pub fn from_index(i: usize) -> Option<FuelType> {
        match i {
            0 => Some(FuelType::Kerolox),
            1 => Some(FuelType::Hydrolox),
            2 => Some(FuelType::Solid),
            _ => None,
        }
    }

    pub fn index(&self) -> usize {
        match self {
            FuelType::Kerolox => 0,
            FuelType::Hydrolox => 1,
            FuelType::Solid => 2,
        }
    }
}

/// Components that make up an engine design.
/// Propellant chemistry determines base performance; Turbopump is required for liquids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EngineComponent {
    /// RP-1/LOX propellant
    Kerolox,
    /// LH2/LOX propellant
    Hydrolox,
    /// HTPB/AP solid propellant
    SolidMotor,
    /// Required for liquid engines
    Turbopump,
}

/// A designable engine: composed of components at a scale, with derived stats and hidden flaws.
#[derive(Debug, Clone)]
pub struct EngineDesign {
    pub components: Vec<EngineComponent>,
    pub scale: f64,
    // Flaw system fields (same role as old EngineSpec)
    pub active_flaws: Vec<Flaw>,
    pub fixed_flaws: Vec<Flaw>,
    pub flaws_generated: bool,
    pub status: EngineStatus,
}

/// Lightweight stats cache stored on RocketStage.
/// Avoids passing the full EngineDesign around.
#[derive(Debug, Clone)]
pub struct EngineDesignSnapshot {
    pub engine_design_id: usize,
    pub name: String,
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
    /// Whether this engine can be modified (only when Untested)
    pub fn can_modify(&self) -> bool {
        matches!(self.status, EngineStatus::Untested)
    }

    /// Get the fuel type from current components
    pub fn fuel_type(&self) -> FuelType {
        if self.components.contains(&EngineComponent::SolidMotor) {
            FuelType::Solid
        } else if self.components.contains(&EngineComponent::Hydrolox) {
            FuelType::Hydrolox
        } else {
            FuelType::Kerolox
        }
    }

    /// Set fuel type by replacing components. Returns false if not modifiable.
    pub fn set_fuel_type(&mut self, fuel: FuelType) -> bool {
        if !self.can_modify() {
            return false;
        }
        self.components = fuel.components();
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

        let (base_mass, base_thrust, ve, density, tank_ratio, base_cost, fixed_mass_ratio, flaw_category) =
            if is_solid {
                (
                    40_000.0,
                    8_000.0,
                    2650.0,
                    costs::SOLID_DENSITY_KG_M3,
                    costs::SOLID_TANK_MASS_RATIO,
                    costs::SOLID_ENGINE_COST,
                    Some(costs::SOLID_MASS_RATIO),
                    FlawCategory::SolidMotor,
                )
            } else if self.components.contains(&EngineComponent::Hydrolox) {
                (
                    300.0,
                    100.0,
                    4500.0,
                    costs::HYDROLOX_DENSITY_KG_M3,
                    costs::HYDROLOX_TANK_MASS_RATIO,
                    costs::HYDROLOX_ENGINE_COST,
                    None,
                    FlawCategory::LiquidEngine,
                )
            } else {
                // Kerolox (default liquid)
                (
                    450.0,
                    500.0,
                    3000.0,
                    costs::KEROLOX_DENSITY_KG_M3,
                    costs::KEROLOX_TANK_MASS_RATIO,
                    costs::KEROLOX_ENGINE_COST,
                    None,
                    FlawCategory::LiquidEngine,
                )
            };

        EngineDesignSnapshot {
            engine_design_id: id,
            name: name.to_string(),
            mass_kg: base_mass * self.scale,
            thrust_kn: base_thrust * self.scale,
            exhaust_velocity_ms: ve,
            base_cost: base_cost * self.scale,
            propellant_density: density,
            tank_mass_ratio: tank_ratio,
            is_solid,
            fixed_mass_ratio,
            flaw_category,
        }
    }

    // ==========================================
    // Flaw Management (moved from EngineSpec)
    // ==========================================

    /// Generate flaws for this engine if not already generated
    pub fn generate_flaws(&mut self, generator: &mut FlawGenerator, engine_design_id: usize) {
        if self.flaws_generated {
            return;
        }

        let category = if self.components.contains(&EngineComponent::SolidMotor) {
            FlawCategory::SolidMotor
        } else {
            FlawCategory::LiquidEngine
        };
        self.active_flaws = generator.generate_engine_flaws_for_type_with_category(
            engine_design_id,
            category,
        );
        self.fixed_flaws.clear();
        self.flaws_generated = true;
    }

    /// Get active (unfixed) flaws
    pub fn get_active_flaws(&self) -> &[Flaw] {
        &self.active_flaws
    }

    /// Get fixed flaws
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
    pub fn fix_flaw(&mut self, id: u32) -> bool {
        if let Some(index) = self.active_flaws.iter().position(|f| f.id == id && f.discovered) {
            let mut flaw = self.active_flaws.remove(index);
            flaw.fixed = true;
            self.fixed_flaws.push(flaw);
            return true;
        }
        false
    }

    /// Fix a flaw by index - moves from active_flaws to fixed_flaws
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
    pub fn submit_to_refining(&mut self, generator: &mut FlawGenerator, engine_design_id: usize) -> bool {
        if !matches!(self.status, EngineStatus::Untested) {
            return false;
        }
        if !self.flaws_generated {
            self.generate_flaws(generator, engine_design_id);
        }
        self.status.start_refining();
        true
    }
}

// ==========================================
// Engine Creation Functions
// ==========================================

/// Create an engine design with the given fuel type and scale
pub fn create_engine(fuel: FuelType, scale: f64) -> EngineDesign {
    EngineDesign {
        components: fuel.components(),
        scale: scale.clamp(ENGINE_SCALE_MIN, ENGINE_SCALE_MAX),
        active_flaws: Vec::new(),
        fixed_flaws: Vec::new(),
        flaws_generated: false,
        status: EngineStatus::Untested,
    }
}

// ==========================================
// Default Engine Creation Functions
// ==========================================

/// Create the default Kerolox engine design (index 0 in the legacy mapping was Hydrolox,
/// but we keep the same indices: 0=Hydrolox, 1=Kerolox, 2=Solid)
pub fn default_kerolox() -> EngineDesign {
    EngineDesign {
        components: vec![EngineComponent::Kerolox, EngineComponent::Turbopump],
        scale: 1.0,
        active_flaws: Vec::new(),
        fixed_flaws: Vec::new(),
        flaws_generated: false,
        status: EngineStatus::Untested,
    }
}

pub fn default_hydrolox() -> EngineDesign {
    EngineDesign {
        components: vec![EngineComponent::Hydrolox, EngineComponent::Turbopump],
        scale: 1.0,
        active_flaws: Vec::new(),
        fixed_flaws: Vec::new(),
        flaws_generated: false,
        status: EngineStatus::Untested,
    }
}

pub fn default_solid() -> EngineDesign {
    EngineDesign {
        components: vec![EngineComponent::SolidMotor],
        scale: 1.0,
        active_flaws: Vec::new(),
        fixed_flaws: Vec::new(),
        flaws_generated: false,
        status: EngineStatus::Untested,
    }
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
    fn test_kerolox_snapshot_matches_old_spec() {
        let design = default_kerolox();
        let snap = design.snapshot(1, "Kerolox");
        assert_eq!(snap.mass_kg, 450.0);
        assert_eq!(snap.thrust_kn, 500.0);
        assert_eq!(snap.exhaust_velocity_ms, 3000.0);
        assert_eq!(snap.base_cost, costs::KEROLOX_ENGINE_COST);
        assert_eq!(snap.propellant_density, costs::KEROLOX_DENSITY_KG_M3);
        assert_eq!(snap.tank_mass_ratio, costs::KEROLOX_TANK_MASS_RATIO);
        assert!(!snap.is_solid);
        assert!(snap.fixed_mass_ratio.is_none());
        assert_eq!(snap.flaw_category, FlawCategory::LiquidEngine);
    }

    #[test]
    fn test_hydrolox_snapshot_matches_old_spec() {
        let design = default_hydrolox();
        let snap = design.snapshot(0, "Hydrolox");
        assert_eq!(snap.mass_kg, 300.0);
        assert_eq!(snap.thrust_kn, 100.0);
        assert_eq!(snap.exhaust_velocity_ms, 4500.0);
        assert_eq!(snap.base_cost, costs::HYDROLOX_ENGINE_COST);
        assert!(!snap.is_solid);
    }

    #[test]
    fn test_solid_snapshot_matches_old_spec() {
        let design = default_solid();
        let snap = design.snapshot(2, "Solid");
        assert_eq!(snap.mass_kg, 40_000.0);
        assert_eq!(snap.thrust_kn, 8_000.0);
        assert_eq!(snap.exhaust_velocity_ms, 2650.0);
        assert!(snap.is_solid);
        assert!((snap.fixed_mass_ratio.unwrap() - 0.88).abs() < 0.01);
        assert_eq!(snap.flaw_category, FlawCategory::SolidMotor);
    }

    #[test]
    fn test_scale_linearity() {
        let mut design = default_kerolox();
        design.scale = 2.0;
        let snap = design.snapshot(0, "Scaled");

        // Mass, thrust, cost scale linearly
        assert_eq!(snap.mass_kg, 900.0);
        assert_eq!(snap.thrust_kn, 1000.0);
        assert_eq!(snap.base_cost, costs::KEROLOX_ENGINE_COST * 2.0);

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
        assert!(design.flaws_generated);
        assert!(design.active_flaws.len() >= 3);

        // Calling again is a no-op
        let count = design.active_flaws.len();
        design.generate_flaws(&mut gen, 1);
        assert_eq!(design.active_flaws.len(), count);
    }

    #[test]
    fn test_fix_flaw_by_id() {
        let mut design = default_kerolox();
        let mut gen = FlawGenerator::new();
        design.generate_flaws(&mut gen, 1);

        // Discover first flaw
        design.active_flaws[0].discovered = true;
        let id = design.active_flaws[0].id;

        assert!(design.fix_flaw(id));
        assert_eq!(design.fixed_flaws.len(), 1);
        assert!(design.fixed_flaws[0].fixed);
    }

    #[test]
    fn test_submit_to_refining() {
        let mut design = default_kerolox();
        let mut gen = FlawGenerator::new();

        assert!(design.submit_to_refining(&mut gen, 1));
        assert!(matches!(design.status, EngineStatus::Refining { .. }));
        assert!(design.flaws_generated);

        // Can't submit again
        assert!(!design.submit_to_refining(&mut gen, 1));
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
        assert!(design.set_fuel_type(FuelType::Hydrolox));
        assert_eq!(design.fuel_type(), FuelType::Hydrolox);

        // Verify snapshot uses new fuel type
        let snap = design.snapshot(0, "Changed");
        assert_eq!(snap.exhaust_velocity_ms, 4500.0);
    }

    #[test]
    fn test_set_fuel_type_blocked_when_not_untested() {
        let mut design = default_kerolox();
        let mut gen = FlawGenerator::new();
        design.submit_to_refining(&mut gen, 0);

        assert!(!design.set_fuel_type(FuelType::Solid));
        // Should still be kerolox
        assert_eq!(design.fuel_type(), FuelType::Kerolox);
    }

    #[test]
    fn test_can_modify() {
        let mut design = default_kerolox();
        assert!(design.can_modify());

        let mut gen = FlawGenerator::new();
        design.submit_to_refining(&mut gen, 0);
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
        design.submit_to_refining(&mut gen, 0);

        assert!(!design.set_scale(2.0));
        assert_eq!(design.scale, 1.0);
    }

    #[test]
    fn test_create_engine() {
        let engine = create_engine(FuelType::Hydrolox, 2.0);
        assert_eq!(engine.fuel_type(), FuelType::Hydrolox);
        assert_eq!(engine.scale, 2.0);
        assert!(engine.can_modify());
    }

    #[test]
    fn test_fuel_type_index_roundtrip() {
        for i in 0..3 {
            let ft = FuelType::from_index(i).unwrap();
            assert_eq!(ft.index(), i);
        }
        assert!(FuelType::from_index(3).is_none());
    }
}
