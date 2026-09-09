//! `game_state` integration-ish unit tests. `use super::*` resolves to
//! `game_state`; the shared fixtures live here and each file below
//! covers one area (17_REFACTOR.md E6).

mod clock;
mod flights;
mod geopolitics;
mod launch;
mod manufacturing;
mod projects;
mod reactors;
mod retire;
mod rockets;
mod ui_tables;

use crate::flight::Payload;
use crate::rocket::RocketDesignId;
use crate::rocket_project::RocketProject;
use crate::company::ProjectRef;
use super::*;
use crate::flaw::FlawTrigger;

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
