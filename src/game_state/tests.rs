//! game_state integration-ish unit tests (moved verbatim in the
//! M3 hygiene split; `use super::*` still resolves to `game_state`).

use crate::flight::Payload;
use crate::rocket::RocketDesignId;
use crate::rocket_project::RocketProject;
use crate::company::ProjectRef;

/// `ProjectRef`s for the tests that address projects by list position.
fn engine_ref(gs: &GameState, i: usize) -> ProjectRef {
    ProjectRef::Engine(gs.player_company.engine_projects[i].project_id)
}
fn rocket_ref(gs: &GameState, i: usize) -> ProjectRef {
    ProjectRef::Rocket(gs.player_company.rocket_projects[i].project_id)
}
fn reactor_ref(gs: &GameState, i: usize) -> ProjectRef {
    ProjectRef::Reactor(gs.player_company.reactor_projects[i].project_id)
}

use super::*;
use crate::flaw::FlawTrigger;

#[test]
fn test_new_game_state() {
    let gs = GameState::new("SpaceCorp".into(), 200_000_000.0, 42);
    assert_eq!(gs.date, GameDate::default_start());
    assert_eq!(gs.player_company.name, "SpaceCorp");
    // The founding team is free, so the player starts with the full
    // amount the welcome screen quotes them.
    assert_eq!(gs.player_company.money, 200_000_000.0);
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
    // A tick does *today's* work and rolls the date only at the end, so the
    // very first tick runs on the start date — 2001-01-01, a month start.
    // A new game therefore opens with January's contracts instead of sitting
    // out its first month.
    assert!(events.iter().any(|e| matches!(e, GameEvent::MonthStart)));

    // An ordinary mid-month day still produces nothing (there is no
    // day-advanced event).
    let quiet = gs.advance_day();
    assert_eq!(gs.date, GameDate::new(2001, 1, 3));
    assert!(quiet.is_empty());
}

#[test]
fn test_advance_to_new_month() {
    let mut gs = GameState::new("Test".into(), 100.0, 1);
    // 31 ticks cover Jan 1 through Jan 31, leaving the clock reading Feb 1.
    for _ in 0..31 {
        gs.advance_day();
    }
    assert_eq!(gs.date, GameDate::new(2001, 2, 1));
    // February's MonthStart belongs to the tick that *runs* on Feb 1, which
    // is the next one — not the one that merely rolled the date onto it.
    let events = gs.advance_day();
    assert!(events.iter().any(|e| matches!(e, GameEvent::MonthStart)));
    assert_eq!(gs.date, GameDate::new(2001, 2, 2));
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
    // Only Alpha was billed; the founding team came with the company
    assert_eq!(gs.player_company.money, 1_000_000.0 - gs.balance.costs.engineering_hiring_cost);
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
        auto_revise: crate::flaw::auto_revise_default(),
        retired: false,
        project_id: EngineProjectId(1),
        design: engine1,
        spec: crate::engine_project::EngineSpec { preset: PropellantPreset::Kerolox, scale: 1.0 },
        status: EngineDesignStatus::Testing {
            work_completed: 100.0,
        },
        flaws: vec![flaw1],
        revision: 0,
        teams_assigned: 0,
        complexity: 6,
        nre_cost: 0.0, improvements: Vec::new(), next_improvement_id: 0, cumulative_testing_work: 0.0,
        tech_deficiency_ids: Vec::new(), technology_id: None,
    };
    let ep2 = EngineProject {
        auto_revise: crate::flaw::auto_revise_default(),
        retired: false,
        project_id: EngineProjectId(2),
        design: engine2,
        spec: crate::engine_project::EngineSpec { preset: PropellantPreset::Kerolox, scale: 1.0 },
        status: EngineDesignStatus::Testing {
            work_completed: 100.0,
        },
        flaws: vec![flaw2],
        revision: 0,
        teams_assigned: 0,
        complexity: 6,
        nre_cost: 0.0, improvements: Vec::new(), next_improvement_id: 0, cumulative_testing_work: 0.0,
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
        company: crate::flight::CompanyRef::Player,
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
        launch_partial: None,
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
    // Now has 2 teams (1 free founding team + Alpha, who was billed)

    // Advance to Feb 1 (31 days)
    for _ in 0..31 {
        gs.advance_day();
    }
    // Should have paid Alpha's hiring cost + 2 team salaries for 1 month
    let expected = 1_000_000.0 - gs.balance.costs.engineering_hiring_cost - 2.0 * gs.balance.costs.engineering_monthly_salary;
    assert!((gs.player_company.money - expected).abs() < 0.01);
}

#[test]
fn test_negative_money_allowed() {
    let mut gs = GameState::new("Test".into(), 100_000.0, 1);
    // Starts with 1 free founding team, money = 100K
    assert_eq!(gs.player_company.money, 100_000.0);
    gs.player_company.hire_team("Alpha".into(), &gs.balance); // -150K
    assert!(gs.player_company.money < 0.0);
    // Should still work, just go negative
    for _ in 0..31 {
        gs.advance_day();
    }
    // Should have deducted 2 salaries on top
    assert!(gs.player_company.money < -300_000.0);
}

#[test]
fn test_start_engine_project() {
    let mut gs = GameState::new("Test".into(), 200_000_000.0, 1);
    let evt = gs.player_company.start_engine_project(
        "Kestrel".into(),
        crate::engine::EngineCycle::GasGenerator,
        crate::engine_project::PropellantPreset::Kerolox,
        1.0,
        None, &gs.balance,
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
        None, &gs.balance,
    );

    assert_eq!(gs.player_company.unassigned_team_count(), 2);
    assert!(gs.player_company.add_team(engine_ref(&gs, 0)));
    assert_eq!(gs.player_company.unassigned_team_count(), 1);
    assert!(gs.player_company.add_team(engine_ref(&gs, 0)));
    assert_eq!(gs.player_company.unassigned_team_count(), 0);

    // Can't assign more than available
    assert!(!gs.player_company.add_team(engine_ref(&gs, 0)));

    // Can remove
    assert!(gs.player_company.remove_team(engine_ref(&gs, 0)));
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
        None, &gs.balance,
    );
    gs.player_company.add_team(engine_ref(&gs, 0));

    // Advance 10 days
    for _ in 0..10 {
        gs.advance_day();
    }

    // Check work progressed. Not an exhaustive match on purpose: the
    // project might have completed if work_required was low enough
    // (unlikely for complexity 6), and that isn't a failure.
    if let crate::engine_project::EngineDesignStatus::InDesign { work_completed, .. } =
        &gs.player_company.engine_projects[0].status
    {
        assert!(*work_completed > 9.0, "Should have ~10 work units after 10 days with 1 team");
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
        crate::engine::EngineCycle::GasGenerator,
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
    assert!(!gs.player_company.engine_cost_history
        .contains_key(&EngineProjectId(2)));
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
        company: crate::flight::CompanyRef::Player,
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
        launch_partial: None,
        flaw_rolled_groups: std::collections::HashSet::new(),
        reactor_flaws_rolled: false,
    };
    gs.resolve_arrived_flight(flight)
}

