//! Ideal De Laval nozzle relations (19_NOZZLES.md step 1).
//!
//! One-dimensional, isentropic, frozen-flow gas dynamics: the thrust
//! coefficient `C_F` as a function of expansion ratio, heat-capacity
//! ratio and ambient pressure. Everything the game derives about a
//! bell — the vacuum Isp gain of a longer nozzle, the sea-level Isp
//! penalty, the bell size a chamber pressure allows, the ambient
//! pressure at which thrust would fall to zero — comes from these
//! relations, and they are pinned to real engines in the tests
//! (Merlin, F-1, RD-180, RS-25, Vulcain 2, Raptor: sea-level Isp
//! within 2 % from chamber pressure and expansion ratio alone).
//!
//! Callers: `EngineDesign` (`set_nozzle`, `with_vacuum_bell`,
//! `atmosphere_response`), `EngineBaseline::design`, and the ascent
//! integrator through [`AtmosphereResponse`].
//!
//! Notation: `ε` expansion ratio (exit area / throat area), `γ`
//! heat-capacity ratio, `p_c` chamber pressure, `p_e` exit pressure,
//! `p_a` ambient pressure. `Isp = C_F · c* / g0`, where `c*` depends
//! only on the propellant and chamber, not on the bell.

/// Vandenkerckhove function Γ(γ): the mass-flow constant of a choked
/// throat, `√γ · (2/(γ+1))^((γ+1)/(2(γ−1)))`.
pub fn vandenkerckhove(gamma: f64) -> f64 {
    gamma.sqrt() * (2.0 / (gamma + 1.0)).powf((gamma + 1.0) / (2.0 * (gamma - 1.0)))
}

/// Expansion ratio that expands the flow to exit pressure `pe_over_pc`
/// (as a fraction of chamber pressure). The area-ratio relation
/// `ε = Γ / √( (2γ/(γ−1)) · r^(2/γ) · (1 − r^((γ−1)/γ)) )`, `r = p_e/p_c`.
pub fn expansion_ratio_for_pressure_ratio(pe_over_pc: f64, gamma: f64) -> f64 {
    let r = pe_over_pc.clamp(1e-9, 0.5);
    let g = gamma;
    let inner = (2.0 * g / (g - 1.0)) * r.powf(2.0 / g) * (1.0 - r.powf((g - 1.0) / g));
    vandenkerckhove(g) / inner.sqrt()
}

