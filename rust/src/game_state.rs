use crate::engine::EngineDesign;
use crate::rocket::RocketDesign;

/// A player's rocket company.
pub struct Company {
    pub name: String,
    pub money: f64,
    pub engine_designs: Vec<EngineDesign>,
    pub rocket_designs: Vec<RocketDesign>,
}

impl Company {
    pub fn new(name: String, starting_money: f64) -> Self {
        Company {
            name,
            money: starting_money,
            engine_designs: Vec::new(),
            rocket_designs: Vec::new(),
        }
    }
}

/// Top-level game state.
pub struct GameState {
    pub day: u32,
    pub player_company: Company,
}

impl GameState {
    pub fn new(company_name: String, starting_money: f64) -> Self {
        GameState {
            day: 1,
            player_company: Company::new(company_name, starting_money),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_game_state() {
        let gs = GameState::new("SpaceCorp".into(), 10_000_000.0);
        assert_eq!(gs.day, 1);
        assert_eq!(gs.player_company.name, "SpaceCorp");
        assert_eq!(gs.player_company.money, 10_000_000.0);
        assert!(gs.player_company.engine_designs.is_empty());
        assert!(gs.player_company.rocket_designs.is_empty());
    }
}
