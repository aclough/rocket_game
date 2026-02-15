use crate::engine_design::FuelType;
use crate::mission_plan::{MissionLeg, MissionPlan};
use crate::rocket_design::RocketDesign;
use std::collections::BTreeMap;

pub type FlightId = u32;

#[derive(Debug, Clone)]
pub struct StageFlightState {
    pub stage_index: usize,
    pub propellant_remaining_kg: f64,
    pub attached: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlightStatus {
    InTransit,
    AtLocation,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct FlightState {
    pub id: FlightId,
    pub design_lineage_index: usize,
    pub revision_number: u32,
    pub current_location: String,
    pub destination: String,
    pub stages: Vec<StageFlightState>,
    pub payload_mass_kg: f64,
    pub status: FlightStatus,
    pub mission_plan: MissionPlan,
}

impl FlightState {
    /// Initialize a flight from a rocket design.
    /// Each stage starts with full propellant and attached.
    pub fn from_design(
        id: FlightId,
        lineage_index: usize,
        revision_number: u32,
        design: &RocketDesign,
        destination: &str,
        mission_plan: MissionPlan,
    ) -> Self {
        let stages = design
            .stages
            .iter()
            .enumerate()
            .map(|(i, stage)| StageFlightState {
                stage_index: i,
                propellant_remaining_kg: stage.propellant_mass_kg,
                attached: true,
            })
            .collect();

        Self {
            id,
            design_lineage_index: lineage_index,
            revision_number,
            current_location: "earth_surface".to_string(),
            destination: destination.to_string(),
            stages,
            payload_mass_kg: design.payload_mass_kg,
            status: FlightStatus::InTransit,
            mission_plan,
        }
    }

    /// Number of legs in the mission plan.
    pub fn leg_count(&self) -> usize {
        self.mission_plan.leg_count()
    }

    /// Get a specific leg of the mission plan.
    pub fn leg(&self, index: usize) -> Option<&MissionLeg> {
        self.mission_plan.legs.get(index)
    }

    /// Mark flight as completed. Updates location and propellant from design data.
    pub fn complete(&mut self, design: &RocketDesign) {
        self.status = FlightStatus::Completed;
        self.current_location = self.destination.clone();

        // Update per-stage propellant remaining from design calculations
        let remaining = design.propellant_remaining_kg();
        for (stage_idx, remaining_kg) in &remaining {
            if let Some(stage) = self.stages.iter_mut().find(|s| s.stage_index == *stage_idx) {
                stage.propellant_remaining_kg = *remaining_kg;
            }
        }

        // Stages not in the remaining list were fully burned — mark as jettisoned with 0 propellant
        for stage in &mut self.stages {
            if !remaining.iter().any(|(idx, _)| *idx == stage.stage_index) {
                stage.propellant_remaining_kg = 0.0;
                stage.attached = false;
            }
        }
    }

    /// Mark flight as failed.
    pub fn fail(&mut self) {
        self.status = FlightStatus::Failed;
    }

    /// Sum of propellant remaining across all attached stages.
    pub fn total_propellant_remaining_kg(&self) -> f64 {
        self.stages
            .iter()
            .filter(|s| s.attached)
            .map(|s| s.propellant_remaining_kg)
            .sum()
    }

    /// Group remaining propellant by fuel type using the design's stage snapshots.
    pub fn propellant_remaining_by_fuel_type(&self, design: &RocketDesign) -> Vec<(FuelType, f64)> {
        let mut by_type: BTreeMap<FuelType, f64> = BTreeMap::new();
        for stage_state in &self.stages {
            if let Some(stage) = design.stages.get(stage_state.stage_index) {
                let fuel_type = stage.engine_snapshot().fuel_type;
                *by_type.entry(fuel_type).or_insert(0.0) += stage_state.propellant_remaining_kg;
            }
        }
        by_type.into_iter().collect()
    }

    /// Whether the flight is still active (InTransit or AtLocation).
    pub fn is_active(&self) -> bool {
        matches!(self.status, FlightStatus::InTransit | FlightStatus::AtLocation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rocket_design::RocketDesign;

    fn leo_plan() -> MissionPlan {
        MissionPlan::from_shortest_path("earth_surface", "leo").unwrap()
    }

    #[test]
    fn test_flight_from_design() {
        let design = RocketDesign::default_design();
        let flight = FlightState::from_design(1, 0, 1, &design, "leo", leo_plan());

        assert_eq!(flight.id, 1);
        assert_eq!(flight.design_lineage_index, 0);
        assert_eq!(flight.revision_number, 1);
        assert_eq!(flight.current_location, "earth_surface");
        assert_eq!(flight.destination, "leo");
        assert_eq!(flight.status, FlightStatus::InTransit);
        assert_eq!(flight.stages.len(), design.stages.len());

        // All stages start attached with full propellant
        for (i, stage) in flight.stages.iter().enumerate() {
            assert_eq!(stage.stage_index, i);
            assert!(stage.attached);
            assert!((stage.propellant_remaining_kg - design.stages[i].propellant_mass_kg).abs() < 0.01);
        }
    }

    #[test]
    fn test_flight_complete() {
        let design = RocketDesign::default_design();
        assert!(design.is_sufficient(), "Default design should be sufficient");

        let mut flight = FlightState::from_design(1, 0, 1, &design, "leo", leo_plan());
        flight.complete(&design);

        assert_eq!(flight.status, FlightStatus::Completed);
        assert_eq!(flight.current_location, "leo");
    }

    #[test]
    fn test_flight_fail() {
        let design = RocketDesign::default_design();
        let mut flight = FlightState::from_design(1, 0, 1, &design, "leo", leo_plan());
        flight.fail();

        assert_eq!(flight.status, FlightStatus::Failed);
        // Location stays at earth_surface on failure
        assert_eq!(flight.current_location, "earth_surface");
    }

    #[test]
    fn test_total_propellant_remaining() {
        let design = RocketDesign::default_design();
        let flight = FlightState::from_design(1, 0, 1, &design, "leo", leo_plan());

        let total = flight.total_propellant_remaining_kg();
        let expected: f64 = design.stages.iter().map(|s| s.propellant_mass_kg).sum();
        assert!((total - expected).abs() < 0.01);
    }

    #[test]
    fn test_is_active() {
        let design = RocketDesign::default_design();

        let mut flight = FlightState::from_design(1, 0, 1, &design, "leo", leo_plan());
        assert!(flight.is_active()); // InTransit

        flight.status = FlightStatus::AtLocation;
        assert!(flight.is_active());

        flight.status = FlightStatus::Completed;
        assert!(!flight.is_active());

        flight.status = FlightStatus::Failed;
        assert!(!flight.is_active());
    }
}
