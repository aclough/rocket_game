use serde::{Serialize, Deserialize};

use crate::location::{self, DELTA_V_MAP};
use crate::stage::Stage;

/// Unique identifier for a rocket design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RocketDesignId(pub u64);

/// Unique identifier for a rocket instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RocketId(pub u64);

/// A rocket design blueprint.
///
/// `stage_groups` is a Vec of sequential groups. Each group is a Vec of stages
/// that are physically present simultaneously:
/// - Outer index: sequential firing order (group 0 fires first)
/// - Inner index: parallel stages within a group
///
/// Example: `[[core, srb1, srb2], [upper]]` — core+SRBs fire together, then upper stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocketDesign {
    pub id: RocketDesignId,
    pub name: String,
    pub stage_groups: Vec<Vec<Stage>>,
}

/// Runtime state for a single stage within a rocket instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageState {
    pub propellant_remaining_kg: f64,
    pub attached: bool,
}

/// Result of a sequential burn operation.
#[derive(Debug, Clone)]
pub struct BurnResult {
    pub dv_achieved: f64,
    /// Groups that consumed any propellant during this burn (includes jettisoned).
    pub groups_burned: Vec<usize>,
    /// Groups that were fully exhausted and jettisoned.
    pub groups_jettisoned: Vec<usize>,
}

/// A physical rocket instance with runtime state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rocket {
    pub id: RocketId,
    pub design_id: RocketDesignId,
    pub location: String,
    pub payload_mass_kg: f64,
    pub stage_states: Vec<Vec<StageState>>,
}

impl RocketDesign {
    /// Total wet mass of the entire vehicle (excluding payload).
    pub fn total_mass_kg(&self) -> f64 {
        self.stage_groups.iter()
            .flat_map(|group| group.iter())
            .map(|stage| stage.wet_mass_kg())
            .sum()
    }

    /// Combined thrust of all stages in a group (Newtons).
    pub fn group_thrust_n(&self, group_index: usize) -> f64 {
        self.stage_groups.get(group_index)
            .map(|group| group.iter().map(|s| s.total_thrust_n()).sum())
            .unwrap_or(0.0)
    }

    /// Validate the design. Returns a list of problems (empty = valid).
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.stage_groups.is_empty() {
            errors.push("Rocket must have at least one stage group".into());
        }
        for (gi, group) in self.stage_groups.iter().enumerate() {
            if group.is_empty() {
                errors.push(format!("Stage group {} is empty", gi));
            }
        }
        errors
    }

    /// Delta-v for a single stage group, accounting for phased parallel burnout.
    ///
    /// When multiple stages fire in parallel, they may have different burn times.
    /// We simulate in phases: all stages fire until the shortest-duration stage
    /// exhausts its propellant, that stage is jettisoned (reducing mass), and the
    /// remaining stages continue. This repeats until all stages in the group are
    /// exhausted.
    ///
    /// `payload_above_kg` is the mass of everything above this group (upper stages
    /// + payload).
    pub fn group_delta_v(&self, group_index: usize, payload_above_kg: f64) -> f64 {
        let group = match self.stage_groups.get(group_index) {
            Some(g) => g,
            None => return 0.0,
        };

        if group.len() == 1 {
            return group[0].delta_v(payload_above_kg);
        }

        // Phased simulation for parallel stages
        phased_parallel_delta_v(group, payload_above_kg)
    }

    /// Total delta-v across all stage groups for a given payload.
    /// Each group's "payload" is everything above it: upper groups + actual payload.
    pub fn total_delta_v(&self, payload_kg: f64) -> f64 {
        let n = self.stage_groups.len();
        let mut total_dv = 0.0;

        // Work from top to bottom to accumulate payload masses, then bottom to top for dv
        // First, compute the dry+wet mass of each group above
        for gi in 0..n {
            let payload_above: f64 = self.stage_groups[gi + 1..].iter()
                .flat_map(|g| g.iter())
                .map(|s| s.wet_mass_kg())
                .sum::<f64>()
                + payload_kg;

            total_dv += self.group_delta_v(gi, payload_above);
        }

        total_dv
    }

    /// Create a Rocket instance from this design at a given location with a payload.
    pub fn instantiate(&self, rocket_id: RocketId, location: &str, payload_mass_kg: f64) -> Rocket {
        let stage_states = self.stage_groups.iter()
            .map(|group| {
                group.iter().map(|stage| StageState {
                    propellant_remaining_kg: stage.propellant_mass_kg,
                    attached: true,
                }).collect()
            })
            .collect();

        Rocket {
            id: rocket_id,
            design_id: self.id,
            location: location.to_string(),
            payload_mass_kg,
            stage_states,
        }
    }
}

