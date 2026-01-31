use godot::prelude::*;

pub mod contract;
pub mod engine;
pub mod flaw;
pub mod game_state;
pub mod launcher;
pub mod rocket_design;
pub mod stage;

mod game_manager;
mod player_finance;
mod rocket_designer;
mod rocket_launcher;
mod test_node;

struct RocketTycoonExtension;

#[gdextension]
unsafe impl ExtensionLibrary for RocketTycoonExtension {}
