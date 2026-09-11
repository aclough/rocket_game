//! The electrical model: what a design's panels, RTGs and fuel cells
//! supply, what its housekeeping draws, how long its batteries last,
//! and the daily power tick a flying rocket runs.

use crate::stage::Stage;

use super::{Rocket, RocketDesign};

/// Effective steady-state output of one power source on a given stage.
/// Same as `PowerSource::steady_output_w` except fuel cells return 0 if
/// the host stage's engine has propellant the cell can't burn (solid
/// or xenon).
/// The power totals over a set of stages — the design's whole stack, or
/// a flying rocket's attached stages — accumulated source by source in
/// stage order, which is how both used to do it (17_3_PHYSICS.md D2).
fn total_supply_w<'a>(stages: impl Iterator<Item = &'a Stage>, sun_distance_au: f64) -> f64 {
    let mut total = 0.0;
    for stage in stages {
        for src in stage.effective_power_sources().iter() {
            total += stage.source_supply_w(src, sun_distance_au);
        }
    }
    total
}

fn total_housekeeping_w<'a>(stages: impl Iterator<Item = &'a Stage>) -> f64 {
    let mut total = 0.0;
    for stage in stages {
        total += stage.housekeeping_w();
    }
    total
}

fn total_battery_kwd<'a>(stages: impl Iterator<Item = &'a Stage>) -> f64 {
    let mut total = 0.0;
    for stage in stages {
        for src in stage.effective_power_sources().iter() {
            if let crate::power::PowerSourceKind::Battery = src.kind {
                total += src.capacity_kwd;
            }
        }
    }
    total
}

/// Steady supply from solar / RTG / reactor (excludes fuel cells, which
/// consume propellant and are the daily tick's fallback).
fn free_supply_w<'a>(stages: impl Iterator<Item = &'a Stage>, sun_distance_au: f64) -> f64 {
    let mut total = 0.0;
    for stage in stages {
        for src in stage.effective_power_sources().iter() {
            match src.kind {
                crate::power::PowerSourceKind::SolarPanel { .. }
                | crate::power::PowerSourceKind::Rtg { .. }
                | crate::power::PowerSourceKind::Reactor { .. } => {
                    total += src.steady_output_w(sun_distance_au);
                }
                _ => {}
            }
        }
    }
    total
}

impl RocketDesign {
    /// Total electrical power supply (watts) at the given heliocentric
    /// distance. Sums steady output of every power source on every stage
    /// — assumes all stages full / attached / charged. Fuel cells only
    /// count if their host stage's engine uses propellants the cell can
    /// burn (no solid, no xenon).
    pub fn total_power_supply_w(&self, sun_distance_au: f64) -> f64 {
        total_supply_w(self.stage_groups.iter().flatten(), sun_distance_au)
    }

    /// Total housekeeping demand (watts) across all stages.
    pub fn total_housekeeping_w(&self) -> f64 {
        total_housekeeping_w(self.stage_groups.iter().flatten())
    }

    /// Total battery capacity (kilowatt-days) across all stages, counting
    /// the default battery on any stage the player left bare.
    pub fn total_battery_kwd(&self) -> f64 {
        total_battery_kwd(self.stage_groups.iter().flatten())
    }

    /// How many days this design can run its own housekeeping at
    /// `sun_distance_au` before the batteries are flat. `INFINITY` when
    /// steady supply covers demand — panels, RTGs and reactors don't run
    /// down.
    ///
    /// Counts every stage, i.e. the design as built before any staging.
    /// This is the figure the designer quotes and the one contract
    /// readiness checks a route against; a craft partway through a flight
    /// has already dropped stages, so ask its `Rocket` instead.
    pub fn endurance_days(&self, sun_distance_au: f64) -> f64 {
        let deficit_w = self.total_housekeeping_w()
            - self.total_power_supply_w(sun_distance_au);
        if deficit_w <= 0.0 {
            return f64::INFINITY;
        }
        self.total_battery_kwd() / (deficit_w / 1000.0)
    }

    /// Power available for engines (after housekeeping is subtracted) at
    /// the given heliocentric distance. Negative supply clamps to zero.
    pub fn power_for_engines_w(&self, sun_distance_au: f64) -> f64 {
        (self.total_power_supply_w(sun_distance_au) - self.total_housekeeping_w()).max(0.0)
    }