/// Compute delta-v for a group of parallel stages with phased burnout.
///
/// Algorithm:
/// 1. Track remaining propellant for each stage
/// 2. Find the stage that runs out of fuel soonest (shortest remaining burn time)
/// 3. All stages fire for that duration; apply Tsiolkovsky for the mass change
/// 4. Jettison the depleted stage(s), reducing total mass
/// 5. Repeat until all stages are depleted
fn phased_parallel_delta_v(stages: &[Stage], payload_above_kg: f64) -> f64 {
    // Working state: (index, remaining_propellant_kg)
    let mut remaining: Vec<(usize, f64)> = stages.iter()
        .enumerate()
        .map(|(i, s)| (i, s.propellant_mass_kg))
        .collect();

    let mut total_dv = 0.0;

    while !remaining.is_empty() {
        // Current total mass: payload + all remaining stages (dry + remaining propellant)
        let stages_mass: f64 = remaining.iter()
            .map(|(i, prop)| stages[*i].dry_mass_kg() + prop)
            .sum();
        let m_initial = payload_above_kg + stages_mass;

        // Find the shortest remaining burn time among active stages
        let min_burn_time = remaining.iter()
            .map(|(i, prop)| {
                let flow = stages[*i].engine.mass_flow_rate() * stages[*i].engine_count as f64;
                if flow <= 0.0 { f64::INFINITY } else { prop / flow }
            })
            .fold(f64::INFINITY, f64::min);

        if min_burn_time <= 0.0 || min_burn_time.is_infinite() {
            break;
        }

        // Total propellant consumed in this phase
        let prop_consumed: f64 = remaining.iter()
            .map(|(i, _)| {
                let flow = stages[*i].engine.mass_flow_rate() * stages[*i].engine_count as f64;
                flow * min_burn_time
            })
            .sum();

        // Compute effective exhaust velocity for this phase
        // For mixed engines: ve_eff = total_thrust / total_mass_flow
        let total_thrust: f64 = remaining.iter()
            .map(|(i, _)| stages[*i].total_thrust_n())
            .sum();
        let total_flow: f64 = remaining.iter()
            .map(|(i, _)| stages[*i].engine.mass_flow_rate() * stages[*i].engine_count as f64)
            .sum();
        let ve_eff = if total_flow > 0.0 { total_thrust / total_flow } else { 0.0 };

        let m_final = m_initial - prop_consumed;
        if m_final <= 0.0 {
            break;
        }

        total_dv += ve_eff * (m_initial / m_final).ln();

        // Update remaining propellant, remove depleted stages
        remaining = remaining.into_iter()
            .filter_map(|(i, prop)| {
                let flow = stages[i].engine.mass_flow_rate() * stages[i].engine_count as f64;
                let new_prop = prop - flow * min_burn_time;
                if new_prop > 1e-6 {
                    Some((i, new_prop))
                } else {
                    None // stage depleted, jettisoned
                }
            })
            .collect();
    }

    total_dv
}

impl Rocket {
    /// Jettison a stage (mark as detached).
    pub fn jettison_stage(&mut self, group: usize, index: usize) -> bool {
        if let Some(state) = self.stage_states.get_mut(group).and_then(|g| g.get_mut(index)) {
            if state.attached {
                state.attached = false;
                state.propellant_remaining_kg = 0.0;
                return true;
            }
        }
        false
    }

    /// Consume propellant from a specific stage to achieve a given delta-v.
    /// Returns the actual delta-v achieved (may be less if propellant runs out).
    pub fn burn(&mut self, design: &RocketDesign, group: usize, index: usize, target_dv: f64) -> f64 {
        // Check preconditions without holding a mutable borrow
        let state_ref = match self.stage_states.get(group).and_then(|g| g.get(index)) {
            Some(s) if s.attached && s.propellant_remaining_kg > 0.0 => s,
            _ => return 0.0,
        };

        let stage = &design.stage_groups[group][index];
        let ve = stage.engine.exhaust_velocity();
        let other_mass = self.attached_mass_except(design, group, index);
        let prop_remaining = state_ref.propellant_remaining_kg;

        let m0 = stage.dry_mass_kg() + prop_remaining + self.payload_mass_kg + other_mass;
        let mf_target = m0 / (target_dv / ve).exp();
        let prop_needed = m0 - mf_target;
        let prop_used = prop_needed.min(prop_remaining);

        // Now take the mutable borrow
        self.stage_states[group][index].propellant_remaining_kg -= prop_used;

        let mf_actual = m0 - prop_used;
        if mf_actual <= 0.0 {
            return 0.0;
        }
        ve * (m0 / mf_actual).ln()
    }

    /// Total remaining delta-v based on current propellant state.
    /// Simplified: treats each group sequentially, each stage in a group independently.
    pub fn remaining_delta_v(&self, design: &RocketDesign) -> f64 {
        let mut total = 0.0;
        let n = self.stage_states.len();

        for gi in 0..n {
            // Payload for this group: everything above
            let payload_above: f64 = (gi + 1..n).map(|gj| {
                design.stage_groups[gj].iter().zip(self.stage_states[gj].iter())
                    .filter(|(_, ss)| ss.attached)
                    .map(|(s, ss)| s.dry_mass_kg() + ss.propellant_remaining_kg)
                    .sum::<f64>()
            }).sum::<f64>() + self.payload_mass_kg;

            // Build temporary stages with remaining propellant for phased calc
            let active_stages: Vec<Stage> = design.stage_groups[gi].iter()
                .zip(self.stage_states[gi].iter())
                .filter(|(_, ss)| ss.attached && ss.propellant_remaining_kg > 0.0)
                .map(|(s, ss)| {
                    let mut s = s.clone();
                    s.propellant_mass_kg = ss.propellant_remaining_kg;
                    s
                })
                .collect();

            if active_stages.len() == 1 {
                total += active_stages[0].delta_v(payload_above);
            } else if active_stages.len() > 1 {
                total += phased_parallel_delta_v(&active_stages, payload_above);
            }
        }

        total
    }

