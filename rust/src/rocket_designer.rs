use godot::prelude::*;

use crate::engine::{costs, EngineType};
use crate::rocket_design::{RocketDesign, DEFAULT_PAYLOAD_KG, TARGET_DELTA_V_MS};

/// Godot-accessible rocket designer node
/// Allows creating and configuring rocket designs from GDScript
#[derive(GodotClass)]
#[class(base=Node)]
pub struct RocketDesigner {
    design: RocketDesign,
    base: Base<Node>,
}

#[godot_api]
impl INode for RocketDesigner {
    fn init(base: Base<Node>) -> Self {
        godot_print!("RocketDesigner initialized");
        Self {
            design: RocketDesign::new(),
            base,
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
        EngineType::all().len() as i32
    }

    /// Returns the name of an engine type by index
    /// 0 = Hydrolox, 1 = Kerolox
    #[func]
    pub fn get_engine_name(&self, engine_type: i32) -> GString {
        match EngineType::from_index(engine_type) {
            Some(et) => GString::from(et.spec().name.as_str()),
            None => GString::from("Unknown"),
        }
    }

    /// Returns the mass of an engine type in kg
    #[func]
    pub fn get_engine_mass(&self, engine_type: i32) -> f64 {
        match EngineType::from_index(engine_type) {
            Some(et) => et.spec().mass_kg,
            None => 0.0,
        }
    }

    /// Returns the thrust of an engine type in kN
    #[func]
    pub fn get_engine_thrust(&self, engine_type: i32) -> f64 {
        match EngineType::from_index(engine_type) {
            Some(et) => et.spec().thrust_kn,
            None => 0.0,
        }
    }

    /// Returns the exhaust velocity of an engine type in m/s
    #[func]
    pub fn get_engine_exhaust_velocity(&self, engine_type: i32) -> f64 {
        match EngineType::from_index(engine_type) {
            Some(et) => et.spec().exhaust_velocity_ms,
            None => 0.0,
        }
    }

    /// Returns the failure rate of an engine type (0.0 to 1.0)
    #[func]
    pub fn get_engine_failure_rate(&self, engine_type: i32) -> f64 {
        match EngineType::from_index(engine_type) {
            Some(et) => et.spec().failure_rate,
            None => 0.0,
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
        let et = EngineType::from_index(engine_type).unwrap_or(EngineType::Kerolox);
        let index = self.design.add_stage(et) as i32;
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
        self.design.stages[stage_index as usize].engine_type.to_index()
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
    #[func]
    pub fn set_stage_engine_count(&mut self, stage_index: i32, count: i32) {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return;
        }
        let count = count.max(1) as u32; // Minimum 1 engine
        self.design.stages[stage_index as usize].engine_count = count;
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

    /// Gets the target delta-v for LEO in m/s
    #[func]
    pub fn get_target_delta_v(&self) -> f64 {
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

    /// Gets the ignition failure rate for a stage
    #[func]
    pub fn get_stage_ignition_failure_rate(&self, stage_index: i32) -> f64 {
        if stage_index < 0 || stage_index as usize >= self.design.stages.len() {
            return 0.0;
        }
        self.design.stages[stage_index as usize].ignition_failure_rate()
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

    /// Gets the failure rate of a launch event by index
    #[func]
    pub fn get_launch_event_failure_rate(&self, event_index: i32) -> f64 {
        let events = self.design.generate_launch_events();
        if event_index < 0 || event_index as usize >= events.len() {
            return 0.0;
        }
        events[event_index as usize].failure_rate
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
        match EngineType::from_index(engine_type) {
            Some(et) => et.engine_cost(),
            None => 0.0,
        }
    }

    /// Gets the propellant density for an engine type in kg/m³
    #[func]
    pub fn get_propellant_density(&self, engine_type: i32) -> f64 {
        match EngineType::from_index(engine_type) {
            Some(et) => et.propellant_density(),
            None => 0.0,
        }
    }

    /// Gets the cost per cubic meter of tank volume
    #[func]
    pub fn get_tank_cost_per_m3(&self) -> f64 {
        costs::TANK_COST_PER_M3
    }

    /// Gets the fixed overhead cost per stage
    #[func]
    pub fn get_stage_overhead_cost(&self) -> f64 {
        costs::STAGE_OVERHEAD_COST
    }

    /// Gets the fixed overhead cost per rocket
    #[func]
    pub fn get_rocket_overhead_cost(&self) -> f64 {
        costs::ROCKET_OVERHEAD_COST
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
        self.design.remaining_budget()
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
    // Signals
    // ==========================================

    /// Signal emitted when the design changes
    #[signal]
    fn design_changed();

    /// Helper to emit design_changed signal
    fn emit_design_changed(&mut self) {
        self.base_mut().emit_signal("design_changed", &[]);
    }
}
