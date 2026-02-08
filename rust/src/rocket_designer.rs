use godot::prelude::*;

use crate::engine::costs;
use crate::engine_design::{default_snapshot, EngineDesignSnapshot};
use crate::flaw::{check_flaw_trigger, Flaw};
use crate::player_finance::PlayerFinance;
use crate::rocket_design::{RocketDesign, DEFAULT_PAYLOAD_KG, TARGET_DELTA_V_MS};

/// Godot-accessible rocket designer node
/// Allows creating and configuring rocket designs from GDScript
#[derive(GodotClass)]
#[class(base=Node)]
pub struct RocketDesigner {
    design: RocketDesign,
    /// Cached engine snapshots (synced from Company via GameManager)
    engine_snapshots: Vec<EngineDesignSnapshot>,
    /// Engine flaw data (active_flaws, fixed_flaws) per engine design (synced from Company)
    engine_designs_flaws: Vec<(Vec<Flaw>, Vec<Flaw>)>,
    /// Whether engine flaws have been synced from Company
    engine_flaws_synced: bool,
    base: Base<Node>,
    /// Reference to player finances (single source of truth for money)
    finance: Option<Gd<PlayerFinance>>,
}

#[godot_api]
impl INode for RocketDesigner {
    fn init(base: Base<Node>) -> Self {
        godot_print!("RocketDesigner initialized");
        // Initialize with default snapshots
        let snapshots = vec![
            default_snapshot(0),
            default_snapshot(1),
            default_snapshot(2),
        ];
        Self {
            design: RocketDesign::new(),
            engine_snapshots: snapshots,
            engine_designs_flaws: vec![(Vec::new(), Vec::new()); 3],
            engine_flaws_synced: false,
            base,
            finance: None,
        }
    }
}

#[godot_api]
impl RocketDesigner {
    // ==========================================
    // Engine Information
    // ==========================================

    /// Returns the number of available engine types
    #[func]
    pub fn get_engine_type_count(&self) -> i32 {
        self.engine_snapshots.len() as i32
    }

    /// Returns the name of an engine type by index
    #[func]
    pub fn get_engine_name(&self, engine_type: i32) -> GString {
        if engine_type >= 0 && (engine_type as usize) < self.engine_snapshots.len() {
            GString::from(self.engine_snapshots[engine_type as usize].name.as_str())
        } else {
            GString::from("Unknown")
        }
    }

    /// Returns the mass of an engine type in kg
    #[func]
    pub fn get_engine_mass(&self, engine_type: i32) -> f64 {
        if engine_type >= 0 && (engine_type as usize) < self.engine_snapshots.len() {
            self.engine_snapshots[engine_type as usize].mass_kg
        } else {
            0.0
        }
    }

    /// Returns the thrust of an engine type in kN
    #[func]
    pub fn get_engine_thrust(&self, engine_type: i32) -> f64 {
        if engine_type >= 0 && (engine_type as usize) < self.engine_snapshots.len() {
            self.engine_snapshots[engine_type as usize].thrust_kn
        } else {
            0.0
        }
    }

    /// Returns the exhaust velocity of an engine type in m/s
    #[func]
    pub fn get_engine_exhaust_velocity(&self, engine_type: i32) -> f64 {
        if engine_type >= 0 && (engine_type as usize) < self.engine_snapshots.len() {
            self.engine_snapshots[engine_type as usize].exhaust_velocity_ms
        } else {
            0.0
        }
    }

    /// Returns the base failure rate of an engine type
    /// Note: Engine failures now come from flaws only, so this always returns 0.0
    #[func]
    pub fn get_engine_failure_rate(&self, _engine_type: i32) -> f64 {
        // Engine failures are handled through the flaw system
        0.0
    }

    /// Check if an engine type is a solid rocket motor
    #[func]
    pub fn is_engine_type_solid(&self, engine_type: i32) -> bool {
        if engine_type >= 0 && (engine_type as usize) < self.engine_snapshots.len() {
            self.engine_snapshots[engine_type as usize].is_solid
        } else {
            false
        }
    }

    // ==========================================
    // Design Management
    // ==========================================

    /// Resets the design to empty
    #[func]
    pub fn reset_design(&mut self) {
        self.design = RocketDesign::new();
        self.emit_design_changed();
    }

    /// Loads the default two-stage design
    #[func]
    pub fn load_default_design(&mut self) {
        self.design = RocketDesign::default_design();
        self.emit_design_changed();
    }

    /// Sets the design name
    #[func]
    pub fn set_design_name(&mut self, name: GString) {
        self.design.name = name.to_string();
    }

    /// Gets the design name
    #[func]
    pub fn get_design_name(&self) -> GString {
        GString::from(self.design.name.as_str())
    }

    // ==========================================
    // Payload
    // ==========================================

    /// Gets the payload mass in kg
    #[func]
    pub fn get_payload_mass(&self) -> f64 {
        self.design.payload_mass_kg
    }

    /// Sets the payload mass in kg
    #[func]
    pub fn set_payload_mass(&mut self, mass: f64) {
        self.design.payload_mass_kg = mass.max(0.0);
        self.emit_design_changed();
    }

    /// Gets the default payload mass
    #[func]
    pub fn get_default_payload_mass(&self) -> f64 {
        DEFAULT_PAYLOAD_KG
    }

    // ==========================================
    // Stage Management
    // ==========================================

    /// Returns the number of stages in the design
    #[func]
    pub fn get_stage_count(&self) -> i32 {
        self.design.stage_count() as i32
    }

    /// Adds a new stage with the given engine type
    /// Returns the index of the new stage
    #[func]
    pub fn add_stage(&mut self, engine_type: i32) -> i32 {
        let id = if engine_type >= 0 && (engine_type as usize) < self.engine_snapshots.len() {
            engine_type as usize
        } else {
            1 // Default to Kerolox
        };
        let snapshot = self.engine_snapshots[id].clone();
        let index = self.design.add_stage(snapshot) as i32;
        self.emit_design_changed();
        index
    }

