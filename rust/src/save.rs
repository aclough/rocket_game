use std::fs;
use std::io;
use std::path::Path;

use crate::game_state::GameState;

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
        assert_eq!(loaded.player_company.money, 200_000_000.0);
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
}
