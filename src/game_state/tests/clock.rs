//! The clock, speed controls and the monthly money tick.

use super::*;

#[test]
fn test_new_game_state() {
    let gs = GameState::new("SpaceCorp".into(), 42);
    assert_eq!(gs.date, GameDate::default_start());
    assert_eq!(gs.player_company.name, "SpaceCorp");
    // The founding team is free, so the player starts with the full
    // amount the welcome screen quotes them.
    assert_eq!(gs.player_company.money, gs.balance.costs.starting_money);
    assert_eq!(gs.speed, GameSpeed::Paused);
    assert_eq!(gs.elapsed_days(), 0);
    // Should have GameStarted event
    assert_eq!(gs.event_log.len(), 1);
    // Should start with 1 engineering team
    assert_eq!(gs.player_company.team_count(), 1);
}

#[test]
fn test_advance_day() {
    let mut gs = GameState::with_money("Test".into(), 100.0, 1);
    let events = gs.advance_day();
    assert_eq!(gs.date, GameDate::new(2001, 1, 2));
    assert_eq!(gs.elapsed_days(), 1);
    // A tick does *today's* work and rolls the date only at the end, so the
    // very first tick runs on the start date — 2001-01-01, a month start.
    // A new game therefore opens with January's contracts instead of sitting
    // out its first month.
    assert!(events.iter().any(|e| matches!(e, GameEvent::MonthStart)));

    // An ordinary mid-month day still produces nothing (there is no
    // day-advanced event).
    let quiet = gs.advance_day();
    assert_eq!(gs.date, GameDate::new(2001, 1, 3));
    assert!(quiet.is_empty());
}

#[test]
fn test_advance_to_new_month() {
    let mut gs = GameState::with_money("Test".into(), 100.0, 1);
    // 31 ticks cover Jan 1 through Jan 31, leaving the clock reading Feb 1.
    for _ in 0..31 {
        gs.advance_day();
    }
    assert_eq!(gs.date, GameDate::new(2001, 2, 1));
    // February's MonthStart belongs to the tick that *runs* on Feb 1, which
    // is the next one — not the one that merely rolled the date onto it.
    let events = gs.advance_day();
    assert!(events.iter().any(|e| matches!(e, GameEvent::MonthStart)));
    assert_eq!(gs.date, GameDate::new(2001, 2, 2));
}

#[test]
fn test_toggle_pause() {
    let mut gs = GameState::with_money("Test".into(), 100.0, 1);
    assert_eq!(gs.speed, GameSpeed::Paused);

    gs.toggle_pause();
    assert_eq!(gs.speed, GameSpeed::Normal);

    gs.toggle_pause();
    assert_eq!(gs.speed, GameSpeed::Paused);

    // Should remember Normal
    gs.toggle_pause();
    assert_eq!(gs.speed, GameSpeed::Normal);
}

#[test]
fn test_toggle_pause_remembers_speed() {
    let mut gs = GameState::with_money("Test".into(), 100.0, 1);
    gs.set_speed(GameSpeed::VeryFast);
    assert_eq!(gs.speed, GameSpeed::VeryFast);

    gs.toggle_pause();
    assert_eq!(gs.speed, GameSpeed::Paused);

    // Should restore VeryFast, not Normal
    gs.toggle_pause();
    assert_eq!(gs.speed, GameSpeed::VeryFast);
}

#[test]
fn test_set_speed() {
    let mut gs = GameState::with_money("Test".into(), 100.0, 1);
    gs.set_speed(GameSpeed::Fast);
    assert_eq!(gs.speed, GameSpeed::Fast);
    gs.set_speed(GameSpeed::VeryFast);
    assert_eq!(gs.speed, GameSpeed::VeryFast);
}

#[test]
fn test_speed_tick_ms() {
    assert!(GameSpeed::Normal.tick_ms() > GameSpeed::Fast.tick_ms());
    assert!(GameSpeed::Fast.tick_ms() > GameSpeed::VeryFast.tick_ms());
}

#[test]
fn test_elapsed_days_after_year() {
    let mut gs = GameState::with_money("Test".into(), 100.0, 1);
    for _ in 0..365 {
        gs.advance_day();
    }
    assert_eq!(gs.elapsed_days(), 365);
    assert_eq!(gs.date, GameDate::new(2002, 1, 1));
}

#[test]
fn test_salary_deduction() {
    let mut gs = GameState::with_money("Test".into(), 1_000_000.0, 1);
    gs.player_company.hire_team("Alpha".into(), &gs.balance);
    // Now has 2 teams (1 free founding team + Alpha, who was billed)

    // Advance to Feb 1 (31 days)
    for _ in 0..31 {
        gs.advance_day();
    }
    // Should have paid Alpha's hiring cost + 2 team salaries for 1 month
    let expected = 1_000_000.0 - gs.balance.costs.engineering_hiring_cost - 2.0 * gs.balance.costs.engineering_monthly_salary;
    assert!((gs.player_company.money - expected).abs() < 0.01);
}

#[test]
fn test_negative_money_allowed() {
    let mut gs = GameState::with_money("Test".into(), 100_000.0, 1);
    // Starts with 1 free founding team, money = 100K
    assert_eq!(gs.player_company.money, 100_000.0);
    gs.player_company.hire_team("Alpha".into(), &gs.balance); // -150K
    assert!(gs.player_company.money < 0.0);
    // Should still work, just go negative
    for _ in 0..31 {
        gs.advance_day();
    }
    // Should have deducted 2 salaries on top
    assert!(gs.player_company.money < -300_000.0);
}
