//! game_state integration-ish unit tests (moved verbatim in the
//! M3 hygiene split; `use super::*` still resolves to `game_state`).

use crate::flight::Payload;
use crate::rocket::RocketDesignId;
use crate::rocket_project::RocketProject;

use super::*;
use crate::flaw::FlawTrigger;

#[test]
fn test_new_game_state() {
    let gs = GameState::new("SpaceCorp".into(), 200_000_000.0, 42);
    assert_eq!(gs.date, GameDate::default_start());
    assert_eq!(gs.player_company.name, "SpaceCorp");
    // Starting money minus one engineering team hiring cost ($150K)
    assert_eq!(gs.player_company.money, 200_000_000.0 - gs.balance.costs.engineering_hiring_cost);
    assert_eq!(gs.speed, GameSpeed::Paused);
    assert_eq!(gs.elapsed_days(), 0);
    // Should have GameStarted event
    assert_eq!(gs.event_log.len(), 1);
    // Should start with 1 engineering team
    assert_eq!(gs.player_company.team_count(), 1);
}

#[test]
fn test_advance_day() {
    let mut gs = GameState::new("Test".into(), 100.0, 1);
    let events = gs.advance_day();
    assert_eq!(gs.date, GameDate::new(2001, 1, 2));
    assert_eq!(gs.elapsed_days(), 1);
    // Normal day should produce no events (DayAdvanced no longer logged)
    assert!(events.is_empty());
}

#[test]
fn test_advance_to_new_month() {
    let mut gs = GameState::new("Test".into(), 100.0, 1);
    // Advance 31 days to get to Feb 1
    for _ in 0..31 {
        gs.advance_day();
    }
    assert_eq!(gs.date, GameDate::new(2001, 2, 1));
    // Last tick should have produced MonthStart
    let recent = gs.event_log.recent(10);
    assert!(recent.iter().any(|(_, e)| matches!(e, GameEvent::MonthStart)));
}

#[test]
fn test_toggle_pause() {
    let mut gs = GameState::new("Test".into(), 100.0, 1);
    assert_eq!(gs.speed, GameSpeed::Paused);

    gs.toggle_pause();
    assert_eq!(gs.speed, GameSpeed::Normal);

    gs.toggle_pause();
    assert_eq!(gs.speed, GameSpeed::Paused);

    // Should remember Normal
    gs.toggle_pause();
    assert_eq!(gs.speed, GameSpeed::Normal);
}

#[test]
fn test_toggle_pause_remembers_speed() {
    let mut gs = GameState::new("Test".into(), 100.0, 1);
    gs.set_speed(GameSpeed::VeryFast);
    assert_eq!(gs.speed, GameSpeed::VeryFast);

    gs.toggle_pause();
    assert_eq!(gs.speed, GameSpeed::Paused);

    // Should restore VeryFast, not Normal
    gs.toggle_pause();
    assert_eq!(gs.speed, GameSpeed::VeryFast);
}

#[test]
fn test_set_speed() {
    let mut gs = GameState::new("Test".into(), 100.0, 1);
    gs.set_speed(GameSpeed::Fast);
    assert_eq!(gs.speed, GameSpeed::Fast);
    gs.set_speed(GameSpeed::VeryFast);
    assert_eq!(gs.speed, GameSpeed::VeryFast);
}

#[test]
fn test_speed_tick_ms() {
    assert!(GameSpeed::Normal.tick_ms() > GameSpeed::Fast.tick_ms());
    assert!(GameSpeed::Fast.tick_ms() > GameSpeed::VeryFast.tick_ms());
}

#[test]
fn test_elapsed_days_after_year() {
    let mut gs = GameState::new("Test".into(), 100.0, 1);
    for _ in 0..365 {
        gs.advance_day();
    }
    assert_eq!(gs.elapsed_days(), 365);
    assert_eq!(gs.date, GameDate::new(2002, 1, 1));
}

#[test]
fn test_hire_team() {
    let mut gs = GameState::new("Test".into(), 1_000_000.0, 1);
    // Starts with 1 team (from Company::new)
    assert_eq!(gs.player_company.team_count(), 1);
    gs.player_company.hire_team("Alpha".into(), &gs.balance);
    assert_eq!(gs.player_company.team_count(), 2);
    // Starting money minus 2 hiring costs (initial team + Alpha)
    assert_eq!(gs.player_company.money, 1_000_000.0 - 2.0 * gs.balance.costs.engineering_hiring_cost);
}

/// Build a 3-stage rocket design with two different engines.
/// Stages 1 & 2 use engine_id=1, stage 3 uses engine_id=2.
/// With 0 payload, stages 1+2 provide enough dv for LEO; stage 3 provides dv for LEO→GTO.
fn make_three_stage_design() -> (RocketDesign, Vec<crate::engine_project::EngineProject>) {
    use crate::engine::{EngineDesign, EngineId, EngineCycle, PropellantFraction};
    use crate::propellant::Propellant;
    use crate::stage::{Stage, StageId};
    use crate::flaw::{Flaw, FlawId, FlawConsequence};
    use crate::engine_project::{EngineProject, EngineProjectId, EngineDesignStatus, PropellantPreset};

    let engine1 = EngineDesign {
        id: EngineId(101),
        name: "Lifter".into(),
        cycle: EngineCycle::GasGenerator,
        thrust_n: 2_000_000.0,
        isp_s: 300.0,
        exit_pressure_pa: 100_000.0,
        needs_atmosphere: false,
        mass_kg: 1500.0,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.6 },
            PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.4 },
        ],
        power_draw_w: 0.0,
    };

    let engine2 = EngineDesign {
        id: EngineId(102),
        name: "Upper".into(),
        cycle: EngineCycle::GasGenerator,
        thrust_n: 100_000.0,
        isp_s: 350.0,
        exit_pressure_pa: 100_000.0,
        needs_atmosphere: false,
        mass_kg: 200.0,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.6 },
            PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.4 },
        ],
        power_draw_w: 0.0,
    };

    let stage1 = Stage {
        id: StageId(1),
        name: "S1".into(),
        engine: engine1.clone(),
        engine_count: 3,
        propellant_mass_kg: 200_000.0,
        structural_mass_kg: 5000.0,
        fairing: None,
        power_sources: Vec::new(),
    };
    let stage2 = Stage {
        id: StageId(2),
        name: "S2".into(),
        engine: engine1.clone(),
        engine_count: 1,
        propellant_mass_kg: 30_000.0,
        structural_mass_kg: 1000.0,
        fairing: None,
        power_sources: Vec::new(),
    };
    // Stage 3 sized so that LEO→GTO (2440 m/s) + GTO→GEO (1500 m/s) = 3940 m/s
    // exceeds its dv, ensuring it gets exhausted and jettisoned mid-flight.
    // With 1000 kg prop, 300 dry, 200 engine = 500 dry, ve=3433: dv ≈ 3433*ln(1500/500) = 3773 m/s
    let stage3 = Stage {
        id: StageId(3),
        name: "S3".into(),
        engine: engine2.clone(),
        engine_count: 1,
        propellant_mass_kg: 1000.0,
        structural_mass_kg: 300.0,
        fairing: None,
        power_sources: Vec::new(),
    };

    let design = RocketDesign {
        id: crate::rocket::RocketDesignId(1),
        name: "TestThreeStage".into(),
        stage_groups: vec![
            vec![stage1],
            vec![stage2],
            vec![stage3],
        ],
    };

    // Engine projects with guaranteed flaws
    let flaw1 = Flaw {
        id: FlawId(1),
        description: "Lifter turbopump vibration".into(),
        consequence: FlawConsequence::PerformanceDegradation(0.01),
        activation_chance: 1.0,
        discovery_probability: 1.0,
        discovered: false, trigger: FlawTrigger::PerFlight,
    };
    let flaw2 = Flaw {
        id: FlawId(2),
        description: "Upper injector erosion".into(),
        consequence: FlawConsequence::PerformanceDegradation(0.01),
        activation_chance: 1.0,
        discovery_probability: 1.0,
        discovered: false, trigger: FlawTrigger::PerFlight,
    };

    let ep1 = EngineProject {
        project_id: EngineProjectId(1),
        design: engine1,
        preset: PropellantPreset::Kerolox,
        scale: 1.0,
        status: EngineDesignStatus::Testing {
            work_completed: 100.0,
        },
        flaws: vec![flaw1],
        revision: 0,
        teams_assigned: 0,
        complexity: 6,
        nre_cost: 0.0, improvements: Vec::new(), cumulative_testing_work: 0.0,
        tech_deficiency_ids: Vec::new(), technology_id: None,
    };
    let ep2 = EngineProject {
        project_id: EngineProjectId(2),
        design: engine2,
        preset: PropellantPreset::Kerolox,
        scale: 1.0,
        status: EngineDesignStatus::Testing {
            work_completed: 100.0,
        },
        flaws: vec![flaw2],
        revision: 0,
        teams_assigned: 0,
        complexity: 6,
        nre_cost: 0.0, improvements: Vec::new(), cumulative_testing_work: 0.0,
        tech_deficiency_ids: Vec::new(), technology_id: None,
    };

    (design, vec![ep1, ep2])
}