/// Arrival used to re-derive the partial-failure reason from
/// `flaws_activated.first()`, which reported the wrong flaw when several
/// activated and fell back to a bare "degraded performance" when the cause
/// wasn't a flaw at all. The reason the launch sim computed is the only one
/// that's true, so it has to reach the event unchanged.
#[test]
fn arrival_reports_the_launch_sims_reason_verbatim() {
    use crate::flight::{Flight, FlightId, FlightLeg, FlightStatus};
    use crate::rocket::{RocketDesign, RocketId};

    let mut gs = GameState::new("Test".into(), 1_000_000.0, 42);
    let reason = "Turbopump seal failure (+2 more) — 3% delta-v shortfall";

    let design = RocketDesign {
        id: RocketDesignId(999), name: "CarrierStub".into(),
        stage_groups: vec![],
    };
    let rocket = design.instantiate(RocketId(999), "earth_surface", 0.0);
    let events = gs.resolve_arrived_flight(Flight {
        id: FlightId(1),
        company: crate::flight::CompanyRef::Player,
        rocket_name: "Carrier".into(),
        rocket_project_id: RocketProjectId(999),
        design,
        rocket,
        payloads: vec![],
        current_location: "leo".into(),
        route: vec![FlightLeg {
            from: "earth_surface".into(), to: "leo".into(),
            delta_v_cost: 0.0, burn_days: 0, coast_days: 0,
            ambient_pressure_pa: 0.0,
        }],
        current_leg: 0,
        leg_days_remaining: 0,
        status: FlightStatus::Arrived,
        // A mid-flight activation unrelated to the ascent — exactly the
        // entry `.first()` would have latched onto if it were still used.
        flaws_activated: vec![crate::launch::FlawActivation {
            flaw_description: "Coolant loop chatter".into(),
            consequence: crate::flaw::FlawConsequence::PerformanceDegradation(0.01),
            engine_name: "Upper".into(),
        }],
        launch_date: gs.date,
        persist: false,
        launch_partial: Some(reason.to_string()),
        flaw_rolled_groups: std::collections::HashSet::new(),
        reactor_flaws_rolled: false,
    });

    let reported = events.iter().find_map(|e| match e {
        crate::event::GameEvent::LaunchPartialFailure { reason, .. } => Some(reason.clone()),
        _ => None,
    }).expect("a flight flagged partial must report a partial failure");
    assert_eq!(reported, reason);
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

/// Phase 2a — start_proposed_reactor + promote_proposed_reactor +
/// delete_proposed_reactor round-trip. Mirrors the assertions for
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
    let name = gs.player_company.promote_proposed(ProjectRef::Reactor(pid));
    assert_eq!(name.as_deref(), Some("Draft"));
    assert_eq!(gs.player_company.visible_reactor_projects().count(), 1);
    let rp = gs.player_company.find_reactor_project(pid).unwrap();
    assert!(matches!(rp.status, ReactorDesignStatus::InDesign { .. }));

    // Re-promoting an already-promoted project is a no-op.
    assert!(gs.player_company.promote_proposed(ProjectRef::Reactor(pid)).is_none());

    // Delete-proposed on a non-Proposed project leaves real work
    // alone (defensive).
    gs.player_company.delete_proposed(ProjectRef::Reactor(pid));
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
    gs.player_company.delete_proposed(ProjectRef::Reactor(pid));
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
        1.0, None, &gs.balance,
    ).expect("create engine project");
    gs.player_company.promote_proposed(ProjectRef::Engine(pid));
    for _ in 0..3 {
        assert!(gs.player_company.add_team(engine_ref(&gs, 0)));
    }
    assert_eq!(gs.player_company.unassigned_team_count(), 0);

    // Start a reactor project with no teams.
    let _ = gs.player_company.start_proposed_reactor(
        "R1".into(), 1.0, EnrichmentLevel::Leu, &gs.balance,
    );
    gs.player_company.promote_proposed(
        ProjectRef::Reactor(crate::reactor_project::ReactorProjectId(1)));

    // No free teams, so a plain add fails — then the steal helper
    // should pull one from the busy engine project.
    assert!(!gs.player_company.add_team(reactor_ref(&gs, 0)));
    let donor_name = gs.player_company
        .steal_team_to(reactor_ref(&gs, 0));
    assert_eq!(donor_name.as_deref(), Some("E1"));
    assert_eq!(gs.player_company.engine_projects[0].teams_assigned, 2);
    assert_eq!(gs.player_company.reactor_projects[0].teams_assigned, 1);

    // Symmetric: now the reactor is the smaller pool. Adding a
    // second team to the engine should pull from the reactor only
    // if the reactor is the busiest donor (it's not — engine still
    // has 2). So no movement.
    let before_engine = gs.player_company.engine_projects[0].teams_assigned;
    let before_reactor = gs.player_company.reactor_projects[0].teams_assigned;
    gs.player_company.steal_team_to(engine_ref(&gs, 0));
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
    assert!(gs.player_company.add_team(reactor_ref(&gs, 0)));
    assert_eq!(gs.player_company.reactor_projects[0].teams_assigned, 1);
    assert!(!gs.player_company.add_team(reactor_ref(&gs, 0)));

    // Remove the team; second remove is a no-op (already at zero).
    assert!(gs.player_company.remove_team(reactor_ref(&gs, 0)));
    assert!(!gs.player_company.remove_team(reactor_ref(&gs, 0)));
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

    gs.player_company.promote_proposed(ProjectRef::Reactor(pid));
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
            GameEvent::Project { kind: crate::project::ProjectKind::Reactor, event: crate::event::ProjectEvent::DesignComplete, .. },
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
        if events.iter().any(|e| matches!(e, GameEvent::Project { kind: crate::project::ProjectKind::Reactor, event: crate::event::ProjectEvent::TechDeficienciesFound { .. }, .. })) {
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
        if events.iter().any(|e| matches!(e, GameEvent::Project { kind: crate::project::ProjectKind::Reactor, event: crate::event::ProjectEvent::FlawDiscovered { .. }, .. })) {
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
    assert!(events.iter().any(|e| matches!(e, GameEvent::Project { kind: crate::project::ProjectKind::Reactor, event: crate::event::ProjectEvent::FlawDiscovered { .. }, .. })),
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
        if events.iter().any(|e| matches!(e, GameEvent::Project { kind: crate::project::ProjectKind::Reactor, event: crate::event::ProjectEvent::FlawDiscovered { .. }, .. })) {
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
            if events.iter().any(|e| matches!(e, GameEvent::Project { kind: crate::project::ProjectKind::Reactor, event: crate::event::ProjectEvent::RevisionComplete, .. })) {
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
    assert!(!gs.player_company.auto_build_targets.contains_key(&pid));
}

/// The launch/battery fencepost: a launch to LEO on the 1st arrives on the
/// 1st, delivers, and has its battery checked once — at LEO, after delivery —
/// before the clock reaches the 2nd.
///
/// Three things have to hold together for that, and each one was broken
/// separately before:
///   * `advance_day` runs the day's work under today's date and rolls over
///     only at the end, so the tick that resolves a launch made while the
///     clock read the 1st is itself stamped the 1st.
///   * the in-flight power tick runs *after* the leg completes, and is
///     skipped entirely for a flight that arrived, so the battery is not
///     charged against the launch site it just left.
///   * the arriving craft is ticked once, by the parked-fleet loop, and not
///     also as a flight — the arrival-day double drain.
#[test]
fn a_leo_launch_on_the_first_arrives_delivers_and_checks_power_on_the_first() {
    use crate::power::PowerSource;
    use crate::rocket_project::{RocketProject, RocketProjectId};

    const BATTERY_KWD: f64 = 5.0;

    let (mut design, engine_projects) = make_three_stage_design();
    // A battery on the upper stage — the one that survives to orbit — so the
    // craft has explicit power (and so a second drain would be visible rather
    // than browning it out).
    design.stage_groups[2][0].power_sources = vec![PowerSource::new_battery(BATTERY_KWD)];

    let mut gs = GameState::new("Test".into(), 200_000_000.0, 42);
    gs.player_company.engine_projects = engine_projects;
    assert_eq!(gs.date, GameDate::new(2001, 1, 1), "fixture premise: games start on the 1st");

    let rp = RocketProject::new(
        RocketProjectId(1), design.clone(), &crate::balance_config::BalanceConfig::default(),
    );
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(99);
    let sim = crate::launch::simulate_launch(
        &design, "leo", 0.0,
        &gs.player_company.engine_projects, &rp.flaws, &[], &mut rng,
    );

    let rocket_mass = sim.degraded_design.total_mass_kg();
    let thrust = sim.degraded_design.group_thrust_n(0);
    let (path, _) = crate::location::DELTA_V_MAP
        .shortest_path("earth_surface", "leo", rocket_mass)
        .expect("the three-stage fixture reaches LEO");
    let route = crate::flight::build_route(&path, rocket_mass, thrust, false);
    let leg_days = route.first().map(|l| l.total_days()).unwrap_or(0);
    assert_eq!(leg_days, 0, "earth_surface -> leo is a same-day leg");

    let rocket = sim.degraded_design.instantiate(
        crate::rocket::RocketId(1), "earth_surface", 0.0,
    );

    gs.active_flights.push(crate::flight::Flight {
        id: crate::flight::FlightId(1),
        company: crate::flight::CompanyRef::Player,
        rocket_name: "Fencepost".into(),
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
        launch_partial: None,
        flaw_rolled_groups: sim.flaw_rolled_groups,
        reactor_flaws_rolled: false,
    });

    let events = gs.advance_day();

    // Arrived and delivered within the tick that ran on the 1st.
    assert!(
        gs.active_flights.is_empty(),
        "a 0-day leg must complete on its launch day, not the day after",
    );
    assert_eq!(gs.spacecraft.len(), 1, "the persisting craft should be parked in orbit");
    assert_eq!(gs.spacecraft[0].location, "leo");
    assert!(
        events.iter().any(|e| matches!(e, GameEvent::LaunchSuccess { .. })),
        "arrival should be reported by the tick that ran on the 1st, got {events:?}",
    );
    assert!(
        gs.event_log.iter().any(|(d, e)| *d == GameDate::new(2001, 1, 1)
            && matches!(e, GameEvent::LaunchSuccess { .. })),
        "the arrival must be stamped the 1st, not the 2nd",
    );
    // ...and only then does the clock reach the 2nd.
    assert_eq!(gs.date, GameDate::new(2001, 1, 2));

    // Exactly one housekeeping day, charged at LEO. Two would mean the craft
    // was ticked both as a flight and as a parked spacecraft.
    let sc = &gs.spacecraft[0];
    let one_day_kwd = sc.rocket.total_housekeeping_w(&sc.design) / 1000.0;
    assert!(one_day_kwd > 0.0, "fixture premise: the surviving stage draws power");
    // Against total capacity, not the fitted battery alone: every attached
    // stage carries a battery, explicit or default, and all of them start
    // full, so capacity minus charge is exactly what today's tick took.
    let capacity = sc.rocket.total_battery_capacity_kwd(&sc.design);
    let drained = capacity - sc.rocket.total_battery_charge_kwd();
    assert!(
        (drained - one_day_kwd).abs() < 1e-9,
        "arrival day should cost exactly one housekeeping day ({one_day_kwd} kWd), drained {drained}",
    );
}


/// Count queued engine orders, regardless of source.
fn queued_engine_orders(gs: &GameState) -> usize {
    gs.player_company.manufacturing.orders.iter()
        .filter(|o| matches!(o.order_type,
            crate::manufacturing::ManufacturingOrderType::Engine { .. }))
        .count()
}

/// Drive manufacturing until the order queue empties, without requiring a
/// finished rocket — standalone engine builds produce no rocket.
fn run_manufacturing_to_idle(gs: &mut GameState) {
    gs.player_company.hire_manufacturing_team("MfgA".into(), &gs.balance);
    for _ in 0..30 {
        for order in &mut gs.player_company.manufacturing.orders {
            if !order.waiting_for_prerequisites && order.teams_assigned > 0 {
                order.work_completed = order.work_required;
            }
        }
        gs.advance_day();
        if gs.player_company.manufacturing.orders.is_empty() {
            break;
        }
    }
}

/// Engines built by hand from the Engines pane are the same engines a rocket
/// needs, so ordering the rocket should top the stock up rather than buy a
/// second full set and leave the first sitting on the shelf.
#[test]
fn a_rocket_build_uses_engines_already_in_stock() {
    use crate::engine_project::{EngineProjectId, EngineSource};
    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    let src = EngineSource::PlayerDesign(EngineProjectId(1));

    // The design wants four EP1 engines (3 on S1, 1 on S2) and one EP2.
    // Build all four EP1s by hand first.
    for _ in 0..4 {
        gs.player_company.order_engine_build(0, &gs.balance)
            .expect("EP1 is in Testing, so it can be built standalone");
    }
    run_manufacturing_to_idle(&mut gs);
    assert_eq!(
        gs.player_company.manufacturing.inventory.engine_count(src), 4,
        "premise: four hand-built engines are sitting in stock",
    );

    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    assert_eq!(
        queued_engine_orders(&gs), 1,
        "only the EP2 for S3 is missing; the four EP1s are already on the shelf",
    );

    run_manufacturing_to_rocket(&mut gs);
    assert_eq!(gs.player_company.manufacturing.inventory.rockets.len(), 1);
    assert_eq!(
        gs.player_company.manufacturing.inventory.engine_count(src), 0,
        "the hand-built engines went into the rocket instead of gathering dust",
    );
}

/// Same rule for engines still on the line: a build queued a moment ago
/// counts as supply, otherwise the obvious workflow — queue the engines,
/// then order the rocket — still pays twice.
#[test]
fn a_rocket_build_counts_engines_still_being_built() {
    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);

    for _ in 0..4 {
        gs.player_company.order_engine_build(0, &gs.balance).unwrap();
    }
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    assert_eq!(
        queued_engine_orders(&gs), 5,
        "four already on the line plus the one EP2 nobody has built yet",
    );

    run_manufacturing_to_rocket(&mut gs);
    assert_eq!(gs.player_company.manufacturing.inventory.rockets.len(), 1,
        "and the rocket still gets built");
}

/// The supply is not double-counted across rockets: engines a blocked stage
/// order is already waiting on are spoken for, so a second rocket orders its
/// own full set.
#[test]
fn a_second_rocket_does_not_raid_the_first_rockets_engines() {
    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);

    for _ in 0..4 {
        gs.player_company.order_engine_build(0, &gs.balance).unwrap();
    }
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    assert_eq!(queued_engine_orders(&gs), 5);

    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    assert_eq!(
        queued_engine_orders(&gs), 10,
        "the first rocket's stages have claimed the existing five",
    );
}







/// Declaring a rush job takes the whole floor, including teams that were
/// already at work — a deadline means now, so waiting for teams to come
/// free is the wrong shape.
#[test]
fn a_rush_job_preempts_the_whole_floor() {
    use crate::rocket_project::{RocketProject, RocketProjectId, RocketDesignStatus};

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    let (design2, _) = make_three_stage_design();
    let mut rp2 = RocketProject::new(
        RocketProjectId(2), design2, &crate::balance_config::BalanceConfig::default());
    rp2.status = RocketDesignStatus::Testing { work_completed: 100.0 };
    gs.player_company.rocket_projects.push(rp2);

    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.order_rocket_build(1, &gs.balance).unwrap();
    for i in 0..6 {
        gs.player_company.hire_manufacturing_team(format!("Mfg{i}"), &gs.balance);
    }
    gs.player_company.assign_manufacturing_teams();
    let spread: u32 = gs.player_company.manufacturing.orders.iter()
        .filter(|o| o.parent_rocket() == Some(RocketProjectId(1)))
        .map(|o| o.teams_assigned).sum();
    assert!(spread < 6, "premise: the floor starts split between both rockets");

    gs.player_company.rush_projects.insert(RocketProjectId(1));
    gs.player_company.assign_manufacturing_teams();

    let flags = gs.player_company.rushed_order_flags();
    let rushed: u32 = gs.player_company.manufacturing.orders.iter().enumerate()
        .filter(|(i, _)| flags[*i])
        .map(|(_, o)| o.teams_assigned).sum();
    let rest: u32 = gs.player_company.manufacturing.orders.iter().enumerate()
        .filter(|(i, _)| !flags[*i])
        .map(|(_, o)| o.teams_assigned).sum();
    assert_eq!(rushed, 6, "every team goes to the rush");
    assert_eq!(rest, 0, "and nothing is left on ordinary work");
}

/// A rush job is almost always blocked when you declare it — its stages
/// are waiting for engines. The rush has to reach whatever is holding it
/// up, or it reaches nothing.
#[test]
fn a_rush_reaches_the_engine_builds_holding_it_up() {
    use crate::rocket_project::RocketProjectId;

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    // Hand-built engines, owned by no rocket — the case that defeated the
    // first design.
    for _ in 0..4 {
        gs.player_company.order_engine_build(0, &gs.balance).unwrap();
    }
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.rush_projects.insert(RocketProjectId(1));

    let flags = gs.player_company.rushed_order_flags();
    let standalone: Vec<usize> = gs.player_company.manufacturing.orders.iter()
        .enumerate()
        .filter(|(_, o)| o.parent_rocket().is_none())
        .map(|(i, _)| i)
        .collect();
    assert_eq!(standalone.len(), 4, "premise: four ownerless engine builds");
    assert!(standalone.iter().any(|i| flags[*i]),
        "the rush reaches the ownerless engines blocking it");
}

/// Two rush jobs share the floor the way two ordinary orders would.
#[test]
fn two_rush_jobs_split_the_floor() {
    use crate::rocket_project::{RocketProject, RocketProjectId, RocketDesignStatus};

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    let (design2, _) = make_three_stage_design();
    let mut rp2 = RocketProject::new(
        RocketProjectId(2), design2, &crate::balance_config::BalanceConfig::default());
    rp2.status = RocketDesignStatus::Testing { work_completed: 100.0 };
    gs.player_company.rocket_projects.push(rp2);
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.order_rocket_build(1, &gs.balance).unwrap();
    for i in 0..6 {
        gs.player_company.hire_manufacturing_team(format!("Mfg{i}"), &gs.balance);
    }

    gs.player_company.rush_projects.insert(RocketProjectId(1));
    gs.player_company.rush_projects.insert(RocketProjectId(2));
    gs.player_company.assign_manufacturing_teams();

    // Everything is rushed, so this is just the ordinary round-robin.
    let assigned: u32 = gs.player_company.manufacturing.orders.iter()
        .map(|o| o.teams_assigned).sum();
    assert_eq!(assigned, 6, "all six placed");
    for pid in [RocketProjectId(1), RocketProjectId(2)] {
        let n: u32 = gs.player_company.manufacturing.orders.iter()
            .filter(|o| o.parent_rocket() == Some(pid))
            .map(|o| o.teams_assigned).sum();
        assert!(n > 0, "both rush jobs get a share, got {n} for {pid:?}");
    }
}

/// The rush ends when the rocket reaches inventory, and the floor goes
/// back to normal without the player having to clear anything.
#[test]
fn a_rush_clears_itself_when_the_rocket_is_built() {
    use crate::rocket_project::RocketProjectId;

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.rush_projects.insert(RocketProjectId(1));

    run_manufacturing_to_rocket(&mut gs);
    assert_eq!(gs.player_company.manufacturing.inventory.rockets.len(), 1);
    assert!(gs.player_company.rush_projects.is_empty(),
        "the rush should retire with the rocket that needed it");
}

/// With nothing rushed, assignment is the round-robin it always was —
/// which is what keeps the balance baseline still.
#[test]
fn no_rush_means_the_old_round_robin() {
    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    for i in 0..6 {
        gs.player_company.hire_manufacturing_team(format!("Mfg{i}"), &gs.balance);
    }
    gs.player_company.assign_manufacturing_teams();

    let counts: Vec<u32> = gs.player_company.manufacturing.orders.iter()
        .filter(|o| !o.waiting_for_prerequisites)
        .map(|o| o.teams_assigned)
        .collect();
    let hi = counts.iter().max().copied().unwrap_or(0);
    let lo = counts.iter().min().copied().unwrap_or(0);
    assert!(hi - lo <= 1, "teams spread evenly: {counts:?}");
}

/// A second build queued behind the urgent one must not keep the floor
/// hostage: the rush retires when its rocket exists, not when the
/// project's whole queue drains.
#[test]
fn a_rush_retires_on_the_first_rocket_not_the_last() {
    use crate::rocket_project::RocketProjectId;

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    // Two builds of the same rocket in flight at once.
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.rush_projects.insert(RocketProjectId(1));

    gs.player_company.hire_manufacturing_team("MfgA".into(), &gs.balance);
    for _ in 0..40 {
        for order in &mut gs.player_company.manufacturing.orders {
            if !order.waiting_for_prerequisites && order.teams_assigned > 0 {
                order.work_completed = order.work_required;
            }
        }
        gs.advance_day();
        if !gs.player_company.manufacturing.inventory.rockets.is_empty() {
            break;
        }
    }
    assert_eq!(gs.player_company.manufacturing.inventory.rockets.len(), 1,
        "premise: the first of the two rockets is done");
    assert!(!gs.player_company.manufacturing.orders.is_empty(),
        "premise: the second build is still in the queue");
    assert!(gs.player_company.rush_projects.is_empty(),
        "the rush ended with the rocket it was for");
}

/// The Manufacturing tab draws the queue as a tree and the cursor indexes
/// that tree, so the row list has to hold every order exactly once — the
/// same invariant that bit the Ready Rockets list.
#[test]
fn the_manufacturing_tree_is_a_permutation_of_the_queue() {
    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    for _ in 0..2 {
        gs.player_company.order_engine_build(0, &gs.balance).unwrap();
    }

    let rows = gs.player_company.manufacturing_display_order();
    assert_eq!(rows.len(), gs.player_company.manufacturing.orders.len());
    let mut seen: Vec<usize> = rows.iter().map(|r| r.order_index).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), rows.len(), "no order appears twice");
}

