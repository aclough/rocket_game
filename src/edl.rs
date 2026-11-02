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

/// What a shielded stage pays to fly an edge by aerobraking instead of
/// burning, m/s: targeting and the deorbit burn. A constant like the
/// landing path factor: the planner has no balance config.
pub const AEROBRAKE_DELTA_V_M_S: f64 = 150.0;

/// Whether `design`'s top group — the one that lands — carries a heat
/// shield.
pub fn shielded_top_stage(design: &RocketDesign) -> bool {
    design.stage_groups.last().is_some_and(|g| g.iter().any(|s| s.heat_shield.is_some()))
}

/// The entry speed an aerobraked leg `from → to` would start at, when
/// `group` of `design` flies it: the shielded top group alone can
/// aerobrake (25_2_EDL_CORE.md §2.3). `None` for a burn.
pub fn aerobrake_leg(design: &RocketDesign, group: usize, from: &str, to: &str) -> Option<f64> {
    if group + 1 != design.stage_groups.len() || !shielded_top_stage(design) {
        return None;
    }
    DELTA_V_MAP.aerobrake_entry_speed(from, to)
}

/// How an entry through air will go for `design`'s top stage, meeting
/// the air at `entry_speed_m_s` with `mass_kg` aboard.
#[derive(Debug, Clone, PartialEq)]
pub enum EntryVerdict {
    NoShield,
    /// Within the rating; the heat met over the heat rated for.
    Ok { heat_fraction: f64 },
    /// Over the rating: the chance of burning through.
    BurnThrough { risk: f64, entry_speed_m_s: f64 },
}

impl EntryVerdict {
    pub fn survives(&self) -> bool {
        !matches!(self, EntryVerdict::NoShield)
    }

    pub fn summary(&self) -> String {
        match self {
            EntryVerdict::NoShield => "no heat shield".to_string(),
            EntryVerdict::Ok { heat_fraction } => format!("entry ok ({:.0}% of the shield's rating)", heat_fraction * 100.0),
            EntryVerdict::BurnThrough { risk, entry_speed_m_s } =>
                format!("burn-through risk {:.0}% at {:.0} m/s", risk * 100.0, entry_speed_m_s),
        }
    }
}

/// The entry verdict of `design`'s top stage at `entry_speed_m_s` with
/// `mass_kg` coming down (its own dry mass and shield included).
pub fn entry_verdict(
    design: &RocketDesign, mass_kg: f64, entry_speed_m_s: f64,
    cfg: &EdlConfig, bodies: &crate::balance_config::BodiesConfig,
) -> EntryVerdict {
    let Some(stage) = design.stage_groups.last().and_then(|g| g.first()) else { return EntryVerdict::NoShield };
    let Some(shield) = stage.heat_shield.as_ref() else { return EntryVerdict::NoShield };
    let beta = ballistic_coefficient(mass_kg, stage.base_area_m2(bodies), cfg);
    let heat = entry_heat_j_m2(entry_speed_m_s, beta, cfg);
    let risk = burn_through_risk(heat, shield.rated_heat_j_m2, cfg);
    if risk > 0.0 {
        EntryVerdict::BurnThrough { risk, entry_speed_m_s }
    } else {
        EntryVerdict::Ok { heat_fraction: if shield.rated_heat_j_m2 > 0.0 { heat / shield.rated_heat_j_m2 } else { 0.0 } }
    }
}

/// The entry speed a route's final descent starts at: the first leg of
/// the trailing run of aerobraked legs. `None` when the route ends
/// without aerobraking (an airless body, or a stage that burned its
/// way down).
pub fn route_entry_speed(route: &[crate::flight::FlightLeg]) -> Option<f64> {
    route.iter().rev().take_while(|leg| leg.aerobraked).last().and_then(|leg| leg.entry_speed_m_s)
}

/// Terminal velocity of `mass_kg` on a drag area `drag_area_m2` at
/// `cd`, in air of `density_kg_m3` under `gravity_m_s2`:
/// `√(2 m g / (ρ C_d A))`.
pub fn terminal_velocity_m_s(mass_kg: f64, drag_area_m2: f64, cd: f64, gravity_m_s2: f64, density_kg_m3: f64) -> f64 {
    let denom = density_kg_m3 * cd * drag_area_m2;
    if denom <= 0.0 {
        return f64::INFINITY;
    }
    (2.0 * mass_kg * gravity_m_s2 / denom).sqrt()
}

