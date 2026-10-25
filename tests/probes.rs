//! Measurement probes — not checks. Every test here is ignored; run a
//! family by module name, in release, with output shown:
//!
//! ```text
//! cargo test --release --test probes <module> -- --ignored --nocapture
//! ```
//!
//! | module | what it prints |
//! |---|---|
//! | `bidding_pipeline` | the bot's bidding state on a stalled seed |
//! | `dinosoar` | DinoSoar's awards, launches and stock over seeds, or on a real save |
//! | `capability` | payload capability, compute timing and ascent models |
//! | `campaigns` | campaign announcement/award/cancel rates over 100 seeds |
//! | `falcon9` | Falcon 9 replicas through the game's models: where the payload goes |
//! | `engines` | each surveyed real engine beside its game analogue: TWR, Isp, cost, build time |
//! | `testing_cadence` | how long the bot tests each project before its first launch, and what it found |
//! | `deficiencies` | every deficiency of every technology for a few seeds: kind, magnitude, solvability |
//!
//! `seed_fairness::measure_year1_distribution` stays with the floors it
//! re-derives: it is defined by that suite's own `achievable` filter,
//! and a copy here would drift from it.

mod bidding_pipeline {
    //! The bot's bidding pipeline on a stalled or bankrupt seed:
    //! project statuses, cost history, and every open solicitation with
    //! the capable-cost verdict the rule engine sees (M4 task 5).

    use rocket_tycoon::game_state::GameState;
    use rocket_tycoon::policy::BasicPolicy;
    use rocket_tycoon::policy::CompanyPolicy;

    /// Advance a fresh BasicPolicy game to `days`, then dump the bidding
    /// pipeline's state: project statuses, cost history, and every open
    /// solicitation with the capable-cost verdict the rule engine sees.
    fn dump_seed(seed: u64, days: u32) {
        let mut policy = BasicPolicy::new();
        let mut gs = GameState::new("Bot".into(), seed);
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
}

mod dinosoar {
    //! DinoSoar behaviour — the competitor-side counterpart of
    //! `seed_fairness::measure_year1_distribution`. Use these to
    //! re-derive competitor tuning (production_lines, margins, initial
    //! stock) after balance changes, and to eyeball a real save.

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
            let mut gs = GameState::new("Probe".into(), seed);
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
}

mod capability {
    //! Capability figures for balance work (`17_2_DELTA_V.md` step 4
    //! on, `17_3_PHYSICS.md` D7): `max_payload_to` from Earth for the sim
    //! bot's template and every rocket in the save corpus, the cost of
    //! the two calls every capability question bottoms out in, and the
    //! ascent models side by side. The numbers printed are the thing
    //! being moved; the plans record them.

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
        let mut game = GameState::new("Probe".into(), 42);
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
        let mut game = GameState::new("Probe".into(), 42);
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
        let mut game = GameState::new("Probe".into(), 42);
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
}

mod campaigns {
    //! Campaign activity under the sim bot across many worlds —
    //! announcement, award and cancel rates for eyeballing spawn tuning.

    use rocket_tycoon::calendar::GameDate;
    use rocket_tycoon::game_state::GameState;

    /// Measurement helper (not a guard): campaign activity under the sim
    /// bot across many worlds — announcement/award/cancel rates for
    /// eyeballing spawn tuning. Run with
    /// `cargo test --release --test probes campaigns -- --ignored --nocapture`.
    #[test]
    #[ignore = "measurement probe; run explicitly with --ignored --nocapture"]
    fn measure_campaign_activity() {
        use rocket_tycoon::event::GameEvent;

        let seeds = 100u64;
        let years = 8u32;
        let mut announced = 0u32;
        let mut liftable_at_announce = 0u32;
        let mut player_won = 0u32;
        let mut dino_won = 0u32;
        let mut rejected = 0u32;
        let mut missed = 0u32;
        let mut cancelled = 0u32;

        for seed in 1..=seeds {
            let mut gs = GameState::new("SimCorp".into(), seed);
            let mut policy = rocket_tycoon::policy::policy_by_name("basic").unwrap();
            let end = GameDate::new(gs.date.year + years, gs.date.month, gs.date.day);
            while gs.date < end {
                policy.act(&mut gs);
                for e in gs.advance_day() {
                    match e {
                        GameEvent::CampaignAnnounced { liftable, .. } => {
                            announced += 1;
                            if liftable { liftable_at_announce += 1; }
                        }
                        GameEvent::CampaignAwarded { .. } => player_won += 1,
                        GameEvent::CampaignAwardedToCompetitor { .. } => dino_won += 1,
                        GameEvent::CampaignBidRejected { .. } => rejected += 1,
                        GameEvent::CampaignMissionMissed { .. } => missed += 1,
                        GameEvent::CampaignCancelled { .. } => cancelled += 1,
                        _ => {}
                    }
                }
            }
        }

        println!(
            "campaign activity over {seeds} seeds x {years}y: \
             announced {announced} ({:.2}/seed, {:.1}% liftable at announce), \
             bot won {player_won}, DinoSoar won {dino_won}, bot rejected {rejected}, \
             missions missed {missed}, programs cancelled {cancelled}",
            announced as f64 / seeds as f64,
            100.0 * liftable_at_announce as f64 / announced.max(1) as f64,
        );
        assert!(announced > 0, "campaigns should announce somewhere in {seeds} seeds");
    }
}

mod falcon9 {
    //! Falcon 9 as a yardstick (19_NOZZLES.md step 4): the same tanks
    //! (411 t / 107.5 t, expendable) flown with Merlin 1D / MVac on the
    //! real dry masses, with the game's structural model, with the
    //! game's starting kerolox family, and with each of the game's
    //! kerolox cycles. Real Falcon 9 does ~22.8 t to LEO and ~8.3 t to
    //! GTO expendable. The gap between rows says which model is
    //! costing what; the 2026‑09‑19 probe that started the nozzle work
    //! read 7.3 t LEO / 0 GTO for the all‑game rocket.

