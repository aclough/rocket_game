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
