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

/// What a flight (or parked spacecraft) is carrying.
///
/// `Spacecraft` is a nested rocket — Apollo CSM carrying the LEM, Saturn V
/// lifting Skylab, Falcon 9 with a Dragon. On arrival at `deploy_at`, it's
/// dropped off as an independent `Spacecraft` in the player's fleet,
/// keeping any of its own `nested_payloads` with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Payload {
    ContractDelivery {
        contract_id: ContractId,
        payload_kg: f64,
    },
    TestMass {
        mass_kg: f64,
    },
    Spacecraft {
        /// Where this payload is dropped off. Phase 1: must equal the
        /// flight's final destination. Phase 2 will allow mid-route
        /// waypoints.
        deploy_at: String,
        design: RocketDesign,
        rocket: Rocket,
        /// What this nested rocket itself carries (CSM-carries-LEM).
        #[serde(default)]
        nested_payloads: Vec<Payload>,
        /// Lineage for cost / launch-history reporting.
        rocket_project_id: RocketProjectId,
        /// Display name for the deployed spacecraft. Inherited from the
        /// inventory rocket at launch time; future work may let the player
        /// customise it per-launch.
        name: String,
    },
}

impl Payload {
    /// Mass this payload contributes to its carrier. Recursive for
    /// `Spacecraft` payloads — sums the nested rocket's current attached-
    /// stage wet mass plus its own nested payloads.
    pub fn mass_kg(&self) -> f64 {
        match self {
            Payload::ContractDelivery { payload_kg, .. } => *payload_kg,
            Payload::TestMass { mass_kg } => *mass_kg,
            Payload::Spacecraft { design, rocket, nested_payloads, .. } => {
                let mut spacecraft_mass = 0.0;
                for (gi, group) in design.stage_groups.iter().enumerate() {
                    for (si, stage) in group.iter().enumerate() {
                        if let Some(state) = rocket.stage_states.get(gi).and_then(|g| g.get(si)) {
                            if state.attached {
                                spacecraft_mass += stage.dry_mass_kg() + state.propellant_remaining_kg;
                            }
                        }
                    }
                }
                spacecraft_mass + nested_payloads.iter().map(|p| p.mass_kg()).sum::<f64>()
            }
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

/// Sub-phase of the current leg, used for status display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightPhase {
    /// Engines firing (first portion of leg, length = leg.burn_days).
    Burning,
    /// Coasting on a ballistic transfer (after the burn portion).
    Coasting,
    /// Final-approach burn into the very last leg of a multi-leg route.
    Arriving,
}

impl FlightPhase {
    pub fn word(self) -> &'static str {
        match self {
            FlightPhase::Burning => "Burning",
            FlightPhase::Coasting => "Coasting",
            FlightPhase::Arriving => "Arriving",
        }
    }
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

    /// What sub-phase the flight is currently in.
    /// Returns None if the flight has completed all legs.
    pub fn current_phase(&self) -> Option<FlightPhase> {
        let leg = self.route.get(self.current_leg)?;
        let elapsed = leg.total_days().saturating_sub(self.leg_days_remaining);
        let in_burn = elapsed < leg.burn_days;
        let is_final_leg = self.current_leg + 1 == self.route.len();
        let is_first_leg = self.current_leg == 0;

        Some(if in_burn {
            // "Arriving" only on the final approach burn after at least one prior leg —
            // a single-leg ascent reads more naturally as "Burning".
            if is_final_leg && !is_first_leg { FlightPhase::Arriving } else { FlightPhase::Burning }
        } else {
            FlightPhase::Coasting
        })
    }

    /// For each remaining leg (starting at `current_leg`), simulate the burn and
    /// return the per-stage-group delta-v contribution.
    ///
    /// Each entry is `Vec<(group_index, dv_provided_m_s)>` for the corresponding leg.
    /// Groups that contributed less than 1 m/s are filtered out.
    pub fn dv_plan(&self) -> Vec<Vec<(usize, f64)>> {
        let mut result = Vec::new();
        let mut sim_rocket = self.rocket.clone();
        let sim_design = self.design.clone();
        let n_groups = sim_design.stage_groups.len();

        for leg_idx in self.current_leg..self.route.len() {
            let leg = &self.route[leg_idx];

            let before: Vec<f64> = (0..n_groups)
                .map(|gi| sim_rocket.group_remaining_delta_v(&sim_design, gi))
                .collect();

            sim_rocket.burn_sequential(&sim_design, leg.delta_v_cost, leg.ambient_pressure_pa);

            let after: Vec<f64> = (0..n_groups)
                .map(|gi| sim_rocket.group_remaining_delta_v(&sim_design, gi))
                .collect();

            let mut contributions: Vec<(usize, f64)> = Vec::new();
            for gi in 0..n_groups {
                // Skip groups with infinite dv (solar sail) — diff is undefined.
                if before[gi].is_infinite() {
                    continue;
                }
                let diff = before[gi] - after[gi];
                if diff > 1.0 {
                    contributions.push((gi, diff));
                }
            }
            result.push(contributions);
        }
        result
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

    /// Build a tiny single-stage RocketDesign + matching Rocket so we can
    /// assemble Payload::Spacecraft test instances without dragging in the
    /// full engine/stage helpers.
    fn tiny_spacecraft(id: u64, prop: f64, dry: f64) -> (RocketDesign, Rocket) {
        use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
        use crate::propellant::Propellant;
        use crate::rocket::{RocketDesign, RocketDesignId, RocketId};
        use crate::stage::{Stage, StageId};
        let engine = EngineDesign {
            id: EngineId(id), name: "TestEng".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 100_000.0, mass_kg: 100.0, isp_s: 300.0,
            exit_pressure_pa: 70_000.0, needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.7 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.3 },
            ],
        };
        let stage = Stage {
            id: StageId(id), name: format!("S{}", id),
            engine, engine_count: 1,
            propellant_mass_kg: prop, structural_mass_kg: dry,
            fairing: None,
        };
        let design = RocketDesign {
            id: RocketDesignId(id), name: format!("Tiny{}", id),
            stage_groups: vec![vec![stage]],
        };
        // Payload mass on the inner rocket = 0 here; tests using nested
        // payloads sum manually.
        let rocket = design.instantiate(RocketId(id), "earth_surface", 0.0);
        (design, rocket)
    }