    use rocket_tycoon::balance_config::BalanceConfig;
    use rocket_tycoon::engine::{EngineCycle, EngineDesign, Propulsion, EngineId, PropellantFraction};
    use rocket_tycoon::engine_project::{EngineProject, EngineProjectId, PropellantPreset};
    use rocket_tycoon::propellant::Propellant;
    use rocket_tycoon::rocket::{DesignPerformance, RocketDesign, RocketDesignId};
    use rocket_tycoon::rocket_perf::max_payload_to;
    use rocket_tycoon::stage::{recompute_structural_masses, Fairing, Stage, StageId};

    const P1: f64 = 411_000.0;
    const P2: f64 = 107_500.0;

    /// Merlin 1D / MVac as built: 97 bar; ε 16 and ε 165.
    fn merlin(id: u64, name: &str, thrust: f64, mass: f64, isp: f64, eps: f64, sea_level: bool) -> EngineDesign {
        let mut e = EngineDesign {
            id: EngineId(id), name: name.into(), cycle: EngineCycle::GasGenerator,
            thrust_n: thrust, mass_kg: mass, isp_s: isp,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
            propulsion: Propulsion::nozzle(0.0, 0.0, 0.0, sea_level),
        };
        e.set_nozzle(9_700_000.0, eps, 1.22);
        e
    }

    fn stage(id: u64, e: EngineDesign, n: u32, prop: f64, structural: f64, fairing: Option<f64>) -> Stage {
        Stage {
            id: StageId(id), name: format!("S{id}"), engine: e, engine_count: n,
            propellant_mass_kg: prop, structural_mass_kg: structural,
            fairing: fairing.map(|m| Fairing { mass_kg: m, diameter_m: 5.2 }),
            power_sources: Vec::new(),
        }
    }

    fn design(s1: Stage, s2: Stage) -> RocketDesign {
        RocketDesign { id: RocketDesignId(1), name: "F9".into(), stage_groups: vec![vec![s1], vec![s2]] }
    }

    /// The game's two bells of a kerolox family at scale 1.
    fn family(cycle: EngineCycle, bal: &BalanceConfig) -> (EngineDesign, EngineDesign) {
        let ep = EngineProject::new(
            EngineProjectId(1), EngineId(1), format!("{cycle:?}"), cycle, PropellantPreset::Kerolox, 1.0, bal,
        ).unwrap();
        (ep.design_variant(false, &bal.nozzle), ep.design_variant(true, &bal.nozzle))
    }

    fn report(label: &str, d: &RocketDesign) {
        println!("\n== {label}");
        for (gi, g) in d.stage_groups.iter().enumerate() {
            let s = &g[0];
            println!(
                "   S{}: {} ×{}  Isp {:.0} s  ε {:.0}  engines {:.0} kg  structure {:.0} kg  dry {:.0} kg  wet {:.0} kg",
                gi + 1, s.engine.name, s.engine_count, s.engine.isp_s, s.engine.propulsion.bell().map_or(0.0, |b| b.1),
                s.engine.mass_kg * s.engine_count as f64, s.structural_mass_kg, s.dry_mass_kg(), s.wet_mass_kg(),
            );
        }
        let p = DesignPerformance::compute(d, 8_300.0, "earth_surface");
        let losses: Vec<String> = p.groups.iter().enumerate()
            .map(|(gi, g)| format!("g{gi} vac {:.0} grav −{:.0} nozzle −{:.0}", g.delta_v_vacuum, g.gravity_loss, g.overexpansion_loss))
            .collect();
        println!("   at 8.3 t: {}  drag −{:.0}  → planner {:.0}", losses.join("; "), p.ascent.drag, p.total_planner_dv());
        println!(
            "   max payload: LEO {:>6.0} kg   GTO {:>5.0} kg      (Falcon 9 expendable: 22 800 / 8 300)",
            max_payload_to(d, "earth_surface", "leo"), max_payload_to(d, "earth_surface", "gto"),
        );
    }

