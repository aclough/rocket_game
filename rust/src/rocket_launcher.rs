use godot::prelude::*;

use crate::launcher::{LaunchResult, LaunchSimulator, LaunchStage};
use crate::rocket_design::{LaunchEvent, RocketDesign};

/// Godot-accessible rocket launcher node
/// Can use either fixed stages (legacy) or dynamic stages from a rocket design
#[derive(GodotClass)]
#[class(base=Node)]
pub struct RocketLauncher {
    base: Base<Node>,
    /// Optional rocket design for dynamic staging
    design: Option<RocketDesign>,
    /// Cached launch events from the design
    cached_events: Vec<LaunchEvent>,
    /// Whether to use the design (true) or fixed stages (false)
    use_design: bool,
}

#[godot_api]
impl INode for RocketLauncher {
    fn init(base: Base<Node>) -> Self {
        godot_print!("RocketLauncher initialized");
        Self {
            base,
            design: None,
            cached_events: Vec::new(),
            use_design: false,
        }
    }
}

#[godot_api]
impl RocketLauncher {
    // ==========================================
    // Design Management
    // ==========================================

    /// Sets whether to use dynamic design stages or fixed legacy stages
    #[func]
    pub fn set_use_design(&mut self, use_design: bool) {
        self.use_design = use_design;
    }

    /// Returns whether using dynamic design stages
    #[func]
    pub fn get_use_design(&self) -> bool {
        self.use_design
    }

    /// Clears the current design and reverts to fixed stages
    #[func]
    pub fn clear_design(&mut self) {
        self.design = None;
        self.cached_events.clear();
        self.use_design = false;
    }

    /// Copies the design from a RocketDesigner node
    /// This is the preferred way to set the design
    /// Uses get_design_clone() which properly copies all stages, snapshots, and flaws
    #[func]
    pub fn copy_design_from(&mut self, designer: Gd<crate::rocket_designer::RocketDesigner>) {
        let designer_ref = designer.bind();
        let design = designer_ref.get_design_clone();

        self.cached_events = design.generate_launch_events();
        self.design = Some(design);
        self.use_design = true;
    }

    /// Returns whether a design is loaded
    #[func]
    pub fn has_design(&self) -> bool {
        self.design.is_some()
    }

    /// Returns the design name (or empty string if no design)
    #[func]
    pub fn get_design_name(&self) -> GString {
        match &self.design {
            Some(d) => GString::from(d.name.as_str()),
            None => GString::from(""),
        }
    }

    // ==========================================
    // Stage Information (Dynamic or Fixed)
    // ==========================================

    /// Returns the number of launch stages/events
    /// Uses design events if use_design is true, otherwise fixed stages
    #[func]
    pub fn get_stage_count(&self) -> i32 {
        if self.use_design && !self.cached_events.is_empty() {
            self.cached_events.len() as i32
        } else {
            LaunchStage::all_stages().len() as i32
        }
    }

    /// Returns a description of a specific stage/event by index
    #[func]
    pub fn get_stage_description(&self, index: i32) -> GString {
        if index < 0 {
            return GString::from("Invalid stage index");
        }

        if self.use_design && !self.cached_events.is_empty() {
            if (index as usize) < self.cached_events.len() {
                GString::from(self.cached_events[index as usize].name.as_str())
            } else {
                GString::from("Invalid stage index")
            }
        } else {
            let stages = LaunchStage::all_stages();
            if (index as usize) < stages.len() {
                GString::from(stages[index as usize].description())
            } else {
                GString::from("Invalid stage index")
            }
        }
    }

    /// Returns the BASE failure probability for a specific stage/event by index
    /// Always returns 0.0 since all failures come from flaws
    #[func]
    pub fn get_stage_failure_rate(&self, _index: i32) -> f64 {
        0.0
    }

