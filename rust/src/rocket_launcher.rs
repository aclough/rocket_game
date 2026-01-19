use godot::prelude::*;
use crate::launcher::{LaunchSimulator, LaunchStage, LaunchResult};

/// Godot-accessible rocket launcher node
#[derive(GodotClass)]
#[class(base=Node)]
pub struct RocketLauncher {
    base: Base<Node>,
}

#[godot_api]
impl INode for RocketLauncher {
    fn init(base: Base<Node>) -> Self {
        godot_print!("RocketLauncher initialized");
        Self { base }
    }
}

#[godot_api]
impl RocketLauncher {
    /// Launches a rocket and returns success status
    /// Returns true if successful, false if failed
    #[func]
    pub fn launch_rocket(&mut self) -> bool {
        let result = LaunchSimulator::simulate_launch();
        matches!(result, LaunchResult::Success)
    }

    /// Launches a rocket and returns the result message
    #[func]
    pub fn launch_rocket_with_message(&mut self) -> GString {
        let result = LaunchSimulator::simulate_launch();
        GString::from(result.message().as_str())
    }

    /// Launches a rocket with stage-by-stage notifications via signals
    /// Emits "stage_entered" signal for each stage
    /// Emits "launch_completed" signal with success status and message
    #[func]
    pub fn launch_rocket_with_stages(&mut self) {
        let result = LaunchSimulator::simulate_launch_with_callback(|stage| {
            // Emit signal for this stage
            self.base_mut().emit_signal(
                "stage_entered",
                &[GString::from(stage.description()).to_variant()],
            );
        });

        // Emit completion signal with result
        let success = matches!(result, LaunchResult::Success);
        let message = result.message();

        self.base_mut().emit_signal(
            "launch_completed",
            &[success.to_variant(), GString::from(message.as_str()).to_variant()],
        );
    }

    /// Returns the number of launch stages
    #[func]
    pub fn get_stage_count(&self) -> i32 {
        LaunchStage::all_stages().len() as i32
    }

    /// Returns a description of a specific stage by index (0-6)
    #[func]
    pub fn get_stage_description(&self, index: i32) -> GString {
        let stages = LaunchStage::all_stages();
        if index >= 0 && (index as usize) < stages.len() {
            GString::from(stages[index as usize].description())
        } else {
            GString::from("Invalid stage index")
        }
    }

    /// Returns the failure probability for a specific stage by index (0-6)
    #[func]
    pub fn get_stage_failure_rate(&self, index: i32) -> f64 {
        let stages = LaunchStage::all_stages();
        if index >= 0 && (index as usize) < stages.len() {
            stages[index as usize].failure_probability()
        } else {
            0.0
        }
    }

    /// Signal emitted when entering a new launch stage
    /// Parameters: stage_name (String)
    #[signal]
    fn stage_entered(stage_name: GString);

    /// Signal emitted when the launch completes (success or failure)
    /// Parameters: success (bool), message (String)
    #[signal]
    fn launch_completed(success: bool, message: GString);
}