/// Exit pressure as a fraction of chamber pressure for a bell of
/// expansion ratio `eps`: the inverse of
/// [`expansion_ratio_for_pressure_ratio`], by bisection (the relation
/// is monotonic for the supersonic branch, `r < 0.5`).
pub fn exit_pressure_ratio(eps: f64, gamma: f64) -> f64 {
    let eps = eps.max(1.0);
    // Supersonic branch: ε falls as r rises towards the throat value.
    let (mut lo, mut hi) = (1e-9_f64, 0.5_f64);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if expansion_ratio_for_pressure_ratio(mid, gamma) > eps {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-13 {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Vacuum thrust coefficient of a bell of expansion ratio `eps`:
/// `√( (2γ²/(γ−1)) · (2/(γ+1))^((γ+1)/(γ−1)) · (1 − r^((γ−1)/γ)) ) + r·ε`.
pub fn thrust_coefficient_vacuum(eps: f64, gamma: f64) -> f64 {
    let g = gamma;
    let r = exit_pressure_ratio(eps, g);
    let momentum = ((2.0 * g * g / (g - 1.0))
        * (2.0 / (g + 1.0)).powf((g + 1.0) / (g - 1.0))
        * (1.0 - r.powf((g - 1.0) / g)))
        .sqrt();
    momentum + r * eps
}

/// Thrust coefficient at ambient pressure `pa_over_pc` (as a fraction
/// of chamber pressure): the vacuum figure less the back-pressure term
/// `ε · p_a / p_c`. Negative when the ambient would push the flow back
/// up the bell; callers clamp.
pub fn thrust_coefficient(eps: f64, gamma: f64, pa_over_pc: f64) -> f64 {
    thrust_coefficient_vacuum(eps, gamma) - eps * pa_over_pc
}

/// The ambient pressure at which a bell's thrust would reach zero,
/// `p_c · C_F,vac / ε`. Thrust (and Isp) at any ambient is then the
/// linear `1 − p_a / p_zero` of the vacuum figure — one constant per
/// nozzle for the ascent's step loop.
pub fn zero_thrust_pressure_pa(chamber_pressure_pa: f64, eps: f64, gamma: f64) -> f64 {
    chamber_pressure_pa * thrust_coefficient_vacuum(eps, gamma) / eps
}

/// Fraction of vacuum thrust (equivalently Isp) a nozzle retains at
/// `ambient_pa`, given its [`zero_thrust_pressure_pa`]. 1.0 in vacuum,
/// 0.0 at or beyond the zero-thrust pressure.
pub fn thrust_fraction(zero_thrust_pressure_pa: f64, ambient_pa: f64) -> f64 {
    if ambient_pa <= 0.0 || zero_thrust_pressure_pa <= 0.0 {
        return 1.0;
    }
    (1.0 - ambient_pa / zero_thrust_pressure_pa).clamp(0.0, 1.0)
}

/// Throat area of an engine producing `vacuum_thrust_n` at
/// `chamber_pressure_pa` through a bell of expansion ratio `eps`:
/// `A_t = F / (C_F,vac · p_c)`.
pub fn throat_area_m2(vacuum_thrust_n: f64, chamber_pressure_pa: f64, eps: f64, gamma: f64) -> f64 {
    vacuum_thrust_n / (thrust_coefficient_vacuum(eps, gamma) * chamber_pressure_pa)
}

/// Exit area of a bell: `ε · A_t`.
pub fn exit_area_m2(throat_area_m2: f64, eps: f64) -> f64 {
    throat_area_m2 * eps
}

/// Diameter of a circular exit of the given area.
pub fn exit_diameter_m(exit_area_m2: f64) -> f64 {
    2.0 * (exit_area_m2 / std::f64::consts::PI).sqrt()
}

/// The largest expansion ratio a bell can have and still fit an exit
/// of `max_exit_diameter_m`, capped at `max_eps`. Never below `eps_min`
/// (the bell it is extending from).
pub fn expansion_ratio_for_exit_diameter(
    throat_area_m2: f64, max_exit_diameter_m: f64, max_eps: f64, eps_min: f64,
) -> f64 {
    let max_exit_area = std::f64::consts::PI * (max_exit_diameter_m / 2.0).powi(2);
    (max_exit_area / throat_area_m2).min(max_eps).max(eps_min)
}

/// Mass of the extra bell that takes a nozzle from `eps_from` to
/// `eps_to`: the added exit area at `areal_density_kg_m2`. Zero when
/// the bell is not extended.
pub fn bell_extension_mass_kg(
    throat_area_m2: f64, eps_from: f64, eps_to: f64, areal_density_kg_m2: f64,
) -> f64 {
    (throat_area_m2 * (eps_to - eps_from)).max(0.0) * areal_density_kg_m2
}

/// Expansion ratio of a bell whose exit pressure is `exit_pressure_pa`
/// at chamber pressure `chamber_pressure_pa`.
pub fn expansion_ratio_for_exit_pressure(chamber_pressure_pa: f64, exit_pressure_pa: f64, gamma: f64) -> f64 {
    expansion_ratio_for_pressure_ratio(exit_pressure_pa / chamber_pressure_pa, gamma)
}

/// How an engine's thrust answers to the air around it: the one thing
/// the ascent integrator asks of a thruster per step. Built once per
/// design (`EngineDesign::atmosphere_response`); the per-step call is
/// arithmetic only. A later `AirBreathing` or `PulseUnit` variant lands
/// here (19_NOZZLES.md §3, the seam toward Option C).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AtmosphereResponse {
    /// Ambient pressure does nothing: electric thrusters, sails, or a
    /// design with no nozzle data.
    None,
    /// A De Laval bell: thrust falls linearly with ambient pressure,
    /// reaching zero at `zero_thrust_pressure_pa`.
    Nozzle { zero_thrust_pressure_pa: f64 },
}

impl AtmosphereResponse {
    /// Fraction of vacuum thrust (and Isp) delivered at `pressure_pa`.
    pub fn thrust_fraction(&self, pressure_pa: f64) -> f64 {
        match *self {
            AtmosphereResponse::None => 1.0,
            AtmosphereResponse::Nozzle { zero_thrust_pressure_pa } => {
                thrust_fraction(zero_thrust_pressure_pa, pressure_pa)
            }
        }
    }

    /// The ambient pressure below which this thruster's loss is under a
    /// tenth of a percent — where an integrator may stop looking the
    /// pressure up. Infinite for a thruster the air never touches.
    pub fn negligible_pressure_pa(&self) -> f64 {
        match *self {
            AtmosphereResponse::None => f64::INFINITY,
            AtmosphereResponse::Nozzle { zero_thrust_pressure_pa } => 1e-3 * zero_thrust_pressure_pa,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: f64 = 1.2;
    const PA: f64 = 101_325.0;
    const BAR: f64 = 100_000.0;

    fn close(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs()
    }

    #[test]
    fn area_ratio_relation_inverts() {
        for eps in [2.0, 8.0, 16.0, 40.0, 69.0, 165.0, 280.0] {
            let r = exit_pressure_ratio(eps, G);
            let back = expansion_ratio_for_pressure_ratio(r, G);
            assert!(close(back, eps, 1e-6), "eps {eps}: r {r} -> {back}");
        }
    }

    #[test]
    fn vacuum_thrust_coefficient_rises_with_expansion_and_flattens() {
        let cf16 = thrust_coefficient_vacuum(16.0, G);
        // Relative gains from the plan's §1 table.
        for (eps, gain) in [(40.0, 1.049), (100.0, 1.086), (150.0, 1.100), (200.0, 1.110), (280.0, 1.120)] {
            let g = thrust_coefficient_vacuum(eps, G) / cf16;
            assert!(close(g, gain, 0.005), "eps {eps}: gain {g}, expected {gain}");
        }
        // Diminishing returns: each doubling adds less than the last.
        let d1 = thrust_coefficient_vacuum(32.0, G) - cf16;
        let d2 = thrust_coefficient_vacuum(64.0, G) - thrust_coefficient_vacuum(32.0, G);
        let d3 = thrust_coefficient_vacuum(128.0, G) - thrust_coefficient_vacuum(64.0, G);
        assert!(d1 > d2 && d2 > d3);
    }

    /// Real engines: chamber pressure and expansion ratio predict the
    /// sea-level Isp from the vacuum figure within 2 %.
    #[test]
    fn sea_level_isp_of_real_engines() {
        let cases: &[(&str, f64, f64, f64, f64)] = &[
            // name, p_c bar, eps, vacuum Isp, real sea-level Isp
            ("Merlin 1D", 97.0, 16.0, 311.0, 282.0),
            ("F-1", 70.0, 16.0, 304.0, 263.0),
            ("RD-180", 257.0, 36.9, 338.0, 311.0),
            ("RS-25", 206.0, 69.0, 452.0, 366.0),
            ("Vulcain 2", 117.0, 58.0, 431.0, 318.0),
            ("Raptor 2", 300.0, 40.0, 350.0, 327.0),
        ];
        for &(name, pc, eps, isp_vac, isp_sl) in cases {
            let frac = thrust_coefficient(eps, G, PA / (pc * BAR)) / thrust_coefficient_vacuum(eps, G);
            let predicted = isp_vac * frac;
            assert!(close(predicted, isp_sl, 0.02), "{name}: predicted {predicted:.1}, real {isp_sl}");
            // The one-constant form the ascent uses agrees with the full formula.
            let pz = zero_thrust_pressure_pa(pc * BAR, eps, G);
            assert!(close(thrust_fraction(pz, PA), frac, 1e-9), "{name}: zero-thrust form");
        }
    }

    #[test]
    fn vacuum_bell_gain_of_merlin_to_mvac() {
        let gain = thrust_coefficient_vacuum(165.0, G) / thrust_coefficient_vacuum(16.0, G);
        // Real: 348 / 311 = 1.119. Ideal frozen flow gives a little less.
        assert!(gain > 1.09 && gain < 1.13, "gain {gain}");
    }

    /// Summerfield: a sea-level bell expands to ~0.4 atm. Its expansion
    /// ratio then follows from chamber pressure alone.
    #[test]
    fn sea_level_expansion_ratio_from_chamber_pressure() {
        for (pc_bar, eps_sl) in [(15.0, 5.6), (45.0, 12.9), (60.0, 16.0), (90.0, 21.9), (200.0, 40.9), (300.0, 56.4)] {
            let eps = expansion_ratio_for_pressure_ratio(0.4 * PA / (pc_bar * BAR), G);
            assert!(close(eps, eps_sl, 0.01), "{pc_bar} bar: eps {eps}, expected {eps_sl}");
        }
    }

    /// Exit diameters of real vacuum bells from thrust, chamber
    /// pressure and expansion ratio.
    #[test]
    fn exit_diameters_of_real_vacuum_bells() {
        for (name, thrust, pc_bar, eps, real_d) in [
            ("MVac", 934_000.0, 97.0, 165.0, 3.3),
            ("RVac", 2_500_000.0, 300.0, 90.0, 2.4),
            ("RL10B-2", 110_000.0, 44.0, 280.0, 2.15),
        ] {
            let at = throat_area_m2(thrust, pc_bar * BAR, eps, G);
            let d = exit_diameter_m(exit_area_m2(at, eps));
            assert!(close(d, real_d, 0.08), "{name}: diameter {d:.2}, real {real_d}");
        }
    }

    #[test]
    fn exit_diameter_limit_bounds_the_vacuum_bell() {
        // Merlin-class throat: a 3 m exit allows ~ε 130; capped at 300; never below the SL bell.
        let at = throat_area_m2(914_000.0, 97.0 * BAR, 16.0, G);
        let eps = expansion_ratio_for_exit_diameter(at, 3.0, 300.0, 16.0);
        assert!(eps > 120.0 && eps < 140.0, "eps {eps}");
        // A tiny throat hits the cap.
        assert_eq!(expansion_ratio_for_exit_diameter(0.001, 3.0, 300.0, 16.0), 300.0);
        // A huge throat cannot go below its own sea-level bell.
        assert_eq!(expansion_ratio_for_exit_diameter(10.0, 3.0, 300.0, 16.0), 16.0);
    }

    #[test]
    fn bell_extension_mass_matches_mvac() {
        let at = throat_area_m2(914_000.0, 97.0 * BAR, 16.0, G);
        // ~8 m² of extra bell at 16 kg/m² ≈ 130 kg (MVac over Merlin 1D).
        let m = bell_extension_mass_kg(at, 16.0, 165.0, 16.0);
        assert!(m > 110.0 && m < 150.0, "mass {m}");
        assert_eq!(bell_extension_mass_kg(at, 165.0, 16.0, 16.0), 0.0);
    }

    #[test]
    fn atmosphere_response() {
        let none = AtmosphereResponse::None;
        assert_eq!(none.thrust_fraction(PA), 1.0);
        assert_eq!(none.negligible_pressure_pa(), f64::INFINITY);
        let bell = AtmosphereResponse::Nozzle { zero_thrust_pressure_pa: 1_000_000.0 };
        assert!(close(bell.thrust_fraction(PA), 1.0 - PA / 1e6, 1e-12));
        assert_eq!(bell.negligible_pressure_pa(), 1_000.0);
        // Round trip: the exit pressure a bell was built to comes back.
        let eps = expansion_ratio_for_exit_pressure(97.0 * BAR, 40_000.0, G);
        assert!(close(exit_pressure_ratio(eps, G) * 97.0 * BAR, 40_000.0, 1e-6));
    }

    #[test]
    fn thrust_fraction_edges() {
        assert_eq!(thrust_fraction(50_000.0, 0.0), 1.0);
        assert_eq!(thrust_fraction(0.0, PA), 1.0);
        assert_eq!(thrust_fraction(50_000.0, 100_000.0), 0.0);
        assert!(close(thrust_fraction(100_000.0, 25_000.0), 0.75, 1e-12));
    }
}
