//! Rocket designs and flying rockets. The design is the vehicle as
//! drawn (stage groups of `Stage`s); the `Rocket` is one built and
//! flying, with propellant and battery state per stage. Split by
//! concern (17_REFACTOR.md E3): `staging` burns and phases, `stats`
//! the planner's performance view, `power` the electrical model,
//! `fingerprint` the memo key.

mod fingerprint;
mod power;
mod staging;
mod stats;

pub use staging::{burn_phases, BurnPhase};
pub use stats::{
    compute_stage_stats, design_ascent, group_gravity_losses, AscentLosses, DesignPerformance,
    GroupPerformance, StageGroupStats,
};

use serde::{Serialize, Deserialize};

use crate::stage::Stage;

use staging::phased_parallel_delta_v;

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
    /// Energy stored in this stage's batteries, in kilowatt-days. Sum
    /// across all `Battery`-kind power sources on the stage; the per-day
    /// power tick drains or recharges this from the supply/demand
    /// balance. Default 0.0 for legacy saves.
    #[serde(default)]
    pub battery_kwd_remaining: f64,
}

/// Result of a sequential burn operation.
#[derive(Debug, Clone)]
pub struct BurnResult {
    pub dv_achieved: f64,
    /// Groups that consumed any propellant during this burn (includes jettisoned).
    pub groups_burned: Vec<usize>,
    /// Groups that were fully exhausted and jettisoned.
    pub groups_jettisoned: Vec<usize>,
    /// Stages that ran dry and were dropped, as (group, stage) — a
    /// booster can leave long before its group is exhausted.
    pub stages_jettisoned: Vec<(usize, usize)>,
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

impl Rocket {
    /// Every stage still on the vehicle, lowest group first, each with
    /// the design it was built to and its state — the one walk behind
    /// every mass, power and flaw question about a flying rocket
    /// (17_3_PHYSICS.md D1). A stage the design has but the instance
    /// lacks a state for (a mismatched save) is not on the vehicle.
    pub fn attached_stages<'a>(
        &'a self, design: &'a RocketDesign,
    ) -> impl Iterator<Item = (usize, usize, &'a Stage, &'a StageState)> + 'a {
        design.stage_groups.iter().enumerate().flat_map(move |(gi, group)| {
            group.iter().enumerate().filter_map(move |(si, stage)| {
                self.stage_states.get(gi).and_then(|g| g.get(si))
                    .filter(|ss| ss.attached)
                    .map(|ss| (gi, si, stage, ss))
            })
        })
    }

    /// The attached stages of group `gi`, with their states.
    pub fn attached_stages_in<'a>(
        &'a self, design: &'a RocketDesign, gi: usize,
    ) -> impl Iterator<Item = (usize, &'a Stage, &'a StageState)> + 'a {
        design.stage_groups.get(gi).into_iter().flat_map(|g| g.iter()).enumerate()
            .filter_map(move |(si, stage)| {
                self.stage_states.get(gi).and_then(|g| g.get(si))
                    .filter(|ss| ss.attached)
                    .map(|ss| (si, stage, ss))
            })
    }

    /// Dry mass plus remaining propellant of everything still attached,
    /// without the payload.
    pub fn attached_mass_kg(&self, design: &RocketDesign) -> f64 {
        self.attached_stages(design)
            .map(|(_, _, stage, ss)| stage.dry_mass_kg() + ss.propellant_remaining_kg)
            .sum()
    }

    /// What group `gi` has to push: the attached stages above it plus the
    /// payload. Summed group by group, as the delta-v code always has.
    pub fn mass_above_group(&self, design: &RocketDesign, gi: usize) -> f64 {
        (gi + 1..self.stage_states.len()).map(|gj| {
            self.attached_stages_in(design, gj)
                .map(|(_, stage, ss)| stage.dry_mass_kg() + ss.propellant_remaining_kg)
                .sum::<f64>()
        }).sum::<f64>() + self.payload_mass_kg
    }

    /// Whether any stage of group `gi` is still attached.
    pub fn group_attached(&self, gi: usize) -> bool {
        self.stage_states.get(gi).is_some_and(|g| g.iter().any(|ss| ss.attached))
    }

    /// Whether group `gi` still has an attached stage with propellant.
    pub fn group_has_propellant(&self, gi: usize) -> bool {
        self.stage_states.get(gi)
            .is_some_and(|g| g.iter().any(|ss| ss.attached && ss.propellant_remaining_kg > 0.0))
    }

    /// Whether group `gi` has an attached solar-sail stage: unbounded delta-v.
    pub fn group_has_attached_sail(&self, design: &RocketDesign, gi: usize) -> bool {
        self.attached_stages_in(design, gi).any(|(_, stage, _)| stage.engine.is_solar_sail())
    }

    /// The lowest group with any stage still attached.
    pub fn lowest_attached_group(&self) -> Option<usize> {
        (0..self.stage_states.len()).find(|&gi| self.group_attached(gi))
    }

    /// The lowest group that can still burn: an attached stage with
    /// propellant.
    pub fn active_group(&self) -> Option<usize> {
        (0..self.stage_states.len()).find(|&gi| self.group_has_propellant(gi))
    }
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

    /// True if any stage in this design uses a low-thrust engine. By
    /// designer rule, low-thrust designs are always single-stage, so this
    /// is equivalent to "the design's thrust class is low-thrust."
    pub fn is_low_thrust(&self) -> bool {
        self.stage_groups.iter().flatten()
            .any(|s| s.engine.is_low_thrust())
    }

    /// Total *vacuum* delta-v across all stage groups for a given
    /// payload — Tsiolkovsky with no losses. Each group's "payload" is
    /// everything above it: upper groups + actual payload. Feasibility
    /// decisions read `DesignPerformance`, which nets out the ascent
    /// losses; this is the raw figure.
    pub fn vacuum_delta_v(&self, payload_kg: f64) -> f64 {
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
                group.iter().map(|stage| {
                    // Sum battery capacity across the stage's power
                    // sources, then start fully charged.
                    let battery_capacity: f64 = stage.effective_power_sources().iter()
                        .filter_map(|p| match &p.kind {
                            crate::power::PowerSourceKind::Battery => Some(p.capacity_kwd),
                            _ => None,
                        })
                        .sum();
                    StageState {
                        propellant_remaining_kg: stage.propellant_mass_kg,
                        attached: true,
                        battery_kwd_remaining: battery_capacity,
                    }
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

#[cfg(test)]
pub(crate) mod test_fixtures {
    use crate::engine::*;
    use crate::propellant::Propellant;

    pub fn kerolox_engine(id: u64, thrust: f64, mass: f64, isp: f64) -> EngineDesign {
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
            power_draw_w: 0.0,
        }
    }

    pub fn solid_engine(id: u64, thrust: f64, mass: f64, isp: f64) -> EngineDesign {
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
            power_draw_w: 0.0,
        }
    }

    // --- Sequential staging tests ---
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::*;
    use crate::rocket::test_fixtures::*;

    #[test]
    fn test_total_mass() {
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

        // wet = structural(2000) + engine(250) + prop(30000) = 32250, plus
        // the default battery the stage carries for having fitted no power.
        let battery = crate::power::PowerSource::default_battery_for_stage(
            &design.stage_groups[0][0],
        ).mass_kg;
        assert_eq!(design.total_mass_kg(), 32_250.0 + battery);
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
}