    #[test]
    #[ignore = "prints the Falcon 9 yardstick; run with --ignored --nocapture"]
    fn falcon9_yardstick() {
        let bal = BalanceConfig::default();
        let m1d = merlin(1, "Merlin 1D", 914_000.0, 470.0, 311.0, 16.0, true);
        let mvac = merlin(2, "MVac", 981_000.0, 600.0, 348.0, 165.0, false);

        report("A: Merlin 1D / MVac, real Falcon 9 dry masses, 1.7 t fairing", &design(
            stage(1, m1d.clone(), 9, P1, 22_200.0 - 9.0 * 470.0, None),
            stage(2, mvac.clone(), 1, P2, 4_000.0 - 600.0, Some(1_700.0)),
        ));

        let mut b = design(stage(1, m1d.clone(), 9, P1, 0.0, None), stage(2, mvac.clone(), 1, P2, 0.0, None));
        recompute_structural_masses(&mut b.stage_groups);
        report("B: Merlin 1D / MVac, game structural model", &b);

        for cycle in [EngineCycle::GasGenerator, EngineCycle::StagedCombustion, EngineCycle::FullFlow] {
            let (sl, vac) = family(cycle, &bal);
            let mut c = design(stage(1, sl, 9, P1, 0.0, None), stage(2, vac, 1, P2, 0.0, None));
            recompute_structural_masses(&mut c.stage_groups);
            report(&format!("C: game kerolox {cycle:?} family (scale 1), game structural model"), &c);
        }

        let (sl, vac) = family(EngineCycle::GasGenerator, &bal);
        report("D: game kerolox GasGenerator family, real Falcon 9 dry masses", &design(
            stage(1, sl.clone(), 9, P1, 22_200.0 - 9.0 * sl.mass_kg, None),
            stage(2, vac.clone(), 1, P2, 4_000.0 - vac.mass_kg, Some(1_700.0)),
        ));
    }
}

mod engines {
    //! The engine yardstick (21_ENGINES.md step 1): each real engine the
    //! plan calibrates against (§3.1 core points, §3.2 outliers; full
    //! data and sources in 21_1_ENGINE_SURVEY.md) beside the game's
    //! analogue — same cycle and propellant, sized to the same thrust,
    //! with the bell the real engine flies. Prints TWR, Isp, unit cost
    //! at the first and twentieth unit, cost per kN and build days.
    //! Every later step of plan 21 records how this table moved.
    //!
    //! Unit cost here is what the game charges to build one: materials
    //! plus build work at the manufacturing line's daily salary, times
    //! the learning curve — the same functions a build order calls.

    use rocket_tycoon::balance_config::BalanceConfig;
    use rocket_tycoon::engine::{EngineCycle as C, EngineDesign, EngineId};
    use rocket_tycoon::engine_project::{EngineProject, EngineProjectId, PropellantPreset as P};
    use rocket_tycoon::resources::engine_material_cost;

    const PAD_PA: f64 = 101_325.0;
    const G0: f64 = 9.80665;

    /// Which bell the real engine flies, and so which figure its thrust
    /// and TWR are quoted at.
    #[derive(Clone, Copy, PartialEq)]
    enum Basis {
        /// A booster: thrust and TWR at sea level, sea-level bell.
        SeaLevel,
        /// An upper-stage engine: vacuum, vacuum bell.
        Vacuum,
    }

    struct Real {
        name: &'static str,
        cycle: C,
        preset: P,
        basis: Basis,
        /// Thrust on `basis` (kN).
        thrust_kn: f64,
        /// TWR on `basis`.
        twr: f64,
        /// Unit cost ($M) and how solid it is; None when unknown.
        cost: Option<(f64, &'static str)>,
        group: &'static str,
    }