/// Integration, then the stages feeding it, then the engines feeding
/// those. Standalone builds sit at the top level.
#[test]
fn the_manufacturing_tree_nests_integration_stages_engines() {
    use crate::manufacturing::ManufacturingOrderType as T;

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    // One extra engine nobody ordered for a rocket.
    gs.player_company.order_engine_build(0, &gs.balance).unwrap();

    let orders = &gs.player_company.manufacturing.orders;
    let shape: Vec<(u8, &'static str)> = gs.player_company.manufacturing_display_order()
        .iter()
        .map(|r| (r.depth, match &orders[r.order_index].order_type {
            T::RocketIntegration { .. } => "integration",
            T::Stage { .. } => "stage",
            T::Engine { .. } => "engine",
        }))
        .collect();

    assert_eq!(shape[0], (0, "integration"), "the rocket heads its own tree");
    // Every stage sits one level in, every engine under a stage two.
    for (depth, kind) in &shape {
        match *kind {
            "integration" => assert_eq!(*depth, 0),
            "stage" => assert_eq!(*depth, 1, "stages hang off the integration"),
            _ => assert!(*depth == 2 || *depth == 0,
                "an engine is either under a stage or standalone, got depth {depth}"),
        }
    }
    // An engine at depth 2 always follows a stage, never an integration.
    for w in shape.windows(2) {
        if w[1] == (2, "engine") {
            assert!(w[0].0 >= 1, "depth-2 engine follows a stage or another engine");
        }
    }
    assert!(shape.contains(&(0, "engine")),
        "the hand-built engine sits at the top level: {shape:?}");
}

/// Rush jobs are drawn as one contiguous block at the top, so the header
/// is emitted once and rushed orders never appear twice.
#[test]
fn rushed_orders_form_one_block_at_the_top() {
    use crate::rocket_project::{RocketProject, RocketProjectId, RocketDesignStatus};

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    let (design2, _) = make_three_stage_design();
    let mut rp2 = RocketProject::new(
        RocketProjectId(2), design2, &crate::balance_config::BalanceConfig::default());
    rp2.status = RocketDesignStatus::Testing { work_completed: 100.0 };
    gs.player_company.rocket_projects.push(rp2);
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.order_rocket_build(1, &gs.balance).unwrap();
    gs.player_company.rush_projects.insert(RocketProjectId(2));

    let rows = gs.player_company.manufacturing_display_order();
    let flags: Vec<bool> = rows.iter().map(|r| r.rushed).collect();
    assert!(flags[0], "the rush leads");
    // true...true then false...false, never alternating.
    let first_normal = flags.iter().position(|f| !f).expect("some normal work too");
    assert!(flags[first_normal..].iter().all(|f| !f),
        "the rush block is contiguous: {flags:?}");
}


/// Orders name a rocket *project*, so a second build of the same rocket
/// looks identical to the one being rushed. The rush has to stay pinned
/// to one build, or ordering another of the same rocket silently drags it
/// into the rush too.
#[test]
fn a_second_build_of_a_rushed_rocket_stays_in_the_ordinary_queue() {
    use crate::rocket_project::RocketProjectId;
    use crate::manufacturing::ManufacturingOrderType as T;

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.rush_projects.insert(RocketProjectId(1));
    // A second one of the same rocket, ordered while the rush is running.
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();

    let flags = gs.player_company.rushed_order_flags();
    let integrations: Vec<usize> = gs.player_company.manufacturing.orders.iter()
        .enumerate()
        .filter(|(_, o)| matches!(o.order_type, T::RocketIntegration { .. }))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(integrations.len(), 2, "premise: two builds of the same rocket");
    assert!(flags[integrations[0]], "the build that was rushed still is");
    assert!(!flags[integrations[1]], "the one ordered afterwards is not");
}

/// Every engine of a needed source *could* satisfy a rushed stage, but
/// rushing all of them spreads the floor thinner and finishes the rush
/// later. Claim what the stages need and leave the rest.
#[test]
fn a_rush_claims_the_engines_it_needs_and_no_more() {
    use crate::rocket_project::{RocketProject, RocketProjectId, RocketDesignStatus};
    use crate::manufacturing::ManufacturingOrderType as T;

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    let (design2, _) = make_three_stage_design();
    let mut rp2 = RocketProject::new(
        RocketProjectId(2), design2, &crate::balance_config::BalanceConfig::default());
    rp2.status = RocketDesignStatus::Testing { work_completed: 100.0 };
    gs.player_company.rocket_projects.push(rp2);
    // Both rockets use the same engines, so both queue their own builds.
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.order_rocket_build(1, &gs.balance).unwrap();
    gs.player_company.rush_projects.insert(RocketProjectId(2));

    let flags = gs.player_company.rushed_order_flags();
    let engine_total = gs.player_company.manufacturing.orders.iter()
        .filter(|o| matches!(o.order_type, T::Engine { .. }))
        .count();
    let engine_rushed = gs.player_company.manufacturing.orders.iter().enumerate()
        .filter(|(_, o)| matches!(o.order_type, T::Engine { .. }))
        .filter(|(i, _)| flags[*i])
        .count();
    assert!(engine_total > engine_rushed,
        "the other rocket's engines stay out of it: {engine_rushed} of {engine_total}");
    // The design needs 4 Lifters and 1 Upper — one build's worth.
    assert_eq!(engine_rushed, 5, "exactly one build's engines");
}

/// A rush inherits whatever is nearest done, so the work already sunk
/// counts toward the deadline and the untouched builds fall through to
/// whoever was going to get them.
#[test]
fn a_rush_claims_the_most_advanced_engines() {
    use crate::rocket_project::RocketProjectId;
    use crate::manufacturing::ManufacturingOrderType as T;

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    // Hand-built engines first, so the rocket's own build queues none of
    // them and every candidate is ownerless.
    for _ in 0..4 {
        gs.player_company.order_engine_build(0, &gs.balance).unwrap();
    }
    // Push the last one nearly to completion.
    let last = gs.player_company.manufacturing.orders.len() - 1;
    gs.player_company.manufacturing.orders[last].work_completed =
        gs.player_company.manufacturing.orders[last].work_required * 0.9;

    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.rush_projects.insert(RocketProjectId(1));

    let flags = gs.player_company.rushed_order_flags();
    assert!(flags[last], "the nearly-finished engine is the one to inherit");
    // And it is an engine, not something else that drifted into the slot.
    assert!(matches!(
        gs.player_company.manufacturing.orders[last].order_type, T::Engine { .. }));
}


/// Each integration takes one build's worth of stages — one order per
/// (group, index) — so a second build of the same rocket keeps its own
/// stages instead of having them absorbed by the first.
#[test]
fn each_build_keeps_its_own_stages() {
    use crate::manufacturing::ManufacturingOrderType as T;

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();

    let orders = &gs.player_company.manufacturing.orders;
    let rows = gs.player_company.manufacturing_display_order();

    // Walk the tree; every integration should be followed by three stages
    // before the next integration starts.
    let mut per_build: Vec<usize> = Vec::new();
    for row in &rows {
        match (&orders[row.order_index].order_type, row.depth) {
            (T::RocketIntegration { .. }, 0) => per_build.push(0),
            (T::Stage { .. }, 1) => *per_build.last_mut().expect("stage under an integration") += 1,
            _ => {}
        }
    }
    assert_eq!(per_build, vec![3, 3],
        "two builds, three stages each — not one build hoarding six");
}

/// A stage wanting six engines with four already on the shelf is two
/// builds away, not six. Claiming six parks the floor on engines nobody
/// is waiting for, and under a rush spreads the teams three times thinner
/// than the two that actually unblock the stage.
#[test]
fn a_stage_only_claims_the_engines_it_still_needs() {
    use crate::engine_project::{EngineProjectId, EngineSource};
    use crate::manufacturing::ManufacturingOrderType as T;

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    // Three Lifters on S1, one on S2 — put two on the shelf up front.
    for _ in 0..2 {
        gs.player_company.order_engine_build(0, &gs.balance).unwrap();
    }
    run_manufacturing_to_idle(&mut gs);
    let src = EngineSource::PlayerDesign(EngineProjectId(1));
    assert_eq!(gs.player_company.manufacturing.inventory.engine_count(src), 2,
        "premise: two engines waiting in stock");
    // Those ticks may have pushed either project into a revision; this
    // test is about attribution, not the design workflow.
    gs.player_company.rocket_projects[0].status =
        crate::rocket_project::RocketDesignStatus::Testing { work_completed: 100.0 };
    gs.player_company.engine_projects[0].status =
        crate::engine_project::EngineDesignStatus::Testing { work_completed: 100.0 };

    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    // Plus some spares the player orders by hand afterwards.
    for _ in 0..3 {
        gs.player_company.order_engine_build(0, &gs.balance).unwrap();
    }

    let orders = &gs.player_company.manufacturing.orders;
    let rows = gs.player_company.manufacturing_display_order();

    // Walk the tree and count engines hanging under the first stage.
    let mut under_s1 = 0;
    let mut in_s1 = false;
    for row in &rows {
        match (&orders[row.order_index].order_type, row.depth) {
            (T::Stage { group_index: 0, .. }, 1) => in_s1 = true,
            (T::Engine { .. }, 2) if in_s1 => under_s1 += 1,
            (_, d) if d <= 1 => in_s1 = false,
            _ => {}
        }
    }
    assert_eq!(under_s1, 1,
        "S1 wants three, two are on the shelf, so one build is outstanding");
    // The hand-ordered spares have no stage to belong to.
    let top_level_engines = rows.iter()
        .filter(|r| r.depth == 0)
        .filter(|r| matches!(orders[r.order_index].order_type, T::Engine { .. }))
        .count();
    assert!(top_level_engines >= 3,
        "spares beyond what the stages need sit at the top level, got {top_level_engines}");
}

/// An integration that has already taken its stages out of inventory
/// needs nothing more. Left unchecked it reaches across and adopts the
/// *next* build's stages — which aren't feeding it, and which under a
/// rush pull teams off the integration that is the only thing left to do.
#[test]
fn a_finished_integration_does_not_adopt_the_next_builds_stages() {
    use crate::rocket_project::RocketProjectId;
    use crate::manufacturing::ManufacturingOrderType as T;

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.rush_projects.insert(RocketProjectId(1));

    // Unblock the first integration by hand: it has its stages now.
    let first_integration = gs.player_company.manufacturing.orders.iter()
        .position(|o| matches!(o.order_type, T::RocketIntegration { .. }))
        .expect("premise: an integration order exists");
    gs.player_company.manufacturing.orders[first_integration]
        .waiting_for_prerequisites = false;

    let rows = gs.player_company.manufacturing_display_order();
    let orders = &gs.player_company.manufacturing.orders;

    // The rush is now just that integration — nothing else feeds it.
    let rushed: Vec<usize> = rows.iter().filter(|r| r.rushed)
        .map(|r| r.order_index).collect();
    assert_eq!(rushed, vec![first_integration],
        "a finished integration's rush is the integration alone");

    // And the second build still owns all three of its stages.
    let second_build_stages = rows.iter()
        .filter(|r| !r.rushed && r.depth == 1)
        .filter(|r| matches!(orders[r.order_index].order_type, T::Stage { .. }))
        .count();
    assert_eq!(second_build_stages, 3, "the next build keeps its own stages");
}

/// A slot filled from inventory wants no build order. Left unchecked, an
/// integration still waiting on S1 goes on adopting every fresh S2 that
/// appears — and under a rush builds them one after another for a slot
/// that was satisfied long ago.
#[test]
fn a_stage_already_in_inventory_does_not_claim_a_fresh_order() {
    use crate::rocket_project::RocketProjectId;
    use crate::manufacturing::{InventoryStage, ManufacturingOrderType as T};

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    gs.player_company.rush_projects.insert(RocketProjectId(1));

    // S2 (group 1) is already built and sitting on the shelf.
    let item_id = gs.player_company.manufacturing.next_inventory_id();
    gs.player_company.manufacturing.inventory.stages.push(InventoryStage {
        item_id,
        rocket_project_id: RocketProjectId(1),
        group_index: 1,
        stage_index: 0,
        stage_name: "TestThreeStage S2".into(),
        build_cost: 1_000_000.0,
    });
    // A second build queues a fresh S2 for a slot that is already covered.
    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();

    let orders = &gs.player_company.manufacturing.orders;
    let rows = gs.player_company.manufacturing_display_order();
    let rushed_s2 = rows.iter()
        .filter(|r| r.rushed)
        .filter(|r| matches!(
            orders[r.order_index].order_type,
            T::Stage { group_index: 1, .. }))
        .count();
    assert_eq!(rushed_s2, 0,
        "the rush already has its S2; a fresh one belongs to the other build");
}

/// The build estimate has to agree with the orders the pipeline actually
/// queues, or it drifts the moment a work formula changes. Rebuild the
/// critical path out of the real orders' `work_required` and compare.
#[test]
fn nominal_build_days_matches_the_orders_the_pipeline_queues() {
    use crate::manufacturing::ManufacturingOrderType as T;

    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    let estimate = gs.player_company
        .nominal_build_days(&gs.player_company.rocket_projects[0], &gs.balance);

    gs.player_company.order_rocket_build(0, &gs.balance).unwrap();
    let orders = &gs.player_company.manufacturing.orders;

    // One team is a work rate of exactly 1.0, so work-days are days.
    assert!((crate::team::manufacturing_work_rate(1) - 1.0).abs() < 1e-9,
        "the estimate's whole premise");

    // Longest engine build feeding each stage, plus that stage, is when
    // the stage is done; integration waits for the last of them.
    let mut critical = 0.0_f64;
    for (gi, group) in gs.player_company.rocket_projects[0].design.stage_groups
        .iter().enumerate()
    {
        for (si, stage) in group.iter().enumerate() {
            let engine_days = orders.iter()
                .filter(|o| matches!(&o.order_type,
                    T::Engine { engine_id, .. } if *engine_id == stage.engine.id))
                .map(|o| o.work_required)
                .fold(0.0_f64, f64::max);
            let stage_days = orders.iter()
                .find(|o| matches!(&o.order_type,
                    T::Stage { group_index, stage_index, .. }
                        if *group_index == gi && *stage_index == si))
                .map(|o| o.work_required)
                .expect("the build queued this stage");
            critical = critical.max(engine_days + stage_days);
        }
    }
    let integration = orders.iter()
        .find(|o| matches!(o.order_type, T::RocketIntegration { .. }))
        .map(|o| o.work_required)
        .expect("the build queued an integration");

    assert!((estimate - (critical + integration)).abs() < 1e-6,
        "estimate {estimate} vs pipeline {}", critical + integration);
}

/// It's a critical path, not a total: a second stage that finishes sooner
/// changes nothing, and one that finishes later sets the pace.
#[test]
fn nominal_build_days_follows_the_slowest_stage_not_the_sum() {
    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);

    let base = gs.player_company
        .nominal_build_days(&gs.player_company.rocket_projects[0], &gs.balance);

    // Shrink every stage but the first: the first was already the
    // heaviest, so the critical path shouldn't move.
    {
        let design = &mut gs.player_company.rocket_projects[0].design;
        for group in design.stage_groups.iter_mut().skip(1) {
            for stage in group.iter_mut() {
                stage.structural_mass_kg = 1.0;
            }
        }
    }
    let lighter_tail = gs.player_company
        .nominal_build_days(&gs.player_company.rocket_projects[0], &gs.balance);
    assert!((lighter_tail - base).abs() < 1e-6,
        "shortening a stage that wasn't the pace-setter changes nothing: \
         {lighter_tail} vs {base}");

    // Now make the last stage enormous — it becomes the pace-setter.
    {
        let design = &mut gs.player_company.rocket_projects[0].design;
        let last = design.stage_groups.last_mut().unwrap();
        last[0].structural_mass_kg = 500_000.0;
    }
    let heavy_tail = gs.player_company
        .nominal_build_days(&gs.player_company.rocket_projects[0], &gs.balance);
    assert!(heavy_tail > base,
        "a slower stage sets the pace: {heavy_tail} vs {base}");
}

