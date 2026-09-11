//! The planner's view of a design: per-group vacuum figures and the
//! one ascent budget (`DesignPerformance`), read by the path planner,
//! the designer's stats table and the launch check.

use crate::location::{self, DELTA_V_MAP};

use super::RocketDesign;

/// One stage group's delta-v budget for a specific mission start.
///
/// The vacuum figure is Tsiolkovsky; the three losses are what the
/// group pays if it burns during an ascent from a surface (see
/// [`AscentLosses`]). Which of them a reader subtracts depends on what
/// it is asking — `planner_dv` for "can this trip be flown",
/// `effective_dv` for the designer's stats table.
#[derive(Debug, Clone)]
pub struct GroupPerformance {
    /// (wet + payload_above) / (dry + payload_above); 1.0 for a sail.
    pub mass_ratio: f64,
    /// Tsiolkovsky delta-v, vacuum, no losses. A solar sail has no
    /// propellant budget and reports 0 here; see `is_sail`.
    pub delta_v_vacuum: f64,
    /// Gravity loss from the ascent integration (m/s).
    pub gravity_loss: f64,
    /// Atmospheric drag loss (first group only, m/s).
    pub aero_drag_loss: f64,
    /// Sea-level Isp penalty (first group in atmosphere, m/s).
    pub overexpansion_loss: f64,
    /// Thrust-to-weight ratio at ignition.
    pub twr: f64,
    /// Burn time in seconds.
    pub burn_time_s: f64,
    /// Solar sail: unbounded delta-v, no propellant. Displays show ∞.
    pub is_sail: bool,
}

impl GroupPerformance {
    /// What the planner may spend from this group: vacuum less the
    /// gravity it pays climbing out and the sea-level Isp penalty its
    /// first burn eats (the flight charges both, in `burn_group` and
    /// the ascent integration). Drag is charged on the ascent edge
    /// against the mass actually flown, so it is not taken here.
    pub fn planner_dv(&self) -> f64 {
        (self.delta_v_vacuum - self.gravity_loss - self.overexpansion_loss).max(0.0)
    }

    /// What the stats table shows: the planner's figure less drag, so
    /// everything is charged to the group.
    pub fn effective_dv(&self) -> f64 {
        if self.is_sail {
            return f64::INFINITY;
        }
        (self.planner_dv() - self.aero_drag_loss).max(0.0)
    }

    /// The vacuum figure as displayed: ∞ for a sail.
    pub fn display_vacuum_dv(&self) -> f64 {
        if self.is_sail { f64::INFINITY } else { self.delta_v_vacuum }
    }
}

/// The designer's stats table row; the same thing.
pub type StageGroupStats = GroupPerformance;

/// What leaving a surface costs. A journey has at most one such leg
/// (the graph invariant `only_surface_ascents_are_atmospheric` pins
/// this); everything after it is a vacuum transfer. A departure from
/// orbit has an empty budget.
#[derive(Debug, Clone, Default)]
pub struct AscentLosses {
    /// Gravity loss charged to each group that burns during the
    /// ascent — group 0, and any later group that ignites before
    /// orbital velocity.
    pub gravity_by_group: Vec<f64>,
    /// Sea-level Isp penalty across the groups (m/s); zero without an
    /// atmosphere.
    pub overexpansion: f64,
    /// Each group's propellant-weighted Isp fraction over the climb
    /// (`AscentGroupResult::isp_fraction`; 1.0 past the first group, see
    /// `design_ascent`): what the flight's burn applies, so it and the
    /// planner agree.
    pub isp_fraction_by_group: Vec<f64>,
    /// Drag on the first group. For display; the planner charges it on
    /// the ascent edge.
    pub drag: f64,
}

/// A design's performance for a mission from `launch_from` carrying
/// `payload_kg`: vacuum figures per group plus the one ascent budget.
/// Computed once and read by the planner, the designer, the launch
/// simulation and the payload search, so they cannot disagree.
#[derive(Debug, Clone)]
pub struct DesignPerformance {
    pub groups: Vec<GroupPerformance>,
    pub ascent: AscentLosses,
}