/// The canopy area that brings `mass_kg` to `speed_m_s` in air of
/// `density_kg_m3` under `gravity_m_s2`.
pub fn drag_area_for_terminal_velocity(mass_kg: f64, speed_m_s: f64, gravity_m_s2: f64, density_kg_m3: f64, cfg: &EdlConfig) -> f64 {
    let denom = density_kg_m3 * cfg.chute_drag_coefficient * speed_m_s * speed_m_s;
    if denom <= 0.0 {
        return 0.0;
    }
    2.0 * mass_kg * gravity_m_s2 / denom
}

/// The chance a touchdown at `speed_m_s` against `limit_m_s` wrecks
/// the stage: nothing within the limit, a ramp to certainty at
/// `touchdown_overshoot` over it.
pub fn touchdown_risk(speed_m_s: f64, limit_m_s: f64, cfg: &EdlConfig) -> f64 {
    if limit_m_s <= 0.0 {
        return 1.0;
    }
    ((speed_m_s / limit_m_s - 1.0) / cfg.touchdown_overshoot.max(1e-9)).clamp(0.0, 1.0)
}

/// Ballistic coefficient, kg/m²: mass over drag area.
pub fn ballistic_coefficient(mass_kg: f64, base_area_m2: f64, cfg: &EdlConfig) -> f64 {
    if base_area_m2 <= 0.0 {
        return f64::INFINITY;
    }
    mass_kg / (cfg.capsule_drag_coefficient * base_area_m2)
}

/// The heat that reaches a m² of face on an entry at `speed_m_s` with
/// ballistic coefficient `beta`, J/m²: a fraction, growing with speed,
/// of the kinetic energy shed per m² (`½ β v²`), so it goes as β v³
/// (25_2_EDL_CORE.md §2.2).
pub fn entry_heat_j_m2(speed_m_s: f64, beta_kg_m2: f64, cfg: &EdlConfig) -> f64 {
    let fraction = cfg.heat_fraction_ref * speed_m_s / cfg.heat_speed_ref_m_s.max(1.0);
    fraction * 0.5 * beta_kg_m2 * speed_m_s * speed_m_s
}

/// The soak a wall of `skin_kg_m2` of `material` takes before it fails,
/// J/m²: its heat capacity from ambient to the material's limit.
pub fn wall_heat_capacity_j_m2(skin_kg_m2: f64, material: crate::structure::StructuralMaterial) -> f64 {
    skin_kg_m2 * material.specific_heat_j_per_kg_k() * (material.temperature_limit_c() - 20.0).max(0.0)
}

/// What a shield of areal density `sigma` lets through to the wall of
/// `heat_j_m2`: it ablates `σ · A` of it and insulates the rest by
/// `exp(−σ / σ_ins)`.
pub fn heat_through_shield_j_m2(heat_j_m2: f64, sigma_kg_m2: f64, cfg: &EdlConfig) -> f64 {
    (heat_j_m2 - sigma_kg_m2 * cfg.ablation_j_per_kg).max(0.0) * (-sigma_kg_m2 / cfg.insulation_kg_m2.max(1e-9)).exp()
}

/// The lightest shield that keeps `heat_j_m2` within what the wall can
/// take, kg/m²; zero when the bare wall already can.
pub fn shield_areal_density_kg_m2(heat_j_m2: f64, wall_capacity_j_m2: f64, cfg: &EdlConfig) -> f64 {
    if heat_through_shield_j_m2(heat_j_m2, 0.0, cfg) <= wall_capacity_j_m2 {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    while heat_through_shield_j_m2(heat_j_m2, hi, cfg) > wall_capacity_j_m2 && hi < 1.0e6 {
        hi *= 2.0;
    }
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if heat_through_shield_j_m2(heat_j_m2, mid, cfg) > wall_capacity_j_m2 { lo = mid } else { hi = mid }
    }
    hi
}

/// The chance an entry meeting `actual_heat_j_m2` burns through a
/// shield rated for `rated_heat_j_m2`: nothing within the rating, then
/// a ramp to certainty at `burn_through_overshoot` over it — the
/// overexpansion rule's shape, a design error rather than a flaw.
pub fn burn_through_risk(actual_heat_j_m2: f64, rated_heat_j_m2: f64, cfg: &EdlConfig) -> f64 {
    if rated_heat_j_m2 <= 0.0 {
        return 1.0;
    }
    ((actual_heat_j_m2 / rated_heat_j_m2 - 1.0) / cfg.burn_through_overshoot.max(1e-9)).clamp(0.0, 1.0)
}

