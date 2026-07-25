//! M4 Task 5 probe: dissect stalled/bankrupt BasicPolicy seeds.
//!
//! Not a check — a measurement harness. Run with
//! `cargo test --release --test task5_probe -- --ignored --nocapture`.

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::game_state::GameState;
use rocket_tycoon::policy::BasicPolicy;
use rocket_tycoon::policy::CompanyPolicy;

/// Advance a fresh BasicPolicy game to `days`, then dump the bidding
/// pipeline's state: project statuses, cost history, and every open
/// solicitation with the capable-cost verdict the rule engine sees.
fn dump_seed(seed: u64, days: u32) {
    let mut policy = BasicPolicy::new();
    let mut gs = GameState::with_balance("Bot".into(), seed, BalanceConfig::default());
    for _ in 0..days {
        policy.act(&mut gs);
        gs.advance_day();
    }
    println!("──── seed {seed} at {} ────", gs.date);
    println!(
        "money ${:.1}M  rep {:.1}  inventory {} rockets",
        gs.player_company.money / 1e6,
        gs.player_company.reputation.total(),
        gs.player_company.manufacturing.inventory.rockets.len(),
    );
    for rp in &gs.player_company.rocket_projects {
        let h = gs.player_company.rocket_cost_history.get(&rp.design.id);
        let recent: Vec<String> = h.map(|h| h.iter().rev().take(5)
            .map(|c| format!("{:.1}M", c / 1e6)).collect()).unwrap_or_default();
        println!(
            "rocket project {:?} rev {} status {:?} — last builds [{}]",
            rp.design.name, rp.revision,
            std::mem::discriminant(&rp.status),
            recent.join(", "),
        );
    }
    for ep in &gs.player_company.engine_projects {
        println!(
            "engine project {:?} status {:?} flaws {} discovered {}",
            ep.design.name, std::mem::discriminant(&ep.status),
            ep.flaws.len(), ep.discovered_flaw_count(),
        );
    }
    let solicitations: Vec<(String, u64, String, f64, f64, Option<f64>)> = gs
        .available_contracts.iter()
        .filter(|c| c.is_solicitation())
        .map(|c| (c.name.clone(), c.market_id.0, c.destination.clone(),
                  c.payload_kg, c.budget_ceiling, c.player_bid))
        .collect();
    for (name, market, dest, payload, ceiling, player_bid) in solicitations {
        let (capable, cost) = gs.player_capable_cost(&dest, payload);
        let bid = cost.map(|c| ((c * 2.0) / 10_000.0).round() * 10_000.0);
        println!(
            "  [{market}] {name}: {payload:.0}kg to {dest}, ceiling ${:.1}M — capable {} cost {:?} → bid {:?}{}{}",
            ceiling / 1e6,
            capable.len(),
            cost.map(|c| format!("${:.1}M", c / 1e6)),
            bid.map(|b| format!("${:.1}M", b / 1e6)),
            if bid.is_some_and(|b| b > ceiling) { "  ← OVER CEILING" } else { "" },
            if player_bid.is_some() { "  (bid placed)" } else { "" },
        );
    }
}

#[test]
#[ignore = "measurement probe, not a check"]
fn probe_stalled_seed() {
    // Seed 172: the Task 4 baseline's one bankruptcy. Recovers to
    // rep +50 with stock on the shelf by mid-2007, then never wins
    // another contract and bleeds salaries to bankruptcy.
    for days in [2400, 2700, 2900] {
        dump_seed(172, days);
    }
}
