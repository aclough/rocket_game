//! Fixtures shared by the integration suites (17_5_G.md step 4 / G2).
//! Each suite that wants one declares `mod common;`.
#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};

use rocket_tycoon::calendar::GameDate;
use rocket_tycoon::event::GameEvent;
use rocket_tycoon::game_state::GameState;

/// A fresh game under default balance (DinoSoar enabled) at `seed`.
pub fn fresh_game(seed: u64) -> GameState {
    GameState::new("Test".into(), seed)
}

/// A unique path under the system temp dir for a save written by a
/// test; `tag` names the test so a leftover file is traceable.
pub fn temp_save_path(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join("rocket_tycoon_test");
    std::fs::create_dir_all(&dir).unwrap();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!("{tag}_{}_{n}.json", std::process::id()))
}

/// Advance `gs` day by day (up to `max_days`), collecting every event
/// fired, until `gs.date` passes `deadline`. Panics if that never
/// happens, so a bug that skips resolution fails loudly instead of
/// silently passing an empty-events test.
pub fn advance_through(gs: &mut GameState, deadline: GameDate, max_days: u32) -> Vec<GameEvent> {
    let mut all = Vec::new();
    for _ in 0..max_days {
        // A tick does its work under the date the clock reads *now* and only
        // rolls over at the end, so bids resolve on the first tick that
        // *runs* past the deadline. Test against the date the body ran
        // under, not the post-tick one — the latter is already a day ahead
        // and would return before the resolving tick ever happened.
        let ran_on = gs.date;
        all.extend(gs.advance_day());
        if ran_on > deadline {
            return all;
        }
    }
    panic!("resolution did not happen within {max_days} days of deadline {deadline}");
}
