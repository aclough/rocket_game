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

/// Build the report body.
///
/// `panic_text` is `Some` only when this came from the crash path; on
/// demand (`F12`) the report is the same document without that section,
/// so a player and a crash produce comparable output.
pub fn report(
    game: &crate::game_state::GameState,
    panic_text: Option<&str>,
    save_path: Option<&std::path::Path>,
) -> String {
    let c = &game.player_company;
    let mut out = String::new();

    let kind = if panic_text.is_some() { "crash" } else { "session" };
    out.push_str(&format!("Rocket Tycoon {kind} report\n"));
    out.push_str(&"=".repeat(21 + kind.len()));
    out.push_str("\n\n");

    // --- Build ---
    out.push_str(&format!("version:   {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("build:     {}\n", env!("ROCKET_TYCOON_GIT")));
    out.push_str(&format!(
        "platform:  {} {}\n", std::env::consts::OS, std::env::consts::ARCH,
    ));
    match crossterm::terminal::size() {
        Ok((w, h)) => out.push_str(&format!("terminal:  {w}x{h}\n")),
        Err(_) => out.push_str("terminal:  unknown\n"),
    }

    // --- World ---
    out.push_str(&format!("\ncompany:   {}\n", c.name));
    out.push_str(&format!("seed:      {}\n", game.seed.seed()));
    out.push_str(&format!(
        "date:      {} (day {}, founded {})\n",
        game.date, game.elapsed_days(), game.start_date,
    ));
    out.push_str(&format!(
        "balance:   {}\n",
        if game.balance == crate::balance_config::BalanceConfig::default() {
            "stock"
        } else {
            "MODIFIED (a balance TOML was loaded — numbers below are not stock)"
        },
    ));
    out.push_str(&format!(
        "economy:   {} ({:+.0}%)\n",
        game.economy.condition.display_name(),
        (game.economy.modifier - 1.0) * 100.0,
    ));

    // --- Company ---
    out.push_str("\nCompany\n-------\n");
    out.push_str(&format!("money:      {:.0}\n", c.money));
    out.push_str(&format!("reputation: {:.1}\n", c.reputation.total()));
    out.push_str(&format!(
        "teams:      {} engineering ({} idle), {} manufacturing\n",
        c.teams.len(), c.unassigned_team_count(), c.manufacturing_teams.len(),
    ));
    out.push_str(&format!("launches:   {}\n", c.launch_history.len()));

    // --- Projects ---
    out.push_str("\nProjects\n--------\n");
    if c.engine_projects.is_empty() && c.rocket_projects.is_empty()
        && c.reactor_projects.is_empty()
    {
        out.push_str("(none)\n");
    }
    for p in &c.engine_projects {
        out.push_str(&format!(
            "engine  {:<24} rev {} {:<10} teams {} flaws {}/{} auto-revise {}\n",
            truncate(&p.design.name, 24), p.revision,
            engine_status(&p.status), p.teams_assigned,
            p.discovered_flaw_count(), p.flaws.len(),
            if p.auto_revise { "on" } else { "off" },
        ));
    }
    for p in &c.rocket_projects {
        out.push_str(&format!(
            "rocket  {:<24} rev {} {:<10} teams {} flaws {}/{} auto-revise {}\n",
            truncate(&p.design.name, 24), p.revision,
            rocket_status(&p.status), p.teams_assigned,
            p.discovered_flaw_count(), p.flaws.len(),
            if p.auto_revise { "on" } else { "off" },
        ));
    }
    for p in &c.reactor_projects {
        out.push_str(&format!(
            "reactor {:<24} rev {} teams {} flaws {}/{} auto-revise {}\n",
            truncate(&p.design.name, 24), p.revision, p.teams_assigned,
            p.discovered_flaw_count(), p.flaws.len(),
            if p.auto_revise { "on" } else { "off" },
        ));
    }

    // --- Manufacturing ---
    out.push_str("\nManufacturing\n-------------\n");
    out.push_str(&format!(
        "orders: {}  inventory: {} engines, {} stages, {} rockets\n",
        c.manufacturing.orders.len(),
        c.manufacturing.inventory.engines.len(),
        c.manufacturing.inventory.stages.len(),
        c.manufacturing.inventory.rockets.len(),
    ));
    for o in &c.manufacturing.orders {
        out.push_str(&format!(
            "  {:<28} teams {}{}\n",
            truncate(&o.order_type.display_name(), 28),
            o.teams_assigned,
            if o.waiting_for_prerequisites { "  (waiting on prerequisites)" } else { "" },
        ));
    }

    // --- Work in flight ---
    out.push_str("\nContracts and flights\n---------------------\n");
    out.push_str(&format!(
        "{} offered, {} accepted, {} flights in transit, {} spacecraft\n",
        game.available_contracts.len(), c.active_contracts.len(),
        game.active_flights.len(), game.spacecraft.len(),
    ));
    for ct in &c.active_contracts {
        out.push_str(&format!(
            "  {:<28} -> {:<10} {:.0} kg  by {}\n",
            truncate(&ct.name, 28), truncate(&ct.destination, 10),
            ct.payload_kg, ct.deadline,
        ));
    }
    for f in &game.active_flights {
        out.push_str(&format!(
            "  flight {:<22} -> {}\n",
            truncate(&f.rocket_name, 22), f.destination(),
        ));
    }
    for comp in &game.competitors {
        out.push_str(&format!(
            "  competitor {:<18} reputation {:.1}\n",
            truncate(&comp.company.name, 18), comp.company.reputation.total(),
        ));
    }

    // --- Attached save ---
    match save_path {
        Some(p) => out.push_str(&format!(
            "\nA save of this game was written to:\n  {}\nPlease attach it — \
             it is what makes the bug reproducible.\n",
            p.display(),
        )),
        None => out.push_str("\nThe game state could NOT be saved.\n"),
    }

    if let Some(text) = panic_text {
        out.push_str("\nPanic\n-----\n");
        out.push_str(text);
        out.push('\n');
    }

    // --- Log ---
    out.push_str("\nEvent log\n---------\n");
    for (date, event) in game.event_log.recent(game.event_log.len()).iter().rev() {
        out.push_str(&format!("{date}: {event}\n"));
    }
    out.push_str(&format!(
        "({} of {} events ever recorded; the log is a ring buffer)\n",
        game.event_log.len(), game.event_log.total_pushed(),
    ));

    out
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).chain(std::iter::once('…')).collect()
    }
}

