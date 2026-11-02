//! Entry, descent and landing physics (25_2_EDL_CORE.md §2.3): what a
//! landing burn costs and whether a stage can hover, hoverslam or not
//! land at all. The limits are `EdlConfig`; the throttle floors are the
//! engine cycles'.

use crate::balance_config::EdlConfig;
use crate::location::DELTA_V_MAP;
use crate::rocket::RocketDesign;

/// How much of a braking burn's duration is spent fighting gravity: the
/// average sine of the flight-path angle over the descent, mostly
/// horizontal braking with a short vertical finish. Apollo's lunar
/// module paid ~230 m/s of loss on a 1 700 m/s descent at this value.
pub const LANDING_PATH_SINE: f64 = 0.25;

/// The gravity loss of landing on an airless body: `g · t_burn ·
/// sin γ`, with the burn lasting the edge's delta-v over the braking
/// acceleration. Zero where there is nothing to land on, or air to
/// land through (chutes, 25_2 step 4), or no thrust to brake with.
pub fn landing_gravity_loss(to: &str, delta_v: f64, braking_accel_m_s2: f64) -> f64 {
    let Some(props) = DELTA_V_MAP.surface_properties(to) else { return 0.0 };
    if props.has_atmosphere || braking_accel_m_s2 <= 0.0 {
        return 0.0;
    }
    props.gravity_m_s2 * (delta_v / braking_accel_m_s2) * LANDING_PATH_SINE
}

/// What the designer, the bid rule and the flight say about setting a
/// stage down on an airless body.
#[derive(Debug, Clone, PartialEq)]
pub enum LandingVerdict {
    /// The stage has no landing burn: nothing to brake with.
    NoBurn,
    /// The engines throttle down to the stage's weight or below.
    Hover,
    /// The engines cannot hold the stage's weight: a timed burn, which
    /// draws extra `Landing` flaws and makes every one bite harder.
    Hoverslam { excess_m_s2: f64, extra_flaws: u32, activation_scale: f64 },
    /// Too much thrust at minimum throttle to time a burn at all.
    TooMuchThrust { excess_m_s2: f64 },
}

impl LandingVerdict {
    /// Whether the stage comes down in one piece, flaws aside.
    pub fn can_land(&self) -> bool {
        matches!(self, LandingVerdict::Hover | LandingVerdict::Hoverslam { .. })
    }

    /// How much harder this landing's flaws bite.
    pub fn activation_scale(&self) -> f64 {
        match self {
            LandingVerdict::Hoverslam { activation_scale, .. } => *activation_scale,
            _ => 1.0,
        }
    }

    /// As the designer and the crash report word it.
    pub fn summary(&self) -> String {
        match self {
            LandingVerdict::NoBurn => "no landing burn".to_string(),
            LandingVerdict::Hover => "hover".to_string(),
            LandingVerdict::Hoverslam { extra_flaws, activation_scale, .. } =>
                format!("hoverslam (+{extra_flaws} flaws, ×{activation_scale:.1})"),
            LandingVerdict::TooMuchThrust { excess_m_s2 } =>
                format!("cannot land: too much thrust ({excess_m_s2:.0} m/s² over gravity at minimum throttle)"),
        }
    }
}

/// The landing engines' excess acceleration over `gravity_m_s2` at a
/// touchdown mass of `mass_kg`, m/s²; negative when they can throttle
/// below the stage's weight. `None` without a landing burn.
pub fn touchdown_excess_m_s2(stage: &crate::stage::Stage, mass_kg: f64, gravity_m_s2: f64) -> Option<f64> {
    stage.landing.as_ref()?;
    if mass_kg <= 0.0 {
        return None;
    }
    Some(stage.landing_min_thrust_n() / mass_kg - gravity_m_s2)
}

/// The verdict for an excess of `excess_m_s2` under `cfg`.
pub fn verdict_for_excess(excess_m_s2: Option<f64>, cfg: &EdlConfig) -> LandingVerdict {
    let Some(excess) = excess_m_s2 else { return LandingVerdict::NoBurn };
    if excess <= 0.0 {
        return LandingVerdict::Hover;
    }
    let max = cfg.landing_max_excess_m_s2.max(f64::EPSILON);
    if excess > max {
        return LandingVerdict::TooMuchThrust { excess_m_s2: excess };
    }
    let fraction = excess / max;
    LandingVerdict::Hoverslam {
        excess_m_s2: excess,
        extra_flaws: (cfg.hoverslam_complexity_max * fraction).round() as u32,
        activation_scale: 1.0 + fraction,
    }
}

/// The landing verdict of `design`'s top stage coming down on `to` with
/// `payload_kg` aboard and its tanks empty. `None` where `to` is not an
/// airless surface: there is nothing to judge yet.
pub fn landing_verdict(design: &RocketDesign, payload_kg: f64, to: &str, cfg: &EdlConfig) -> Option<LandingVerdict> {
    let group = design.stage_groups.last()?;
    let dry: f64 = group.iter().map(|s| s.dry_mass_kg()).sum();
    landing_verdict_at_mass(design, dry + payload_kg, to, cfg)
}

/// `landing_verdict` at a given touchdown mass — a flight's, with
/// whatever is still aboard.
pub fn landing_verdict_at_mass(design: &RocketDesign, touchdown_mass_kg: f64, to: &str, cfg: &EdlConfig) -> Option<LandingVerdict> {
    let props = DELTA_V_MAP.surface_properties(to).filter(|p| !p.has_atmosphere)?;
    let stage = design.stage_groups.last()?.first()?;
    let excess = touchdown_excess_m_s2(stage, touchdown_mass_kg, props.gravity_m_s2);
    Some(verdict_for_excess(excess, cfg))
}

