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

/// How an engine makes thrust, and what its surroundings do to it
/// (20_PROPULSION.md). One value per design; `EngineCycle` stays the
/// R&D key (complexity, flaw pool, the editor's Cycle row) and
/// `engine_baseline` decides which `Propulsion` a family gets.
///
/// **Contract for a new kind** (the questions every variant answers,
/// checked exhaustively by `propulsion_contract` in the tests): how the
/// air acts on its thrust (`atmosphere_response`), its thrust at a
/// place (`thrust_fraction`), its electrical draw (`power_draw_w`),
/// whether it routes from orbit only (`is_low_thrust`), whether it
/// burns propellant (`consumes_propellant`), which bell if any
/// (`is_sea_level_bell`, `exit_pressure_pa`); plus, outside this
/// enum, a cycle, a baseline, a flaw pool and an editor entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Propulsion {
    /// Exhaust through a De Laval bell: chemical and nuclear thermal.
    /// Zero chamber pressure or expansion ratio means "no bell data"
    /// (a fixture); `has_nozzle` is then false and the air does nothing.
    Nozzle {
        chamber_pressure_pa: f64,
        expansion_ratio: f64,
        /// Heat-capacity ratio of the exhaust; 0 reads as `DEFAULT_GAMMA`.
        gamma: f64,
        /// The short bell, built to fire at sea level; the long bell
        /// is `false`.
        sea_level: bool,
    },
    /// Electric thruster: thrust is capped by the electrical power the
    /// stage can spare, `power_draw_w` being the draw at full thrust.
    Electric { power_draw_w: f64 },
    /// Solar sail: no propellant; thrust from sunlight.
    Sail,
}

/// Where an engine is firing, for [`Propulsion::thrust_fraction`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThrustEnvironment {
    pub ambient_pressure_pa: f64,
    pub sun_distance_au: f64,
}

impl ThrustEnvironment {
    /// Vacuum at 1 AU: what every non-atmospheric burn sees today.
    pub const VACUUM_1AU: ThrustEnvironment = ThrustEnvironment { ambient_pressure_pa: 0.0, sun_distance_au: 1.0 };

    /// An atmosphere at 1 AU.
    pub fn at_pressure(ambient_pressure_pa: f64) -> Self {
        ThrustEnvironment { ambient_pressure_pa, sun_distance_au: 1.0 }
    }

    /// Vacuum at `sun_distance_au`: every burn away from a surface.
    pub fn at_sun(sun_distance_au: f64) -> Self {
        ThrustEnvironment { ambient_pressure_pa: 0.0, sun_distance_au }
    }
}

/// Heat-capacity ratio assumed for a bell that carries none.
pub const DEFAULT_GAMMA: f64 = 1.2;

impl Propulsion {
    /// A bell with `chamber_pressure_pa`, `expansion_ratio` and `gamma`,
    /// short (`sea_level`) or long.
    pub fn nozzle(chamber_pressure_pa: f64, expansion_ratio: f64, gamma: f64, sea_level: bool) -> Self {
        Propulsion::Nozzle { chamber_pressure_pa, expansion_ratio, gamma, sea_level }
    }

    /// A bell that exhausts at `exit_pressure_pa` from `chamber_pressure_pa`.
    pub fn nozzle_for_exit_pressure(chamber_pressure_pa: f64, exit_pressure_pa: f64, gamma: f64, sea_level: bool) -> Self {
        let eps = if chamber_pressure_pa > 0.0 && exit_pressure_pa > 0.0 {
            nozzle::expansion_ratio_for_exit_pressure(chamber_pressure_pa, exit_pressure_pa, gamma)
        } else {
            0.0
        };
        Propulsion::nozzle(chamber_pressure_pa, eps, gamma, sea_level)
    }

    /// Carries a bell the air acts on.
    pub fn has_nozzle(&self) -> bool {
        matches!(self, Propulsion::Nozzle { chamber_pressure_pa, expansion_ratio, .. }
            if *chamber_pressure_pa > 0.0 && *expansion_ratio > 0.0)
    }

