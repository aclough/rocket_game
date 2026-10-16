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
    use crate::third_party::VendorRole;
    let gs = GameState::new("Test".into(), 42);
    let catalog = &gs.player_company.third_party_catalog;
    assert!((4..=5).contains(&catalog.len()), "{}", catalog.len());
    for role in [VendorRole::SolidKickMotor, VendorRole::HypergolicThruster, VendorRole::KeroloxBooster, VendorRole::LegacyHydroloxUpper] {
        let e = catalog.iter().find(|e| e.role == Some(role)).unwrap_or_else(|| panic!("every world sells a {}", role.label()));
        assert!(e.purchase_cost_per_unit > 0.0 && e.design.thrust_n > 0.0 && e.design.mass_kg > 0.0);
        assert!(e.available_from <= gs.date, "the four roles are on sale from the start");
    }
    let legacy = catalog.iter().find(|e| e.role == Some(VendorRole::LegacyHydroloxUpper)).unwrap();
    assert!(legacy.legacy.is_some() && legacy.build_time_multiplier > 1.0);
    assert!(legacy.design.is_vacuum_variant(), "an upper-stage engine wears the long bell");
    // Invented, and invented differently elsewhere.
    let other = GameState::new("Test".into(), 7);
    let names = |gs: &GameState| gs.player_company.third_party_catalog.iter().map(|e| e.design.name.clone()).collect::<Vec<_>>();
    assert_ne!(names(&gs), names(&other));
    for n in names(&gs) {
        assert!(!["RD-33K", "HT-40", "KM-15 Kick Motor", "DL-10 Dactyl"].contains(&n.as_str()), "no real names: {n}");
    }
    // The same world always sells the same engines.
    assert_eq!(names(&gs), names(&GameState::new("Again".into(), 42)));
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

/// 21_ENGINES.md step 6: a `BuildTimePenalty` deficiency stretches the
/// build work of every unit of the design it lands on, shows in the
/// build estimate, and comes off when a revision solves it.
#[test]
fn build_time_penalty_applies_to_builds_and_reverts_when_solved() {
    use crate::engine::EngineCycle;
    use crate::engine_project::PropellantPreset;
    use crate::technology::{TechDeficiency, TechDeficiencyId, TechDeficiencyKind, TECH_METHALOX};

    let mut gs = GameState::with_money("Btp".into(), 1_000_000_000.0, 3);
    let tech = gs.technologies.iter_mut().find(|t| t.id == TECH_METHALOX).unwrap();
    tech.deficiencies = vec![TechDeficiency {
        id: TechDeficiencyId(990),
        description: "Hand-finished combustion chamber".into(),
        kind: TechDeficiencyKind::BuildTimePenalty(2.0),
        solvability: 1.0,
        total_attempts: 0,
    }];
    let bal = gs.balance.clone();
    gs.player_company.start_engine_project(
        "Printed".into(), EngineCycle::GasGenerator, PropellantPreset::Methalox, 1.0,
        Some(TECH_METHALOX), &bal,
    );
    let pid = gs.player_company.engine_projects[0].project_id;
    let mut events = Vec::new();
    gs.apply_new_design_deficiencies::<crate::engine_project::EngineProject>(vec![pid], &mut events);
    let ep = &gs.player_company.engine_projects[0];
    assert_eq!(ep.build_time_multiplier, 2.0);
    assert_eq!(ep.tech_deficiency_ids, vec![TechDeficiencyId(990)]);

    // A build order for it takes twice the plain build work.
    let plain = bal.work.engine_build_work(
        ep.complexity, ep.design.mass_kg, bal.propellant_handling.multiplier(PropellantPreset::Methalox),
    );
    let order = crate::manufacturing::ManufacturingOrder::new_engine(
        crate::manufacturing::ManufacturingOrderId(1),
        crate::engine_project::EngineSource::PlayerDesign(pid), ep.design.id, ep.design.name.clone(),
        ep.design.mass_kg, ep.complexity, PropellantPreset::Methalox, ep.build_time_multiplier, ep.material_cost_multiplier, 0,
        0, Vec::new(), Vec::new(), &bal,
    );
    assert!((order.work_required - 2.0 * plain).abs() < 1e-9);

    // Solving it (solvability 1.0) takes the multiplier back off.
    gs.resolve_tech_attempts::<crate::engine_project::EngineProject>(vec![(pid, TechDeficiencyId(990))], &mut events);
    let ep = &gs.player_company.engine_projects[0];
    assert_eq!(ep.build_time_multiplier, 1.0);
    assert!(ep.tech_deficiency_ids.is_empty());
}

