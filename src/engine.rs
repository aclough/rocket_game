use serde::{Serialize, Deserialize};

use crate::project::Direction;
use crate::technology::TechDeficiencyKind;

use crate::propellant::Propellant;
use crate::balance_config::NozzleConfig;
use crate::nozzle::{self, AtmosphereResponse};

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
    /// Ion/Hall-effect thruster — very high Isp, very low thrust.
    ElectricPropulsion,
    /// Solar sail — thrust from solar radiation pressure, no propellant.
    SolarSail,
}

impl EngineCycle {
    /// Human-readable name for panes and editors.
    pub fn display_name(&self) -> &'static str {
        match self {
            EngineCycle::PressureFed => "Pressure Fed",
            EngineCycle::GasGenerator => "Gas Generator",
            EngineCycle::Expander => "Expander",
            EngineCycle::StagedCombustion => "Staged Combustion",
            EngineCycle::FullFlow => "Full Flow",
            EngineCycle::NuclearThermal => "Nuclear Thermal",
            EngineCycle::ElectricPropulsion => "Electric Propulsion",
            EngineCycle::SolarSail => "Solar Sail",
        }
    }
}

/// A single propellant component in the engine's mix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// True when this unit carries the sea-level nozzle. The stored
    /// inverse of "vacuum variant" — see `is_vacuum_variant`. Nothing
    /// in the simulation reads this directly; the physics goes through
    /// `exit_pressure_pa` (see `isp_fraction_at_pressure` and
    /// `overexpansion_risk`). It exists so the UI can tell which bell
    /// a stage is flying without re-deriving it from pressures.
    pub needs_atmosphere: bool,
    pub propellant_mix: Vec<PropellantFraction>,
    /// Electrical power required (watts) to operate at full rated thrust.
    /// Default 0 — chemical, nuclear-thermal, and solar-sail engines
    /// don't consume electrical power. Set for ElectricPropulsion at
    /// engine-design time. When positive, the rocket's available power
    /// (supply minus housekeeping) caps the engine's effective thrust.
    #[serde(default)]
    pub power_draw_w: f64,
    /// Chamber pressure (Pa). With `expansion_ratio` and `gamma` this is
    /// the nozzle: it sets how much of the vacuum thrust the air takes
    /// back (`atmosphere_response`), the bell a chamber pressure allows
    /// at sea level, and the exit pressure the separation risk reads.
    /// 0 means "no nozzle" — electric, sail, or a design from before
    /// nozzles were modelled (`save::sanitize` back-fills those).
    #[serde(default)]
    pub chamber_pressure_pa: f64,
    /// Expansion ratio of this variant's bell (exit area / throat area).
    #[serde(default)]
    pub expansion_ratio: f64,
    /// Heat-capacity ratio of the exhaust; 0 reads as `DEFAULT_GAMMA`.
    #[serde(default)]
    pub gamma: f64,
}

/// Heat-capacity ratio assumed for a design that carries none.
pub const DEFAULT_GAMMA: f64 = 1.2;

impl EngineDesign {
    /// Apply or revert the stat effect of one technology deficiency.
    /// Complexity penalties land on the project, not the design, and
    /// reactor-domain kinds never reach an engine; both are no-ops here.
    pub fn apply_deficiency(&mut self, kind: &TechDeficiencyKind, dir: Direction) {
        use Direction::{Apply, Revert};
        use TechDeficiencyKind as K;
        match (kind, dir) {
            (K::IspPenalty(f), Apply) => self.isp_s *= 1.0 - f,
            (K::IspPenalty(f), Revert) => self.isp_s /= 1.0 - f,
            (K::MassPenalty(f), Apply) => self.mass_kg *= 1.0 + f,
            (K::MassPenalty(f), Revert) => self.mass_kg /= 1.0 + f,
            (K::ThrustPenalty(f), Apply) => self.thrust_n *= 1.0 - f,
            (K::ThrustPenalty(f), Revert) => self.thrust_n /= 1.0 - f,
            (K::ComplexityPenalty(_) | K::PowerPenalty(_), _) => {}
        }
    }

    /// Whether this unit carries the vacuum nozzle. The choice is made
    /// per stage in the rocket designer; both variants come from one
    /// engine project.
    pub fn is_vacuum_variant(&self) -> bool {
        !self.needs_atmosphere
    }

    /// Effective exhaust velocity in m/s (Isp * g0).
    pub fn exhaust_velocity(&self) -> f64 {
        self.isp_s * G0
    }