    /// The bell's `(chamber pressure, expansion ratio, gamma)`; None
    /// without a usable bell.
    pub fn bell(&self) -> Option<(f64, f64, f64)> {
        match *self {
            Propulsion::Nozzle { chamber_pressure_pa, expansion_ratio, gamma, .. } if self.has_nozzle() => {
                Some((chamber_pressure_pa, expansion_ratio, if gamma > 0.0 { gamma } else { DEFAULT_GAMMA }))
            }
            _ => None,
        }
    }

    /// The short bell. False for the long bell and for anything without one.
    pub fn is_sea_level_bell(&self) -> bool {
        matches!(self, Propulsion::Nozzle { sea_level: true, .. })
    }

    /// Exit pressure of the bell (Pa); 0 without one. Derived, never stored.
    pub fn exit_pressure_pa(&self) -> f64 {
        match self.bell() {
            Some((pc, eps, gamma)) => pc * nozzle::exit_pressure_ratio(eps, gamma),
            None => 0.0,
        }
    }

    /// What the air does to thrust: one constant for a bell, nothing
    /// for the rest (19_NOZZLES.md §3).
    pub fn atmosphere_response(&self) -> AtmosphereResponse {
        match self.bell() {
            Some((pc, eps, gamma)) => AtmosphereResponse::Nozzle {
                zero_thrust_pressure_pa: nozzle::zero_thrust_pressure_pa(pc, eps, gamma),
            },
            None => AtmosphereResponse::None,
        }
    }

    /// Fraction of rated (vacuum, 1 AU) thrust delivered at `env`. A
    /// bell loses to the air; a sail's push follows the sunlight, the
    /// inverse square of its distance from the Sun — a quarter at 2 AU,
    /// and more than rated inside 1 AU.
    pub fn thrust_fraction(&self, env: &ThrustEnvironment) -> f64 {
        match self {
            Propulsion::Nozzle { .. } => self.atmosphere_response().thrust_fraction(env.ambient_pressure_pa),
            Propulsion::Electric { .. } => 1.0,
            Propulsion::Sail => {
                if env.sun_distance_au > 0.0 { 1.0 / (env.sun_distance_au * env.sun_distance_au) } else { 1.0 }
            }
        }
    }

    /// Electrical power at full thrust (W); 0 unless electric.
    pub fn power_draw_w(&self) -> f64 {
        match *self {
            Propulsion::Electric { power_draw_w } => power_draw_w,
            Propulsion::Nozzle { .. } | Propulsion::Sail => 0.0,
        }
    }

    /// Whether this kind never lifts off: it routes from orbit on the
    /// delta-v graph's spiral edges.
    pub fn is_low_thrust(&self) -> bool {
        matches!(self, Propulsion::Electric { .. } | Propulsion::Sail)
    }

    /// Whether a burn consumes propellant (a sail's burn is ∞).
    pub fn consumes_propellant(&self) -> bool {
        !matches!(self, Propulsion::Sail)
    }
}

/// An engine design blueprint.
///
/// Reads through [`EngineDesignRepr`], so a save from before
/// `propulsion` existed — nozzle data as flat fields, or none at all —
/// loads as the same design.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "EngineDesignRepr")]
pub struct EngineDesign {
    pub id: EngineId,
    pub name: String,
    pub cycle: EngineCycle,
    /// Rated thrust: in vacuum, at 1 AU (N).
    pub thrust_n: f64,
    pub mass_kg: f64,
    /// Vacuum Isp (s).
    pub isp_s: f64,
    pub propellant_mix: Vec<PropellantFraction>,
    /// How it makes thrust and what its surroundings do to it.
    pub propulsion: Propulsion,
}