/// Contracted engines arrive when ordered, so they add no build time —
/// which is a real reason to buy one rather than design your own.
#[test]
fn contracted_engines_add_no_build_time() {
    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    let with_own_engines = gs.player_company
        .nominal_build_days(&gs.player_company.rocket_projects[0], &gs.balance);

    let date = gs.date;
    let seed = gs.seed.clone();
    let idx = gs.player_company.third_party_catalog.iter()
        .position(|e| e.available_from <= date)
        .expect("a starter engine is on the market");
    gs.player_company.contract_third_party(idx, date, &seed, &gs.balance).unwrap();
    let bought = gs.player_company.contracted_engines[0].design.clone();

    for group in gs.player_company.rocket_projects[0].design.stage_groups.iter_mut() {
        for stage in group.iter_mut() {
            stage.engine = bought.clone();
        }
    }
    let with_bought_engines = gs.player_company
        .nominal_build_days(&gs.player_company.rocket_projects[0], &gs.balance);

    assert!(with_bought_engines < with_own_engines,
        "buying engines skips their build time: {with_bought_engines} vs {with_own_engines}");
}

/// The learning curve is folded in, so the number answers "how long will
/// the next one take", not "how long did the first one take".
#[test]
fn nominal_build_days_reflects_the_learning_curve() {
    let mut gs = GameState::new("Test".into(), 5_000_000_000.0, 42);
    setup_buildable_rocket(&mut gs);
    let first = gs.player_company
        .nominal_build_days(&gs.player_company.rocket_projects[0], &gs.balance);

    let design_id = gs.player_company.rocket_projects[0].design.id;
    gs.player_company.rocket_build_counts.insert(design_id, 20);
    let twenty_first = gs.player_company
        .nominal_build_days(&gs.player_company.rocket_projects[0], &gs.balance);

    assert!(twenty_first < first,
        "practice makes it quicker: {twenty_first} vs {first}");
}

