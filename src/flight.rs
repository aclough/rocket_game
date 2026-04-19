use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::contract::ContractId;
use crate::launch::FlawActivation;
use crate::location::DELTA_V_MAP;
use crate::rocket::{Rocket, RocketDesign};
use crate::rocket_project::RocketProjectId;

/// Unique identifier for a flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlightId(pub u64);

/// What a flight is carrying. Expandable later for fuel, crew, probes, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Payload {
    ContractDelivery {
        contract_id: ContractId,
        payload_kg: f64,
    },
    TestMass {
        mass_kg: f64,
    },
}

impl Payload {
    pub fn mass_kg(&self) -> f64 {
        match self {
            Payload::ContractDelivery { payload_kg, .. } => *payload_kg,
            Payload::TestMass { mass_kg } => *mass_kg,
        }
    }
}

/// Status of a flight in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlightStatus {
    InTransit,
    Arrived,
    Failed { reason: String },
    Stranded,
}

/// A leg of a flight route through the location graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightLeg {
    pub from: String,
    pub to: String,
    pub delta_v_cost: f64,
    pub burn_days: u32,
    pub coast_days: u32,
    /// Ambient pressure at departure in Pa (>0 for atmospheric launches).
    #[serde(default)]
    pub ambient_pressure_pa: f64,
}

impl FlightLeg {
    pub fn total_days(&self) -> u32 {
        self.burn_days + self.coast_days
    }
}

/// A rocket in flight through the location graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flight {
    pub id: FlightId,
    pub rocket_name: String,
    pub rocket_project_id: RocketProjectId,
    pub design: RocketDesign,
    /// Runtime rocket instance with per-stage propellant tracking.
    pub rocket: Rocket,
    pub payloads: Vec<Payload>,
    pub current_location: String,
    pub route: Vec<FlightLeg>,
    pub current_leg: usize,
    pub leg_days_remaining: u32,
    pub status: FlightStatus,
    pub flaws_activated: Vec<FlawActivation>,
    pub launch_date: GameDate,
    /// Whether to persist as a Spacecraft on arrival.
    #[serde(default)]
    pub persist: bool,
    /// Whether the launch sim determined a partial failure (degraded dv near required).
    #[serde(default)]
    pub launch_partial: bool,
    /// Stage groups that have already had flaws rolled (to avoid rolling per-leg).
    #[serde(default)]
    pub flaw_rolled_groups: std::collections::HashSet<usize>,
}

impl Flight {
    /// Total payload mass across all payloads.
    pub fn total_payload_kg(&self) -> f64 {
        self.payloads.iter().map(|p| p.mass_kg()).sum()
    }

    /// Final destination of this flight.
    pub fn destination(&self) -> &str {
        self.route.last()
            .map(|leg| leg.to.as_str())
            .unwrap_or(&self.current_location)
    }

    /// Total days remaining across all unfinished legs.
    pub fn eta_days(&self) -> u32 {
        let mut total = self.leg_days_remaining;
        for leg in self.route.iter().skip(self.current_leg + 1) {
            total += leg.total_days();
        }
        total
    }
}

/// Build a flight route from a shortest-path result.
/// Returns the list of flight legs with delta-v costs, burn times, and coast times.
pub fn build_route(
    path: &[&'static str],
    rocket_mass_kg: f64,
    total_thrust_n: f64,
    low_thrust: bool,
) -> Vec<FlightLeg> {
    let mut legs = Vec::new();
    for window in path.windows(2) {
        let from = window[0];
        let to = window[1];
        if let Some(transfer) = DELTA_V_MAP.transfer(from, to) {
            let dv_cost = transfer.delta_v_for(low_thrust, rocket_mass_kg)
                .unwrap_or_else(|| transfer.total_delta_v(rocket_mass_kg));
            let coast_days = transfer.transit_days;

            // Burn time: dv / acceleration, where acceleration = thrust / mass
            let burn_days = if total_thrust_n > 0.0 {
                let accel = total_thrust_n / rocket_mass_kg;
                let burn_time_s = dv_cost / accel;
                let days = (burn_time_s / 86400.0).ceil() as u32;
                days
            } else {
                0
            };

            // Look up ambient pressure at the departure location
            let ambient_pressure_pa = if transfer.through_atmosphere {
                DELTA_V_MAP.surface_properties(from)
                    .map_or(0.0, |p| p.ambient_pressure_pa)
            } else {
                0.0
            };

            legs.push(FlightLeg {
                from: from.to_string(),
                to: to.to_string(),
                delta_v_cost: dv_cost,
                burn_days,
                coast_days,
                ambient_pressure_pa,
            });
        }
    }
    legs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_route_leo() {
        // Earth surface -> LEO is a single leg
        let path = vec!["earth_surface", "leo"];
        let legs = build_route(&path, 500_000.0, 7_000_000.0, false);
        assert_eq!(legs.len(), 1);
        assert_eq!(legs[0].from, "earth_surface");
        assert_eq!(legs[0].to, "leo");
        assert!(legs[0].delta_v_cost > 0.0);
        // Chemical rocket burn to LEO is sub-day
        assert_eq!(legs[0].burn_days, 1); // ceil of a few minutes
    }

    #[test]
    fn test_build_route_multi_hop() {
        // Earth surface -> lunar surface goes through multiple nodes
        let path_opt = DELTA_V_MAP.shortest_path("earth_surface", "lunar_surface", 500_000.0);
        assert!(path_opt.is_some());
        let (path, _) = path_opt.unwrap();
        let legs = build_route(&path, 500_000.0, 7_000_000.0, false);
        assert!(legs.len() > 1);
        // Total coast days should be > 0 for a lunar mission
        let total_coast: u32 = legs.iter().map(|l| l.coast_days).sum();
        assert!(total_coast > 0);
    }

    #[test]
    fn test_flight_eta() {
        let design = crate::rocket::RocketDesign {
            id: crate::rocket::RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![],
        };
        let rocket = design.instantiate(
            crate::rocket::RocketId(1), "earth_surface", 100.0,
        );
        let flight = Flight {
            id: FlightId(1),
            rocket_name: "Test".into(),
            rocket_project_id: RocketProjectId(1),
            design,
            rocket,
            payloads: vec![Payload::TestMass { mass_kg: 100.0 }],
            current_location: "earth_surface".into(),
            route: vec![
                FlightLeg {
                    from: "earth_surface".into(), to: "leo".into(),
                    delta_v_cost: 9400.0, burn_days: 1, coast_days: 0,
                    ambient_pressure_pa: 101_325.0,
                },
                FlightLeg {
                    from: "leo".into(), to: "gto".into(),
                    delta_v_cost: 2440.0, burn_days: 0, coast_days: 1,
                    ambient_pressure_pa: 0.0,
                },
            ],
            current_leg: 0,
            leg_days_remaining: 1,
            status: FlightStatus::InTransit,
            flaws_activated: vec![],
            launch_date: crate::calendar::GameDate::new(2001, 1, 1),
            persist: false,
            launch_partial: false,
            flaw_rolled_groups: std::collections::HashSet::new(),
        };
        // On leg 0 with 1 day remaining + leg 1 has 0+1=1 day
        assert_eq!(flight.eta_days(), 2);
        assert_eq!(flight.destination(), "gto");
        assert_eq!(flight.total_payload_kg(), 100.0);
    }
}