/// What the designer, the bid rule and the flight say about setting a
/// stage down on an airless body.
#[derive(Debug, Clone, PartialEq)]
pub enum LandingVerdict {
    /// The stage has no landing burn: nothing to brake with.
    NoBurn,
    /// A landing burn but nothing to stand on: a crash on land.
    NoLegs,
    /// Through air with neither canopy nor landing burn.
    NoWayDown,
    /// The canopy cannot open here: thin air for a subsonic package, or
    /// a body too dense to slow before the ground.
    ChuteCannotOpen { reason: &'static str },
    /// Down under the canopy onto water.
    Splashdown { speed_m_s: f64 },
    /// Down under the canopy onto legs.
    UnderCanopy { speed_m_s: f64 },
    /// Down under the canopy faster than what it lands on takes: the
    /// chance it is wrecked.
    TooFast { speed_m_s: f64, limit_m_s: f64, risk: f64 },
    /// The engines throttle down to the stage's weight or below.
    Hover,
    /// The engines cannot hold the stage's weight: a timed burn, which
    /// draws extra `Landing` flaws and makes every one bite harder.
    Hoverslam { excess_m_s2: f64, extra_flaws: u32, activation_scale: f64 },
    /// Too much thrust at minimum throttle to time a burn at all.
    TooMuchThrust { excess_m_s2: f64 },
}

impl LandingVerdict {
    /// Whether the stage comes down in one piece, flaws aside (a
    /// touchdown over its limit counts while it is not a sure loss).
    pub fn can_land(&self) -> bool {
        match self {
            LandingVerdict::Hover | LandingVerdict::Hoverslam { .. }
            | LandingVerdict::Splashdown { .. } | LandingVerdict::UnderCanopy { .. } => true,
            LandingVerdict::TooFast { risk, .. } => *risk < 1.0,
            _ => false,
        }
    }

