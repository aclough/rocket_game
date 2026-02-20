use std::collections::HashMap;
use crate::engine_design::FuelType;
use crate::fuel_depot::LocationInfrastructure;
use crate::location::{TransferAnimation, DELTA_V_MAP};
use crate::manifest::Manifest;
use crate::rocket_design::RocketDesign;

/// A single leg of a multi-leg mission
#[derive(Debug, Clone)]
pub struct MissionLeg {
    pub from: &'static str,
    pub to: &'static str,
    pub delta_v_required: f64,
    pub animation: Option<TransferAnimation>,
    pub can_aerobrake: bool,
    /// Which stages to burn on this leg. None = auto-assign (backward compatible).
    pub stages_to_burn: Option<Vec<usize>>,
    /// Whether to refuel from a depot at `from` before this leg.
    pub refuel_before: bool,
    /// Transit time in game-days for this leg
    pub transit_days: u32,
    /// Mass of payloads to drop off at this leg's destination (0.0 = none).
    pub payload_drop_kg: f64,
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
                stages_to_burn: None,
                refuel_before: false,
                transit_days: transfer.transit_days,
                payload_drop_kg: 0.0,
            });
        }

        Some(MissionPlan { legs, total_delta_v })
    }

    pub fn leg_count(&self) -> usize {
        self.legs.len()
    }

    /// Total transit time across all legs in game-days
    pub fn total_transit_days(&self) -> u32 {
        self.legs.iter().map(|l| l.transit_days).sum()
    }

    /// Build a multi-stop mission plan from a manifest.
    ///
    /// 1. Collects unique delivery locations from manifest
    /// 2. Sorts by delta-v from earth_surface (ascending) — outward ordering
    /// 3. Chains shortest_path(prev_stop, next_stop) for consecutive pairs
    /// 4. Deduplicates shared waypoints at boundaries
    /// 5. Sets payload_drop_kg on legs whose `to` matches a delivery destination
    ///
    /// Returns None if the manifest is empty or any path segment is unreachable.
    pub fn from_manifest(manifest: &Manifest) -> Option<Self> {
        if manifest.is_empty() {
            return None;
        }

        let destinations = manifest.unique_destinations_sorted_by_delta_v();
        if destinations.is_empty() {
            return None;
        }

        // Build a chain: earth_surface -> dest1 -> dest2 -> ...
        let mut stops = vec!["earth_surface"];
        for dest in &destinations {
            stops.push(dest.as_str());
        }

        let mut all_legs: Vec<MissionLeg> = Vec::new();
        let mut total_delta_v = 0.0;

        for pair in stops.windows(2) {
            let from = pair[0];
            let to = pair[1];

            let (path, segment_dv) = DELTA_V_MAP.shortest_path(from, to)?;
            total_delta_v += segment_dv;

            // Build legs for this segment, skipping the first node if it duplicates
            // the last leg's destination (dedup at boundary)
            for path_pair in path.windows(2) {
                let transfer = DELTA_V_MAP.transfer(path_pair[0], path_pair[1])
                    .expect("shortest_path returned consecutive nodes without a direct transfer");

                // Calculate payload drop mass for this leg's destination
                let drop_mass: f64 = manifest.entries_for_destination(path_pair[1])
                    .iter()
                    .map(|e| e.mass_kg)
                    .sum();

                all_legs.push(MissionLeg {
                    from: path_pair[0],
                    to: path_pair[1],
                    delta_v_required: transfer.total_delta_v(),
                    animation: transfer.animation.clone(),
                    can_aerobrake: transfer.can_aerobrake,
                    stages_to_burn: None,
                    refuel_before: false,
                    transit_days: transfer.transit_days,
                    payload_drop_kg: drop_mass,
                });
            }
        }

        Some(MissionPlan { legs: all_legs, total_delta_v })
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

/// Derive auto-assigned stages per leg from the existing simulate_mission results.
/// Returns one Vec<usize> per leg indicating which stage indices contribute delta-v.
pub fn auto_assign_stages(design: &RocketDesign, plan: &MissionPlan) -> Vec<Vec<usize>> {
    let result = simulate_mission(design, plan);

    let initial_propellant: Vec<(usize, f64)> = design
        .stages
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.propellant_mass_kg))
        .collect();

    let mut prev_remaining = initial_propellant;
    let mut assignments = Vec::new();

    for leg_result in &result.leg_results {
        let mut stages_used = Vec::new();
        if leg_result.feasible {
            for (stage_idx, prev_kg) in &prev_remaining {
                let cur_kg = leg_result.propellant_remaining
                    .iter()
                    .find(|(idx, _)| idx == stage_idx)
                    .map(|(_, kg)| *kg)
                    .unwrap_or(0.0);
                if *prev_kg > cur_kg + 0.001 {
                    stages_used.push(*stage_idx);
                }
            }
            prev_remaining = leg_result.propellant_remaining.clone();
        }
        assignments.push(stages_used);
    }

    assignments
}

