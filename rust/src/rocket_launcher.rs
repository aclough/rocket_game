use godot::prelude::*;

use crate::launcher::{LaunchResult, LaunchSimulator, LaunchStage};
use crate::mission_plan::MissionPlan;
use crate::rocket_design::{LaunchEvent, MissionLegEvents, RocketDesign};

/// Godot-accessible rocket launcher node
/// Can use either fixed stages (legacy) or dynamic stages from a rocket design
#[derive(GodotClass)]
#[class(base=Node)]
pub struct RocketLauncher {
    base: Base<Node>,
    /// Optional rocket design for dynamic staging
    design: Option<RocketDesign>,
    /// Cached launch events grouped by mission leg
    cached_leg_events: Vec<MissionLegEvents>,
    /// Flattened events for backward-compatible flat API
    cached_flat_events: Vec<LaunchEvent>,
    /// Mission plan for multi-leg missions
    mission_plan: Option<MissionPlan>,
    /// Whether to use the design (true) or fixed stages (false)
    use_design: bool,
}

impl RocketLauncher {
    /// Rebuild the flat event cache from leg events
    fn rebuild_flat_cache(&mut self) {
        self.cached_flat_events = self.cached_leg_events
            .iter()
            .flat_map(|leg| leg.events.iter().cloned())
            .collect();
    }
}

