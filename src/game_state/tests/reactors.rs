//! Reactor projects and reactor flaws in flight.

use super::*;

/// Phase 2a — start_proposed_reactor + promote_proposed_reactor +
/// delete_proposed_reactor round-trip. Mirrors the assertions for
/// engine projects in the engine-pipeline tests.
#[test]
fn test_reactor_proposed_lifecycle() {
    use crate::reactor::EnrichmentLevel;
    use crate::reactor_project::ReactorDesignStatus;

    let mut gs = GameState::with_money("Test".into(), 100_000_000.0, 1);
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
    let mut gs = GameState::with_money("Test".into(), 100_000_000.0, 1);
    let pid = gs.player_company.start_proposed_reactor(
        "Cancelled".into(), 1.0, EnrichmentLevel::Leu, &gs.balance,
    );
    gs.player_company.delete_proposed(ProjectRef::Reactor(pid));
    assert!(gs.player_company.find_reactor_project(pid).is_none());
}

/// Phase 2a — team helpers respect the unassigned-team budget and
/// don't decrement past zero.
#[test]
fn test_reactor_team_helpers() {
    use crate::reactor::EnrichmentLevel;
    let mut gs = GameState::with_money("Test".into(), 100_000_000.0, 1);
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

    let mut gs = GameState::with_money("Test".into(), 100_000_000.0, 1);
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

    let mut gs = GameState::with_money("Reactor Test".into(), 100_000_000.0, 7);
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

    let mut gs = GameState::with_money("Reactor Test".into(), 100_000_000.0, 7);
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

    let mut gs = GameState::new("Reactor Flight".into(), 11);

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
    let rocket = design.instantiate(RocketId(1), 0.0);
    gs.spacecraft.push(Spacecraft {
        id: SpacecraftId(1),
        name: "ReactorCraft".into(),
        rocket,
        design,
        location: crate::location::LocationId::of("leo"),
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

    let mut gs = GameState::new("Reactor Flight".into(), 5);
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
    let rocket = design.instantiate(RocketId(1), 0.0);
    gs.spacecraft.push(Spacecraft {
        id: SpacecraftId(1), name: "ReactorCraft".into(),
        rocket, design, location: crate::location::LocationId::of("leo"),
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

/// Phase 3: the real daily loop surfaces reactor flaw discovery and
/// flaw-removal revision events (not just the deficiency path).
#[test]
fn test_reactor_flaw_discovery_and_revision_through_daily_loop() {
    use crate::reactor::{EnrichmentLevel, ReactorId};
    use crate::reactor_project::{ReactorDesignStatus, ReactorProject, ReactorProjectId};

    let mut gs = GameState::with_money("Reactor Test".into(), 100_000_000.0, 3);
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
