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
