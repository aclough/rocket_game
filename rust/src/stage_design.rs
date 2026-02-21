/// Stage design: a first-class design object wrapping a RocketStage with identity and workflow.
///
/// RocketStage remains the physics snapshot type (mass, thrust, delta-v, costs).
/// StageDesign adds: unique ID, name, classification, and an independent engineering workflow.

use crate::design_workflow::DesignWorkflow;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_design::default_snapshot;

    #[test]
    fn test_new_stage_design() {
        let stage = RocketStage::new(default_snapshot(1)); // Kerolox
        let sd = StageDesign::new(1, "Test Stage".to_string(), stage);
        assert_eq!(sd.id, 1);
        assert_eq!(sd.name, "Test Stage");
        assert_eq!(sd.stage.engine_count, 1);
        assert!(matches!(sd.workflow.status, crate::design_workflow::DesignStatus::Specification));
    }

    #[test]
    fn test_stage_class_upper() {
        let stage = RocketStage::new(default_snapshot(1));
        let sd = StageDesign::new(1, "Upper".to_string(), stage);
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

        // Physics methods work through the stage field
        assert_eq!(sd.stage.engine_count, 3);
        assert!(sd.stage.total_thrust_kn() > 0.0);
        assert!(sd.stage.dry_mass_kg() > 0.0);
    }
}
