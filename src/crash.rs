//! Crash handling: give the terminal back, keep the game, say where
//! it went.
//!
//! A TUI panic is uniquely hostile. The process dies inside the
//! alternate screen with raw mode on, so the panic message scrolls past
//! somewhere the player can't see and the shell is left without echo or
//! a working Ctrl-C — the usual reaction is to close the window, losing
//! both the session and any chance of a bug report.
//!
//! The flow here:
//! 1. A panic hook captures the message and backtrace instead of
//!    printing them into the alternate screen, where they'd be lost.
//! 2. `App::run` catches the unwind, restores the terminal *first*, and
//!    only then reports.
//! 3. The rescued `GameState` is written to a crash save, kept out of
//!    the autosave rotation so a later autosave can't clobber the one
//!    file that reproduces the bug.
//!
//! The save is the bug report; the text is what gets pasted into chat.
//! M5 Task 5 extends the text with a full state summary — this module
//! owns the plumbing, not the contents.

use std::sync::Mutex;

/// Panic message + backtrace, stashed by the hook for the catch site.
static LAST_PANIC: Mutex<Option<String>> = Mutex::new(None);

/// Install the panic hook. Call once, before entering the alternate
/// screen.
///
/// The hook deliberately prints nothing: stderr at that moment goes
/// into the alternate screen and vanishes when we leave it.
pub fn install_hook() {
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let text = format!("{info}\n\nBacktrace:\n{backtrace}");
        if let Ok(mut slot) = LAST_PANIC.lock() {
            *slot = Some(text);
        }
    }));
}

/// Take the captured panic text, if a panic happened.
pub fn take_panic_text() -> Option<String> {
    LAST_PANIC.lock().ok().and_then(|mut slot| slot.take())
}

/// Build the crash report body. `panic_text` is what the hook caught.
pub fn report(
    game: &crate::game_state::GameState,
    panic_text: &str,
    save_path: Option<&std::path::Path>,
) -> String {
    let c = &game.player_company;
    let mut out = String::new();

    out.push_str("Rocket Tycoon crash report\n");
    out.push_str("==========================\n\n");
    out.push_str(&format!("version:   {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("platform:  {} {}\n", std::env::consts::OS, std::env::consts::ARCH));
    out.push_str(&format!("company:   {}\n", c.name));
    out.push_str(&format!("seed:      {}\n", game.seed.seed()));
    out.push_str(&format!("date:      {} (day {})\n", game.date, game.elapsed_days()));
    out.push_str(&format!("money:     {:.0}\n", c.money));
    out.push_str(&format!(
        "projects:  {} engine, {} rocket, {} reactor\n",
        c.engine_projects.len(), c.rocket_projects.len(), c.reactor_projects.len(),
    ));
    out.push_str(&format!(
        "teams:     {} engineering, {} manufacturing\n",
        c.teams.len(), c.manufacturing_teams.len(),
    ));
    out.push_str(&format!("launches:  {}\n", c.launch_history.len()));
    out.push_str(&format!("flights:   {} in transit\n", game.active_flights.len()));
    match save_path {
        Some(p) => out.push_str(&format!(
            "\nA save of this game was written to:\n  {}\nPlease attach it — \
             it is what makes the bug reproducible.\n",
            p.display(),
        )),
        None => out.push_str("\nThe game state could NOT be saved.\n"),
    }

    out.push_str("\nPanic\n-----\n");
    out.push_str(panic_text);
    out.push('\n');

    out.push_str("\nRecent events\n-------------\n");
    for (date, event) in game.event_log.recent(60).iter().rev() {
        out.push_str(&format!("{date}: {event}\n"));
    }

    out
}

/// Where crash reports go.
pub fn report_path(company_name: &str) -> std::path::PathBuf {
    let safe: String = company_name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    crate::save::save_dir()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("reports")
        .join(format!("{safe}-crash.txt"))
}

/// Rescue what we can after a caught panic: write the save, write the
/// report, and return the human-facing message to print once the
/// terminal is usable again.
///
/// Best-effort by construction — a crash handler that panics is worse
/// than none, so every step degrades to a note in the message.
pub fn handle_panic(game: &crate::game_state::GameState, panic_text: &str) -> String {
    let save_path = crate::save::emergency_save_path(&game.player_company.name);
    let saved = crate::save::save_game(game, &save_path).is_ok();

    let body = report(game, panic_text, saved.then_some(save_path.as_path()));

    let report_path = report_path(&game.player_company.name);
    let report_written = std::fs::create_dir_all(
        report_path.parent().unwrap_or(std::path::Path::new(".")),
    )
    .and_then(|_| std::fs::write(&report_path, &body))
    .is_ok();

    let mut msg = String::from(
        "\nRocket Tycoon hit a bug and had to stop. Sorry.\n\n",
    );
    if saved {
        msg.push_str(&format!("  Your game was saved to: {}\n", save_path.display()));
    } else {
        msg.push_str("  The game state could not be saved.\n");
    }
    if report_written {
        msg.push_str(&format!("  A crash report was written to: {}\n", report_path.display()));
        msg.push_str("\nPlease send both files along with what you were doing.\n");
    } else {
        msg.push_str("\nThe crash report could not be written; the details follow.\n\n");
        msg.push_str(&body);
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;

    fn game() -> GameState {
        let mut g = GameState::new("Crash Co".into(), 200_000_000.0, 3);
        for _ in 0..40 {
            g.advance_day();
        }
        g
    }

    /// The report must contain what's needed to reproduce: seed, date,
    /// company, and the panic itself.
    #[test]
    fn report_carries_what_a_repro_needs() {
        let g = game();
        let text = report(&g, "panicked at 'boom', src/lib.rs:1", None);

        assert!(text.contains("Crash Co"));
        assert!(text.contains("seed:      3"), "the seed reproduces the world");
        assert!(text.contains("boom"), "the panic message must survive");
        assert!(text.contains("Recent events"), "the log gives context");
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
    }

    /// A crash handler that crashes is worse than none. Run the whole
    /// path against a real mid-game state.
    #[test]
    fn handling_a_panic_does_not_panic() {
        let g = game();
        let msg = handle_panic(&g, "panicked at 'boom'");
        assert!(msg.contains("had to stop"));
        // Either it saved or it said it couldn't — never silence.
        assert!(msg.contains("saved to") || msg.contains("could not be saved"));
    }

    /// The hook stashes rather than prints, and take() clears it.
    #[test]
    fn hook_captures_and_clears() {
        *LAST_PANIC.lock().unwrap() = Some("stashed".into());
        assert_eq!(take_panic_text().as_deref(), Some("stashed"));
        assert!(take_panic_text().is_none(), "taking should clear the slot");
    }
}
