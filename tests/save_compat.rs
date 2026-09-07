//! Load-compatibility against a corpus of real saves from earlier
//! milestones.
//!
//! The files in `tests/saves/` are **not** synthetic. Each was produced
//! by checking out the milestone's own commit in a git worktree,
//! running `BasicPolicy` for three game years, and saving — so they
//! carry exactly the fields that version wrote, and none that it
//! didn't. Their era differences are visible in the JSON: m1 has 18
//! top-level keys, m2 has 20, m3 and m4 have 22 (competitors appear at
//! m3), m5 adds `save_version`.
//!
//! | file | commit | milestone |
//! |---|---|---|
//! | `m1.json` | `b986f00` | M1.5 — before seeded markets |
//! | `m2.json` | `22ec784` | M2 complete — before competitors |
//! | `m3.json` | `df830eb` | M3 complete — before the M4 cost retune |
//! | `m4.json` | `9e200ae` | M4 complete — the pre-M5 shipping state |
//! | `m5.json` | `e79811a` | M5 complete, save v1 — before the R&D pipeline unification |
//!
//! This is the test that catches a field rename silently breaking
//! everyone's saved game between releases. To add an era, generate the
//! file the same way rather than hand-editing an existing one — a
//! hand-stripped save proves only that serde ignores unknown fields,
//! which is not the thing at risk. The recipe is `generate_corpus_snapshot`
//! below; run it from the era's own commit:
//!
//! ```text
//! CORPUS_ERA=m6 cargo test --release --test save_compat generate_corpus_snapshot -- --ignored
//! ```

use std::path::PathBuf;

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::calendar::GameDate;
use rocket_tycoon::game_state::GameState;
use rocket_tycoon::policy::{BasicPolicy, CompanyPolicy};
use rocket_tycoon::save;

fn corpus() -> Vec<(&'static str, PathBuf)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/saves");
    ["m1", "m2", "m3", "m4", "m5"].iter()
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
        let written_at = raw.get("save_version").and_then(|v| v.as_u64()).unwrap_or(0);
        assert!(
            written_at < u64::from(save::SAVE_VERSION),
            "{era}: corpus files predate the current save version; that's the point of them",
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

/// The corpus recipe, as code: seed 42, "Corpus Co", `BasicPolicy` for
/// three game years, default balance, saved to `tests/saves/<era>.json`.
/// Ignored so the suite never overwrites an era by accident; run it by
/// hand from the commit the era should capture (see the module doc).
#[test]
#[ignore]
fn generate_corpus_snapshot() {
    let era = std::env::var("CORPUS_ERA")
        .expect("set CORPUS_ERA=<name> to choose the output file");
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/saves")
        .join(format!("{era}.json"));
    assert!(!out.exists(), "{} already exists; eras are never regenerated", out.display());

    let mut policy = BasicPolicy::new();
    let mut gs = GameState::with_balance("Corpus Co".into(), 42, BalanceConfig::default());
    let end = GameDate::new(gs.date.year + 3, 1, 1);
    while gs.date < end {
        policy.act(&mut gs);
        gs.advance_day();
    }
    save::save_game(&gs, &out).unwrap();
    eprintln!("wrote {}", out.display());
}
