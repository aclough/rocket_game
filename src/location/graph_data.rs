//! The inner-solar-system graph as data: every location, every
//! transfer and its price, built once into `DELTA_V_MAP`. Nothing here
//! is an algorithm; see `mod.rs` for the searches over it.

use super::*;

// ─── Builder helpers for the inner-solar-system graph ────────────────
// These are private to the module and exist so the long graph-construction
// function below stays readable.

fn loc_orbit(
    id: &'static str, display: &'static str, short: &'static str, parent: &'static str,
) -> Location {
    Location {
        id, display_name: display, short_name: short,
        location_type: LocationType::Orbit, parent_body: parent,
    }
}

fn loc_lagrange(
    id: &'static str, display: &'static str, short: &'static str, parent: &'static str,
) -> Location {
    Location {
        id, display_name: display, short_name: short,
        location_type: LocationType::LagrangePoint, parent_body: parent,
    }
}

#[allow(clippy::too_many_arguments)] // constructor-style, callers read positionally with names at the call site
fn loc_surface(
    id: &'static str, display: &'static str, short: &'static str, parent: &'static str,
    gravity: f64, radius: f64, has_atm: bool, atm_density: f64, ambient: f64, scale_height: f64,
) -> Location {
    Location {
        id, display_name: display, short_name: short,
        location_type: LocationType::Surface(SurfaceProperties {
            gravity_m_s2: gravity, radius_m: radius,
            has_atmosphere: has_atm, atmosphere_density: atm_density,
            ambient_pressure_pa: ambient, scale_height_m: scale_height,
        }),
        parent_body: parent,
    }
}

/// Push a high-thrust-only symmetric edge pair (a ↔ b).
fn add_impulsive_pair(
    transfers: &mut Vec<Transfer>,
    a: &'static str, b: &'static str, dv: f64, days: u32,
) {
    let make = |from, to| Transfer {
        from, to, delta_v: dv,
        through_atmosphere: false,
        can_aerobrake: false, transit_days: days,
        low_thrust_ok: false, low_thrust_delta_v: None,
    };
    transfers.push(make(a, b));
    transfers.push(make(b, a));
}

/// Push a low-thrust-friendly symmetric edge pair (a ↔ b).
/// `lt_dv = None` means low-thrust uses the same dv as high-thrust (small
/// burns where spiral inefficiency is negligible).
fn add_spiral_pair(
    transfers: &mut Vec<Transfer>,
    a: &'static str, b: &'static str, dv: f64, lt_dv: Option<f64>, days: u32,
) {
    let make = |from, to| Transfer {
        from, to, delta_v: dv,
        through_atmosphere: false,
        can_aerobrake: false, transit_days: days,
        low_thrust_ok: true, low_thrust_delta_v: lt_dv,
    };
    transfers.push(make(a, b));
    transfers.push(make(b, a));
}

/// Push a surface ↔ orbit pair with the same nominal dv both ways.
/// Ascent edge sets `through_atmosphere` (drag is added on top of dv);
/// descent edge sets `can_aerobrake` (flag for future aerobrake savings).
/// `lt_dv = Some(...)` allows low-thrust to use this pair (e.g. Bennu).
fn add_ground_pair(
    transfers: &mut Vec<Transfer>,
    surface: &'static str, orbit: &'static str, dv: f64, days: u32,
    has_atm: bool, lt_dv: Option<f64>,
) {
    let lt_ok = lt_dv.is_some();
    transfers.push(Transfer {
        from: surface, to: orbit, delta_v: dv,
        through_atmosphere: has_atm,
        can_aerobrake: false, transit_days: days,
        low_thrust_ok: lt_ok, low_thrust_delta_v: lt_dv,
    });
    transfers.push(Transfer {
        from: orbit, to: surface, delta_v: dv,
        through_atmosphere: false,
        can_aerobrake: has_atm, transit_days: days,
        low_thrust_ok: lt_ok, low_thrust_delta_v: lt_dv,
    });
}

