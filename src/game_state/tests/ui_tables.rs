//! The contract rows and payload table the UI draws, checked against
//! game state. The plan (E6) would put these beside `ui/draw.rs`; they
//! stay here because they lean on the three-stage fixture.

use super::*;

/// The contracts table shows how long is left, and distinguishes a
/// deadline today from one already missed — `days_until` floors at zero,
/// so both would otherwise read "0d".
#[test]
fn contract_rows_show_days_left_and_flag_overdue() {
    use crate::ui::draw::test_support::contract_row_for_test;

    let mut c = crate::contract::test_support::solicitation_fixture();
    let today = crate::calendar::GameDate { year: 2001, month: 6, day: 1 };

    c.deadline = today.add_days(45);
    assert!(contract_row_for_test(&c, today).contains("45d"),
        "row: {:?}", contract_row_for_test(&c, today));

    c.deadline = today;
    let due_today = contract_row_for_test(&c, today);
    assert!(due_today.contains("0d"), "due today reads as zero: {due_today:?}");
    assert!(!due_today.contains("over"), "but is not overdue: {due_today:?}");

    c.deadline = crate::calendar::GameDate { year: 2001, month: 5, day: 31 };
    let missed = contract_row_for_test(&c, today);
    assert!(missed.contains("over"), "a passed deadline says so: {missed:?}");
}

/// Lifting the mass and arriving with the lights on are separate
/// failures. The payload figure alone reads as success, so the table has
/// to say when a destination is reachable but not survivable.
#[test]
fn the_payload_table_flags_destinations_it_cannot_stay_alive_to_reach() {
    let mut gs = GameState::with_money("Test".into(), 1_000_000_000.0, 42);
    let (design, engine_projects) = make_three_stage_design();
    gs.player_company.engine_projects = engine_projects;
    assert!(
        design.stage_groups.iter().flatten().all(|s| s.power_sources.is_empty()),
        "fixture premise: nothing fitted, so every stage flies the default battery",
    );

    let dests = ["leo", "gto", "meo"];
    let table = gs.payload_table(&design, "earth_surface", &dests);
    let row = |name: &str| table.iter()
        .find(|r| r.destination == name)
        .unwrap_or_else(|| panic!("{name} missing from {:?}",
            table.iter().map(|r| r.destination).collect::<Vec<_>>()));

    // Same-day deliveries: the default battery covers them.
    for same_day in ["Low Earth Orbit", "Geostationary Transfer"] {
        let r = row(same_day);
        assert!(r.max_payload_kg > 0.0);
        assert!(r.survives, "{same_day} is a same-day trip");
    }

    // MEO climbs through LEO, so the craft is still flying on day two
    // with nothing aboard generating power.
    let meo = row("Medium Earth Orbit");
    assert!(meo.max_payload_kg > 0.0, "it can lift the mass");
    assert!(!meo.survives, "but it goes dark before it arrives");
}

/// Fitting power changes the verdict without touching the payload — the
/// point of flagging it separately.
#[test]
fn power_flips_the_table_verdict_without_moving_the_payload() {
    let mut gs = GameState::with_money("Test".into(), 1_000_000_000.0, 42);
    let (bare, engine_projects) = make_three_stage_design();
    gs.player_company.engine_projects = engine_projects;

    let mut solar = bare.clone();
    {
        let upper = solar.stage_groups.last_mut().unwrap().first_mut().unwrap();
        let demand = upper.housekeeping_w();
        upper.power_sources.push(crate::power::PowerSource::new_solar_panel(demand * 4.0));
    }

    let dests = ["meo"];
    let before = gs.payload_table(&bare, "earth_surface", &dests);
    let after = gs.payload_table(&solar, "earth_surface", &dests);

    assert!(!before[0].survives, "premise: bare can't stay alive to MEO");
    assert!(after[0].survives, "a panel keeps the lights on the whole way");
}