    #[test]
    fn test_payload_mass_recursive() {
        // Outer Spacecraft (LEM-class): wet mass ~700kg, no nested payloads.
        let (lem_design, lem_rocket) = tiny_spacecraft(1, 500.0, 100.0);
        // engine = 100 kg, dry = 100 + 100 = 200; wet = 700.
        let lem_payload = Payload::Spacecraft {
            deploy_at: "lunar_surface".into(),
            design: lem_design.clone(),
            rocket: lem_rocket,
            nested_payloads: vec![],
            rocket_project_id: RocketProjectId(1),
            name: "LEM".into(),
        };
        assert!((lem_payload.mass_kg() - 700.0).abs() < 0.01);

        // Outer Spacecraft (CSM-class) carrying the LEM as a nested payload.
        let (csm_design, csm_rocket) = tiny_spacecraft(2, 1000.0, 200.0);
        // engine = 100, dry = 100 + 200 = 300; wet = 1300.
        let csm_payload = Payload::Spacecraft {
            deploy_at: "lunar_orbit".into(),
            design: csm_design.clone(),
            rocket: csm_rocket,
            nested_payloads: vec![lem_payload],
            rocket_project_id: RocketProjectId(2),
            name: "CSM".into(),
        };
        // CSM (1300) + LEM (700) = 2000.
        assert!((csm_payload.mass_kg() - 2000.0).abs() < 0.01);
    }

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

