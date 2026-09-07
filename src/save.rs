use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::game_state::GameState;

/// List saved games as (company_name, full_path), sorted by modification time (newest first).
pub fn list_saves() -> Vec<(String, PathBuf)> {
    let dir = save_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut saves: Vec<(String, PathBuf, std::time::SystemTime)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_stem()?.to_string_lossy().to_string();
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((name, path, mtime))
        })
        .collect();
    saves.sort_by_key(|&(_, _, mtime)| std::cmp::Reverse(mtime)); // newest first
    saves.into_iter().map(|(name, path, _)| (name, path)).collect()
}

/// Save game state to a JSON file.
pub fn save_game(state: &GameState, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(io::Error::other)?;
    fs::write(path, json)
}

/// Current save format version.
///
/// Bump this **in the same edit** as adding a `migrate` arm below.
/// Version 0 means "written before the field existed" — every save
/// from M1 through M4 deserializes as 0 via `#[serde(default)]`.
///
/// v1 (M5) introduced versioning itself and contains no format
/// changes: everything the old loader repaired turned out to be
/// state repair rather than format migration, and lives in
/// `sanitize`.
///
/// v2: revision queues address flaws and improvements by id instead of
/// by vec index, and improvements carry an `id`. See `migrate_json`.
pub const SAVE_VERSION: u32 = 2;