    /// Returns the TOTAL failure probability for a specific stage/event
    /// All failures come from flaws only
    #[func]
    pub fn get_total_failure_rate(&mut self, index: i32) -> f64 {
        let flaw_rate = self.get_flaw_failure_rate(index);
        let event_name = if index >= 0 && (index as usize) < self.cached_events.len() {
            self.cached_events[index as usize].name.clone()
        } else {
            "unknown".to_string()
        };
        godot_print!("get_total_failure_rate: event={}, flaw_rate={}", event_name, flaw_rate);
        // Cap total failure rate at 95%
        flaw_rate.min(0.95)
    }

    /// Returns the combined flaw failure contribution for a specific event
    /// Uses only the flaws copied from the designer (includes both design and engine flaws)
    #[func]
    pub fn get_flaw_failure_rate(&mut self, index: i32) -> f64 {
        if let Some(design) = &self.design {
            if index >= 0 && (index as usize) < self.cached_events.len() {
                let event = &self.cached_events[index as usize];
                let event_name = &event.name;

                // Get the engine design ID for this stage
                let stage_engine_design_id = design.stages
                    .get(event.rocket_stage)
                    .map(|s| s.engine_design_id);

                // All flaws (design + engine) are in the copied design
                // Don't use self.engine_registry - those flaws weren't fixed by the user
                return design.get_flaw_failure_contribution(event_name, stage_engine_design_id);
            }
        }
        0.0
    }

    /// Returns the full description of a launch event (for dynamic stages only)
    #[func]
    pub fn get_event_description(&self, index: i32) -> GString {
        if index < 0 || !self.use_design {
            return GString::from("");
        }

        if (index as usize) < self.cached_events.len() {
            GString::from(self.cached_events[index as usize].description.as_str())
        } else {
            GString::from("")
        }
    }

    /// Returns which rocket stage (0-indexed) an event belongs to
    #[func]
    pub fn get_event_rocket_stage(&self, index: i32) -> i32 {
        if index < 0 || !self.use_design {
            return -1;
        }

        if (index as usize) < self.cached_events.len() {
            self.cached_events[index as usize].rocket_stage as i32
        } else {
            -1
        }
    }

    // ==========================================
    // Design Information
    // ==========================================

    /// Returns the total delta-v of the loaded design
    #[func]
    pub fn get_design_delta_v(&self) -> f64 {
        match &self.design {
            Some(d) => d.total_delta_v(),
            None => 0.0,
        }
    }

    /// Returns whether the loaded design is sufficient for the mission
    #[func]
    pub fn is_design_sufficient(&self) -> bool {
        match &self.design {
            Some(d) => d.is_sufficient(),
            None => false,
        }
    }

    /// Returns the mission success probability for the loaded design
    #[func]
    pub fn get_design_success_probability(&self) -> f64 {
        match &self.design {
            Some(d) => d.mission_success_probability(),
            None => 0.0,
        }
    }

    // ==========================================
    // Legacy Launch Methods (Fixed Stages)
    // ==========================================

    /// Launches a rocket using fixed stages and returns success status
    /// Returns true if successful, false if failed
    #[func]
    pub fn launch_rocket(&mut self) -> bool {
        let result = LaunchSimulator::simulate_launch();
        matches!(result, LaunchResult::Success)
    }

    /// Launches a rocket using fixed stages and returns the result message
    #[func]
    pub fn launch_rocket_with_message(&mut self) -> GString {
        let result = LaunchSimulator::simulate_launch();
        GString::from(result.message().as_str())
    }

    /// Launches a rocket with stage-by-stage notifications via signals (fixed stages)
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
            &[
                success.to_variant(),
                GString::from(message.as_str()).to_variant(),
            ],
        );
    }

    // ==========================================
    // Signals
    // ==========================================

    /// Signal emitted when entering a new launch stage
    /// Parameters: stage_name (String)
    #[signal]
    fn stage_entered(stage_name: GString);

    /// Signal emitted when the launch completes (success or failure)
    /// Parameters: success (bool), message (String)
    #[signal]
    fn launch_completed(success: bool, message: GString);
}
