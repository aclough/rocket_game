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
}
