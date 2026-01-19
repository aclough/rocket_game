use godot::prelude::*;

pub mod launcher;
mod test_node;
mod rocket_launcher;

struct RocketTycoonExtension;

#[gdextension]
unsafe impl ExtensionLibrary for RocketTycoonExtension {}
