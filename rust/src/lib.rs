use godot::prelude::*;

pub mod balance;
pub mod company;
pub mod contract;
pub mod depot_design;
pub mod cost_tracker;
pub mod design_lineage;
pub mod design_workflow;
pub mod engine;
pub mod engine_design;
pub mod engineering_team;
pub mod flaw;
pub mod flight_state;
pub mod fuel_depot;
pub mod game_state;
pub mod launch_site;
pub mod location;
pub mod manufacturing;
pub mod mission_plan;
pub mod launcher;
pub mod resources;
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