    /// Removes a stage by index
    /// Returns true if successful
    #[func]
    pub fn remove_stage(&mut self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        let result = self.design.remove_stage(index as usize).is_some();
        if result {
            self.emit_design_changed();
        }
        result
    }

    /// Moves a stage from one position to another
    #[func]
    pub fn move_stage(&mut self, from: i32, to: i32) {
        if from < 0 || to < 0 {
            return;
        }
        self.design.move_stage(from as usize, to as usize);
        self.emit_design_changed();
    }

    /// Gets the engine type index for a stage
    #[func]
    pub fn get_stage_engine_type(&self, stage_index: i32) -> i32 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return -1;
        }
        self.design.stages[stage_index as usize].engine_design_id as i32
    }

    /// Gets the engine type name for a stage (e.g., "Kerolox", "Hydrolox", "Solid")
    #[func]
    pub fn get_stage_engine_type_name(&self, stage_index: i32) -> GString {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return GString::from("");
        }
        GString::from(self.design.stages[stage_index as usize].engine_snapshot().name.as_str())
    }

    /// Check if a stage uses solid rocket motors
    #[func]
    pub fn is_stage_solid(&self, stage_index: i32) -> bool {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return false;
        }
        self.design.stages[stage_index as usize].is_solid()
    }

    // ==========================================
    // Stage Configuration
    // ==========================================

    /// Gets the number of engines in a stage
    #[func]
    pub fn get_stage_engine_count(&self, stage_index: i32) -> i32 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return 0;
        }
        self.design.stages[stage_index as usize].engine_count as i32
    }

    /// Sets the number of engines in a stage
    /// For solid motors, this also updates the propellant mass
    #[func]
    pub fn set_stage_engine_count(&mut self, stage_index: i32, count: i32) {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return;
        }
        // Use set_engine_count which handles solid propellant updates
        self.design.stages[stage_index as usize].set_engine_count(count.max(1) as u32);
        self.emit_design_changed();
    }

    /// Gets the mass fraction for a stage (propellant / total mass including payload above)
    #[func]
    pub fn get_stage_mass_fraction(&self, stage_index: i32) -> f64 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return 0.0;
        }
        self.design.stage_mass_fraction(stage_index as usize)
    }

    /// Sets the mass fraction for a stage (updates propellant mass)
    #[func]
    pub fn set_stage_mass_fraction(&mut self, stage_index: i32, fraction: f64) {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return;
        }
        let fraction = fraction.clamp(0.1, 0.95);
        self.design.set_stage_mass_fraction(stage_index as usize, fraction);
        self.emit_design_changed();
    }

    /// Gets the propellant mass for a stage in kg
    #[func]
    pub fn get_stage_propellant_mass(&self, stage_index: i32) -> f64 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return 0.0;
        }
        self.design.stages[stage_index as usize].propellant_mass_kg
    }

    /// Sets the propellant mass for a stage in kg
    #[func]
    pub fn set_stage_propellant_mass(&mut self, stage_index: i32, mass: f64) {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return;
        }
        self.design.stages[stage_index as usize].propellant_mass_kg = mass.max(0.0);
        self.emit_design_changed();
    }

    /// Gets the dry mass (engines only) for a stage in kg
    #[func]
    pub fn get_stage_dry_mass(&self, stage_index: i32) -> f64 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return 0.0;
        }
        self.design.stages[stage_index as usize].dry_mass_kg()
    }

    /// Gets the wet mass (engines + propellant) for a stage in kg
    #[func]
    pub fn get_stage_wet_mass(&self, stage_index: i32) -> f64 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return 0.0;
        }
        self.design.stages[stage_index as usize].wet_mass_kg()
    }

    /// Gets the total thrust for a stage in kN
    #[func]
    pub fn get_stage_thrust(&self, stage_index: i32) -> f64 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return 0.0;
        }
        self.design.stages[stage_index as usize].total_thrust_kn()
    }

    // ==========================================
    // Delta-V Calculations
    // ==========================================

    /// Gets the delta-v contribution of a stage in m/s
    #[func]
    pub fn get_stage_delta_v(&self, stage_index: i32) -> f64 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return 0.0;
        }
        self.design.stage_delta_v(stage_index as usize)
    }

    /// Gets the total delta-v of the rocket in m/s
    #[func]
    pub fn get_total_delta_v(&self) -> f64 {
        self.design.total_delta_v()
    }

    /// Gets the target delta-v for the current mission in m/s
    #[func]
    pub fn get_target_delta_v(&self) -> f64 {
        self.design.target_delta_v()
    }

    /// Sets the target delta-v for the current mission in m/s
    #[func]
    pub fn set_target_delta_v(&mut self, delta_v: f64) {
        self.design.set_target_delta_v(delta_v);
        self.emit_design_changed();
    }

    /// Gets the default target delta-v (LEO)
    #[func]
    pub fn get_default_target_delta_v(&self) -> f64 {
        TARGET_DELTA_V_MS
    }

    /// Gets the delta-v margin (positive = excess, negative = shortfall)
    #[func]
    pub fn get_delta_v_margin(&self) -> f64 {
        self.design.delta_v_margin()
    }

    /// Gets the effective delta-v as a percentage of target (100 = exactly sufficient)
    #[func]
    pub fn get_delta_v_percentage(&self) -> f64 {
        self.design.delta_v_percentage()
    }

    /// Gets the ideal delta-v as a percentage of target (ignoring gravity losses)
    #[func]
    pub fn get_ideal_delta_v_percentage(&self) -> f64 {
        self.design.ideal_delta_v_percentage()
    }

    // ==========================================
    // TWR and Gravity Loss
    // ==========================================

    /// Gets the initial TWR for a stage (thrust / weight at ignition)
    #[func]
    pub fn get_stage_twr(&self, stage_index: i32) -> f64 {
        if stage_index < 0 {
            return 0.0;
        }
        self.design.stage_twr(stage_index as usize)
    }

    /// Gets the gravity loss coefficient for a stage (0.0 to 1.0)
    /// Higher values mean more of the burn is fighting gravity
    #[func]
    pub fn get_stage_gravity_coefficient(&self, stage_index: i32) -> f64 {
        if stage_index < 0 {
            return 0.0;
        }
        self.design.stage_gravity_coefficient(stage_index as usize)
    }

    /// Gets the gravity loss for a stage in m/s
    #[func]
    pub fn get_stage_gravity_loss(&self, stage_index: i32) -> f64 {
        if stage_index < 0 {
            return 0.0;
        }
        self.design.stage_gravity_loss(stage_index as usize)
    }

    /// Gets the effective delta-v for a stage (after gravity losses) in m/s
    #[func]
    pub fn get_stage_effective_delta_v(&self, stage_index: i32) -> f64 {
        if stage_index < 0 {
            return 0.0;
        }
        self.design.stage_effective_delta_v(stage_index as usize)
    }

    /// Gets the total effective delta-v of the rocket (after gravity losses) in m/s
    #[func]
    pub fn get_total_effective_delta_v(&self) -> f64 {
        self.design.total_effective_delta_v()
    }

    /// Gets the total gravity loss across all stages in m/s
    #[func]
    pub fn get_total_gravity_loss(&self) -> f64 {
        self.design.total_gravity_loss()
    }

    /// Gets the overall gravity efficiency (effective_dv / ideal_dv)
    #[func]
    pub fn get_gravity_efficiency(&self) -> f64 {
        self.design.gravity_efficiency()
    }

    /// Returns true if the design has sufficient delta-v for the mission
    #[func]
    pub fn is_design_sufficient(&self) -> bool {
        self.design.is_sufficient()
    }

    /// Returns true if the design is valid (has at least one stage)
    #[func]
    pub fn is_design_valid(&self) -> bool {
        self.design.is_valid()
    }

    // ==========================================
    // Mass Calculations
    // ==========================================

    /// Gets the total wet mass of the rocket in kg
    #[func]
    pub fn get_total_wet_mass(&self) -> f64 {
        self.design.total_wet_mass_kg()
    }

    /// Gets the total dry mass of the rocket in kg
    #[func]
    pub fn get_total_dry_mass(&self) -> f64 {
        self.design.total_dry_mass_kg()
    }

    /// Gets the thrust-to-weight ratio at liftoff
    #[func]
    pub fn get_liftoff_twr(&self) -> f64 {
        self.design.liftoff_twr()
    }

    // ==========================================
    // Mission Success
    // ==========================================

    /// Gets the overall mission success probability (0.0 to 1.0)
    #[func]
    pub fn get_mission_success_probability(&self) -> f64 {
        self.design.mission_success_probability()
    }

    /// Gets the base ignition failure rate for a stage
    /// Note: Engine failures now come from flaws only, so this always returns 0.0
    #[func]
    pub fn get_stage_ignition_failure_rate(&self, _stage_index: i32) -> f64 {
        // Engine failures are handled through the flaw system
        0.0
    }

    // ==========================================
    // Launch Events
    // ==========================================

    /// Gets the number of launch events for the current design
    #[func]
    pub fn get_launch_event_count(&self) -> i32 {
        self.design.generate_launch_events().len() as i32
    }

    /// Gets the name of a launch event by index
    #[func]
    pub fn get_launch_event_name(&self, event_index: i32) -> GString {
        let events = self.design.generate_launch_events();
        if event_index < 0 || event_index as usize >= events.len() {
            return GString::from("");
        }
        GString::from(events[event_index as usize].name.as_str())
    }

    /// Gets the description of a launch event by index
    #[func]
    pub fn get_launch_event_description(&self, event_index: i32) -> GString {
        let events = self.design.generate_launch_events();
        if event_index < 0 || event_index as usize >= events.len() {
            return GString::from("");
        }
        GString::from(events[event_index as usize].description.as_str())
    }

    /// Gets the base failure rate of a launch event by index
    /// Always returns 0.0 since all failures come from flaws
    #[func]
    pub fn get_launch_event_failure_rate(&self, _event_index: i32) -> f64 {
        0.0
    }

    /// Gets the rocket stage index for a launch event
    #[func]
    pub fn get_launch_event_stage(&self, event_index: i32) -> i32 {
        let events = self.design.generate_launch_events();
        if event_index < 0 || event_index as usize >= events.len() {
            return -1;
        }
        events[event_index as usize].rocket_stage as i32
    }

    // ==========================================
    // Budget & Cost
    // ==========================================

    /// Gets the starting budget in dollars
    #[func]
    pub fn get_starting_budget(&self) -> f64 {
        RocketDesign::starting_budget()
    }

    /// Gets the cost of a single engine of the given type in dollars
    #[func]
    pub fn get_engine_cost(&self, engine_type: i32) -> f64 {
        if engine_type >= 0 && (engine_type as usize) < self.engine_snapshots.len() {
            self.engine_snapshots[engine_type as usize].base_cost
        } else {
            0.0
        }
    }

    /// Gets the propellant density for an engine type in kg/m³
    #[func]
    pub fn get_propellant_density(&self, engine_type: i32) -> f64 {
        if engine_type >= 0 && (engine_type as usize) < self.engine_snapshots.len() {
            self.engine_snapshots[engine_type as usize].propellant_density
        } else {
            0.0
        }
    }

    /// Gets the fixed material cost per stage (assembly hardware)
    #[func]
    pub fn get_stage_overhead_cost(&self) -> f64 {
        crate::resources::stage_assembly_cost()
    }

    /// Gets the fixed material cost per rocket (integration)
    #[func]
    pub fn get_rocket_overhead_cost(&self) -> f64 {
        crate::resources::rocket_integration_cost()
    }

    /// Gets the tank volume for a stage in cubic meters
    #[func]
    pub fn get_stage_tank_volume(&self, stage_index: i32) -> f64 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return 0.0;
        }
        self.design.stages[stage_index as usize].tank_volume_m3()
    }

    /// Gets the engine cost for a stage in dollars
    #[func]
    pub fn get_stage_engine_cost(&self, stage_index: i32) -> f64 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return 0.0;
        }
        self.design.stages[stage_index as usize].engine_cost()
    }

    /// Gets the tank cost for a stage in dollars
    #[func]
    pub fn get_stage_tank_cost(&self, stage_index: i32) -> f64 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return 0.0;
        }
        self.design.stages[stage_index as usize].tank_cost()
    }

    /// Gets the total cost of a stage in dollars (engines + tanks + overhead)
    #[func]
    pub fn get_stage_cost(&self, stage_index: i32) -> f64 {
        if stage_index < 0 {
            return 0.0;
        }
        self.design.stage_cost(stage_index as usize)
    }

    /// Gets the total cost of all stages in dollars
    #[func]
    pub fn get_total_stages_cost(&self) -> f64 {
        self.design.total_stages_cost()
    }

    /// Gets the total cost of the rocket in dollars (all stages + rocket overhead)
    #[func]
    pub fn get_total_cost(&self) -> f64 {
        self.design.total_cost()
    }

    /// Gets the remaining budget in dollars (starting budget - total cost)
    #[func]
    pub fn get_remaining_budget(&self) -> f64 {
        if let Some(ref finance) = self.finance {
            // Remaining = current money - rocket cost
            finance.bind().get_money() - self.design.total_cost()
        } else {
            self.design.remaining_budget()
        }
    }

    /// Returns true if the design is within budget
    #[func]
    pub fn is_within_budget(&self) -> bool {
        self.design.is_within_budget()
    }

    /// Returns true if the design is launchable (sufficient delta-v AND within budget)
    #[func]
    pub fn is_launchable(&self) -> bool {
        self.design.is_launchable()
    }

    // ==========================================
    // Booster Management
    // ==========================================

    /// Check if a stage is a booster (fires in parallel with stage below it)
    #[func]
    pub fn is_stage_booster(&self, stage_index: i32) -> bool {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return false;
        }
        self.design.stages[stage_index as usize].is_booster
    }

    /// Set whether a stage is a booster
    /// Returns true if successful, false if validation failed
    #[func]
    pub fn set_stage_booster(&mut self, stage_index: i32, is_booster: bool) -> bool {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return false;
        }

        if is_booster {
            // Validate before setting
            if !self.design.can_be_booster(stage_index as usize) {
                return false;
            }
        }

        self.design.stages[stage_index as usize].is_booster = is_booster;
        self.emit_design_changed();
        true
    }

    /// Check if a stage can be made a booster
    /// Returns true if the stage meets booster requirements
    #[func]
    pub fn can_be_booster(&self, stage_index: i32) -> bool {
        if stage_index < 0 {
            return false;
        }
        self.design.can_be_booster(stage_index as usize)
    }

    /// Get the validation error for making a stage a booster
    /// Returns empty string if valid, or an error message
    #[func]
    pub fn get_booster_validation_error(&self, stage_index: i32) -> GString {
        if stage_index < 0 {
            return GString::from("Invalid stage index");
        }
        match self.design.get_booster_validation_error(stage_index as usize) {
            Some(err) => GString::from(err.as_str()),
            None => GString::from(""),
        }
    }

    /// Get the combined TWR during booster burn for a core stage
    /// Returns 0 if the stage has no boosters or is itself a booster
    #[func]
    pub fn get_combined_twr_during_boost(&self, stage_index: i32) -> f64 {
        if stage_index < 0 {
            return 0.0;
        }
        self.design
            .get_combined_twr_during_boost(stage_index as usize)
            .unwrap_or(0.0)
    }

    /// Check if a stage has boosters attached to it
    #[func]
    pub fn stage_has_boosters(&self, stage_index: i32) -> bool {
        if stage_index < 0 {
            return false;
        }
        let groups = self.design.find_booster_groups();
        for group in &groups {
            if group.core_stage_index == stage_index as usize && !group.booster_indices.is_empty() {
                return true;
            }
        }
        false
    }

    /// Get the number of boosters attached to a stage
    #[func]
    pub fn get_booster_count(&self, stage_index: i32) -> i32 {
        if stage_index < 0 {
            return 0;
        }
        let groups = self.design.find_booster_groups();
        for group in &groups {
            if group.core_stage_index == stage_index as usize {
                return group.booster_indices.len() as i32;
            }
        }
        0
    }

    /// Validate all booster configurations
    /// Returns empty string if valid, or an error message
    #[func]
    pub fn validate_boosters(&self) -> GString {
        match self.design.validate_boosters() {
            Ok(()) => GString::from(""),
            Err(err) => GString::from(err.as_str()),
        }
    }

    // ==========================================
    // Flaw System
    // ==========================================

    /// Ensure flaws are generated for this design
    /// Call this before testing or launching
    /// This will automatically reset flaws if the design has changed significantly
    #[func]
    pub fn ensure_flaws_generated(&mut self) {
        // Debug: log flaw state before
        godot_print!(
            "ensure_flaws_generated: BEFORE - active_flaws={}, fixed_flaws={}, flaws_generated={}",
            self.design.active_flaws.len(),
            self.design.fixed_flaws.len(),
            self.design.flaws_generated
        );
        godot_print!(
            "ensure_flaws_generated: signature={}, stored={}",
            self.design.compute_design_signature(),
            self.design.get_flaw_design_signature()
        );

        // First check if design has changed since flaws were generated
        let was_reset = self.design.check_and_reset_flaws_if_changed();
        if was_reset {
            godot_print!("ensure_flaws_generated: FLAWS WERE RESET!");
        }

        // Then generate design flaws if needed (engine flaws are synced from Company)
        let mut generator = crate::flaw::FlawGenerator::new();
        self.design.generate_flaws(&mut generator);

        // Debug: log flaw state after
        godot_print!(
            "ensure_flaws_generated: AFTER - active_flaws={}, fixed_flaws={}",
            self.design.active_flaws.len(),
            self.design.fixed_flaws.len()
        );
    }

    /// Check if flaws have been generated
    #[func]
    pub fn has_flaws_generated(&self) -> bool {
        self.design.has_flaws_generated()
    }

    /// Check if the design has changed since flaws were generated
    #[func]
    pub fn design_changed_since_flaws(&self) -> bool {
        self.design.design_changed_since_flaws()
    }

    /// Manually reset flaws (e.g., when starting a new design)
    #[func]
    pub fn reset_flaws(&mut self) {
        self.design.reset_flaws();
        self.emit_design_changed();
    }

    // ==========================================
    // Combined Flaw Access Helpers
    // ==========================================
    // Flaws are stored in two places:
    // - Design flaws: on self.design (active_flaws + fixed_flaws)
    // - Engine flaws: on self.engine_designs_flaws for each engine design
    // The Godot API presents these as a unified list.

    /// Get the total count of all flaws (design + engine)
    fn get_total_flaw_count(&self) -> usize {
        let design_count = self.design.get_flaw_count();
        let engine_count: usize = self.design.get_unique_engine_design_ids()
            .iter()
            .filter(|&&id| id < self.engine_designs_flaws.len())
            .map(|&id| {
                let (ref active, ref fixed) = self.engine_designs_flaws[id];
                active.len() + fixed.len()
            })
            .sum();
        design_count + engine_count
    }

    /// Get a flaw by combined index (design flaws first, then engine flaws)
    /// Returns (flaw_ref, is_engine_flaw, engine_design_id_if_engine)
    fn get_flaw_by_combined_index(&self, index: usize) -> Option<(&crate::flaw::Flaw, bool, Option<usize>)> {
        let design_count = self.design.get_flaw_count();

        if index < design_count {
            // It's a design flaw
            return self.design.get_flaw(index).map(|f| (f, false, None));
        }

        // It's an engine flaw - find which engine design
        let mut offset = design_count;
        for id in self.design.get_unique_engine_design_ids() {
            if id < self.engine_designs_flaws.len() {
                let (ref active, ref fixed) = self.engine_designs_flaws[id];
                let et_flaw_count = active.len() + fixed.len();
                if index < offset + et_flaw_count {
                    let local_idx = index - offset;
                    let flaw = if local_idx < active.len() {
                        &active[local_idx]
                    } else {
                        &fixed[local_idx - active.len()]
                    };
                    return Some((flaw, true, Some(id)));
                }
                offset += et_flaw_count;
            }
        }
        None
    }

    /// Get the total number of flaws
    #[func]
    pub fn get_flaw_count(&self) -> i32 {
        self.get_total_flaw_count() as i32
    }

    /// Get the number of discovered flaws (design + engine)
    #[func]
    pub fn get_discovered_flaw_count(&self) -> i32 {
        let design_discovered = self.design.get_discovered_flaw_count();
        let engine_discovered: usize = self.design.get_unique_engine_design_ids()
            .iter()
            .filter(|&&id| id < self.engine_designs_flaws.len())
            .map(|&id| {
                self.engine_designs_flaws[id].0.iter().filter(|f| f.discovered).count()
            })
            .sum();
        (design_discovered + engine_discovered) as i32
    }

    /// Get the number of fixed flaws (design + engine)
    #[func]
    pub fn get_fixed_flaw_count(&self) -> i32 {
        let design_fixed = self.design.get_fixed_flaw_count();
        let engine_fixed: usize = self.design.get_unique_engine_design_ids()
            .iter()
            .filter(|&&id| id < self.engine_designs_flaws.len())
            .map(|&id| self.engine_designs_flaws[id].1.len())
            .sum();
        (design_fixed + engine_fixed) as i32
    }

    /// Get the number of unknown (undiscovered, unfixed) flaws (design + engine)
    #[func]
    pub fn get_unknown_flaw_count(&self) -> i32 {
        let design_unknown = self.design.get_unknown_flaw_count();
        let engine_unknown: usize = self.design.get_unique_engine_design_ids()
            .iter()
            .filter(|&&id| id < self.engine_designs_flaws.len())
            .map(|&id| {
                self.engine_designs_flaws[id].0.iter().filter(|f| !f.discovered).count()
            })
            .sum();
        (design_unknown + engine_unknown) as i32
    }

    /// Get the name of a flaw by index
    #[func]
    pub fn get_flaw_name(&self, index: i32) -> GString {
        if index < 0 {
            return GString::from("");
        }
        match self.get_flaw_by_combined_index(index as usize) {
            Some((flaw, _, _)) => GString::from(flaw.name.as_str()),
            None => GString::from(""),
        }
    }

    /// Get the description of a flaw by index
    #[func]
    pub fn get_flaw_description(&self, index: i32) -> GString {
        if index < 0 {
            return GString::from("");
        }
        match self.get_flaw_by_combined_index(index as usize) {
            Some((flaw, _, _)) => GString::from(flaw.description.as_str()),
            None => GString::from(""),
        }
    }

    /// Check if a flaw is discovered
    #[func]
    pub fn is_flaw_discovered(&self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        match self.get_flaw_by_combined_index(index as usize) {
            Some((flaw, _, _)) => flaw.discovered,
            None => false,
        }
    }

    /// Check if a flaw is fixed
    #[func]
    pub fn is_flaw_fixed(&self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        match self.get_flaw_by_combined_index(index as usize) {
            Some((flaw, _, _)) => flaw.fixed,
            None => false,
        }
    }

    /// Check if a flaw is an engine type (vs design type)
    #[func]
    pub fn is_flaw_engine_type(&self, index: i32) -> bool {
        if index < 0 {
            return false;
        }
        match self.get_flaw_by_combined_index(index as usize) {
            Some((flaw, _, _)) => flaw.flaw_type == crate::flaw::FlawType::Engine,
            None => false,
        }
    }

    /// Get the trigger type index for a flaw (0=Ignition, 1=Liftoff, 2=MaxQ, 3=Separation, 4=PayloadRelease)
    #[func]
    pub fn get_flaw_trigger_type(&self, index: i32) -> i32 {
        if index < 0 {
            return -1;
        }
        match self.get_flaw_by_combined_index(index as usize) {
            Some((flaw, _, _)) => flaw.trigger_event_type.to_index(),
            None => -1,
        }
    }

    /// Run an engine test - tests engine flaws for all engine types in the design
    /// Returns array of discovered flaw names
    #[func]
    pub fn run_engine_test(&mut self) -> Array<GString> {
        // Check and deduct cost via PlayerFinance
        if !self.deduct_cost(costs::ENGINE_TEST_COST) {
            return Array::new();
        }

        // Test engine flaws for each engine design used
        let mut all_discovered = Vec::new();
        for id in self.design.get_unique_engine_design_ids() {
            if id < self.engine_designs_flaws.len() {
                let discovered = crate::flaw::run_engine_test_for_type(
                    &mut self.engine_designs_flaws[id].0,
                    id,
                );
                all_discovered.extend(discovered);
            }
        }

        let mut result = Array::new();
        for name in all_discovered {
            result.push(&GString::from(name.as_str()));
        }
        self.emit_design_changed();
        result
    }

    /// Run an engine test for a specific engine design - returns array of discovered flaw names
    #[func]
    pub fn run_engine_test_for_type(&mut self, engine_type: i32) -> Array<GString> {
        // Check and deduct cost via PlayerFinance
        if !self.deduct_cost(costs::ENGINE_TEST_COST) {
            return Array::new();
        }

        // Test engine flaws for this specific engine design
        let mut result = Array::new();
        let id = engine_type as usize;
        if engine_type >= 0 && id < self.engine_designs_flaws.len() {
            let discovered = crate::flaw::run_engine_test_for_type(
                &mut self.engine_designs_flaws[id].0,
                id,
            );
            for name in discovered {
                result.push(&GString::from(name.as_str()));
            }
        }

        self.emit_design_changed();
        result
    }

    /// Get the list of unique engine types in the design
    /// Returns array of engine type indices (engine_design_ids as i32)
    #[func]
    pub fn get_unique_engine_types(&self) -> Array<i32> {
        let ids = self.design.get_unique_engine_design_ids();
        let mut result = Array::new();
        for id in ids {
            result.push(id as i32);
        }
        result
    }

    /// Get the engine design id for a flaw (returns -1 if not an engine flaw or invalid index)
    #[func]
    pub fn get_flaw_engine_design_id(&self, index: i32) -> i32 {
        if index < 0 {
            return -1;
        }
        match self.get_flaw_by_combined_index(index as usize) {
            Some((flaw, _, engine_design_id)) => {
                // For engine flaws, return the engine design id from context or from the flaw
                if let Some(id) = engine_design_id {
                    id as i32
                } else {
                    flaw.engine_design_id.map(|id| id as i32).unwrap_or(-1)
                }
            }
            None => -1,
        }
    }

    /// Check if any flaw triggers at a given event
    /// Checks both design flaws and engine flaws.
    /// stage_engine_design_id: the engine design id of the stage that failed (-1 if unknown)
    /// Returns the flaw ID if a flaw caused failure, or -1 if no flaw triggered
    #[func]
    pub fn check_flaw_trigger(&mut self, event_name: GString, stage_engine_design_id: i32) -> i32 {
        let event = event_name.to_string();
        let engine_id = if stage_engine_design_id >= 0 {
            Some(stage_engine_design_id as usize)
        } else {
            None
        };

        // First check design flaws
        if let Some(id) = self.design.check_flaw_trigger(&event, engine_id) {
            return id as i32;
        }

        // Then check engine flaws (if we have an engine design id)
        if let Some(idx) = engine_id {
            if idx < self.engine_designs_flaws.len() {
                let active = &self.engine_designs_flaws[idx].0;
                if let Some(id) = check_flaw_trigger(active, &event, engine_id) {
                    return id as i32;
                }
            }
        }

        -1
    }

    /// Mark a flaw as discovered by its ID (used when failure occurs)
    /// Checks both design flaws and engine flaws.
    /// Returns the flaw name if found, or empty string
    #[func]
    pub fn discover_flaw_by_id(&mut self, flaw_id: i32) -> GString {
        if flaw_id < 0 {
            return GString::from("");
        }

        // First try design flaws
        if let Some(name) = self.design.discover_flaw_by_id(flaw_id as u32) {
            self.emit_design_changed();
            return GString::from(name.as_str());
        }

        // Then try engine flaws
        for (_, (ref mut active, _)) in self.engine_designs_flaws.iter_mut().enumerate() {
            for flaw in active.iter_mut() {
                if flaw.id == flaw_id as u32 && !flaw.discovered {
                    flaw.discovered = true;
                    let name = GString::from(flaw.name.as_str());
                    self.emit_design_changed();
                    return name;
                }
            }
        }

        GString::from("")
    }

    /// Get the failure rate for a flaw by index
    #[func]
    pub fn get_flaw_failure_rate(&self, index: i32) -> f64 {
        if index < 0 {
            return 0.0;
        }
        match self.get_flaw_by_combined_index(index as usize) {
            Some((flaw, _, _)) => flaw.failure_rate,
            None => 0.0
        }
    }

    /// Run a rocket test - returns array of discovered flaw names
    #[func]
    pub fn run_rocket_test(&mut self) -> Array<GString> {
        // Check and deduct cost via PlayerFinance
        if !self.deduct_cost(costs::ROCKET_TEST_COST) {
            return Array::new();
        }

        // Run the test (this also updates design.testing_spent but we ignore that)
        let discovered = self.design.run_rocket_test_no_cost();
        let mut result = Array::new();
        for name in discovered {
            result.push(&GString::from(name.as_str()));
        }
        self.emit_design_changed();
        result
    }

    /// Fix a flaw by combined index - returns true if successful
    /// Routes to design flaws or engine registry as appropriate
    #[func]
    pub fn fix_flaw(&mut self, index: i32) -> bool {
        if index < 0 {
            return false;
        }

        // Check budget first via PlayerFinance
        if !self.can_afford(costs::FLAW_FIX_COST) {
            return false;
        }

        let design_flaw_count = self.design.get_flaw_count();

        if (index as usize) < design_flaw_count {
            // It's a design flaw - fix it on the design (without cost handling)
            let result = self.design.fix_flaw_by_index_no_cost(index as usize);
            if result {
                self.deduct_cost(costs::FLAW_FIX_COST);
                self.emit_design_changed();
            }
            return result;
        }

        // It's an engine flaw - find which engine design and fix it there
        let mut offset = design_flaw_count;
        for id in self.design.get_unique_engine_design_ids() {
            if id < self.engine_designs_flaws.len() {
                let (ref active, ref fixed) = self.engine_designs_flaws[id];
                let et_active_count = active.len();

                if (index as usize) < offset + et_active_count {
                    // Found the right engine design - get the flaw id and fix it
                    let local_idx = (index as usize) - offset;
                    if local_idx < active.len() && active[local_idx].discovered {
                        let flaw_id = active[local_idx].id;
                        // Fix: remove from active, add to fixed
                        let (ref mut active_mut, ref mut fixed_mut) = self.engine_designs_flaws[id];
                        if let Some(pos) = active_mut.iter().position(|f| f.id == flaw_id && f.discovered) {
                            let mut flaw = active_mut.remove(pos);
                            flaw.fixed = true;
                            fixed_mut.push(flaw);
                            self.deduct_cost(costs::FLAW_FIX_COST);
                            self.emit_design_changed();
                            return true;
                        }
                    }
                    return false;
                }
                // Skip fixed flaws in the count since we only fix active ones
                offset += et_active_count + fixed.len();
            }
        }

        false
    }

    /// Get the cost of an engine test
    #[func]
    pub fn get_engine_test_cost(&self) -> f64 {
        RocketDesign::engine_test_cost()
    }

    /// Get the cost of a rocket test
    #[func]
    pub fn get_rocket_test_cost(&self) -> f64 {
        RocketDesign::rocket_test_cost()
    }

    /// Get the cost to fix a flaw
    #[func]
    pub fn get_flaw_fix_cost(&self) -> f64 {
        RocketDesign::flaw_fix_cost()
    }

    /// Check if we can afford an engine test
    #[func]
    pub fn can_afford_engine_test(&self) -> bool {
        self.can_afford(costs::ENGINE_TEST_COST)
    }

    /// Check if we can afford a rocket test
    #[func]
    pub fn can_afford_rocket_test(&self) -> bool {
        self.can_afford(costs::ROCKET_TEST_COST)
    }

    /// Check if we can afford to fix a flaw
    #[func]
    pub fn can_afford_fix(&self) -> bool {
        self.can_afford(costs::FLAW_FIX_COST)
    }

    /// Get the estimated success rate including flaws
    /// Combines design flaws and engine flaws from the registry
    #[func]
    pub fn get_estimated_success_rate(&self) -> f64 {
        let base_success = self.design.mission_success_probability();

        // Start with design flaw success rate
        let design_flaw_success: f64 = self.design.active_flaws
            .iter()
            .filter(|f| !f.fixed)
            .map(|f| 1.0 - f.failure_rate)
            .product();

        // Multiply by engine flaw success rates for each engine design used
        let engine_flaw_success: f64 = self.design.get_unique_engine_design_ids()
            .iter()
            .filter(|&&id| id < self.engine_designs_flaws.len())
            .map(|&id| {
                self.engine_designs_flaws[id].0
                    .iter()
                    .filter(|f| !f.fixed)
                    .map(|f| 1.0 - f.failure_rate)
                    .product::<f64>()
            })
            .product();

        base_success * design_flaw_success * engine_flaw_success
    }

    /// Get the estimated range of unknown flaws (min, max)
    #[func]
    pub fn get_estimated_unknown_flaw_range(&self) -> Array<i32> {
        let (min, max) = self.design.estimate_unknown_flaws();
        let mut result = Array::new();
        result.push(min as i32);
        result.push(max as i32);
        result
    }

    /// Get total testing spent
    #[func]
    pub fn get_testing_spent(&self) -> f64 {
        self.design.get_testing_spent()
    }

    // ==========================================
    // Signals
    // ==========================================

    /// Signal emitted when the design changes
    #[signal]
    fn design_changed();

    /// Helper to emit design_changed signal
    fn emit_design_changed(&mut self) {
        self.base_mut().emit_signal("design_changed", &[]);
    }

    /// Get a clone of the internal design (for syncing with GameState)
    /// This merges engine flaws from the synced data into the design clone
    /// so that engine flaw state is preserved when saving
    pub fn get_design_clone(&self) -> crate::rocket_design::RocketDesign {
        let mut design = self.design.clone();

        // Debug: log what we're cloning
        godot_print!(
            "get_design_clone: BEFORE merge - active_flaws={}, fixed_flaws={}",
            design.active_flaws.len(),
            design.fixed_flaws.len()
        );

        // Merge engine flaws into the design clone
        // This ensures engine flaws are saved with the design
        for id in self.design.get_unique_engine_design_ids() {
            if id < self.engine_designs_flaws.len() {
                let (ref active, ref fixed) = self.engine_designs_flaws[id];

                // Add active engine flaws that aren't already in the design
                for flaw in active {
                    if !design.active_flaws.iter().any(|f| f.id == flaw.id) {
                        design.active_flaws.push(flaw.clone());
                    }
                }

                // Add fixed engine flaws that aren't already in the design
                for flaw in fixed {
                    if !design.fixed_flaws.iter().any(|f| f.id == flaw.id) {
                        design.fixed_flaws.push(flaw.clone());
                    }
                }
            }
        }

        // Debug: log final state
        godot_print!(
            "get_design_clone: AFTER merge - active_flaws={}, fixed_flaws={}",
            design.active_flaws.len(),
            design.fixed_flaws.len()
        );

        design
    }

    /// Set the internal design from an external source
    /// This also restores engine flaws from the design to the synced flaw data
    pub fn set_design(&mut self, design: crate::rocket_design::RocketDesign) {
        // Debug: log incoming design
        godot_print!(
            "set_design: INCOMING - active_flaws={}, fixed_flaws={}, flaws_generated={}",
            design.active_flaws.len(),
            design.fixed_flaws.len(),
            design.flaws_generated
        );
        for (i, flaw) in design.active_flaws.iter().enumerate() {
            godot_print!("  active[{}]: {} type={:?}", i, flaw.name, flaw.flaw_type);
        }
        for (i, flaw) in design.fixed_flaws.iter().enumerate() {
            godot_print!("  fixed[{}]: {} type={:?}", i, flaw.name, flaw.flaw_type);
        }

        // Extract engine flaws from the design
        let engine_active_flaws: Vec<_> = design
            .active_flaws
            .iter()
            .filter(|f| f.flaw_type == crate::flaw::FlawType::Engine)
            .cloned()
            .collect();
        let engine_fixed_flaws: Vec<_> = design
            .fixed_flaws
            .iter()
            .filter(|f| f.flaw_type == crate::flaw::FlawType::Engine)
            .cloned()
            .collect();

        godot_print!(
            "set_design: extracted engine flaws - active={}, fixed={}",
            engine_active_flaws.len(),
            engine_fixed_flaws.len()
        );

        // Restore engine flaws to the synced flaw data by engine design id
        for flaw in &engine_active_flaws {
            if let Some(idx) = flaw.engine_design_id {
                if idx < self.engine_designs_flaws.len() {
                    let (ref mut active, ref fixed) = self.engine_designs_flaws[idx];
                    // Only add if not already present
                    if !active.iter().any(|f| f.id == flaw.id)
                        && !fixed.iter().any(|f| f.id == flaw.id)
                    {
                        active.push(flaw.clone());
                    } else if let Some(existing) = active.iter_mut().find(|f| f.id == flaw.id) {
                        // Update discovered state
                        existing.discovered = flaw.discovered;
                    }
                }
            }
        }
        for flaw in &engine_fixed_flaws {
            if let Some(idx) = flaw.engine_design_id {
                if idx < self.engine_designs_flaws.len() {
                    let (ref mut active, ref mut fixed) = self.engine_designs_flaws[idx];
                    // Remove from active if present, add to fixed if not present
                    active.retain(|f| f.id != flaw.id);
                    if !fixed.iter().any(|f| f.id == flaw.id) {
                        fixed.push(flaw.clone());
                    }
                }
            }
        }

        // Now set the design (keeping only non-engine flaws in the design's lists)
        let mut clean_design = design;
        let before_active = clean_design.active_flaws.len();
        let before_fixed = clean_design.fixed_flaws.len();
        clean_design.active_flaws.retain(|f| f.flaw_type != crate::flaw::FlawType::Engine);
        clean_design.fixed_flaws.retain(|f| f.flaw_type != crate::flaw::FlawType::Engine);

        godot_print!(
            "set_design: AFTER filter - active: {} -> {}, fixed: {} -> {}",
            before_active,
            clean_design.active_flaws.len(),
            before_fixed,
            clean_design.fixed_flaws.len()
        );
        godot_print!(
            "set_design: clean_design flaws_generated={}, signature={}",
            clean_design.flaws_generated,
            clean_design.get_flaw_design_signature()
        );

        self.design = clean_design;
        self.emit_design_changed();
    }

    /// Sync engine data from Company (called by GameManager)
    /// Updates engine snapshots and flaw data
    pub fn sync_engine_data(
        &mut self,
        snapshots: Vec<EngineDesignSnapshot>,
        flaws: Vec<(Vec<Flaw>, Vec<Flaw>)>,
    ) {
        self.engine_snapshots = snapshots;
        self.engine_designs_flaws = flaws;
        self.engine_flaws_synced = true;
    }

    /// Set the PlayerFinance reference
    pub fn set_finance(&mut self, finance: Gd<PlayerFinance>) {
        self.finance = Some(finance);
    }

    /// Get the PlayerFinance reference (if set)
    #[func]
    pub fn get_finance(&self) -> Option<Gd<PlayerFinance>> {
        self.finance.clone()
    }

    /// Check if player can afford a cost (uses PlayerFinance if available, falls back to design.budget)
    fn can_afford(&self, amount: f64) -> bool {
        if let Some(ref finance) = self.finance {
            finance.bind().can_afford(amount)
        } else {
            self.design.remaining_budget() >= amount
        }
    }

    /// Deduct a cost from player finances (uses PlayerFinance if available)
    /// Returns true if successful
    fn deduct_cost(&mut self, amount: f64) -> bool {
        if let Some(ref mut finance) = self.finance {
            finance.bind_mut().deduct(amount)
        } else {
            // Fallback to design.testing_spent for backwards compatibility
            if self.design.remaining_budget() >= amount {
                self.design.testing_spent += amount;
                true
            } else {
                false
            }
        }
    }
}
