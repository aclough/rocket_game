use crate::location::{TransferAnimation, DELTA_V_MAP};
use crate::rocket_design::RocketDesign;

/// A single leg of a multi-leg mission
#[derive(Debug, Clone)]
pub struct MissionLeg {
    pub from: &'static str,
    pub to: &'static str,
    pub delta_v_required: f64,
    pub animation: Option<TransferAnimation>,
    pub can_aerobrake: bool,
}

/// A complete mission plan decomposed into sequential transfer legs
#[derive(Debug, Clone)]
pub struct MissionPlan {
    pub legs: Vec<MissionLeg>,
    pub total_delta_v: f64,
}

impl MissionPlan {
    /// Build a mission plan from the shortest path between two locations.
    /// Returns None if no path exists.
    pub fn from_shortest_path(from: &str, to: &str) -> Option<Self> {
        let (path, total_delta_v) = DELTA_V_MAP.shortest_path(from, to)?;

        let mut legs = Vec::new();
        for pair in path.windows(2) {
            let transfer = DELTA_V_MAP.transfer(pair[0], pair[1])
                .expect("shortest_path returned consecutive nodes without a direct transfer");
            legs.push(MissionLeg {
                from: pair[0],
                to: pair[1],
                delta_v_required: transfer.total_delta_v(),
                animation: transfer.animation.clone(),
                can_aerobrake: transfer.can_aerobrake,
            });
        }

        Some(MissionPlan { legs, total_delta_v })
    }

    pub fn leg_count(&self) -> usize {
        self.legs.len()
    }
}

/// Result of simulating a single mission leg
#[derive(Debug, Clone)]
pub struct LegSimResult {
    pub leg_index: usize,
    pub feasible: bool,
    pub propellant_consumed_kg: f64,
    pub stages_jettisoned: Vec<usize>,
    pub propellant_remaining: Vec<(usize, f64)>,
}

/// Result of simulating an entire mission
#[derive(Debug, Clone)]
pub struct MissionSimResult {
    pub feasible: bool,
    pub leg_results: Vec<LegSimResult>,
    pub final_propellant_remaining: Vec<(usize, f64)>,
    pub total_propellant_consumed_kg: f64,
}

