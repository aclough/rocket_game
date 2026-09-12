//! Burns and staging: the phases a stage group burns in (core and
//! boosters together, then the core alone), the design's delta-v per
//! group, and the flying rocket's sequential burn with jettison.

use crate::location::{self, DELTA_V_MAP};
use crate::stage::Stage;

use super::{BurnResult, DesignPerformance, Rocket, RocketDesign};

/// One interval of a stage group's burn during which the same stages
/// fire. A Shuttle-style group has two: core + boosters until the
/// boosters run dry, then the core alone with the boosters' structure
/// gone. This is the primitive under the vacuum delta-v, the ascent
/// integration and the in-flight burn, so all three see the same
/// vehicle.
#[derive(Debug, Clone)]
pub struct BurnPhase {
    /// Indices (into the stage list given to `burn_phases`) firing.
    pub active: Vec<usize>,
    pub thrust_n: f64,
    pub mass_flow_kg_s: f64,
    pub duration_s: f64,
    /// Propellant consumed during this phase.
    pub propellant_kg: f64,
    /// Everything above the group plus the group's mass, at phase start
    /// and end.
    pub mass_start_kg: f64,
    pub mass_end_kg: f64,
    /// Stages that run dry as this phase ends and drop off.
    pub jettisoned: Vec<usize>,
    pub dry_mass_dropped_kg: f64,
}

impl BurnPhase {
    /// Tsiolkovsky delta-v of this phase, at the phase's blended
    /// exhaust velocity (total thrust over total flow).
    pub fn delta_v(&self) -> f64 {
        if self.mass_flow_kg_s <= 0.0 || self.mass_end_kg <= 0.0 {
            return 0.0;
        }
        (self.thrust_n / self.mass_flow_kg_s) * (self.mass_start_kg / self.mass_end_kg).ln()
    }

    /// The ascent integrator's view of this phase; `stages` is the list
    /// `active` indexes, for the nozzles firing.
    pub fn ascent_phase(&self, stages: &[Stage]) -> location::AscentPhase {
        location::AscentPhase {
            thrust_n: self.thrust_n,
            mass_flow_kg_s: self.mass_flow_kg_s,
            propellant_kg: self.propellant_kg,
            dry_mass_dropped_kg: self.dry_mass_dropped_kg,
            nozzles: self.active.iter().map(|&k| location::AscentNozzle {
                exit_pressure_pa: stages[k].engine.exit_pressure_pa,
                thrust_n: stages[k].total_thrust_n(),
            }).collect(),
        }
    }
}

/// Burn `stages` down from `remaining_kg` of propellant each, with
/// `payload_above_kg` on top, as phases: all stages fire, the one with
/// the shortest remaining burn empties and drops off, repeat.
pub fn burn_phases(stages: &[Stage], remaining_kg: &[f64], payload_above_kg: f64) -> Vec<BurnPhase> {
    let mut remaining: Vec<(usize, f64)> = remaining_kg.iter().copied().enumerate().collect();
    let mut phases = Vec::new();

    while !remaining.is_empty() {
        // Current total mass: payload + all remaining stages (dry + remaining propellant)
        let stages_mass: f64 = remaining.iter()
            .map(|(i, prop)| stages[*i].dry_mass_kg() + prop)
            .sum();
        let m_initial = payload_above_kg + stages_mass;

        // The shortest remaining burn among the firing stages ends the phase.
        let min_burn_time = remaining.iter()
            .map(|(i, prop)| {
                let flow = stages[*i].mass_flow_kg_s();
                if flow <= 0.0 { f64::INFINITY } else { prop / flow }
            })
            .fold(f64::INFINITY, f64::min);
        if min_burn_time <= 0.0 || min_burn_time.is_infinite() {
            break;
        }

        let total_thrust: f64 = remaining.iter().map(|(i, _)| stages[*i].total_thrust_n()).sum();
        let total_flow: f64 = remaining.iter().map(|(i, _)| stages[*i].mass_flow_kg_s()).sum();

        // What each stage burns this phase; a stage that empties burns
        // exactly what it had, so the accounting closes.
        let mut next = Vec::new();
        let mut jettisoned = Vec::new();
        let mut consumed = 0.0;
        let mut dry_dropped = 0.0;
        for &(i, prop) in &remaining {
            let burned = stages[i].mass_flow_kg_s() * min_burn_time;
            let left = prop - burned;
            if left > 1e-6 {
                consumed += burned;
                next.push((i, left));
            } else {
                consumed += prop;
                jettisoned.push(i);
                dry_dropped += stages[i].dry_mass_kg();
            }
        }
        let m_final = m_initial - consumed;
        if m_final <= 0.0 {
            break;
        }

        phases.push(BurnPhase {
            active: remaining.iter().map(|(i, _)| *i).collect(),
            thrust_n: total_thrust,
            mass_flow_kg_s: total_flow,
            duration_s: min_burn_time,
            propellant_kg: consumed,
            mass_start_kg: m_initial,
            mass_end_kg: m_final,
            jettisoned,
            dry_mass_dropped_kg: dry_dropped,
        });
        remaining = next;
    }

    phases
}