    /// Mass flow rate in kg/s (thrust / exhaust_velocity).
    /// Returns 0.0 for solar sails (no propellant consumed).
    pub fn mass_flow_rate(&self) -> f64 {
        let ve = self.exhaust_velocity();
        if ve == 0.0 { 0.0 } else { self.thrust_n / ve }
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

    /// The exhaust's heat-capacity ratio, `DEFAULT_GAMMA` when unset.
    pub fn gamma_or_default(&self) -> f64 {
        if self.gamma > 0.0 { self.gamma } else { DEFAULT_GAMMA }
    }

    /// Whether this design carries a De Laval nozzle the air can act on.
    pub fn has_nozzle(&self) -> bool {
        self.chamber_pressure_pa > 0.0 && self.expansion_ratio > 0.0
    }

    /// Fit a bell: chamber pressure, expansion ratio and gamma, with the
    /// exit pressure they imply.
    pub fn set_nozzle(&mut self, chamber_pressure_pa: f64, expansion_ratio: f64, gamma: f64) {
        self.chamber_pressure_pa = chamber_pressure_pa;
        self.expansion_ratio = expansion_ratio;
        self.gamma = gamma;
        self.exit_pressure_pa = if self.has_nozzle() {
            chamber_pressure_pa * nozzle::exit_pressure_ratio(expansion_ratio, self.gamma_or_default())
        } else {
            0.0
        };
    }

    /// Fit the bell that exhausts at `exit_pressure_pa` from a chamber
    /// at `chamber_pressure_pa`. How `save::sanitize` gives a design
    /// from before nozzles were modelled the bell its stored exit
    /// pressure describes, and how tests ask for "a nozzle built for
    /// X kPa".
    pub fn set_nozzle_for_exit_pressure(&mut self, chamber_pressure_pa: f64, exit_pressure_pa: f64, gamma: f64) {
        if chamber_pressure_pa <= 0.0 || exit_pressure_pa <= 0.0 {
            self.set_nozzle(0.0, 0.0, gamma);
            return;
        }
        let eps = nozzle::expansion_ratio_for_exit_pressure(chamber_pressure_pa, exit_pressure_pa, gamma);
        self.set_nozzle(chamber_pressure_pa, eps, gamma);
    }

    /// Throat area of this design's nozzle (m²); 0 without one.
    pub fn throat_area_m2(&self) -> f64 {
        if !self.has_nozzle() {
            return 0.0;
        }
        nozzle::throat_area_m2(self.thrust_n, self.chamber_pressure_pa, self.expansion_ratio, self.gamma_or_default())
    }

    /// This design with the long bell: the largest expansion ratio that
    /// fits `cfg.max_vacuum_bell_exit_m`, capped at
    /// `cfg.max_expansion_ratio`. Isp and thrust rise by the thrust-
    /// coefficient ratio (same chamber, same mass flow); mass grows by
    /// the added bell area at `cfg.bell_areal_density_kg_m2`. A design
    /// without a nozzle only loses `needs_atmosphere`.
    pub fn with_vacuum_bell(&self, cfg: &NozzleConfig) -> EngineDesign {
        let mut vac = self.clone();
        vac.needs_atmosphere = false;
        if !self.has_nozzle() {
            return vac;
        }
        let gamma = self.gamma_or_default();
        let throat = self.throat_area_m2();
        let eps_vac = nozzle::expansion_ratio_for_exit_diameter(
            throat, cfg.max_vacuum_bell_exit_m, cfg.max_expansion_ratio, self.expansion_ratio,
        );
        let gain = nozzle::thrust_coefficient_vacuum(eps_vac, gamma)
            / nozzle::thrust_coefficient_vacuum(self.expansion_ratio, gamma);
        vac.isp_s = self.isp_s * gain;
        vac.thrust_n = self.thrust_n * gain;
        vac.mass_kg = self.mass_kg
            + nozzle::bell_extension_mass_kg(throat, self.expansion_ratio, eps_vac, cfg.bell_areal_density_kg_m2);
        vac.set_nozzle(self.chamber_pressure_pa, eps_vac, gamma);
        vac
    }

    /// How the air acts on this design's thrust: the per-step question
    /// the ascent asks. One constant for a bell, nothing for the rest.
    pub fn atmosphere_response(&self) -> AtmosphereResponse {
        if !self.has_nozzle() {
            return AtmosphereResponse::None;
        }
        AtmosphereResponse::Nozzle {
            zero_thrust_pressure_pa: nozzle::zero_thrust_pressure_pa(
                self.chamber_pressure_pa, self.expansion_ratio, self.gamma_or_default(),
            ),
        }
    }

    /// Fraction of vacuum Isp (and thrust) retained at the given ambient
    /// pressure; 1.0 in vacuum or without a nozzle.
    pub fn isp_fraction_at(&self, ambient_pressure_pa: f64) -> f64 {
        self.atmosphere_response().thrust_fraction(ambient_pressure_pa)
    }

    /// Per-engine probability of destruction from flow separation when
    /// the air outside is far denser than the bell's exit: a ramp of
    /// `cfg.separation_risk_slope` per unit of ambient / exit pressure
    /// beyond `cfg.separation_risk_start_ratio`, clamped to [0, 1]. 0 in
    /// vacuum, without a nozzle, or when the bell is matched.
    pub fn overexpansion_destruction_risk(&self, ambient_pressure_pa: f64, cfg: &NozzleConfig) -> f64 {
        if ambient_pressure_pa <= 0.0 || self.exit_pressure_pa <= 0.0 {
            return 0.0;
        }
        let ratio = ambient_pressure_pa / self.exit_pressure_pa;
        ((ratio - cfg.separation_risk_start_ratio) * cfg.separation_risk_slope).clamp(0.0, 1.0)
    }

    /// Whether this engine is a low-thrust type (ion, Hall, solar sail).
    /// Low-thrust engines can only use transfer edges marked low_thrust_ok.
    /// A solid motor: one propellant, the solid mix. Its tank is its
    /// casing, so the designer can't resize it by the step.
    pub fn is_solid(&self) -> bool {
        self.propellant_mix.len() == 1
            && self.propellant_mix[0].propellant == crate::propellant::Propellant::SolidMix
    }

    pub fn is_low_thrust(&self) -> bool {
        matches!(self.cycle, EngineCycle::ElectricPropulsion | EngineCycle::SolarSail)
    }

    /// Whether this engine is a solar sail (no propellant, infinite dv).
    pub fn is_solar_sail(&self) -> bool {
        matches!(self.cycle, EngineCycle::SolarSail)
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

    /// Merlin 1D-like: 97 bar, ε 16 sea-level bell.
    fn test_kerolox_engine() -> EngineDesign {
        let mut e = EngineDesign {
            id: EngineId(1),
            name: "Merlin-like".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 914_000.0,
            mass_kg: 470.0,
            isp_s: 311.0,
            exit_pressure_pa: 0.0,
            needs_atmosphere: true,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
            power_draw_w: 0.0,
            chamber_pressure_pa: 0.0,
            expansion_ratio: 0.0,
            gamma: 0.0,
        };
        e.set_nozzle(9_700_000.0, 16.0, 1.2);
        e
    }

    /// RL10-like: 44 bar, a bell exhausting at 5 kPa.
    fn test_hydrolox_engine() -> EngineDesign {
        let mut e = EngineDesign {
            id: EngineId(2),
            name: "RL-10-like".into(),
            cycle: EngineCycle::Expander,
            thrust_n: 110_000.0,
            mass_kg: 170.0,
            isp_s: 465.0,
            exit_pressure_pa: 0.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.833 },
                PropellantFraction { propellant: Propellant::LH2, mass_fraction: 0.167 },
            ],
            power_draw_w: 0.0,
            chamber_pressure_pa: 0.0,
            expansion_ratio: 0.0,
            gamma: 0.0,
        };
        e.set_nozzle_for_exit_pressure(4_400_000.0, 5_000.0, 1.2);
        e
    }