    #[allow(clippy::too_many_arguments)]
    fn r(name: &'static str, cycle: C, preset: P, basis: Basis, thrust_kn: f64, twr: f64,
         cost: Option<(f64, &'static str)>, group: &'static str) -> Real {
        Real { name, cycle, preset, basis, thrust_kn, twr, cost, group }
    }

    fn survey() -> Vec<Real> {
        use Basis::{SeaLevel as SL, Vacuum as V};
        vec![
            // §3.1 core points
            r("Merlin 1C", C::GasGenerator, P::Kerolox, SL, 420.0, 68.0, None, "core"),
            r("RD-107A", C::GasGenerator, P::Kerolox, SL, 839.0, 79.0, None, "core"),
            r("RS-27A", C::GasGenerator, P::Kerolox, SL, 890.0, 79.0, None, "core"),
            r("F-1", C::GasGenerator, P::Kerolox, SL, 6_770.0, 82.0, Some((40.0, "≤, 1966 contract in today's $")), "core"),
            r("Viking 5C", C::GasGenerator, P::Hypergolic, SL, 678.0, 84.0, None, "core"),
            r("YF-20B", C::GasGenerator, P::Hypergolic, SL, 731.5, 105.0, None, "core"),
            r("RD-191", C::StagedCombustion, P::Kerolox, SL, 1_922.0, 86.0, Some((4.7, "domestic est. 2019")), "core"),
            r("RD-180", C::StagedCombustion, P::Kerolox, SL, 3_830.0, 71.0, Some((10.0, "1996 deal; $23M by 2014")), "core"),
            r("RD-181", C::StagedCombustion, P::Kerolox, SL, 1_922.0, 86.0, Some((11.2, "2014 export")), "core"),
            r("Vulcain 2", C::GasGenerator, P::Hydrolox, SL, 940.0, 53.0, Some((11.0, "≈€10M, 2017")), "core"),
            r("LE-7A", C::StagedCombustion, P::Hydrolox, SL, 870.0, 49.0, None, "core"),
            r("RS-68", C::GasGenerator, P::Hydrolox, SL, 2_950.0, 46.0, Some((20.0, "weak")), "core"),
            r("RL10A-4-2", C::Expander, P::Hydrolox, V, 99.0, 59.0, Some((14.5, "≤, 2016 contract")), "core"),
            r("LE-5B", C::Expander, P::Hydrolox, V, 137.0, 47.0, None, "core"),
            r("HM7B", C::GasGenerator, P::Hydrolox, V, 62.2, 38.0, None, "core"),
            r("Vinci", C::Expander, P::Hydrolox, V, 180.0, 33.0, None, "core"),
            r("J-2", C::GasGenerator, P::Hydrolox, V, 1_033.0, 59.0, None, "core"),
            r("AJ10-190", C::PressureFed, P::Hypergolic, V, 26.7, 23.0, None, "core"),
            r("Aestus", C::PressureFed, P::Hypergolic, V, 29.6, 27.0, None, "core"),
            r("Kestrel", C::PressureFed, P::Kerolox, V, 31.0, 60.0, None, "core"),
            r("S5.98M", C::GasGenerator, P::Hypergolic, V, 19.6, 21.0, None, "core"),
            r("Aestus II", C::GasGenerator, P::Hypergolic, V, 55.4, 41.0, None, "core"),
            r("RD-58M", C::StagedCombustion, P::Kerolox, V, 83.4, 27.0, None, "core"),
            // §3.2 outliers
            r("Merlin 1D B5", C::GasGenerator, P::Kerolox, SL, 845.0, 183.0, Some((1.0, "weak, $0.4-2M")), "outlier"),
            r("NK-33", C::StagedCombustion, P::Kerolox, SL, 1_510.0, 124.0, Some((1.1, "surplus sale, not production")), "outlier"),
            r("RD-275M", C::StagedCombustion, P::Hypergolic, SL, 1_671.0, 155.0, None, "outlier"),
            r("Raptor 2", C::FullFlow, P::Methalox, SL, 2_256.0, 141.0, Some((1.0, "weak, stated goal")), "outlier"),
            r("Raptor 3", C::FullFlow, P::Methalox, SL, 2_746.0, 184.0, None, "outlier"),
            r("BE-4", C::StagedCombustion, P::Methalox, SL, 2_460.0, 0.0, Some((8.0, "rumour")), "outlier"),
            r("RS-25D", C::StagedCombustion, P::Hydrolox, SL, 1_860.0, 54.0, Some((104.5, "OIG, 2015 $")), "outlier"),
            r("LE-9", C::Expander, P::Hydrolox, V, 1_471.0, 60.0, None, "outlier"),
        ]
    }

    /// The game's analogue: the family at scale 1 on the real engine's
    /// basis, then rescaled so its thrust on that basis matches.
    fn analogue(real: &Real, bal: &BalanceConfig) -> Option<(EngineProject, EngineDesign, f64)> {
        let unit = EngineProject::new(EngineProjectId(1), EngineId(1), real.name.into(), real.cycle, real.preset, 1.0, bal)?;
        let on_basis = |e: &EngineDesign| match real.basis {
            Basis::SeaLevel => e.thrust_n * e.isp_fraction_at(PAD_PA),
            Basis::Vacuum => e.thrust_n,
        };
        let vacuum = real.basis == Basis::Vacuum;
        // A vacuum bell's gain depends on the throat (the exit-diameter
        // limit), so thrust is not quite linear in scale: correct a few
        // times until it lands.
        let mut scale = real.thrust_kn * 1_000.0 / on_basis(&unit.design_variant(vacuum, &bal.nozzle));
        let mut ep = unit;
        for _ in 0..6 {
            ep = EngineProject::new(EngineProjectId(1), EngineId(1), real.name.into(), real.cycle, real.preset, scale, bal)?;
            scale *= real.thrust_kn * 1_000.0 / on_basis(&ep.design_variant(vacuum, &bal.nozzle));
        }
        let engine = ep.design_variant(vacuum, &bal.nozzle);
        let scale = ep.scale(&bal.nozzle);
        Some((ep, engine, scale))
    }

    #[test]
    #[ignore = "prints the engine yardstick; run with --ignored --nocapture"]
    #[allow(clippy::print_literal)] // the header row is spelled out beside the row format
    fn engine_yardstick() {
        let bal = BalanceConfig::default();
        let day = bal.costs.daily_manufacturing_salary();
        let l20 = bal.work.learning_curve_multiplier(20);
        println!(
            "{:<13} {:<8} {:<22} {:>3} {:>8} {:>6} | {:>6} {:>6} {:>5} | {:>9} {:>6} {:>6} {:>7} {:>6} | {}",
            "engine", "group", "game family", "bs", "thrust", "scale", "TWR", "real", "×", "Isp p/v", "$M 1", "$M 20", "$k/kN", "days", "real cost",
        );
        let mut ratios: Vec<(&str, f64)> = Vec::new();
        for real in survey() {
            let Some((ep, e, scale)) = analogue(&real, &bal) else {
                println!("{:<13} no game family for {:?}/{:?}", real.name, real.cycle, real.preset);
                continue;
            };
            let pad = e.isp_fraction_at(PAD_PA);
            let (thrust, isp) = match real.basis {
                Basis::SeaLevel => (e.thrust_n * pad, format!("{:.0}/{:.0}", e.isp_s * pad, e.isp_s)),
                Basis::Vacuum => (e.thrust_n, format!("–/{:.0}", e.isp_s)),
            };
            let twr = thrust / (e.mass_kg * G0);
            let material = engine_material_cost(ep.spec.preset, e.mass_kg, &bal.costs.resource_prices);
            let days = bal.work.engine_build_work(
                ep.complexity, e.mass_kg, bal.propellant_handling.multiplier(ep.spec.preset),
            );
            let unit1 = material + days * day;
            let unit20 = unit1 * l20;
            let ratio = if real.twr > 0.0 { twr / real.twr } else { f64::NAN };
            if real.twr > 0.0 {
                ratios.push((real.group, ratio));
            }
            let real_cost = real.cost.map_or(String::new(), |(m, basis)|
                format!("${m:.1}M = ${:.1}k/kN ({basis})", m * 1_000.0 / real.thrust_kn));
            println!(
                "{:<13} {:<8} {:<22} {:>3} {:>7.0}k {:>6.2} | {:>6.0} {:>6} {:>5.2} | {:>9} {:>6.2} {:>6.2} {:>7.1} {:>6.0} | {}",
                real.name, real.group, format!("{:?}/{:?}", real.cycle, real.preset),
                if real.basis == Basis::SeaLevel { "SL" } else { "vac" },
                thrust / 1_000.0, scale, twr,
                if real.twr > 0.0 { format!("{:.0}", real.twr) } else { "?".into() },
                ratio, isp, unit1 / 1e6, unit20 / 1e6, unit20 / thrust, days, real_cost,
            );
        }
        for group in ["core", "outlier"] {
            let mut rs: Vec<f64> = ratios.iter().filter(|(g, _)| *g == group).map(|(_, r)| *r).collect();
            if rs.is_empty() {
                continue;
            }
            rs.sort_by(|a, b| a.total_cmp(b));
            let median = rs[rs.len() / 2];
            let within = rs.iter().filter(|r| (0.8..=1.25).contains(*r)).count();
            println!("TWR game/real, {group}: median ×{median:.2}; {within} of {} within 0.8–1.25×", rs.len());
        }
    }
}

mod testing_cadence {
    //! How long the bot tests before its first launch: per project,
    //! the testing cycles run, flaws generated / found / fixed / still
    //! hidden at the first launch, and the months from design complete
    //! to that launch (22_FLAWS.md §2).