#[godot_api]
impl INode for RocketLauncher {
    fn init(base: Base<Node>) -> Self {
        godot_print!("RocketLauncher initialized");
        Self {
            base,
            design: None,
            cached_leg_events: Vec::new(),
            cached_flat_events: Vec::new(),
            mission_plan: None,
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
        self.cached_leg_events.clear();
        self.cached_flat_events.clear();
        self.mission_plan = None;
        self.use_design = false;
    }

    /// Copies the design from a RocketDesigner node
    /// This is the preferred way to set the design
    /// Uses get_design_clone() which properly copies all stages, snapshots, and flaws
    /// Note: Does not generate leg events until set_mission_plan() is called.
    /// Falls back to flat launch events for backward compatibility.
    #[func]
    pub fn copy_design_from(&mut self, designer: Gd<crate::rocket_designer::RocketDesigner>) {
        let designer_ref = designer.bind();
        let design = designer_ref.get_design_clone();

        // Generate flat events as fallback (used if set_mission_plan is never called)
        self.cached_flat_events = design.generate_launch_events();
        self.cached_leg_events.clear();
        self.design = Some(design);
        self.use_design = true;
    }

    /// Set the mission plan and generate leg-grouped events.
    /// Must be called after copy_design_from() with the destination location_id.
    #[func]
    pub fn set_mission_plan(&mut self, destination: GString) {
        let dest = destination.to_string();
        if let Some(plan) = MissionPlan::from_shortest_path("earth_surface", &dest) {
            if let Some(design) = &self.design {
                self.cached_leg_events = design.generate_mission_events(&plan);
                self.rebuild_flat_cache();
            }
            self.mission_plan = Some(plan);
        }
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
    // Leg-Based Event API
    // ==========================================

    /// Returns the number of mission legs
    #[func]
    pub fn get_leg_count(&self) -> i32 {
        self.cached_leg_events.len() as i32
    }

    /// Returns the origin location of a leg
    #[func]
    pub fn get_leg_from(&self, leg_index: i32) -> GString {
        if leg_index >= 0 && (leg_index as usize) < self.cached_leg_events.len() {
            GString::from(self.cached_leg_events[leg_index as usize].from.as_str())
        } else {
            GString::from("")
        }
    }

    /// Returns the destination location of a leg
    #[func]
    pub fn get_leg_to(&self, leg_index: i32) -> GString {
        if leg_index >= 0 && (leg_index as usize) < self.cached_leg_events.len() {
            GString::from(self.cached_leg_events[leg_index as usize].to.as_str())
        } else {
            GString::from("")
        }
    }

    /// Returns the number of events in a specific leg
    #[func]
    pub fn get_leg_event_count(&self, leg_index: i32) -> i32 {
        if leg_index >= 0 && (leg_index as usize) < self.cached_leg_events.len() {
            self.cached_leg_events[leg_index as usize].events.len() as i32
        } else {
            0
        }
    }

    /// Returns the name of a specific event within a leg
    #[func]
    pub fn get_leg_event_name(&self, leg_index: i32, event_index: i32) -> GString {
        self.get_leg_event(leg_index, event_index)
            .map(|e| GString::from(e.name.as_str()))
            .unwrap_or_default()
    }

    /// Returns the description of a specific event within a leg
    #[func]
    pub fn get_leg_event_description(&self, leg_index: i32, event_index: i32) -> GString {
        self.get_leg_event(leg_index, event_index)
            .map(|e| GString::from(e.description.as_str()))
            .unwrap_or_default()
    }

    /// Returns the flaw failure rate for a specific event within a leg
    #[func]
    pub fn get_leg_event_failure_rate(&self, leg_index: i32, event_index: i32) -> f64 {
        if let Some(event) = self.get_leg_event(leg_index, event_index) {
            if let Some(design) = &self.design {
                let stage_engine_design_id = design.stages
                    .get(event.rocket_stage)
                    .map(|s| s.engine_design_id);
                let flaw_rate = design.get_flaw_failure_contribution(&event.name, stage_engine_design_id);
                return flaw_rate.min(0.95);
            }
        }
        0.0
    }

    /// Returns the flaw-only failure rate for a specific event within a leg
    #[func]
    pub fn get_leg_event_flaw_failure_rate(&self, leg_index: i32, event_index: i32) -> f64 {
        if let Some(event) = self.get_leg_event(leg_index, event_index) {
            if let Some(design) = &self.design {
                let stage_engine_design_id = design.stages
                    .get(event.rocket_stage)
                    .map(|s| s.engine_design_id);
                return design.get_flaw_failure_contribution(&event.name, stage_engine_design_id);
            }
        }
        0.0
    }

    /// Returns which rocket stage (0-indexed) a leg event belongs to
    #[func]
    pub fn get_leg_event_rocket_stage(&self, leg_index: i32, event_index: i32) -> i32 {
        self.get_leg_event(leg_index, event_index)
            .map(|e| e.rocket_stage as i32)
            .unwrap_or(-1)
    }

    // ==========================================
    // Flat Event API (backward compatibility)
    // ==========================================

    /// Returns the number of launch stages/events
    /// Uses design events if use_design is true, otherwise fixed stages
    #[func]
    pub fn get_stage_count(&self) -> i32 {
        if self.use_design && !self.cached_flat_events.is_empty() {
            self.cached_flat_events.len() as i32
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

        if self.use_design && !self.cached_flat_events.is_empty() {
            if (index as usize) < self.cached_flat_events.len() {
                GString::from(self.cached_flat_events[index as usize].name.as_str())
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
        let event_name = if index >= 0 && (index as usize) < self.cached_flat_events.len() {
            self.cached_flat_events[index as usize].name.clone()
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
            if index >= 0 && (index as usize) < self.cached_flat_events.len() {
                let event = &self.cached_flat_events[index as usize];
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

        if (index as usize) < self.cached_flat_events.len() {
            GString::from(self.cached_flat_events[index as usize].description.as_str())
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

        if (index as usize) < self.cached_flat_events.len() {
            self.cached_flat_events[index as usize].rocket_stage as i32
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
    // Mission Leg Planning
    // ==========================================

    /// Set which stages should burn on a specific leg
    #[func]
    pub fn set_leg_stages_to_burn(&mut self, leg_index: i32, stages: PackedInt32Array) {
        if let Some(plan) = &mut self.mission_plan {
            if leg_index >= 0 && (leg_index as usize) < plan.legs.len() {
                let stage_indices: Vec<usize> = stages.as_slice()
                    .iter()
                    .map(|&s| s as usize)
                    .collect();
                plan.legs[leg_index as usize].stages_to_burn = Some(stage_indices);
            }
        }
    }

    /// Get the stages assigned to burn on a specific leg
    /// Returns auto-assigned stages if no explicit assignment
    #[func]
    pub fn get_leg_stages_to_burn(&self, leg_index: i32) -> PackedInt32Array {
        if let Some(plan) = &self.mission_plan {
            if leg_index >= 0 && (leg_index as usize) < plan.legs.len() {
                let leg = &plan.legs[leg_index as usize];
                if let Some(stages) = &leg.stages_to_burn {
                    return PackedInt32Array::from(
                        stages.iter().map(|&s| s as i32).collect::<Vec<i32>>().as_slice()
                    );
                }
                // Fall back to auto-assigned stages
                if let Some(design) = &self.design {
                    let auto = crate::mission_plan::auto_assign_stages(design, plan);
                    if let Some(leg_stages) = auto.get(leg_index as usize) {
                        return PackedInt32Array::from(
                            leg_stages.iter().map(|&s| s as i32).collect::<Vec<i32>>().as_slice()
                        );
                    }
                }
            }
        }
        PackedInt32Array::new()
    }

    /// Set whether to refuel from a depot before a specific leg
    #[func]
    pub fn set_leg_refuel_before(&mut self, leg_index: i32, refuel: bool) {
        if let Some(plan) = &mut self.mission_plan {
            if leg_index >= 0 && (leg_index as usize) < plan.legs.len() {
                plan.legs[leg_index as usize].refuel_before = refuel;
            }
        }
    }

    /// Get whether refueling is set before a specific leg
    #[func]
    pub fn get_leg_refuel_before(&self, leg_index: i32) -> bool {
        if let Some(plan) = &self.mission_plan {
            if leg_index >= 0 && (leg_index as usize) < plan.legs.len() {
                return plan.legs[leg_index as usize].refuel_before;
            }
        }
        false
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

impl RocketLauncher {
    /// Helper to get a specific event from a leg
    fn get_leg_event(&self, leg_index: i32, event_index: i32) -> Option<&LaunchEvent> {
        if leg_index < 0 || event_index < 0 {
            return None;
        }
        self.cached_leg_events
            .get(leg_index as usize)
            .and_then(|leg| leg.events.get(event_index as usize))
    }

    /// Simulate the mission with infrastructure (depots) for refueling support.
    /// Returns whether the planned mission is feasible.
    pub fn simulate_with_infrastructure(
        &self,
        infrastructure: &std::collections::HashMap<String, crate::fuel_depot::LocationInfrastructure>,
    ) -> bool {
        let plan = match &self.mission_plan {
            Some(p) => p,
            None => return false,
        };
        let design = match &self.design {
            Some(d) => d,
            None => return false,
        };
        crate::mission_plan::simulate_mission_with_plan(design, plan, infrastructure).feasible
    }
}