    /// Burn through stage groups sequentially to achieve target delta-v.
    /// Burns the lowest attached group first; when exhausted, jettisons it and
    /// continues with the next group. Returns actual delta-v achieved and
    /// which groups were jettisoned.
    pub fn burn_sequential(&mut self, design: &RocketDesign, target_dv: f64) -> BurnResult {
        let mut dv_remaining = target_dv;
        let mut dv_achieved = 0.0;
        let mut groups_burned = Vec::new();
        let mut groups_jettisoned = Vec::new();
        let n = self.stage_states.len();

        for gi in 0..n {
            if dv_remaining <= 0.0 {
                break;
            }

            // Check if this group has any attached stages with propellant
            let has_fuel = self.stage_states[gi].iter()
                .any(|ss| ss.attached && ss.propellant_remaining_kg > 0.0);
            if !has_fuel {
                continue;
            }

            // Compute how much dv this group can provide
            let group_dv = self.group_remaining_delta_v(design, gi);
            if group_dv <= 0.0 {
                continue;
            }

            if group_dv >= dv_remaining {
                // This group can satisfy the remaining target — partial burn
                let burned = self.burn_group(design, gi, dv_remaining);
                dv_achieved += burned;
                dv_remaining -= burned;
                groups_burned.push(gi);
            } else {
                // Exhaust this entire group, then jettison
                let burned = self.burn_group(design, gi, group_dv);
                dv_achieved += burned;
                dv_remaining -= burned;

                // Jettison all stages in this group
                for si in 0..self.stage_states[gi].len() {
                    self.jettison_stage(gi, si);
                }
                groups_burned.push(gi);
                groups_jettisoned.push(gi);
            }
        }

        BurnResult { dv_achieved, groups_burned, groups_jettisoned }
    }

    /// Compute remaining delta-v for a single group given current propellant state.
    pub fn group_remaining_delta_v(&self, design: &RocketDesign, gi: usize) -> f64 {
        let n = self.stage_states.len();
        let payload_above: f64 = (gi + 1..n).map(|gj| {
            design.stage_groups[gj].iter().zip(self.stage_states[gj].iter())
                .filter(|(_, ss)| ss.attached)
                .map(|(s, ss)| s.dry_mass_kg() + ss.propellant_remaining_kg)
                .sum::<f64>()
        }).sum::<f64>() + self.payload_mass_kg;

        let active_stages: Vec<Stage> = design.stage_groups[gi].iter()
            .zip(self.stage_states[gi].iter())
            .filter(|(_, ss)| ss.attached && ss.propellant_remaining_kg > 0.0)
            .map(|(s, ss)| {
                let mut s = s.clone();
                s.propellant_mass_kg = ss.propellant_remaining_kg;
                s
            })
            .collect();

        if active_stages.len() == 1 {
            active_stages[0].delta_v(payload_above)
        } else if active_stages.len() > 1 {
            phased_parallel_delta_v(&active_stages, payload_above)
        } else {
            0.0
        }
    }

    /// Burn a specific group for a target delta-v, consuming propellant proportionally
    /// across all active stages in the group. Returns actual dv achieved.
    fn burn_group(&mut self, design: &RocketDesign, gi: usize, target_dv: f64) -> f64 {
        let n = self.stage_states.len();

        // Compute payload above this group
        let payload_above: f64 = (gi + 1..n).map(|gj| {
            design.stage_groups[gj].iter().zip(self.stage_states[gj].iter())
                .filter(|(_, ss)| ss.attached)
                .map(|(s, ss)| s.dry_mass_kg() + ss.propellant_remaining_kg)
                .sum::<f64>()
        }).sum::<f64>() + self.payload_mass_kg;

        // Get active stages in this group
        let active_indices: Vec<usize> = self.stage_states[gi].iter()
            .enumerate()
            .filter(|(_, ss)| ss.attached && ss.propellant_remaining_kg > 0.0)
            .map(|(i, _)| i)
            .collect();

        if active_indices.is_empty() {
            return 0.0;
        }

        // Compute effective exhaust velocity for the group
        let total_thrust: f64 = active_indices.iter()
            .map(|&si| {
                let stage = &design.stage_groups[gi][si];
                stage.total_thrust_n()
            })
            .sum();
        let total_flow: f64 = active_indices.iter()
            .map(|&si| {
                let stage = &design.stage_groups[gi][si];
                stage.engine.mass_flow_rate() * stage.engine_count as f64
            })
            .sum();
        let ve = if total_flow > 0.0 { total_thrust / total_flow } else { return 0.0 };

        // Total initial mass
        let group_mass: f64 = active_indices.iter()
            .map(|&si| {
                design.stage_groups[gi][si].dry_mass_kg()
                    + self.stage_states[gi][si].propellant_remaining_kg
            })
            .sum();
        let m0 = group_mass + payload_above;

        // Compute propellant needed for target_dv
        let mf_target = m0 / (target_dv / ve).exp();
        let prop_needed = m0 - mf_target;

        // Total propellant available
        let total_prop: f64 = active_indices.iter()
            .map(|&si| self.stage_states[gi][si].propellant_remaining_kg)
            .sum();

        let prop_used = prop_needed.min(total_prop).max(0.0);

        // Distribute consumed propellant proportionally by mass flow rate
        for &si in &active_indices {
            let stage = &design.stage_groups[gi][si];
            let flow = stage.engine.mass_flow_rate() * stage.engine_count as f64;
            let fraction = if total_flow > 0.0 { flow / total_flow } else { 0.0 };
            let consumed = prop_used * fraction;
            self.stage_states[gi][si].propellant_remaining_kg =
                (self.stage_states[gi][si].propellant_remaining_kg - consumed).max(0.0);
        }

        // Compute actual dv achieved
        let mf_actual = m0 - prop_used;
        if mf_actual <= 0.0 {
            return 0.0;
        }
        ve * (m0 / mf_actual).ln()
    }