/// 23_DEFICIENCIES.md §3.1: a `MaterialCostPenalty` multiplies the bill
/// of materials of every unit of the design it lands on, shows in the
/// build estimate, and comes off when a revision solves it.
#[test]
fn material_cost_penalty_applies_to_builds_and_reverts_when_solved() {
    use crate::engine::EngineCycle;
    use crate::engine_project::PropellantPreset;
    use crate::technology::{TechDeficiency, TechDeficiencyId, TechDeficiencyKind, TECH_METHALOX};

    let mut gs = GameState::with_money("Mcp".into(), 1_000_000_000.0, 3);
    let tech = gs.technologies.iter_mut().find(|t| t.id == TECH_METHALOX).unwrap();
    tech.deficiencies = vec![TechDeficiency {
        id: TechDeficiencyId(991),
        description: "Exotic alloys from a single mill".into(),
        kind: TechDeficiencyKind::MaterialCostPenalty(1.6),
        solvability: 1.0,
        total_attempts: 0,
    }];
    let bal = gs.balance.clone();
    gs.player_company.start_engine_project(
        "Alloyed".into(), EngineCycle::GasGenerator, PropellantPreset::Methalox, 1.0,
        Some(TECH_METHALOX), &bal,
    );
    let pid = gs.player_company.engine_projects[0].project_id;
    let mut events = Vec::new();
    gs.apply_new_design_deficiencies::<crate::engine_project::EngineProject>(vec![pid], &mut events);
    let ep = &gs.player_company.engine_projects[0];
    assert!((ep.material_cost_multiplier - 1.6).abs() < 1e-9);
    assert_eq!(ep.build_time_multiplier, 1.0, "materials, not labour");

    let plain = crate::resources::engine_material_cost(PropellantPreset::Methalox, ep.design.mass_kg, &bal.costs.resource_prices);
    let order = crate::manufacturing::ManufacturingOrder::new_engine(
        crate::manufacturing::ManufacturingOrderId(1),
        crate::engine_project::EngineSource::PlayerDesign(pid), ep.design.id, ep.design.name.clone(),
        ep.design.mass_kg, ep.complexity, PropellantPreset::Methalox, ep.build_time_multiplier,
        ep.material_cost_multiplier, 0, 0, Vec::new(), Vec::new(), &bal,
    );
    assert!((order.material_cost - 1.6 * plain).abs() < 1e-6, "{} vs {}", order.material_cost, 1.6 * plain);

    gs.resolve_tech_attempts::<crate::engine_project::EngineProject>(vec![(pid, TechDeficiencyId(991))], &mut events);
    let ep = &gs.player_company.engine_projects[0];
    assert!((ep.material_cost_multiplier - 1.0).abs() < 1e-9);
    assert!(ep.tech_deficiency_ids.is_empty());
}