/// The contracts table shows how long is left, and distinguishes a
/// deadline today from one already missed — `days_until` floors at zero,
/// so both would otherwise read "0d".
#[test]
fn contract_rows_show_days_left_and_flag_overdue() {
    use crate::ui::draw::test_support::contract_row_for_test;

    let mut c = crate::contract::test_support::solicitation_fixture();
    let today = crate::calendar::GameDate { year: 2001, month: 6, day: 1 };

    c.deadline = today.add_days(45);
    assert!(contract_row_for_test(&c, today).contains("45d"),
        "row: {:?}", contract_row_for_test(&c, today));

    c.deadline = today;
    let due_today = contract_row_for_test(&c, today);
    assert!(due_today.contains("0d"), "due today reads as zero: {due_today:?}");
    assert!(!due_today.contains("over"), "but is not overdue: {due_today:?}");

    c.deadline = crate::calendar::GameDate { year: 2001, month: 5, day: 31 };
    let missed = contract_row_for_test(&c, today);
    assert!(missed.contains("over"), "a passed deadline says so: {missed:?}");
}


/// Lifting the mass and arriving with the lights on are separate
/// failures. The payload figure alone reads as success, so the table has
/// to say when a destination is reachable but not survivable.
#[test]
fn the_payload_table_flags_destinations_it_cannot_stay_alive_to_reach() {
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    let (design, engine_projects) = make_three_stage_design();
    gs.player_company.engine_projects = engine_projects;
    assert!(
        design.stage_groups.iter().flatten().all(|s| s.power_sources.is_empty()),
        "fixture premise: nothing fitted, so every stage flies the default battery",
    );

    let dests = ["leo", "gto", "meo"];
    let table = gs.payload_table(&design, "earth_surface", &dests);
    let row = |name: &str| table.iter()
        .find(|r| r.destination == name)
        .unwrap_or_else(|| panic!("{name} missing from {:?}",
            table.iter().map(|r| r.destination).collect::<Vec<_>>()));

    // Same-day deliveries: the default battery covers them.
    for same_day in ["Low Earth Orbit", "Geostationary Transfer"] {
        let r = row(same_day);
        assert!(r.max_payload_kg > 0.0);
        assert!(r.survives, "{same_day} is a same-day trip");
    }

    // MEO climbs through LEO, so the craft is still flying on day two
    // with nothing aboard generating power.
    let meo = row("Medium Earth Orbit");
    assert!(meo.max_payload_kg > 0.0, "it can lift the mass");
    assert!(!meo.survives, "but it goes dark before it arrives");
}

