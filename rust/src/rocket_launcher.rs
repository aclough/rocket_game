use godot::prelude::*;

use crate::engine::EngineType;
use crate::launcher::{LaunchResult, LaunchSimulator, LaunchStage};
use crate::rocket_design::{LaunchEvent, RocketDesign};
use crate::stage::RocketStage;

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

    /// Sets the rocket design from individual stage data
    /// Call this after configuring a RocketDesigner, passing the stage data
    ///
    /// stages_data format: Array of [engine_type: int, engine_count: int, propellant_mass: float]
    #[func]
    pub fn set_design_from_data(&mut self, payload_mass: f64, stages_data: Array<Array<Variant>>) {
        let mut design = RocketDesign::new();
        design.payload_mass_kg = payload_mass;

        for stage_data in stages_data.iter_shared() {
            if stage_data.len() >= 3 {
                let engine_type_idx = stage_data
                    .get(0)
                    .and_then(|v| v.try_to::<i32>().ok())
                    .unwrap_or(1);
                let engine_count = stage_data
                    .get(1)
                    .and_then(|v| v.try_to::<i32>().ok())
                    .unwrap_or(1);
                let propellant_mass = stage_data
                    .get(2)
                    .and_then(|v| v.try_to::<f64>().ok())
                    .unwrap_or(1000.0);

                let engine_type =
                    EngineType::from_index(engine_type_idx).unwrap_or(EngineType::Kerolox);

                let mut stage = RocketStage::new(engine_type);
                stage.engine_count = engine_count.max(1) as u32;
                stage.propellant_mass_kg = propellant_mass.max(0.0);

                design.stages.push(stage);
            }
        }

        self.cached_events = design.generate_launch_events();
        self.design = Some(design);
        self.use_design = true;
    }

    /// Copies the design from a RocketDesigner node
    /// This is the preferred way to set the design
    /// Also copies flaws so flaw-based failure rates are available
    #[func]
    pub fn copy_design_from(&mut self, designer: Gd<crate::rocket_designer::RocketDesigner>) {
        let designer_ref = designer.bind();

        let mut design = RocketDesign::new();
        design.payload_mass_kg = designer_ref.get_payload_mass();
        design.name = designer_ref.get_design_name().to_string();

        let stage_count = designer_ref.get_stage_count();
        for i in 0..stage_count {
            let engine_type_idx = designer_ref.get_stage_engine_type(i);
            let engine_count = designer_ref.get_stage_engine_count(i);
            let propellant_mass = designer_ref.get_stage_propellant_mass(i);

            let engine_type =
                EngineType::from_index(engine_type_idx).unwrap_or(EngineType::Kerolox);
            let is_booster = designer_ref.is_stage_booster(i);

            let mut stage = RocketStage::new(engine_type);
            stage.engine_count = engine_count.max(1) as u32;
            stage.propellant_mass_kg = propellant_mass.max(0.0);
            stage.is_booster = is_booster;

            design.stages.push(stage);
        }

        // Copy flaws from the designer
        let flaw_count = designer_ref.get_flaw_count();
        for i in 0..flaw_count {
            let name = designer_ref.get_flaw_name(i).to_string();
            let description = designer_ref.get_flaw_description(i).to_string();
            let discovered = designer_ref.is_flaw_discovered(i);
            let fixed = designer_ref.is_flaw_fixed(i);
            let is_engine = designer_ref.is_flaw_engine_type(i);

            // Create a flaw with matching properties
            // We use dummy values for rates since they're already factored in
            let flaw = crate::flaw::Flaw {
                id: (i + 1) as u32,
                flaw_type: if is_engine {
                    crate::flaw::FlawType::Engine
                } else {
                    crate::flaw::FlawType::Design
                },
                name,
                description,
                failure_rate: 0.10, // Will use get_flaw_failure_rate from designer
                testing_modifier: 0.8,
                trigger_event_type: if is_engine {
                    crate::flaw::FlawTrigger::Ignition
                } else {
                    crate::flaw::FlawTrigger::MaxQ
                },
                discovered,
                fixed,
            };
            design.flaws.push(flaw);
        }
        design.flaws_generated = designer_ref.has_flaws_generated();

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
    /// Does not include flaw contributions
    #[func]
    pub fn get_stage_failure_rate(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }

        if self.use_design && !self.cached_events.is_empty() {
            if (index as usize) < self.cached_events.len() {
                self.cached_events[index as usize].failure_rate
            } else {
                0.0
            }
        } else {
            let stages = LaunchStage::all_stages();
            if (index as usize) < stages.len() {
                stages[index as usize].failure_probability()
            } else {
                0.0
            }
        }
    }

    /// Returns the TOTAL failure probability for a specific stage/event
    /// Includes base failure rate PLUS flaw contributions
    #[func]
    pub fn get_total_failure_rate(&self, index: i32) -> f64 {
        let base_rate = self.get_stage_failure_rate(index);

        // Add flaw contributions if we have a design with flaws
        if let Some(design) = &self.design {
            if index >= 0 && (index as usize) < self.cached_events.len() {
                let event_name = &self.cached_events[index as usize].name;
                let flaw_rate = design.get_flaw_failure_contribution(event_name);
                // Cap total failure rate at 95%
                return (base_rate + flaw_rate).min(0.95);
            }
        }

        base_rate
    }

    /// Returns the flaw failure contribution for a specific event
    #[func]
    pub fn get_flaw_failure_rate(&self, index: i32) -> f64 {
        if let Some(design) = &self.design {
            if index >= 0 && (index as usize) < self.cached_events.len() {
                let event_name = &self.cached_events[index as usize].name;
                return design.get_flaw_failure_contribution(event_name);
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