/// The verdict a design carries into its flaw draw: its landing stage
/// on the body it was designed for, empty. `None` for a design without
/// a landing burn.
pub fn design_landing_verdict(design: &RocketDesign, cfg: &EdlConfig) -> Option<LandingVerdict> {
    let group = design.stage_groups.last()?;
    let stage = group.first()?;
    let burn = stage.landing.as_ref()?;
    let dry: f64 = group.iter().map(|s| s.dry_mass_kg()).sum();
    let gravity = DELTA_V_MAP.surface_properties(burn.on.name()).map_or(0.0, |p| p.gravity_m_s2);
    Some(verdict_for_excess(touchdown_excess_m_s2(stage, dry, gravity), cfg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineCycle;
    use crate::stage::{LandingBurn, Stage};
    use crate::location::LocationId;

    fn lander(thrust_n: f64, cycle: EngineCycle, engines: u32, landing: Option<u32>) -> Stage {
        let mut engine = crate::test_util::kerolox_engine(1, thrust_n, 150.0, 311.0);
        engine.cycle = cycle;
        Stage {
            id: crate::stage::StageId(1), name: "Lander".into(), engine, engine_count: engines,
            propellant_mass_kg: 2_350.0, structural_mass_kg: 2_000.0, fairing: None,
            material: crate::structure::StructuralMaterial::Aluminium, power_sources: Vec::new(),
            landing: landing.map(|n| LandingBurn { engines: n, on: LocationId::of("lunar_surface") }),
        }
    }

    /// Hover needs the engines to throttle to the stage's weight; the
    /// hoverslam band is absolute, so the same excess reads the same on
    /// any body; past the ceiling nothing lands.
    #[test]
    fn hover_hoverslam_and_the_ceiling() {
        let cfg = EdlConfig::default();
        let moon = 1.62;
        // A 45 kN pressure-fed descent engine at 10 % on 4.5 t: 1 m/s², under lunar g.
        let lm = lander(45_000.0, EngineCycle::PressureFed, 1, Some(1));
        let excess = touchdown_excess_m_s2(&lm, 4_500.0, moon).unwrap();
        assert!(excess < 0.0, "{excess}");
        assert_eq!(verdict_for_excess(Some(excess), &cfg), LandingVerdict::Hover);
        // The same engine as a gas generator (60 %): 6 m/s² over 1.62 → a hoverslam.
        let gg = lander(45_000.0, EngineCycle::GasGenerator, 1, Some(1));
        let excess = touchdown_excess_m_s2(&gg, 4_500.0, moon).unwrap();
        assert!((excess - (6.0 - moon)).abs() < 1e-9);
        match verdict_for_excess(Some(excess), &cfg) {
            LandingVerdict::Hoverslam { extra_flaws, activation_scale, .. } => {
                assert_eq!(extra_flaws, 1, "4.4 of 20 m/s² is a fifth of the three extra flaws");
                assert!((activation_scale - (1.0 + excess / 20.0)).abs() < 1e-9);
            }
            other => panic!("{other:?}"),
        }
        // Four gas-generator engines lit: 22 m/s² over gravity — nothing lands.
        let four = lander(45_000.0, EngineCycle::GasGenerator, 4, Some(4));
        let excess = touchdown_excess_m_s2(&four, 4_500.0, moon).unwrap();
        assert!((excess - (24.0 - moon)).abs() < 1e-9);
        assert!(matches!(verdict_for_excess(Some(excess), &cfg), LandingVerdict::TooMuchThrust { .. }));
        // The excess is in m/s² whatever the body: the Hoverslam band on Earth is the same width.
        let on_earth = touchdown_excess_m_s2(&gg, 4_500.0, 9.81).unwrap();
        assert!((on_earth - (6.0 - 9.81)).abs() < 1e-9, "and here it hovers");
        // No landing burn: no verdict to compute.
        assert_eq!(touchdown_excess_m_s2(&lander(45_000.0, EngineCycle::PressureFed, 1, None), 4_500.0, moon), None);
        assert_eq!(verdict_for_excess(None, &cfg), LandingVerdict::NoBurn);
        assert!(!LandingVerdict::NoBurn.can_land());
    }

    /// The landing charge is a gravity loss: Apollo's module paid about
    /// 230 m/s coming down at 3 m/s²; a sharper braking burn pays less,
    /// and air or no surface pays nothing here.
    #[test]
    fn landing_loss_is_gravity_times_burn_time() {
        let lm = landing_gravity_loss("lunar_surface", 1_700.0, 3.0);
        assert!((lm - 1.62 * (1_700.0 / 3.0) * LANDING_PATH_SINE).abs() < 1.0, "{lm}");
        assert!((200.0..260.0).contains(&lm), "{lm}");
        assert!(landing_gravity_loss("lunar_surface", 1_700.0, 6.0) < lm);
        assert_eq!(landing_gravity_loss("earth_surface", 7_800.0, 10.0), 0.0, "air: not this model");
        assert_eq!(landing_gravity_loss("leo", 1_000.0, 10.0), 0.0, "no surface");
        assert_eq!(landing_gravity_loss("lunar_surface", 1_700.0, 0.0), 0.0, "nothing to brake with");
    }
}