    /// Effective combined thrust of a group, derated by available
    /// electrical power. Self-powered engines (power_draw_w == 0) always
    /// produce nominal thrust; electric engines scale down by
    /// `min(1, available / required)` and consume their share of the
    /// available pool. Power is allocated to stages in their order
    /// within the group.
    pub fn group_effective_thrust_n(&self, group_index: usize, available_power_w: f64) -> f64 {
        let group = match self.stage_groups.get(group_index) {
            Some(g) => g,
            None => return 0.0,
        };
        let mut total = 0.0;
        let mut remaining = available_power_w;
        for stage in group {
            let nominal = stage.total_thrust_n();
            let required = stage.engine.power_draw_w * stage.engine_count as f64;
            if required <= 0.0 {
                total += nominal;
            } else if remaining <= 0.0 {
                // Out of electrical power for this stage.
            } else {
                let fraction = (remaining / required).min(1.0);
                total += nominal * fraction;
                remaining -= required * fraction;
            }
        }
        total
    }
}

impl Rocket {
    /// Sum of steady-state power output (watts) across all attached
    /// stages' power sources, evaluated at `sun_distance_au`. Fuel cells
    /// only count when their host stage's engine has compatible
    /// propellant.
    pub fn total_power_supply_w(&self, design: &RocketDesign, sun_distance_au: f64) -> f64 {
        total_supply_w(self.attached_stages(design).map(|(_, _, s, _)| s), sun_distance_au)
    }

    /// Sum of housekeeping draw (watts) across all attached stages.
    pub fn total_housekeeping_w(&self, design: &RocketDesign) -> f64 {
        total_housekeeping_w(self.attached_stages(design).map(|(_, _, s, _)| s))
    }

    /// Sum of battery capacity (kilowatt-days) across all attached stages.
    pub fn total_battery_kwd(&self, design: &RocketDesign) -> f64 {
        total_battery_kwd(self.attached_stages(design).map(|(_, _, s, _)| s))
    }

    /// Current battery charge (kilowatt-days) summed across attached stages.
    pub fn total_battery_charge_kwd(&self) -> f64 {
        self.stage_states.iter()
            .flat_map(|g| g.iter())
            .filter(|ss| ss.attached)
            .map(|ss| ss.battery_kwd_remaining)
            .sum()
    }

    /// Run one day of power balance.
    ///
    /// Priority of supply against housekeeping demand:
    ///   1. Free supply (solar / RTG / reactor) — surplus charges
    ///      batteries up to capacity.
    ///   2. If still in deficit, fuel cells fire up to cover the
    ///      remainder and consume propellant from their stage.
    ///   3. If still in deficit, batteries discharge.
    ///   4. If batteries hit zero with demand unmet, brownout (return
    ///      true).
    ///
    /// Runs on every rocket. There is no grandfathered case: a stage the
    /// player left bare still carries its default battery, so it lives
    /// about a day away from the pad rather than forever.
    pub fn run_daily_power_tick(
        &mut self, design: &RocketDesign, sun_distance_au: f64,
    ) -> bool {
        let free_supply_w = self.free_supply_w(design, sun_distance_au);
        let demand_w = self.total_housekeeping_w(design);
        let net_w = free_supply_w - demand_w;
        if net_w >= 0.0 {
            // Surplus: recharge batteries proportionally.
            self.distribute_charge_kwd(design, net_w / 1000.0);
            return false;
        }
        // Deficit. Run fuel cells (consume propellant) before batteries.
        let deficit_w = -net_w;
        let fuel_cell_produced_w = self.run_fuel_cells_for_w(design, deficit_w);
        let remaining_w = (deficit_w - fuel_cell_produced_w).max(0.0);
        if remaining_w <= 1e-6 {
            return false;
        }
        let deficit_kwd = remaining_w / 1000.0;
        let drained = self.drain_battery_kwd(deficit_kwd);
        drained < deficit_kwd - 1e-9
    }

    /// Steady supply from the attached stages' solar / RTG / reactor.
    fn free_supply_w(&self, design: &RocketDesign, sun_distance_au: f64) -> f64 {
        free_supply_w(self.attached_stages(design).map(|(_, _, s, _)| s), sun_distance_au)
    }