/// Add a body's full side-branch off the heliocentric ladder:
/// `transfer ↔ capture ↔ orbit ↔ surface`.
/// All edges symmetric. The capture and orbit links are spiral-friendly;
/// the surface link is created by `add_ground_pair`.
#[allow(clippy::too_many_arguments)] // constructor-style, callers read positionally with names at the call site
fn add_body_branch(
    transfers: &mut Vec<Transfer>,
    transfer_node: &'static str, capture: &'static str,
    orbit: &'static str, surface: &'static str,
    capture_dv: f64, capture_days: u32,
    capture_to_orbit_dv: f64,
    orbit_to_surface_dv: f64,
    surface_has_atm: bool,
    surface_low_thrust: Option<f64>,
) {
    // transfer ↔ capture: heliocentric injection burn (large; spiral penalty 1.5x)
    add_spiral_pair(
        transfers, transfer_node, capture, capture_dv,
        Some(capture_dv * 1.5), capture_days,
    );
    // capture ↔ orbit: orbital insertion (small; same dv both classes)
    add_spiral_pair(transfers, capture, orbit, capture_to_orbit_dv, None, 0);
    // orbit ↔ surface: launch/landing
    add_ground_pair(
        transfers, surface, orbit, orbit_to_surface_dv, 0,
        surface_has_atm, surface_low_thrust,
    );
}

