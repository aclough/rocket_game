//! Flights and spacecraft: arrival, payload deployment, docking, and
//! what a mid-flight stage loss does.

use super::*;

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
    let capacity = sc.rocket.total_battery_kwd(&sc.design);
    let drained = capacity - sc.rocket.total_battery_charge_kwd();
    assert!(
        (drained - one_day_kwd).abs() < 1e-9,
        "arrival day should cost exactly one housekeeping day ({one_day_kwd} kWd), drained {drained}",
    );
}