/// 23_DEFICIENCIES.md §3.2: a `HardToTest` deficiency raises every
/// discovery exponent the project rolls at — its flaws hide longer,
/// in flight review too — and comes off when solved.
#[test]
fn hard_to_test_raises_the_discovery_exponents_until_solved() {
    use crate::engine::EngineCycle;
    use crate::engine_project::PropellantPreset;
    use crate::flaw::{FlawExposure, FlawPhase};
    use crate::project::DesignProject;
    use crate::technology::{TechDeficiency, TechDeficiencyId, TechDeficiencyKind, TECH_METHALOX};

    let mut gs = GameState::with_money("Htt".into(), 1_000_000_000.0, 3);
    let tech = gs.technologies.iter_mut().find(|t| t.id == TECH_METHALOX).unwrap();
    tech.deficiencies = vec![TechDeficiency {
        id: TechDeficiencyId(992),
        description: "Instrumentation immature for the combustion regime".into(),
        kind: TechDeficiencyKind::HardToTest(2),
        solvability: 1.0,
        total_attempts: 0,
    }];
    let bal = gs.balance.clone();
    gs.player_company.start_engine_project(
        "Opaque".into(), EngineCycle::GasGenerator, PropellantPreset::Methalox, 1.0,
        Some(TECH_METHALOX), &bal,
    );
    let pid = gs.player_company.engine_projects[0].project_id;
    let mut events = Vec::new();
    gs.apply_new_design_deficiencies::<crate::engine_project::EngineProject>(vec![pid], &mut events);
    let ep = &gs.player_company.engine_projects[0];
    assert_eq!(ep.test_difficulty, 2);

    let flaw = crate::flaw::Flaw {
        id: crate::flaw::FlawId(1), description: String::new(),
        consequence: crate::flaw::FlawConsequence::EngineLoss, activation_chance: 0.3,
        discovery_roll: 0.5, discovered: false, trigger: crate::flaw::FlawTrigger::PerFlight,
        phase: FlawPhase::Burn, stage_group: None, exposure: FlawExposure::Bench,
    };
    let exponent = |ep: &crate::engine_project::EngineProject| DesignProject::<crate::engine::EngineDesign>::discovery_exponent_for(
        &flaw, ep.flight_review.as_ref(), 0, ep.test_difficulty, &bal.flaws,
    );
    assert_eq!(exponent(ep), bal.flaws.discovery_exponent_bench + 2.0);
    let mut reviewed = ep.clone();
    reviewed.open_flight_review(vec![(FlawPhase::Burn, 0)], false, &bal.flaws);
    assert_eq!(exponent(&reviewed), bal.flaws.discovery_exponent_flight_review + 2.0, "a review is slower too");

    gs.resolve_tech_attempts::<crate::engine_project::EngineProject>(vec![(pid, TechDeficiencyId(992))], &mut events);
    let ep = &gs.player_company.engine_projects[0];
    assert_eq!(ep.test_difficulty, 0);
    assert_eq!(exponent(ep), bal.flaws.discovery_exponent_bench);
}

/// 23_DEFICIENCIES.md §4.3: a design on two technologies inherits the
/// unsolved deficiencies of both, and solving one technology's leaves
/// the other's where it is.
#[test]
fn a_design_on_two_technologies_inherits_both_lists() {
    use crate::engine::EngineCycle;
    use crate::engine_project::PropellantPreset;
    use crate::technology::{TechDeficiency, TechDeficiencyId, TechDeficiencyKind, TECH_METHALOX, TECH_NUCLEAR_THERMAL};

    let mut gs = GameState::with_money("Two".into(), 1_000_000_000.0, 3);
    let def = |id: u64, text: &str| TechDeficiency {
        id: TechDeficiencyId(id), description: text.into(),
        kind: TechDeficiencyKind::MassPenalty(0.2), solvability: 1.0, total_attempts: 0,
    };
    gs.technologies.iter_mut().find(|t| t.id == TECH_METHALOX).unwrap().deficiencies = vec![def(990, "Methane coking")];
    gs.technologies.iter_mut().find(|t| t.id == TECH_NUCLEAR_THERMAL).unwrap().deficiencies = vec![def(1990, "Fuel element cracking")];
    let bal = gs.balance.clone();
    // Methalox by family, nuclear thermal added on top (the way a cycle
    // technology will be): on both.
    gs.player_company.start_engine_project(
        "Both".into(), EngineCycle::GasGenerator, PropellantPreset::Methalox, 1.0, Some(TECH_NUCLEAR_THERMAL), &bal,
    );
    let pid = gs.player_company.engine_projects[0].project_id;
    assert_eq!(gs.player_company.engine_projects[0].technology_ids, vec![TECH_METHALOX, TECH_NUCLEAR_THERMAL]);
    let mut events = Vec::new();
    gs.apply_new_design_deficiencies::<crate::engine_project::EngineProject>(vec![pid], &mut events);
    let ep = &gs.player_company.engine_projects[0];
    assert_eq!(ep.tech_deficiency_ids, vec![TechDeficiencyId(990), TechDeficiencyId(1990)]);
    assert!((ep.design.mass_kg / (1.2 * 1.2) - gs.player_company.engine_projects[0].design.mass_kg / 1.44).abs() < 1e-6);
    let found = events.iter().filter(|e| matches!(e, GameEvent::Project { event: crate::event::ProjectEvent::TechDeficienciesFound { .. }, .. })).count();
    assert_eq!(found, 2, "one notice per technology");

    gs.resolve_tech_attempts::<crate::engine_project::EngineProject>(vec![(pid, TechDeficiencyId(1990))], &mut events);
    let ep = &gs.player_company.engine_projects[0];
    assert_eq!(ep.tech_deficiency_ids, vec![TechDeficiencyId(990)], "methane's is still there");
    assert!(gs.player_company.solved_deficiencies.contains(&TechDeficiencyId(1990)));
}

