use godot::prelude::*;

pub mod company;
pub mod contract;
pub mod engine;
pub mod engineering_team;
pub mod flaw;
pub mod game_state;
pub mod launch_site;
pub mod launcher;
pub mod rocket_design;
pub mod stage;
pub mod time_system;
pub mod world_seed;

mod game_manager;
mod player_finance;
mod rocket_designer;
mod rocket_launcher;
mod test_node;

struct RocketTycoonExtension;

#[gdextension]
unsafe impl ExtensionLibrary for RocketTycoonExtension {}
