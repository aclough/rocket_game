//! Building a launch's payload list from the picked contracts (the
//! action methods lifted from the UI in M1 Task 3a).

use super::*;

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
    let mut gs = GameState::new("Test".into(), 1);
    let (dest, payloads) = gs.build_launch_payloads(&[], &[]).unwrap();
    assert_eq!(dest, "leo");
    assert_eq!(payloads.len(), 1);
    assert!(matches!(payloads[0], Payload::TestMass { .. }));
}

#[test]
fn test_build_launch_payloads_shared_destination() {
    let mut gs = GameState::new("Test".into(), 1);
    let a = push_contract(&mut gs, 1, "gto");
    let b = push_contract(&mut gs, 2, "gto");
    let (dest, payloads) = gs.build_launch_payloads(&[a, b], &[]).unwrap();
    assert_eq!(dest, "gto");
    assert_eq!(payloads.len(), 2);
    assert!(payloads.iter().all(|p| matches!(p, Payload::ContractDelivery { .. })));
}

#[test]
fn test_build_launch_payloads_conflicting_destinations() {
    let mut gs = GameState::new("Test".into(), 1);
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
    let mut gs = GameState::new("Test".into(), 1);
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
            grounded_until: None,
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

/// A rocket flaw with a given consequence, certain to fire at its event.
fn certain_flaw(id: u64, phase: crate::flaw::FlawPhase, group: usize, consequence: crate::flaw::FlawConsequence) -> crate::flaw::Flaw {
    crate::flaw::Flaw {
        id: crate::flaw::FlawId(id), description: format!("certain flaw {id}"), consequence,
        activation_chance: 1.0, discovery_roll: 0.0, discovered: false,
        trigger: crate::flaw::FlawTrigger::PerFlight, phase, stage_group: Some(group),
        exposure: crate::flaw::FlawExposure::Analysis,
    }
}

/// The three-stage fixture on the shelf, carrying `flaws` as its snapshot
/// and on its project.
fn shelve_rocket(gs: &mut GameState, flaws: Vec<crate::flaw::Flaw>) -> crate::manufacturing::InventoryItemId {
    let (design, engine_projects) = make_three_stage_design();
    gs.player_company.engine_projects = engine_projects;
    // The fixture's engine flaws are certain 1% degradations; keep the
    // vehicle nominal so the test reads the rocket flaw alone.
    for ep in &mut gs.player_company.engine_projects {
        ep.flaws.clear();
    }
    let mut rp = RocketProject::new(RocketProjectId(1), design, &gs.balance);
    rp.flaws = flaws.clone();
    let design_id = rp.design.id;
    gs.player_company.rocket_projects.push(rp);
    let item_id = crate::manufacturing::InventoryItemId(10);
    gs.player_company.manufacturing.inventory.rockets.push(crate::manufacturing::InventoryRocket {
        item_id, rocket_project_id: RocketProjectId(1), design_id, rocket_name: "Shelf".into(),
        build_cost: 0.0, revision: 0, rocket_flaws: flaws, grounded_until: None,
    });
    item_id
}

/// A pad abort scrubs the launch: the vehicle goes back on the shelf
/// grounded for the stand-down, the flaw is found, nothing flew, and the
/// launch waits out the stand-down.
#[test]
fn a_pad_abort_scrubs_and_grounds_the_vehicle() {
    use crate::flaw::{FlawConsequence, FlawPhase};
    let mut gs = GameState::new("Test".into(), 1);
    let item = shelve_rocket(&mut gs, vec![certain_flaw(1, FlawPhase::Ignition, 0, FlawConsequence::PadAbort { delay_days: 4 })]);
    let start = gs.date;

    let (events, record) = gs.launch_rocket(item, "leo", vec![Payload::TestMass { mass_kg: 10.0 }], false)
        .expect("the launch is attempted");
    let record = record.expect("a scrub makes a record");
    assert!(matches!(record.outcome, crate::launch::LaunchOutcome::Scrubbed { delay_days: 4, .. }), "{:?}", record.outcome);
    assert!(events.iter().any(|e| matches!(e, GameEvent::LaunchScrubbed { delay_days: 4, .. })));
    assert!(gs.active_flights.is_empty(), "nothing flew");
    assert_eq!(gs.player_company.manufacturing.inventory.rockets.len(), 1, "back on the shelf");
    assert_eq!(gs.rocket_grounded_until(item), Some(start.add_days(4)));
    assert!(gs.player_company.rocket_projects[0].flaws[0].discovered, "the pad found it");
    assert_eq!(gs.player_company.engine_projects[0].flights, 0, "a scrub is no flight heritage");

    assert!(gs.launch_rocket(item, "leo", vec![Payload::TestMass { mass_kg: 10.0 }], false).is_none(),
        "grounded vehicles do not launch");
    for _ in 0..4 {
        gs.advance_day();
    }
    assert_eq!(gs.rocket_grounded_until(item), None);
    // The flaw is known now but not revised; the snapshot still carries
    // it, so it scrubs again — which is the point of revising first.
    let (_, record) = gs.launch_rocket(item, "leo", vec![Payload::TestMass { mass_kg: 10.0 }], false).unwrap();
    assert!(matches!(record.unwrap().outcome, crate::launch::LaunchOutcome::Scrubbed { .. }));
}

/// A payload-loss flaw fires at deploy: the vehicle arrives, the contract
/// is forfeit and unpaid, and the record reads as a partial failure.
#[test]
fn a_payload_loss_at_deploy_forfeits_the_contract() {
    use crate::flaw::{FlawConsequence, FlawPhase};
    let mut gs = GameState::new("Test".into(), 1);
    let item = shelve_rocket(&mut gs, vec![certain_flaw(1, FlawPhase::PayloadDeploy, 2, FlawConsequence::PayloadLoss)]);
    gs.player_company.active_contracts.push(Contract {
        id: crate::contract::ContractId(7), name: "Smallsat".into(), destination: "leo".into(),
        payload_kg: 10.0, payment: 10_000_000.0, deadline: GameDate::new(2005, 1, 1),
        status: crate::contract::ContractStatus::Accepted, market_id: crate::contract::MarketId::default(),
        campaign_id: None, bid_deadline: None, budget_ceiling: 0.0, player_bid: None,
    });
    let money = gs.player_company.money;
    let (dest, payloads) = gs.build_launch_payloads(&[0], &[]).unwrap();
    let (_, record) = gs.launch_rocket(item, &dest, payloads, false).expect("launched");
    assert!(record.is_none(), "the ascent itself is clean: {record:?}");

    let mut all_events = Vec::new();
    for _ in 0..60 {
        all_events.extend(gs.advance_day());
        if gs.active_flights.is_empty() {
            break;
        }
    }
    assert!(gs.active_flights.is_empty(), "the flight should have arrived");
    assert!(all_events.iter().any(|e| matches!(e, GameEvent::PayloadLost { .. })), "{all_events:?}");
    assert!(!all_events.iter().any(|e| matches!(e, GameEvent::PaymentReceived { .. })), "nothing is paid");
    assert!(gs.player_company.active_contracts.is_empty(), "the contract is forfeit");
    assert!(gs.player_company.money <= money, "no payment arrived");
    let last = gs.player_company.launch_history.last().expect("a record");
    assert!(matches!(&last.outcome, crate::launch::LaunchOutcome::PartialFailure { reason } if reason.contains("Payload lost")), "{:?}", last.outcome);
    assert!(gs.player_company.rocket_projects[0].flaws[0].discovered);
}
