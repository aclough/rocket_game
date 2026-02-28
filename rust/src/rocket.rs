use serde::{Serialize, Deserialize};

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

/// A physical rocket instance with runtime state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rocket {
    pub id: RocketId,
    pub design_id: RocketDesignId,
    pub location: &'static str,
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
    pub fn instantiate(&self, rocket_id: RocketId, location: &'static str, payload_mass_kg: f64) -> Rocket {
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
            location,
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
}