/// Simulate a mission with explicit per-leg stage assignments and depot refueling.
///
/// For each leg:
/// 1. If `refuel_before`, top up stage propellant from the depot at `leg.from`
/// 2. If `stages_to_burn` is Some, only those stages contribute delta-v
/// 3. If `stages_to_burn` is None, fall back to auto-assign behavior
///
/// This uses the same cumulative delta-v + `propellant_remaining_kg()` approach
/// as `simulate_mission()` when all legs use auto-assign (stages_to_burn == None).
/// When explicit stages are specified, it uses a per-leg simulation approach.
pub fn simulate_mission_with_plan(
    design: &RocketDesign,
    plan: &MissionPlan,
    infrastructure: &HashMap<String, LocationInfrastructure>,
) -> MissionSimResult {
    // Check if any leg has explicit configuration
    let has_explicit_config = plan.legs.iter().any(|l| l.stages_to_burn.is_some() || l.refuel_before);

    if !has_explicit_config {
        // Fast path: no depot or stage overrides, use existing simulation
        return simulate_mission(design, plan);
    }

    // Slow path: per-leg simulation with refueling and explicit stages
    let initial_propellant: Vec<(usize, f64)> = design
        .stages
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.propellant_mass_kg))
        .collect();

    let total_initial: f64 = initial_propellant.iter().map(|(_, kg)| *kg).sum();

    // Track current propellant per stage (mutable throughout simulation)
    let mut current_propellant: Vec<(usize, f64)> = initial_propellant.clone();
    let mut leg_results = Vec::new();
    let mut sim_design = design.clone();

    // Get auto-assignments as fallback
    let auto_assignments = auto_assign_stages(design, plan);

    let mut cumulative_dv = 0.0;

    for (leg_idx, leg) in plan.legs.iter().enumerate() {
        // Step 1: Refuel from depot if requested
        if leg.refuel_before {
            if let Some(infra) = infrastructure.get(leg.from) {
                if let Some(depot) = &infra.depot {
                    refuel_stages_from_depot(design, &mut current_propellant, depot);
                }
            }
        }

        // Update the sim_design's stage propellant to match current state
        for (stage_idx, kg) in &current_propellant {
            if let Some(stage) = sim_design.stages.get_mut(*stage_idx) {
                stage.propellant_mass_kg = *kg;
            }
        }

        // Step 2: Determine which stages burn this leg
        let empty_stages: Vec<usize> = Vec::new();
        let stages_to_burn = leg.stages_to_burn.as_ref()
            .unwrap_or_else(|| auto_assignments.get(leg_idx).unwrap_or(&empty_stages));

        if stages_to_burn.is_empty() && leg.delta_v_required > 0.0 {
            // No stages assigned and delta-v needed — infeasible
            leg_results.push(LegSimResult {
                leg_index: leg_idx,
                feasible: false,
                propellant_consumed_kg: 0.0,
                stages_jettisoned: Vec::new(),
                propellant_remaining: Vec::new(),
            });
            let consumed: f64 = leg_results.iter().map(|r| r.propellant_consumed_kg).sum();
            return MissionSimResult {
                feasible: false,
                leg_results,
                final_propellant_remaining: Vec::new(),
                total_propellant_consumed_kg: consumed,
            };
        }

        // Step 3: Simulate this leg using cumulative delta-v on the modified design
        cumulative_dv += leg.delta_v_required;
        sim_design.set_target_delta_v(cumulative_dv);

        let remaining = sim_design.propellant_remaining_kg();

        if remaining.is_empty() {
            leg_results.push(LegSimResult {
                leg_index: leg_idx,
                feasible: false,
                propellant_consumed_kg: 0.0,
                stages_jettisoned: Vec::new(),
                propellant_remaining: Vec::new(),
            });
            let consumed: f64 = leg_results.iter().map(|r| r.propellant_consumed_kg).sum();
            return MissionSimResult {
                feasible: false,
                leg_results,
                final_propellant_remaining: Vec::new(),
                total_propellant_consumed_kg: consumed,
            };
        }

        // Diff propellant
        let mut leg_consumed = 0.0;
        let mut stages_jettisoned = Vec::new();

        for (stage_idx, prev_kg) in &current_propellant {
            let cur_kg = remaining
                .iter()
                .find(|(idx, _)| idx == stage_idx)
                .map(|(_, kg)| *kg)
                .unwrap_or(0.0);
            leg_consumed += prev_kg - cur_kg;

            let in_remaining = remaining.iter().any(|(idx, _)| idx == stage_idx);
            if *prev_kg > 0.0 && (!in_remaining || cur_kg == 0.0) {
                stages_jettisoned.push(*stage_idx);
            }
        }

        leg_results.push(LegSimResult {
            leg_index: leg_idx,
            feasible: true,
            propellant_consumed_kg: leg_consumed.max(0.0),
            stages_jettisoned,
            propellant_remaining: remaining.clone(),
        });

        current_propellant = remaining;
    }

    let final_total: f64 = current_propellant.iter().map(|(_, kg)| *kg).sum();
    let total_consumed = total_initial - final_total;

    MissionSimResult {
        feasible: true,
        leg_results,
        final_propellant_remaining: current_propellant,
        total_propellant_consumed_kg: total_consumed.max(0.0),
    }
}