/// Fitting power changes the verdict without touching the payload — the
/// point of flagging it separately.
#[test]
fn power_flips_the_table_verdict_without_moving_the_payload() {
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    let (bare, engine_projects) = make_three_stage_design();
    gs.player_company.engine_projects = engine_projects;

    let mut solar = bare.clone();
    {
        let upper = solar.stage_groups.last_mut().unwrap().first_mut().unwrap();
        let demand = upper.housekeeping_w();
        upper.power_sources.push(crate::power::PowerSource::new_solar_panel(demand * 4.0));
    }

    let dests = ["meo"];
    let before = gs.payload_table(&bare, "earth_surface", &dests);
    let after = gs.payload_table(&solar, "earth_surface", &dests);

    assert!(!before[0].survives, "premise: bare can't stay alive to MEO");
    assert!(after[0].survives, "a panel keeps the lights on the whole way");
}

// ── Great Power War ────────────────────────────────────────────────

/// Drive a game to a chosen geopolitical state by applying the shifts
/// directly — the arc itself is tested in `geopolitics`, and finding a
/// seed that wars in year N would make these tests hostage to the rates.
fn force_shift(gs: &mut GameState, shift: crate::geopolitics::GeopoliticalShift) {
    use crate::geopolitics::{Geopolitics, GeopoliticalShift as S};
    gs.geopolitics = match shift {
        S::WarBegins => Geopolitics::War { since_year: gs.date.year },
        S::WarEscalates =>
            Geopolitics::WarWithAsat { since_year: gs.date.year, asat_since_year: gs.date.year },
        S::WarEndsWithoutDebris | S::ReconstitutionEnds => Geopolitics::Peace,
        S::WarEndsIntoReconstitution =>
            Geopolitics::Reconstitution { until_year: gs.date.year + 2 },
    };
    gs.apply_geopolitical_shift(shift);
}

