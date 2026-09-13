//! Teams and R&D projects driven through the game state.

use super::*;

#[test]
fn test_hire_team() {
    let mut gs = GameState::with_money("Test".into(), 1_000_000.0, 1);
    // Starts with 1 team (from Company::new)
    assert_eq!(gs.player_company.team_count(), 1);
    gs.player_company.hire_team("Alpha".into(), &gs.balance);
    assert_eq!(gs.player_company.team_count(), 2);
    // Only Alpha was billed; the founding team came with the company
    assert_eq!(gs.player_company.money, 1_000_000.0 - gs.balance.costs.engineering_hiring_cost);
}

#[test]
fn test_start_engine_project() {
    let mut gs = GameState::new("Test".into(), 1);
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
    let mut gs = GameState::new("Test".into(), 1);
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
    let gs = GameState::new("Test".into(), 42);
    assert_eq!(gs.player_company.third_party_catalog.len(), 3);
}

#[test]
fn test_contract_third_party() {
    let mut gs = GameState::new("Test".into(), 42);
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
    let mut gs = GameState::new("Test".into(), 1);
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

/// Cross-pool team stealing. `+` on the Reactors pane should be
/// able to pull a team from a busy engine project; symmetric for
/// engines/rockets pulling from reactors.
#[test]
fn test_cross_pool_engineering_team_steal() {
    use crate::reactor::EnrichmentLevel;
    let mut gs = GameState::with_money("Test".into(), 100_000_000.0, 1);
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

#[test]
fn test_cycle_auto_build_target_requires_testing_and_wraps() {
    let mut gs = GameState::new("Test".into(), 1);
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