#[test]
fn test_flaw_scoping_by_stage_usage() {
    use rand::SeedableRng;
    use crate::engine::EngineId;
    use crate::rocket_project::{RocketProject, RocketProjectId};

    let (design, engine_projects) = make_three_stage_design();

    // Verify stages 1+2 can reach LEO with 0 payload
    let dv_12 = {
        let two_stage = RocketDesign {
            id: design.id,
            name: design.name.clone(),
            stage_groups: vec![
                design.stage_groups[0].clone(),
                design.stage_groups[1].clone(),
            ],
        };
        two_stage.total_delta_v(0.0)
    };
    let total_dv = design.total_delta_v(0.0);
    assert!(dv_12 > 9400.0,
        "Stages 1+2 should provide enough dv for LEO, got {:.0}", dv_12);
    assert!(total_dv > dv_12 + 2000.0,
        "Stage 3 should add significant dv, got total {:.0} vs 1+2={:.0}", total_dv, dv_12);

    // --- Part 1: Launch to LEO, only stages 1+2 flaws should fire ---
    let rp = RocketProject::new(RocketProjectId(1), design.clone(), &crate::balance_config::BalanceConfig::default());
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    let sim = crate::launch::simulate_launch(
        &design, "leo", 0.0,
        &engine_projects, &rp.flaws, &[], &mut rng,
    );

    assert!(matches!(sim.outcome, crate::launch::LaunchOutcome::Success),
        "Launch to LEO should succeed, got {:?}", sim.outcome);
    // Only group 0 (stage 1) flaws should fire at launch.
    // Stage 2 (group 1) and stage 3 (group 2) flaws are deferred to mid-flight.
    assert_eq!(sim.flaws_activated.len(), 1,
        "Only group 0 flaw should fire at launch, got {:?}", sim.flaws_activated);
    assert_eq!(sim.flaws_activated[0].flaw_description, "Lifter turbopump vibration");
    assert_eq!(sim.flaw_rolled_groups.len(), 1);
    assert!(sim.flaw_rolled_groups.contains(&0));

    // --- Part 2: Create a spacecraft at LEO and fly to GTO ---
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 42);
    gs.player_company.engine_projects = engine_projects;
    // Reset flaw discovery for the fly phase
    for ep in &mut gs.player_company.engine_projects {
        for flaw in &mut ep.flaws {
            flaw.discovered = false;
        }
    }

    // Instantiate the rocket from the degraded design (as launch_rocket would)
    let rocket = sim.degraded_design.instantiate(
        crate::rocket::RocketId(1), "leo", 0.0,
    );

    // Simulate that stages 1+2 are jettisoned (as they would be after LEO insertion)
    let mut rocket = rocket;
    for si in 0..rocket.stage_states[0].len() {
        rocket.jettison_stage(0, si);
    }
    for si in 0..rocket.stage_states[1].len() {
        rocket.jettison_stage(1, si);
    }

    // Verify we're on stage 3 (group index 2)
    let current_group = (0..sim.degraded_design.stage_groups.len())
        .find(|&gi| rocket.stage_states.get(gi)
            .map(|ss| ss.iter().any(|s| s.attached))
            .unwrap_or(false));
    assert_eq!(current_group, Some(2), "Should be on stage 3 (group index 2)");

    // Add as spacecraft
    let sc = Spacecraft {
        id: SpacecraftId(1),
        name: "TestCraft".into(),
        rocket,
        design: sim.degraded_design,
        location: "leo".into(),
        rocket_project_id: RocketProjectId(1),
        payloads: Vec::new(),
    };
    gs.spacecraft.push(sc);

    // Fly spacecraft to GEO (LEO→GTO→GEO, 3940 m/s total, exceeds stage 3 dv)
    // Stage 3 will be exhausted and jettisoned mid-flight, triggering flaw roll.
    gs.fly_spacecraft(0, "geo");
    assert_eq!(gs.active_flights.len(), 1, "Should have one active flight");
    assert!(gs.spacecraft.is_empty(), "Spacecraft should be consumed");

    // Advance days until the flight completes (arrives or strands)
    for _ in 0..30 {
        gs.advance_day();
        if gs.active_flights.is_empty() {
            break;
        }
    }

    // Flight should have completed (stranded after stage exhaustion is OK)
    assert!(gs.active_flights.is_empty(),
        "Flight should have completed, still have {} active", gs.active_flights.len());

    // Check that the Upper engine flaw was discovered mid-flight
    let ep2 = gs.player_company.engine_projects.iter()
        .find(|ep| ep.design.id == EngineId(102))
        .expect("Should find engine project 102");
    assert!(ep2.flaws[0].discovered,
        "Upper engine flaw should be discovered after stage 3 jettison");

    // Check the event log for the mid-flight flaw activation
    let flaw_events: Vec<_> = gs.event_log.iter()
        .filter(|(_, e)| matches!(e, GameEvent::MidFlightFlawActivated { .. }))
        .collect();
    assert_eq!(flaw_events.len(), 1,
        "Should have exactly one mid-flight flaw event, got {}", flaw_events.len());
}

#[test]
fn test_spacecraft_has_remaining_dv_after_leo_launch() {
    use crate::rocket_project::{RocketProject, RocketProjectId};

    let (design, engine_projects) = make_three_stage_design();

    let mut gs = GameState::new("Test".into(), 200_000_000.0, 42);
    gs.player_company.engine_projects = engine_projects;

    // Simulate launch to get degraded design
    let rp = RocketProject::new(RocketProjectId(1), design.clone(), &crate::balance_config::BalanceConfig::default());
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(99);
    let sim = crate::launch::simulate_launch(
        &design, "leo", 0.0,
        &gs.player_company.engine_projects, &rp.flaws, &[], &mut rng,
    );

    // Build route and instantiate rocket
    let rocket_mass = sim.degraded_design.total_mass_kg();
    let thrust = sim.degraded_design.group_thrust_n(0);
    let path = crate::location::DELTA_V_MAP
        .shortest_path("earth_surface", "leo", rocket_mass);
    let route = match path {
        Some((p, _)) => crate::flight::build_route(&p, rocket_mass, thrust, false),
        None => vec![],
    };
    let rocket = sim.degraded_design.instantiate(
        crate::rocket::RocketId(1), "earth_surface", 0.0,
    );
    let leg_days = route.first().map(|l| l.total_days()).unwrap_or(0);

    let flight = crate::flight::Flight {
        id: crate::flight::FlightId(1),
        rocket_name: "TestRocket".into(),
        rocket_project_id: RocketProjectId(1),
        design: sim.degraded_design,
        rocket,
        payloads: vec![],
        current_location: "earth_surface".into(),
        route,
        current_leg: 0,
        leg_days_remaining: leg_days,
        status: crate::flight::FlightStatus::InTransit,
        flaws_activated: sim.flaws_activated,
        launch_date: gs.date,
        persist: true,
        launch_partial: false,
        flaw_rolled_groups: sim.flaw_rolled_groups,
        reactor_flaws_rolled: false,
    };

    gs.active_flights.push(flight);

    // Advance days until flight arrives
    for _ in 0..10 {
        gs.advance_day();
        if gs.active_flights.is_empty() { break; }
    }

    assert!(gs.active_flights.is_empty(), "Flight should have arrived");
    assert_eq!(gs.spacecraft.len(), 1, "Should have a spacecraft");

    let sc = &gs.spacecraft[0];
    let remaining = sc.remaining_delta_v();
    assert!(remaining > 1000.0,
        "Spacecraft should have significant remaining dv, got {:.0}", remaining);
}

#[test]
fn test_salary_deduction() {
    let mut gs = GameState::new("Test".into(), 1_000_000.0, 1);
    gs.player_company.hire_team("Alpha".into(), &gs.balance);
    // Now has 2 teams (1 initial + Alpha), paid 2 hiring costs

    // Advance to Feb 1 (31 days)
    for _ in 0..31 {
        gs.advance_day();
    }
    // Should have paid 2 hiring costs + 2 team salaries for 1 month
    let expected = 1_000_000.0 - 2.0 * gs.balance.costs.engineering_hiring_cost - 2.0 * gs.balance.costs.engineering_monthly_salary;
    assert!((gs.player_company.money - expected).abs() < 0.01);
}

#[test]
fn test_negative_money_allowed() {
    let mut gs = GameState::new("Test".into(), 100_000.0, 1);
    // Starts with 1 team (hiring cost $150K), money = 100K - 150K = -50K
    assert!(gs.player_company.money < 0.0);
    gs.player_company.hire_team("Alpha".into(), &gs.balance); // another -150K
    assert!(gs.player_company.money < -150_000.0);
    // Should still work, just go negative
    for _ in 0..31 {
        gs.advance_day();
    }
    // Should have deducted 2 salaries on top
    assert!(gs.player_company.money < -200_000.0);
}

#[test]
fn test_start_engine_project() {
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
    let evt = gs.player_company.start_engine_project(
        "Kestrel".into(),
        crate::engine::EngineCycle::GasGenerator,
        crate::engine_project::PropellantPreset::Kerolox,
        1.0,
        true, None, &gs.balance,
    );
    assert!(evt.is_some());
    assert_eq!(gs.player_company.engine_projects.len(), 1);
}

#[test]
fn test_team_assignment() {
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
    // Starts with 1 team, hire another
    gs.player_company.hire_team("Alpha".into(), &gs.balance);
    gs.player_company.start_engine_project(
        "Kestrel".into(),
        crate::engine::EngineCycle::GasGenerator,
        crate::engine_project::PropellantPreset::Kerolox,
        1.0,
        true, None, &gs.balance,
    );

    assert_eq!(gs.player_company.unassigned_team_count(), 2);
    assert!(gs.player_company.add_team_to_project(0));
    assert_eq!(gs.player_company.unassigned_team_count(), 1);
    assert!(gs.player_company.add_team_to_project(0));
    assert_eq!(gs.player_company.unassigned_team_count(), 0);

    // Can't assign more than available
    assert!(!gs.player_company.add_team_to_project(0));

    // Can remove
    assert!(gs.player_company.remove_team_from_project(0));
    assert_eq!(gs.player_company.unassigned_team_count(), 1);
}

#[test]
fn test_third_party_catalog() {
    let gs = GameState::new("Test".into(), 200_000_000.0, 42);
    assert_eq!(gs.player_company.third_party_catalog.len(), 3);
}