    use std::collections::HashMap;

    use rocket_tycoon::game_state::GameState;
    use rocket_tycoon::policy::{BasicPolicy, CompanyPolicy};
    use rocket_tycoon::project::DesignStatus;

    #[derive(Default, Clone)]
    struct Snap {
        initial_flaws: usize,
        designed_day: u32,
    }

    #[derive(Default)]
    struct Row {
        kind: &'static str,
        cycles: f64,
        initial: usize,
        fixed: usize,
        known: usize,
        hidden: usize,
        revisions: u32,
        months_testing: f64,
    }

    fn probe_seed(seed: u64, cycle_work: f64) -> Vec<Row> {
        let mut policy = BasicPolicy::new();
        let mut gs = GameState::new("Bot".into(), seed);
        let mut snaps: HashMap<String, Snap> = HashMap::new();
        let mut day = 0u32;
        loop {
            policy.act(&mut gs);
            gs.advance_day();
            day += 1;
            let co = &gs.player_company;
            for p in &co.engine_projects {
                let key = format!("E{}", p.design.name);
                if !snaps.contains_key(&key) && !matches!(p.status, DesignStatus::Proposed { .. } | DesignStatus::InDesign { .. }) {
                    snaps.insert(key, Snap { initial_flaws: p.flaws.len(), designed_day: day });
                }
            }
            for p in &co.rocket_projects {
                let key = format!("R{}", p.design.name);
                if !snaps.contains_key(&key) && !matches!(p.status, DesignStatus::Proposed { .. } | DesignStatus::InDesign { .. }) {
                    snaps.insert(key, Snap { initial_flaws: p.flaws.len(), designed_day: day });
                }
            }
            if !co.launch_history.is_empty() || day > 365 * 6 {
                break;
            }
        }
        let co = &gs.player_company;
        let mut rows = Vec::new();
        let mut push = |kind: &'static str, name: &str, flaws: &[rocket_tycoon::flaw::Flaw], work: f64, revision: u32| {
            let Some(s) = snaps.get(&format!("{}{}", if kind == "engine" { "E" } else { "R" }, name)) else { return };
            let known = flaws.iter().filter(|f| f.discovered).count();
            rows.push(Row {
                kind,
                cycles: work / cycle_work,
                initial: s.initial_flaws,
                fixed: s.initial_flaws.saturating_sub(flaws.len()),
                known,
                hidden: flaws.len() - known,
                revisions: revision,
                months_testing: (day - s.designed_day) as f64 / 30.4,
            });
        };
        for p in &co.engine_projects {
            push("engine", &p.design.name, &p.flaws, p.cumulative_testing_work, p.revision);
        }
        for p in &co.rocket_projects {
            push("rocket", &p.design.name, &p.flaws, p.cumulative_testing_work, p.revision);
        }
        rows
    }