    /// Fire fuel cells to cover up to `required_w` of demand. Each cell
    /// produces up to its rated peak_w and consumes propellant from its
    /// own stage at `kg_per_kwd` kg per kilowatt-day of output. If the
    /// stage runs out of propellant, the cell's output is reduced
    /// proportionally. Returns the actual total watts produced.
    fn run_fuel_cells_for_w(
        &mut self, design: &RocketDesign, required_w: f64,
    ) -> f64 {
        if required_w <= 0.0 {
            return 0.0;
        }
        let mut produced_w = 0.0;
        let mut remaining_w = required_w;
        // Indices first: the cells drain their own stage's propellant.
        let attached: Vec<(usize, usize)> = self.attached_stages(design)
            .map(|(gi, si, _, _)| (gi, si))
            .collect();
        for (gi, si) in attached {
            let stage = &design.stage_groups[gi][si];
            if !crate::power::fuel_cell_can_run_on(&stage.engine) {
                continue;
            }
            for src in stage.effective_power_sources().iter() {
                if remaining_w <= 0.0 { return produced_w; }
                let (peak_w, kg_per_kwd) = match src.kind {
                    crate::power::PowerSourceKind::FuelCell { peak_w, kg_per_kwd }
                        => (peak_w, kg_per_kwd),
                    _ => continue,
                };
                let desired_w = peak_w.min(remaining_w);
                let desired_kwd = desired_w / 1000.0;
                let propellant_needed = desired_kwd * kg_per_kwd;
                let avail = self.stage_states[gi][si].propellant_remaining_kg;
                let consumed = propellant_needed.min(avail);
                self.stage_states[gi][si].propellant_remaining_kg -= consumed;
                let actual_kwd = if kg_per_kwd > 0.0 {
                    consumed / kg_per_kwd
                } else { desired_kwd };
                let actual_w = actual_kwd * 1000.0;
                produced_w += actual_w;
                remaining_w -= actual_w;
            }
        }
        produced_w
    }

    /// Distribute `kwd` of charge across attached batteries, respecting
    /// capacity. Helper for `run_daily_power_tick`.
    fn distribute_charge_kwd(&mut self, design: &RocketDesign, mut kwd: f64) {
        // Each attached stage's battery room, lowest group first; indices
        // first because the charge is written back into the states.
        let attached: Vec<(usize, usize)> = self.attached_stages(design)
            .map(|(gi, si, _, _)| (gi, si))
            .collect();
        for (gi, si) in attached {
            let stage_capacity = design.stage_groups[gi][si].battery_capacity_kwd();
            if stage_capacity <= 0.0 { continue; }
            let state = &mut self.stage_states[gi][si];
            let room = stage_capacity - state.battery_kwd_remaining;
            let add = kwd.min(room).max(0.0);
            state.battery_kwd_remaining += add;
            kwd -= add;
            if kwd <= 0.0 { return; }
        }
    }

    /// Drain `kwd` from attached batteries proportionally. Returns the
    /// amount actually drained (== requested unless batteries hit zero).
    fn drain_battery_kwd(&mut self, kwd: f64) -> f64 {
        let total_charge: f64 = self.stage_states.iter()
            .flat_map(|g| g.iter())
            .filter(|ss| ss.attached)
            .map(|ss| ss.battery_kwd_remaining)
            .sum();
        if total_charge <= 0.0 { return 0.0; }
        let drain = kwd.min(total_charge);
        let frac = drain / total_charge;
        for group in &mut self.stage_states {
            for ss in group.iter_mut() {
                if ss.attached {
                    ss.battery_kwd_remaining *= 1.0 - frac;
                }
            }
        }
        drain
    }
}

#[cfg(test)]
mod tests {
    use crate::rocket::*;
    use crate::engine::*;
    use crate::propellant::Propellant;
    use crate::stage::*;
    use crate::rocket::test_fixtures::*;