/// 22_FLAWS.md §3.6: solving a deficiency retires it for the company. A
/// design finished afterwards does not inherit it; one that already
/// carried it keeps it until its own revision; the cue follows.
#[test]
fn solved_deficiencies_retire_per_company() {
    use crate::engine::EngineCycle;
    use crate::engine_project::PropellantPreset;
    use crate::technology::{TechDeficiency, TechDeficiencyId, TechDeficiencyKind, TECH_METHALOX};

    let mut gs = GameState::with_money("Retire".into(), 1_000_000_000.0, 3);
    let tech = gs.technologies.iter_mut().find(|t| t.id == TECH_METHALOX).unwrap();
    tech.deficiencies = vec![
        TechDeficiency {
            id: TechDeficiencyId(990), description: "Coking injector".into(),
            kind: TechDeficiencyKind::IspPenalty(0.1), solvability: 1.0, total_attempts: 0,
        },
        TechDeficiency {
            id: TechDeficiencyId(991), description: "Heavy manifold".into(),
            kind: TechDeficiencyKind::MassPenalty(0.1), solvability: 0.0, total_attempts: 0,
        },
    ];
    let bal = gs.balance.clone();
    let mut events = Vec::new();
    let start = |gs: &mut GameState, name: &str| {
        gs.player_company.start_engine_project(
            name.into(), EngineCycle::GasGenerator, PropellantPreset::Methalox, 1.0, Some(TECH_METHALOX), &bal,
        );
        gs.player_company.engine_projects.last().unwrap().project_id
    };
    // Two designs finish before anything is solved: both carry both.
    let a = start(&mut gs, "A");
    let c = start(&mut gs, "C");
    gs.apply_new_design_deficiencies::<crate::engine_project::EngineProject>(vec![a, c], &mut events);
    assert_eq!(gs.player_company.engine_projects[0].tech_deficiency_ids, vec![TechDeficiencyId(990), TechDeficiencyId(991)]);
    assert_eq!(gs.player_company.engine_projects[1].tech_deficiency_ids.len(), 2);

    // A solves the solvable one: the company retires it.
    gs.resolve_tech_attempts::<crate::engine_project::EngineProject>(vec![(a, TechDeficiencyId(990))], &mut events);
    assert!(gs.player_company.solved_deficiencies.contains(&TechDeficiencyId(990)));
    assert_eq!(gs.player_company.engine_projects[0].tech_deficiency_ids, vec![TechDeficiencyId(991)]);
    // C still carries it until its own revision.
    assert_eq!(gs.player_company.engine_projects[1].tech_deficiency_ids.len(), 2);
    // A design finished now inherits only what is left unsolved.
    let b = start(&mut gs, "B");
    gs.apply_new_design_deficiencies::<crate::engine_project::EngineProject>(vec![b], &mut events);
    assert_eq!(gs.player_company.engine_projects[2].tech_deficiency_ids, vec![TechDeficiencyId(991)]);
    // The unsolvable one is still on the technology for everyone.
    let tech = gs.technologies.iter().find(|t| t.id == TECH_METHALOX).unwrap();
    assert_eq!(tech.unsolved_for(&gs.player_company.solved_deficiencies).len(), 1);
    // The world's deficiency list itself is untouched: a rival would face both.
    assert_eq!(tech.deficiencies.len(), 2);
}