    #[test]
    #[ignore]
    fn bot_testing_before_first_launch() {
        let cycle_work = rocket_tycoon::balance_config::BalanceConfig::default().work.testing_cycle_work;
        println!("{:<5} {:<7} {:>6} {:>5} {:>5} {:>5} {:>6} {:>4} {:>7}",
            "seed", "kind", "cycles", "init", "fixed", "known", "hidden", "rev", "months");
        let mut sums: HashMap<&'static str, (usize, Row)> = HashMap::new();
        for seed in 1..=20u64 {
            for r in probe_seed(seed, cycle_work) {
                println!("{:<5} {:<7} {:>6.1} {:>5} {:>5} {:>5} {:>6} {:>4} {:>7.1}",
                    seed, r.kind, r.cycles, r.initial, r.fixed, r.known, r.hidden, r.revisions, r.months_testing);
                let e = sums.entry(r.kind).or_insert_with(|| (0, Row { kind: r.kind, ..Default::default() }));
                e.0 += 1;
                e.1.cycles += r.cycles; e.1.initial += r.initial; e.1.fixed += r.fixed;
                e.1.known += r.known; e.1.hidden += r.hidden; e.1.revisions += r.revisions;
                e.1.months_testing += r.months_testing;
            }
        }
        println!("── averages over 20 seeds");
        for (kind, (n, s)) in &sums {
            let n = *n as f64;
            println!("{:<7} cycles {:.1}  init {:.1}  fixed {:.1}  known {:.1}  hidden {:.1}  rev {:.1}  months {:.1}",
                kind, s.cycles / n, s.initial as f64 / n, s.fixed as f64 / n, s.known as f64 / n,
                s.hidden as f64 / n, s.revisions as f64 / n, s.months_testing / n);
        }
    }
}

mod first_launch_outcomes {
    //! How the bot's first launches end, by event: failed on the pad,
    //! partial (arrived short), lost or stranded in flight, or clean.
    //! For reading a curve shift off its cause (22_FLAWS.md step 2).

    use rocket_tycoon::event::GameEvent;
    use rocket_tycoon::game_state::GameState;
    use rocket_tycoon::policy::{BasicPolicy, CompanyPolicy};

    #[test]
    #[ignore]
    fn first_launch_by_outcome() {
        let mut counts = std::collections::BTreeMap::<&str, u32>::new();
        let mut scrubs = 0u32;
        for seed in 1..=100u64 {
            let mut policy = BasicPolicy::new();
            let mut gs = GameState::new("Bot".into(), seed);
            let mut outcome: Option<&str> = None;
            for _ in 0..365 * 6 {
                let before = gs.event_log.total_pushed();
                policy.act(&mut gs);
                gs.advance_day();
                let n = (gs.event_log.total_pushed() - before) as usize;
                for (_, e) in gs.event_log.recent(n) {
                    let label = match e {
                        GameEvent::LaunchFailure { .. } => "pad failure",
                        GameEvent::LaunchPartialFailure { .. } => "partial (arrived short)",
                        GameEvent::SpacecraftLost { .. } => "lost in flight",
                        GameEvent::SpacecraftStranded { .. } => "stranded",
                        GameEvent::LaunchSuccess { .. } => "success",
                        GameEvent::LaunchScrubbed { .. } => { scrubs += 1; continue; }
                        _ => continue,
                    };
                    if outcome.is_none() { outcome = Some(label); }
                }
                if outcome.is_some() { break; }
            }
            *counts.entry(outcome.unwrap_or("never launched")).or_default() += 1;
        }
        for (k, v) in &counts {
            println!("{v:>4}  {k}");
        }
        println!("{scrubs:>4}  scrubs before the first launch (not counted as launches)");
    }
}

mod deficiencies {
    //! What a world's technologies carry: every deficiency of every
    //! technology for a handful of seeds, with kind, magnitude and
    //! solvability (23_DEFICIENCIES.md).

    use rocket_tycoon::balance_config::BalanceConfig;
    use rocket_tycoon::seed::GameSeed;
    use rocket_tycoon::technology::generate_technologies;

    #[test]
    #[ignore]
    fn a_worlds_deficiencies() {
        let cfg = BalanceConfig::default().technology;
        for seed in [1u64, 7, 42, 100] {
            println!("── seed {seed}");
            for tech in generate_technologies(&GameSeed::new(seed), &cfg) {
                println!("  {} (difficulty {}, {} deficienc{})", tech.name, tech.difficulty,
                    tech.deficiencies.len(), if tech.deficiencies.len() == 1 { "y" } else { "ies" });
                for d in &tech.deficiencies {
                    println!("    {:<44} {:<16} solvability {:.2}", d.description, d.kind.to_string(), d.solvability);
                }
            }
        }
    }
}


mod small_launch {
    //! Strategy viability (ROADMAP "different strategies"): what an
    //! Electron-class small launcher costs and lifts under the game's
    //! own rules, next to the bot's medium vehicle. Prints liftoff mass,
    //! payload to LEO/SSO, estimated unit cost, nominal build days and
    //! design work, so the smallsat economics can be argued from numbers.
    use rocket_tycoon::engine::EngineCycle;
    use rocket_tycoon::engine_project::PropellantPreset;
    use rocket_tycoon::game_state::GameState;
    use rocket_tycoon::rocket_perf::max_payload_to;
    use rocket_tycoon::stage::{autosize_propellant, recompute_structural_masses, Stage, StageId};

    /// Scale of a family whose sea-level (or vacuum) thrust is `target_n`.
    fn scale_for_thrust(state: &mut GameState, cycle: EngineCycle, preset: PropellantPreset, target_n: f64, vacuum: bool) -> f64 {
        let mut scale = 1.0;
        for _ in 0..4 {
            let mut probe = state.player_company.clone();
            probe.start_engine_project("probe".into(), cycle, preset, scale, None, &state.balance);
            let ep = probe.engine_projects.last().unwrap();
            let thrust = ep.design_variant(vacuum, &state.balance.nozzle).thrust_n;
            scale *= target_n / thrust;
        }
        scale
    }