#[test]
fn test_contract_third_party() {
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 42);
    let initial_money = gs.player_company.money;
    let date = gs.date;
    let seed = gs.seed.clone();

    let evt = gs.player_company.contract_third_party(0, date, &seed, &gs.balance);
    assert!(evt.is_some());
    assert_eq!(gs.player_company.contracted_engines.len(), 1);
    // No money deducted for contracting
    assert!((gs.player_company.money - initial_money).abs() < 0.01);
    // Engine should not be added to engine_projects
    assert_eq!(gs.player_company.engine_projects.len(), 0);
}

#[test]
fn test_design_work_progresses() {
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
    gs.player_company.hire_team("Alpha".into(), &gs.balance);
    gs.player_company.start_engine_project(
        "Kestrel".into(),
        crate::engine::EngineCycle::GasGenerator,
        crate::engine_project::PropellantPreset::Kerolox,
        1.0,
        true, None, &gs.balance,
    );
    gs.player_company.add_team_to_project(0);

    // Advance 10 days
    for _ in 0..10 {
        gs.advance_day();
    }

    // Check work progressed
    match &gs.player_company.engine_projects[0].status {
        crate::engine_project::EngineDesignStatus::InDesign { work_completed, .. } => {
            assert!(*work_completed > 9.0, "Should have ~10 work units after 10 days with 1 team");
        }
        _ => {} // might have completed if work_required was low enough (unlikely for complexity 6)
    }
}

/// Test a three-stage hybrid rocket: chemical stages 1-2 for LEO, ion stage for
/// transit to NEA, then hypergolic thruster for asteroid surface landing.
/// Verifies that low-thrust pathfinding routes through low-thrust edges for the
/// ion stage, and the planner switches to chemical pathfinding after staging.
#[test]
fn test_hybrid_ion_chemical_to_asteroid_surface() {
    use crate::engine::{EngineDesign, EngineId, EngineCycle, PropellantFraction};
    use crate::propellant::Propellant;
    use crate::stage::{Stage, StageId};
    use crate::rocket::{RocketDesign, RocketId};
    use crate::location::DELTA_V_MAP;

    // Stage 1: big kerolox booster for LEO
    let booster_engine = EngineDesign {
        id: EngineId(201),
        name: "Booster".into(),
        cycle: EngineCycle::GasGenerator,
        thrust_n: 2_000_000.0,
        isp_s: 300.0,
        exit_pressure_pa: 80_000.0,
        needs_atmosphere: false,
        mass_kg: 1500.0,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.73 },
            PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.27 },
        ],
        power_draw_w: 0.0,
    };
    let stage1 = Stage {
        id: StageId(1), name: "S1".into(),
        engine: booster_engine.clone(), engine_count: 3,
        propellant_mass_kg: 200_000.0, structural_mass_kg: 5000.0,
        fairing: None,
        power_sources: Vec::new(),
    };
    let stage2 = Stage {
        id: StageId(2), name: "S2".into(),
        engine: booster_engine.clone(), engine_count: 1,
        propellant_mass_kg: 30_000.0, structural_mass_kg: 1000.0,
        fairing: None,
        power_sources: Vec::new(),
    };

    // Stage 3: ion engine for transit (very high Isp, very low thrust)
    let ion_engine = EngineDesign {
        id: EngineId(202),
        name: "Ion Drive".into(),
        cycle: EngineCycle::ElectricPropulsion,
        thrust_n: 1.0,
        isp_s: 3000.0,
        exit_pressure_pa: 0.0,
        needs_atmosphere: false,
        mass_kg: 50.0,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::Xenon, mass_fraction: 1.0 },
        ],
        power_draw_w: 0.0,
    };
    let ion_stage = Stage {
        id: StageId(3), name: "Ion".into(),
        engine: ion_engine.clone(), engine_count: 1,
        propellant_mass_kg: 500.0, structural_mass_kg: 50.0,
        fairing: None,
        power_sources: Vec::new(),
    };

    // Stage 4: small hypergolic thruster for asteroid landing
    let hyp_engine = EngineDesign {
        id: EngineId(203),
        name: "Lander".into(),
        cycle: EngineCycle::PressureFed,
        thrust_n: 5_000.0,
        isp_s: 280.0,
        exit_pressure_pa: 7_000.0,
        needs_atmosphere: false,
        mass_kg: 20.0,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::NTO, mass_fraction: 0.57 },
            PropellantFraction { propellant: Propellant::UDMH, mass_fraction: 0.43 },
        ],
        power_draw_w: 0.0,
    };
    let lander_stage = Stage {
        id: StageId(4), name: "Lander".into(),
        engine: hyp_engine.clone(), engine_count: 1,
        propellant_mass_kg: 100.0, structural_mass_kg: 20.0,
        fairing: None,
        power_sources: Vec::new(),
    };

    let design = RocketDesign {
        id: RocketDesignId(10),
        name: "Asteroid Explorer".into(),
        stage_groups: vec![
            vec![stage1],   // group 0: booster
            vec![stage2],   // group 1: upper chemical
            vec![ion_stage],    // group 2: ion transit
            vec![lander_stage], // group 3: hypergolic lander
        ],
    };

    // Instantiate at LEO (as if we've already launched)
    let mut rocket = design.instantiate(RocketId(1), "leo", 0.0);

    // Jettison groups 0 and 1 (already used for launch)
    for si in 0..rocket.stage_states[0].len() {
        rocket.jettison_stage(0, si);
    }
    for si in 0..rocket.stage_states[1].len() {
        rocket.jettison_stage(1, si);
    }

    // Now the active stage is group 2 (ion)
    assert!(rocket.is_current_stage_low_thrust(&design),
        "Ion stage should be classified as low-thrust");

    // Ion stage should be able to reach Eros orbit (low-thrust path).
    let remaining_dv = rocket.remaining_delta_v(&design);
    assert!(remaining_dv > 7000.0,
        "Ion stage should have enough dv for Eros transit, got {}", remaining_dv);

    let eros_path = DELTA_V_MAP.shortest_path_constrained(
        "leo", "eros_orbit", 1000.0, true,
    );
    assert!(eros_path.is_some(), "Low-thrust path LEO→Eros orbit should exist");

    // Ion stage should NOT be able to reach Eros surface (Eros gravity
    // is just above the ion-drive landing threshold).
    let surface_path = DELTA_V_MAP.shortest_path_constrained(
        "leo", "eros_surface", 1000.0, true,
    );
    assert!(surface_path.is_none(),
        "Low-thrust should not reach Eros surface");

    // Simulate burning the ion stage along the Eros transit.
    let (_, eros_dv) = eros_path.unwrap();
    let burn_result = rocket.burn_sequential(&design, eros_dv, 0.0);
    assert!(burn_result.dv_achieved > 6000.0,
        "Should burn significant dv for Eros transit, got {}", burn_result.dv_achieved);

    // The chemical lander handles eros_orbit → eros_surface (high-thrust only).
    let chemical_path = DELTA_V_MAP.shortest_path_constrained(
        "eros_orbit", "eros_surface", 200.0, false,
    );
    assert!(chemical_path.is_some(), "Chemical path Eros orbit → surface should exist");
    let (path, _dv) = chemical_path.unwrap();
    assert_eq!(path, vec!["eros_orbit", "eros_surface"]);

    // After ion stage, lander should not be low-thrust.
    assert!(!design.stage_groups[3][0].engine.is_low_thrust(),
        "Lander engine should be high-thrust (chemical)");
}

/// Set up a game state with a Testing-status rocket project ready for build.
fn setup_buildable_rocket(gs: &mut GameState) -> RocketProjectId {
    use crate::rocket_project::{RocketProject, RocketProjectId, RocketDesignStatus};

    let (design, engine_projects) = make_three_stage_design();
    gs.player_company.engine_projects = engine_projects;

    let mut rp = RocketProject::new(RocketProjectId(1), design, &crate::balance_config::BalanceConfig::default());
    rp.status = RocketDesignStatus::Testing { work_completed: 100.0 };
    let rp_id = rp.project_id;
    gs.player_company.rocket_projects.push(rp);
    rp_id
}

/// Drive the manufacturing pipeline to completion by force-finishing all
/// orders each day and advancing the game until inventory holds a rocket.
/// Cap at 30 iterations to avoid infinite loops if something is wrong.
fn run_manufacturing_to_rocket(gs: &mut GameState) {
    // Hire a manufacturing team so auto-assignment can pick orders up.
    gs.player_company.hire_manufacturing_team("MfgA".into(), &gs.balance);
    for _ in 0..30 {
        // Force every active order to "almost complete" so the next day's
        // work tick finishes them. We still tick advance_day so the full
        // event-handling pipeline (try_unblock, history pushes) runs.
        for order in &mut gs.player_company.manufacturing.orders {
            if !order.waiting_for_prerequisites && order.teams_assigned > 0 {
                order.work_completed = order.work_required;
            }
        }
        gs.advance_day();
        if !gs.player_company.manufacturing.inventory.rockets.is_empty()
            && gs.player_company.manufacturing.orders.is_empty()
        {
            break;
        }
    }
}

#[test]
fn test_engine_build_accrues_labor_cost() {
    // Direct manufacturing-layer test: an engine order with a team
    // assigned should accrue per-day labor that exceeds material cost
    // for a multi-day build.
    use crate::manufacturing::{ManufacturingOrder, ManufacturingOrderId};
    use crate::engine_project::PropellantPreset;
    use crate::engine::EngineId;
    use crate::engine_project::EngineSource;

    let mut order = ManufacturingOrder::new_engine(
        ManufacturingOrderId(1),
        EngineSource::PlayerDesign(crate::engine_project::EngineProjectId(1)),
        EngineId(1),
        "Test".into(),
        500.0,
        6,
        PropellantPreset::Kerolox,
        0,
        0,
        Vec::new(),
        Vec::new(),
        &crate::balance_config::BalanceConfig::default(),
    );
    let material = order.material_cost;
    order.teams_assigned = 1;

    // Tick 30 days of work — this is roughly one team-month = $300K of labor.
    let costs = crate::balance_config::CostsConfig::default();
    for _ in 0..30 {
        order.apply_daily_work(&costs);
    }
    let expected_month_labor = costs.manufacturing_monthly_salary;
    assert!((order.labor_cost - expected_month_labor).abs() < 1.0,
        "labor after 30 days should be ≈ one month salary, got {}", order.labor_cost);
    // Material cost should be unchanged by the work loop.
    assert!((order.material_cost - material).abs() < 0.01);
}