fn engine_status(s: &crate::engine_project::EngineDesignStatus) -> &'static str {
    use crate::engine_project::EngineDesignStatus as S;
    match s {
        S::Proposed { .. } => "proposed",
        S::InDesign { .. } => "in-design",
        S::Testing { .. } => "testing",
        S::Revising { .. } => "revising",
    }
}

fn rocket_status(s: &crate::rocket_project::RocketDesignStatus) -> &'static str {
    use crate::rocket_project::RocketDesignStatus as S;
    match s {
        S::InDesign { .. } => "in-design",
        S::Testing { .. } => "testing",
        S::Revising { .. } => "revising",
    }
}

/// Directory reports are written to (`<data dir>/reports`).
pub fn reports_dir() -> std::path::PathBuf {
    crate::save::data_dir().join("reports")
}

/// Where a report goes, relative to a data directory. `tag` separates a
/// crash from an on-demand dump; the in-game date keeps successive
/// dumps from one session from overwriting each other, without needing
/// a wall clock.
pub fn report_path_in(
    data_dir: &std::path::Path,
    company_name: &str,
    tag: &str,
    date: crate::calendar::GameDate,
) -> std::path::PathBuf {
    let safe = sanitize(company_name);
    data_dir.join("reports").join(format!(
        "{safe}-{tag}-{:04}-{:02}-{:02}.txt", date.year, date.month, date.day,
    ))
}