/// The wire form of an `EngineDesign`: the current shape plus every
/// field a past version stored flat. `From` folds the old fields into
/// `propulsion` and gives a pre-nozzle design the bell its exit
/// pressure described (`engine_project::chamber_pressure_for_legacy`).
#[derive(Deserialize)]
pub struct EngineDesignRepr {
    id: EngineId,
    name: String,
    cycle: EngineCycle,
    thrust_n: f64,
    mass_kg: f64,
    isp_s: f64,
    propellant_mix: Vec<PropellantFraction>,
    #[serde(default)]
    propulsion: Option<Propulsion>,
    // Legacy flat fields.
    #[serde(default)]
    exit_pressure_pa: f64,
    #[serde(default)]
    needs_atmosphere: bool,
    #[serde(default)]
    power_draw_w: f64,
    #[serde(default)]
    chamber_pressure_pa: f64,
    #[serde(default)]
    expansion_ratio: f64,
    #[serde(default)]
    gamma: f64,
}

impl From<EngineDesignRepr> for EngineDesign {
    fn from(r: EngineDesignRepr) -> Self {
        let propulsion = r.propulsion.unwrap_or_else(|| match r.cycle {
            EngineCycle::ElectricPropulsion => Propulsion::Electric { power_draw_w: r.power_draw_w },
            EngineCycle::SolarSail => Propulsion::Sail,
            _ => {
                if r.chamber_pressure_pa <= 0.0 && r.exit_pressure_pa > 0.0 {
                    // Before nozzles were modelled: fit the bell the stored
                    // exit pressure describes, at the family's chamber pressure.
                    match crate::engine_project::chamber_pressure_for_legacy(r.cycle, &r.propellant_mix) {
                        Some((pc, gamma)) => Propulsion::nozzle_for_exit_pressure(pc, r.exit_pressure_pa, gamma, r.needs_atmosphere),
                        None => Propulsion::nozzle(0.0, 0.0, 0.0, r.needs_atmosphere),
                    }
                } else {
                    Propulsion::nozzle(r.chamber_pressure_pa, r.expansion_ratio, r.gamma, r.needs_atmosphere)
                }
            }
        });
        EngineDesign {
            id: r.id, name: r.name, cycle: r.cycle, thrust_n: r.thrust_n, mass_kg: r.mass_kg,
            isp_s: r.isp_s, propellant_mix: r.propellant_mix, propulsion,
        }
    }
}

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

    /// Whether this unit flies the long bell — or has no bell for the
    /// air to act on. The stored inverse used to be `needs_atmosphere`.
    pub fn is_vacuum_variant(&self) -> bool {
        !self.propulsion.is_sea_level_bell()
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

        if !self.propulsion_matches_cycle() {
            errors.push(format!(
                "{:?} cycle with {:?} propulsion", self.cycle, self.propulsion
            ));
        }

        errors
    }

    /// Whether this design carries a De Laval nozzle the air can act on.
    pub fn has_nozzle(&self) -> bool {
        self.propulsion.has_nozzle()
    }

    /// Exit pressure of the bell (Pa); 0 without one.
    pub fn exit_pressure_pa(&self) -> f64 {
        self.propulsion.exit_pressure_pa()
    }

    /// Electrical power at full thrust (W); 0 unless electric.
    pub fn power_draw_w(&self) -> f64 {
        self.propulsion.power_draw_w()
    }

    /// Which bell the design was: the short one unless it is already
    /// the long one.
    fn keeps_sea_level_bell(&self) -> bool {
        match self.propulsion {
            Propulsion::Nozzle { sea_level, .. } => sea_level,
            _ => true,
        }
    }

    /// Fit a bell: chamber pressure, expansion ratio and gamma. Keeps
    /// which bell (short or long) the design was; a design that had no
    /// bell becomes a short one.
    pub fn set_nozzle(&mut self, chamber_pressure_pa: f64, expansion_ratio: f64, gamma: f64) {
        let sea_level = self.keeps_sea_level_bell();
        self.propulsion = Propulsion::nozzle(chamber_pressure_pa, expansion_ratio, gamma, sea_level);
    }

    /// Fit the bell that exhausts at `exit_pressure_pa` from a chamber
    /// at `chamber_pressure_pa`: how tests ask for "a nozzle built for
    /// X kPa".
    pub fn set_nozzle_for_exit_pressure(&mut self, chamber_pressure_pa: f64, exit_pressure_pa: f64, gamma: f64) {
        let sea_level = self.keeps_sea_level_bell();
        self.propulsion = Propulsion::nozzle_for_exit_pressure(chamber_pressure_pa, exit_pressure_pa, gamma, sea_level);
    }

    /// Throat area of this design's nozzle (m²); 0 without one.
    pub fn throat_area_m2(&self) -> f64 {
        match self.propulsion.bell() {
            Some((pc, eps, gamma)) => nozzle::throat_area_m2(self.thrust_n, pc, eps, gamma),
            None => 0.0,
        }
    }

    /// This design with the long bell: the largest expansion ratio that
    /// fits `cfg.max_vacuum_bell_exit_m`, capped at
    /// `cfg.max_expansion_ratio`. Isp and thrust rise by the thrust-
    /// coefficient ratio (same chamber, same mass flow); mass grows by
    /// the added bell area at `cfg.bell_areal_density_kg_m2`. A design
    /// without a bell is returned as it is (a bell-less `Nozzle` fixture
    /// only loses its sea-level flag).
    pub fn with_vacuum_bell(&self, cfg: &NozzleConfig) -> EngineDesign {
        let mut vac = self.clone();
        let Some((pc, eps_sl, gamma)) = self.propulsion.bell() else {
            if let Propulsion::Nozzle { sea_level, .. } = &mut vac.propulsion {
                *sea_level = false;
            }
            return vac;
        };
        let throat = self.throat_area_m2();
        let eps_vac = nozzle::expansion_ratio_for_exit_diameter(
            throat, cfg.max_vacuum_bell_exit_m, cfg.max_expansion_ratio, eps_sl,
        );
        let gain = nozzle::thrust_coefficient_vacuum(eps_vac, gamma)
            / nozzle::thrust_coefficient_vacuum(eps_sl, gamma);
        vac.isp_s = self.isp_s * gain;
        vac.thrust_n = self.thrust_n * gain;
        vac.mass_kg = self.mass_kg
            + nozzle::bell_extension_mass_kg(throat, eps_sl, eps_vac, cfg.bell_areal_density_kg_m2);
        vac.propulsion = Propulsion::nozzle(pc, eps_vac, gamma, false);
        vac
    }

    /// How the air acts on this design's thrust: the per-step question
    /// the ascent asks. One constant for a bell, nothing for the rest.
    pub fn atmosphere_response(&self) -> AtmosphereResponse {
        self.propulsion.atmosphere_response()
    }

    /// Fraction of vacuum Isp (and thrust) retained at the given ambient
    /// pressure; 1.0 in vacuum or without a nozzle.
    pub fn isp_fraction_at(&self, ambient_pressure_pa: f64) -> f64 {
        self.propulsion.thrust_fraction(&ThrustEnvironment::at_pressure(ambient_pressure_pa))
    }

    /// Per-engine probability of destruction from flow separation when
    /// the air outside is far denser than the bell's exit: a ramp of
    /// `cfg.separation_risk_slope` per unit of ambient / exit pressure
    /// beyond `cfg.separation_risk_start_ratio`, clamped to [0, 1]. 0 in
    /// vacuum, without a nozzle, or when the bell is matched.
    pub fn overexpansion_destruction_risk(&self, ambient_pressure_pa: f64, cfg: &NozzleConfig) -> f64 {
        let exit = self.exit_pressure_pa();
        if ambient_pressure_pa <= 0.0 || exit <= 0.0 {
            return 0.0;
        }
        let ratio = ambient_pressure_pa / exit;
        ((ratio - cfg.separation_risk_start_ratio) * cfg.separation_risk_slope).clamp(0.0, 1.0)
    }

    /// A solid motor: one propellant, the solid mix. Its tank is its
    /// casing, so the designer can't resize it by the step.
    pub fn is_solid(&self) -> bool {
        self.propellant_mix.len() == 1
            && self.propellant_mix[0].propellant == crate::propellant::Propellant::SolidMix
    }

    /// Whether this engine is a low-thrust type (ion, Hall, solar sail).
    /// Low-thrust engines can only use transfer edges marked low_thrust_ok.
    pub fn is_low_thrust(&self) -> bool {
        self.propulsion.is_low_thrust()
    }

    /// Whether this engine is a solar sail (no propellant, infinite dv).
    pub fn is_solar_sail(&self) -> bool {
        !self.propulsion.consumes_propellant()
    }

    /// The propulsion kind the cycle implies: the cycle is the R&D key
    /// and `engine_baseline` hands each family its kind, so the two must
    /// agree on a design.
    pub fn propulsion_matches_cycle(&self) -> bool {
        match (&self.propulsion, self.cycle) {
            (Propulsion::Electric { .. }, EngineCycle::ElectricPropulsion) => true,
            (Propulsion::Sail, EngineCycle::SolarSail) => true,
            (Propulsion::Nozzle { .. }, EngineCycle::ElectricPropulsion | EngineCycle::SolarSail) => false,
            (Propulsion::Nozzle { .. }, _) => true,
            _ => false,
        }
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
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
            propulsion: Propulsion::nozzle(0.0, 0.0, 0.0, true),
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
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.833 },
                PropellantFraction { propellant: Propellant::LH2, mass_fraction: 0.167 },
            ],
            propulsion: Propulsion::nozzle(0.0, 0.0, 0.0, false),
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
        assert!((engine.exit_pressure_pa() - 65_700.0).abs() < 1_000.0,
            "ε 16 at 97 bar exhausts near 66 kPa, got {}", engine.exit_pressure_pa());
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
        ion.propulsion = Propulsion::Electric { power_draw_w: 30_000.0 };
        assert!(!ion.has_nozzle());
        assert_eq!(ion.power_draw_w(), 30_000.0);
        assert!(ion.is_vacuum_variant(), "no bell: nothing to fly at sea level");
        assert_eq!(ion.atmosphere_response(), crate::nozzle::AtmosphereResponse::None);
        assert_eq!(ion.isp_fraction_at(101_325.0), 1.0);
        assert_eq!(ion.overexpansion_destruction_risk(101_325.0, &cfg()), 0.0);
        assert_eq!(ion.exit_pressure_pa(), 0.0);
    }

    #[test]
    fn test_vacuum_bell_variant() {
        let sl = test_kerolox_engine();
        let vac = sl.with_vacuum_bell(&cfg());
        // A Merlin throat fits ~ε 130 inside a 3 m exit.
        let (vac_pc, vac_eps, _) = vac.propulsion.bell().unwrap();
        let (sl_pc, _, _) = sl.propulsion.bell().unwrap();
        assert!(vac_eps > 120.0 && vac_eps < 140.0, "ε {}", vac_eps);
        let gain = vac.isp_s / sl.isp_s;
        assert!(gain > 1.08 && gain < 1.11, "Isp gain {gain}");
        assert!((vac.thrust_n / sl.thrust_n - gain).abs() < 1e-9);
        // ~6 m² of added bell at 20 kg/m².
        assert!(vac.mass_kg - sl.mass_kg > 100.0 && vac.mass_kg - sl.mass_kg < 160.0,
            "bell mass {}", vac.mass_kg - sl.mass_kg);
        assert!(!vac.propulsion.is_sea_level_bell() && vac.is_vacuum_variant());
        assert!(vac.exit_pressure_pa() < 10_000.0);
        assert_eq!(vac_pc, sl_pc);
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
        assert!((engine.exit_pressure_pa() - 20_000.0).abs() < 1.0);
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

    #[test]
    fn kind_questions_read_the_enum_not_the_cycle() {
        let mut e = test_kerolox_engine();
        assert!(!e.is_low_thrust() && !e.is_solar_sail() && e.validate().is_empty());
        // A nozzle design whose cycle says "electric" is inconsistent,
        // and the kind questions follow the propulsion, not the cycle.
        e.cycle = EngineCycle::ElectricPropulsion;
        assert!(!e.is_low_thrust(), "a bell is not low-thrust whatever the cycle says");
        assert!(e.validate().iter().any(|m| m.contains("propulsion")));
        e.propulsion = Propulsion::Electric { power_draw_w: 1.0 };
        assert!(e.is_low_thrust() && !e.is_solar_sail() && e.validate().is_empty());
        e.cycle = EngineCycle::SolarSail;
        e.propulsion = Propulsion::Sail;
        assert!(e.is_low_thrust() && e.is_solar_sail());
        assert!(e.validate().iter().all(|m| !m.contains("propulsion")));
    }

    /// Saves from before `propulsion` load through the wire form:
    /// flat nozzle fields fold into `Nozzle`, a pre-nozzle design gets
    /// the bell its exit pressure described, and an old ion thruster
    /// keeps its power draw.
    #[test]
    fn legacy_designs_fold_into_propulsion() {
        // The kerolox preset's own mix, so the fold finds the family.
        let mix = r#"[{"propellant":"LOX","mass_fraction":0.73},{"propellant":"RP1","mass_fraction":0.27}]"#;
        // Pre-nozzle (before 19_NOZZLES.md): exit pressure and the bell flag only.
        let old: EngineDesign = serde_json::from_str(&format!(r#"{{"id":1,"name":"Old","cycle":"GasGenerator",
            "thrust_n":900000.0,"mass_kg":1100.0,"isp_s":300.0,"exit_pressure_pa":70000.0,
            "needs_atmosphere":true,"propellant_mix":{mix}}}"#)).unwrap();
        let (pc, eps, gamma) = old.propulsion.bell().expect("a bell was fitted");
        assert_eq!(pc, 9_000_000.0, "the kerolox gas generator's chamber pressure");
        assert!((gamma - 1.22).abs() < 1e-9, "kerolox exhaust gamma, got {gamma}");
        // A mix no preset owns still gets the cycle's kerolox figure and the default gamma.
        let odd: EngineDesign = serde_json::from_str(r#"{"id":4,"name":"Odd","cycle":"StagedCombustion",
            "thrust_n":1680000.0,"mass_kg":1220.0,"isp_s":297.0,"exit_pressure_pa":80000.0,"needs_atmosphere":true,
            "propellant_mix":[{"propellant":"LOX","mass_fraction":0.6},{"propellant":"RP1","mass_fraction":0.4}]}"#).unwrap();
        let (odd_pc, _, odd_gamma) = odd.propulsion.bell().unwrap();
        assert_eq!((odd_pc, odd_gamma), (25_000_000.0, DEFAULT_GAMMA));
        assert!(eps > 1.0 && (old.exit_pressure_pa() - 70_000.0).abs() < 1e-3,
            "the bell exhausts at the stored pressure, got {}", old.exit_pressure_pa());
        assert!(!old.is_vacuum_variant());
        // Flat nozzle fields (19_NOZZLES.md step 2) fold as they are.
        let flat: EngineDesign = serde_json::from_str(&format!(r#"{{"id":2,"name":"Flat","cycle":"GasGenerator",
            "thrust_n":900000.0,"mass_kg":1100.0,"isp_s":311.0,"exit_pressure_pa":1.0,"needs_atmosphere":false,
            "power_draw_w":0.0,"chamber_pressure_pa":9700000.0,"expansion_ratio":16.0,"gamma":1.2,
            "propellant_mix":{mix}}}"#)).unwrap();
        assert_eq!(flat.propulsion, Propulsion::nozzle(9_700_000.0, 16.0, 1.2, false));
        // An old ion thruster keeps its draw and has no bell.
        let ion: EngineDesign = serde_json::from_str(r#"{"id":3,"name":"Ion","cycle":"ElectricPropulsion",
            "thrust_n":1.0,"mass_kg":35.0,"isp_s":3000.0,"exit_pressure_pa":0.0,"needs_atmosphere":false,
            "power_draw_w":30000.0,"propellant_mix":[{"propellant":"Xenon","mass_fraction":1.0}]}"#).unwrap();
        assert_eq!(ion.propulsion, Propulsion::Electric { power_draw_w: 30_000.0 });
        // The current shape round-trips exactly.
        let json = serde_json::to_string(&flat).unwrap();
        assert!(json.contains("\"propulsion\"") && !json.contains("\"exit_pressure_pa\""));
        let back: EngineDesign = serde_json::from_str(&json).unwrap();
        assert_eq!(back.propulsion, flat.propulsion);
    }

    /// The contract a propulsion kind signs (see the enum's doc): every
    /// question it must answer, asked of one design of each kind. The
    /// `match` is exhaustive on purpose — adding a variant fails to
    /// compile here until its row is written, and the rest of the
    /// contract (a cycle, a baseline, a flaw pool, an editor entry) is
    /// listed alongside so it is not forgotten.
    #[test]
    fn propulsion_contract() {
        let bell = Propulsion::nozzle(9_000_000.0, 20.0, 1.2, true);
        let ion = Propulsion::Electric { power_draw_w: 30_000.0 };
        let sail = Propulsion::Sail;
        let pad = ThrustEnvironment::at_pressure(101_325.0);
        let far = ThrustEnvironment::at_sun(3.0);
        for p in [&bell, &ion, &sail] {
            // Rated thrust is the vacuum, 1 AU figure; air and distance
            // only take from it (a sail inside 1 AU is the one exception).
            assert!(p.thrust_fraction(&ThrustEnvironment::VACUUM_1AU) == 1.0);
            assert!(p.thrust_fraction(&pad) <= 1.0 && p.thrust_fraction(&far) <= 1.0);
            match p {
                Propulsion::Nozzle { .. } => {
                    assert!(p.has_nozzle() && p.is_sea_level_bell() && p.exit_pressure_pa() > 0.0);
                    assert!(matches!(p.atmosphere_response(), AtmosphereResponse::Nozzle { .. }));
                    assert!(p.thrust_fraction(&pad) < 1.0, "the air takes something back");
                    assert_eq!(p.power_draw_w(), 0.0);
                    assert!(!p.is_low_thrust() && p.consumes_propellant());
                    // Cycle: any chemical cycle or NuclearThermal. Baseline:
                    // `engine_baseline`. Flaws: the per-cycle pools. Editor:
                    // the Cycle row.
                }
                Propulsion::Electric { .. } => {
                    assert!(!p.has_nozzle() && !p.is_sea_level_bell() && p.exit_pressure_pa() == 0.0);
                    assert_eq!(p.atmosphere_response(), AtmosphereResponse::None);
                    assert_eq!(p.thrust_fraction(&pad), 1.0);
                    assert!(p.power_draw_w() > 0.0, "the flight derates it by power");
                    assert!(p.is_low_thrust() && p.consumes_propellant());
                    // Cycle: ElectricPropulsion. Flaws: ELECTRIC_FLAWS.
                }
                Propulsion::Sail => {
                    assert!(!p.has_nozzle() && !p.is_sea_level_bell() && p.exit_pressure_pa() == 0.0);
                    assert_eq!(p.atmosphere_response(), AtmosphereResponse::None);
                    assert_eq!(p.power_draw_w(), 0.0);
                    assert!(p.is_low_thrust() && !p.consumes_propellant());
                    assert!((p.thrust_fraction(&ThrustEnvironment::at_sun(2.0)) - 0.25).abs() < 1e-12);
                    assert!((p.thrust_fraction(&ThrustEnvironment::at_sun(0.5)) - 4.0).abs() < 1e-12);
                    assert_eq!(p.thrust_fraction(&pad), 1.0, "air does nothing to a sail");
                    // Cycle: SolarSail. Flaws: SOLAR_SAIL_FLAWS.
                }
            }
        }
    }
}