    fn cfg() -> NozzleConfig {
        NozzleConfig::default()
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
        // 914000 / 3049.87 ≈ 299.7
        assert!((mdot - 299.7).abs() < 1.0, "got {}", mdot);
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
        let engine = test_kerolox_engine();
        // In vacuum (0 Pa ambient), no penalty
        assert_eq!(engine.isp_fraction_at(0.0), 1.0);
        // In vacuum (negative, shouldn't happen but guard)
        assert_eq!(engine.isp_fraction_at(-1.0), 1.0);
    }

    #[test]
    fn test_isp_fraction_sea_level_engine() {
        let engine = test_kerolox_engine();
        let frac = engine.isp_fraction_at(101_325.0);
        // Merlin 1D: 282 / 311 = 0.907 at the pad.
        assert!(frac > 0.89 && frac < 0.92,
            "Sea-level bell should lose ~9% Isp at the pad, got fraction {}", frac);
        assert!((engine.exit_pressure_pa - 65_700.0).abs() < 1_000.0,
            "ε 16 at 97 bar exhausts near 66 kPa, got {}", engine.exit_pressure_pa);
    }

    #[test]
    fn test_isp_fraction_vacuum_engine() {
        let engine = test_hydrolox_engine(); // exit_pressure = 5 kPa
        let frac = engine.isp_fraction_at(101_325.0);
        // A 5 kPa bell at 44 bar is deep into back-pressure at the pad.
        assert!(frac > 0.1 && frac < 0.5,
            "Vacuum bell should lose most of its thrust at sea level, got fraction {}", frac);
    }

