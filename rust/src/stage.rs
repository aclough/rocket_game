use crate::engine::EngineDesign;

/// Unique identifier for a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StageId(pub u64);

/// A payload fairing that sits on top of a stage.
#[derive(Debug, Clone)]
pub struct Fairing {
    pub mass_kg: f64,
    pub diameter_m: f64,
}

/// A rocket stage: structural mass, engines, propellant, and optional fairing.
///
/// The stage holds a reference to its engine design (by clone) and the number of
/// engines of that type. It does NOT own fuel composition — that comes from the engine.
#[derive(Debug, Clone)]
pub struct Stage {
    pub id: StageId,
    pub name: String,
    pub engine: EngineDesign,
    pub engine_count: u32,
    pub propellant_mass_kg: f64,
    pub structural_mass_kg: f64,
    pub fairing: Option<Fairing>,
}

impl Stage {
    /// Dry mass: structural mass + all engines + fairing (if present).
    pub fn dry_mass_kg(&self) -> f64 {
        let engine_mass = self.engine.mass_kg * self.engine_count as f64;
        let fairing_mass = self.fairing.as_ref().map_or(0.0, |f| f.mass_kg);
        self.structural_mass_kg + engine_mass + fairing_mass
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
        }
    }

    #[test]
    fn test_dry_mass_no_fairing() {
        let s = test_stage();
        // structural 1500 + 1 engine * 500 = 2000
        assert_eq!(s.dry_mass_kg(), 2000.0);
    }

    #[test]
    fn test_dry_mass_with_fairing() {
        let mut s = test_stage();
        s.fairing = Some(Fairing { mass_kg: 200.0, diameter_m: 4.0 });
        assert_eq!(s.dry_mass_kg(), 2200.0);
    }

    #[test]
    fn test_wet_mass() {
        let s = test_stage();
        assert_eq!(s.wet_mass_kg(), 22_000.0);
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
        // 1500 + 3*500 = 3000
        assert_eq!(s.dry_mass_kg(), 3000.0);
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
        let expected = ve * (22_000.0_f64 / 2_000.0).ln();
        let dv = s.delta_v(0.0);
        assert!((dv - expected).abs() < 1.0, "expected {}, got {}", expected, dv);
    }

    #[test]
    fn test_delta_v_with_payload() {
        let s = test_stage();
        let ve = s.engine.exhaust_velocity();
        let payload = 5_000.0;
        let expected = ve * ((22_000.0_f64 + payload) / (2_000.0 + payload)).ln();
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