impl RocketDesign {
    /// The phases of stage group `gi`'s burn with full tanks and
    /// `payload_above_kg` (upper groups plus payload) on top.
    pub fn burn_phases(&self, gi: usize, payload_above_kg: f64) -> Vec<BurnPhase> {
        let Some(group) = self.stage_groups.get(gi) else { return Vec::new() };
        let full: Vec<f64> = group.iter().map(|s| s.propellant_mass_kg).collect();
        burn_phases(group, &full, payload_above_kg)
    }
}

/// Delta-v of a group of parallel stages with phased burnout: the sum
/// over its phases.
pub(super) fn phased_parallel_delta_v(stages: &[Stage], payload_above_kg: f64) -> f64 {
    let full: Vec<f64> = stages.iter().map(|s| s.propellant_mass_kg).collect();
    burn_phases(stages, &full, payload_above_kg).iter().map(|p| p.delta_v()).sum()
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

    /// Whether the current active stage group (lowest with propellant or solar sail) is low-thrust.
    pub fn is_current_stage_low_thrust(&self, design: &RocketDesign) -> bool {
        for (gi, group) in design.stage_groups.iter().enumerate() {
            let is_active = self.stage_states.get(gi)
                .is_some_and(|ss| ss.iter().any(|s| s.attached && (
                    s.propellant_remaining_kg > 0.0
                    || group.iter().any(|st| st.engine.is_solar_sail())
                )));
            if is_active {
                return group.iter().any(|s| s.engine.is_low_thrust());
            }
        }
        false
    }

    /// Total remaining delta-v based on current propellant state: each
    /// group's [`Self::group_remaining_delta_v`], lowest first (∞ with a
    /// sail aboard).
    pub fn remaining_delta_v(&self, design: &RocketDesign) -> f64 {
        (0..self.stage_states.len())
            .map(|gi| self.group_remaining_delta_v(design, gi))
            .sum()
    }

    /// Burn through stage groups sequentially to achieve target delta-v.
    /// Burns the lowest attached group first; when exhausted, jettisons it and
    /// continues with the next group. Returns actual delta-v achieved and
    /// which groups were jettisoned.
    ///
    /// `from` is where the burn starts. From a surface with an atmosphere
    /// the first group pays the sea-level Isp penalty the ascent
    /// integration averaged over its climb
    /// (`AscentLosses::isp_fraction_by_group`) — the same figure the
    /// planner budgeted, so the flight arrives with what the design
    /// promised. Anywhere else, and for every later group, the nozzles
    /// run in vacuum.
    pub fn burn_sequential(&mut self, design: &RocketDesign, target_dv: f64, from: &str) -> BurnResult {
        let mut dv_remaining = target_dv;
        let mut dv_achieved = 0.0;
        let mut groups_burned = Vec::new();
        let mut groups_jettisoned = Vec::new();
        let mut stages_jettisoned = Vec::new();
        let n = self.stage_states.len();
        let atmospheric = DELTA_V_MAP.surface_properties(from).is_some_and(|p| p.has_atmosphere);
        let isp_fractions: Vec<f64> = if atmospheric {
            DesignPerformance::compute(design, self.payload_mass_kg, from).ascent.isp_fraction_by_group
        } else {
            Vec::new()
        };

        for gi in 0..n {
            if dv_remaining <= 0.0 {
                break;
            }

            // Solar sail: infinite dv, no propellant consumed
            if self.group_has_attached_sail(design, gi) {
                dv_achieved += dv_remaining;
                groups_burned.push(gi);
                break;
            }

            if !self.group_has_propellant(gi) {
                continue;
            }

            // Compute how much dv this group can provide
            let group_dv = self.group_remaining_delta_v(design, gi);
            if group_dv <= 0.0 {
                continue;
            }

            let isp_fraction = isp_fractions.get(gi).copied().unwrap_or(1.0);

            if group_dv >= dv_remaining {
                // This group can satisfy the remaining target — partial burn
                let (burned, dropped) = self.burn_group(design, gi, dv_remaining, isp_fraction);
                dv_achieved += burned;
                dv_remaining -= burned;
                groups_burned.push(gi);
                stages_jettisoned.extend(dropped.into_iter().map(|si| (gi, si)));
            } else {
                // Exhaust this entire group — burn all propellant
                let (burned, dropped) = self.burn_group(design, gi, f64::INFINITY, isp_fraction);
                dv_achieved += burned;
                dv_remaining -= burned;
                stages_jettisoned.extend(dropped.into_iter().map(|si| (gi, si)));

                // Jettison whatever is left of the group (stages that
                // never had propellant, or that `burn_group` drained
                // without emptying below its threshold).
                for si in 0..self.stage_states[gi].len() {
                    if self.jettison_stage(gi, si) {
                        stages_jettisoned.push((gi, si));
                    }
                }
                groups_burned.push(gi);
                groups_jettisoned.push(gi);
            }
        }

        BurnResult { dv_achieved, groups_burned, groups_jettisoned, stages_jettisoned }
    }

    /// Compute remaining delta-v for a single group given current propellant state.
    pub fn group_remaining_delta_v(&self, design: &RocketDesign, gi: usize) -> f64 {
        // Solar sail: infinite dv
        if self.group_has_attached_sail(design, gi) {
            return f64::INFINITY;
        }

        let payload_above = self.mass_above_group(design, gi);

        let active_stages: Vec<Stage> = self.attached_stages_in(design, gi)
            .filter(|(_, _, ss)| ss.propellant_remaining_kg > 0.0)
            .map(|(_, s, ss)| {
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

    /// Burn a specific group for a target delta-v, phase by phase: the
    /// stages fire together, the shortest tank empties and that stage is
    /// jettisoned, and the rest carry on — the same phases the design's
    /// delta-v and the ascent integration are built from. `isp_fraction`
    /// scales the exhaust velocity for the air the group burns through
    /// (1.0 in vacuum). Returns the delta-v achieved and the stages
    /// dropped along the way.
    fn burn_group(
        &mut self, design: &RocketDesign, gi: usize, target_dv: f64, isp_fraction: f64,
    ) -> (f64, Vec<usize>) {

        let payload_above = self.mass_above_group(design, gi);

        let group = &design.stage_groups[gi];
        let mut dv_remaining = target_dv;
        let mut dv_achieved = 0.0;
        let mut dropped = Vec::new();

        loop {
            // Stages still firing, with what they have left.
            let active: Vec<usize> = self.stage_states[gi].iter()
                .enumerate()
                .filter(|(_, ss)| ss.attached && ss.propellant_remaining_kg > 0.0)
                .map(|(i, _)| i)
                .collect();
            if active.is_empty() || dv_remaining <= 0.0 {
                break;
            }
            let subset: Vec<Stage> = active.iter().map(|&si| group[si].clone()).collect();
            let remaining: Vec<f64> = active.iter()
                .map(|&si| self.stage_states[gi][si].propellant_remaining_kg)
                .collect();
            let Some(phase) = burn_phases(&subset, &remaining, payload_above).into_iter().next() else {
                break;
            };

            // Exhaust velocity for this phase, less the overexpansion Isp
            // penalty the ascent averaged for this group.
            let total_thrust: f64 = phase.active.iter()
                .map(|&k| subset[k].total_thrust_n())
                .sum::<f64>() * isp_fraction;
            let total_flow = phase.mass_flow_kg_s;
            if total_flow <= 0.0 {
                break;
            }
            let ve = total_thrust / total_flow;
            let m0 = phase.mass_start_kg;

            // Propellant the remaining target needs at this exhaust velocity,
            // capped at what the phase holds.
            let mf_target = m0 / (dv_remaining / ve).exp();
            let prop_needed = m0 - mf_target;
            let prop_used = prop_needed.min(phase.propellant_kg).max(0.0);
            let whole_phase = prop_needed >= phase.propellant_kg;

            // Spend it across the firing stages in proportion to mass flow.
            for &k in &phase.active {
                let si = active[k];
                let fraction = subset[k].mass_flow_kg_s() / total_flow;
                let ss = &mut self.stage_states[gi][si];
                ss.propellant_remaining_kg = (ss.propellant_remaining_kg - prop_used * fraction).max(0.0);
            }
            let mf_actual = m0 - prop_used;
            if mf_actual <= 0.0 {
                break;
            }
            let burned = ve * (m0 / mf_actual).ln();
            dv_achieved += burned;
            dv_remaining -= burned;

            if !whole_phase {
                break;
            }
            // The phase ran to its end: the stages that emptied drop off,
            // and the next phase (if any) carries on without them.
            for &k in &phase.jettisoned {
                let si = active[k];
                if self.jettison_stage(gi, si) {
                    dropped.push(si);
                }
            }
        }

        (dv_achieved, dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rocket::*;
    use crate::engine::*;
    use crate::propellant::Propellant;
    use crate::stage::*;
    use crate::rocket::test_fixtures::*;

    #[test]
    fn test_two_stage_sequential_delta_v() {
        let engine1 = kerolox_engine(1, 1_000_000.0, 500.0, 280.0);
        let engine2 = kerolox_engine(2, 200_000.0, 100.0, 340.0);

        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine1.clone(), engine_count: 1,
            propellant_mass_kg: 50_000.0, structural_mass_kg: 3_000.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: engine2.clone(), engine_count: 1,
            propellant_mass_kg: 10_000.0, structural_mass_kg: 500.0,
            fairing: None,
            power_sources: Vec::new(),
        };

        let rocket = RocketDesign {
            id: RocketDesignId(1),
            name: "TwoStager".into(),
            stage_groups: vec![vec![s1.clone()], vec![s2.clone()]],
        };

        let payload = 1_000.0;
        let total_dv = rocket.vacuum_delta_v(payload);

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
            power_sources: Vec::new(),
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
            power_sources: Vec::new(),
        };
        let srb = Stage {
            id: StageId(2), name: "SRB".into(),
            engine: srb_engine.clone(), engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
            power_sources: Vec::new(),
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

    /// The ascent integration has to see the boosters leave: after they
    /// run dry the core climbs alone on a fraction of the thrust, so the
    /// vehicle fights gravity for longer than a model that kept the
    /// whole group's thrust until the whole group's propellant was gone.
    #[test]
    fn gravity_loss_sees_boosters_burn_out() {
        let core_engine = kerolox_engine(1, 800_000.0, 400.0, 311.0);
        let srb_engine = solid_engine(2, 1_500_000.0, 200.0, 250.0);
        let core = Stage {
            id: StageId(1), name: "Core".into(),
            engine: core_engine, engine_count: 1,
            propellant_mass_kg: 100_000.0, structural_mass_kg: 5_000.0,
            fairing: None, power_sources: Vec::new(),
        };
        let srb = Stage {
            id: StageId(2), name: "SRB".into(),
            engine: srb_engine, engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None, power_sources: Vec::new(),
        };
        let design = RocketDesign {
            id: RocketDesignId(1), name: "CorePlusSRBs".into(),
            stage_groups: vec![vec![core.clone(), srb.clone(), srb.clone()]],
        };
        let payload = 5_000.0;

        let phases = design.burn_phases(0, payload);
        assert_eq!(phases.len(), 2, "boosters then core alone");
        assert_eq!(phases[0].jettisoned.len(), 2, "both boosters leave together");
        assert!(phases[1].thrust_n < phases[0].thrust_n);

        let phased = group_gravity_losses(&design, payload, "earth_surface")[0];
        let props = DELTA_V_MAP.surface_properties("earth_surface").unwrap();
        let group = &design.stage_groups[0];
        let lumped = location::simulate_gravity_losses(
            props,
            &[(
                group.iter().map(|s| s.total_thrust_n()).sum(),
                group.iter().map(|s| s.mass_flow_kg_s()).sum(),
                group.iter().map(|s| s.propellant_mass_kg).sum(),
                group.iter().map(|s| s.dry_mass_kg()).sum(),
            )],
            design.total_mass_kg() + payload,
        )[0];
        assert!(phased > lumped,
            "phased loss {phased:.0} should exceed the lumped model's {lumped:.0}");
    }

    /// In flight the boosters drop off when their tanks empty, not when
    /// the whole group is spent.
    #[test]
    fn boosters_drop_off_when_empty_in_flight() {
        let core_engine = kerolox_engine(1, 800_000.0, 400.0, 311.0);
        let srb_engine = solid_engine(2, 1_500_000.0, 200.0, 250.0);
        let core = Stage {
            id: StageId(1), name: "Core".into(),
            engine: core_engine, engine_count: 1,
            propellant_mass_kg: 100_000.0, structural_mass_kg: 5_000.0,
            fairing: None, power_sources: Vec::new(),
        };
        let srb = Stage {
            id: StageId(2), name: "SRB".into(),
            engine: srb_engine, engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None, power_sources: Vec::new(),
        };
        let design = RocketDesign {
            id: RocketDesignId(1), name: "CorePlusSRBs".into(),
            stage_groups: vec![vec![core, srb.clone(), srb]],
        };
        let payload = 5_000.0;
        let mut rocket = design.instantiate(RocketId(1), payload);
        let first_phase_dv = design.burn_phases(0, payload)[0].delta_v();

        // Burn a little past the boosters' phase.
        let result = rocket.burn_sequential(&design, first_phase_dv + 200.0, "leo");
        assert!((result.dv_achieved - (first_phase_dv + 200.0)).abs() < 1.0);
        assert!(!rocket.stage_states[0][1].attached && !rocket.stage_states[0][2].attached,
            "boosters jettisoned once empty");
        assert!(rocket.stage_states[0][0].attached
            && rocket.stage_states[0][0].propellant_remaining_kg > 0.0,
            "core keeps flying");
        assert_eq!(result.stages_jettisoned, vec![(0, 1), (0, 2)]);
        assert!(result.groups_jettisoned.is_empty(), "the group as a whole is not spent");
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
            power_sources: Vec::new(),
        };
        let srb = Stage {
            id: StageId(2), name: "SRB".into(),
            engine: srb_engine.clone(), engine_count: 1,
            propellant_mass_kg: 20_000.0, structural_mass_kg: 1_500.0,
            fairing: None,
            power_sources: Vec::new(),
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
            power_sources: Vec::new(),
        };
        let srb = Stage {
            id: StageId(2), name: "SRB".into(),
            engine: srb_engine, engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        let upper = Stage {
            id: StageId(3), name: "Upper".into(),
            engine: upper_engine, engine_count: 1,
            propellant_mass_kg: 15_000.0, structural_mass_kg: 800.0,
            fairing: Some(Fairing { mass_kg: 200.0, diameter_m: 4.0 }),
            power_sources: Vec::new(),
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
        let total_dv = rocket.vacuum_delta_v(payload);
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
            power_sources: Vec::new(),
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 8_000.0, structural_mass_kg: 500.0,
            fairing: None,
            power_sources: Vec::new(),
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };

        let payload = 1_000.0;
        let rocket = design.instantiate(RocketId(1), payload);

        // Fresh rocket should have same delta-v as design
        let design_dv = design.vacuum_delta_v(payload);
        let instance_dv = rocket.remaining_delta_v(&design);
        assert!(
            (design_dv - instance_dv).abs() < 1.0,
            "design_dv={}, instance_dv={}", design_dv, instance_dv
        );
    }

    #[test]
    fn test_jettison_stage() {
        let engine = kerolox_engine(1, 500_000.0, 250.0, 300.0);
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 8_000.0, structural_mass_kg: 500.0,
            fairing: None,
            power_sources: Vec::new(),
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };

        let mut rocket = design.instantiate(RocketId(1), 1_000.0);

        assert!(rocket.jettison_stage(0, 0));
        assert!(!rocket.stage_states[0][0].attached);
        assert_eq!(rocket.stage_states[0][0].propellant_remaining_kg, 0.0);

        // Can't jettison again
        assert!(!rocket.jettison_stage(0, 0));
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
            power_draw_w: 0.0,
        };
        let lander_engine = kerolox_engine(11, 50_000.0, 100.0, 320.0);

        let ion_stage = Stage {
            id: StageId(10), name: "Ion".into(),
            engine: ion_engine, engine_count: 1,
            propellant_mass_kg: 200.0, structural_mass_kg: 100.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        let lander_stage = Stage {
            id: StageId(11), name: "Lander".into(),
            engine: lander_engine, engine_count: 1,
            propellant_mass_kg: 5_000.0, structural_mass_kg: 500.0,
            fairing: None,
            power_sources: Vec::new(),
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "IonLander".into(),
            stage_groups: vec![vec![ion_stage, lander_stage]],
        };

        assert!(design.validate().is_empty());
        let dv = design.vacuum_delta_v(500.0);
        assert!(dv > 0.0, "Should have positive delta-v");
    }

    // ==========================================
    // Stage stats tests
    // ==========================================

    #[test]
    fn test_burn_sequential_single_group() {
        let engine = kerolox_engine(1, 500_000.0, 250.0, 300.0);
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: engine.clone(), engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
            power_sources: Vec::new(),
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1]],
        };

        let mut rocket = design.instantiate(RocketId(1), 1_000.0);
        let initial_dv = rocket.remaining_delta_v(&design);

        let result = rocket.burn_sequential(&design, 1_000.0, "leo");
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
            power_sources: Vec::new(),
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: engine2, engine_count: 1,
            propellant_mass_kg: 10_000.0, structural_mass_kg: 500.0,
            fairing: None,
            power_sources: Vec::new(),
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "TwoStager".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };

        let mut rocket = design.instantiate(RocketId(1), 1_000.0);
        let total_dv = rocket.remaining_delta_v(&design);

        // Burn for more than the first stage can provide — should cross into second stage
        let s1_dv = rocket.group_remaining_delta_v(&design, 0);
        let target = s1_dv + 500.0; // need some from S2

        let result = rocket.burn_sequential(&design, target, "leo");
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
            power_sources: Vec::new(),
        };

        let design = RocketDesign {
            id: RocketDesignId(1),
            name: "Test".into(),
            stage_groups: vec![vec![s1]],
        };

        let mut rocket = design.instantiate(RocketId(1), 1_000.0);
        let total_dv = rocket.remaining_delta_v(&design);

        // Ask for way more than available
        let result = rocket.burn_sequential(&design, total_dv + 5_000.0, "leo");
        assert!((result.dv_achieved - total_dv).abs() < 50.0,
            "Should only burn total available dv={}, got {}", total_dv, result.dv_achieved);
    }
}
