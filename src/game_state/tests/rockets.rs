//! Rocket designs flown end to end: flaw scoping, delta-v left in orbit,
//! and the chemical-then-ion asteroid mission.

use super::*;

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
        two_stage.vacuum_delta_v(0.0)
    };
    let total_dv = design.vacuum_delta_v(0.0);
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
        crate::rocket::RocketId(1), 0.0,
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
        location: crate::location::LocationId::of("leo"),
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
        crate::rocket::RocketId(1), 0.0,
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
        current_location: crate::location::LocationId::of("earth_surface"),
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
    let mut rocket = design.instantiate(RocketId(1), 0.0);

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
    let burn_result = rocket.burn_sequential(&design, eros_dv, "leo");
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
