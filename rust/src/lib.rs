use godot::prelude::*;

pub mod engine;
pub mod launcher;
pub mod rocket_design;
pub mod stage;

mod rocket_designer;
mod rocket_launcher;
mod test_node;

struct RocketTycoonExtension;

#[gdextension]
unsafe impl ExtensionLibrary for RocketTycoonExtension {}