impl DesignPerformance {
    pub fn compute(design: &RocketDesign, payload_kg: f64, launch_from: &str) -> Self {
        let n = design.stage_groups.len();
        if n == 0 {
            return DesignPerformance { groups: Vec::new(), ascent: AscentLosses::default() };
        }

        let surface_props = DELTA_V_MAP.surface_properties(launch_from);
        // Surface gravity (for TWR reference) — fall back to Earth so TWR
        // numbers stay readable when launching from a non-surface location.
        let surface_g = surface_props.map_or(9.81, |p| p.gravity_m_s2);
        let has_atmosphere = surface_props.is_some_and(|p| p.has_atmosphere);

        let total_mass = design.total_mass_kg() + payload_kg;
        let ascent_groups = design_ascent(design, payload_kg, launch_from);
        let first_stage_aero = if has_atmosphere {
            location::aero_drag_loss(total_mass)
        } else {
            0.0
        };

        let mut groups = Vec::with_capacity(n);
        for (gi, (group, ascent_group)) in design.stage_groups.iter().zip(&ascent_groups).enumerate() {
            let thrust: f64 = group.iter().map(|s| s.total_thrust_n()).sum();
            let gravity_loss = ascent_group.gravity_loss;

            // Mass above this group: upper groups + payload
            let payload_above: f64 = design.stage_groups[gi + 1..].iter()
                .flat_map(|g| g.iter())
                .map(|s| s.wet_mass_kg())
                .sum::<f64>()
                + payload_kg;
            let group_wet: f64 = group.iter().map(|s| s.wet_mass_kg()).sum();
            let group_dry: f64 = group.iter().map(|s| s.dry_mass_kg()).sum();

            let is_sail = group.iter().any(|s| s.engine.is_solar_sail());
            let mass_ratio = if is_sail { 1.0 } else {
                (group_wet + payload_above) / (group_dry + payload_above)
            };
            let delta_v_vacuum = design.group_delta_v(gi, payload_above);
            let twr = if (group_wet + payload_above) > 0.0 {
                thrust / ((group_wet + payload_above) * surface_g)
            } else {
                0.0
            };
            // The group's burn lasts as long as its longest tank.
            let burn_time_s = design.burn_phases(gi, payload_above).iter().map(|p| p.duration_s).sum();

            let aero_drag_loss = if gi == 0 { first_stage_aero } else { 0.0 };

            // Sea-level Isp penalty, as the ascent averaged it over the
            // group's climb (nothing for a group that burns in vacuum).
            let overexpansion_loss = delta_v_vacuum * (1.0 - ascent_group.isp_fraction);

            groups.push(GroupPerformance {
                mass_ratio,
                delta_v_vacuum,
                gravity_loss,
                aero_drag_loss,
                overexpansion_loss,
                twr,
                burn_time_s,
                is_sail,
            });
        }

        let ascent = AscentLosses {
            gravity_by_group: ascent_groups.iter().map(|g| g.gravity_loss).collect(),
            overexpansion: groups.iter().map(|g| g.overexpansion_loss).sum(),
            isp_fraction_by_group: ascent_groups.iter().map(|g| g.isp_fraction).collect(),
            drag: first_stage_aero,
        };
        DesignPerformance { groups, ascent }
    }

    /// What the planner may spend from group `gi`; 0 past the last group.
    pub fn planner_dv(&self, gi: usize) -> f64 {
        self.groups.get(gi).map_or(0.0, |g| g.planner_dv())
    }

    /// Σ vacuum delta-v (∞ with a sail aboard).
    pub fn total_vacuum_dv(&self) -> f64 {
        self.groups.iter().map(|g| g.display_vacuum_dv()).sum()
    }

    /// Σ `planner_dv` — the "available" figure the planner means.
    pub fn total_planner_dv(&self) -> f64 {
        self.groups.iter().map(|g| g.planner_dv()).sum()
    }

    /// Σ `effective_dv` — the stats table's total.
    pub fn total_effective_dv(&self) -> f64 {
        self.groups.iter().map(|g| g.effective_dv()).sum()
    }
}

/// Per-stage-group stats for the rocket designer display; the groups of
/// [`DesignPerformance::compute`].
pub fn compute_stage_stats(
    design: &RocketDesign,
    payload_kg: f64,
    launch_from: &str,
) -> Vec<StageGroupStats> {
    DesignPerformance::compute(design, payload_kg, launch_from).groups
}