#[test]
fn test_rocket_cost_history_includes_full_cost_at_completion() {
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);

    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    run_manufacturing_to_rocket(&mut gs);

    let design_id = gs.player_company.rocket_projects[0].design.id;
    let history = gs.player_company.rocket_cost_history.get(&design_id)
        .expect("rocket cost history should be populated at integration");
    assert_eq!(history.len(), 1);
    // The recorded cost must exceed pure material total — labor must be
    // present. The integration alone is many days × team-day salary.
    let recorded = history[0];
    assert!(recorded > 1_000_000.0,
        "recorded rocket cost should reflect labor too; got {}", recorded);
}

#[test]
fn test_engine_cost_history_populated_on_completion() {
    use crate::engine_project::EngineProjectId;
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);

    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    run_manufacturing_to_rocket(&mut gs);

    // Three-stage design: 4 EP1 engines (3 on S1 + 1 on S2), 1 EP2 (S3).
    let ep1_history = gs.player_company.engine_cost_history
        .get(&EngineProjectId(1)).expect("EP1 history populated");
    assert_eq!(ep1_history.len(), 4);
    let ep2_history = gs.player_company.engine_cost_history
        .get(&EngineProjectId(2)).expect("EP2 history populated");
    assert_eq!(ep2_history.len(), 1);

    // Each entry should include labor — for our setup (single team,
    // engine work_required = 108) labor is on the order of $1M, which
    // dwarfs the materials for a 1500kg engine.
    assert!(ep1_history.iter().all(|&c| c > 100_000.0),
        "engine cost should include labor: history={:?}", ep1_history);
}

#[test]
fn test_contracted_engine_build_count_increments_at_order_time() {
    use crate::engine_project::EngineProjectId;
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 42);

    let date = gs.date;
    let seed = gs.seed.clone();
    let tp_idx = gs.player_company.third_party_catalog.iter()
        .position(|e| e.available_from <= date)
        .expect("at least one starter engine should be available");
    gs.player_company.contract_third_party(tp_idx, date, &seed, &gs.balance)
        .expect("contracting should succeed");
    let ce_id = gs.player_company.contracted_engines[0].id;

    let (mut design, engine_projects) = make_three_stage_design();
    gs.player_company.engine_projects = engine_projects;

    let contracted_engine = gs.player_company.contracted_engines[0].design.clone();
    for stage in design.stage_groups[0].iter_mut() {
        stage.engine = contracted_engine.clone();
    }
    let stage1_count = design.stage_groups[0][0].engine_count;

    use crate::rocket_project::{RocketProject, RocketProjectId, RocketDesignStatus};
    let mut rp = RocketProject::new(RocketProjectId(1), design, &crate::balance_config::BalanceConfig::default());
    rp.status = RocketDesignStatus::Testing { work_completed: 100.0 };
    gs.player_company.rocket_projects.push(rp);

    // Contracted engines are billed and counted at order time (instant
    // delivery to inventory) — no manufacturing cycle needed.
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();

    let count = *gs.player_company.contracted_engine_build_counts
        .get(&ce_id).unwrap_or(&0);
    assert_eq!(count, stage1_count);
    // Player-designed engine history is populated only after the build
    // pipeline runs — at this point it's still empty.
    assert!(gs.player_company.engine_cost_history
        .get(&EngineProjectId(2)).is_none());
}

/// Build a tiny single-stage RocketDesign suitable for use as a
/// Payload::Spacecraft in arrival/deployment tests.
fn tiny_payload_spacecraft(
    id: u64, name: &str, deploy_at: &str, nested: Vec<Payload>,
) -> Payload {
    use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
    use crate::propellant::Propellant;
    use crate::rocket::{RocketDesign, RocketId};
    use crate::stage::{Stage, StageId};
    let engine = EngineDesign {
        id: EngineId(id), name: "TinyEng".into(),
        cycle: EngineCycle::GasGenerator,
        thrust_n: 100_000.0, mass_kg: 100.0, isp_s: 300.0,
        exit_pressure_pa: 70_000.0, needs_atmosphere: false,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.7 },
            PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.3 },
        ],
        power_draw_w: 0.0,
    };
    let stage = Stage {
        id: StageId(id), name: format!("S{}", id),
        engine, engine_count: 1,
        propellant_mass_kg: 500.0, structural_mass_kg: 100.0,
        fairing: None,
        power_sources: Vec::new(),
    };
    let design = RocketDesign {
        id: RocketDesignId(id), name: name.into(),
        stage_groups: vec![vec![stage]],
    };
    let nested_mass: f64 = nested.iter().map(|p| p.mass_kg()).sum();
    let rocket = design.instantiate(RocketId(id), "earth_surface", nested_mass);
    Payload::Spacecraft {
        deploy_at: Some(deploy_at.into()),
        design,
        rocket,
        nested_payloads: nested,
        rocket_project_id: RocketProjectId(id),
        name: name.into(),
    }
}

/// Assemble a minimal Flight with the given payloads and run the
/// arrival path. Used by deployment tests below to skip the full
/// launch+manufacturing pipeline.
fn arrive_test_flight(
    gs: &mut GameState, destination: &str, payloads: Vec<Payload>,
) -> Vec<crate::event::GameEvent> {
    use crate::flight::{Flight, FlightId, FlightLeg, FlightStatus};
    use crate::rocket::{RocketDesign, RocketId};

    // Empty carrier design — arrival logic doesn't care about its dv.
    let design = RocketDesign {
        id: RocketDesignId(999), name: "CarrierStub".into(),
        stage_groups: vec![],
    };
    let rocket = design.instantiate(RocketId(999), "earth_surface", 0.0);
    let flight = Flight {
        id: FlightId(1),
        rocket_name: "Carrier".into(),
        rocket_project_id: RocketProjectId(999),
        design,
        rocket,
        payloads,
        current_location: destination.into(),
        route: vec![FlightLeg {
            from: "earth_surface".into(),
            to: destination.into(),
            delta_v_cost: 0.0, burn_days: 0, coast_days: 0,
            ambient_pressure_pa: 0.0,
        }],
        current_leg: 0,
        leg_days_remaining: 0,
        status: FlightStatus::Arrived,
        flaws_activated: vec![],
        launch_date: gs.date,
        persist: false,
        launch_partial: false,
        flaw_rolled_groups: std::collections::HashSet::new(),
        reactor_flaws_rolled: false,
    };
    gs.resolve_arrived_flight(flight)
}

#[test]
fn test_spacecraft_payload_deployed_on_arrival() {
    // Skylab-style: Saturn V drops a station as a Spacecraft at LEO.
    let mut gs = GameState::new("Test".into(), 1_000_000.0, 42);
    let skylab = tiny_payload_spacecraft(1, "Skylab", "leo", vec![]);
    let events = arrive_test_flight(&mut gs, "leo", vec![skylab]);
    assert_eq!(gs.spacecraft.len(), 1, "Skylab should be in fleet");
    let sc = &gs.spacecraft[0];
    assert_eq!(sc.name, "Skylab");
    assert_eq!(sc.location, "leo");
    assert!(sc.payloads.is_empty());
    assert!(events.iter().any(|e| matches!(
        e, crate::event::GameEvent::SpacecraftDeployed { spacecraft_name, .. }
            if spacecraft_name == "Skylab"
    )));
}

#[test]
fn test_csm_carrying_lem_keeps_lem_after_deployment() {
    // Apollo-style: CSM is deployed at lunar_orbit carrying LEM as its
    // own payload. The LEM stays *with* CSM (in CSM.payloads), not
    // separately in the fleet, until CSM later flies somewhere.
    let mut gs = GameState::new("Test".into(), 1_000_000.0, 42);
    let lem = tiny_payload_spacecraft(2, "LEM", "lunar_surface", vec![]);
    let csm = tiny_payload_spacecraft(1, "CSM", "lunar_orbit", vec![lem]);
    arrive_test_flight(&mut gs, "lunar_orbit", vec![csm]);

    assert_eq!(gs.spacecraft.len(), 1, "only CSM in fleet, LEM is its payload");
    let csm_sc = &gs.spacecraft[0];
    assert_eq!(csm_sc.name, "CSM");
    assert_eq!(csm_sc.location, "lunar_orbit");
    assert_eq!(csm_sc.payloads.len(), 1);
    match &csm_sc.payloads[0] {
        Payload::Spacecraft { name, deploy_at, .. } => {
            assert_eq!(name, "LEM");
            assert_eq!(deploy_at.as_deref(), Some("lunar_surface"));
        }
        _ => panic!("expected nested Spacecraft payload"),
    }
}