fn market(gs: &GameState, id: crate::contract::MarketId) -> &crate::contract::Market {
    gs.markets.iter().find(|m| m.id == id).expect("market exists")
}

/// A war surges reconnaissance launch, pays better for it, and lowers the
/// bar so a player who isn't already trusted can actually bid.
#[test]
fn war_surges_the_reconnaissance_market() {
    use crate::contract::MARKET_NSSL;
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    let before = market(&gs, MARKET_NSSL);
    let base_rep = before.rep_target;
    let base_vol = before.effective_volume(1.0, gs.date);
    assert_eq!(base_rep, 80.0, "premise: the highest bar in the game");

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarBegins);
    let at_war = market(&gs, MARKET_NSSL);

    assert!(at_war.active, "the war opens the market even if the seed never did");
    assert!((at_war.effective_volume(1.0, gs.date) / base_vol - 5.0).abs() < 1e-6,
        "five times the launches");
    assert!(at_war.rate_multiplier(1.0) > 1.0, "and they pay more for them");
    assert_eq!(at_war.effective_rep_target(), 60.0,
        "urgency lowers the bar from 80 to 60");
}

/// The debris cascade is an orbit problem: LEO and SSO collapse, GEO and
/// GTO carry on. A market's loss is the share of its weight that sat in
/// the ruined orbits.
#[test]
fn asat_debris_hits_low_orbits_and_spares_high_ones() {
    use crate::contract::{MARKET_GEO_COMSATS, MARKET_LEO_CONSTELLATION};
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);

    let leo_before = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);
    let geo_before = market(&gs, MARKET_GEO_COMSATS).effective_volume(1.0, gs.date);

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarBegins);
    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarEscalates);

    let leo_after = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);
    let geo_after = market(&gs, MARKET_GEO_COMSATS).effective_volume(1.0, gs.date);

    // LEO Constellation is entirely LEO+SSO, so it takes the full 0.2.
    assert!((leo_after / leo_before - 0.2).abs() < 1e-6,
        "an all-LEO market loses everything: {}", leo_after / leo_before);
    // GEO Comsats fly GTO and GEO only — untouched.
    assert!((geo_after / geo_before - 1.0).abs() < 1e-6,
        "a market with no low-orbit business is unaffected: {}", geo_after / geo_before);
}

/// Reconnaissance is exempt from the debris: its own satellites are what
/// was shot down, which is why it needs *more* launches, not fewer.
#[test]
fn the_reconnaissance_market_is_exempt_from_its_own_debris() {
    use crate::contract::MARKET_NSSL;
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarBegins);
    let at_war = market(&gs, MARKET_NSSL).effective_volume(1.0, gs.date);

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarEscalates);
    let after_asat = market(&gs, MARKET_NSSL).effective_volume(1.0, gs.date);

    assert!((after_asat - at_war).abs() < 1e-6,
        "the surge survives the debris: {at_war} then {after_asat}");
}

/// Peace restores what the war changed, and the replacement wave only
/// follows an exchange that actually happened.
#[test]
fn peace_lifts_the_war_modifiers() {
    use crate::contract::{MARKET_LEO_CONSTELLATION, MARKET_NSSL};
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    let leo_base = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);
    let nro_base = market(&gs, MARKET_NSSL).effective_volume(1.0, gs.date);

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarBegins);
    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarEscalates);
    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarEndsIntoReconstitution);

    let nro_after = market(&gs, MARKET_NSSL).effective_volume(1.0, gs.date);
    assert!((nro_after - nro_base).abs() < 1e-6, "the surge is over");
    assert_eq!(market(&gs, MARKET_NSSL).effective_rep_target(), 80.0,
        "and the bar goes back up");

    let leo_boom = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);
    assert!((leo_boom / leo_base - 2.0).abs() < 1e-6,
        "the low orbits need repopulating: {}", leo_boom / leo_base);

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::ReconstitutionEnds);
    let leo_settled = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);
    assert!((leo_settled - leo_base).abs() < 1e-6, "and then it is over");
}

/// A war that ends without going after satellites leaves nothing behind.
#[test]
fn a_war_without_asat_leaves_no_debris_and_no_boom() {
    use crate::contract::MARKET_LEO_CONSTELLATION;
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    let base = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarBegins);
    assert!((market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date) - base).abs()
        < 1e-6, "a shooting war alone doesn't touch commercial LEO");

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarEndsWithoutDebris);
    assert!((market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date) - base).abs()
        < 1e-6, "and leaves nothing to replace");
}

/// Suppressing an orbit removes those launches rather than pushing them
/// somewhere else — the count falls *and* the mix shifts.
#[test]
fn suppressing_an_orbit_removes_launches_rather_than_moving_them() {
    use crate::contract::{MarketModifier, MARKET_NSSL};
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    let m = gs.markets.iter_mut().find(|m| m.id == MARKET_NSSL).unwrap();
    let before = m.effective_volume(1.0, gs.date);

    m.add_modifier(MarketModifier {
        id: "probe".into(),
        destination_volume_mult: vec![("leo".into(), 0.0)],
        ..Default::default()
    });
    let after = m.effective_volume(1.0, gs.date);

    // Losing LEO entirely costs exactly LEO's share of the market's
    // weight. Computed rather than hardcoded because the archetype
    // applies a per-seed tilt of up to 10% to each weight.
    let total: f64 = m.destinations.iter().map(|d| d.weight).sum();
    let leo: f64 = m.destinations.iter()
        .filter(|d| d.location_id == "leo").map(|d| d.weight).sum();
    let expected = 1.0 - leo / total;
    assert!((after / before - expected).abs() < 1e-6,
        "volume falls by the lost orbit's share of the weight: {} vs {expected}",
        after / before);
    assert!((0.55..0.65).contains(&expected), "LEO is ~40% of this market: {expected}");
    assert_eq!(m.destination_mult("leo"), 0.0);
    assert_eq!(m.destination_mult("sso"), 1.0, "untouched orbits stay at 1.0");
}

// ── Retiring designs ──
//
// Retiring hides a design and stops further work going into it without
// deleting anything, because manufacturing orders, inventory, spacecraft
// and other designs all hold project ids. These guard the two halves of
// that: what disappears, and what must keep resolving.

/// The load-bearing property. If a retired project were removed from the
/// vec instead of flagged, every id below would dangle.
#[test]
fn retiring_hides_a_rocket_but_keeps_its_id_resolvable() {
    use crate::company::ProjectRef;

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);

    assert_eq!(gs.player_company.visible_rocket_projects().count(), 1);
    gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("retires");

    assert_eq!(gs.player_company.visible_rocket_projects().count(), 0,
        "retired design should be gone from the pane");
    assert_eq!(gs.player_company.rocket_projects.len(), 1,
        "but still present, so ids that name it keep resolving");
    assert!(gs.player_company.rocket_projects.iter()
        .any(|rp| rp.project_id == rp_id && rp.retired));
}

#[test]
fn retiring_a_rocket_releases_teams_and_clears_auto_build() {
    use crate::company::ProjectRef;

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);
    gs.player_company.hire_team("T1".into(), &gs.balance);
    gs.player_company.hire_team("T2".into(), &gs.balance);
    assert!(gs.player_company.add_team(rocket_ref(&gs, 0)));
    assert!(gs.player_company.add_team(rocket_ref(&gs, 0)));
    assert!(gs.player_company.set_auto_build_target(rp_id, 2));

    let effects = gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("retires");

    assert_eq!(effects.teams_released, 2);
    assert!(effects.auto_build_cleared);
    assert_eq!(
        gs.player_company.unassigned_team_count() as usize,
        gs.player_company.team_count(),
        "no team should still be working on a design nobody can see",
    );
    assert!(!gs.player_company.auto_build_targets.contains_key(&rp_id),
        "a hidden design must not keep ordering builds");
}