    #[allow(clippy::too_many_arguments)]
    fn vehicle(
        label: &str, seed: u64,
        boost: (EngineCycle, PropellantPreset, f64, u32),
        upper: (EngineCycle, PropellantPreset, f64, u32),
        payload_kg: f64,
    ) {
        let mut state = GameState::with_money("Probe".into(), 1e9, seed);
        let bs = scale_for_thrust(&mut state, boost.0, boost.1, boost.2, false);
        let us = scale_for_thrust(&mut state, upper.0, upper.1, upper.2, true);
        state.player_company.start_engine_project("Booster".into(), boost.0, boost.1, bs, None, &state.balance);
        state.player_company.start_engine_project("Upper".into(), upper.0, upper.1, us, None, &state.balance);
        let bal = state.balance.clone();
        let bep = state.player_company.engine_projects[0].clone();
        let uep = state.player_company.engine_projects[1].clone();
        let mut s1 = Stage {
            id: StageId(1), name: "S1".into(), engine: bep.design_variant(false, &bal.nozzle),
            engine_count: boost.3, propellant_mass_kg: 10_000.0, structural_mass_kg: 1_000.0,
            fairing: None, power_sources: Vec::new(),
        };
        let mut s2 = Stage {
            id: StageId(2), name: "S2".into(), engine: uep.design_variant(true, &bal.nozzle),
            engine_count: upper.3, propellant_mass_kg: 2_000.0, structural_mass_kg: 200.0,
            fairing: None, power_sources: Vec::new(),
        };
        s1.power_sources.push(rocket_tycoon::power::solar_panel_for_stage_demand(&s1));
        s2.power_sources.push(rocket_tycoon::power::solar_panel_for_stage_demand(&s2));
        let mut groups = vec![vec![s1], vec![s2]];
        for _ in 0..4 {
            let p2 = autosize_propellant(&groups, 1, 0, payload_kg, "earth_surface");
            groups[1][0].propellant_mass_kg = p2;
            recompute_structural_masses(&mut groups);
            let p1 = autosize_propellant(&groups, 0, 0, payload_kg, "earth_surface");
            groups[0][0].propellant_mass_kg = p1;
            recompute_structural_masses(&mut groups);
        }
        state.player_company.start_rocket_project(label.into(), groups, &bal);
        let rp = state.player_company.rocket_projects.last().unwrap().clone();
        let company = &state.player_company;
        let liftoff: f64 = rp.design.stage_groups.iter().flatten().map(|s| s.wet_mass_kg()).sum::<f64>() + payload_kg;
        println!("\n== {label}");
        for (gi, g) in rp.design.stage_groups.iter().enumerate() {
            let s = &g[0];
            println!(
                "   S{}: {} ×{}  {:.0} kN each  {:.0} kg each  Isp {:.0}  prop {:.0} kg  structure {:.0} kg",
                gi + 1, s.engine.name, s.engine_count, s.engine.thrust_n / 1e3, s.engine.mass_kg,
                s.engine.isp_s, s.propellant_mass_kg, s.structural_mass_kg,
            );
        }
        println!(
            "   liftoff {:.1} t   payload LEO {:.0} kg  SSO {:.0} kg  GTO {:.0} kg",
            liftoff / 1e3,
            max_payload_to(&rp.design, "earth_surface", "leo"),
            max_payload_to(&rp.design, "earth_surface", "sso"),
            max_payload_to(&rp.design, "earth_surface", "gto"),
        );
        let perf = rocket_tycoon::rocket::DesignPerformance::compute(&rp.design, payload_kg, "earth_surface");
        let losses: Vec<String> = perf.groups.iter().enumerate()
            .map(|(gi, g)| format!("g{gi} vac {:.0} grav −{:.0} nozzle −{:.0}", g.delta_v_vacuum, g.gravity_loss, g.overexpansion_loss))
            .collect();
        println!("   at {payload_kg:.0} kg: {}  drag −{:.0}  → planner {:.0}", losses.join("; "), perf.ascent.drag, perf.total_planner_dv());
        for (gi, g) in rp.design.stage_groups.iter().enumerate() {
            let s = &g[0];
            let mix: Vec<_> = s.engine.propellant_mix.iter().map(|f| (f.propellant, f.mass_fraction)).collect();
            let b = rocket_tycoon::structure::compute_structural_mass(
                s.propellant_mass_kg, &mix, &s.engine, s.engine_count, gi == 0, gi + 1 < rp.design.stage_groups.len());
            println!("   S{} structure: tank {:.0}  thrust structure {:.0}  aero {:.0}  interstage {:.0}  (engines {:.0})",
                gi + 1, b.tank_mass, b.thrust_structure, b.aero_shell, b.interstage, s.engine.mass_kg * s.engine_count as f64);
        }
        let prices = &bal.costs.resource_prices;
        let fixed = rp.design.stage_groups.iter().flatten().count() as f64 * rocket_tycoon::resources::stage_assembly_cost(prices)
            + rocket_tycoon::resources::rocket_integration_cost(prices);
        println!("   of which fixed assembly + integration ${:.1}M", fixed / 1e6);
        let eng_design = bal.work.design_work_required(bep.complexity, bs) + bal.work.design_work_required(uep.complexity, us);
        println!(
            "   unit cost ${:.1}M (materials, 1st unit)   build {:.0} line-days   design work: engines {:.0} + rocket {:.0} team-days",
            company.estimated_build_cost(&rp, &bal) / 1e6,
            company.nominal_build_days(&rp, &bal),
            eng_design, bal.work.rocket_design_work_required(rp.complexity),
        );
    }

