//! Manufacturing: cost history, engine stock, the rush lane, the queue
//! tree and nominal build times.

use super::*;

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
    let mut gs = GameState::with_money("Test".into(), 1_000_000_000.0, 42);
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
    let mut gs = GameState::with_money("Test".into(), 1_000_000_000.0, 42);
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
    let mut gs = GameState::new("Test".into(), 42);

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
    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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
    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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
    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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
    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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
    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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

    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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
    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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
    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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
    let mut gs = GameState::with_money("Test".into(), 5_000_000_000.0, 42);
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
