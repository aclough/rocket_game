//! Capability probe for balance work (`17_2_DELTA_V.md` step 4 and on):
//! prints `max_payload_to` from Earth for the sim bot's template and for
//! every rocket in the save corpus, so a change to the planner's
//! delta-v budget can be compared before and after. Ignored; run with
//!
//! ```text
//! cargo test --release --test capability_probe -- --ignored --nocapture
//! ```
//!
//! Not a regression test: the numbers it prints are the thing being
//! moved, and the plan records them.

use std::path::PathBuf;

use rocket_tycoon::game_state::GameState;
use rocket_tycoon::policy::{BasicPolicy, CompanyPolicy};
use rocket_tycoon::rocket::{DesignPerformance, RocketDesign};
use rocket_tycoon::rocket_perf::max_payload_to;
use rocket_tycoon::save;

const DESTINATIONS: [&str; 4] = ["leo", "gto", "geo", "lunar_orbit"];

fn report(label: &str, design: &RocketDesign) {
    let caps: Vec<String> = DESTINATIONS.iter()
        .map(|to| format!("{to} {:>6.0}", max_payload_to(design, "earth_surface", to)))
        .collect();
    let perf = DesignPerformance::compute(design, 1_000.0, "earth_surface");
    println!(
        "{label:<28} {}   | at 1 t: planner {:>6.0}  vac {:>6.0}  grav {:>5.0}  overexp {:>4.0}  drag {:>4.0}",
        caps.join("  "),
        perf.total_planner_dv(),
        perf.total_vacuum_dv(),
        perf.ascent.gravity_by_group.iter().sum::<f64>(),
        perf.ascent.overexpansion,
        perf.ascent.drag,
    );
    let ascent = rocket_tycoon::rocket::design_ascent(design, 1_000.0, "earth_surface");
    for (gi, (g, a)) in perf.groups.iter().zip(&ascent).enumerate() {
        println!(
            "    group {gi}: vac {:>6.0}  grav {:>5.0}  isp frac {:.3} (pad {:.3})  nozzle loss {:>4.0}  burnout alt {:>6.1} km  twr {:.2}  burn {:>4.0} s",
            g.delta_v_vacuum, g.gravity_loss, a.isp_fraction,
            design.stage_groups[gi][0].engine.isp_fraction_at(101_325.0),
            g.overexpansion_loss, a.burnout_altitude_m / 1000.0, g.twr, g.burn_time_s,
        );
    }
}

#[test]
#[ignore = "prints capability figures for balance work; run with --ignored --nocapture"]
fn capability_probe() {
    // The bot's template, as the step 0 record measured it (seed 42, day 730).
    let mut policy = BasicPolicy::new();
    let mut game = GameState::new("Probe".into(), 200_000_000.0, 42);
    for _ in 0..730 {
        policy.act(&mut game);
        game.advance_day();
    }
    for rp in &game.player_company.rocket_projects {
        report(&format!("bot template {}", rp.design.name), &rp.design);
    }

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/saves");
    for era in ["m1", "m2", "m3", "m4", "m5"] {
        let path = dir.join(format!("{era}.json"));
        let state = save::load_game(&path)
            .unwrap_or_else(|e| panic!("failed to load {}: {e}", path.display()));
        for rp in &state.player_company.rocket_projects {
            report(&format!("{era} {}", rp.design.name), &rp.design);
        }
    }
}

/// Cost of one `DesignPerformance::compute` and one `max_payload_to`
/// for the bot's template — the two calls every capability question
/// bottoms out in. Prints microseconds per call.
#[test]
#[ignore = "timing probe; run with --ignored --nocapture"]
fn compute_timing() {
    let mut policy = BasicPolicy::new();
    let mut game = GameState::new("Probe".into(), 200_000_000.0, 42);
    for _ in 0..730 {
        policy.act(&mut game);
        game.advance_day();
    }
    let design = game.player_company.rocket_projects[0].design.clone();
    let payloads: Vec<f64> = (0..20).map(|i| 200.0 + 150.0 * i as f64).collect();
    let t = std::time::Instant::now();
    let mut sink = 0.0;
    for _ in 0..200 {
        for &p in &payloads {
            sink += DesignPerformance::compute(&design, p, "earth_surface").total_planner_dv();
        }
    }
    let per_compute = t.elapsed().as_secs_f64() * 1e6 / (200.0 * payloads.len() as f64);
    let t = std::time::Instant::now();
    for _ in 0..20 {
        for to in DESTINATIONS {
            sink += max_payload_to(&design, "earth_surface", to);
        }
    }
    let per_cap = t.elapsed().as_secs_f64() * 1e6 / (20.0 * DESTINATIONS.len() as f64);
    println!("TIMING compute {per_compute:.1} us/call   max_payload_to {per_cap:.0} us/call   (sink {sink:.0})");
}