/// Where a report goes in the default data directory.
pub fn report_path(company_name: &str, tag: &str, date: crate::calendar::GameDate) -> std::path::PathBuf {
    report_path_in(&crate::save::data_dir(), company_name, tag, date)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Where an on-demand dump parks its snapshot save, relative to a data
/// directory. Separate from both the autosave rotation and the crash
/// save, so pressing F12 never costs the player a recovery point.
pub fn snapshot_save_path_in(data_dir: &std::path::Path, company_name: &str) -> std::path::PathBuf {
    data_dir.join("saves").join(format!("{}.report.json", sanitize(company_name)))
}

/// Snapshot save path in the default data directory.
pub fn snapshot_save_path(company_name: &str) -> std::path::PathBuf {
    snapshot_save_path_in(&crate::save::data_dir(), company_name)
}

/// Write a session report on demand (the `F12` path).
///
/// Writes a snapshot save beside it and names that save in the text,
/// because the save is what actually reproduces a bug — the text is
/// what gets pasted into chat. Returns the report path on success.
pub fn write_session_report_in(
    data_dir: &std::path::Path,
    game: &crate::game_state::GameState,
) -> std::io::Result<std::path::PathBuf> {
    let name = &game.player_company.name;

    let save_path = snapshot_save_path_in(data_dir, name);
    let saved = crate::save::save_game(game, &save_path).is_ok();

    let body = report(game, None, saved.then_some(save_path.as_path()));
    let path = report_path_in(data_dir, name, "session", game.date);
    std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")))?;
    std::fs::write(&path, body)?;
    Ok(path)
}

/// Write a session report to the default data directory.
pub fn write_session_report(
    game: &crate::game_state::GameState,
) -> std::io::Result<std::path::PathBuf> {
    write_session_report_in(&crate::save::data_dir(), game)
}

/// Rescue what we can after a caught panic: write the save, write the
/// report, and return the human-facing message to print once the
/// terminal is usable again.
///
/// Best-effort by construction — a crash handler that panics is worse
/// than none, so every step degrades to a note in the message.
pub fn handle_panic_in(
    data_dir: &std::path::Path,
    game: &crate::game_state::GameState,
    panic_text: &str,
) -> String {
    let name = &game.player_company.name;
    let save_path = data_dir.join("saves")
        .join(format!("{}.crash.json", sanitize(name)));
    let saved = crate::save::save_game(game, &save_path).is_ok();

    let body = report(game, Some(panic_text), saved.then_some(save_path.as_path()));

    let report_path = report_path_in(data_dir, name, "crash", game.date);
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

/// Rescue after a caught panic, writing to the default data directory.
pub fn handle_panic(game: &crate::game_state::GameState, panic_text: &str) -> String {
    handle_panic_in(&crate::save::data_dir(), game, panic_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;

    /// A game with enough going on that the report has real sections
    /// to fill, rather than printing "(none)" everywhere.
    fn game() -> GameState {
        use crate::policy::{BasicPolicy, CompanyPolicy};
        let mut g = GameState::new("Report Co".into(), 200_000_000.0, 3);
        let mut policy = BasicPolicy::new();
        for _ in 0..500 {
            policy.act(&mut g);
            g.advance_day();
        }
        g
    }

    /// The report must carry what's needed to reproduce: build, seed,
    /// date, and whether the balance numbers are stock.
    #[test]
    fn report_carries_what_a_repro_needs() {
        let g = game();
        let text = report(&g, None, None);

        assert!(text.contains("Report Co"));
        assert!(text.contains("seed:      3"), "the seed reproduces the world");
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains("build:"), "the git stamp identifies the code");
        assert!(text.contains("balance:   stock"),
            "a modified balance would invalidate every number below it");
        assert!(text.contains("Event log"), "the log gives context");
        assert!(text.contains("Projects"), "project states are the usual suspect");
    }

    /// A session dump and a crash dump are the same document; only the
    /// panic section and the heading differ. Keeps the two paths
    /// comparable when a player sends one of each.
    #[test]
    fn crash_and_session_reports_differ_only_by_the_panic_section() {
        let g = game();
        let session = report(&g, None, None);
        let crash = report(&g, Some("panicked at 'boom'"), None);

        assert!(session.starts_with("Rocket Tycoon session report"));
        assert!(crash.starts_with("Rocket Tycoon crash report"));
        assert!(!session.contains("Panic\n-----"), "no panic section on demand");
        assert!(crash.contains("boom"), "the panic message must survive");

        // Everything else lines up.
        for marker in ["Company\n-------", "Projects\n--------", "Event log"] {
            assert!(session.contains(marker) && crash.contains(marker),
                "both reports should carry {marker:?}");
        }
    }

    /// The report states the project detail a bug usually turns on:
    /// status, flaw counts, and whether auto-revise was on.
    #[test]
    fn report_describes_projects_in_enough_detail() {
        let g = game();
        assert!(!g.player_company.engine_projects.is_empty(),
            "precondition: the bot built engines");
        let text = report(&g, None, None);

        assert!(text.contains("engine  "), "engine projects should be listed");
        assert!(text.contains("auto-revise"), "the flag changes behaviour, so report it");
        assert!(text.contains("flaws "), "discovered/total flaw counts");
    }

    /// A private data directory. Tests must never write into the
    /// player's real `~/.rocket_tycoon` — an earlier version of this
    /// test did, and left crash saves in a live install.
    fn temp_data_dir(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rt_report_{}_{}_{}",
            tag, std::process::id(), N.fetch_add(1, Ordering::Relaxed),
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A crash handler that crashes is worse than none. Run the whole
    /// path — save, report, message — against a real mid-game state.
    #[test]
    fn handling_a_panic_writes_both_files_and_says_where() {
        let dir = temp_data_dir("panic");
        let g = game();
        let msg = handle_panic_in(&dir, &g, "panicked at 'boom'");

        assert!(msg.contains("had to stop"));
        assert!(msg.contains("saved to"), "the rescued save must be findable");

        let save = dir.join("saves").join("Report_Co.crash.json");
        assert!(save.exists(), "the crash save should be on disk");
        let report_file = report_path_in(&dir, "Report Co", "crash", g.date);
        assert!(report_file.exists(), "the crash report should be on disk");
        assert!(std::fs::read_to_string(&report_file).unwrap().contains("boom"));

        // The rescued save is the point of the exercise: it must load.
        let reloaded = crate::save::load_game(&save)
            .expect("a crash save that won't load is worthless");
        assert_eq!(reloaded.seed.seed(), g.seed.seed());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// F12 writes a report plus a snapshot save, and names the save in
    /// the text so the player knows to attach it.
    #[test]
    fn a_session_report_writes_a_snapshot_beside_it() {
        let dir = temp_data_dir("session");
        let g = game();
        let path = write_session_report_in(&dir, &g).expect("report should write");

        let body = std::fs::read_to_string(&path).unwrap();
        let snapshot = snapshot_save_path_in(&dir, "Report Co");
        assert!(snapshot.exists(), "a snapshot save should sit beside the report");
        assert!(
            body.contains(&snapshot.display().to_string()),
            "the report must name the save to attach",
        );
        assert!(crate::save::load_game(&snapshot).is_ok(), "the snapshot must load");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Empty state is the case a report generator is most likely to
    /// panic on, and it's exactly when a new player hits a bug.
    #[test]
    fn a_brand_new_company_reports_without_panicking() {
        let g = GameState::new("Day One".into(), 200_000_000.0, 1);
        let text = report(&g, None, None);
        assert!(text.contains("Day One"));
        assert!(text.contains("(none)"), "no projects yet, and that's fine");
    }

    /// Report paths are distinct per kind and per in-game date, so a
    /// second dump doesn't silently replace the first.
    #[test]
    fn report_paths_do_not_collide() {
        let d1 = crate::calendar::GameDate::new(2001, 3, 4);
        let d2 = crate::calendar::GameDate::new(2001, 3, 5);
        assert_ne!(report_path("Co", "session", d1), report_path("Co", "crash", d1));
        assert_ne!(report_path("Co", "session", d1), report_path("Co", "session", d2));

        // The snapshot save must not collide with the crash save or the
        // autosave rotation — pressing F12 shouldn't cost a recovery point.
        let snap = snapshot_save_path("Co");
        assert_ne!(snap, crate::save::emergency_save_path("Co"));
        for slot in 1..=crate::save::AUTOSAVE_SLOTS {
            assert_ne!(snap, crate::save::autosave_slot_path("Co", slot));
        }
    }

    /// The hook stashes rather than prints, and take() clears it.
    #[test]
    fn hook_captures_and_clears() {
        *LAST_PANIC.lock().unwrap() = Some("stashed".into());
        assert_eq!(take_panic_text().as_deref(), Some("stashed"));
        assert!(take_panic_text().is_none(), "taking should clear the slot");
    }
}