    /// Mass of all attached stages except the one at (group, index), plus their propellant.
    fn attached_mass_except(&self, design: &RocketDesign, skip_group: usize, skip_index: usize) -> f64 {
        let mut mass = 0.0;
        for (gi, group) in self.stage_states.iter().enumerate() {
            for (si, ss) in group.iter().enumerate() {
                if gi == skip_group && si == skip_index {
                    continue;
                }
                if ss.attached {
                    mass += design.stage_groups[gi][si].dry_mass_kg() + ss.propellant_remaining_kg;
                }
            }
        }
        mass
    }
}

/// Per-stage-group performance statistics for the rocket designer display.
#[derive(Debug, Clone)]
pub struct StageGroupStats {
    /// Mass ratio: (wet + payload_above) / (dry + payload_above)
    pub mass_ratio: f64,
    /// Tsiolkovsky delta-v (vacuum, no losses)
    pub delta_v_vacuum: f64,
    /// Gravity loss from numerical simulation (m/s)
    pub gravity_loss: f64,
    /// Atmospheric drag loss (first stage only, m/s)
    pub aero_drag_loss: f64,
    /// Effective delta-v: vacuum - gravity - aero
    pub delta_v_effective: f64,
    /// Thrust-to-weight ratio at ignition
    pub twr: f64,
    /// Burn time in seconds
    pub burn_time_s: f64,
}