    /// Build a 2-leg flight (Earth Surface -> LEO -> GTO) using a real
    /// 2-stage rocket design so the dv-plan dry-run has something to bite into.
    fn make_two_leg_flight() -> Flight {
        use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
        use crate::propellant::Propellant;
        use crate::rocket::{RocketDesign, RocketDesignId, RocketId};
        use crate::stage::{Stage, StageId};

        let booster_engine = EngineDesign {
            id: EngineId(1), name: "Booster".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 7_000_000.0, mass_kg: 1_500.0, isp_s: 280.0,
            exit_pressure_pa: 70_000.0, needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
        };
        let upper_engine = EngineDesign {
            id: EngineId(2), name: "Upper".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 1_000_000.0, mass_kg: 800.0, isp_s: 340.0,
            exit_pressure_pa: 10_000.0, needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
        };
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: booster_engine, engine_count: 1,
            propellant_mass_kg: 350_000.0, structural_mass_kg: 25_000.0,
            fairing: None,
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: upper_engine, engine_count: 1,
            propellant_mass_kg: 90_000.0, structural_mass_kg: 5_000.0,
            fairing: None,
        };
        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "TwoStage".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };
        let rocket = design.instantiate(RocketId(1), "earth_surface", 5_000.0);

        Flight {
            id: FlightId(1),
            rocket_name: "TwoStage".into(),
            rocket_project_id: RocketProjectId(1),
            design,
            rocket,
            payloads: vec![Payload::TestMass { mass_kg: 5_000.0 }],
            current_location: "earth_surface".into(),
            route: vec![
                FlightLeg {
                    from: "earth_surface".into(), to: "leo".into(),
                    delta_v_cost: 9_400.0, burn_days: 1, coast_days: 0,
                    ambient_pressure_pa: 101_325.0,
                },
                FlightLeg {
                    from: "leo".into(), to: "gto".into(),
                    delta_v_cost: 2_440.0, burn_days: 1, coast_days: 2,
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
        }
    }

    #[test]
    fn test_phase_burning_during_burn_portion() {
        let mut flight = make_two_leg_flight();
        // Leg 0: 1 burn day, 0 coast. leg_days_remaining=1 (full), elapsed=0 → burn phase.
        // It's also leg 0 (first leg), so single-leg-style "Burning" wins over "Arriving".
        flight.current_leg = 0;
        flight.leg_days_remaining = 1;
        assert_eq!(flight.current_phase(), Some(FlightPhase::Burning));
    }

    #[test]
    fn test_phase_coasting_after_burn() {
        let mut flight = make_two_leg_flight();
        // Leg 1: 1 burn day, 2 coast days. leg_days_remaining=2 → elapsed=1 → in coast.
        flight.current_leg = 1;
        flight.leg_days_remaining = 2;
        assert_eq!(flight.current_phase(), Some(FlightPhase::Coasting));
    }

    #[test]
    fn test_phase_arriving_on_final_leg_burn() {
        let mut flight = make_two_leg_flight();
        // Final leg, burn portion (elapsed=0 < burn_days=1), and not the first leg.
        flight.current_leg = 1;
        flight.leg_days_remaining = 3; // total=3, elapsed=0 → burn phase
        assert_eq!(flight.current_phase(), Some(FlightPhase::Arriving));
    }

    #[test]
    fn test_phase_single_leg_flight_burns_not_arrives() {
        // A flight with only one leg should read as "Burning" during its burn,
        // not "Arriving" — single-leg ascents are launches, not arrivals.
        let mut flight = make_two_leg_flight();
        flight.route.truncate(1);
        flight.current_leg = 0;
        flight.leg_days_remaining = 1;
        assert_eq!(flight.current_phase(), Some(FlightPhase::Burning));
    }

    #[test]
    fn test_dv_plan_two_leg_burns_each_stage() {
        let flight = make_two_leg_flight();
        let plan = flight.dv_plan();
        assert_eq!(plan.len(), 2, "one entry per remaining leg");

        // Leg 0 (Earth Surface -> LEO, 9.4 km/s) should drain stage 0 (booster).
        // It might also dip into stage 1 if booster can't provide all of it.
        let leg0_groups: Vec<usize> = plan[0].iter().map(|(gi, _)| *gi).collect();
        assert!(leg0_groups.contains(&0), "booster should contribute on leg 0");

        // Leg 1 (LEO -> GTO, 2.44 km/s) should be served by stage 1 (upper).
        // Stage 0 should NOT contribute (it's been jettisoned by then).
        let leg1_groups: Vec<usize> = plan[1].iter().map(|(gi, _)| *gi).collect();
        assert!(leg1_groups.contains(&1), "upper stage should contribute on leg 1");
        assert!(!leg1_groups.contains(&0), "booster should already be gone by leg 1");
    }

    #[test]
    fn test_dv_plan_starts_at_current_leg() {
        // After leg 0 has been completed, dv_plan should only return entries for
        // leg 1 onward — it walks remaining legs only.
        let mut flight = make_two_leg_flight();
        flight.current_leg = 1;
        flight.leg_days_remaining = 3;
        let plan = flight.dv_plan();
        assert_eq!(plan.len(), 1);
    }
}
