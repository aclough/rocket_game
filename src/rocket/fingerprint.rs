//! The design's identity for the route memos: a hash of every field
//! the planner reads.

use super::RocketDesign;

impl RocketDesign {
    /// A cheap identity for "this exact vehicle", for memoizing route
    /// planning against.
    ///
    /// Keyed on every field the planner reads rather than on a project id
    /// and revision, for two reasons: the rocket designer edits a design
    /// that belongs to no project yet, and a design *modification* changes
    /// the stage groups without bumping the revision — so the old key
    /// could go stale on a vehicle that had genuinely changed.
    ///
    /// Names and propellant mixes are left out on purpose: they don't move
    /// any number the planner uses (mix reaches it through `isp_s`), and
    /// leaving them out means renaming a stage doesn't throw the cache
    /// away.
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for group in &self.stage_groups {
            group.len().hash(&mut h);
            for stage in group {
                let e = &stage.engine;
                e.id.0.hash(&mut h);
                // The planner classes edges by `is_low_thrust`, which
                // reads the cycle; a degraded clone keeps its id, so the
                // id alone would not tell two cycles apart.
                e.cycle.hash(&mut h);
                e.thrust_n.to_bits().hash(&mut h);
                e.mass_kg.to_bits().hash(&mut h);
                e.isp_s.to_bits().hash(&mut h);
                // The propulsion kind and its data: which bell, or the
                // electrical draw the flight derates by.
                match e.propulsion {
                    crate::engine::Propulsion::Nozzle { chamber_pressure_pa, expansion_ratio, gamma, sea_level } => {
                        0u8.hash(&mut h);
                        chamber_pressure_pa.to_bits().hash(&mut h);
                        expansion_ratio.to_bits().hash(&mut h);
                        gamma.to_bits().hash(&mut h);
                        sea_level.hash(&mut h);
                    }
                    crate::engine::Propulsion::Electric { power_draw_w } => {
                        1u8.hash(&mut h);
                        power_draw_w.to_bits().hash(&mut h);
                    }
                    crate::engine::Propulsion::Sail => 2u8.hash(&mut h),
                }
                stage.engine_count.hash(&mut h);
                stage.propellant_mass_kg.to_bits().hash(&mut h);
                stage.structural_mass_kg.to_bits().hash(&mut h);
                stage.fairing.as_ref().map(|f| f.mass_kg.to_bits()).hash(&mut h);
                // The effective rack, so the default battery a bare stage
                // carries is part of the identity like any other kit.
                for src in stage.effective_power_sources().iter() {
                    src.mass_kg.to_bits().hash(&mut h);
                    src.capacity_kwd.to_bits().hash(&mut h);
                    match &src.kind {
                        crate::power::PowerSourceKind::Battery => 0u8.hash(&mut h),
                        crate::power::PowerSourceKind::SolarPanel { peak_w_at_1au } => {
                            1u8.hash(&mut h);
                            peak_w_at_1au.to_bits().hash(&mut h);
                        }
                        crate::power::PowerSourceKind::Rtg { steady_w } => {
                            2u8.hash(&mut h);
                            steady_w.to_bits().hash(&mut h);
                        }
                        crate::power::PowerSourceKind::FuelCell { peak_w, kg_per_kwd } => {
                            3u8.hash(&mut h);
                            peak_w.to_bits().hash(&mut h);
                            kg_per_kwd.to_bits().hash(&mut h);
                        }
                        crate::power::PowerSourceKind::Reactor { design } => {
                            4u8.hash(&mut h);
                            design.steady_w.to_bits().hash(&mut h);
                        }
                    }
                }
            }
        }
        h.finish()
    }
}
