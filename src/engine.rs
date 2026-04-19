use serde::{Serialize, Deserialize};

use crate::propellant::Propellant;

/// Standard gravity (m/s²), used for Isp <-> exhaust velocity conversion.
pub const G0: f64 = 9.80665;

/// Engine thermodynamic cycle type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EngineCycle {
    PressureFed,
    GasGenerator,
    Expander,
    StagedCombustion,
    FullFlow,
    NuclearThermal,
}

/// A single propellant component in the engine's mix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropellantFraction {
    pub propellant: Propellant,
    pub mass_fraction: f64,
}

/// Unique identifier for an engine design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngineId(pub u64);

/// An engine design blueprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineDesign {
    pub id: EngineId,
    pub name: String,
    pub cycle: EngineCycle,
    pub thrust_n: f64,
    pub mass_kg: f64,
    pub isp_s: f64,
    pub exit_pressure_pa: f64,
    pub needs_atmosphere: bool,
    pub propellant_mix: Vec<PropellantFraction>,
}

impl EngineDesign {
    /// Effective exhaust velocity in m/s (Isp * g0).
    pub fn exhaust_velocity(&self) -> f64 {
        self.isp_s * G0
    }

    /// Mass flow rate in kg/s (thrust / exhaust_velocity).
    pub fn mass_flow_rate(&self) -> f64 {
        self.thrust_n / self.exhaust_velocity()
    }

    /// Validate the engine design. Returns a list of problems (empty = valid).
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.thrust_n <= 0.0 {
            errors.push("Thrust must be positive".into());
        }
        if self.mass_kg <= 0.0 {
            errors.push("Mass must be positive".into());
        }
        if self.isp_s <= 0.0 {
            errors.push("Isp must be positive".into());
        }
        if self.propellant_mix.is_empty() {
            errors.push("Propellant mix must not be empty".into());
        }

        let sum: f64 = self.propellant_mix.iter().map(|f| f.mass_fraction).sum();
        if (sum - 1.0).abs() > 1e-6 {
            errors.push(format!(
                "Propellant fractions must sum to 1.0, got {:.6}", sum
            ));
        }

        for frac in &self.propellant_mix {
            if frac.mass_fraction <= 0.0 || frac.mass_fraction > 1.0 {
                errors.push(format!(
                    "{:?} fraction {:.4} out of range (0, 1]",
                    frac.propellant, frac.mass_fraction
                ));
            }
        }

