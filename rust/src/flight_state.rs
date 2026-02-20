use crate::engine_design::FuelType;
use crate::mission_plan::{MissionLeg, MissionPlan};
use crate::payload::Payload;
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
    pub current_leg_index: usize,
    /// Days remaining in transit for the current leg
    pub transit_days_remaining: u32,
    /// Payloads carried by this flight
    pub payloads: Vec<Payload>,
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

        // Initialize transit timer from first leg
        let transit_days_remaining = mission_plan.legs.first()
            .map(|l| l.transit_days)
            .unwrap_or(0);

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
            current_leg_index: 0,
            transit_days_remaining,
            payloads: Vec::new(),
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

    /// Advance to the next leg after completing the current one.
    /// Updates current_location to the completed leg's destination.
    /// Sets transit timer for the next leg.
    pub fn advance_leg(&mut self) {
        if let Some(leg) = self.mission_plan.legs.get(self.current_leg_index) {
            self.current_location = leg.to.to_string();
            self.current_leg_index += 1;
            self.start_current_leg_transit();
        }
    }

    /// Set the transit timer from the current leg's transit_days
    pub fn start_current_leg_transit(&mut self) {
        self.transit_days_remaining = self.mission_plan.legs
            .get(self.current_leg_index)
            .map(|l| l.transit_days)
            .unwrap_or(0);
    }

    /// Tick one day of transit. Returns true when the current leg's transit is complete.
    pub fn tick_transit_day(&mut self) -> bool {
        if self.transit_days_remaining > 0 {
            self.transit_days_remaining -= 1;
        }
        self.transit_days_remaining == 0
    }

    /// Check if all legs are completed
    pub fn all_legs_completed(&self) -> bool {
        self.current_leg_index >= self.mission_plan.legs.len()
    }

    /// Total transit days remaining across current and future legs
    pub fn total_transit_days_remaining(&self) -> u32 {
        let mut total = self.transit_days_remaining;
        for leg in self.mission_plan.legs.iter().skip(self.current_leg_index + 1) {
            total += leg.transit_days;
        }
        total
    }

    /// Mark flight as failed at a specific leg.
    /// Location stays at the failed leg's origin (stranded at previous location).
    pub fn fail_at_leg(&mut self, leg_index: usize) {
        self.status = FlightStatus::Failed;
        if let Some(leg) = self.mission_plan.legs.get(leg_index) {
            self.current_location = leg.from.to_string();
        }
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

    /// Remove and return all payloads whose destination matches the given location_id.
    /// Also reduces payload_mass_kg by the delivered mass.
    pub fn deliver_payloads_at(&mut self, location_id: &str) -> Vec<Payload> {
        let mut delivered = Vec::new();
        let mut i = 0;
        while i < self.payloads.len() {
            if self.payloads[i].destination == location_id {
                let p = self.payloads.remove(i);
                self.payload_mass_kg = (self.payload_mass_kg - p.mass_kg).max(0.0);
                delivered.push(p);
            } else {
                i += 1;
            }
        }
        delivered
    }

    /// Whether the flight is still active (InTransit or AtLocation).
    pub fn is_active(&self) -> bool {
        matches!(self.status, FlightStatus::InTransit | FlightStatus::AtLocation)
    }

    /// Total reward across all payloads.
    pub fn total_reward(&self) -> f64 {
        self.payloads.iter().map(|p| p.reward()).sum()
    }

    /// All contract IDs from payloads.
    pub fn contract_ids(&self) -> Vec<u32> {
        self.payloads.iter().filter_map(|p| p.contract_id()).collect()
    }

    /// Whether any payload is a contract satellite.
    pub fn has_contract_payload(&self) -> bool {
        self.payloads.iter().any(|p| p.is_contract())
    }

    /// Whether any payload is a fuel depot.
    pub fn has_depot_payload(&self) -> bool {
        self.payloads.iter().any(|p| p.is_depot())
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
    fn test_advance_leg() {
        let design = RocketDesign::default_design();
        let geo_plan = MissionPlan::from_shortest_path("earth_surface", "geo").unwrap();
        let mut flight = FlightState::from_design(1, 0, 1, &design, "geo", geo_plan);

        assert_eq!(flight.current_leg_index, 0);
        assert_eq!(flight.current_location, "earth_surface");

        // Advance through leg 0: earth_surface -> leo
        flight.advance_leg();
        assert_eq!(flight.current_leg_index, 1);
        assert_eq!(flight.current_location, "leo");

        // Advance through leg 1: leo -> gto
        flight.advance_leg();
        assert_eq!(flight.current_leg_index, 2);
        assert_eq!(flight.current_location, "gto");

        // Advance through leg 2: gto -> geo
        flight.advance_leg();
        assert_eq!(flight.current_leg_index, 3);
        assert_eq!(flight.current_location, "geo");
    }

    #[test]
    fn test_fail_at_leg() {
        let design = RocketDesign::default_design();
        let geo_plan = MissionPlan::from_shortest_path("earth_surface", "geo").unwrap();
        let mut flight = FlightState::from_design(1, 0, 1, &design, "geo", geo_plan);

        // Advance leg 0 successfully
        flight.advance_leg();
        assert_eq!(flight.current_location, "leo");

        // Fail at leg 1 (leo -> gto): stranded at leo
        flight.fail_at_leg(1);
        assert_eq!(flight.status, FlightStatus::Failed);
        assert_eq!(flight.current_location, "leo");
    }

    #[test]
    fn test_fail_at_surface_leg() {
        let design = RocketDesign::default_design();
        let geo_plan = MissionPlan::from_shortest_path("earth_surface", "geo").unwrap();
        let mut flight = FlightState::from_design(1, 0, 1, &design, "geo", geo_plan);

        // Fail at leg 0 (earth_surface -> leo): stranded at earth_surface
        flight.fail_at_leg(0);
        assert_eq!(flight.status, FlightStatus::Failed);
        assert_eq!(flight.current_location, "earth_surface");
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

    #[test]
    fn test_tick_transit_day_zero_transit() {
        let design = RocketDesign::default_design();
        let mut flight = FlightState::from_design(1, 0, 1, &design, "leo", leo_plan());

        // LEO has 0 transit days — first tick should complete immediately
        assert_eq!(flight.transit_days_remaining, 0);
        assert!(flight.tick_transit_day());
    }

    #[test]
    fn test_tick_transit_day_multi_day() {
        let design = RocketDesign::default_design();
        // GEO: earth_surface(0) -> leo(1) -> gto(0) -> geo
        let geo_plan = MissionPlan::from_shortest_path("earth_surface", "geo").unwrap();
        let mut flight = FlightState::from_design(1, 0, 1, &design, "geo", geo_plan);

        // First leg: earth_surface -> leo, 0 transit days
        assert_eq!(flight.transit_days_remaining, 0);
        assert!(flight.tick_transit_day()); // completes immediately
        flight.advance_leg();
        assert_eq!(flight.current_location, "leo");

        // Second leg: leo -> gto, 1 transit day
        assert_eq!(flight.transit_days_remaining, 1);
        // tick: 1 -> 0, returns true (transit complete for this leg)
        assert!(flight.tick_transit_day());
        flight.advance_leg();
        assert_eq!(flight.current_location, "gto");

        // Third leg: gto -> geo, 0 transit days
        assert_eq!(flight.transit_days_remaining, 0);
        assert!(flight.tick_transit_day());
        flight.advance_leg();
        assert_eq!(flight.current_location, "geo");
        assert!(flight.all_legs_completed());
    }

    #[test]
    fn test_tick_transit_multi_leg_with_transit() {
        let design = RocketDesign::default_design();
        // lunar_surface: earth_surface(0) -> leo(4) -> lunar_orbit(0) -> lunar_surface
        let plan = MissionPlan::from_shortest_path("earth_surface", "lunar_surface").unwrap();
        let mut flight = FlightState::from_design(1, 0, 1, &design, "lunar_surface", plan);

        // Leg 0: earth_surface -> leo, 0 transit
        assert_eq!(flight.transit_days_remaining, 0);
        assert!(flight.tick_transit_day());
        flight.advance_leg();
        assert_eq!(flight.current_location, "leo");

        // Leg 1: leo -> lunar_orbit, 4 transit days
        assert_eq!(flight.transit_days_remaining, 4);
        for day in 0..3 {
            assert!(!flight.tick_transit_day(), "Day {} should not complete", day);
        }
        assert!(flight.tick_transit_day()); // Day 4 completes
        flight.advance_leg();
        assert_eq!(flight.current_location, "lunar_orbit");

        // Leg 2: lunar_orbit -> lunar_surface, 0 transit
        assert_eq!(flight.transit_days_remaining, 0);
        assert!(flight.tick_transit_day());
        flight.advance_leg();
        assert_eq!(flight.current_location, "lunar_surface");
        assert!(flight.all_legs_completed());
    }

    #[test]
    fn test_total_transit_days_remaining() {
        let design = RocketDesign::default_design();
        let plan = MissionPlan::from_shortest_path("earth_surface", "lunar_surface").unwrap();
        let mut flight = FlightState::from_design(1, 0, 1, &design, "lunar_surface", plan);

        // At start: 0 (leg 0) + 4 (leg 1) + 0 (leg 2) = 4
        assert_eq!(flight.total_transit_days_remaining(), 4);

        // After completing leg 0
        flight.tick_transit_day();
        flight.advance_leg();
        // Now at leg 1: 4 remaining + 0 (leg 2) = 4
        assert_eq!(flight.total_transit_days_remaining(), 4);

        // After 2 ticks of leg 1
        flight.tick_transit_day();
        flight.tick_transit_day();
        assert_eq!(flight.total_transit_days_remaining(), 2);
    }

    #[test]
    fn test_flight_payload_helpers() {
        use crate::payload::Payload;

        let design = RocketDesign::default_design();
        let mut flight = FlightState::from_design(1, 0, 1, &design, "leo", leo_plan());

        // Default: no payloads
        assert!(!flight.has_contract_payload());
        assert!(!flight.has_depot_payload());
        assert_eq!(flight.total_reward(), 0.0);
        assert!(flight.contract_ids().is_empty());

        // Add a contract payload
        flight.payloads.push(Payload::contract_satellite(
            1, 42, "Comms".to_string(), 500.0, 5_000_000.0, "leo".to_string(),
        ));
        assert!(flight.has_contract_payload());
        assert!(!flight.has_depot_payload());
        assert_eq!(flight.total_reward(), 5_000_000.0);
        assert_eq!(flight.contract_ids(), vec![42]);
    }
}
