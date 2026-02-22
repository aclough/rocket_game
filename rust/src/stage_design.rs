/// Stage design: a first-class design object wrapping a RocketStage with identity and workflow.
///
/// RocketStage remains the physics snapshot type (mass, thrust, delta-v, costs).
/// StageDesign adds: unique ID, name, classification, and an independent engineering workflow.

use crate::design_workflow::DesignWorkflow;
use crate::engine_design::EngineDesignSnapshot;
use crate::flaw::FlawGenerator;
use crate::resources::TankMaterial;
use crate::stage::RocketStage;

/// Unique identifier for a stage design.
pub type StageDesignId = u32;

/// Classification of what role a stage plays.
#[derive(Debug, Clone, PartialEq)]
pub enum StageClass {
    /// First stage or strap-on — flies through atmosphere
    Booster,
    /// Upper stage — also flies through atmosphere during ascent
    UpperStage,
    // Spacecraft — deferred to Phase 3
}

/// A stage design with identity and engineering workflow.
#[derive(Debug, Clone)]
pub struct StageDesign {
    /// Unique ID for this stage design
    pub id: StageDesignId,
    /// Human-readable name
    pub name: String,
    /// Physics core — all mass/thrust/cost calculations live on RocketStage
    pub stage: RocketStage,
    /// Independent engineering/testing pipeline
    pub workflow: DesignWorkflow,
}

impl StageDesign {
    /// Create a new stage design wrapping the given RocketStage.
    pub fn new(id: StageDesignId, name: String, stage: RocketStage) -> Self {
        Self {
            id,
            name,
            stage,
            workflow: DesignWorkflow::new(),
        }
    }

    /// Derive the stage class from the underlying RocketStage.
    pub fn stage_class(&self) -> StageClass {
        if self.stage.is_booster {
            StageClass::Booster
        } else {
            StageClass::UpperStage
        }
    }

    /// Whether this stage design can be modified (only when in Specification)
    pub fn can_modify(&self) -> bool {
        self.workflow.status.can_edit()
    }

    /// Set the engine for this stage. Guarded by can_modify().
    pub fn set_engine(&mut self, engine_design_id: usize, snapshot: EngineDesignSnapshot) -> bool {
        if !self.can_modify() {
            return false;
        }
        self.stage.update_snapshot(snapshot);
        self.stage.engine_design_id = engine_design_id;
        true
    }

    /// Set the engine count. Guarded by can_modify().
    pub fn set_engine_count(&mut self, count: u32) -> bool {
        if !self.can_modify() {
            return false;
        }
        self.stage.set_engine_count(count);
        true
    }

    /// Set the propellant mass in kg. Guarded by can_modify().
    /// No-op for solid motors (they have fixed mass ratio).
    pub fn set_propellant_mass(&mut self, mass_kg: f64) -> bool {
        if !self.can_modify() {
            return false;
        }
        if self.stage.is_solid() {
            return false;
        }
        self.stage.propellant_mass_kg = mass_kg.max(0.0);
        true
    }

    /// Set whether this stage is a booster. Guarded by can_modify().
    pub fn set_is_booster(&mut self, is_booster: bool) -> bool {
        if !self.can_modify() {
            return false;
        }
        self.stage.is_booster = is_booster;
        true
    }

    /// Set the tank material. Guarded by can_modify().
    pub fn set_tank_material(&mut self, material: TankMaterial) -> bool {
        if !self.can_modify() {
            return false;
        }
        self.stage.tank_material = material;
        true
    }

    /// Submit stage design to engineering (generates flaws and transitions workflow).
    pub fn submit_to_engineering(&mut self, generator: &mut FlawGenerator, stage_design_index: usize) -> bool {
        if !self.workflow.status.can_edit() {
            return false;
        }
        if !self.workflow.flaws_generated {
            self.generate_flaws(generator, stage_design_index);
        }
        self.workflow.submit_to_engineering()
    }