/// Compute per-stage-group stats for a rocket design.
///
/// `payload_kg` and `launch_from` are user-configurable in the designer.
pub fn compute_stage_stats(
    design: &RocketDesign,
    payload_kg: f64,
    launch_from: &str,
) -> Vec<StageGroupStats> {
    let n = design.stage_groups.len();
    if n == 0 {
        return Vec::new();
    }

    let surface_props = DELTA_V_MAP.surface_properties(launch_from);
    let surface_g = surface_props.map_or(9.81, |p| p.gravity_m_s2);
    let has_atmosphere = surface_props.map_or(false, |p| p.has_atmosphere);

    // Collect per-group params for gravity sim: (thrust_n, mass_flow_kg_s, propellant_kg)
    let mut stage_params: Vec<(f64, f64, f64)> = Vec::with_capacity(n);
    for group in &design.stage_groups {
        let thrust: f64 = group.iter().map(|s| s.total_thrust_n()).sum();
        let flow: f64 = group.iter()
            .map(|s| s.engine.mass_flow_rate() * s.engine_count as f64)
            .sum();
        let prop: f64 = group.iter().map(|s| s.propellant_mass_kg).sum();
        stage_params.push((thrust, flow, prop));
    }

    let total_mass = design.total_mass_kg() + payload_kg;
    let body_radius = surface_props.map_or(6_371_000.0, |p| p.radius_m);
    let gravity_losses = location::simulate_gravity_losses(surface_g, body_radius, &stage_params, total_mass);

    // Compute aero drag loss for first stage only
    let first_stage_aero = if has_atmosphere {
        location::aero_drag_loss(total_mass)
    } else {
        0.0
    };

    let mut results = Vec::with_capacity(n);

    for gi in 0..n {
        let group = &design.stage_groups[gi];
        let (thrust, flow, prop) = stage_params[gi];

        // Mass above this group: upper groups + payload
        let payload_above: f64 = design.stage_groups[gi + 1..].iter()
            .flat_map(|g| g.iter())
            .map(|s| s.wet_mass_kg())
            .sum::<f64>()
            + payload_kg;

        let group_wet: f64 = group.iter().map(|s| s.wet_mass_kg()).sum();
        let group_dry: f64 = group.iter().map(|s| s.dry_mass_kg()).sum();

        let mass_ratio = (group_wet + payload_above) / (group_dry + payload_above);
        let delta_v_vacuum = design.group_delta_v(gi, payload_above);
        let twr = if (group_wet + payload_above) > 0.0 {
            thrust / ((group_wet + payload_above) * surface_g)
        } else {
            0.0
        };
        let burn_time = if flow > 0.0 { prop / flow } else { 0.0 };

        let grav_loss = gravity_losses[gi];
        let aero_loss = if gi == 0 { first_stage_aero } else { 0.0 };
        let delta_v_effective = (delta_v_vacuum - grav_loss - aero_loss).max(0.0);

        results.push(StageGroupStats {
            mass_ratio,
            delta_v_vacuum,
            gravity_loss: grav_loss,
            aero_drag_loss: aero_loss,
            delta_v_effective,
            twr,
            burn_time_s: burn_time,
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::*;
    use crate::propellant::Propellant;
    use crate::stage::*;

    fn kerolox_engine(id: u64, thrust: f64, mass: f64, isp: f64) -> EngineDesign {
        EngineDesign {
            id: EngineId(id),
            name: format!("Engine-{}", id),
            cycle: EngineCycle::GasGenerator,
            thrust_n: thrust,
            mass_kg: mass,
            isp_s: isp,
            exit_pressure_pa: 70_000.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
        }
    }

    fn solid_engine(id: u64, thrust: f64, mass: f64, isp: f64) -> EngineDesign {
        EngineDesign {
            id: EngineId(id),
            name: format!("SRB-{}", id),
            cycle: EngineCycle::PressureFed,
            thrust_n: thrust,
            mass_kg: mass,
            isp_s: isp,
            exit_pressure_pa: 100_000.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::SolidMix, mass_fraction: 1.0 },
            ],
        }
    }

    // --- Sequential staging tests ---

    #[test]
    fn test_two_stage_sequential_delta_v() {
        let engine1 = kerolox_engine(1, 1_000_000.0, 500.0, 280.0);
        let engine2 = kerolox_engine(2, 200_000.0, 100.0, 340.0);

        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine1.clone(), engine_count: 1,
            propellant_mass_kg: 50_000.0, structural_mass_kg: 3_000.0,
            fairing: None,
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: engine2.clone(), engine_count: 1,
            propellant_mass_kg: 10_000.0, structural_mass_kg: 500.0,
            fairing: None,
        };

        let rocket = RocketDesign {
            id: RocketDesignId(1),
            name: "TwoStager".into(),
            stage_groups: vec![vec![s1.clone()], vec![s2.clone()]],
        };

        let payload = 1_000.0;
        let total_dv = rocket.total_delta_v(payload);

        // S2 payload = just the actual payload
        let s2_dv = s2.delta_v(payload);
        // S1 payload = S2 wet mass + payload
        let s1_payload = s2.wet_mass_kg() + payload;
        let s1_dv = s1.delta_v(s1_payload);

        let expected = s1_dv + s2_dv;
        assert!(
            (total_dv - expected).abs() < 1.0,
            "total_dv={}, expected={} (s1_dv={}, s2_dv={})",
            total_dv, expected, s1_dv, s2_dv
        );
    }

    // --- Parallel burnout tests ---

    #[test]
    fn test_parallel_identical_stages_same_as_single() {
        // Two identical stages in parallel should give the same delta-v as one
        // stage with doubled thrust (same mass ratio, same Ve)
        let engine = kerolox_engine(1, 500_000.0, 250.0, 300.0);

        let stage = Stage {
            id: StageId(1), name: "Booster".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 20_000.0, structural_mass_kg: 1_000.0,
            fairing: None,
        };

        let rocket = RocketDesign {
            id: RocketDesignId(1),
            name: "TwinBooster".into(),
            stage_groups: vec![vec![stage.clone(), stage.clone()]],
        };

        let payload = 2_000.0;
        let parallel_dv = rocket.group_delta_v(0, payload);

        // Two identical parallel stages: Ve * ln((2*wet + payload) / (2*dry + payload))
        let ve = engine.exhaust_velocity();
        let m0 = 2.0 * stage.wet_mass_kg() + payload;
        let mf = 2.0 * stage.dry_mass_kg() + payload;
        let expected = ve * (m0 / mf).ln();

        assert!(
            (parallel_dv - expected).abs() < 1.0,
            "parallel_dv={}, expected={}", parallel_dv, expected
        );
    }

    #[test]
    fn test_core_plus_srbs_phased_burnout() {
        // SRBs burn out before the core. The simulation should:
        // Phase 1: all three fire until SRBs deplete
        // Phase 2: core continues alone with reduced mass
        let core_engine = kerolox_engine(1, 800_000.0, 400.0, 311.0);
        let srb_engine = solid_engine(2, 1_500_000.0, 200.0, 250.0);

        let core = Stage {
            id: StageId(1), name: "Core".into(),
            engine: core_engine.clone(), engine_count: 1,
            propellant_mass_kg: 100_000.0, structural_mass_kg: 5_000.0,
            fairing: None,
        };
        let srb = Stage {
            id: StageId(2), name: "SRB".into(),
            engine: srb_engine.clone(), engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
        };

        let rocket = RocketDesign {
            id: RocketDesignId(1),
            name: "CorePlusSRBs".into(),
            stage_groups: vec![vec![core.clone(), srb.clone(), srb.clone()]],
        };

        let payload = 5_000.0;
        let dv = rocket.group_delta_v(0, payload);

        // dv should be greater than just the core alone (SRBs help)
        let core_only_dv = core.delta_v(payload);
        assert!(
            dv > core_only_dv,
            "Parallel dv {} should exceed core-only dv {}", dv, core_only_dv
        );

        // dv should be positive and reasonable (less than 20 km/s for these params)
        assert!(dv > 0.0 && dv < 20_000.0, "dv={} out of reasonable range", dv);
    }

    #[test]
    fn test_core_plus_srbs_two_phases() {
        // Verify that the phased calculation produces a different (better) result
        // than naively treating all stages as having the same burn time
        let core_engine = kerolox_engine(1, 500_000.0, 300.0, 320.0);
        let srb_engine = solid_engine(2, 1_000_000.0, 150.0, 240.0);

        let core = Stage {
            id: StageId(1), name: "Core".into(),
            engine: core_engine.clone(), engine_count: 1,
            propellant_mass_kg: 80_000.0, structural_mass_kg: 4_000.0,
            fairing: None,
        };
        let srb = Stage {
            id: StageId(2), name: "SRB".into(),
            engine: srb_engine.clone(), engine_count: 1,
            propellant_mass_kg: 20_000.0, structural_mass_kg: 1_500.0,
            fairing: None,
        };

        let payload = 10_000.0;
        let phased_dv = phased_parallel_delta_v(&[core.clone(), srb.clone()], payload);

        // Compare with naive: treat as single burn with average Ve
        // (this should be different because SRBs separate mid-burn)
        let total_thrust = core.total_thrust_n() + srb.total_thrust_n();
        let total_flow = core.engine.mass_flow_rate() + srb.engine.mass_flow_rate();
        let naive_ve = total_thrust / total_flow;
        let m0 = core.wet_mass_kg() + srb.wet_mass_kg() + payload;
        let mf = core.dry_mass_kg() + srb.dry_mass_kg() + payload;
        let naive_dv = naive_ve * (m0 / mf).ln();

        // Phased should be BETTER than naive (mass drops when SRBs jettison)
        assert!(
            phased_dv > naive_dv,
            "Phased dv {} should exceed naive dv {} (SRB jettison saves mass)",
            phased_dv, naive_dv
        );
    }

    // --- Multi-group tests ---

    #[test]
    fn test_full_rocket_core_srbs_upper() {
        let core_engine = kerolox_engine(1, 800_000.0, 400.0, 311.0);
        let srb_engine = solid_engine(2, 1_500_000.0, 200.0, 250.0);
        let upper_engine = kerolox_engine(3, 100_000.0, 80.0, 348.0);

        let core = Stage {
            id: StageId(1), name: "Core".into(),
            engine: core_engine, engine_count: 1,
            propellant_mass_kg: 100_000.0, structural_mass_kg: 5_000.0,
            fairing: None,
        };
        let srb = Stage {
            id: StageId(2), name: "SRB".into(),
            engine: srb_engine, engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
        };
        let upper = Stage {
            id: StageId(3), name: "Upper".into(),
            engine: upper_engine, engine_count: 1,
            propellant_mass_kg: 15_000.0, structural_mass_kg: 800.0,
            fairing: Some(Fairing { mass_kg: 200.0, diameter_m: 4.0 }),
        };

        let rocket = RocketDesign {
            id: RocketDesignId(1),
            name: "Atlas-like".into(),
            stage_groups: vec![
                vec![core, srb.clone(), srb],
                vec![upper],
            ],
        };

        assert!(rocket.validate().is_empty());

        let payload = 5_000.0;
        let total_dv = rocket.total_delta_v(payload);
        assert!(total_dv > 5_000.0, "Should have significant delta-v, got {}", total_dv);
        assert!(total_dv < 20_000.0, "Sanity check: {}", total_dv);
    }

    // --- Rocket instance tests ---

    #[test]
    fn test_instantiate_and_remaining_dv() {
        let engine = kerolox_engine(1, 500_000.0, 250.0, 300.0);
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 8_000.0, structural_mass_kg: 500.0,
            fairing: None,
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };

        let payload = 1_000.0;
        let rocket = design.instantiate(RocketId(1), "earth_surface", payload);

        // Fresh rocket should have same delta-v as design
        let design_dv = design.total_delta_v(payload);
        let instance_dv = rocket.remaining_delta_v(&design);
        assert!(
            (design_dv - instance_dv).abs() < 1.0,
            "design_dv={}, instance_dv={}", design_dv, instance_dv
        );
    }

    #[test]
    fn test_burn_consumes_propellant() {
        let engine = kerolox_engine(1, 500_000.0, 250.0, 300.0);
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1]],
        };

        let mut rocket = design.instantiate(RocketId(1), "earth_surface", 1_000.0);
        let initial_dv = rocket.remaining_delta_v(&design);

        let burned = rocket.burn(&design, 0, 0, 1_000.0);
        assert!((burned - 1_000.0).abs() < 1.0, "Should burn ~1000 m/s, got {}", burned);

        let after_dv = rocket.remaining_delta_v(&design);
        assert!(after_dv < initial_dv, "Delta-v should decrease after burn");
        assert!((initial_dv - after_dv - 1_000.0).abs() < 50.0,
            "Should have lost ~1000 m/s of dv capability");
    }

    #[test]
    fn test_jettison_stage() {
        let engine = kerolox_engine(1, 500_000.0, 250.0, 300.0);
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 8_000.0, structural_mass_kg: 500.0,
            fairing: None,
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };

        let mut rocket = design.instantiate(RocketId(1), "earth_surface", 1_000.0);

        assert!(rocket.jettison_stage(0, 0));
        assert!(!rocket.stage_states[0][0].attached);
        assert_eq!(rocket.stage_states[0][0].propellant_remaining_kg, 0.0);

        // Can't jettison again
        assert!(!rocket.jettison_stage(0, 0));
    }

    #[test]
    fn test_total_mass() {
        let engine = kerolox_engine(1, 500_000.0, 250.0, 300.0);
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1]],
        };

        // wet = structural(2000) + engine(250) + prop(30000) = 32250
        assert_eq!(design.total_mass_kg(), 32_250.0);
    }

    #[test]
    fn test_validation() {
        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Empty".into(),
            stage_groups: vec![],
        };
        assert!(!design.validate().is_empty());

        let design2 = RocketDesign {
            id: RocketDesignId(2),
            name: "EmptyGroup".into(),
            stage_groups: vec![vec![]],
        };
        assert!(!design2.validate().is_empty());
    }

    #[test]
    fn test_multi_stage_available_in_group() {
        // Two different stages in the same group (e.g., ion + lander)
        // Both should be available; delta-v should account for both
        let ion_engine = EngineDesign {
            id: EngineId(10),
            name: "Ion".into(),
            cycle: EngineCycle::PressureFed, // placeholder cycle
            thrust_n: 1.0,
            mass_kg: 50.0,
            isp_s: 3000.0,
            exit_pressure_pa: 0.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 1.0 },
            ],
        };
        let lander_engine = kerolox_engine(11, 50_000.0, 100.0, 320.0);

        let ion_stage = Stage {
            id: StageId(10), name: "Ion".into(),
            engine: ion_engine, engine_count: 1,
            propellant_mass_kg: 200.0, structural_mass_kg: 100.0,
            fairing: None,
        };
        let lander_stage = Stage {
            id: StageId(11), name: "Lander".into(),
            engine: lander_engine, engine_count: 1,
            propellant_mass_kg: 5_000.0, structural_mass_kg: 500.0,
            fairing: None,
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "IonLander".into(),
            stage_groups: vec![vec![ion_stage, lander_stage]],
        };

        assert!(design.validate().is_empty());
        let dv = design.total_delta_v(500.0);
        assert!(dv > 0.0, "Should have positive delta-v");
    }

    // ==========================================
    // Stage stats tests
    // ==========================================

    #[test]
    fn test_compute_stage_stats_two_stage() {
        // Realistic two-stage: high-thrust first stage, lighter upper stage
        let engine1 = kerolox_engine(1, 2_000_000.0, 500.0, 300.0);
        let engine2 = kerolox_engine(2, 400_000.0, 100.0, 340.0);

        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine1, engine_count: 1,
            propellant_mass_kg: 80_000.0, structural_mass_kg: 3_000.0,
            fairing: None,
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: engine2, engine_count: 1,
            propellant_mass_kg: 15_000.0, structural_mass_kg: 500.0,
            fairing: None,
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };

        let stats = compute_stage_stats(&design, 1_000.0, "earth_surface");
        assert_eq!(stats.len(), 2);

        // First stage should have gravity and aero losses
        assert!(stats[0].gravity_loss > 0.0, "S1 should have gravity loss");
        assert!(stats[0].aero_drag_loss > 0.0, "S1 should have aero loss on Earth");
        assert!(stats[0].delta_v_effective < stats[0].delta_v_vacuum,
            "S1 effective dv should be less than vacuum");
        assert!(stats[0].twr > 0.0, "S1 should have positive TWR");
        assert!(stats[0].mass_ratio > 1.0, "S1 mass ratio should be > 1");

        // Second stage should have no aero loss
        assert_eq!(stats[1].aero_drag_loss, 0.0, "S2 should have no aero loss");
        // Both stages have gravity losses, but effective dv should be less than vacuum for both
        assert!(stats[1].delta_v_effective <= stats[1].delta_v_vacuum,
            "Upper stage effective dv should not exceed vacuum");
    }

    #[test]
    fn test_stage_stats_more_engines_less_gravity_loss() {
        let engine = kerolox_engine(1, 500_000.0, 200.0, 300.0);

        // 1 engine first stage
        let s1_single = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
        };
        let design_single = RocketDesign {
            id: RocketDesignId(1),
            name: "Single".into(),
            stage_groups: vec![vec![s1_single]],
        };

        // 3 engine first stage
        let s1_triple = Stage {
            id: StageId(2), name: "S1".into(),
            engine: engine.clone(), engine_count: 3,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
        };
        let design_triple = RocketDesign {
            id: RocketDesignId(2),
            name: "Triple".into(),
            stage_groups: vec![vec![s1_triple]],
        };

        let stats_single = compute_stage_stats(&design_single, 1_000.0, "earth_surface");
        let stats_triple = compute_stage_stats(&design_triple, 1_000.0, "earth_surface");

        assert!(stats_triple[0].twr > stats_single[0].twr,
            "3 engines should have higher TWR");
        assert!(stats_triple[0].gravity_loss < stats_single[0].gravity_loss,
            "3 engines (loss={:.0}) should have less gravity loss than 1 engine (loss={:.0})",
            stats_triple[0].gravity_loss, stats_single[0].gravity_loss);
    }

    #[test]
    fn test_stage_stats_lunar_no_aero() {
        let engine = kerolox_engine(1, 500_000.0, 200.0, 300.0);
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine, engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
        };
        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1]],
        };

        let stats = compute_stage_stats(&design, 1_000.0, "lunar_surface");
        assert_eq!(stats[0].aero_drag_loss, 0.0, "No aero loss on Moon");
        assert!(stats[0].gravity_loss > 0.0, "Should still have gravity loss on Moon");
    }

    #[test]
    fn test_stage_stats_empty_design() {
        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Empty".into(),
            stage_groups: vec![],
        };
        let stats = compute_stage_stats(&design, 1_000.0, "earth_surface");
        assert!(stats.is_empty());
    }

    // ==========================================
    // burn_sequential tests
    // ==========================================

    #[test]
    fn test_burn_sequential_single_group() {
        let engine = kerolox_engine(1, 500_000.0, 250.0, 300.0);
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1]],
        };

        let mut rocket = design.instantiate(RocketId(1), "earth_surface", 1_000.0);
        let initial_dv = rocket.remaining_delta_v(&design);

        let result = rocket.burn_sequential(&design, 1_000.0);
        assert!((result.dv_achieved - 1_000.0).abs() < 1.0, "Should burn ~1000 m/s, got {}", result.dv_achieved);
        assert!(result.groups_jettisoned.is_empty());

        let after_dv = rocket.remaining_delta_v(&design);
        assert!(after_dv < initial_dv);
        assert!((initial_dv - after_dv - 1_000.0).abs() < 50.0);
    }

    #[test]
    fn test_burn_sequential_two_groups_crosses_staging() {
        let engine1 = kerolox_engine(1, 1_000_000.0, 500.0, 280.0);
        let engine2 = kerolox_engine(2, 200_000.0, 100.0, 340.0);

        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine1, engine_count: 1,
            propellant_mass_kg: 50_000.0, structural_mass_kg: 3_000.0,
            fairing: None,
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: engine2, engine_count: 1,
            propellant_mass_kg: 10_000.0, structural_mass_kg: 500.0,
            fairing: None,
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "TwoStager".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };

        let mut rocket = design.instantiate(RocketId(1), "earth_surface", 1_000.0);
        let total_dv = rocket.remaining_delta_v(&design);

        // Burn for more than the first stage can provide — should cross into second stage
        let s1_dv = rocket.group_remaining_delta_v(&design, 0);
        let target = s1_dv + 500.0; // need some from S2

        let result = rocket.burn_sequential(&design, target);
        assert!((result.dv_achieved - target).abs() < 50.0,
            "Should burn ~{} m/s, got {}", target, result.dv_achieved);
        assert_eq!(result.groups_jettisoned, vec![0]);

        // First stage should be jettisoned
        assert!(!rocket.stage_states[0][0].attached,
            "S1 should be jettisoned after exhaustion");

        // Should have some dv left in S2
        let remaining = rocket.remaining_delta_v(&design);
        assert!(remaining > 0.0, "Should have dv remaining in S2");
        assert!((total_dv - result.dv_achieved - remaining).abs() < 100.0,
            "total={}, burned={}, remaining={}", total_dv, result.dv_achieved, remaining);
    }

    #[test]
    fn test_burn_sequential_exceeds_total_dv() {
        let engine = kerolox_engine(1, 500_000.0, 250.0, 300.0);
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 10_000.0, structural_mass_kg: 1_000.0,
            fairing: None,
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1]],
        };

        let mut rocket = design.instantiate(RocketId(1), "earth_surface", 1_000.0);
        let total_dv = rocket.remaining_delta_v(&design);

        // Ask for way more than available
        let result = rocket.burn_sequential(&design, total_dv + 5_000.0);
        assert!((result.dv_achieved - total_dv).abs() < 50.0,
            "Should only burn total available dv={}, got {}", total_dv, result.dv_achieved);
    }
}