/// Load game state from a JSON file, running any migrations the file
/// needs, then stamping it with the current version.
///
/// Shape changes are applied to the raw JSON *before* it is typed
/// (`migrate_json`), so a field that no longer exists in the Rust
/// types can still be read and converted. State repair that needs game
/// logic runs afterwards on the typed value (`sanitize`, `migrate`).
pub fn load_game(path: &Path) -> io::Result<GameState> {
    let json = fs::read_to_string(path)?;
    let invalid = |e: serde_json::Error| io::Error::new(io::ErrorKind::InvalidData, e);
    let mut raw: serde_json::Value = serde_json::from_str(&json).map_err(invalid)?;
    let from = raw.get("save_version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    migrate_json(&mut raw, from);
    let mut state: GameState = serde_json::from_value(raw).map_err(invalid)?;

    // Not serialized — rebuilt on every load regardless of version.
    state.seed.fix_after_load();

    sanitize(&mut state);
    migrate(&mut state);
    state.save_version = SAVE_VERSION;
    Ok(state)
}

/// Repairs that apply to *every* save, new or old, because they fix
/// states the game can legitimately be saved in — not format changes.
fn sanitize(state: &mut GameState) {
    // Sweep stale `Proposed` projects. These belong to a rocket-designer
    // session that was open when the game was saved (or that crashed
    // before finishing). They're hidden from the panes anyway and there
    // is no way to revive a sketch session, so drop them. This is not a
    // migration: a save written today can contain them.
    state.player_company.engine_projects.retain(|ep|
        !matches!(ep.status, crate::engine_project::EngineDesignStatus::Proposed { .. })
    );
    state.player_company.reactor_projects.retain(|rp|
        !matches!(rp.status, crate::reactor_project::ReactorDesignStatus::Proposed { .. })
    );

    // A world with competitors enabled but none present is incoherent
    // however it arose — a pre-M3 save that predates DinoSoar, or a
    // later one whose competitor list was emptied. DinoSoar joins as a
    // fresh company with the seeded realization it would have had at
    // that world's creation.
    //
    // This is deliberately *not* version-gated. Gating it on v0 broke
    // `competitor_survives_save_load`, which clears the list on a
    // current-version save — and that test is right: the repair is
    // about the state being inconsistent, not about the file being old.
    if state.competitors.is_empty() && state.balance.competitor.enabled {
        state.competitors.push(
            crate::competitor::realize_dinosoar(&state.seed, &state.balance),
        );
    }
}

/// Version-gated shape migrations on the untyped JSON, applied in
/// order. Each arm must be idempotent and must leave the document
/// loadable by the next arm. `from` is the version the file was
/// written at (0 for files that predate versioning).
fn migrate_json(root: &mut serde_json::Value, from: u32) {
    if from < 2 {
        // v1 -> v2: revision queues held vec indices into `flaws` /
        // `improvements`, which shifted under a removal; they now hold
        // ids, and improvements gained one. Improvements are snapshotted
        // onto manufacturing orders and inventory as well as living on
        // projects, so the id stamp walks the whole document; the queue
        // rewrite is project-only.
        stamp_improvement_ids(root);
        if let Some(company) = root.get_mut("player_company") {
            migrate_company_to_v2(company);
        }
        if let Some(comps) = root.get_mut("competitors").and_then(|c| c.as_array_mut()) {
            for comp in comps {
                if let Some(company) = comp.get_mut("company") {
                    migrate_company_to_v2(company);
                }
            }
        }
    }
}

fn migrate_company_to_v2(company: &mut serde_json::Value) {
    for list in ["engine_projects", "rocket_projects", "reactor_projects"] {
        let Some(projects) = company.get_mut(list).and_then(|l| l.as_array_mut()) else {
            continue;
        };
        for project in projects {
            migrate_project_to_v2(project);
        }
    }
}

/// Give every improvement in the document an `id` equal to its position
/// in its array. Project improvement vecs were append-only, so the
/// position was already a stable identity; snapshots elsewhere (build
/// orders, inventory) are copies and only need the field to exist.
fn stamp_improvement_ids(value: &mut serde_json::Value) {
    use serde_json::{json, Value};
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if key == "improvements" {
                    if let Some(imps) = child.as_array_mut() {
                        for (i, imp) in imps.iter_mut().enumerate() {
                            if let Some(obj) = imp.as_object_mut() {
                                obj.entry("id").or_insert(json!(i));
                            }
                        }
                    }
                } else {
                    stamp_improvement_ids(child);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(stamp_improvement_ids),
        _ => {}
    }
}

/// One project's v1 -> v2 rewrite: set the improvement allocator past
/// the ids `stamp_improvement_ids` assigned, then translate any
/// `Revising` queue from positions to ids.
fn migrate_project_to_v2(project: &mut serde_json::Value) {
    use serde_json::{json, Value};

    let flaw_ids: Vec<Value> = project.get("flaws")
        .and_then(|f| f.as_array())
        .map(|flaws| flaws.iter().map(|f| f.get("id").cloned().unwrap_or(Value::Null)).collect())
        .unwrap_or_default();

    let improvement_count = project.get("improvements")
        .and_then(|i| i.as_array())
        .map_or(0, |i| i.len());
    if let Some(obj) = project.as_object_mut() {
        if obj.contains_key("improvements") {
            obj.entry("next_improvement_id").or_insert(json!(improvement_count));
        }
    }

    let Some(revising) = project.get_mut("status")
        .and_then(|s| s.get_mut("Revising"))
        .and_then(|r| r.as_object_mut())
    else {
        return;
    };
    let positions = |v: Value| -> Vec<usize> {
        v.as_array().into_iter().flatten()
            .filter_map(|i| i.as_u64()).map(|i| i as usize).collect()
    };
    // Rocket projects named the queue `remaining_indices`; engines and
    // reactors `remaining_flaw_indices`.
    for old_key in ["remaining_indices", "remaining_flaw_indices"] {
        if let Some(idx) = revising.remove(old_key) {
            let ids: Vec<Value> = positions(idx).into_iter()
                .filter_map(|i| flaw_ids.get(i).cloned())
                .collect();
            revising.insert("remaining_flaw_ids".into(), Value::Array(ids));
        }
    }
    if let Some(idx) = revising.remove("remaining_improvement_indices") {
        // Ids were just assigned as positions, so this is the identity.
        let ids: Vec<Value> = positions(idx).into_iter().map(|i| json!(i)).collect();
        revising.insert("remaining_improvement_ids".into(), Value::Array(ids));
    }
}

/// Version-gated migrations that need the typed state. Empty today;
/// shape changes belong in `migrate_json`.
fn migrate(_state: &mut GameState) {}

/// How many rotating autosave slots each company keeps.
pub const AUTOSAVE_SLOTS: u32 = 3;

/// Path for autosave slot `n` (1-based) of a company, inside `dir`.
pub fn autosave_slot_path_in(dir: &Path, company_name: &str, slot: u32) -> PathBuf {
    dir.join(format!("{}.auto{}.json", sanitize_name(company_name), slot))
}

/// Path for autosave slot `n` (1-based) in the default save directory.
pub fn autosave_slot_path(company_name: &str, slot: u32) -> PathBuf {
    autosave_slot_path_in(&save_dir(), company_name, slot)
}

/// The slot to write next: the first one that doesn't exist yet,
/// otherwise the oldest. Derived from the filesystem rather than
/// tracked in the save, so rotation survives restarts and crashes
/// without a counter to keep in sync.
pub fn next_autosave_slot_in(dir: &Path, company_name: &str) -> u32 {
    let mut oldest = (1, None::<std::time::SystemTime>);
    for slot in 1..=AUTOSAVE_SLOTS {
        let path = autosave_slot_path_in(dir, company_name, slot);
        let mtime = fs::metadata(&path).ok().and_then(|m| m.modified().ok());
        match mtime {
            // An empty slot always wins — fill all three before reusing.
            None => return slot,
            Some(t) => {
                if oldest.1.is_none_or(|best| t < best) {
                    oldest = (slot, Some(t));
                }
            }
        }
    }
    oldest.0
}

/// The slot to write next in the default save directory.
pub fn next_autosave_slot(company_name: &str) -> u32 {
    next_autosave_slot_in(&save_dir(), company_name)
}

/// Write a rotating autosave into `dir`. Returns the path written.
pub fn autosave_in(dir: &Path, state: &GameState) -> io::Result<PathBuf> {
    let name = &state.player_company.name;
    let path = autosave_slot_path_in(dir, name, next_autosave_slot_in(dir, name));
    save_game(state, &path)?;
    Ok(path)
}

/// Write a rotating autosave to the default save directory.
pub fn autosave(state: &GameState) -> io::Result<PathBuf> {
    autosave_in(&save_dir(), state)
}

/// Where a crash dump puts the state it rescued. Kept out of the
/// autosave rotation so a later autosave can't overwrite the one save
/// that captures the bug.
pub fn emergency_save_path(company_name: &str) -> PathBuf {
    save_dir().join(format!("{}.crash.json", sanitize_name(company_name)))
}

/// Filesystem-safe form of a company name.
fn sanitize_name(company_name: &str) -> String {
    company_name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Root data directory — saves, autosaves, and reports all live under
/// here, and this is the only function that decides where "here" is.
///
/// - Windows: `%APPDATA%\RocketTycoon` (no leading dot — a dotdir is a
///   Unix convention and just looks like a broken folder in Explorer).
/// - Everywhere else: `$HOME/.rocket_tycoon`, unchanged from M1, so
///   nobody's existing saves move. There is no migration to write:
///   the Unix path never changed and Windows has no prior installs to
///   migrate from.
///
/// `ROCKET_TYCOON_DATA_DIR` overrides both. That exists for players who
/// keep their home directory tidy, and it means the game can be pointed
/// at a scratch directory without touching `HOME` — which broke `cargo`
/// itself the first time the tests tried it, since rustup lives there.
pub fn data_dir() -> PathBuf {
    resolve_data_dir(
        std::env::var_os("ROCKET_TYCOON_DATA_DIR").as_deref(),
        std::env::var_os("APPDATA").as_deref(),
        std::env::var_os("HOME").as_deref(),
        cfg!(windows),
    )
}

/// The path choice as a pure function of the environment, so the
/// Windows rule is testable from Linux and vice versa. `cfg!(windows)`
/// is passed in rather than read here for exactly that reason: a
/// `#[cfg]`-gated body would only ever be checked by the platform it
/// was written for, and CI would find the mistake instead of the tests.
fn resolve_data_dir(
    over: Option<&std::ffi::OsStr>,
    appdata: Option<&std::ffi::OsStr>,
    home: Option<&std::ffi::OsStr>,
    windows: bool,
) -> PathBuf {
    // An override set to the empty string is a misconfiguration, not a
    // request to write into the filesystem root.
    if let Some(dir) = over.filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let (base, name) = if windows {
        (appdata, "RocketTycoon")
    } else {
        (home, ".rocket_tycoon")
    };
    match base.filter(|b| !b.is_empty()) {
        Some(base) => Path::new(base).join(name),
        // Both vars are set by every normal login on their platform.
        // Falling back to the working directory keeps a save possible
        // in a stripped environment (a bare `cron`, a Docker `RUN`)
        // rather than losing the player's game to a panic.
        None => PathBuf::from(".").join(name),
    }
}

/// Default save directory.
pub fn save_dir() -> PathBuf {
    data_dir().join("saves")
}

/// Build a save file path for a company name.
pub fn save_path(company_name: &str) -> std::path::PathBuf {
    save_dir().join(format!("{}.json", sanitize_name(company_name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_path() -> std::path::PathBuf {
        // Unique per call so save tests running in parallel never share a
        // file (they'd otherwise read each other's data intermittently).
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join("rocket_tycoon_test");
        fs::create_dir_all(&dir).unwrap();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        dir.join(format!("test_save_{}_{}.json", std::process::id(), n))
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let path = temp_path();
        let mut state = GameState::new("TestCorp".into(), 200_000_000.0, 42);

        // Advance a few days to have some state
        for _ in 0..5 {
            state.advance_day();
        }

        save_game(&state, &path).expect("save failed");
        let loaded = load_game(&path).expect("load failed");

        assert_eq!(loaded.date, state.date);
        assert_eq!(loaded.start_date, state.start_date);
        assert_eq!(loaded.player_company.name, "TestCorp");
        // Starting money minus initial team hiring cost
        assert!((loaded.player_company.money - state.player_company.money).abs() < 0.01);
        assert_eq!(loaded.seed.seed(), 42);
        assert_eq!(loaded.event_log.len(), state.event_log.len());

        // Clean up
        let _ = fs::remove_file(&path);
    }

    /// A v1 save with an engine project mid-revision: the flaw queue
    /// held positions, improvements had no ids, and the project had no
    /// allocator. After load the queue names the same flaws by id and
    /// the improvements are numbered by position.
    #[test]
    fn v1_revision_queues_migrate_to_ids() {
        use crate::engine::EngineCycle;
        use crate::engine_project::{EngineDesignStatus, PropellantPreset};
        use crate::flaw::{Flaw, FlawConsequence, FlawId, FlawTrigger};
        use crate::project::ImprovementId;
        use serde_json::{json, Value};

        let mut state = GameState::new("Mig Co".into(), 200_000_000.0, 42);
        let balance = state.balance.clone();
        state.player_company.start_engine_project(
            "Old".into(), EngineCycle::GasGenerator, PropellantPreset::Kerolox, 1.0, None, &balance,
        );
        {
            let ep = &mut state.player_company.engine_projects[0];
            for (id, discovered) in [(10u64, false), (11, true), (12, true)] {
                ep.flaws.push(Flaw {
                    id: FlawId(id),
                    description: format!("flaw {id}"),
                    consequence: FlawConsequence::EngineLoss,
                    activation_chance: 0.1,
                    discovery_probability: 0.5,
                    discovered,
                    trigger: FlawTrigger::PerFlight,
                });
            }
            for (i, actualized) in [(0u64, true), (1, false)] {
                ep.improvements.push(crate::engine_project::EngineImprovement {
                    id: ImprovementId(i),
                    description: format!("imp {i}"),
                    kind: crate::engine_project::EngineImprovementKind::Isp(0.02),
                    actualized,
                });
            }
            ep.next_improvement_id = 2;
            ep.status = EngineDesignStatus::Revising {
                remaining_flaw_ids: vec![FlawId(11), FlawId(12)],
                remaining_improvement_ids: vec![ImprovementId(1)],
                remaining_tech_deficiency_ids: Vec::new(),
                work_completed: 3.0,
            };
        }

        // Downgrade the document to the v1 shape by hand.
        let mut raw = serde_json::to_value(&state).unwrap();
        raw.as_object_mut().unwrap().insert("save_version".into(), json!(1));
        let ep = &mut raw["player_company"]["engine_projects"][0];
        ep.as_object_mut().unwrap().remove("next_improvement_id");
        for imp in ep["improvements"].as_array_mut().unwrap() {
            imp.as_object_mut().unwrap().remove("id");
        }
        let rev = ep["status"]["Revising"].as_object_mut().unwrap();
        rev.remove("remaining_flaw_ids");
        rev.remove("remaining_improvement_ids");
        rev.insert("remaining_flaw_indices".into(), json!([1, 2]));
        rev.insert("remaining_improvement_indices".into(), json!([1]));
        assert!(ep["improvements"][0].get("id").is_none(), "premise: v1 has no ids");

        let path = temp_path();
        fs::write(&path, serde_json::to_string(&raw).unwrap()).unwrap();
        let loaded = load_game(&path).expect("v1 save should load");
        let _ = fs::remove_file(&path);

        assert_eq!(loaded.save_version, SAVE_VERSION);
        let ep = &loaded.player_company.engine_projects[0];
        assert_eq!(ep.next_improvement_id, 2);
        assert_eq!(ep.improvements[0].id, ImprovementId(0));
        assert_eq!(ep.improvements[1].id, ImprovementId(1));
        match &ep.status {
            EngineDesignStatus::Revising { remaining_flaw_ids, remaining_improvement_ids, work_completed, .. } => {
                assert_eq!(remaining_flaw_ids, &vec![FlawId(11), FlawId(12)]);
                assert_eq!(remaining_improvement_ids, &vec![ImprovementId(1)]);
                assert_eq!(*work_completed, 3.0);
            }
            other => panic!("expected Revising, got {other:?}"),
        }
        let _: Value = serde_json::to_value(&loaded).unwrap(); // re-serialisable
    }

    /// The rocket variant named its queue differently. Taken from the
    /// real m4 corpus save with one project's flaws and status
    /// rewritten (the bot had revised every flaw away, so two are put
    /// back), so the rest of the document is genuine v0 output.
    #[test]
    fn v0_rocket_revision_queue_migrates_to_ids() {
        use crate::rocket_project::RocketDesignStatus;
        use serde_json::json;

        let corpus = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/saves/m4.json");
        let mut raw: serde_json::Value = serde_json::from_str(&fs::read_to_string(corpus).unwrap()).unwrap();
        let rp = &mut raw["player_company"]["rocket_projects"][0];
        let flaw = |id: u64| json!({
            "id": id, "description": format!("flaw {id}"), "consequence": "EngineLoss",
            "activation_chance": 0.1, "discovery_probability": 0.5, "discovered": true,
            "trigger": "PerFlight",
        });
        let flaw_ids = [501u64, 502];
        rp["flaws"] = json!([flaw(flaw_ids[0]), flaw(flaw_ids[1])]);
        rp["status"] = json!({ "Revising": { "remaining_indices": [1, 0], "work_completed": 0.0 } });

        let path = temp_path();
        fs::write(&path, serde_json::to_string(&raw).unwrap()).unwrap();
        let loaded = load_game(&path).expect("v0 save should load");
        let _ = fs::remove_file(&path);

        match &loaded.player_company.rocket_projects[0].status {
            RocketDesignStatus::Revising { remaining_flaw_ids, .. } => {
                let got: Vec<u64> = remaining_flaw_ids.iter().map(|f| f.0).collect();
                assert_eq!(got, vec![flaw_ids[1], flaw_ids[0]], "positions map to the flaws they pointed at, in order");
            }
            other => panic!("expected Revising, got {other:?}"),
        }
    }

    #[test]
    fn test_save_path_sanitization() {
        let path = save_path("My Cool Company!");
        assert!(path.to_string_lossy().contains("My_Cool_Company_"));
    }

    /// `resolve_data_dir` is pure, so both platforms' rules are checked
    /// on every platform. Reads nothing from the real environment.
    mod data_dir {
        use super::super::resolve_data_dir;
        use std::ffi::OsStr;
        use std::path::PathBuf;

        fn os(s: &str) -> &OsStr {
            OsStr::new(s)
        }

        fn resolve(over: Option<&str>, appdata: &str, home: &str, windows: bool) -> PathBuf {
            resolve_data_dir(
                over.map(os), Some(os(appdata)), Some(os(home)), windows,
            )
        }

        #[test]
        fn each_platform_uses_its_own_convention() {
            assert_eq!(
                resolve(None, r"C:\Users\a\AppData\Roaming", r"C:\Users\a", true),
                PathBuf::from(r"C:\Users\a\AppData\Roaming").join("RocketTycoon"),
                "Windows should use %APPDATA%, with no leading dot",
            );
            assert_eq!(
                resolve(None, "", "/home/a", false),
                PathBuf::from("/home/a/.rocket_tycoon"),
                "Unix must keep the M1 path — existing saves live there",
            );
        }

        #[test]
        fn the_override_wins_on_both_platforms() {
            for windows in [true, false] {
                assert_eq!(
                    resolve(Some("/scratch/rt"), r"C:\AppData", "/home/a", windows),
                    PathBuf::from("/scratch/rt"),
                );
            }
        }

        #[test]
        fn an_empty_variable_is_ignored_rather_than_used_as_a_root() {
            // `VAR=` is a misconfiguration; joining onto it would put
            // saves in the filesystem root.
            assert_eq!(
                resolve(Some(""), "", "/home/a", false),
                PathBuf::from("/home/a/.rocket_tycoon"),
            );
            assert_eq!(
                resolve(None, "", "", true),
                PathBuf::from("./RocketTycoon"),
            );
        }

        #[test]
        fn a_missing_home_falls_back_to_the_working_directory() {
            // Saving somewhere odd beats panicking and losing the game.
            assert_eq!(
                resolve_data_dir(None, None, None, false),
                PathBuf::from("./.rocket_tycoon"),
            );
            assert_eq!(
                resolve_data_dir(None, None, None, true),
                PathBuf::from("./RocketTycoon"),
            );
        }
    }

    #[test]
    fn test_save_and_load_with_spacecraft_payload() {
        // Round-trip a Spacecraft (carrying a nested Spacecraft payload)
        // through save/load and confirm the nested manifest survives.
        use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
        use crate::flight::Payload;
        use crate::game_state::Spacecraft;
        use crate::propellant::Propellant;
        use crate::rocket::{RocketDesign, RocketDesignId, RocketId};
        use crate::rocket_project::RocketProjectId;
        use crate::stage::{Stage, StageId};

        let path = temp_path();
        let mut state = GameState::new("PayloadCorp".into(), 100.0, 7);

        let make_design = |id: u64, name: &str| -> RocketDesign {
            let engine = EngineDesign {
                id: EngineId(id), name: "E".into(),
                cycle: EngineCycle::GasGenerator,
                thrust_n: 1.0, mass_kg: 1.0, isp_s: 100.0,
                exit_pressure_pa: 1.0, needs_atmosphere: false,
                propellant_mix: vec![PropellantFraction {
                    propellant: Propellant::LOX, mass_fraction: 1.0,
                }],
                power_draw_w: 0.0,
            };
            let stage = Stage {
                id: StageId(id), name: "S".into(),
                engine, engine_count: 1,
                propellant_mass_kg: 100.0, structural_mass_kg: 10.0,
                fairing: None,
                power_sources: Vec::new(),
            };
            RocketDesign {
                id: RocketDesignId(id), name: name.into(),
                stage_groups: vec![vec![stage]],
            }
        };
        let csm_design = make_design(1, "CSM");
        let lem_design = make_design(2, "LEM");
        let lem_rocket = lem_design.instantiate(RocketId(2), "lunar_orbit", 0.0);
        let csm_rocket = csm_design.instantiate(RocketId(1), "lunar_orbit", 0.0);

        let lem_payload = Payload::Spacecraft {
            deploy_at: Some("lunar_surface".into()),
            design: lem_design,
            rocket: lem_rocket,
            nested_payloads: vec![],
            rocket_project_id: RocketProjectId(2),
            name: "LEM".into(),
        };
        state.spacecraft.push(Spacecraft {
            id: crate::game_state::SpacecraftId(1),
            name: "CSM".into(),
            rocket: csm_rocket,
            design: csm_design,
            location: "lunar_orbit".into(),
            rocket_project_id: RocketProjectId(1),
            payloads: vec![lem_payload],
        });

        save_game(&state, &path).expect("save failed");
        let loaded = load_game(&path).expect("load failed");

        assert_eq!(loaded.spacecraft.len(), 1);
        assert_eq!(loaded.spacecraft[0].payloads.len(), 1);
        match &loaded.spacecraft[0].payloads[0] {
            Payload::Spacecraft { name, deploy_at, .. } => {
                assert_eq!(name, "LEM");
                assert_eq!(deploy_at.as_deref(), Some("lunar_surface"));
            }
            _ => panic!("nested payload variant lost in round-trip"),
        }

        let _ = fs::remove_file(&path);
    }
}

#[cfg(test)]
mod autosave_tests {
    use super::*;
    use crate::game_state::GameState;

    /// A private temp directory per test. Deliberately does *not*
    /// touch `HOME`: `save_dir()` reads it at call time, so mutating it
    /// would be a process-wide change that could race
    /// `test_save_path_sanitization` under the default parallel
    /// runner. The `*_in` variants take the directory instead.
    fn temp_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rt_autosave_{}_{}_{}",
            tag, std::process::id(), N.fetch_add(1, Ordering::Relaxed),
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Slots fill 1, 2, 3 before anything is reused — so three
    /// autosaves back is always available, not just the latest.
    #[test]
    fn autosave_fills_every_slot_before_reusing_one() {
        let dir = temp_dir("fill");
        let game = GameState::new("Rotate Co".into(), 100.0, 1);
        for expected in 1..=AUTOSAVE_SLOTS {
            assert_eq!(next_autosave_slot_in(&dir, "Rotate Co"), expected);
            autosave_in(&dir, &game).expect("autosave should succeed");
        }
        for slot in 1..=AUTOSAVE_SLOTS {
            assert!(
                autosave_slot_path_in(&dir, "Rotate Co", slot).exists(),
                "slot {slot} should have been written",
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    /// Once full, the oldest slot is the one overwritten.
    #[test]
    fn a_full_rotation_replaces_the_oldest() {
        let dir = temp_dir("oldest");
        let game = GameState::new("Rotate Co".into(), 100.0, 1);
        for _ in 1..=AUTOSAVE_SLOTS {
            autosave_in(&dir, &game).unwrap();
        }
        // Make slot 2 the oldest by pushing the others forward.
        let now = std::time::SystemTime::now();
        for slot in [1, 3] {
            let f = fs::File::options().write(true)
                .open(autosave_slot_path_in(&dir, "Rotate Co", slot)).unwrap();
            f.set_modified(now + std::time::Duration::from_secs(60)).unwrap();
        }
        assert_eq!(next_autosave_slot_in(&dir, "Rotate Co"), 2,
            "the oldest slot should be reused first");
        let _ = fs::remove_dir_all(&dir);
    }

    /// The crash save is outside the rotation, so a later autosave
    /// can't overwrite the one file that reproduces the bug.
    #[test]
    fn the_crash_save_is_not_an_autosave_slot() {
        let crash = emergency_save_path("Rotate Co");
        for slot in 1..=AUTOSAVE_SLOTS {
            assert_ne!(crash, autosave_slot_path("Rotate Co", slot));
        }
        assert!(crash.to_string_lossy().contains("crash"));
    }

    /// An autosave is a real save: it round-trips through the same
    /// loader, migrations and all.
    #[test]
    fn autosaves_load_back() {
        let dir = temp_dir("roundtrip");
        let mut game = GameState::new("Rotate Co".into(), 100.0, 5);
        for _ in 0..70 {
            game.advance_day();
        }
        let path = autosave_in(&dir, &game).unwrap();
        let loaded = load_game(&path).expect("an autosave must be loadable");
        assert_eq!(loaded.date, game.date);
        assert_eq!(loaded.save_version, SAVE_VERSION);
        let _ = fs::remove_dir_all(&dir);
    }
}