    /// Generate flaws for this stage design based on engine count and tank material.
    pub fn generate_flaws(&mut self, generator: &mut FlawGenerator, stage_design_index: usize) {
        if self.workflow.flaws_generated {
            return;
        }
        self.workflow.active_flaws = generator.generate_stage_flaws(
            stage_design_index,
            self.stage.engine_count,
            self.stage.tank_material,
        );
        self.workflow.fixed_flaws.clear();
        self.workflow.flaws_generated = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_design::default_snapshot;
    use crate::design_workflow::DesignStatus;

    fn make_stage_design() -> StageDesign {
        let stage = RocketStage::new(default_snapshot(1)); // Kerolox
        StageDesign::new(1, "Test Stage".to_string(), stage)
    }

    #[test]
    fn test_new_stage_design() {
        let sd = make_stage_design();
        assert_eq!(sd.id, 1);
        assert_eq!(sd.name, "Test Stage");
        assert_eq!(sd.stage.engine_count, 1);
        assert!(matches!(sd.workflow.status, DesignStatus::Specification));
    }

    #[test]
    fn test_stage_class_upper() {
        let sd = make_stage_design();
        assert_eq!(sd.stage_class(), StageClass::UpperStage);
    }

    #[test]
    fn test_stage_class_booster() {
        let mut stage = RocketStage::new(default_snapshot(1));
        stage.is_booster = true;
        let sd = StageDesign::new(2, "Booster".to_string(), stage);
        assert_eq!(sd.stage_class(), StageClass::Booster);
    }

    #[test]
    fn test_physics_delegation() {
        let mut stage = RocketStage::new(default_snapshot(1));
        stage.engine_count = 3;
        stage.propellant_mass_kg = 10000.0;
        let sd = StageDesign::new(1, "Test".to_string(), stage);

        assert_eq!(sd.stage.engine_count, 3);
        assert!(sd.stage.total_thrust_kn() > 0.0);
        assert!(sd.stage.dry_mass_kg() > 0.0);
    }

    #[test]
    fn test_can_modify() {
        let mut sd = make_stage_design();
        assert!(sd.can_modify());

        // After submitting to engineering, can't modify
        let mut gen = FlawGenerator::new();
        sd.submit_to_engineering(&mut gen, 0);
        assert!(!sd.can_modify());
    }

    #[test]
    fn test_set_engine_count() {
        let mut sd = make_stage_design();
        assert!(sd.set_engine_count(5));
        assert_eq!(sd.stage.engine_count, 5);

        // Can't set below 1
        assert!(sd.set_engine_count(0));
        assert_eq!(sd.stage.engine_count, 1);
    }

    #[test]
    fn test_set_propellant_mass() {
        let mut sd = make_stage_design();
        assert!(sd.set_propellant_mass(5000.0));
        assert_eq!(sd.stage.propellant_mass_kg, 5000.0);

        // Negative clamped to 0
        assert!(sd.set_propellant_mass(-100.0));
        assert_eq!(sd.stage.propellant_mass_kg, 0.0);
    }

    #[test]
    fn test_set_is_booster() {
        let mut sd = make_stage_design();
        assert!(!sd.stage.is_booster);
        assert!(sd.set_is_booster(true));
        assert!(sd.stage.is_booster);
    }

    #[test]
    fn test_set_tank_material() {
        let mut sd = make_stage_design();
        assert!(sd.set_tank_material(TankMaterial::CarbonComposite));
        assert_eq!(sd.stage.tank_material, TankMaterial::CarbonComposite);
    }

    #[test]
    fn test_set_engine() {
        let mut sd = make_stage_design();
        let hydrolox_snap = default_snapshot(0); // Hydrolox
        assert!(sd.set_engine(0, hydrolox_snap));
        assert_eq!(sd.stage.engine_design_id, 0);
    }

    #[test]
    fn test_setters_guarded_after_submit() {
        let mut sd = make_stage_design();
        let mut gen = FlawGenerator::new();
        sd.submit_to_engineering(&mut gen, 0);

        assert!(!sd.set_engine_count(5));
        assert!(!sd.set_propellant_mass(5000.0));
        assert!(!sd.set_is_booster(true));
        assert!(!sd.set_tank_material(TankMaterial::CarbonComposite));
        assert!(!sd.set_engine(0, default_snapshot(0)));
    }

    #[test]
    fn test_submit_to_engineering() {
        let mut sd = make_stage_design();
        let mut gen = FlawGenerator::new();
        assert!(sd.submit_to_engineering(&mut gen, 0));
        assert!(matches!(sd.workflow.status, DesignStatus::Engineering { .. }));
        assert!(sd.workflow.flaws_generated);

        // Can't submit again
        assert!(!sd.submit_to_engineering(&mut gen, 0));
    }
}