    /// The chance the touchdown itself wrecks the stage.
    pub fn touchdown_risk(&self) -> f64 {
        match self {
            LandingVerdict::TooFast { risk, .. } => *risk,
            _ => 0.0,
        }
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
            LandingVerdict::NoLegs => "no landing legs".to_string(),
            LandingVerdict::NoWayDown => "no way down: no canopy, no landing burn".to_string(),
            LandingVerdict::ChuteCannotOpen { reason } => format!("canopy cannot open: {reason}"),
            LandingVerdict::Splashdown { speed_m_s } => format!("splashdown at {speed_m_s:.0} m/s"),
            LandingVerdict::UnderCanopy { speed_m_s } => format!("lands under canopy at {speed_m_s:.0} m/s"),
            LandingVerdict::TooFast { speed_m_s, limit_m_s, risk } =>
                format!("too fast: {speed_m_s:.0} m/s against {limit_m_s:.0} (risk {:.0}%)", risk * 100.0),
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

/// The verdict for an excess of `excess_m_s2` under `cfg`, on a stage
/// with or without legs to stand on.
pub fn verdict_for_excess(excess_m_s2: Option<f64>, cfg: &EdlConfig) -> LandingVerdict {
    verdict_for_excess_on_legs(excess_m_s2, true, cfg)
}

/// `verdict_for_excess`; without legs a landing burn ends on its side.
pub fn verdict_for_excess_on_legs(excess_m_s2: Option<f64>, legs: bool, cfg: &EdlConfig) -> LandingVerdict {
    let Some(excess) = excess_m_s2 else { return LandingVerdict::NoBurn };
    if !legs {
        return LandingVerdict::NoLegs;
    }
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
/// `payload_kg` aboard and its tanks empty. `None` where `to` is not a
/// surface.
pub fn landing_verdict(
    design: &RocketDesign, payload_kg: f64, to: &str, cfg: &EdlConfig, bodies: &crate::balance_config::BodiesConfig,
) -> Option<LandingVerdict> {
    let group = design.stage_groups.last()?;
    let dry: f64 = group.iter().map(|s| s.dry_mass_kg()).sum();
    landing_verdict_at_mass(design, dry + payload_kg, to, cfg, bodies)
}

/// `landing_verdict` at a given touchdown mass — a flight's, with
/// whatever is still aboard.
pub fn landing_verdict_at_mass(
    design: &RocketDesign, touchdown_mass_kg: f64, to: &str, cfg: &EdlConfig, bodies: &crate::balance_config::BodiesConfig,
) -> Option<LandingVerdict> {
    let props = DELTA_V_MAP.surface_properties(to)?;
    let stage = design.stage_groups.last()?.first()?;
    let excess = touchdown_excess_m_s2(stage, touchdown_mass_kg, props.gravity_m_s2);
    if !props.has_atmosphere {
        return Some(verdict_for_excess_on_legs(excess, stage.legs.is_some(), cfg));
    }
    // Through air: a landing burn finishes the job whatever the canopy
    // did, as on an airless body; otherwise the canopy sets the
    // touchdown speed and legs or water take it (25_2_EDL_CORE.md §2.3).
    if stage.landing.is_some() {
        return Some(verdict_for_excess_on_legs(excess, stage.legs.is_some(), cfg));
    }
    let Some(canopy) = stage.recovery.as_ref() else { return Some(LandingVerdict::NoWayDown) };
    let thin = props.atmosphere_density < cfg.subsonic_chute_min_density_kg_m3;
    if thin {
        if canopy.kind == crate::stage::RecoveryKind::Parachutes {
            return Some(LandingVerdict::ChuteCannotOpen { reason: "too thin for a subsonic canopy" });
        }
        // The body's face: an aeroshell's if it is rated for one, else
        // whatever the stage is.
        let area = stage.base_area_m2(bodies);
        if ballistic_coefficient(touchdown_mass_kg, area, cfg) > cfg.thin_air_chute_beta_limit_kg_m2 {
            return Some(LandingVerdict::ChuteCannotOpen { reason: "too dense to slow before the ground" });
        }
    }
    let speed = terminal_velocity_m_s(
        touchdown_mass_kg, canopy.drag_area_m2, cfg.chute_drag_coefficient, props.gravity_m_s2, props.atmosphere_density,
    );
    let (limit, ok) = if stage.legs.is_some() {
        (cfg.legs_touchdown_limit_m_s, LandingVerdict::UnderCanopy { speed_m_s: speed })
    } else if props.has_oceans {
        (cfg.splashdown_limit_m_s, LandingVerdict::Splashdown { speed_m_s: speed })
    } else {
        return Some(LandingVerdict::NoLegs);
    };
    let risk = touchdown_risk(speed, limit, cfg);
    Some(if risk > 0.0 { LandingVerdict::TooFast { speed_m_s: speed, limit_m_s: limit, risk } } else { ok })
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
    Some(verdict_for_excess_on_legs(touchdown_excess_m_s2(stage, dry, gravity), stage.legs.is_some(), cfg))
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
            landing: landing.map(|n| LandingBurn { engines: n, on: LocationId::of("lunar_surface") }), heat_shield: None, legs: None, recovery: None,
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

    /// The shield is solved from the heat, the density and the wall
    /// behind it (25_2_EDL_CORE.md §2.2): LEO return at β 300 needs tens
    /// of kg/m², a lunar return about twice that, a broad Mars lander a
    /// few; a steel wall takes more soak than aluminium, so the shield
    /// in front of it is lighter; over the rating the risk ramps to
    /// certainty.
    #[test]
    fn shields_are_solved_from_heat_and_the_wall() {
        use crate::structure::StructuralMaterial as M;
        let cfg = EdlConfig::default();
        let q = |v: f64, beta: f64| entry_heat_j_m2(v, beta, &cfg);
        assert!((q(7_800.0, 300.0) / 1e6 - 91.3).abs() < 1.0, "{}", q(7_800.0, 300.0) / 1e6);
        assert!(q(11_500.0, 300.0) > 2.5 * q(7_800.0, 300.0), "heat goes as v³");
        let steel = wall_heat_capacity_j_m2(20.0, M::Steel);
        let aluminium = wall_heat_capacity_j_m2(20.0, M::Aluminium);
        assert!(steel > 2.0 * aluminium, "{steel} vs {aluminium}");
        let leo_steel = shield_areal_density_kg_m2(q(8_000.0, 300.0), steel, &cfg);
        let leo_al = shield_areal_density_kg_m2(q(8_000.0, 300.0), aluminium, &cfg);
        assert!((20.0..35.0).contains(&leo_steel), "{leo_steel}");
        assert!(leo_al > leo_steel * 1.1, "{leo_al} vs {leo_steel}");
        let lunar_steel = shield_areal_density_kg_m2(q(11_500.0, 300.0), steel, &cfg);
        let lunar_al = shield_areal_density_kg_m2(q(11_500.0, 300.0), aluminium, &cfg);
        assert!((45.0..80.0).contains(&lunar_steel), "{lunar_steel}");
        eprintln!("shield kg/m²: LEO steel {leo_steel:.1} al {leo_al:.1}; lunar steel {lunar_steel:.1} al {lunar_al:.1}");
        assert!(lunar_al > lunar_steel, "steel saves shield at lunar speeds too");
        let mars = shield_areal_density_kg_m2(q(6_000.0, 150.0), steel, &cfg);
        assert!((2.0..12.0).contains(&mars), "{mars}");
        assert_eq!(shield_areal_density_kg_m2(1.0e6, 5.0e6, &cfg), 0.0, "a wall that takes it bare needs nothing");
        // Over the rating.
        assert_eq!(burn_through_risk(90.0, 100.0, &cfg), 0.0);
        assert!((burn_through_risk(125.0, 100.0, &cfg) - 0.5).abs() < 1e-9);
        assert_eq!(burn_through_risk(200.0, 100.0, &cfg), 1.0);
        assert_eq!(burn_through_risk(10.0, 0.0, &cfg), 1.0, "no shield at all");
    }

    /// Only the shielded top group aerobrakes, and the descent's entry
    /// speed is read off the first leg of the trailing aerobraked run.
    #[test]
    fn the_shielded_top_stage_aerobrakes_and_the_chain_sets_the_speed() {
        use crate::flight::FlightLeg;
        use crate::stage::{EntryRating, HeatShield};
        let mut design = crate::rocket::RocketDesign {
            id: crate::rocket::RocketDesignId(1), name: "Capsule".into(),
            stage_groups: vec![
                vec![lander(500_000.0, EngineCycle::GasGenerator, 1, None)],
                vec![lander(20_000.0, EngineCycle::PressureFed, 1, None)],
            ],
        };
        assert_eq!(aerobrake_leg(&design, 1, "leo", "earth_surface"), None, "no shield");
        design.stage_groups[1][0].heat_shield = Some(HeatShield {
            rating: EntryRating::LeoReturn, areal_density_kg_m2: 30.0, mass_kg: 100.0, rated_heat_j_m2: 1.0e8,
        });
        assert_eq!(aerobrake_leg(&design, 1, "leo", "earth_surface"), Some(7_800.0));
        assert_eq!(aerobrake_leg(&design, 0, "leo", "earth_surface"), None, "the booster is still attached");
        assert_eq!(aerobrake_leg(&design, 1, "leo", "gto"), None);

        let leg = |from: &str, to: &str, aerobraked: bool, speed: Option<f64>| FlightLeg {
            from: LocationId::of(from), to: LocationId::of(to), delta_v_cost: 0.0, burn_days: 0, coast_days: 0,
            aerobraked, entry_speed_m_s: speed,
        };
        let lunar_return = vec![
            leg("earth_surface", "leo", false, None),
            leg("leo", "lunar_orbit", false, None),
            leg("lunar_orbit", "leo", true, Some(11_000.0)),
            leg("leo", "earth_surface", true, Some(7_800.0)),
        ];
        assert_eq!(route_entry_speed(&lunar_return), Some(11_000.0));
        assert_eq!(route_entry_speed(&lunar_return[..2]), None);
    }

    /// Through air the canopy sets the touchdown: parachutes bring a
    /// capsule to a splashdown on Earth but cannot open in Mars's air,
    /// the supersonic canopy opens there but leaves it far too fast for
    /// legs, so a Mars lander burns the last of it; nothing at all is
    /// no way down (25_2_EDL_CORE.md §2.2–2.3).
    #[test]
    fn canopies_set_the_touchdown() {
        use crate::balance_config::BodiesConfig;
        use crate::stage::{LandingBurn, Legs, Recovery, RecoveryKind};
        let cfg = EdlConfig::default();
        let bodies = BodiesConfig::default();
        let mut capsule = lander(60_000.0, EngineCycle::PressureFed, 1, None);
        capsule.propellant_mass_kg = 400.0;
        let design_with = |stage: crate::stage::Stage| crate::rocket::RocketDesign {
            id: crate::rocket::RocketDesignId(1), name: "Capsule".into(), stage_groups: vec![vec![stage]],
        };
        assert_eq!(landing_verdict(&design_with(capsule.clone()), 0.0, "earth_surface", &cfg, &bodies), Some(LandingVerdict::NoWayDown));

        capsule.recovery = Some(Recovery { kind: RecoveryKind::Parachutes, rated_mass_kg: 0.0, mass_kg: 0.0, drag_area_m2: 0.0 });
        capsule.fit_edl(0.0, &cfg, &bodies);
        let canopy = capsule.recovery.clone().unwrap();
        assert!((canopy.rated_mass_kg - capsule.dry_mass_kg() - 400.0 + canopy.mass_kg).abs() < 1.0, "rated for the wet stage: {canopy:?}");
        assert!((canopy.mass_kg - cfg.parachutes_mass_fraction * canopy.rated_mass_kg).abs() < 1e-6);
        let earth = landing_verdict(&design_with(capsule.clone()), 0.0, "earth_surface", &cfg, &bodies).unwrap();
        match earth {
            LandingVerdict::Splashdown { speed_m_s } => assert!(speed_m_s < cfg.parachutes_design_speed_m_s && speed_m_s > 5.0, "{speed_m_s}"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            landing_verdict(&design_with(capsule.clone()), 0.0, "mars_surface", &cfg, &bodies),
            Some(LandingVerdict::ChuteCannotOpen { .. })
        ));
        // Legs on Earth under parachutes: 7 m/s against 3 is a sure loss.
        let mut legged = capsule.clone();
        legged.legs = Some(Legs { mass_kg: 50.0 });
        match landing_verdict(&design_with(legged), 0.0, "earth_surface", &cfg, &bodies).unwrap() {
            LandingVerdict::TooFast { risk, limit_m_s, .. } => { assert_eq!(risk, 1.0); assert_eq!(limit_m_s, cfg.legs_touchdown_limit_m_s); }
            other => panic!("{other:?}"),
        }

        // Mars: a lander in a broad aeroshell is light enough for the
        // supersonic canopy to open, at ~60 m/s; the burn and legs
        // finish it. A five-tonne brick is too dense for any canopy.
        use crate::stage::{EntryRating, HeatShield};
        let mut mars = capsule.clone();
        mars.structural_mass_kg = 800.0;
        mars.heat_shield = Some(HeatShield { rating: EntryRating::MarsOrbit, areal_density_kg_m2: 0.0, mass_kg: 0.0, rated_heat_j_m2: 0.0 });
        mars.recovery = Some(Recovery { kind: RecoveryKind::SupersonicChute, rated_mass_kg: 0.0, mass_kg: 0.0, drag_area_m2: 0.0 });
        mars.fit_edl(0.0, &cfg, &bodies);
        let beta = ballistic_coefficient(mars.dry_mass_kg(), mars.base_area_m2(&bodies), &cfg);
        assert!(beta < cfg.thin_air_chute_beta_limit_kg_m2, "β {beta}");
        match landing_verdict(&design_with(mars.clone()), 0.0, "mars_surface", &cfg, &bodies).unwrap() {
            LandingVerdict::NoLegs => {}
            other => panic!("no legs, no water on Mars: {other:?}"),
        }
        mars.legs = Some(Legs { mass_kg: 50.0 });
        match landing_verdict(&design_with(mars.clone()), 0.0, "mars_surface", &cfg, &bodies).unwrap() {
            LandingVerdict::TooFast { speed_m_s, risk, .. } => { assert!((30.0..70.0).contains(&speed_m_s), "{speed_m_s}"); assert_eq!(risk, 1.0); }
            other => panic!("{other:?}"),
        }
        mars.landing = Some(LandingBurn { engines: 1, on: LocationId::of("mars_surface") });
        assert!(landing_verdict(&design_with(mars), 0.0, "mars_surface", &cfg, &bodies).unwrap().can_land());
        let mut brick = capsule.clone();
        brick.structural_mass_kg = 5_000.0;
        brick.heat_shield = Some(HeatShield { rating: EntryRating::MarsOrbit, areal_density_kg_m2: 0.0, mass_kg: 0.0, rated_heat_j_m2: 0.0 });
        brick.recovery = Some(Recovery { kind: RecoveryKind::SupersonicChute, rated_mass_kg: 0.0, mass_kg: 0.0, drag_area_m2: 0.0 });
        brick.fit_edl(0.0, &cfg, &bodies);
        assert!(matches!(
            landing_verdict(&design_with(brick), 0.0, "mars_surface", &cfg, &bodies),
            Some(LandingVerdict::ChuteCannotOpen { reason }) if reason.contains("dense")
        ));
        // The physics helpers themselves.
        assert!((terminal_velocity_m_s(1_000.0, 409.0, 0.8, 9.81, 1.225) - 7.0).abs() < 0.1);
        assert_eq!(touchdown_risk(2.0, 3.0, &cfg), 0.0);
        assert!((touchdown_risk(4.5, 3.0, &cfg) - 0.5).abs() < 1e-9);
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