#[test]
fn test_multiple_payloads_at_same_destination() {
    // Rideshare: a launch carrying two contract deliveries to LEO. The
    // arrival handler must pay both contracts.
    use crate::contract::{Contract, ContractId, ContractStatus};
    use crate::calendar::GameDate;
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    let starting_money = gs.player_company.money;
    let contract_a = Contract {
        id: ContractId(1), name: "A".into(),
        destination: "leo".into(), payload_kg: 100.0, payment: 1_000_000.0,
        deadline: GameDate::new(2099, 1, 1),
        status: ContractStatus::Accepted,
        market_id: Default::default(),
        campaign_id: None,
        bid_deadline: None,
        budget_ceiling: 0.0,
        player_bid: None,
    };
    let contract_b = Contract {
        id: ContractId(2), name: "B".into(),
        destination: "leo".into(), payload_kg: 200.0, payment: 2_000_000.0,
        deadline: GameDate::new(2099, 1, 1),
        status: ContractStatus::Accepted,
        market_id: Default::default(),
        campaign_id: None,
        bid_deadline: None,
        budget_ceiling: 0.0,
        player_bid: None,
    };
    gs.player_company.active_contracts.push(contract_a);
    gs.player_company.active_contracts.push(contract_b);

    let payloads = vec![
        Payload::ContractDelivery { contract_id: ContractId(1), payload_kg: 100.0 },
        Payload::ContractDelivery { contract_id: ContractId(2), payload_kg: 200.0 },
    ];
    arrive_test_flight(&mut gs, "leo", payloads);

    assert_eq!(gs.player_company.active_contracts.len(), 0,
        "both contracts should be completed and removed");
    // Money increased by 3M (1M + 2M from the two contracts).
    let earned = gs.player_company.money - starting_money;
    assert!((earned - 3_000_000.0).abs() < 1.0,
        "expected 3M paid out, got {}", earned);
}

/// Push a freshly-built minimal Spacecraft into `gs.spacecraft` at
/// `location` with the given name. Returns its index.
fn push_test_spacecraft(gs: &mut GameState, id: u64, name: &str, location: &str) -> usize {
    use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
    use crate::propellant::Propellant;
    use crate::rocket::{RocketDesign, RocketId};
    use crate::stage::{Stage, StageId};
    let engine = EngineDesign {
        id: EngineId(id), name: "E".into(),
        cycle: EngineCycle::GasGenerator,
        thrust_n: 1.0, mass_kg: 1.0, isp_s: 100.0,
        exit_pressure_pa: 1.0, needs_atmosphere: false,
        propellant_mix: vec![PropellantFraction {
            propellant: Propellant::LOX, mass_fraction: 1.0,
        }],
        power_draw_w: 0.0,
    };
    let stage = Stage {
        id: StageId(id), name: "S".into(),
        engine, engine_count: 1,
        propellant_mass_kg: 100.0, structural_mass_kg: 10.0,
        fairing: None,
        power_sources: Vec::new(),
    };
    let design = RocketDesign {
        id: RocketDesignId(id), name: name.into(),
        stage_groups: vec![vec![stage]],
    };
    let rocket = design.instantiate(RocketId(id), location, 0.0);
    gs.spacecraft.push(Spacecraft {
        id: SpacecraftId(id),
        name: name.into(),
        rocket, design,
        location: location.into(),
        rocket_project_id: RocketProjectId(id),
        payloads: Vec::new(),
    });
    gs.spacecraft.len() - 1
}

#[test]
fn test_dock_combines_two_spacecraft() {
    let mut gs = GameState::new("T".into(), 1.0, 0);
    let csm = push_test_spacecraft(&mut gs, 1, "CSM", "lunar_orbit");
    let lem = push_test_spacecraft(&mut gs, 2, "LEM", "lunar_orbit");
    // Dock LEM onto CSM.
    assert!(gs.dock_spacecraft(lem, csm));
    assert_eq!(gs.spacecraft.len(), 1);
    let carrier = &gs.spacecraft[0];
    assert_eq!(carrier.name, "CSM");
    assert_eq!(carrier.payloads.len(), 1);
    match &carrier.payloads[0] {
        Payload::Spacecraft { name, deploy_at, .. } => {
            assert_eq!(name, "LEM");
            assert!(deploy_at.is_none(), "manual undock only");
        }
        _ => panic!("expected Spacecraft payload"),
    }
}

#[test]
fn test_dock_rejects_different_locations() {
    let mut gs = GameState::new("T".into(), 1.0, 0);
    let a = push_test_spacecraft(&mut gs, 1, "A", "leo");
    let b = push_test_spacecraft(&mut gs, 2, "B", "lunar_orbit");
    assert!(!gs.dock_spacecraft(a, b),
        "dock should refuse cross-location");
    assert_eq!(gs.spacecraft.len(), 2,
        "no spacecraft removed on rejected dock");
}

#[test]
fn test_undock_restores_fleet_member() {
    let mut gs = GameState::new("T".into(), 1.0, 0);
    let csm = push_test_spacecraft(&mut gs, 1, "CSM", "lunar_orbit");
    let lem = push_test_spacecraft(&mut gs, 2, "LEM", "lunar_orbit");
    assert!(gs.dock_spacecraft(lem, csm));
    assert_eq!(gs.spacecraft.len(), 1);
    // Carrier index after dock is 0 (was csm, now alone).
    assert!(gs.undock_payload(0, 0));
    assert_eq!(gs.spacecraft.len(), 2);
    // The undocked LEM should be at the same location.
    let lem_idx = gs.spacecraft.iter()
        .position(|sc| sc.name == "LEM")
        .expect("LEM back in fleet");
    assert_eq!(gs.spacecraft[lem_idx].location, "lunar_orbit");
}

#[test]
fn test_dock_then_fly_keeps_payload_aboard() {
    // After docking with deploy_at = None, flying the carrier should
    // not auto-detach the docked payload.
    let mut gs = GameState::new("T".into(), 1.0, 0);
    let csm = push_test_spacecraft(&mut gs, 1, "CSM", "lunar_orbit");
    let lem = push_test_spacecraft(&mut gs, 2, "LEM", "lunar_orbit");
    gs.dock_spacecraft(lem, csm);

    // Synthesize an arrival of the CSM at earth_escape — the existing
    // arrival path should keep the docked LEM aboard because deploy_at
    // is None (never matches a destination).
    let payloads = std::mem::take(&mut gs.spacecraft[0].payloads);
    let _events = arrive_test_flight(&mut gs, "earth_escape", payloads);
    // arrive_test_flight builds its own carrier, so the docked LEM
    // payload becomes a "remaining_payload" on a non-persisted flight,
    // which means it gets dropped — that's fine for this assertion:
    // we just want to confirm the payload was NOT in deployed_spacecraft.
    let deployed_lem = gs.spacecraft.iter().any(|sc| sc.name == "LEM");
    assert!(!deployed_lem,
        "deploy_at = None should never auto-detach on arrival");
}

#[test]
fn test_undock_with_nested_payloads() {
    // Build a chain: A docked into B, B docked into C. Undock B from C
    // and confirm A is still nested in B.
    let mut gs = GameState::new("T".into(), 1.0, 0);
    let _a = push_test_spacecraft(&mut gs, 1, "A", "leo");
    let _b = push_test_spacecraft(&mut gs, 2, "B", "leo");
    let _c = push_test_spacecraft(&mut gs, 3, "C", "leo");
    // Dock A onto B (indices currently 0, 1, 2; B is at 1, A at 0).
    assert!(gs.dock_spacecraft(0, 1));
    // After: spacecraft = [B(carrying A), C]. Dock B onto C.
    assert!(gs.dock_spacecraft(0, 1));
    // After: spacecraft = [C(carrying B(carrying A))]. Undock B from C.
    assert!(gs.undock_payload(0, 0));
    // Now: C alone in fleet, B in fleet with A nested.
    let b = gs.spacecraft.iter().find(|sc| sc.name == "B")
        .expect("B back in fleet");
    assert_eq!(b.payloads.len(), 1);
    match &b.payloads[0] {
        Payload::Spacecraft { name, .. } => assert_eq!(name, "A"),
        _ => panic!("expected nested A"),
    }
}