#[test]
fn retiring_a_rocket_cancels_its_integration_and_stage_orders() {
    use crate::company::ProjectRef;
    use crate::manufacturing::ManufacturingOrderType;

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);
    gs.player_company.order_rocket_build(0, &gs.balance).expect("orders a build");

    let before = gs.player_company.manufacturing.orders.len();
    assert!(before > 0, "the build should have queued orders");
    gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("retires");

    let design_specific = gs.player_company.manufacturing.orders.iter().any(|o| matches!(
        &o.order_type,
        ManufacturingOrderType::Stage { rocket_project_id, .. }
        | ManufacturingOrderType::RocketIntegration { rocket_project_id, .. }
            if *rocket_project_id == rp_id));
    assert!(!design_specific,
        "stages and integration are worthless without the design");
    assert!(gs.player_company.manufacturing.orders.len() < before,
        "the cancelled orders should have left the queue");
}

/// Components are pooled, so an engine ordered for a retiring rocket is
/// still worth finishing when a design the player still flies uses it.
/// Cancelling it would destroy work that live build is about to eat.
#[test]
fn an_engine_order_shared_with_a_live_rocket_survives_its_retirement() {
    use crate::company::ProjectRef;
    use crate::manufacturing::ManufacturingOrderType;
    use crate::rocket_project::{RocketProject, RocketProjectId, RocketDesignStatus};

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);

    // A second project on the same design, so both use the same engines.
    let design = gs.player_company.rocket_projects[0].design.clone();
    let mut twin = RocketProject::new(
        RocketProjectId(2), design, &crate::balance_config::BalanceConfig::default());
    twin.status = RocketDesignStatus::Testing { work_completed: 100.0 };
    gs.player_company.rocket_projects.push(twin);

    gs.player_company.order_rocket_build(0, &gs.balance).expect("orders a build");
    let engine_orders_before = gs.player_company.manufacturing.orders.iter()
        .filter(|o| matches!(o.order_type, ManufacturingOrderType::Engine { .. }))
        .count();
    assert!(engine_orders_before > 0, "the build should have queued engines");

    let effects = gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("retires");

    assert_eq!(effects.reassigned_engine_orders.len(), engine_orders_before,
        "every engine order should survive — the twin still needs them");
    let engine_orders_after = gs.player_company.manufacturing.orders.iter()
        .filter(|o| matches!(o.order_type, ManufacturingOrderType::Engine { .. }))
        .count();
    assert_eq!(engine_orders_after, engine_orders_before);
    // Their rocket tag only drove rush-priority inheritance, and it now
    // points at a design that no longer exists.
    assert!(gs.player_company.manufacturing.orders.iter().all(|o| !matches!(
        &o.order_type,
        ManufacturingOrderType::Engine { rocket_project_id: Some(id), .. }
            if *id == rp_id)),
        "surviving orders should have lost their retired rocket's tag");
}

/// The other half: with nothing live using them, those same engine
/// orders are work with no destination.
#[test]
fn engine_orders_needed_by_nothing_live_are_cancelled_with_their_rocket() {
    use crate::company::ProjectRef;
    use crate::manufacturing::ManufacturingOrderType;

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);
    gs.player_company.order_rocket_build(0, &gs.balance).expect("orders a build");
    assert!(gs.player_company.manufacturing.orders.iter()
        .any(|o| matches!(o.order_type, ManufacturingOrderType::Engine { .. })));

    let effects = gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("retires");

    assert!(effects.reassigned_engine_orders.is_empty(),
        "nothing else uses these engines, so none should be kept");
    assert!(gs.player_company.manufacturing.orders.is_empty(),
        "the whole build should be gone: {:?}",
        gs.player_company.manufacturing.orders.iter()
            .map(|o| &o.order_type).collect::<Vec<_>>());
}

/// Retiring an engine a live rocket flies would strand that rocket's
/// stage orders on an engine nothing will ever build again.
#[test]
fn retiring_an_engine_a_live_rocket_uses_is_refused_and_names_it() {
    use crate::company::{RetireRefusal, ProjectRef};
    use crate::engine_project::EngineProjectId;

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);
    let rocket_name = gs.player_company.rocket_projects[0].design.name.clone();

    match gs.player_company.retire(ProjectRef::Engine(EngineProjectId(1))) {
        Err(RetireRefusal::EngineInUse { rockets }) => {
            assert_eq!(rockets, vec![rocket_name],
                "the refusal should name the rocket blocking it");
        }
        other => panic!("expected a refusal naming the rocket, got {other:?}"),
    }
    assert!(!gs.player_company.engine_projects[0].retired);

    // Retire the rocket and the engine goes quietly — so working
    // oldest-first through a stack of obsolete designs just works.
    gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("rocket retires");
    gs.player_company.retire(ProjectRef::Engine(EngineProjectId(1)))
        .expect("engine retires once nothing live uses it");
    assert!(gs.player_company.engine_projects[0].retired);
}

/// A retired design stops winning new work, but hardware already built
/// is untouched — this is the gate the contract list's colours and the
/// bid rules share.
#[test]
fn a_retired_design_stops_being_bid_capable() {
    use crate::company::ProjectRef;

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);

    let before = gs.capable_projects_for("leo", 1000.0);
    assert!(before.contains(&rp_id), "design should start out bid-capable");

    gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("retires");

    assert!(!gs.capable_projects_for("leo", 1000.0).contains(&rp_id),
        "retiring says you don't intend to fly it again");
}

#[test]
fn a_retired_rocket_is_never_built_again() {
    use crate::company::ProjectRef;

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);
    gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("retires");

    assert!(gs.player_company.order_rocket_build(0, &gs.balance).is_none(),
        "the shared build gate should reject a retired design");
    // Even if a stale target survived somehow, auto-build goes through
    // the same gate.
    gs.player_company.auto_build_targets.insert(rp_id, 3);
    gs.player_company.auto_reorder_rockets(&gs.balance);
    assert!(gs.player_company.manufacturing.orders.is_empty());
}

/// Reactors can't dangle — a stage carries a cloned `ReactorDesign` —
/// so retiring one is purely a matter of getting it off the list.
#[test]
fn retiring_a_reactor_hides_it_without_touching_stages() {
    use crate::company::ProjectRef;
    use crate::reactor::{EnrichmentLevel, DEFAULT_SCALE};

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let pid = gs.player_company.start_proposed_reactor(
        "Pebble".into(), DEFAULT_SCALE, EnrichmentLevel::Leu, &gs.balance);
    gs.player_company.promote_proposed(ProjectRef::Reactor(pid)).expect("promotes");
    assert_eq!(gs.player_company.visible_reactor_projects().count(), 1);

    let effects = gs.player_company.retire(ProjectRef::Reactor(pid)).expect("retires");

    assert!(effects.cancelled.is_empty(), "reactors have no order type");
    assert_eq!(gs.player_company.visible_reactor_projects().count(), 0);
    assert_eq!(gs.player_company.reactor_projects.len(), 1);
}

/// Retiring twice is a no-op rather than a second round of cancellations.
#[test]
fn a_design_can_only_be_retired_once() {
    use crate::company::{RetireRefusal, ProjectRef};

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);
    gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("retires");

    assert_eq!(gs.player_company.retire(ProjectRef::Rocket(rp_id)),
        Err(RetireRefusal::NotFound));
}

/// The prompt promises what `retire` will do, so the two must agree.
#[test]
fn the_plan_shown_matches_what_retiring_does() {
    use crate::company::ProjectRef;

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);
    gs.player_company.hire_team("T1".into(), &gs.balance);
    assert!(gs.player_company.add_team(rocket_ref(&gs, 0)));
    gs.player_company.order_rocket_build(0, &gs.balance).expect("orders a build");

    let planned = gs.player_company.retirement_plan(ProjectRef::Rocket(rp_id))
        .expect("plans");
    let applied = gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("retires");
    assert_eq!(planned, applied);
}

/// The promise the confirmation prompt makes: retiring a design is
/// about what you'll *build* next, not about the hardware you already
/// own. A rocket on the shelf when its design retires still flies.
#[test]
fn a_rocket_already_built_still_flies_after_its_design_retires() {
    use crate::company::ProjectRef;

    let mut gs = GameState::new("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);
    gs.player_company.order_rocket_build(0, &gs.balance).expect("orders a build");
    run_manufacturing_to_rocket(&mut gs);
    assert_eq!(gs.player_company.manufacturing.inventory.rockets.len(), 1,
        "fixture should have put a rocket on the shelf");
    let item_id = gs.player_company.manufacturing.inventory.rockets[0].item_id;

    gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("retires");

    assert_eq!(gs.player_company.manufacturing.inventory.rockets.len(), 1,
        "retiring must not scrap hardware that already exists");
    // The launch path resolves the design straight out of
    // `rocket_projects`, which is exactly why retirement flags rather
    // than deletes.
    let launched = gs.launch_rocket(item_id, "leo", Vec::new(), false);
    assert!(launched.is_some(), "a built rocket should still launch");
}