    fn powered_design(panel_w: f64, battery_kwd: f64) -> RocketDesign {
        use crate::power::PowerSource;
        let mut s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: kerolox_engine(1, 1_000_000.0, 500.0, 280.0),
            engine_count: 1,
            propellant_mass_kg: 50_000.0, structural_mass_kg: 3_000.0,
            fairing: None, power_sources: Vec::new(),
        };
        if panel_w > 0.0 {
            s1.power_sources.push(PowerSource::new_solar_panel(panel_w));
        }
        if battery_kwd > 0.0 {
            s1.power_sources.push(PowerSource::new_battery(battery_kwd));
        }
        RocketDesign {
            id: RocketDesignId(1), name: "Powered".into(),
            stage_groups: vec![vec![s1]],
        }
    }

    #[test]
    fn endurance_reports_how_long_a_design_can_keep_its_own_lights_on() {
        // Nothing fitted: the default battery, sized to exactly one day of
        // the stage's own housekeeping. This is the figure that decides
        // whether a design can survive a route.
        let bare = powered_design(0.0, 0.0);
        let days = bare.endurance_days(1.0);
        assert!(
            (days - crate::power::DEFAULT_BATTERY_DAYS).abs() < 1e-9,
            "a bare design should last exactly the default reserve, got {days}",
        );

        // A panel that covers housekeeping never runs down.
        let demand = bare.total_housekeeping_w();
        let solar = powered_design(demand * 2.0, 1.0);
        assert!(
            solar.endurance_days(1.0).is_infinite(),
            "steady supply above demand means the batteries never drain",
        );

        // The same panel out at 10 AU collects 1% of the light, so the
        // battery is back to setting the clock.
        let far = solar.endurance_days(10.0);
        assert!(far.is_finite() && far > 0.0, "expected a finite reserve, got {far}");
    }

    #[test]
    fn a_stage_with_no_power_sources_still_carries_its_default_battery() {
        // A bare stage used to be exempt from the power system entirely,
        // which made it immortal anywhere in the solar system. It now flies
        // the default battery: no supply, one day of reserve, then dark.
        let design = powered_design(0.0, 0.0);
        let stage = &design.stage_groups[0][0];
        assert!(stage.power_sources.is_empty(), "fixture premise: nothing fitted");

        let sources = stage.effective_power_sources();
        assert_eq!(sources.len(), 1, "a bare stage still has power kit");
        assert!(matches!(sources[0].kind, crate::power::PowerSourceKind::Battery));
        assert!(sources[0].mass_kg > 0.0, "the default battery has mass");
        assert!(
            design.total_mass_kg() > stage.structural_mass_kg,
            "and that mass reaches the design's totals",
        );

        let mut rocket = design.instantiate(RocketId(1), "leo", 1000.0);
        assert!(
            !rocket.run_daily_power_tick(&design, 1.0),
            "the first day runs on the reserve",
        );
        assert!(
            rocket.run_daily_power_tick(&design, 1.0),
            "with nothing generating, the reserve is gone and the craft browns out",
        );
    }

    #[test]
    fn solar_panel_keeps_battery_topped_up_at_earth() {
        // A panel sized comfortably above housekeeping recharges battery.
        let design = powered_design(2000.0, 1.0);
        let mut rocket = design.instantiate(RocketId(1), "leo", 0.0);
        // Drain the battery a bit, then tick.
        rocket.stage_states[0][0].battery_kwd_remaining = 0.5;
        let supply = rocket.total_power_supply_w(&design, 1.0);
        let demand = rocket.total_housekeeping_w(&design);
        assert!(supply > demand, "expected supply {} > demand {}", supply, demand);
        let brownout = rocket.run_daily_power_tick(&design, 1.0);
        assert!(!brownout);
        // Battery should have charged.
        assert!(rocket.stage_states[0][0].battery_kwd_remaining > 0.5);
    }

    #[test]
    fn battery_drains_when_far_from_sun_and_browns_out() {
        // Solar panel sized for Earth's orbit, but flight is at 5 AU
        // (Jupiter-ish). Panel output ≪ housekeeping → battery drains
        // each day; eventually empty → brownout.
        let design = powered_design(1000.0, 1.0);
        let mut rocket = design.instantiate(RocketId(1), "leo", 0.0);
        let mut browned_out = false;
        for _ in 0..100 {
            if rocket.run_daily_power_tick(&design, 5.0) {
                browned_out = true;
                break;
            }
        }
        assert!(browned_out, "expected brownout at 5 AU within 100 days");
        assert!(rocket.total_battery_charge_kwd() < 1e-6,
            "battery should be drained after brownout");
    }

    #[test]
    fn rtg_only_design_never_browns_out_if_steady_supply_covers_demand() {
        // RTG supplying more than housekeeping: never browns out, even
        // far from the sun.
        use crate::power::{PowerSource, RtgClass};
        let mut s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: kerolox_engine(1, 1_000_000.0, 500.0, 280.0),
            engine_count: 1,
            propellant_mass_kg: 50_000.0,
            structural_mass_kg: 100.0, // tiny bus, low housekeeping
            fairing: None,
            power_sources: vec![PowerSource::new_rtg(RtgClass::Cassini)],
        };
        // small battery for bookkeeping
        s1.power_sources.push(PowerSource::new_battery(0.5));
        let design = RocketDesign {
            id: RocketDesignId(1), name: "Probe".into(),
            stage_groups: vec![vec![s1]],
        };
        let mut rocket = design.instantiate(RocketId(1), "earth_surface", 0.0);
        for _ in 0..1000 {
            assert!(!rocket.run_daily_power_tick(&design, 30.0)); // way out
        }
    }

    fn ion_engine_design(thrust_n: f64, power_draw_w: f64) -> EngineDesign {
        EngineDesign {
            id: EngineId(1), name: "Ion".into(),
            cycle: EngineCycle::ElectricPropulsion,
            thrust_n, mass_kg: 35.0, isp_s: 3000.0,
            exit_pressure_pa: 0.0, needs_atmosphere: false,
            propellant_mix: vec![PropellantFraction {
                propellant: Propellant::Xenon, mass_fraction: 1.0,
            }],
            power_draw_w,
        }
    }

    fn ion_stage_design(thrust_n: f64, power_draw_w: f64, panel_w: f64) -> RocketDesign {
        use crate::power::PowerSource;
        let mut stage = Stage {
            id: StageId(1), name: "S1".into(),
            engine: ion_engine_design(thrust_n, power_draw_w),
            engine_count: 1,
            propellant_mass_kg: 1_000.0, structural_mass_kg: 100.0,
            fairing: None, power_sources: Vec::new(),
        };
        if panel_w > 0.0 {
            stage.power_sources.push(PowerSource::new_solar_panel(panel_w));
        }
        RocketDesign {
            id: RocketDesignId(1), name: "Ion".into(),
            stage_groups: vec![vec![stage]],
        }
    }

    #[test]
    fn chemical_engine_thrust_unchanged_by_power() {
        let design = powered_design(0.0, 0.0); // no panels at all
        // Chemical engine: power_draw_w = 0, so derate is a no-op.
        let nominal = design.group_thrust_n(0);
        let effective = design.group_effective_thrust_n(0, 0.0);
        assert!((nominal - effective).abs() < 1e-6,
            "chemical thrust should not depend on power");
    }

    #[test]
    fn ion_engine_full_thrust_with_ample_power() {
        // 10 N ion engine drawing 300 kW; provide a 500 kW panel at 1 AU.
        let design = ion_stage_design(10.0, 300_000.0, 500_000.0);
        let avail = design.power_for_engines_w(1.0);
        let effective = design.group_effective_thrust_n(0, avail);
        let nominal = design.group_thrust_n(0);
        assert!((effective - nominal).abs() < 1e-6, "expected full thrust");
    }

    #[test]
    fn ion_engine_half_thrust_with_half_power() {
        // 10 N ion at 300 kW. Provide a panel that delivers half the
        // need after housekeeping. Housekeeping ≈ structural * 0.1 W/kg
        // = 100 kg * 0.1 = 10 W (negligible vs. 300 kW). So a 150 kW
        // panel covers half the engine's draw → ~half thrust.
        let design = ion_stage_design(10.0, 300_000.0, 150_000.0);
        let avail = design.power_for_engines_w(1.0);
        let effective = design.group_effective_thrust_n(0, avail);
        let nominal = design.group_thrust_n(0);
        assert!(effective < nominal * 0.6 && effective > nominal * 0.4,
            "expected ~half thrust, got {} of nominal {}", effective, nominal);
    }

    #[test]
    fn ion_engine_zero_thrust_with_no_panel() {
        // No power source at all → zero thrust.
        let design = ion_stage_design(10.0, 300_000.0, 0.0);
        let avail = design.power_for_engines_w(1.0);
        assert_eq!(avail, 0.0);
        let effective = design.group_effective_thrust_n(0, avail);
        assert_eq!(effective, 0.0);
    }

    fn hydrolox_engine() -> EngineDesign {
        EngineDesign {
            id: EngineId(1), name: "RL-10-like".into(),
            cycle: EngineCycle::Expander,
            thrust_n: 100_000.0, mass_kg: 170.0, isp_s: 450.0,
            exit_pressure_pa: 5_000.0, needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.833 },
                PropellantFraction { propellant: Propellant::LH2, mass_fraction: 0.167 },
            ],
            power_draw_w: 0.0,
        }
    }

    fn fuel_celled_hydrolox_stage(fuel_cell_w: f64, prop_kg: f64) -> RocketDesign {
        use crate::power::PowerSource;
        let stage = Stage {
            id: StageId(1), name: "S1".into(),
            engine: hydrolox_engine(),
            engine_count: 1,
            propellant_mass_kg: prop_kg,
            structural_mass_kg: 500.0,
            fairing: None,
            power_sources: vec![PowerSource::new_fuel_cell(fuel_cell_w)],
        };
        RocketDesign {
            id: RocketDesignId(1), name: "HydroloxCell".into(),
            stage_groups: vec![vec![stage]],
        }
    }

    #[test]
    fn fuel_cell_covers_deficit_and_burns_propellant() {
        // Hydrolox stage with a 1 kW fuel cell, plenty of propellant.
        // No solar/RTG → free supply is 0 → fuel cell must cover the
        // housekeeping demand by burning propellant.
        let design = fuel_celled_hydrolox_stage(1_000.0, 5_000.0);
        let mut rocket = design.instantiate(RocketId(1), "earth_surface", 0.0);
        let prop_before = rocket.stage_states[0][0].propellant_remaining_kg;
        let brownout = rocket.run_daily_power_tick(&design, 1.0);
        assert!(!brownout, "fuel cell should cover housekeeping");
        let prop_after = rocket.stage_states[0][0].propellant_remaining_kg;
        assert!(prop_after < prop_before,
            "fuel cell should consume propellant ({} -> {})",
            prop_before, prop_after);
    }

    #[test]
    fn fuel_cell_on_xenon_stage_does_nothing() {
        // Put a fuel cell on an ion stage. Engine burns xenon — no
        // hydrocarbon for the cell. Cell produces nothing → battery
        // would have to cover (none here) → brownout.
        use crate::power::PowerSource;
        let stage = Stage {
            id: StageId(1), name: "S1".into(),
            engine: ion_engine_design(5.0, 150_000.0),
            engine_count: 1,
            propellant_mass_kg: 1_000.0,
            structural_mass_kg: 200.0,
            fairing: None,
            power_sources: vec![PowerSource::new_fuel_cell(1_000.0)],
        };
        let design = RocketDesign {
            id: RocketDesignId(1), name: "IonCell".into(),
            stage_groups: vec![vec![stage]],
        };
        let mut rocket = design.instantiate(RocketId(1), "earth_surface", 0.0);
        let prop_before = rocket.stage_states[0][0].propellant_remaining_kg;
        let brownout = rocket.run_daily_power_tick(&design, 1.0);
        assert!(brownout, "fuel cell shouldn't run on xenon → brownout");
        let prop_after = rocket.stage_states[0][0].propellant_remaining_kg;
        assert_eq!(prop_before, prop_after,
            "fuel cell on xenon should not consume propellant");
    }

    #[test]
    fn fuel_cell_with_empty_propellant_browns_out() {
        // Start with the stage's propellant near zero. Cell can't run.
        let design = fuel_celled_hydrolox_stage(1_000.0, 0.001);
        let mut rocket = design.instantiate(RocketId(1), "earth_surface", 0.0);
        let brownout = rocket.run_daily_power_tick(&design, 1.0);
        assert!(brownout, "no propellant → fuel cell idle → brownout");
    }

    #[test]
    fn ion_engine_thrust_falls_off_with_distance() {
        // Same 500 kW panel, 300 kW engine. At 1 AU full thrust; at 3 AU
        // panel delivers 500/9 ≈ 55 kW, well below 300 kW → strongly
        // derated thrust.
        let design = ion_stage_design(10.0, 300_000.0, 500_000.0);
        let avail_1au = design.power_for_engines_w(1.0);
        let avail_3au = design.power_for_engines_w(3.0);
        let t_1au = design.group_effective_thrust_n(0, avail_1au);
        let t_3au = design.group_effective_thrust_n(0, avail_3au);
        let nominal = design.group_thrust_n(0);
        assert!((t_1au - nominal).abs() < 1e-6,
            "1 AU should be full thrust, got {} of nominal {}", t_1au, nominal);
        assert!(t_3au < nominal * 0.3,
            "3 AU should be heavily derated, got {} of nominal {}", t_3au, nominal);
    }
}