#[test]
fn test_save_and_load_with_docked_spacecraft() {
    // Round-trip a docked configuration through save/load.
    use crate::save::{save_game, load_game};
    let mut gs = GameState::new("DockCorp".into(), 1.0, 99);
    let csm = push_test_spacecraft(&mut gs, 1, "CSM", "lunar_orbit");
    let lem = push_test_spacecraft(&mut gs, 2, "LEM", "lunar_orbit");
    gs.dock_spacecraft(lem, csm);

    let path = std::env::temp_dir().join(format!(
        "dock_test_{}.json", std::process::id()));
    save_game(&gs, &path).unwrap();
    let loaded = load_game(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(loaded.spacecraft.len(), 1);
    let carrier = &loaded.spacecraft[0];
    assert_eq!(carrier.name, "CSM");
    match &carrier.payloads[0] {
        Payload::Spacecraft { name, deploy_at, .. } => {
            assert_eq!(name, "LEM");
            assert!(deploy_at.is_none(), "deploy_at = None survives round-trip");
        }
        _ => panic!("expected nested Spacecraft payload"),
    }
}

/// Phase 2a — start_proposed_reactor + promote_proposed_reactor
/// + delete_proposed_reactor round-trip. Mirrors the assertions for
/// engine projects in the engine-pipeline tests.
#[test]
fn test_reactor_proposed_lifecycle() {
    use crate::reactor::EnrichmentLevel;
    use crate::reactor_project::ReactorDesignStatus;

    let mut gs = GameState::new("Test".into(), 100_000_000.0, 1);
    let pid = gs.player_company.start_proposed_reactor(
        "Draft".into(), 1.0, EnrichmentLevel::Leu, &gs.balance,
    );
    let rp = gs.player_company.find_reactor_project(pid).unwrap();
    assert!(matches!(rp.status, ReactorDesignStatus::Proposed { .. }));
    // Hidden from the Reactors pane.
    assert_eq!(gs.player_company.visible_reactor_projects().count(), 0);

    // Promote: now in InDesign and visible.
    let name = gs.player_company.promote_proposed_reactor(pid);
    assert_eq!(name.as_deref(), Some("Draft"));
    assert_eq!(gs.player_company.visible_reactor_projects().count(), 1);
    let rp = gs.player_company.find_reactor_project(pid).unwrap();
    assert!(matches!(rp.status, ReactorDesignStatus::InDesign { .. }));

    // Re-promoting an already-promoted project is a no-op.
    assert!(gs.player_company.promote_proposed_reactor(pid).is_none());

    // Delete-proposed on a non-Proposed project leaves real work
    // alone (defensive).
    gs.player_company.delete_proposed_reactor(pid);
    assert!(gs.player_company.find_reactor_project(pid).is_some());
}

/// Phase 2a — cancelling the editor (delete_proposed_reactor on a
/// still-Proposed project) removes it from the company.
#[test]
fn test_reactor_proposed_can_be_deleted() {
    use crate::reactor::EnrichmentLevel;
    let mut gs = GameState::new("Test".into(), 100_000_000.0, 1);
    let pid = gs.player_company.start_proposed_reactor(
        "Cancelled".into(), 1.0, EnrichmentLevel::Leu, &gs.balance,
    );
    gs.player_company.delete_proposed_reactor(pid);
    assert!(gs.player_company.find_reactor_project(pid).is_none());
}

/// Cross-pool team stealing. `+` on the Reactors pane should be
/// able to pull a team from a busy engine project; symmetric for
/// engines/rockets pulling from reactors.
#[test]
fn test_cross_pool_engineering_team_steal() {
    use crate::reactor::EnrichmentLevel;
    let mut gs = GameState::new("Test".into(), 100_000_000.0, 1);
    // Hire two more teams so the engine project can carry a load
    // worth stealing from.
    gs.player_company.hire_team("Team 2".into(), &gs.balance);
    gs.player_company.hire_team("Team 3".into(), &gs.balance);

    // Start an engine project; load it with 3 teams.
    let pid = gs.player_company.start_proposed_engine_project(
        "E1".into(),
        crate::engine::EngineCycle::GasGenerator,
        crate::engine_project::PropellantPreset::Kerolox,
        1.0, false, None, &gs.balance,
    ).expect("create engine project");
    gs.player_company.promote_proposed_engine(pid);
    for _ in 0..3 {
        assert!(gs.player_company.add_team_to_project(0));
    }
    assert_eq!(gs.player_company.unassigned_team_count(), 0);

    // Start a reactor project with no teams.
    let _ = gs.player_company.start_proposed_reactor(
        "R1".into(), 1.0, EnrichmentLevel::Leu, &gs.balance,
    );
    gs.player_company.promote_proposed_reactor(
        crate::reactor_project::ReactorProjectId(1));

    // No free teams, so a plain add fails — then the steal helper
    // should pull one from the busy engine project.
    assert!(!gs.player_company.add_team_to_reactor_project(0));
    let donor_name = gs.player_company
        .steal_engineering_team_to_reactor_project(0);
    assert_eq!(donor_name.as_deref(), Some("E1"));
    assert_eq!(gs.player_company.engine_projects[0].teams_assigned, 2);
    assert_eq!(gs.player_company.reactor_projects[0].teams_assigned, 1);

    // Symmetric: now the reactor is the smaller pool. Adding a
    // second team to the engine should pull from the reactor only
    // if the reactor is the busiest donor (it's not — engine still
    // has 2). So no movement.
    let before_engine = gs.player_company.engine_projects[0].teams_assigned;
    let before_reactor = gs.player_company.reactor_projects[0].teams_assigned;
    gs.player_company.steal_engineering_team_to_engine_project(0);
    // Donor search includes the target's own project too if it's
    // not excluded; here the target IS the engine project so the
    // engine's own teams are excluded → steal pulls from the
    // reactor.
    assert_eq!(gs.player_company.reactor_projects[0].teams_assigned, before_reactor - 1);
    assert_eq!(gs.player_company.engine_projects[0].teams_assigned, before_engine + 1);
}

/// Phase 2a — team helpers respect the unassigned-team budget and
/// don't decrement past zero.
#[test]
fn test_reactor_team_helpers() {
    use crate::reactor::EnrichmentLevel;
    let mut gs = GameState::new("Test".into(), 100_000_000.0, 1);
    let _pid = gs.player_company.start_proposed_reactor(
        "Mk1".into(), 1.0, EnrichmentLevel::Leu, &gs.balance,
    );
    // Defaults: 1 engineering team (created in Company::new), all
    // unassigned. Adding once succeeds; the second add fails (no
    // free teams).
    assert!(gs.player_company.add_team_to_reactor_project(0));
    assert_eq!(gs.player_company.reactor_projects[0].teams_assigned, 1);
    assert!(!gs.player_company.add_team_to_reactor_project(0));

    // Remove the team; second remove is a no-op (already at zero).
    assert!(gs.player_company.remove_team_from_reactor_project(0));
    assert!(!gs.player_company.remove_team_from_reactor_project(0));
}

/// Phase 2a — completed reactors (Testing+) appear in the
/// installable list.
#[test]
fn test_installable_reactors_filter() {
    use crate::reactor::EnrichmentLevel;
    use crate::reactor_project::ReactorDesignStatus;

    let mut gs = GameState::new("Test".into(), 100_000_000.0, 1);
    let pid = gs.player_company.start_proposed_reactor(
        "Mk1".into(), 1.0, EnrichmentLevel::Leu, &gs.balance,
    );
    // Proposed: not installable.
    assert_eq!(gs.player_company.installable_reactor_projects().count(), 0);

    gs.player_company.promote_proposed_reactor(pid);
    // InDesign: not yet installable (design not finished).
    assert_eq!(gs.player_company.installable_reactor_projects().count(), 0);

    // Force into Testing for the test.
    let rp = gs.player_company.find_reactor_project_mut(pid).unwrap();
    rp.status = ReactorDesignStatus::Testing { work_completed: 0.0 };
    assert_eq!(gs.player_company.installable_reactor_projects().count(), 1);
}

/// End-to-end Phase-1 reactor pipeline check: programmatic project
/// → daily ticks accrue work → status transitions to Testing → a
/// `ReactorDesignComplete` event reaches the game's event stream.
#[test]
fn test_reactor_project_advances_to_testing() {
    use crate::reactor::{EnrichmentLevel, ReactorId};
    use crate::reactor_project::{
        ReactorDesignStatus, ReactorProject, ReactorProjectId,
    };

    let mut gs = GameState::new("Reactor Test".into(), 100_000_000.0, 7);
    let mut project = ReactorProject::new(
        ReactorProjectId(1),
        ReactorId(1),
        "Mk1 Reactor".into(),
        1.0,
        EnrichmentLevel::Leu,
        &crate::balance_config::BalanceConfig::default(),
    );
    project.teams_assigned = 4;
    gs.player_company.reactor_projects.push(project);

    let mut saw_complete = false;
    // Cap iterations so a regression fails the test rather than the
    // process; reactor design at scale 1.0 / complexity 8 = ~192
    // work-days, which 4 teams should clear well under this bound.
    for _ in 0..5_000 {
        let events = gs.advance_day();
        if events.iter().any(|e| matches!(
            e,
            GameEvent::ReactorDesignComplete { .. },
        )) {
            saw_complete = true;
            break;
        }
    }
    assert!(saw_complete, "reactor project should reach Testing");
    let p = &gs.player_company.reactor_projects[0];
    assert!(matches!(p.status, ReactorDesignStatus::Testing { .. }));
    // NRE accrued because teams_assigned > 0 throughout.
    assert!(p.nre_cost > 0.0);
}

/// Phase 3: on design completion the fission-reactor tech's
/// deficiencies roll onto the project (Option-2 gating), applying
/// power/mass/complexity penalties, and a subsequent revision feeds
/// solve attempts back to the technology.
#[test]
fn test_reactor_tech_deficiencies_apply_and_revise() {
    use crate::reactor::{EnrichmentLevel, ReactorId, REF_STEADY_W};
    use crate::reactor_project::{ReactorDesignStatus, ReactorProject, ReactorProjectId};
    use crate::technology::TECH_FISSION_REACTOR;

    let mut gs = GameState::new("Reactor Test".into(), 100_000_000.0, 7);
    let mut project = ReactorProject::new(
        ReactorProjectId(1), ReactorId(1), "Mk1 Reactor".into(), 1.0, EnrichmentLevel::Leu,
        &crate::balance_config::BalanceConfig::default(),
    );
    project.teams_assigned = 4;
    gs.player_company.reactor_projects.push(project);

    let mut saw_deficiencies_evt = false;
    for _ in 0..5_000 {
        let events = gs.advance_day();
        if events.iter().any(|e| matches!(e, GameEvent::ReactorTechDeficienciesFound { .. })) {
            saw_deficiencies_evt = true;
        }
        if matches!(
            gs.player_company.reactor_projects[0].status,
            ReactorDesignStatus::Testing { .. }
        ) {
            break;
        }
    }

    let tech = gs.technologies.iter().find(|t| t.id == TECH_FISSION_REACTOR).unwrap();
    let def_count = tech.deficiencies.len();
    // Difficulty-2 tech always has at least two deficiencies.
    assert!(def_count >= 2);
    assert!(saw_deficiencies_evt, "should log a ReactorTechDeficienciesFound event");

    let p = &gs.player_company.reactor_projects[0];
    assert_eq!(p.tech_deficiency_ids.len(), def_count,
        "all reactor-tech deficiencies should attach to the design");
    // Penalties only ever reduce power / raise mass / raise complexity.
    assert!(p.design.steady_w <= REF_STEADY_W + 1.0);
    assert!(p.design.mass_kg >= 5_000.0 - 1.0);
    assert!(p.complexity >= crate::reactor_project::REACTOR_BASE_COMPLEXITY);
    // Mass invariant holds after penalty application.
    let expected = p.design.reactor_mass_kg + p.design.radiator.mass_kg;
    assert!((p.design.mass_kg - expected).abs() < 1e-6);

    // Kick off a revision and confirm solve attempts reach the tech.
    let attempts_before: u32 = gs.technologies.iter()
        .find(|t| t.id == TECH_FISSION_REACTOR).unwrap()
        .deficiencies.iter().map(|d| d.total_attempts).sum();
    assert!(gs.player_company.reactor_projects[0].start_revision());
    for _ in 0..400 {
        gs.advance_day();
        if matches!(
            gs.player_company.reactor_projects[0].status,
            ReactorDesignStatus::Testing { .. }
        ) {
            break;
        }
    }
    let attempts_after: u32 = gs.technologies.iter()
        .find(|t| t.id == TECH_FISSION_REACTOR).unwrap()
        .deficiencies.iter().map(|d| d.total_attempts).sum();
    assert!(attempts_after > attempts_before,
        "revision should feed deficiency solve attempts to the tech");
}

/// Phase 3b: an installed reactor's PerDay endurance flaw rolls
/// during transit, degrades the flying reactor's output, and is
/// discovered on the owning reactor project — driven through the real
/// `advance_day` flight loop.
#[test]
fn test_reactor_flaw_activates_mid_flight() {
    use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
    use crate::flaw::{Flaw, FlawConsequence, FlawId, FlawTrigger};
    use crate::power::PowerSource;
    use crate::propellant::Propellant;
    use crate::reactor::{EnrichmentLevel, ReactorDesign, ReactorId};
    use crate::reactor_project::{ReactorProject, ReactorProjectId};
    use crate::rocket::{RocketDesign, RocketId};
    use crate::stage::{Stage, StageId};

    let mut gs = GameState::new("Reactor Flight".into(), 200_000_000.0, 11);

    // A reactor project carrying a guaranteed PerDay endurance flaw.
    let reactor_id = ReactorId(50);
    let mut rproj = ReactorProject::new(
        ReactorProjectId(1), reactor_id, "R".into(), 1.0, EnrichmentLevel::Leu,
        &crate::balance_config::BalanceConfig::default(),
    );
    rproj.status = crate::reactor_project::ReactorDesignStatus::Testing { work_completed: 0.0 };
    rproj.flaws = vec![Flaw {
        id: FlawId(1),
        description: "Fuel burnup lowers reactivity over time".into(),
        consequence: FlawConsequence::PerformanceDegradation(0.2),
        activation_chance: 1.0, // PerDay daily_rate → 1.0 (fires every day)
        discovery_probability: 1.0,
        discovered: false,
        trigger: FlawTrigger::PerDay,
    }];
    gs.player_company.reactor_projects.push(rproj);

    // A chemical-engine spacecraft carrying a reactor with that design id.
    let engine = EngineDesign {
        id: EngineId(1), name: "E".into(),
        cycle: EngineCycle::GasGenerator,
        thrust_n: 100_000.0, mass_kg: 200.0, isp_s: 350.0,
        exit_pressure_pa: 70_000.0, needs_atmosphere: false,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.7 },
            PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.3 },
        ],
        power_draw_w: 0.0,
    };
    let reactor_design = ReactorDesign::new(reactor_id, "R".into(), 1.0, EnrichmentLevel::Leu, &crate::balance_config::CostsConfig::default());
    let steady_full = reactor_design.steady_w;
    let stage = Stage {
        id: StageId(1), name: "S".into(),
        engine, engine_count: 1,
        propellant_mass_kg: 40_000.0, structural_mass_kg: 1_000.0,
        fairing: None,
        power_sources: vec![PowerSource::from_reactor_design(reactor_design)],
    };
    let design = RocketDesign {
        id: RocketDesignId(1), name: "ReactorCraft".into(),
        stage_groups: vec![vec![stage]],
    };
    let rocket = design.instantiate(RocketId(1), "leo", 0.0);
    gs.spacecraft.push(Spacecraft {
        id: SpacecraftId(1),
        name: "ReactorCraft".into(),
        rocket,
        design,
        location: "leo".into(),
        rocket_project_id: RocketProjectId(0),
        payloads: Vec::new(),
    });

    gs.fly_spacecraft(0, "geo");
    assert_eq!(gs.active_flights.len(), 1, "spacecraft should be in flight");

    // Advance until the reactor flaw is discovered (or the flight ends).
    let mut discovered = false;
    for _ in 0..60 {
        let events = gs.advance_day();
        if events.iter().any(|e| matches!(e, GameEvent::ReactorFlawDiscovered { .. })) {
            discovered = true;
            break;
        }
        if gs.active_flights.is_empty() {
            break;
        }
    }

    assert!(discovered, "reactor endurance flaw should be discovered mid-flight");
    let rp = gs.player_company.reactor_projects.iter()
        .find(|rp| rp.design.id == reactor_id).unwrap();
    assert!(rp.flaws[0].discovered, "flaw should be marked discovered on the project");
    // Sanity: the degradation multiplier is < 1, so a flying reactor
    // that took the hit outputs less than its rated power.
    assert!(steady_full > 0.0);
}

