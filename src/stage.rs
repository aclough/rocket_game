use serde::{Serialize, Deserialize};

use crate::engine::EngineDesign;
use crate::power::PowerSource;

/// Unique identifier for a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StageId(pub u64);

/// A payload fairing that sits on top of a stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fairing {
    pub mass_kg: f64,
    pub diameter_m: f64,
}

/// A rocket stage: structural mass, engines, propellant, optional fairing,
/// and any power sources (batteries, panels, RTGs, etc.).
///
/// The stage holds a reference to its engine design (by clone) and the number of
/// engines of that type. It does NOT own fuel composition — that comes from the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    pub id: StageId,
    pub name: String,
    pub engine: EngineDesign,
    pub engine_count: u32,
    pub propellant_mass_kg: f64,
    pub structural_mass_kg: f64,
    pub fairing: Option<Fairing>,
    /// Power sources (batteries, solar panels, RTGs…) the player fitted to
    /// this stage. Empty is normal — and empty does not mean unpowered.
    /// Read it through [`Stage::effective_power_sources`] for anything
    /// physical; this field is only the explicit, editable list.
    #[serde(default)]
    pub power_sources: Vec<PowerSource>,
}

impl Stage {
    /// The stage's power sources as the physics sees them: the explicit
    /// list, or — when the player fitted nothing — the default battery
    /// every stage carries.
    ///
    /// Every mass, supply, capacity and drain calculation goes through
    /// here, so there is no such thing as a stage outside the power
    /// system. A stage used to be exempt when this list was empty, which
    /// made a design with no power sources immortal anywhere in the solar
    /// system.
    pub fn effective_power_sources(&self) -> std::borrow::Cow<'_, [PowerSource]> {
        if self.power_sources.is_empty() {
            std::borrow::Cow::Owned(vec![PowerSource::default_battery_for_stage(self)])
        } else {
            std::borrow::Cow::Borrowed(&self.power_sources)
        }
    }

    /// Dry mass: structural mass, all engines, the fairing if present, and
    /// the power sources.
    ///
    /// The default battery counts toward this — it is real kit, so it shows
    /// up in delta-v and thrust-to-weight like anything else bolted on.
    pub fn dry_mass_kg(&self) -> f64 {
        let engine_mass = self.engine.mass_kg * self.engine_count as f64;
        let fairing_mass = self.fairing.as_ref().map_or(0.0, |f| f.mass_kg);
        let power_mass: f64 = self.effective_power_sources().iter().map(|p| p.mass_kg).sum();
        self.structural_mass_kg + engine_mass + fairing_mass + power_mass
    }

    /// Steady-state housekeeping draw in watts. Approximates ~1 W per 10 kg
    /// of dry mass (excluding power sources themselves so adding panels
    /// doesn't increase your own load).
    pub fn housekeeping_w(&self) -> f64 {
        let engine_mass = self.engine.mass_kg * self.engine_count as f64;
        let fairing_mass = self.fairing.as_ref().map_or(0.0, |f| f.mass_kg);
        let bus_mass = self.structural_mass_kg + engine_mass + fairing_mass;
        bus_mass * 0.1 // 1 W per 10 kg
    }

    /// Wet mass: dry mass + propellant.
    pub fn wet_mass_kg(&self) -> f64 {
        self.dry_mass_kg() + self.propellant_mass_kg
    }

    /// Total thrust from all engines on this stage (Newtons).
    pub fn total_thrust_n(&self) -> f64 {
        self.engine.thrust_n * self.engine_count as f64
    }

    /// Burn time in seconds (all propellant, all engines firing).
    pub fn burn_time_s(&self) -> f64 {
        let flow_rate = self.engine.mass_flow_rate() * self.engine_count as f64;
        if flow_rate <= 0.0 {
            return 0.0;
        }
        self.propellant_mass_kg / flow_rate
    }

    /// Delta-v this stage provides, given a payload mass sitting above it.
    /// Uses the Tsiolkovsky rocket equation: dv = Ve * ln(m0 / mf)
    /// where m0 = wet + payload, mf = dry + payload.
    pub fn delta_v(&self, payload_mass_kg: f64) -> f64 {
        let m0 = self.wet_mass_kg() + payload_mass_kg;
        let mf = self.dry_mass_kg() + payload_mass_kg;
        if mf <= 0.0 {
            return 0.0;
        }
        self.engine.exhaust_velocity() * (m0 / mf).ln()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::*;
    use crate::propellant::Propellant;

    fn test_engine() -> EngineDesign {
        EngineDesign {
            id: EngineId(1),
            name: "TestEngine".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 1_000_000.0,
            mass_kg: 500.0,
            isp_s: 300.0,
            exit_pressure_pa: 100_000.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
            power_draw_w: 0.0,
        }
    }

    fn test_stage() -> Stage {
        Stage {
            id: StageId(1),
            name: "S1".into(),
            engine: test_engine(),
            engine_count: 1,
            propellant_mass_kg: 20_000.0,
            structural_mass_kg: 1_500.0,
            fairing: None,
            power_sources: Vec::new(),
        }
    }

    /// Mass of the battery a bare stage carries by default. Every dry-mass
    /// figure below includes it: the stage fits no power sources of its own,
    /// so `effective_power_sources` hands it the default kit, and that kit
    /// weighs something.
    fn default_battery_mass(s: &Stage) -> f64 {
        PowerSource::default_battery_for_stage(s).mass_kg
    }

    #[test]
    fn test_dry_mass_no_fairing() {
        let s = test_stage();
        // structural 1500 + 1 engine * 500 = 2000, + the default battery
        assert_eq!(s.dry_mass_kg(), 2000.0 + default_battery_mass(&s));
    }

    #[test]
    fn test_dry_mass_with_fairing() {
        let mut s = test_stage();
        s.fairing = Some(Fairing { mass_kg: 200.0, diameter_m: 4.0 });
        assert_eq!(s.dry_mass_kg(), 2200.0 + default_battery_mass(&s));
    }

    #[test]
    fn test_wet_mass() {
        let s = test_stage();
        assert_eq!(s.wet_mass_kg(), 22_000.0 + default_battery_mass(&s));
    }

    #[test]
    fn test_multi_engine_thrust() {
        let mut s = test_stage();
        s.engine_count = 9;
        assert_eq!(s.total_thrust_n(), 9_000_000.0);
    }

    #[test]
    fn test_multi_engine_dry_mass() {
        let mut s = test_stage();
        s.engine_count = 3;
        // 1500 + 3*500 = 3000, + the default battery
        assert_eq!(s.dry_mass_kg(), 3000.0 + default_battery_mass(&s));
    }

    #[test]
    fn test_burn_time() {
        let s = test_stage();
        let ve = s.engine.exhaust_velocity(); // 300 * 9.80665 ≈ 2941.995
        let flow = s.engine.thrust_n / ve; // 1e6 / 2942 ≈ 339.9
        let expected = 20_000.0 / flow;
        assert!((s.burn_time_s() - expected).abs() < 0.1, "got {}", s.burn_time_s());
    }

    #[test]
    fn test_delta_v_no_payload() {
        let s = test_stage();
        let ve = s.engine.exhaust_velocity();
        // Against the stage's real masses, not the bare 2000/22000: the
        // default battery is part of the dry mass it has to haul.
        let expected = ve * (s.wet_mass_kg() / s.dry_mass_kg()).ln();
        let dv = s.delta_v(0.0);
        assert!((dv - expected).abs() < 1.0, "expected {}, got {}", expected, dv);
        assert!(
            s.dry_mass_kg() > 2_000.0,
            "premise: the default battery is real mass, got {}", s.dry_mass_kg(),
        );
    }

    #[test]
    fn test_delta_v_with_payload() {
        let s = test_stage();
        let ve = s.engine.exhaust_velocity();
        let payload = 5_000.0;
        let expected = ve
            * ((s.wet_mass_kg() + payload) / (s.dry_mass_kg() + payload)).ln();
        let dv = s.delta_v(payload);
        assert!((dv - expected).abs() < 1.0, "expected {}, got {}", expected, dv);
    }

    #[test]
    fn test_more_payload_less_delta_v() {
        let s = test_stage();
        let dv_light = s.delta_v(1_000.0);
        let dv_heavy = s.delta_v(10_000.0);
        assert!(dv_light > dv_heavy);
    }
}