/// Per-stage-group gravity loss (m/s) for a design ascending from
/// `launch_from`, carrying `payload_kg`.
///
/// This is the *only* place the ascent profile is integrated. Both the
/// designer stats table and the path planner call it, so what the player
/// reads as "Eff dV" and what the planner charges to reach orbit can't
/// drift apart. Departures from a non-surface location (an orbital depot)
/// have no vertical ascent and so no loss.
///
/// Losses accumulate across groups in firing order: a group that ignites
/// after the vehicle is already near orbital velocity is pitched over far
/// enough that its own loss is close to zero, which is what lets the same
/// vector serve a mid-mission transfer burn as well as the ascent.
pub fn group_gravity_losses(
    design: &RocketDesign,
    payload_kg: f64,
    launch_from: &str,
) -> Vec<f64> {
    design_ascent(design, payload_kg, launch_from).iter().map(|g| g.gravity_loss).collect()
}

/// The ascent integration for a design launched from `launch_from`
/// with `payload_kg` aboard: gravity loss and averaged Isp fraction per
/// stage group (see [`location::simulate_ascent`]). From anywhere but a
/// surface there is no climb: no loss, full Isp.
pub fn design_ascent(
    design: &RocketDesign,
    payload_kg: f64,
    launch_from: &str,
) -> Vec<location::AscentGroupResult> {
    let n = design.stage_groups.len();
    let Some(props) = DELTA_V_MAP.surface_properties(launch_from) else {
        return vec![location::AscentGroupResult { gravity_loss: 0.0, isp_fraction: 1.0, burnout_altitude_m: 0.0 }; n];
    };
    // Each group as its burn phases, so a booster that runs dry early
    // takes its thrust and its structure out of the integration when it
    // actually leaves.
    //
    // A low-thrust group is zeroed out rather than integrated. The delta-v
    // graph already models electric propulsion as spiralling *from orbit*
    // (`low_thrust_delta_v`), never as lifting off, and an ion stage can't
    // hold itself up anyway: the integrator would watch its velocity decay
    // to zero and then bill it for gravity across a burn lasting weeks.
    let groups: Vec<Vec<location::AscentPhase>> = (0..n)
        .map(|gi| {
            let group = &design.stage_groups[gi];
            if group.iter().any(|s| s.engine.is_low_thrust()) {
                return Vec::new();
            }
            let payload_above: f64 = design.stage_groups[gi + 1..].iter()
                .flat_map(|g| g.iter())
                .map(|s| s.wet_mass_kg())
                .sum::<f64>()
                + payload_kg;
            // Every group is charged the sea-level Isp penalty for the
            // air at the altitude it actually burns through: since D7
            // (17_3_PHYSICS.md) the integrator stages where launchers
            // stage, so an upper stage lighting at 65–95 km sees none.
            design.burn_phases(gi, payload_above).iter().map(|p| p.ascent_phase(group)).collect()
        })
        .collect();
    location::simulate_ascent(props, &groups, design.total_mass_kg() + payload_kg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rocket::*;
    use crate::stage::*;
    use crate::rocket::test_fixtures::*;

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
            power_sources: Vec::new(),
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: engine2, engine_count: 1,
            propellant_mass_kg: 15_000.0, structural_mass_kg: 500.0,
            fairing: None,
            power_sources: Vec::new(),
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
        assert!(stats[0].effective_dv() < stats[0].delta_v_vacuum,
            "S1 effective dv should be less than vacuum");
        assert!(stats[0].twr > 0.0, "S1 should have positive TWR");
        assert!(stats[0].mass_ratio > 1.0, "S1 mass ratio should be > 1");

        // Second stage should have no aero loss
        assert_eq!(stats[1].aero_drag_loss, 0.0, "S2 should have no aero loss");
        // Both stages have gravity losses, but effective dv should be less than vacuum for both
        assert!(stats[1].effective_dv() <= stats[1].delta_v_vacuum,
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
            power_sources: Vec::new(),
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
            power_sources: Vec::new(),
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
            engine, engine_count: 1,
            propellant_mass_kg: 30_000.0, structural_mass_kg: 2_000.0,
            fairing: None,
            power_sources: Vec::new(),
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
}