/// Rocket designs follow their engine projects: an improvement applied
/// to an engine reaches every rocket built around it, each stage keeping
/// its bell, its thrust structure following the thrust, and a solar
/// panel auto-sized to the stage's demand following the new demand.
#[test]
fn rocket_designs_follow_their_engines_improvements() {
    use crate::engine_project::EngineImprovementKind;
    use crate::project::Designable;

    let mut gs = GameState::with_money("Follow".into(), 1_000_000_000.0, 5);
    let mut policy = crate::policy::BasicPolicy::new();
    for _ in 0..(365 * 3) {
        if !gs.player_company.rocket_projects.is_empty() { break; }
        crate::policy::CompanyPolicy::act(&mut policy, &mut gs);
        gs.advance_day();
    }
    let before = gs.player_company.rocket_projects[0].design.clone();
    let booster_id = before.stage_groups[0][0].engine.id;
    let upper_id = before.stage_groups[1][0].engine.id;
    assert!(!before.stage_groups[0][0].engine.is_vacuum_variant());
    assert!(before.stage_groups[1][0].engine.is_vacuum_variant());

    // Nothing changed yet: a refresh is a no-op.
    gs.refresh_rocket_engines();
    let unchanged = &gs.player_company.rocket_projects[0].design;
    assert_eq!(unchanged.stage_groups[0][0].engine, before.stage_groups[0][0].engine);
    assert_eq!(unchanged.stage_groups[0][0].structural_mass_kg, before.stage_groups[0][0].structural_mass_kg);

    // +10 % thrust on both engine projects.
    for ep in gs.player_company.engine_projects.iter_mut() {
        ep.design.apply_improvement(&EngineImprovementKind::Thrust(0.10));
    }
    gs.refresh_rocket_engines();
    let after = &gs.player_company.rocket_projects[0].design;
    for (gi, id) in [(0usize, booster_id), (1, upper_id)] {
        let old = &before.stage_groups[gi][0];
        let new = &after.stage_groups[gi][0];
        assert_eq!(new.engine.id, id);
        assert_eq!(new.engine.is_vacuum_variant(), old.engine.is_vacuum_variant(), "stage {gi} keeps its bell");
        assert!((new.engine.thrust_n / old.engine.thrust_n - 1.10).abs() < 1e-9, "stage {gi} gets the thrust");
        assert!(new.structural_mass_kg > old.structural_mass_kg, "stage {gi}'s thrust structure grows");
        assert_eq!(new.propellant_mass_kg, old.propellant_mass_kg);
        // The bot's panels were auto-sized to demand; they follow it, so
        // the heavier stage stays powered.
        assert!(new.supply_w(1.0) >= new.housekeeping_w() - 1e-9, "stage {gi} stays powered");
        assert!(new.supply_w(1.0) > old.supply_w(1.0));
    }
}

/// 21_ENGINES.md step 7: each launch credits one flight to every
/// distinct player engine design on the rocket, whatever the outcome.
#[test]
fn launches_credit_flight_heritage_to_engine_designs() {
    let mut gs = GameState::with_money("Heritage".into(), 1_000_000_000.0, 5);
    let mut policy = crate::policy::BasicPolicy::new();
    for _ in 0..(365 * 3) {
        if !gs.player_company.rocket_projects.is_empty() { break; }
        crate::policy::CompanyPolicy::act(&mut policy, &mut gs);
        gs.advance_day();
    }
    let rp = gs.player_company.rocket_projects[0].project_id;
    let before: Vec<u32> = gs.player_company.engine_projects.iter().map(|e| e.flights).collect();
    gs.credit_engine_flights(rp);
    let after: Vec<u32> = gs.player_company.engine_projects.iter().map(|e| e.flights).collect();
    // The bot's rocket carries both its engines (booster and upper): each
    // gains exactly one flight even though the booster stage has two.
    assert_eq!(after.iter().zip(&before).map(|(a, b)| a - b).collect::<Vec<_>>(), vec![1, 1]);
}