impl DeltaVMap {
    /// Build the inner-solar-system delta-v graph (Mercury through the
    /// asteroid belt, plus Earth/Moon and a couple of NEAs).
    pub fn earth_moon() -> Self {
        let locations = vec![
            // ─── Earth system ───
            loc_surface("earth_surface", "Earth Surface", "EARTH", "earth",
                9.81, 6_371_000.0, true, 1.225, 101_325.0, 8_500.0),
            loc_orbit("suborbital", "Suborbital", "SUB", "earth"),
            loc_orbit("leo", "Low Earth Orbit", "LEO", "earth"),
            loc_orbit("sso", "Sun-Synchronous Orbit", "SSO", "earth"),
            loc_orbit("meo", "Medium Earth Orbit", "MEO", "earth"),
            loc_orbit("gto", "Geostationary Transfer", "GTO", "earth"),
            loc_orbit("geo", "Geostationary Orbit", "GEO", "earth"),
            loc_orbit("earth_escape", "Earth Escape", "ESC", "sun"),
            loc_lagrange("l1", "Earth-Moon L1", "L1", "earth"),
            loc_lagrange("l2", "Earth-Moon L2", "L2", "earth"),
            loc_orbit("lunar_orbit", "Lunar Orbit", "LLO", "moon"),
            loc_surface("lunar_surface", "Lunar Surface", "MOON", "moon",
                1.62, 1_737_000.0, false, 0.0, 0.0, 0.0),
            // ─── Mercury ───
            loc_orbit("mercury_transfer", "Mercury Transfer", "MTRF", "sun"),
            loc_orbit("mercury_capture", "Mercury Capture", "MCAP", "mercury"),
            loc_orbit("mercury_orbit_100km", "Mercury 100km Orbit", "MORB", "mercury"),
            loc_surface("mercury_surface", "Mercury Surface", "MERC", "mercury",
                3.7, 2_440_000.0, false, 0.0, 0.0, 0.0),
            // ─── Venus (balloons at 1 bar instead of surface) ───
            loc_orbit("venus_transfer", "Venus Transfer", "VTRF", "sun"),
            loc_orbit("venus_capture", "Venus Capture", "VCAP", "venus"),
            loc_orbit("venus_orbit_400km", "Venus 400km Orbit", "VORB", "venus"),
            // Scale height at the 1-bar level (~50 km up, ~340 K, CO2).
            loc_surface("venus_balloons", "Venus 1bar Balloons", "VBAL", "venus",
                8.69, 6_101_800.0, true, 1.2, 100_000.0, 7_500.0),
            // ─── Mars + moons ───
            loc_orbit("mars_transfer", "Mars Transfer", "MARTR", "sun"),
            loc_orbit("mars_capture", "Mars Capture", "MARC", "mars"),
            loc_orbit("mars_orbit_200km", "Mars 200km Orbit", "MARO", "mars"),
            loc_surface("mars_surface", "Mars Surface", "MARS", "mars",
                3.71, 3_389_500.0, true, 0.020, 600.0, 11_100.0),
            loc_orbit("phobos_transfer", "Phobos Transfer", "PHTR", "mars"),
            loc_orbit("phobos_orbit", "Phobos Orbit", "PHOR", "phobos"),
            loc_surface("phobos_surface", "Phobos Surface", "PHOB", "phobos",
                0.0057, 11_000.0, false, 0.0, 0.0, 0.0),
            loc_orbit("deimos_transfer", "Deimos Transfer", "DETR", "mars"),
            loc_orbit("deimos_orbit", "Deimos Orbit", "DEOR", "deimos"),
            loc_surface("deimos_surface", "Deimos Surface", "DEIM", "deimos",
                0.003, 6_200.0, false, 0.0, 0.0, 0.0),
            // ─── Asteroid belt (Vesta, Ceres, Hygiea — Pallas skipped) ───
            loc_orbit("vesta_transfer", "Vesta Transfer", "VETR", "sun"),
            loc_orbit("vesta_capture", "Vesta Capture", "VECP", "vesta"),
            loc_orbit("vesta_orbit_20km", "Vesta 20km Orbit", "VEOR", "vesta"),
            loc_surface("vesta_surface", "Vesta Surface", "VEST", "vesta",
                0.25, 262_700.0, false, 0.0, 0.0, 0.0),
            loc_orbit("ceres_transfer", "Ceres Transfer", "CETR", "sun"),
            loc_orbit("ceres_capture", "Ceres Capture", "CECP", "ceres"),
            loc_orbit("ceres_orbit_20km", "Ceres 20km Orbit", "CEOR", "ceres"),
            loc_surface("ceres_surface", "Ceres Surface", "CERE", "ceres",
                0.27, 473_000.0, false, 0.0, 0.0, 0.0),
            loc_orbit("hygiea_transfer", "Hygiea Transfer", "HYTR", "sun"),
            loc_orbit("hygiea_capture", "Hygiea Capture", "HYCP", "hygiea"),
            loc_orbit("hygiea_orbit_20km", "Hygiea 20km Orbit", "HYOR", "hygiea"),
            loc_surface("hygiea_surface", "Hygiea Surface", "HYGI", "hygiea",
                0.13, 200_000.0, false, 0.0, 0.0, 0.0),
            // ─── NEAs (Eros, Bennu) ───
            loc_orbit("eros_transfer", "Eros Transfer", "ERTR", "sun"),
            loc_orbit("eros_capture", "Eros Capture", "ERCP", "eros"),
            loc_orbit("eros_orbit", "Eros Orbit", "EROR", "eros"),
            loc_surface("eros_surface", "Eros Surface", "EROS", "eros",
                0.0059, 16_840.0, false, 0.0, 0.0, 0.0),
            loc_orbit("bennu_transfer", "Bennu Transfer", "BNTR", "sun"),
            loc_orbit("bennu_capture", "Bennu Capture", "BNCP", "bennu"),
            loc_orbit("bennu_orbit", "Bennu Orbit", "BNOR", "bennu"),
            loc_surface("bennu_surface", "Bennu Surface", "BENN", "bennu",
                0.000060, 245.0, false, 0.0, 0.0, 0.0),
        ];

        let mut transfers: Vec<Transfer> = Vec::new();

        // ─── Earth surface launches ───
        // Suborbital is genuinely one-way (you fall back, no thrust needed),
        // so it stays asymmetric.
        transfers.push(Transfer {
            from: "earth_surface", to: "suborbital", delta_v: 3500.0,
            through_atmosphere: true,
            can_aerobrake: false, transit_days: 0,
            low_thrust_ok: false, low_thrust_delta_v: None,
        });
        // Earth surface ↔ LEO: same nominal dv both ways, drag on ascent only.
        add_ground_pair(&mut transfers, "earth_surface", "leo", 7800.0, 0, true, None);
        // Earth surface ↔ SSO. SSO is a launch destination, not somewhere you
        // transfer to: +150 m/s for the higher orbit (~700 km against ~200)
        // and +400 for throwing away the eastward rotation assist that a
        // low-inclination LEO launch gets free. Losses are charged separately
        // by the launch-site model, so this is ideal velocity like the 7800.
        add_ground_pair(&mut transfers, "earth_surface", "sso", 8500.0, 0, true, None);
        // Earth surface ↔ GTO. A commercial GTO mission injects off the ascent
        // and separates the same day; it does not stop in LEO. Slightly under
        // the 7800 + 2440 via-LEO sum because a direct ascent never
        // circularises at perigee altitude only to climb back out of it.
        add_ground_pair(&mut transfers, "earth_surface", "gto", 10_100.0, 0, true, None);

        // ─── Earth orbital climb (low-thrust climbs the ladder) ───
        // LEO ↔ SSO is a ~70° plane change, not a nudge: 2·v·sin(Δi/2) at
        // v ≈ 7600 m/s. Electric propulsion is *worse* here, not better —
        // Edelbaum's 2·v·sin(π/4·Δi) for a combined circular transfer. Both
        // are "launch a second rocket instead" numbers, which is the point;
        // the edge exists so a craft already in orbit has a sane answer
        // rather than being routed back through the ground and relaunched.
        add_spiral_pair(&mut transfers, "leo", "sso", 8700.0, Some(12_400.0), 0);
        add_spiral_pair(&mut transfers, "leo", "meo", 2100.0, Some(3500.0), 0);
        add_spiral_pair(&mut transfers, "meo", "geo", 2000.0, Some(2500.0), 0);
        add_spiral_pair(&mut transfers, "geo", "earth_escape", 700.0, Some(1500.0), 0);

        // ─── Earth high-thrust shortcuts (no low-thrust direct shortcuts to escape) ───
        add_impulsive_pair(&mut transfers, "leo", "gto", 2440.0, 1);
        // Circularising at GEO is an apogee-raising campaign, not a burn: a
        // few burns at successive apogees with the spacecraft coasting through
        // the belts on its own power in between. That coast is where a GEO
        // mission's endurance requirement comes from.
        add_impulsive_pair(&mut transfers, "gto", "geo", 1500.0, 3);
        add_impulsive_pair(&mut transfers, "leo", "lunar_orbit", 3850.0, 4);
        add_impulsive_pair(&mut transfers, "lunar_orbit", "earth_escape", 93.0, 4);

        // ─── Lagrange points and lunar surface ───
        add_spiral_pair(&mut transfers, "leo", "l1", 3150.0, None, 5);
        add_spiral_pair(&mut transfers, "l1", "lunar_orbit", 700.0, None, 2);
        add_spiral_pair(&mut transfers, "leo", "l2", 3200.0, None, 5);
        add_spiral_pair(&mut transfers, "l2", "lunar_orbit", 800.0, None, 2);
        add_ground_pair(&mut transfers, "lunar_surface", "lunar_orbit", 1700.0, 0, false, None);

        // ─── Heliocentric backbone (Hohmann ladder) ───
        add_spiral_pair(&mut transfers, "mercury_transfer", "venus_transfer",
            2085.0, Some(2085.0 * 1.5), 50);
        add_spiral_pair(&mut transfers, "venus_transfer", "earth_escape",
            280.0, Some(280.0 * 1.5), 100);
        add_spiral_pair(&mut transfers, "earth_escape", "mars_transfer",
            388.0, Some(388.0 * 1.5), 200);
        add_spiral_pair(&mut transfers, "mars_transfer", "vesta_transfer",
            923.0, Some(923.0 * 1.5), 100);
        add_spiral_pair(&mut transfers, "vesta_transfer", "ceres_transfer",
            379.0, Some(379.0 * 1.5), 100);
        add_spiral_pair(&mut transfers, "ceres_transfer", "hygiea_transfer",
            570.0, Some(570.0 * 1.5), 100);

        // ─── NEA branches off Earth escape (not on the planetary ladder) ───
        add_spiral_pair(&mut transfers, "earth_escape", "eros_transfer",
            600.0, Some(900.0), 200);
        add_spiral_pair(&mut transfers, "earth_escape", "bennu_transfer",
            400.0, Some(600.0), 150);

        // ─── Body branches: transfer → capture → orbit → surface ───
        // Mercury (no atmosphere)
        add_body_branch(&mut transfers, "mercury_transfer", "mercury_capture",
            "mercury_orbit_100km", "mercury_surface",
            3062.0, 30, 1220.0, 6310.0, false, None);
        // Venus (1bar balloons instead of surface; descent through atmosphere)
        add_body_branch(&mut transfers, "venus_transfer", "venus_capture",
            "venus_orbit_400km", "venus_balloons",
            359.0, 30, 2939.0, 1500.0, true, None);
        // Mars (atmosphere)
        add_body_branch(&mut transfers, "mars_transfer", "mars_capture",
            "mars_orbit_200km", "mars_surface",
            673.0, 30, 3578.0, 4100.0, true, None);
        // Vesta / Ceres / Hygiea (no atmosphere, large injection burns)
        add_body_branch(&mut transfers, "vesta_transfer", "vesta_capture",
            "vesta_orbit_20km", "vesta_surface",
            4096.0, 30, 102.0, 173.0, false, None);
        add_body_branch(&mut transfers, "ceres_transfer", "ceres_capture",
            "ceres_orbit_20km", "ceres_surface",
            4381.0, 30, 148.0, 280.0, false, None);
        add_body_branch(&mut transfers, "hygiea_transfer", "hygiea_capture",
            "hygiea_orbit_20km", "hygiea_surface",
            4915.0, 30, 63.0, 139.0, false, None);
        // Eros (NEA, tiny gravity but high-thrust only)
        add_body_branch(&mut transfers, "eros_transfer", "eros_capture",
            "eros_orbit", "eros_surface",
            30.0, 30, 5.0, 10.0, false, None);
        // Bennu (NEA, gravity so low ion drives can land)
        add_body_branch(&mut transfers, "bennu_transfer", "bennu_capture",
            "bennu_orbit", "bennu_surface",
            20.0, 30, 5.0, 5.0, false, Some(5.0));

        // ─── Mars moons (Phobos and Deimos branch off mars_capture) ───
        // Phobos: tiny gravity but ion-thrust margin too small per the
        // planning rule — keep landing high-thrust only.
        add_spiral_pair(&mut transfers, "mars_capture", "phobos_transfer", 535.0, None, 1);
        add_spiral_pair(&mut transfers, "phobos_transfer", "phobos_orbit", 3.0, None, 0);
        add_ground_pair(&mut transfers, "phobos_surface", "phobos_orbit",
            6.0, 0, false, None);
        add_spiral_pair(&mut transfers, "mars_capture", "deimos_transfer", 649.0, None, 1);
        add_spiral_pair(&mut transfers, "deimos_transfer", "deimos_orbit", 2.0, None, 0);
        add_ground_pair(&mut transfers, "deimos_surface", "deimos_orbit",
            4.0, 0, false, None);

        DeltaVMap {
            locations,
            transfers,
        }
    }
}