        errors
    }

    /// Isp fraction retained when operating at the given ambient pressure.
    /// Returns 1.0 in vacuum or when the engine is not overexpanded.
    /// K = 0.20: a vacuum engine (7 kPa exit) at sea level loses ~19% Isp.
    pub fn isp_fraction_at(&self, ambient_pressure_pa: f64) -> f64 {
        if ambient_pressure_pa <= 0.0 || self.exit_pressure_pa >= ambient_pressure_pa {
            return 1.0;
        }
        let k = 0.20;
        (1.0 - k * (1.0 - self.exit_pressure_pa / ambient_pressure_pa)).max(0.0)
    }

    /// Effective Isp at the given ambient pressure (accounting for overexpansion).
    pub fn effective_isp_at(&self, ambient_pressure_pa: f64) -> f64 {
        self.isp_s * self.isp_fraction_at(ambient_pressure_pa)
    }

    /// Effective exhaust velocity at the given ambient pressure.
    pub fn effective_exhaust_velocity_at(&self, ambient_pressure_pa: f64) -> f64 {
        self.effective_isp_at(ambient_pressure_pa) * G0
    }

    /// Per-engine probability of destruction from flow separation due to
    /// severe overexpansion. Returns 0.0 when safely matched or in vacuum.
    /// Formula: ((ambient / exit) - 4) * 0.2, clamped to [0, 1].
    pub fn overexpansion_destruction_risk(&self, ambient_pressure_pa: f64) -> f64 {
        if ambient_pressure_pa <= 0.0 || self.exit_pressure_pa <= 0.0 {
            return 0.0;
        }
        let ratio = ambient_pressure_pa / self.exit_pressure_pa;
        ((ratio - 4.0) * 0.2).clamp(0.0, 1.0)
    }

    /// Propellant cost per kg of total propellant consumed.
    pub fn propellant_cost_per_kg(&self) -> f64 {
        self.propellant_mix.iter()
            .map(|f| f.mass_fraction * f.propellant.cost_per_kg())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_kerolox_engine() -> EngineDesign {
        EngineDesign {
            id: EngineId(1),
            name: "Merlin-like".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 845_000.0,
            mass_kg: 470.0,
            isp_s: 311.0,
            exit_pressure_pa: 70_000.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
        }
    }

    fn test_hydrolox_engine() -> EngineDesign {
        EngineDesign {
            id: EngineId(2),
            name: "RL-10-like".into(),
            cycle: EngineCycle::Expander,
            thrust_n: 110_000.0,
            mass_kg: 170.0,
            isp_s: 465.0,
            exit_pressure_pa: 5_000.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.833 },
                PropellantFraction { propellant: Propellant::LH2, mass_fraction: 0.167 },
            ],
        }
    }

    #[test]
    fn test_exhaust_velocity() {
        let engine = test_kerolox_engine();
        let ve = engine.exhaust_velocity();
        // 311 * 9.80665 ≈ 3049.87
        assert!((ve - 3049.87).abs() < 1.0, "got {}", ve);
    }

    #[test]
    fn test_mass_flow_rate() {
        let engine = test_kerolox_engine();
        let mdot = engine.mass_flow_rate();
        // 845000 / 3049.87 ≈ 277.1
        assert!((mdot - 277.1).abs() < 1.0, "got {}", mdot);
    }

    #[test]
    fn test_valid_engine() {
        let engine = test_kerolox_engine();
        assert!(engine.validate().is_empty());
    }

    #[test]
    fn test_invalid_fractions() {
        let mut engine = test_kerolox_engine();
        engine.propellant_mix[0].mass_fraction = 0.5; // sum now 0.775
        let errors = engine.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("sum to 1.0")));
    }

    #[test]
    fn test_empty_mix() {
        let mut engine = test_kerolox_engine();
        engine.propellant_mix.clear();
        let errors = engine.validate();
        assert!(errors.iter().any(|e| e.contains("not be empty")));
    }

    #[test]
    fn test_propellant_cost() {
        let engine = test_kerolox_engine();
        let cost = engine.propellant_cost_per_kg();
        // 0.725 * 0.16 + 0.275 * 1.10 = 0.116 + 0.3025 = 0.4185
        assert!((cost - 0.4185).abs() < 0.001, "got {}", cost);
    }

    #[test]
    fn test_hydrolox_higher_isp() {
        let kero = test_kerolox_engine();
        let hydro = test_hydrolox_engine();
        assert!(hydro.isp_s > kero.isp_s);
        assert!(hydro.exhaust_velocity() > kero.exhaust_velocity());
    }

    #[test]
    fn test_isp_fraction_vacuum() {
        let engine = test_kerolox_engine(); // exit_pressure = 70 kPa
        // In vacuum (0 Pa ambient), no penalty
        assert_eq!(engine.isp_fraction_at(0.0), 1.0);
        // In vacuum (negative, shouldn't happen but guard)
        assert_eq!(engine.isp_fraction_at(-1.0), 1.0);
    }

    #[test]
    fn test_isp_fraction_sea_level_engine() {
        let engine = test_kerolox_engine(); // exit_pressure = 70 kPa
        let frac = engine.isp_fraction_at(101_325.0);
        // 1.0 - 0.20 * (1.0 - 70000/101325) = 1.0 - 0.20 * 0.309 = 0.938
        assert!(frac > 0.93 && frac < 0.95,
            "Sea-level engine should lose ~6% Isp, got fraction {}", frac);
    }

    #[test]
    fn test_isp_fraction_vacuum_engine() {
        let engine = test_hydrolox_engine(); // exit_pressure = 5 kPa
        let frac = engine.isp_fraction_at(101_325.0);
        // 1.0 - 0.20 * (1.0 - 5000/101325) = 1.0 - 0.20 * 0.951 = 0.810
        assert!(frac > 0.80 && frac < 0.82,
            "Vacuum engine should lose ~19% Isp at sea level, got fraction {}", frac);
    }

    #[test]
    fn test_overexpansion_no_risk_sea_level_engine() {
        let engine = test_kerolox_engine(); // exit_pressure = 70 kPa
        let risk = engine.overexpansion_destruction_risk(101_325.0);
        // ratio = 101325/70000 = 1.45, (1.45 - 4) * 0.2 < 0 → 0
        assert_eq!(risk, 0.0);
    }

    #[test]
    fn test_overexpansion_risk_vacuum_engine() {
        let engine = test_hydrolox_engine(); // exit_pressure = 5 kPa
        let risk = engine.overexpansion_destruction_risk(101_325.0);
        // ratio = 101325/5000 = 20.265, (20.265 - 4) * 0.2 = 3.253 → capped at 1.0
        assert_eq!(risk, 1.0, "Deep vacuum engine should have 100% destruction risk");
    }

    #[test]
    fn test_overexpansion_risk_moderate() {
        // Engine with exit_pressure = 20 kPa
        let mut engine = test_kerolox_engine();
        engine.exit_pressure_pa = 20_000.0;
        let risk = engine.overexpansion_destruction_risk(101_325.0);
        // ratio = 101325/20000 = 5.066, (5.066 - 4) * 0.2 = 0.213
        assert!(risk > 0.20 && risk < 0.22,
            "20 kPa engine should have ~21% risk, got {}", risk);
    }

    #[test]
    fn test_overexpansion_risk_in_vacuum() {
        let engine = test_hydrolox_engine();
        let risk = engine.overexpansion_destruction_risk(0.0);
        assert_eq!(risk, 0.0, "No risk in vacuum");
    }
}