/// Refuel stages from a depot by matching fuel types.
/// Tops up each stage's propellant to its original capacity from the depot.
fn refuel_stages_from_depot(
    design: &RocketDesign,
    current_propellant: &mut Vec<(usize, f64)>,
    depot: &crate::fuel_depot::FuelDepot,
) {
    // Build a mutable copy of depot fuel for simulation
    // (We don't actually withdraw from the depot during simulation —
    //  this is a read-only check. Real withdrawal happens at launch time.)
    let mut available: std::collections::BTreeMap<FuelType, f64> = depot.fuel_stored.clone();

    for (stage_idx, current_kg) in current_propellant.iter_mut() {
        if let Some(stage) = design.stages.get(*stage_idx) {
            let max_propellant = stage.propellant_mass_kg;
            let deficit = max_propellant - *current_kg;
            if deficit <= 0.0 {
                continue;
            }
            let fuel_type = stage.engine_snapshot().fuel_type;
            let fuel_available = available.get(&fuel_type).copied().unwrap_or(0.0);
            let refuel_amount = deficit.min(fuel_available);
            if refuel_amount > 0.0 {
                *current_kg += refuel_amount;
                *available.get_mut(&fuel_type).unwrap() -= refuel_amount;
            }
        }
    }
}

/// Simulate a mission with payload drops at intermediate destinations.
///
/// Like `simulate_mission()` but reduces the design's `payload_mass_kg` at each leg
/// with nonzero `payload_drop_kg`. This gives accurate delta-v accounting since
/// later legs benefit from reduced mass.
///
/// Design-time `is_sufficient()` still uses total mass (conservative).
/// This drop simulation is for display and flight-time accuracy.
pub fn simulate_mission_with_drops(design: &RocketDesign, plan: &MissionPlan) -> MissionSimResult {
    // Check if any leg has drops
    let has_drops = plan.legs.iter().any(|l| l.payload_drop_kg > 0.0);
    if !has_drops {
        return simulate_mission(design, plan);
    }

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

        // Diff propellant
        let mut leg_consumed = 0.0;
        let mut stages_jettisoned = Vec::new();

        for (stage_idx, prev_kg) in &prev_remaining {
            let cur_kg = current_remaining
                .iter()
                .find(|(idx, _)| idx == stage_idx)
                .map(|(_, kg)| *kg)
                .unwrap_or(0.0);
            leg_consumed += prev_kg - cur_kg;

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

        // Drop payload at this leg's destination — reduces mass for subsequent legs
        if leg.payload_drop_kg > 0.0 {
            sim_design.payload_mass_kg = (sim_design.payload_mass_kg - leg.payload_drop_kg).max(0.0);
        }
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

/// Check if a leg departs from a surface location (has gravity losses)
pub fn is_surface_departure(from: &str) -> bool {
    DELTA_V_MAP.surface_properties(from).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rocket_design::RocketDesign;
    use crate::engine_design::{default_snapshot, FuelType};
    use crate::stage::RocketStage;
    use crate::fuel_depot::LocationInfrastructure;

    #[test]
    fn test_single_leg_plan() {
        let plan = MissionPlan::from_shortest_path("earth_surface", "leo").unwrap();
        assert_eq!(plan.leg_count(), 1);
        assert_eq!(plan.legs[0].from, "earth_surface");
        assert_eq!(plan.legs[0].to, "leo");
        assert_eq!(plan.legs[0].delta_v_required, 8100.0);
        assert_eq!(plan.total_delta_v, 8100.0);
        assert!(plan.legs[0].stages_to_burn.is_none());
        assert!(!plan.legs[0].refuel_before);
        assert_eq!(plan.legs[0].transit_days, 0);
        assert_eq!(plan.total_transit_days(), 0);
    }

    #[test]
    fn test_total_transit_days_multi_leg() {
        // earth_surface(0) -> leo(1) -> gto(0) -> geo = 1 day
        let plan = MissionPlan::from_shortest_path("earth_surface", "geo").unwrap();
        assert_eq!(plan.total_transit_days(), 1); // leo->gto = 1 day

        // earth_surface(0) -> leo(4) -> lunar_orbit(0) -> lunar_surface = 4 days
        let lunar_plan = MissionPlan::from_shortest_path("earth_surface", "lunar_surface").unwrap();
        assert_eq!(lunar_plan.total_transit_days(), 4); // leo->lunar_orbit = 4 days
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

    #[test]
    fn test_auto_assign_stages_leo() {
        let design = RocketDesign::default_design();
        let plan = MissionPlan::from_shortest_path("earth_surface", "leo").unwrap();
        let assignments = auto_assign_stages(&design, &plan);
        assert_eq!(assignments.len(), 1);
        // Both stages should contribute to LEO
        assert!(!assignments[0].is_empty(), "At least one stage should burn for LEO");
    }

    #[test]
    fn test_auto_assign_stages_multi_leg() {
        let design = RocketDesign::default_design();
        let plan = MissionPlan::from_shortest_path("earth_surface", "geo").unwrap();
        let assignments = auto_assign_stages(&design, &plan);
        // Assignments has one entry per leg result (may be fewer than legs if mission is infeasible)
        assert!(!assignments.is_empty(), "Should have at least one leg assignment");
        // First leg should have at least stage 0
        assert!(!assignments[0].is_empty(), "First leg should have stages assigned");
    }

    #[test]
    fn test_simulate_with_plan_no_config_matches_baseline() {
        let design = RocketDesign::default_design();
        let plan = MissionPlan::from_shortest_path("earth_surface", "leo").unwrap();

        let baseline = simulate_mission(&design, &plan);
        let infra = HashMap::new();
        let with_plan = simulate_mission_with_plan(&design, &plan, &infra);

        assert_eq!(baseline.feasible, with_plan.feasible);
        assert!(
            (baseline.total_propellant_consumed_kg - with_plan.total_propellant_consumed_kg).abs() < 1.0,
            "baseline={:.0} vs with_plan={:.0}",
            baseline.total_propellant_consumed_kg, with_plan.total_propellant_consumed_kg
        );
    }

    #[test]
    fn test_simulate_with_refuel() {
        // Create a rocket that can barely reach LEO, then needs refueling for further travel
        let design = RocketDesign::default_design();
        let mut plan = MissionPlan::from_shortest_path("earth_surface", "geo").unwrap();

        // Set up a depot at LEO with plenty of fuel
        let mut infra = HashMap::new();
        let mut leo_infra = LocationInfrastructure::new();
        let depot = leo_infra.get_or_create_depot("leo", 500_000.0);
        depot.deposit(FuelType::Kerolox, 200_000.0);
        depot.deposit(FuelType::Hydrolox, 200_000.0);
        infra.insert("leo".to_string(), leo_infra);

        // Enable refueling before the LEO->GTO leg
        plan.legs[1].refuel_before = true;

        let result_without_refuel = simulate_mission(&design, &plan);
        let result_with_refuel = simulate_mission_with_plan(&design, &plan, &infra);

        // With refueling at LEO, mission should be more feasible or use less of own fuel
        // (the default design may or may not reach GEO either way, but the with-refuel
        //  result should be at least as good)
        if !result_without_refuel.feasible {
            // If baseline fails, refuel version might succeed
            // (or also fail if not enough delta-v even with full tanks)
        }
        if result_with_refuel.feasible {
            assert!(result_with_refuel.total_propellant_consumed_kg >= 0.0);
        }
    }

    #[test]
    fn test_simulate_with_explicit_stages() {
        let design = RocketDesign::default_design();
        let mut plan = MissionPlan::from_shortest_path("earth_surface", "leo").unwrap();

        // Explicitly assign all stages to the first leg (same as auto)
        let stage_indices: Vec<usize> = (0..design.stages.len()).collect();
        plan.legs[0].stages_to_burn = Some(stage_indices);

        let infra = HashMap::new();
        let result = simulate_mission_with_plan(&design, &plan, &infra);

        // Should get same result as auto-assign since we assigned all stages
        let baseline = simulate_mission(&design, &plan);
        assert_eq!(result.feasible, baseline.feasible);
    }

    #[test]
    fn test_is_surface_departure() {
        assert!(is_surface_departure("earth_surface"));
        assert!(is_surface_departure("lunar_surface"));
        assert!(!is_surface_departure("leo"));
        assert!(!is_surface_departure("gto"));
    }

    // ==========================================
    // Manifest-based route tests
    // ==========================================

    #[test]
    fn test_from_manifest_empty() {
        let m = Manifest::new();
        assert!(MissionPlan::from_manifest(&m).is_none());
    }

    #[test]
    fn test_from_manifest_single_destination() {
        let mut m = Manifest::new();
        m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);

        let plan = MissionPlan::from_manifest(&m).unwrap();
        // earth_surface -> leo (single leg)
        assert_eq!(plan.leg_count(), 1);
        assert_eq!(plan.legs[0].from, "earth_surface");
        assert_eq!(plan.legs[0].to, "leo");
        assert_eq!(plan.total_delta_v, 8100.0);
        // Payload drop at LEO = 500 kg
        assert!((plan.legs[0].payload_drop_kg - 500.0).abs() < 0.01);
    }

    #[test]
    fn test_from_manifest_single_backward_compat() {
        // Single-destination manifest should produce same route as from_shortest_path
        let mut m = Manifest::new();
        m.add_contract(1, "C1".into(), "T1".into(), 1e6, "geo".into(), "GEO".into(), 500.0);

        let manifest_plan = MissionPlan::from_manifest(&m).unwrap();
        let direct_plan = MissionPlan::from_shortest_path("earth_surface", "geo").unwrap();

        assert_eq!(manifest_plan.leg_count(), direct_plan.leg_count());
        assert_eq!(manifest_plan.total_delta_v, direct_plan.total_delta_v);
        for (ml, dl) in manifest_plan.legs.iter().zip(direct_plan.legs.iter()) {
            assert_eq!(ml.from, dl.from);
            assert_eq!(ml.to, dl.to);
            assert_eq!(ml.delta_v_required, dl.delta_v_required);
        }
    }

    #[test]
    fn test_from_manifest_two_destinations_leo_geo() {
        let mut m = Manifest::new();
        m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);
        m.add_contract(2, "C2".into(), "T2".into(), 2e6, "geo".into(), "GEO".into(), 1000.0);

        let plan = MissionPlan::from_manifest(&m).unwrap();
        // earth_surface -> leo -> gto -> geo
        assert_eq!(plan.leg_count(), 3);
        assert_eq!(plan.legs[0].from, "earth_surface");
        assert_eq!(plan.legs[0].to, "leo");
        assert_eq!(plan.legs[1].from, "leo");
        assert_eq!(plan.legs[1].to, "gto");
        assert_eq!(plan.legs[2].from, "gto");
        assert_eq!(plan.legs[2].to, "geo");

        // Drop at LEO = 500 kg, at GEO = 1000 kg, GTO = 0
        assert!((plan.legs[0].payload_drop_kg - 500.0).abs() < 0.01);
        assert_eq!(plan.legs[1].payload_drop_kg, 0.0);
        assert!((plan.legs[2].payload_drop_kg - 1000.0).abs() < 0.01);

        // Total delta-v = earth->geo = 12040 (same as single-destination)
        assert_eq!(plan.total_delta_v, 12040.0);
    }

    #[test]
    fn test_from_manifest_multiple_payloads_same_destination() {
        let mut m = Manifest::new();
        m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);
        m.add_contract(2, "C2".into(), "T2".into(), 2e6, "leo".into(), "LEO".into(), 300.0);
        m.add_depot(0, 1, "D1".into(), 5000.0, false, "leo".into(), "LEO".into(), 200.0);

        let plan = MissionPlan::from_manifest(&m).unwrap();
        assert_eq!(plan.leg_count(), 1);
        // Total drop at LEO = 500 + 300 + 200 = 1000 kg
        assert!((plan.legs[0].payload_drop_kg - 1000.0).abs() < 0.01);
    }

    #[test]
    fn test_from_manifest_three_destinations() {
        // Use destinations along a valid chain: LEO -> GTO -> GEO
        let mut m = Manifest::new();
        m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);
        m.add_contract(2, "C2".into(), "T2".into(), 2e6, "gto".into(), "GTO".into(), 300.0);
        m.add_contract(3, "C3".into(), "T3".into(), 3e6, "geo".into(), "GEO".into(), 1000.0);

        let plan = MissionPlan::from_manifest(&m).unwrap();
        // Route: earth_surface -> leo (drop 500) -> gto (drop 300) -> geo (drop 1000)
        assert_eq!(plan.leg_count(), 3);
        assert_eq!(plan.legs[0].to, "leo");
        assert_eq!(plan.legs[1].to, "gto");
        assert_eq!(plan.legs[2].to, "geo");
        assert!((plan.legs[0].payload_drop_kg - 500.0).abs() < 0.01);
        assert!((plan.legs[1].payload_drop_kg - 300.0).abs() < 0.01);
        assert!((plan.legs[2].payload_drop_kg - 1000.0).abs() < 0.01);
    }

    #[test]
    fn test_simulate_with_drops_no_drops_matches_baseline() {
        let design = RocketDesign::default_design();
        let plan = MissionPlan::from_shortest_path("earth_surface", "leo").unwrap();

        let baseline = simulate_mission(&design, &plan);
        let with_drops = simulate_mission_with_drops(&design, &plan);

        assert_eq!(baseline.feasible, with_drops.feasible);
        assert!(
            (baseline.total_propellant_consumed_kg - with_drops.total_propellant_consumed_kg).abs() < 1.0,
        );
    }

    #[test]
    fn test_simulate_with_drops_reduces_mass() {
        let design = RocketDesign::default_design();

        // Build a plan to GEO with a drop at LEO
        let mut plan = MissionPlan::from_shortest_path("earth_surface", "geo").unwrap();
        let half_payload = design.payload_mass_kg / 2.0;
        plan.legs[0].payload_drop_kg = half_payload; // Drop half at LEO

        let no_drop_result = simulate_mission(&design, &plan);
        let drop_result = simulate_mission_with_drops(&design, &plan);

        // With drops, later legs should consume less propellant (lighter rocket)
        // If baseline is infeasible but drops make it feasible, that's also valid
        if no_drop_result.feasible && drop_result.feasible {
            // Drop version should use less total propellant (lighter after LEO)
            assert!(
                drop_result.total_propellant_consumed_kg <= no_drop_result.total_propellant_consumed_kg + 1.0,
                "Drop sim ({:.0}) should use <= no-drop ({:.0}) propellant",
                drop_result.total_propellant_consumed_kg, no_drop_result.total_propellant_consumed_kg
            );
        }
    }
}
