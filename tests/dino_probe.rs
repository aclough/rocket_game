//! Measurement probes for DinoSoar behavior (not checks) — the
//! competitor-side counterpart of `measure_year1_distribution`. Use
//! these to re-derive competitor tuning (production_lines, margins,
//! initial stock) after balance changes, and to eyeball a real save.
//! `cargo test --release --test dino_probe -- --ignored --nocapture`

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::event::GameEvent;
use rocket_tycoon::game_state::GameState;

/// Load a real pre-M3 save (path via DINO_PROBE_SAVE env var), confirm
/// DinoSoar is backfilled, and run 120 days watching its behavior.
#[test]
#[ignore = "manual probe against a real save file"]
fn probe_real_save_backfill() {
    let Ok(path) = std::env::var("DINO_PROBE_SAVE") else {
        eprintln!("set DINO_PROBE_SAVE to a save path");
        return;
    };
    let mut gs = rocket_tycoon::save::load_game(std::path::Path::new(&path))
        .expect("save should load");
    assert_eq!(gs.competitors.len(), 1, "DinoSoar should be backfilled on load");
    println!(
        "loaded {} at {:?}: DinoSoar fail_rate {:.4}, stock {}, money ${:.0}M",
        gs.player_company.name, gs.date,
        gs.competitors[0].failure_rate,
        gs.competitors[0].company.manufacturing.inventory.rockets.len(),
        gs.competitors[0].company.money / 1e6,
    );
    let mut awards = 0u32;
    let mut launches = 0u32;
    let mut builds = 0u32;
    for _ in 0..120 {
        for evt in gs.advance_day() {
            match evt {
                GameEvent::ContractAwardedToCompetitor { contract_name, amount, .. } => {
                    awards += 1;
                    println!("  {:?}: award {} at ${:.1}M", gs.date, contract_name, amount / 1e6);
                }
                GameEvent::CompetitorLaunch { contract_name, success, .. } => {
                    launches += 1;
                    println!("  {:?}: launch {} success={}", gs.date, contract_name, success);
                }
                GameEvent::CompetitorRocketBuilt { .. } => builds += 1,
                _ => {}
            }
        }
    }
    println!("120 days: {awards} awards, {launches} launches, {builds} builds");
    assert!(awards > 0, "DinoSoar should win something within 120 days of a live save");
}

#[test]
#[ignore = "measurement probe, not a check"]
fn probe_dinosoar_behavior() {
    let mut agg_awards = Vec::new();
    let mut agg_builds = Vec::new();
    let mut agg_fail = Vec::new();
    for seed in [1u64, 5, 10, 27, 51, 100, 150, 199] {
        let mut gs = GameState::with_balance("Probe".into(), seed, BalanceConfig::default());
        let mut awards = 0u32;
        let mut launches = 0u32;
        let mut failures = 0u32;
        let mut builds = 0u32;
        let years = 4;
        let start_year = gs.date.year;
        while gs.date.year < start_year + years {
            for evt in gs.advance_day() {
                match evt {
                    GameEvent::ContractAwardedToCompetitor { .. } => awards += 1,
                    GameEvent::CompetitorLaunch { success, .. } => {
                        launches += 1;
                        if !success { failures += 1; }
                    }
                    GameEvent::CompetitorRocketBuilt { .. } => builds += 1,
                    _ => {}
                }
            }
        }
        let d = &gs.competitors[0];
        println!(
            "seed {seed:>3}: fail_rate {:.4}  awards {awards:>3}  launches {launches:>3} ({failures} failed)  \
             builds {builds:>3}  stock {}  reserved {}  money ${:.0}M  rep {:.0}  marginal ${:.1}M",
            d.failure_rate,
            d.company.manufacturing.inventory.rockets.len(),
            d.scheduled_launches.len(),
            d.company.money / 1e6,
            d.company.reputation.total(),
            d.marginal_cost(&gs.balance) / 1e6,
        );
        agg_awards.push(awards);
        agg_builds.push(builds);
        agg_fail.push(failures);
    }
    println!(
        "awards over {} seeds: min {} max {}",
        agg_awards.len(),
        agg_awards.iter().min().unwrap(),
        agg_awards.iter().max().unwrap(),
    );
    let _ = agg_builds;
    let _ = agg_fail;
}