    #[test]
    fn test_no_nozzle_is_untouched_by_air() {
        let mut ion = test_kerolox_engine();
        ion.set_nozzle(0.0, 0.0, 0.0);
        assert!(!ion.has_nozzle());
        assert_eq!(ion.atmosphere_response(), crate::nozzle::AtmosphereResponse::None);
        assert_eq!(ion.isp_fraction_at(101_325.0), 1.0);
        assert_eq!(ion.overexpansion_destruction_risk(101_325.0, &cfg()), 0.0);
        assert_eq!(ion.exit_pressure_pa, 0.0);
    }

    #[test]
    fn test_vacuum_bell_variant() {
        let sl = test_kerolox_engine();
        let vac = sl.with_vacuum_bell(&cfg());
        // A Merlin throat fits ~ε 130 inside a 3 m exit.
        assert!(vac.expansion_ratio > 120.0 && vac.expansion_ratio < 140.0, "ε {}", vac.expansion_ratio);
        let gain = vac.isp_s / sl.isp_s;
        assert!(gain > 1.08 && gain < 1.11, "Isp gain {gain}");
        assert!((vac.thrust_n / sl.thrust_n - gain).abs() < 1e-9);
        // ~6 m² of added bell at 20 kg/m².
        assert!(vac.mass_kg - sl.mass_kg > 100.0 && vac.mass_kg - sl.mass_kg < 160.0,
            "bell mass {}", vac.mass_kg - sl.mass_kg);
        assert!(!vac.needs_atmosphere && vac.is_vacuum_variant());
        assert!(vac.exit_pressure_pa < 10_000.0);
        assert_eq!(vac.chamber_pressure_pa, sl.chamber_pressure_pa);
        // The long bell at the pad: heavily penalised and at risk.
        assert!(vac.isp_fraction_at(101_325.0) < 0.5);
        assert_eq!(vac.overexpansion_destruction_risk(101_325.0, &cfg()), 1.0);
    }

    #[test]
    fn test_overexpansion_no_risk_sea_level_engine() {
        let engine = test_kerolox_engine(); // exit ~58 kPa
        let risk = engine.overexpansion_destruction_risk(101_325.0, &cfg());
        // ratio = 101325/57800 = 1.75 < 3 → 0
        assert_eq!(risk, 0.0);
    }

    #[test]
    fn test_overexpansion_risk_vacuum_engine() {
        let engine = test_hydrolox_engine(); // exit_pressure = 5 kPa
        let risk = engine.overexpansion_destruction_risk(101_325.0, &cfg());
        // ratio = 101325/5000 = 20.3, (20.3 - 3) * 0.2 → capped at 1.0
        assert_eq!(risk, 1.0, "Deep vacuum engine should have 100% destruction risk");
    }

    #[test]
    fn test_overexpansion_risk_moderate() {
        // A bell exhausting at 20 kPa.
        let mut engine = test_kerolox_engine();
        engine.set_nozzle_for_exit_pressure(9_700_000.0, 20_000.0, 1.2);
        assert!((engine.exit_pressure_pa - 20_000.0).abs() < 1.0);
        let risk = engine.overexpansion_destruction_risk(101_325.0, &cfg());
        // ratio = 101325/20000 = 5.07, (5.07 - 3) * 0.2 = 0.413
        assert!(risk > 0.40 && risk < 0.42,
            "20 kPa engine should have ~41% risk, got {}", risk);
    }

    #[test]
    fn test_overexpansion_risk_in_vacuum() {
        let engine = test_hydrolox_engine();
        let risk = engine.overexpansion_destruction_risk(0.0, &cfg());
        assert_eq!(risk, 0.0, "No risk in vacuum");
    }
}
