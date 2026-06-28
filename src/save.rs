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
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_stem()?.to_string_lossy().to_string();
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((name, path, mtime))
        })
        .collect();
    saves.sort_by(|a, b| b.2.cmp(&a.2)); // newest first
    saves.into_iter().map(|(name, path, _)| (name, path)).collect()
}

/// Save game state to a JSON file.
pub fn save_game(state: &GameState, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    fs::write(path, json)
}

/// Load game state from a JSON file.
pub fn load_game(path: &Path) -> io::Result<GameState> {
    let json = fs::read_to_string(path)?;
    let mut state: GameState = serde_json::from_str(&json)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    // Re-initialize the contingent RNG (not serialized)
    state.seed.fix_after_load();
    // Sweep stale `Proposed` engine projects — these belong to an
    // unfinished rocket designer session that was running when the game
    // was last saved (or quit/crashed before completion). They're
    // hidden from the engine pane anyway, and there's no way to revive
    // a sketch session, so drop them.
    state.player_company.engine_projects.retain(|ep|
        !matches!(ep.status, crate::engine_project::EngineDesignStatus::Proposed { .. })
    );
    // Same sweep for reactor projects — Phase 2 will surface a
    // designer that can create Proposed reactors, but any survivor
    // here is a draft from an aborted session.
    state.player_company.reactor_projects.retain(|rp|
        !matches!(rp.status, crate::reactor_project::ReactorDesignStatus::Proposed { .. })
    );
    Ok(state)
}

/// Default save directory.
pub fn save_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".rocket_tycoon").join("saves")
}

/// Build a save file path for a company name.
pub fn save_path(company_name: &str) -> std::path::PathBuf {
    let sanitized: String = company_name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    save_dir().join(format!("{}.json", sanitized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_path() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("rocket_tycoon_test");
        fs::create_dir_all(&dir).unwrap();
        dir.join(format!("test_save_{}.json", std::process::id()))
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

    #[test]
    fn test_save_path_sanitization() {
        let path = save_path("My Cool Company!");
        assert!(path.to_string_lossy().contains("My_Cool_Company_"));
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
