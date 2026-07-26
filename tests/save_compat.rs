//! Load-compatibility against a corpus of real saves from earlier
//! milestones.
//!
//! The files in `tests/saves/` are **not** synthetic. Each was produced
//! by checking out the milestone's own commit in a git worktree,
//! running `BasicPolicy` for three game years, and saving — so they
//! carry exactly the fields that version wrote, and none that it
//! didn't. Their era differences are visible in the JSON: m1 has 18
//! top-level keys, m2 has 20, m3 and m4 have 22 (competitors appear at
//! m3).
//!
//! | file | commit | milestone |
//! |---|---|---|
//! | `m1.json` | `b986f00` | M1.5 — before seeded markets |
//! | `m2.json` | `22ec784` | M2 complete — before competitors |
//! | `m3.json` | `df830eb` | M3 complete — before the M4 cost retune |
//! | `m4.json` | `9e200ae` | M4 complete — the pre-M5 shipping state |
//!
//! This is the test that catches a field rename silently breaking
//! everyone's saved game between releases. To add an era, generate the
//! file the same way rather than hand-editing an existing one — a
//! hand-stripped save proves only that serde ignores unknown fields,
//! which is not the thing at risk.

use std::path::PathBuf;

use rocket_tycoon::game_state::GameState;
use rocket_tycoon::save;

fn corpus() -> Vec<(&'static str, PathBuf)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/saves");
    ["m1", "m2", "m3", "m4"].iter()
        .map(|era| (*era, dir.join(format!("{era}.json"))))
        .collect()
}

fn load(path: &std::path::Path) -> GameState {
    save::load_game(path)
        .unwrap_or_else(|e| panic!("failed to load {}: {e}", path.display()))
}

/// Every corpus save still loads, and comes out coherent enough to
/// keep playing.
#[test]
fn every_era_loads_and_is_playable() {
    for (era, path) in corpus() {
        assert!(path.exists(), "{era} corpus save is missing at {}", path.display());
        let mut state = load(&path);

        assert_eq!(state.player_company.name, "Corpus Co", "{era}: company survived");
        assert!(state.date.year >= 2003, "{era}: three game years elapsed");
        assert!(state.player_company.money.is_finite(), "{era}: money is a number");
        assert!(
            !state.player_company.engine_projects.is_empty(),
            "{era}: the bot's engine projects survived",
        );
        assert!(
            !state.markets.is_empty(),
            "{era}: markets are present (defaulted for pre-M2 saves)",
        );

        // The real proof it's playable: it ticks without panicking and
        // time actually moves.
        let before = state.date;
        for _ in 0..90 {
            state.advance_day();
        }
        assert!(state.date > before, "{era}: the clock advanced after loading");
    }
}

/// Old saves are stamped with the current version on load, and the
/// always-run repairs give pre-M3 worlds the competitor they never had.
#[test]
fn loading_stamps_the_version_and_repairs_old_worlds() {
    for (era, path) in corpus() {
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(
            raw.get("save_version").is_none(),
            "{era}: corpus files predate save_version; that's the point of them",
        );

        let state = load(&path);
        assert_eq!(
            state.save_version, save::SAVE_VERSION,
            "{era}: load should stamp the current version",
        );

        // Every world has a competitor afterwards, including the m1/m2
        // ones written before DinoSoar existed.
        assert!(
            !state.competitors.is_empty(),
            "{era}: loading should backfill DinoSoar",
        );
    }
}

/// Loading is idempotent: save what you loaded, load it again, and
/// nothing shifts. Catches a migration that isn't safe to re-run.
#[test]
fn round_tripping_a_migrated_save_is_stable() {
    let dir = std::env::temp_dir().join("rocket_tycoon_compat");
    std::fs::create_dir_all(&dir).unwrap();

    for (era, path) in corpus() {
        let first = load(&path);
        let out = dir.join(format!("{era}-{}.json", std::process::id()));
        save::save_game(&first, &out).unwrap();
        let second = load(&out);

        let a = serde_json::to_value(&first).unwrap();
        let b = serde_json::to_value(&second).unwrap();
        assert_eq!(a, b, "{era}: re-loading a migrated save changed it");

        assert_eq!(
            second.competitors.len(), first.competitors.len(),
            "{era}: the competitor backfill must not stack up on re-load",
        );
        let _ = std::fs::remove_file(&out);
    }
}