/// Simulate whether a rocket design can complete all legs of a mission plan.
///
/// Reuses `propellant_remaining_kg()` with cumulative delta-v targets.
/// For each leg, sets the design's target_delta_v to the cumulative sum
/// through that leg and diffs propellant remaining with the previous leg.
pub fn simulate_mission(design: &RocketDesign, plan: &MissionPlan) -> MissionSimResult {
    let initial_propellant: Vec<(usize, f64)> = design
        .stages
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.propellant_mass_kg))
        .collect();

    let total_initial: f64 = initial_propellant.iter().map(|(_, kg)| *kg).sum();

    let mut sim_design = design.clone();
    let mut cumulative_dv = 0.0;
    let mut leg_results = Vec::new();
    let mut prev_remaining = initial_propellant.clone();

    for (leg_idx, leg) in plan.legs.iter().enumerate() {
        cumulative_dv += leg.delta_v_required;
        sim_design.set_target_delta_v(cumulative_dv);

        let current_remaining = sim_design.propellant_remaining_kg();

        if current_remaining.is_empty() {
            // Infeasible at this leg
            leg_results.push(LegSimResult {
                leg_index: leg_idx,
                feasible: false,
                propellant_consumed_kg: 0.0,
                stages_jettisoned: Vec::new(),
                propellant_remaining: Vec::new(),
            });

            let consumed_so_far: f64 = leg_results.iter().map(|r| r.propellant_consumed_kg).sum();
            return MissionSimResult {
                feasible: false,
                leg_results,
                final_propellant_remaining: Vec::new(),
                total_propellant_consumed_kg: consumed_so_far,
            };
        }

        // Diff propellant: consumed this leg = prev - current for each stage
        let mut leg_consumed = 0.0;
        let mut stages_jettisoned = Vec::new();

        for (stage_idx, prev_kg) in &prev_remaining {
            let cur_kg = current_remaining
                .iter()
                .find(|(idx, _)| idx == stage_idx)
                .map(|(_, kg)| *kg)
                .unwrap_or(0.0);
            leg_consumed += prev_kg - cur_kg;

            // Stage jettisoned if it was in prev with propellant but now at 0 or missing
            let in_current = current_remaining.iter().any(|(idx, _)| idx == stage_idx);
            if *prev_kg > 0.0 && (!in_current || cur_kg == 0.0) {
                stages_jettisoned.push(*stage_idx);
            }
        }

        leg_results.push(LegSimResult {
            leg_index: leg_idx,
            feasible: true,
            propellant_consumed_kg: leg_consumed.max(0.0),
            stages_jettisoned,
            propellant_remaining: current_remaining.clone(),
        });

        prev_remaining = current_remaining;
    }

    let final_total: f64 = prev_remaining.iter().map(|(_, kg)| *kg).sum();
    let total_consumed = total_initial - final_total;

    MissionSimResult {
        feasible: true,
        leg_results,
        final_propellant_remaining: prev_remaining,
        total_propellant_consumed_kg: total_consumed.max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rocket_design::RocketDesign;
    use crate::engine_design::default_snapshot;
    use crate::stage::RocketStage;

    #[test]
    fn test_single_leg_plan() {
        let plan = MissionPlan::from_shortest_path("earth_surface", "leo").unwrap();
        assert_eq!(plan.leg_count(), 1);
        assert_eq!(plan.legs[0].from, "earth_surface");
        assert_eq!(plan.legs[0].to, "leo");
        assert_eq!(plan.legs[0].delta_v_required, 8100.0);
        assert_eq!(plan.total_delta_v, 8100.0);
    }

    #[test]
    fn test_multi_hop_plan() {
        let plan = MissionPlan::from_shortest_path("earth_surface", "geo").unwrap();
        // earth_surface -> leo -> gto -> geo
        assert_eq!(plan.leg_count(), 3);
        assert_eq!(plan.legs[0].from, "earth_surface");
        assert_eq!(plan.legs[0].to, "leo");
        assert_eq!(plan.legs[1].from, "leo");
        assert_eq!(plan.legs[1].to, "gto");
        assert_eq!(plan.legs[2].from, "gto");
        assert_eq!(plan.legs[2].to, "geo");
        assert_eq!(plan.total_delta_v, 12040.0);
    }

    #[test]
    fn test_lunar_plan() {
        let plan = MissionPlan::from_shortest_path("earth_surface", "lunar_surface").unwrap();
        // earth_surface -> leo -> lunar_orbit -> lunar_surface
        assert_eq!(plan.leg_count(), 3);
        assert_eq!(plan.total_delta_v, 13650.0);
    }

    #[test]
    fn test_no_path_returns_none() {
        // No path from LEO back to earth_surface
        assert!(MissionPlan::from_shortest_path("leo", "earth_surface").is_none());
    }

    #[test]
    fn test_simulate_sufficient() {
        let design = RocketDesign::default_design();
        let plan = MissionPlan::from_shortest_path("earth_surface", "leo").unwrap();
        let result = simulate_mission(&design, &plan);
        assert!(result.feasible, "Default design should reach LEO");
        assert_eq!(result.leg_results.len(), 1);
        assert!(result.leg_results[0].feasible);
    }

    #[test]
    fn test_simulate_insufficient() {
        // Tiny rocket that can't reach LEO
        let mut design = RocketDesign::new();
        let mut stage = RocketStage::new(default_snapshot(1)); // Kerolox
        stage.propellant_mass_kg = 100.0; // Way too little
        design.stages.push(stage);

        let plan = MissionPlan::from_shortest_path("earth_surface", "leo").unwrap();
        let result = simulate_mission(&design, &plan);
        assert!(!result.feasible, "Tiny rocket should not reach LEO");
    }

    #[test]
    fn test_propellant_consumption_sums() {
        let design = RocketDesign::default_design();
        let plan = MissionPlan::from_shortest_path("earth_surface", "leo").unwrap();
        let result = simulate_mission(&design, &plan);

        assert!(result.feasible);
        let per_leg_sum: f64 = result.leg_results.iter().map(|r| r.propellant_consumed_kg).sum();
        assert!(
            (per_leg_sum - result.total_propellant_consumed_kg).abs() < 1.0,
            "Per-leg sum ({:.0}) should equal total ({:.0})",
            per_leg_sum, result.total_propellant_consumed_kg
        );
    }

    #[test]
    fn test_jettisoned_stages_tracked() {
        // Default design has 2 stages; first should be jettisoned for LEO
        let design = RocketDesign::default_design();
        let plan = MissionPlan::from_shortest_path("earth_surface", "leo").unwrap();
        let result = simulate_mission(&design, &plan);

        assert!(result.feasible);
        // First stage (index 0) should be jettisoned after the LEO leg
        let all_jettisoned: Vec<usize> = result.leg_results.iter()
            .flat_map(|r| r.stages_jettisoned.iter().copied())
            .collect();
        assert!(
            all_jettisoned.contains(&0),
            "First stage should be jettisoned, got: {:?}", all_jettisoned
        );
    }
}