/// Phase 3b timing fix: a reactor's one-shot PerFlight flaw fires on
/// the flight's FIRST in-transit day (reactors run from flight
/// start), not when the reactor's stage engine happens to fire.
#[test]
fn test_reactor_perflight_flaw_fires_at_flight_start() {
    use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
    use crate::flaw::{Flaw, FlawConsequence, FlawId, FlawTrigger};
    use crate::power::PowerSource;
    use crate::propellant::Propellant;
    use crate::reactor::{EnrichmentLevel, ReactorDesign, ReactorId};
    use crate::reactor_project::{ReactorProject, ReactorProjectId};
    use crate::rocket::{RocketDesign, RocketId};
    use crate::stage::{Stage, StageId};

    let mut gs = GameState::new("Reactor Flight".into(), 200_000_000.0, 5);
    let reactor_id = ReactorId(50);
    let mut rproj = ReactorProject::new(
        ReactorProjectId(1), reactor_id, "R".into(), 1.0, EnrichmentLevel::Leu,
        &crate::balance_config::BalanceConfig::default(),
    );
    rproj.status = crate::reactor_project::ReactorDesignStatus::Testing { work_completed: 0.0 };
    rproj.flaws = vec![Flaw {
        id: FlawId(1),
        description: "Reactor overheats and trips offline".into(),
        consequence: FlawConsequence::PerformanceDegradation(0.1),
        activation_chance: 1.0, // guaranteed PerFlight
        discovery_probability: 1.0,
        discovered: false,
        trigger: FlawTrigger::PerFlight,
    }];
    gs.player_company.reactor_projects.push(rproj);

    let engine = EngineDesign {
        id: EngineId(1), name: "E".into(),
        cycle: EngineCycle::GasGenerator,
        thrust_n: 100_000.0, mass_kg: 200.0, isp_s: 350.0,
        exit_pressure_pa: 70_000.0, needs_atmosphere: false,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.7 },
            PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.3 },
        ],
        power_draw_w: 0.0,
    };
    let reactor_design = ReactorDesign::new(reactor_id, "R".into(), 1.0, EnrichmentLevel::Leu, &crate::balance_config::CostsConfig::default());
    let stage = Stage {
        id: StageId(1), name: "S".into(),
        engine, engine_count: 1,
        propellant_mass_kg: 40_000.0, structural_mass_kg: 1_000.0,
        fairing: None,
        power_sources: vec![PowerSource::from_reactor_design(reactor_design)],
    };
    let design = RocketDesign {
        id: RocketDesignId(1), name: "ReactorCraft".into(),
        stage_groups: vec![vec![stage]],
    };
    let rocket = design.instantiate(RocketId(1), "leo", 0.0);
    gs.spacecraft.push(Spacecraft {
        id: SpacecraftId(1), name: "ReactorCraft".into(),
        rocket, design, location: "leo".into(),
        rocket_project_id: RocketProjectId(0),
        payloads: Vec::new(),
    });

    gs.fly_spacecraft(0, "geo");
    assert_eq!(gs.active_flights.len(), 1);

    // First flight day: the PerFlight reactor flaw must already fire.
    let events = gs.advance_day();
    assert!(events.iter().any(|e| matches!(e, GameEvent::ReactorFlawDiscovered { .. })),
        "PerFlight reactor flaw should fire on the first flight day");
    let rp = gs.player_company.reactor_projects.iter()
        .find(|rp| rp.design.id == reactor_id).unwrap();
    assert!(rp.flaws[0].discovered);
}