    #[test]
    #[ignore = "prints small-launcher economics; run with --ignored --nocapture"]
    fn small_launcher_economics() {
        use EngineCycle as C;
        use PropellantPreset as P;
        // The bot's medium vehicle, for scale.
        vehicle("BLV-class (2× kerolox GG + hydrolox GG)", 1,
            (C::GasGenerator, P::Kerolox, 750e3, 2), (C::GasGenerator, P::Hydrolox, 100e3, 1), 3_000.0);
        // Electron: nine 25 kN kerolox engines, one vacuum engine, ~300 kg to SSO.
        vehicle("Electron-class (9× 25 kN kerolox GG)", 1,
            (C::GasGenerator, P::Kerolox, 25e3, 9), (C::GasGenerator, P::Kerolox, 25e3, 1), 300.0);
        // Firefly Alpha: four 190 kN kerolox engines, one vacuum, ~1 t to LEO.
        vehicle("Alpha-class (4× 190 kN kerolox GG)", 1,
            (C::GasGenerator, P::Kerolox, 190e3, 4), (C::GasGenerator, P::Kerolox, 70e3, 1), 1_000.0);
        // One bigger engine instead of nine: what the cost model rewards.
        vehicle("Mono small (1× 225 kN kerolox GG)", 1,
            (C::GasGenerator, P::Kerolox, 225e3, 1), (C::GasGenerator, P::Kerolox, 25e3, 1), 300.0);
        vehicle("Pressure-fed small (1× 225 kN kerolox PF)", 1,
            (C::PressureFed, P::Kerolox, 225e3, 1), (C::PressureFed, P::Kerolox, 25e3, 1), 300.0);
        // Medium-lift alternatives to the kerolox/hydrolox pair.
        vehicle("Medium methalox (2× 750 kN GG + 100 kN GG)", 1,
            (C::GasGenerator, P::Methalox, 750e3, 2), (C::GasGenerator, P::Methalox, 100e3, 1), 3_000.0);
        vehicle("Medium kerolox + hypergolic upper", 1,
            (C::GasGenerator, P::Kerolox, 750e3, 2), (C::PressureFed, P::Hypergolic, 100e3, 1), 3_000.0);
        vehicle("Medium kerolox SC (2× 750 kN) + kerolox GG upper", 1,
            (C::StagedCombustion, P::Kerolox, 750e3, 2), (C::GasGenerator, P::Kerolox, 100e3, 1), 3_000.0);
        vehicle("Medium solid S1 (1× 1.5 MN) + hydrolox upper", 1,
            (C::PressureFed, P::Solid, 1_500e3, 1), (C::GasGenerator, P::Hydrolox, 100e3, 1), 3_000.0);
        vehicle("Medium hydrolox SSTO-ish (2× 750 kN GG + 100 kN GG)", 1,
            (C::GasGenerator, P::Hydrolox, 750e3, 2), (C::GasGenerator, P::Hydrolox, 100e3, 1), 3_000.0);
    }

    /// How many small contracts a world offers: contracts by payload
    /// bucket per year over the first four years, with their pay, for a
    /// few seeds and no player activity.
    #[test]
    #[ignore = "prints the smallsat contract supply; run with --ignored --nocapture"]
    fn small_contract_supply() {
        use std::collections::HashMap;
        let buckets = [(300.0, "≤300 kg"), (1_000.0, "≤1 t"), (3_000.0, "≤3 t"), (f64::MAX, ">3 t")];
        for seed in [1u64, 2, 3, 4] {
            let mut state = GameState::with_money("Probe".into(), 1e9, seed);
            let mut seen: HashMap<u64, (u32, f64, f64, String)> = HashMap::new();
            let start_year = state.date.year;
            for _ in 0..(365 * 4) {
                state.advance_day();
                for c in &state.available_contracts {
                    seen.entry(c.id.0).or_insert((state.date.year - start_year, c.payload_kg, c.payment, c.destination.clone()));
                }
            }
            println!("\n── seed {seed}: contracts offered per year by payload (count, mean pay, mean $/kg)");
            for year in 0..4 {
                let mut line = format!("   year {}:", year + 1);
                for (cap, label) in buckets {
                    let rows: Vec<_> = seen.values().filter(|(y, kg, _, _)| *y == year && *kg <= cap
                        && buckets.iter().take_while(|(c2, _)| *c2 < cap).all(|(c2, _)| *kg > *c2)).collect();
                    let n = rows.len();
                    let pay = rows.iter().map(|r| r.2).sum::<f64>() / n.max(1) as f64;
                    let per_kg = rows.iter().map(|r| r.2 / r.1).sum::<f64>() / n.max(1) as f64;
                    line += &format!("  {label}: {n:>2} ${:>4.1}M ${:>3.0}k/kg", pay / 1e6, per_kg / 1e3);
                }
                println!("{line}");
            }
            let small: Vec<_> = seen.values().filter(|(_, kg, _, _)| *kg <= 500.0).collect();
            let sso = small.iter().filter(|r| r.3 == "sso").count();
            println!("   ≤500 kg over 4 years: {} ({} to SSO), total pay ${:.0}M", small.len(), sso, small.iter().map(|r| r.2).sum::<f64>() / 1e6);
        }
    }
}
