//! Retiring designs.
//!
//! Retiring hides a design and stops further work going into it without
//! deleting anything, because manufacturing orders, inventory, spacecraft
//! and other designs all hold project ids. These guard the two halves of
//! that: what disappears, and what must keep resolving.

use super::*;

/// The load-bearing property. If a retired project were removed from the
/// vec instead of flagged, every id below would dangle.
#[test]
fn retiring_hides_a_rocket_but_keeps_its_id_resolvable() {
    use crate::company::ProjectRef;

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
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

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
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

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
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

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
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

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
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

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
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

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
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

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
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

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
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

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
    let rp_id = setup_buildable_rocket(&mut gs);
    gs.player_company.retire(ProjectRef::Rocket(rp_id)).expect("retires");

    assert_eq!(gs.player_company.retire(ProjectRef::Rocket(rp_id)),
        Err(RetireRefusal::NotFound));
}

/// The prompt promises what `retire` will do, so the two must agree.
#[test]
fn the_plan_shown_matches_what_retiring_does() {
    use crate::company::ProjectRef;

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
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

    let mut gs = GameState::with_money("Test".into(), 500_000_000.0, 1);
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