/// The ascent models side by side (17_3_PHYSICS.md D7): staging
/// altitude and gravity loss per group for a Falcon 9 class vehicle, a
/// Space Shuttle stack and a PSLV core under the pre-D7 gravity turn and
/// the pitch program, plus the bot's template and the early corpus
/// rocket under the default.
#[test]
#[ignore = "ascent model comparison; run with --ignored --nocapture"]
fn ascent_models() {
    use rocket_tycoon::location::DELTA_V_MAP;
    use rocket_tycoon::rocket::design_ascent;
    let mut policy = BasicPolicy::new();
    let mut game = GameState::new("Probe".into(), 200_000_000.0, 42);
    for _ in 0..730 {
        policy.act(&mut game);
        game.advance_day();
    }
    let bot = game.player_company.rocket_projects[0].design.clone();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/saves");
    let m1 = save::load_game(&dir.join("m1.json")).unwrap().player_company.rocket_projects[0].design.clone();
    use rocket_tycoon::location::{AscentProfile, DEFAULT_ASCENT_PROFILE, LEGACY_GRAVITY_TURN};
    let profiles: Vec<(&str, AscentProfile)> = vec![
        ("gravity turn (pre-D7)", LEGACY_GRAVITY_TURN),
        ("pitch program (default)", DEFAULT_ASCENT_PROFILE),
    ];
    let earth = DELTA_V_MAP.surface_properties("earth_surface").unwrap();
    // Falcon 9 v1.2 as the anchor test flies it.
    let f9 = || {
        let s1_thrust = 8_000_000.0; let s1_flow = s1_thrust / (300.0 * 9.80665);
        let s2_thrust = 934_000.0; let s2_flow = s2_thrust / (348.0 * 9.80665);
        let mass = 411_000.0 + 22_200.0 + 107_500.0 + 4_000.0 + 15_600.0;
        let groups = vec![
            vec![rocket_tycoon::location::AscentPhase { thrust_n: s1_thrust, mass_flow_kg_s: s1_flow, propellant_kg: 411_000.0, dry_mass_dropped_kg: 22_200.0, nozzles: Vec::new() }],
            vec![rocket_tycoon::location::AscentPhase { thrust_n: s2_thrust, mass_flow_kg_s: s2_flow, propellant_kg: 107_500.0, dry_mass_dropped_kg: 4_000.0, nozzles: Vec::new() }],
        ];
        (groups, mass)
    };
    use rocket_tycoon::location::AscentPhase as P;
    let ph = |thrust: f64, isp: f64, prop: f64, dry: f64| P { thrust_n: thrust, mass_flow_kg_s: thrust / (isp * 9.80665), propellant_kg: prop, dry_mass_dropped_kg: dry, nozzles: Vec::new() };
    // Space Shuttle: one group — 3 SSMEs + 2 SRBs for 124 s, then the core alone to MECO; orbiter 78 t + 25 t payload.
    let shuttle = || {
        let ssme = 3.0 * 2_280_000.0; let ssme_flow = ssme / (452.0 * 9.80665);
        let srb = 2.0 * 14_000_000.0; let srb_flow = 1_000_000.0 / 124.0;
        let et_in_a = ssme_flow * 124.0;
        let a = P { thrust_n: ssme + srb, mass_flow_kg_s: ssme_flow + srb_flow, propellant_kg: 1_000_000.0 + et_in_a, dry_mass_dropped_kg: 182_000.0, nozzles: Vec::new() };
        let b = P { thrust_n: ssme, mass_flow_kg_s: ssme_flow, propellant_kg: 730_000.0 - et_in_a, dry_mass_dropped_kg: 26_500.0, nozzles: Vec::new() };
        let mass = 1_000_000.0 + 730_000.0 + 182_000.0 + 26_500.0 + 78_000.0 + 25_000.0;
        (vec![vec![a, b]], mass)
    };
    // PSLV core: four stages, 1.5 t payload.
    let pslv = || {
        let g = vec![vec![ph(4_800_000.0, 237.0, 138_000.0, 30_000.0)], vec![ph(800_000.0, 293.0, 41_500.0, 5_300.0)],
                     vec![ph(240_000.0, 295.0, 7_600.0, 1_100.0)], vec![ph(14_600.0, 308.0, 2_500.0, 900.0)]];
        let mass = 138_000.0 + 30_000.0 + 41_500.0 + 5_300.0 + 7_600.0 + 1_100.0 + 2_500.0 + 900.0 + 1_500.0;
        (g, mass)
    };
    let fmt = |r: &[rocket_tycoon::location::AscentGroupResult]| -> String {
        r.iter().enumerate().map(|(i, g)| format!("G{i} {:>5.0} @ {:>6.1} km", g.gravity_loss, g.burnout_altitude_m / 1000.0)).collect::<Vec<_>>().join("  ")
            + &format!("   total {:>5.0}", r.iter().map(|g| g.gravity_loss).sum::<f64>())
    };
    for (label, profile) in &profiles {
        println!("── {label:<24} {profile:?}");
        let (groups, mass) = f9();
        let r = rocket_tycoon::location::simulate_ascent_with(profile, earth, &groups, mass);
        println!("    Falcon 9 (twr 1.4):  {}", fmt(&r));
        let (groups, mass) = shuttle();
        let r = rocket_tycoon::location::simulate_ascent_with(profile, earth, &groups, mass);
        println!("    Shuttle (1 group):   {}", fmt(&r));
        let (groups, mass) = pslv();
        let r = rocket_tycoon::location::simulate_ascent_with(profile, earth, &groups, mass);
        println!("    PSLV (4 stages):     {}", fmt(&r));
        // Apollo LM ascent stage off the Moon (15.6 kN, Isp 311, 2.35 t propellant, 2.15 t dry; lunar TWR 2.0)
        // and a Mars ascent vehicle (100 kN, Isp 350, 15 t propellant, 3 t dry; Mars TWR 1.5).
        let moon = DELTA_V_MAP.surface_properties("lunar_surface").unwrap();
        let mars = DELTA_V_MAP.surface_properties("mars_surface").unwrap();
        let lm = vec![vec![ph(15_600.0, 311.0, 2_353.0, 2_150.0)]];
        let r = rocket_tycoon::location::simulate_ascent_with(profile, moon, &lm, 4_503.0);
        println!("    LM ascent (Moon):    {}   (stage dv {:.0}, lunar orbital {:.0})", fmt(&r), 311.0 * 9.80665 * (4_503.0f64 / 2_150.0).ln(), moon.orbital_velocity());
        let mav = vec![vec![ph(100_000.0, 350.0, 15_000.0, 3_000.0)]];
        let r = rocket_tycoon::location::simulate_ascent_with(profile, mars, &mav, 18_000.0);
        println!("    MAV (Mars):          {}   (stage dv {:.0}, mars orbital {:.0})", fmt(&r), 350.0 * 9.80665 * (18_000.0f64 / 3_000.0).ln(), mars.orbital_velocity());
        if *profile != DEFAULT_ASCENT_PROFILE {
            continue; // designs are flown with the default profile only
        }
        for (label, design) in [("bot template", &bot), ("m1 rocket", &m1)] {
            let r = design_ascent(design, 1_000.0, "earth_surface");
            let leo = max_payload_to(design, "earth_surface", "leo");
            let geo = max_payload_to(design, "earth_surface", "geo");
            let perf = DesignPerformance::compute(design, 1_000.0, "earth_surface");
            println!("    {label:<20} S1 grav {:>5.0} @ {:>5.1} km (isp {:.3})  S2 grav {:>5.0} @ {:>6.1} km   planner {:>6.0}   LEO {:>5.0} kg  GEO {:>4.0} kg",
                r[0].gravity_loss, r[0].burnout_altitude_m / 1000.0, r[0].isp_fraction, r[1].gravity_loss,
                r[1].burnout_altitude_m / 1000.0, perf.total_planner_dv(), leo, geo);
        }
    }
}