/// A catastrophic StageLoss flaw mid-flight destroys the vehicle:
/// it's reported as lost (not stranded) and dents reputation.
#[test]
fn test_mid_flight_stage_loss_destroys_vehicle() {
    use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
    use crate::flaw::{Flaw, FlawConsequence, FlawId, FlawTrigger};
    use crate::power::PowerSource;
    use crate::propellant::Propellant;
    use crate::reactor::{EnrichmentLevel, ReactorDesign, ReactorId};
    use crate::reactor_project::{ReactorProject, ReactorProjectId};
    use crate::rocket::{RocketDesign, RocketId};
    use crate::stage::{Stage, StageId};

    let mut gs = GameState::new("Reactor Flight".into(), 200_000_000.0, 9);
    let reactor_id = ReactorId(50);
    let mut rproj = ReactorProject::new(
        ReactorProjectId(1), reactor_id, "R".into(), 1.0, EnrichmentLevel::Leu,
        &crate::balance_config::BalanceConfig::default(),
    );
    rproj.status = crate::reactor_project::ReactorDesignStatus::Testing { work_completed: 0.0 };
    rproj.flaws = vec![Flaw {
        id: FlawId(1),
        description: "Uncontrolled criticality excursion".into(),
        consequence: FlawConsequence::StageLoss,
        activation_chance: 1.0, // PerDay daily_rate → 1.0
        discovery_probability: 1.0,
        discovered: false,
        trigger: FlawTrigger::PerDay,
    }];
    gs.player_company.reactor_projects.push(rproj);

    let engine = EngineDesign {
        id: EngineId(1), name: "E".into(),
        cycle: EngineCycle::GasGenerator,
        thrust_n: 100_000.0, mass_kg: 200.0, isp_s: 350.0,
        exit_pressure_pa: 70_000.0, needs_atmosphere: false,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.7 },
            PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.3 },
        ],
        power_draw_w: 0.0,
    };
    let reactor_design = ReactorDesign::new(reactor_id, "R".into(), 1.0, EnrichmentLevel::Leu, &crate::balance_config::CostsConfig::default());
    let stage = Stage {
        id: StageId(1), name: "S".into(),
        engine, engine_count: 1,
        propellant_mass_kg: 40_000.0, structural_mass_kg: 1_000.0,
        fairing: None,
        power_sources: vec![PowerSource::from_reactor_design(reactor_design)],
    };
    let design = RocketDesign {
        id: RocketDesignId(1), name: "Doomed".into(),
        stage_groups: vec![vec![stage]],
    };
    let rocket = design.instantiate(RocketId(1), "leo", 0.0);
    gs.spacecraft.push(Spacecraft {
        id: SpacecraftId(1), name: "Doomed".into(),
        rocket, design, location: "leo".into(),
        rocket_project_id: RocketProjectId(0),
        payloads: Vec::new(),
    });

    let rep_before = gs.player_company.reputation.total();
    gs.fly_spacecraft(0, "geo");
    assert_eq!(gs.active_flights.len(), 1);

    // Advance until the flight ends; collect all events.
    let mut all_events = Vec::new();
    for _ in 0..30 {
        all_events.extend(gs.advance_day());
        if gs.active_flights.is_empty() {
            break;
        }
    }

    assert!(gs.active_flights.is_empty(), "destroyed flight should be removed");
    assert!(all_events.iter().any(|e| matches!(e, GameEvent::SpacecraftLost { .. })),
        "should report the vehicle as lost");
    assert!(!all_events.iter().any(|e| matches!(e, GameEvent::SpacecraftStranded { .. })),
        "a destroyed vehicle must not also be reported as stranded");
    assert!(gs.player_company.reputation.total() < rep_before,
        "destroying a vehicle should hit reputation");
}

/// Phase 3: the real daily loop surfaces reactor flaw discovery and
/// flaw-removal revision events (not just the deficiency path).
#[test]
fn test_reactor_flaw_discovery_and_revision_through_daily_loop() {
    use crate::reactor::{EnrichmentLevel, ReactorId};
    use crate::reactor_project::{ReactorDesignStatus, ReactorProject, ReactorProjectId};

    let mut gs = GameState::new("Reactor Test".into(), 100_000_000.0, 3);
    let mut project = ReactorProject::new(
        ReactorProjectId(1), ReactorId(1), "Mk1 Reactor".into(), 1.0, EnrichmentLevel::Leu,
        &crate::balance_config::BalanceConfig::default(),
    );
    project.teams_assigned = 4;
    gs.player_company.reactor_projects.push(project);

    // Advance to Testing.
    for _ in 0..5_000 {
        gs.advance_day();
        if matches!(
            gs.player_company.reactor_projects[0].status,
            ReactorDesignStatus::Testing { .. }
        ) {
            break;
        }
    }
    // Force all flaws visible so testing surfaces them deterministically.
    for f in &mut gs.player_company.reactor_projects[0].flaws {
        f.discovery_probability = 0.99;
    }
    let total_flaws = gs.player_company.reactor_projects[0].flaws.len();

    let mut saw_flaw_discovered = false;
    for _ in 0..400 {
        let events = gs.advance_day();
        if events.iter().any(|e| matches!(e, GameEvent::ReactorFlawDiscovered { .. })) {
            saw_flaw_discovered = true;
        }
        if gs.player_company.reactor_projects[0].discovered_flaw_count() == total_flaws {
            break;
        }
    }
    // (If the design happened to roll zero flaws, there's nothing to
    // discover — only assert when flaws exist.)
    if total_flaws > 0 {
        assert!(saw_flaw_discovered, "daily loop should log flaw discovery");
    }

    // Revise: the loop should remove flaws and log revision completion.
    let discovered_before = gs.player_company.reactor_projects[0].discovered_flaw_count();
    if discovered_before > 0 {
        assert!(gs.player_company.reactor_projects[0].start_revision());
        let mut saw_revision = false;
        for _ in 0..600 {
            let events = gs.advance_day();
            if events.iter().any(|e| matches!(e, GameEvent::ReactorRevisionComplete { .. })) {
                saw_revision = true;
            }
            if matches!(
                gs.player_company.reactor_projects[0].status,
                ReactorDesignStatus::Testing { .. }
            ) {
                break;
            }
        }
        assert!(saw_revision, "daily loop should log revision completion");
        assert_eq!(
            gs.player_company.reactor_projects[0].discovered_flaw_count(), 0,
            "revision should clear discovered flaws",
        );
    }
}
// ── Lifted-from-UI action methods (M1 Task 3a) ──

fn push_contract(gs: &mut GameState, id: u64, destination: &str) -> usize {
    gs.player_company.active_contracts.push(Contract {
        id: crate::contract::ContractId(id),
        name: format!("C{}", id),
        destination: destination.into(),
        payload_kg: 1_000.0,
        payment: 10_000_000.0,
        deadline: GameDate::new(2002, 1, 1),
        status: crate::contract::ContractStatus::Accepted,
        market_id: crate::contract::MarketId::default(),
        campaign_id: None,
        bid_deadline: None,
        budget_ceiling: 0.0,
        player_bid: None,
    });
    gs.player_company.active_contracts.len() - 1
}

#[test]
fn test_build_launch_payloads_empty_is_leo_test_mass() {
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
    let (dest, payloads) = gs.build_launch_payloads(&[], &[]).unwrap();
    assert_eq!(dest, "leo");
    assert_eq!(payloads.len(), 1);
    assert!(matches!(payloads[0], Payload::TestMass { .. }));
}

#[test]
fn test_build_launch_payloads_shared_destination() {
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
    let a = push_contract(&mut gs, 1, "gto");
    let b = push_contract(&mut gs, 2, "gto");
    let (dest, payloads) = gs.build_launch_payloads(&[a, b], &[]).unwrap();
    assert_eq!(dest, "gto");
    assert_eq!(payloads.len(), 2);
    assert!(payloads.iter().all(|p| matches!(p, Payload::ContractDelivery { .. })));
}

#[test]
fn test_build_launch_payloads_conflicting_destinations() {
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
    let a = push_contract(&mut gs, 1, "leo");
    let b = push_contract(&mut gs, 2, "gto");
    let err = gs.build_launch_payloads(&[a, b], &[]).unwrap_err();
    assert!(matches!(err, ManifestError::ConflictingDestinations { .. }));
}

#[test]
fn test_build_launch_payloads_validates_before_consuming() {
    // One real spacecraft in inventory plus one bogus id: the call
    // must fail AND leave the real spacecraft in inventory (validate
    // everything before taking anything).
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
    let (design, engine_projects) = make_three_stage_design();
    gs.player_company.engine_projects = engine_projects;
    let rp = RocketProject::new(
        RocketProjectId(1), design, &gs.balance,
    );
    let design_id = rp.design.id;
    gs.player_company.rocket_projects.push(rp);
    gs.player_company.manufacturing.inventory.rockets.push(
        crate::manufacturing::InventoryRocket {
            item_id: crate::manufacturing::InventoryItemId(10),
            rocket_project_id: RocketProjectId(1),
            design_id,
            rocket_name: "Real".into(),
            build_cost: 0.0,
            revision: 0,
            rocket_flaws: Vec::new(),
        });

    let real = crate::manufacturing::InventoryItemId(10);
    let bogus = crate::manufacturing::InventoryItemId(999);
    let err = gs.build_launch_payloads(&[], &[real, bogus]).unwrap_err();
    assert_eq!(err, ManifestError::SpacecraftMissing);
    assert_eq!(gs.player_company.manufacturing.inventory.rockets.len(), 1,
        "failed manifest must not consume inventory");

    // With only the real pick it succeeds and consumes it.
    let (dest, payloads) = gs.build_launch_payloads(&[], &[real]).unwrap();
    assert_eq!(dest, "leo");
    assert_eq!(payloads.len(), 1);
    assert!(matches!(payloads[0], Payload::Spacecraft { .. }));
    assert!(gs.player_company.manufacturing.inventory.rockets.is_empty());
}

#[test]
fn test_buy_floor_space_debits_money() {
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
    let before = gs.player_company.money;
    let cost = gs.player_company.buy_floor_space(2, &gs.balance.clone());
    assert_eq!(cost, 2.0 * gs.balance.costs.floor_space_cost);
    assert_eq!(gs.player_company.money, before - cost);
    assert_eq!(gs.player_company.manufacturing.floor_space.under_construction.len(), 1);
}

#[test]
fn test_cycle_auto_build_target_requires_testing_and_wraps() {
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
    let (design, engine_projects) = make_three_stage_design();
    gs.player_company.engine_projects = engine_projects;
    let mut rp = RocketProject::new(RocketProjectId(1), design, &gs.balance.clone());
    let pid = rp.project_id;

    // InDesign: not settable.
    gs.player_company.rocket_projects.push(rp.clone());
    assert_eq!(gs.player_company.cycle_auto_build_target(0), None);
    gs.player_company.rocket_projects.clear();

    // Testing: cycles 1 → 2 → 3 → 0 (0 removes the entry).
    rp.status = crate::rocket_project::RocketDesignStatus::Testing { work_completed: 0.0 };
    gs.player_company.rocket_projects.push(rp);
    assert_eq!(gs.player_company.cycle_auto_build_target(0), Some(1));
    assert_eq!(gs.player_company.cycle_auto_build_target(0), Some(2));
    assert_eq!(gs.player_company.cycle_auto_build_target(0), Some(3));
    assert_eq!(gs.player_company.cycle_auto_build_target(0), Some(0));
    assert!(gs.player_company.auto_build_targets.get(&pid).is_none());
}
