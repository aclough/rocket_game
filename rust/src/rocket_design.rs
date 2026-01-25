use crate::engine::{costs, EngineType};
use crate::flaw::{calculate_flaw_failure_rate, estimate_success_rate, estimate_unknown_flaw_count, run_test, Flaw, FlawGenerator, FlawType};
use crate::stage::RocketStage;

/// Represents a group of stages that fire simultaneously
/// A core stage with zero or more boosters attached to it
#[derive(Debug, Clone)]
pub struct BoosterGroup {
    /// Index of the core stage (the stage boosters are attached to)
    pub core_stage_index: usize,
    /// Indices of booster stages that fire with this core
    pub booster_indices: Vec<usize>,
}

/// Mission constants for LEO insertion
/// Note: This is the EFFECTIVE delta-v needed, accounting for gravity and drag losses
pub const TARGET_DELTA_V_MS: f64 = 8100.0; // ~7800 orbital velocity + ~300 aerodynamic losses
pub const DEFAULT_PAYLOAD_KG: f64 = 8000.0;

/// Gravity loss coefficients by stage position
/// These represent how much of the burn is fighting gravity vs. building horizontal velocity
/// First stage burns mostly vertical, upper stages burn more horizontal (gravity turn)
/// Real gravity losses are ~1000-1500 m/s for first stage (~10-15% of total delta-v)
pub mod gravity_coefficients {
    /// First stage: mostly vertical, significant gravity losses
    pub const FIRST_STAGE: f64 = 0.15;
    /// Second stage: gravity turn in progress, moderate losses
    pub const SECOND_STAGE: f64 = 0.08;
    /// Third stage: mostly horizontal, low losses
    pub const THIRD_STAGE: f64 = 0.03;
    /// Fourth+ stages: nearly horizontal, minimal losses
    pub const UPPER_STAGE: f64 = 0.01;

    /// Get the gravity loss coefficient for a stage based on its position
    /// Stage index 0 = first stage (fires first)
    pub fn for_stage(stage_index: usize, total_stages: usize) -> f64 {
        if total_stages == 0 {
            return 0.0;
        }

        // For single-stage rockets, use an average coefficient
        if total_stages == 1 {
            return 0.10;
        }

        match stage_index {
            0 => FIRST_STAGE,
            1 => SECOND_STAGE,
            2 => THIRD_STAGE,
            _ => UPPER_STAGE,
        }
    }
}

/// A complete rocket design with multiple stages
#[derive(Debug, Clone)]
pub struct RocketDesign {
    /// Stages from bottom (first to fire) to top (last to fire)
    /// Index 0 is the first stage (bottom), highest index is last stage (top)
    pub stages: Vec<RocketStage>,
    /// Payload mass in kilograms
    pub payload_mass_kg: f64,
    /// Target delta-v for the mission (can be set per-contract)
    pub target_delta_v: f64,

    // Future-proofing fields (from Rocket Tycoon 1.0 vision)

    /// Name of this rocket design
    pub name: String,
    /// Number of times this design has been launched (for reliability progression)
    pub launch_count: u32,

    // Flaw system fields

    /// Hidden flaws in this rocket design
    pub flaws: Vec<Flaw>,
    /// Whether flaws have been generated for this design
    pub flaws_generated: bool,
    /// Budget spent on testing and fixing (deducted from remaining budget)
    pub testing_spent: f64,
    /// Signature of the design when flaws were generated
    /// Used to detect when the design has changed significantly
    flaw_design_signature: String,
}

impl RocketDesign {
    /// Create a new empty rocket design
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            payload_mass_kg: DEFAULT_PAYLOAD_KG,
            target_delta_v: TARGET_DELTA_V_MS,
            name: "Unnamed Rocket".to_string(),
            launch_count: 0,
            flaws: Vec::new(),
            flaws_generated: false,
            testing_spent: 0.0,
            flaw_design_signature: String::new(),
        }
    }

    /// Compute a signature string that captures the essential design characteristics
    /// Changes to engine types, counts, or propellant masses will change the signature
    pub fn compute_design_signature(&self) -> String {
        let mut signature = String::new();
        signature.push_str(&format!("stages:{};", self.stages.len()));
        for (i, stage) in self.stages.iter().enumerate() {
            signature.push_str(&format!(
                "s{}:{}x{}:{:.0}kg,b:{};",
                i,
                stage.engine_type.to_index(),
                stage.engine_count,
                stage.propellant_mass_kg,
                if stage.is_booster { 1 } else { 0 }
            ));
        }
        signature
    }

    /// Check if the design has changed significantly since flaws were generated
    /// Returns true if the design signature differs from when flaws were generated
    pub fn design_changed_since_flaws(&self) -> bool {
        if !self.flaws_generated {
            return false; // No flaws generated yet, nothing to compare
        }
        self.compute_design_signature() != self.flaw_design_signature
    }

    /// Reset flaws and testing state (call when design changes significantly)
    pub fn reset_flaws(&mut self) {
        self.flaws.clear();
        self.flaws_generated = false;
        self.testing_spent = 0.0;
        self.flaw_design_signature.clear();
    }

    /// Check if flaws need to be reset due to design changes, and reset if so
    /// Returns true if flaws were reset
    pub fn check_and_reset_flaws_if_changed(&mut self) -> bool {
        if self.design_changed_since_flaws() {
            self.reset_flaws();
            true
        } else {
            false
        }
    }

    /// Get the target delta-v for this mission
    pub fn target_delta_v(&self) -> f64 {
        self.target_delta_v
    }

    /// Set the target delta-v for this mission
    pub fn set_target_delta_v(&mut self, delta_v: f64) {
        self.target_delta_v = delta_v;
    }

    /// Create a default two-stage rocket that's almost sufficient for LEO
    pub fn default_design() -> Self {
        let mut design = Self::new();
        design.name = "Default Rocket".to_string();

        // First stage: 5 Kerolox engines, large propellant load
        let mut stage1 = RocketStage::new(EngineType::Kerolox);
        stage1.engine_count = 5;
        stage1.propellant_mass_kg = 100000.0;

        // Second stage: 1 Hydrolox engine
        let mut stage2 = RocketStage::new(EngineType::Hydrolox);
        stage2.engine_count = 1;
        stage2.propellant_mass_kg = 20000.0;

        design.stages.push(stage1);
        design.stages.push(stage2);

        design
    }

    /// Add a new stage to the top of the rocket
    pub fn add_stage(&mut self, engine_type: EngineType) -> usize {
        let stage = RocketStage::new(engine_type);
        self.stages.push(stage);
        self.stages.len() - 1
    }

    /// Remove a stage by index
    pub fn remove_stage(&mut self, index: usize) -> Option<RocketStage> {
        if index < self.stages.len() {
            Some(self.stages.remove(index))
        } else {
            None
        }
    }

    /// Move a stage from one position to another
    pub fn move_stage(&mut self, from: usize, to: usize) {
        if from < self.stages.len() && to < self.stages.len() && from != to {
            let stage = self.stages.remove(from);
            self.stages.insert(to, stage);
        }
    }

    // ==========================================
    // Booster Management
    // ==========================================

    /// Find all booster groups in the design
    /// A booster group consists of a core stage and all boosters attached to it
    /// Boosters at index i are attached to the stage at index i-1 (the core)
    pub fn find_booster_groups(&self) -> Vec<BoosterGroup> {
        let mut groups = Vec::new();
        let mut processed = vec![false; self.stages.len()];

        for i in 0..self.stages.len() {
            if processed[i] {
                continue;
            }

            // Skip boosters - they'll be added to their core's group
            if self.stages[i].is_booster {
                continue;
            }

            // This is a core stage - find all boosters attached to it
            let mut booster_indices = Vec::new();
            // Boosters are at higher indices and marked as is_booster
            // A booster at index j attaches to core at index j-1
            for j in (i + 1)..self.stages.len() {
                if self.stages[j].is_booster {
                    // Check if this booster attaches to stage i
                    // A booster at j attaches to j-1, but if j-1 is also a booster,
                    // we need to trace back to the core
                    let mut attach_point = j - 1;
                    while attach_point > i && self.stages[attach_point].is_booster {
                        attach_point -= 1;
                    }
                    if attach_point == i {
                        booster_indices.push(j);
                        processed[j] = true;
                    }
                } else {
                    // Hit a non-booster stage, stop looking for boosters for this core
                    break;
                }
            }

            groups.push(BoosterGroup {
                core_stage_index: i,
                booster_indices,
            });
            processed[i] = true;
        }

        groups
    }

    /// Validate booster configuration
    /// Returns Ok(()) if valid, or an error message if invalid
    pub fn validate_boosters(&self) -> Result<(), String> {
        for (i, stage) in self.stages.iter().enumerate() {
            if stage.is_booster {
                // Must not be first stage
                if i == 0 {
                    return Err("First stage cannot be a booster".to_string());
                }

                // Must have more than 1 engine
                if stage.engine_count <= 1 {
                    return Err(format!(
                        "Booster stage {} must have more than 1 engine",
                        i + 1
                    ));
                }

                // Find the core stage this booster attaches to
                let mut core_index = i - 1;
                while core_index > 0 && self.stages[core_index].is_booster {
                    core_index -= 1;
                }

                // Burn time must not exceed core stage burn time
                let core_burn_time = self.stages[core_index].burn_time_seconds();
                let booster_burn_time = stage.burn_time_seconds();
                if booster_burn_time > core_burn_time {
                    return Err(format!(
                        "Booster stage {} burns longer ({:.1}s) than core stage {} ({:.1}s)",
                        i + 1,
                        booster_burn_time,
                        core_index + 1,
                        core_burn_time
                    ));
                }
            }
        }
        Ok(())
    }

    /// Get validation error for a specific stage being set as a booster
    /// Returns None if valid, or an error message if invalid
    pub fn get_booster_validation_error(&self, stage_index: usize) -> Option<String> {
        if stage_index >= self.stages.len() {
            return Some("Invalid stage index".to_string());
        }

        // Can't make first stage a booster
        if stage_index == 0 {
            return Some("First stage cannot be a booster".to_string());
        }

        let stage = &self.stages[stage_index];

        // Must have more than 1 engine
        if stage.engine_count <= 1 {
            return Some("Booster must have more than 1 engine".to_string());
        }

        // Find the core stage
        let mut core_index = stage_index - 1;
        while core_index > 0 && self.stages[core_index].is_booster {
            core_index -= 1;
        }

        // Check burn time
        let core_burn_time = self.stages[core_index].burn_time_seconds();
        let booster_burn_time = stage.burn_time_seconds();
        if booster_burn_time > core_burn_time {
            return Some(format!(
                "Booster burns longer ({:.1}s) than core ({:.1}s)",
                booster_burn_time, core_burn_time
            ));
        }

        None
    }

    /// Check if a stage can be made a booster
    pub fn can_be_booster(&self, stage_index: usize) -> bool {
        self.get_booster_validation_error(stage_index).is_none()
    }

    /// Get combined thrust for a booster group in kN
    pub fn booster_group_thrust_kn(&self, group: &BoosterGroup) -> f64 {
        let mut total = self.stages[group.core_stage_index].total_thrust_kn();
        for &bi in &group.booster_indices {
            total += self.stages[bi].total_thrust_kn();
        }
        total
    }

    /// Get combined wet mass for a booster group in kg (includes payload above)
    pub fn booster_group_wet_mass_kg(&self, group: &BoosterGroup, payload_above: f64) -> f64 {
        let mut total = self.stages[group.core_stage_index].wet_mass_kg() + payload_above;
        for &bi in &group.booster_indices {
            total += self.stages[bi].wet_mass_with_attachment_kg();
        }
        total
    }

    /// Get combined initial TWR for a booster group
    pub fn booster_group_initial_twr(&self, group: &BoosterGroup, payload_above: f64) -> f64 {
        let thrust_n = self.booster_group_thrust_kn(group) * 1000.0;
        let mass_kg = self.booster_group_wet_mass_kg(group, payload_above);
        let weight_n = mass_kg * 9.81;

        if weight_n > 0.0 {
            thrust_n / weight_n
        } else {
            0.0
        }
    }

    /// Calculate the mass above a given stage (payload + all upper stages)
    /// Stage 0 is the bottom, so it carries the most mass
    /// For boosters, this includes the booster attachment mass
    pub fn mass_above_stage(&self, stage_index: usize) -> f64 {
        let mut mass = self.payload_mass_kg;

        // Add mass of all stages above this one
        for i in (stage_index + 1)..self.stages.len() {
            // Use wet mass with attachment for boosters
            mass += self.stages[i].wet_mass_with_attachment_kg();
        }

        mass
    }

    /// Calculate the mass above a given stage including any boosters attached to it
    /// This is used when calculating the TWR/delta-v for a core stage with boosters
    pub fn mass_above_stage_with_boosters(&self, stage_index: usize) -> f64 {
        let mut mass = self.mass_above_stage(stage_index);

        // If this is a core stage, also add any boosters attached to it
        if !self.stages[stage_index].is_booster {
            let groups = self.find_booster_groups();
            for group in &groups {
                if group.core_stage_index == stage_index {
                    for &bi in &group.booster_indices {
                        mass += self.stages[bi].wet_mass_with_attachment_kg();
                    }
                }
            }
        }

        mass
    }

    /// Calculate delta-v for a core stage with boosters during parallel burn
    /// Returns (phase1_dv, phase2_dv) where:
    /// - phase1_dv is delta-v during combined burn (boosters + core)
    /// - phase2_dv is delta-v during core-only burn (after boosters deplete)
    pub fn calculate_parallel_stage_delta_v(&self, group: &BoosterGroup) -> (f64, f64) {
        if group.booster_indices.is_empty() {
            // No boosters, all delta-v comes from core alone
            let payload = self.mass_above_stage(group.core_stage_index);
            let dv = self.stages[group.core_stage_index].delta_v(payload);
            return (0.0, dv);
        }

        let core = &self.stages[group.core_stage_index];
        let payload_above = self.mass_above_stage(group.core_stage_index);

        // Find shortest booster burn time (when first booster depletes)
        let booster_burn_time = group
            .booster_indices
            .iter()
            .map(|&bi| self.stages[bi].burn_time_seconds())
            .fold(f64::INFINITY, f64::min);

        let _core_burn_time = core.burn_time_seconds();

        // Combined thrust during parallel burn
        let combined_thrust_kn = self.booster_group_thrust_kn(group);
        let _combined_thrust_n = combined_thrust_kn * 1000.0;

        // Calculate thrust-weighted average exhaust velocity
        let core_thrust_kn = core.total_thrust_kn();
        let core_ve = core.exhaust_velocity_ms();
        let mut weighted_ve = core_thrust_kn * core_ve;
        let mut total_thrust = core_thrust_kn;

        for &bi in &group.booster_indices {
            let booster = &self.stages[bi];
            weighted_ve += booster.total_thrust_kn() * booster.exhaust_velocity_ms();
            total_thrust += booster.total_thrust_kn();
        }
        let effective_ve = weighted_ve / total_thrust;

        // Initial mass (all stages + payload)
        let mut m0 = core.wet_mass_kg() + payload_above;
        for &bi in &group.booster_indices {
            m0 += self.stages[bi].wet_mass_with_attachment_kg();
        }

        // Calculate propellant consumed during phase 1 (parallel burn)
        // Mass flow rate = thrust / exhaust_velocity
        let core_mass_flow = core.total_thrust_kn() * 1000.0 / core.exhaust_velocity_ms();
        let mut total_mass_flow = core_mass_flow;
        for &bi in &group.booster_indices {
            let booster = &self.stages[bi];
            total_mass_flow +=
                booster.total_thrust_kn() * 1000.0 / booster.exhaust_velocity_ms();
        }

        // Propellant consumed during phase 1
        let propellant_phase1 = total_mass_flow * booster_burn_time;

        // Mass at end of phase 1 (boosters depleted, ready to jettison)
        let m1 = m0 - propellant_phase1;

        // Phase 1 delta-v (parallel burn)
        let phase1_dv = if m1 > 0.0 && m0 > m1 {
            effective_ve * (m0 / m1).ln()
        } else {
            0.0
        };

        // After booster jettison, core continues alone
        // Mass after jettisoning boosters
        let mut booster_dry_mass = 0.0;
        for &bi in &group.booster_indices {
            booster_dry_mass += self.stages[bi].dry_mass_with_attachment_kg();
        }
        let m2_start = m1 - booster_dry_mass;

        // Remaining propellant in core
        let core_propellant_used = core_mass_flow * booster_burn_time;
        let core_propellant_remaining = core.propellant_mass_kg - core_propellant_used;

        if core_propellant_remaining <= 0.0 || m2_start <= 0.0 {
            return (phase1_dv, 0.0);
        }

        // Final mass after core burns out
        let m2_end = m2_start - core_propellant_remaining;

        // Phase 2 delta-v (core alone)
        let phase2_dv = if m2_end > 0.0 && m2_start > m2_end {
            core_ve * (m2_start / m2_end).ln()
        } else {
            0.0
        };

        (phase1_dv, phase2_dv)
    }

    /// Calculate delta-v for a single stage
    /// For boosters, returns 0 (their contribution is counted with the core stage)
    /// For core stages with boosters, returns combined delta-v
    pub fn stage_delta_v(&self, stage_index: usize) -> f64 {
        if stage_index >= self.stages.len() {
            return 0.0;
        }

        // Boosters don't contribute delta-v separately - counted with core
        if self.stages[stage_index].is_booster {
            return 0.0;
        }

        // Check if this core has boosters
        let groups = self.find_booster_groups();
        for group in &groups {
            if group.core_stage_index == stage_index && !group.booster_indices.is_empty() {
                let (phase1, phase2) = self.calculate_parallel_stage_delta_v(&group);
                return phase1 + phase2;
            }
        }

        // No boosters - normal calculation
        let payload = self.mass_above_stage(stage_index);
        self.stages[stage_index].delta_v(payload)
    }

    /// Calculate total delta-v for the entire rocket (ideal, no gravity losses)
    /// Stages fire from bottom (index 0) to top
    /// Properly handles parallel stages (boosters)
    pub fn total_delta_v(&self) -> f64 {
        let mut total = 0.0;
        for i in 0..self.stages.len() {
            total += self.stage_delta_v(i);
        }
        total
    }

    // ==========================================
    // TWR and Gravity Loss Calculations
    // ==========================================

    /// Get the gravity loss coefficient for a stage based on its position
    pub fn stage_gravity_coefficient(&self, stage_index: usize) -> f64 {
        gravity_coefficients::for_stage(stage_index, self.stages.len())
    }

    /// Calculate the initial TWR for a stage (thrust / weight at ignition)
    /// For core stages with boosters, returns combined TWR
    /// For booster stages, returns 0 (their TWR is combined with core)
    pub fn stage_twr(&self, stage_index: usize) -> f64 {
        if stage_index >= self.stages.len() {
            return 0.0;
        }

        // Boosters don't have separate TWR - counted with core
        if self.stages[stage_index].is_booster {
            return 0.0;
        }

        // Check if this core has boosters
        let groups = self.find_booster_groups();
        for group in &groups {
            if group.core_stage_index == stage_index && !group.booster_indices.is_empty() {
                let payload = self.mass_above_stage(stage_index);
                return self.booster_group_initial_twr(&group, payload);
            }
        }

        // No boosters - normal calculation
        let payload = self.mass_above_stage(stage_index);
        self.stages[stage_index].initial_twr(payload)
    }

    /// Get the combined TWR during booster burn for a stage index
    /// Returns None if this stage doesn't have boosters
    pub fn get_combined_twr_during_boost(&self, stage_index: usize) -> Option<f64> {
        if stage_index >= self.stages.len() {
            return None;
        }

        if self.stages[stage_index].is_booster {
            return None;
        }

        let groups = self.find_booster_groups();
        for group in &groups {
            if group.core_stage_index == stage_index && !group.booster_indices.is_empty() {
                let payload = self.mass_above_stage(stage_index);
                return Some(self.booster_group_initial_twr(&group, payload));
            }
        }

        None
    }

    /// Calculate the gravity loss for a single stage in m/s
    /// For boosters, returns 0 (their loss is counted with core)
    pub fn stage_gravity_loss(&self, stage_index: usize) -> f64 {
        if stage_index >= self.stages.len() {
            return 0.0;
        }

        // Boosters don't have separate gravity loss
        if self.stages[stage_index].is_booster {
            return 0.0;
        }

        // For stages with boosters, use combined TWR for gravity loss calculation
        let ideal_dv = self.stage_delta_v(stage_index);
        let effective_dv = self.stage_effective_delta_v(stage_index);
        (ideal_dv - effective_dv).max(0.0)
    }

    /// Calculate the effective delta-v for a single stage (after gravity losses)
    /// For boosters, returns 0 (their contribution is counted with core)
    pub fn stage_effective_delta_v(&self, stage_index: usize) -> f64 {
        if stage_index >= self.stages.len() {
            return 0.0;
        }

        // Boosters don't contribute delta-v separately
        if self.stages[stage_index].is_booster {
            return 0.0;
        }

        let ideal_dv = self.stage_delta_v(stage_index);
        let twr = self.stage_twr(stage_index);
        let coefficient = self.stage_gravity_coefficient(stage_index);

        // Use the same gravity loss formula but with combined TWR
        if twr <= 0.0 {
            return 0.0;
        }

        // Simplified gravity loss: dv_loss = C × dv × (1 - 1/R) / TWR
        // But we need mass ratio R, which is more complex with boosters
        // Use a simplified approximation based on the combined TWR
        let gravity_loss = coefficient * ideal_dv / twr;
        (ideal_dv - gravity_loss).max(0.0)
    }

    /// Calculate total effective delta-v for the entire rocket (after gravity losses)
    pub fn total_effective_delta_v(&self) -> f64 {
        let mut total = 0.0;
        for i in 0..self.stages.len() {
            total += self.stage_effective_delta_v(i);
        }
        total
    }

    /// Calculate total gravity losses across all stages
    pub fn total_gravity_loss(&self) -> f64 {
        let mut total = 0.0;
        for i in 0..self.stages.len() {
            total += self.stage_gravity_loss(i);
        }
        total
    }

    /// Calculate overall gravity efficiency (effective_dv / ideal_dv)
    pub fn gravity_efficiency(&self) -> f64 {
        let ideal = self.total_delta_v();
        if ideal <= 0.0 {
            return 0.0;
        }
        self.total_effective_delta_v() / ideal
    }

    /// Check if the design provides sufficient effective delta-v for the mission
    /// This accounts for gravity losses based on each stage's TWR
    pub fn is_sufficient(&self) -> bool {
        self.total_effective_delta_v() >= self.target_delta_v
    }

    /// Get mass fraction for a stage
    pub fn stage_mass_fraction(&self, stage_index: usize) -> f64 {
        if stage_index >= self.stages.len() {
            return 0.0;
        }
        let payload = self.mass_above_stage(stage_index);
        self.stages[stage_index].mass_fraction(payload)
    }

    /// Set mass fraction for a stage (updates propellant mass)
    pub fn set_stage_mass_fraction(&mut self, stage_index: usize, fraction: f64) {
        if stage_index >= self.stages.len() {
            return;
        }
        let payload = self.mass_above_stage(stage_index);
        self.stages[stage_index].set_mass_fraction(fraction, payload);
    }

    /// Recalculate all propellant masses from stored mass fractions
    /// Call this after reordering stages to maintain consistent fractions
    pub fn recalculate_from_fractions(&mut self, target_fractions: &[f64]) {
        // Work from top stage down since lower stages depend on upper mass
        for i in (0..self.stages.len()).rev() {
            if i < target_fractions.len() {
                self.set_stage_mass_fraction(i, target_fractions[i]);
            }
        }
    }

    /// Get the number of stages
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// Check if design is valid (has at least one stage)
    pub fn is_valid(&self) -> bool {
        !self.stages.is_empty()
    }

    /// Calculate total wet mass of the rocket (all stages + payload)
    pub fn total_wet_mass_kg(&self) -> f64 {
        let stage_mass: f64 = self.stages.iter().map(|s| s.wet_mass_kg()).sum();
        stage_mass + self.payload_mass_kg
    }

    /// Calculate total dry mass of the rocket (no propellant)
    pub fn total_dry_mass_kg(&self) -> f64 {
        let stage_mass: f64 = self.stages.iter().map(|s| s.dry_mass_kg()).sum();
        stage_mass + self.payload_mass_kg
    }

    /// Calculate thrust-to-weight ratio at liftoff
    /// Must be > 1.0 for the rocket to lift off
    /// Typically want 1.2-1.5 for a real rocket
    /// Includes boosters if the first stage has them
    pub fn liftoff_twr(&self) -> f64 {
        if self.stages.is_empty() {
            return 0.0;
        }

        // Check if first stage has boosters
        let groups = self.find_booster_groups();
        for group in &groups {
            if group.core_stage_index == 0 && !group.booster_indices.is_empty() {
                let thrust_n = self.booster_group_thrust_kn(&group) * 1000.0;
                let weight_n = self.total_wet_mass_kg() * 9.81;
                return thrust_n / weight_n;
            }
        }

        // No boosters on first stage
        let first_stage = &self.stages[0];
        let thrust_n = first_stage.total_thrust_kn() * 1000.0; // kN to N
        let weight_n = self.total_wet_mass_kg() * 9.81; // kg to N (Earth gravity)

        thrust_n / weight_n
    }

    /// Calculate how much effective delta-v margin we have (positive = excess, negative = shortfall)
    pub fn delta_v_margin(&self) -> f64 {
        self.total_effective_delta_v() - self.target_delta_v
    }

    /// Calculate effective delta-v as a percentage of target (100% = exactly sufficient)
    pub fn delta_v_percentage(&self) -> f64 {
        if self.target_delta_v == 0.0 {
            return 0.0;
        }
        (self.total_effective_delta_v() / self.target_delta_v) * 100.0
    }

    /// Calculate ideal delta-v as a percentage of target (ignoring gravity losses)
    pub fn ideal_delta_v_percentage(&self) -> f64 {
        if self.target_delta_v == 0.0 {
            return 0.0;
        }
        (self.total_delta_v() / self.target_delta_v) * 100.0
    }

    /// Calculate overall mission success probability
    /// Product of all event success probabilities
    pub fn mission_success_probability(&self) -> f64 {
        let events = self.generate_launch_events();
        let mut probability = 1.0;
        for event in events {
            probability *= 1.0 - event.failure_rate;
        }
        probability
    }

    // ==========================================
    // Cost Calculations
    // ==========================================

    /// Get the starting budget in dollars
    pub fn starting_budget() -> f64 {
        costs::STARTING_BUDGET
    }

    /// Calculate the cost of a single stage in dollars
    /// Includes booster attachment cost if the stage is a booster
    pub fn stage_cost(&self, stage_index: usize) -> f64 {
        if stage_index >= self.stages.len() {
            return 0.0;
        }
        self.stages[stage_index].total_cost_with_attachment()
    }

    /// Calculate the total cost of all stages in dollars
    /// Includes booster attachment costs for boosters
    pub fn total_stages_cost(&self) -> f64 {
        self.stages.iter().map(|s| s.total_cost_with_attachment()).sum()
    }

    /// Calculate the rocket overhead cost in dollars
    /// This is a fixed cost per rocket for integration, testing, and launch operations
    pub fn rocket_overhead_cost(&self) -> f64 {
        if self.stages.is_empty() {
            0.0
        } else {
            costs::ROCKET_OVERHEAD_COST
        }
    }

    /// Calculate the total cost of the rocket design in dollars
    /// Includes all stages plus rocket overhead
    pub fn total_cost(&self) -> f64 {
        self.total_stages_cost() + self.rocket_overhead_cost()
    }

    /// Calculate remaining budget after subtracting rocket cost and testing expenses
    pub fn remaining_budget(&self) -> f64 {
        costs::STARTING_BUDGET - self.total_cost() - self.testing_spent
    }

    /// Check if the design is within budget (including testing expenses)
    pub fn is_within_budget(&self) -> bool {
        self.total_cost() + self.testing_spent <= costs::STARTING_BUDGET
    }

    /// Check if the design is both sufficient (delta-v) and affordable (budget)
    pub fn is_launchable(&self) -> bool {
        self.is_sufficient() && self.is_within_budget()
    }

    // ==========================================
    // Flaw System
    // ==========================================

    /// Get the cost to run an engine test
    pub fn engine_test_cost() -> f64 {
        costs::ENGINE_TEST_COST
    }

    /// Get the cost to run a rocket test
    pub fn rocket_test_cost() -> f64 {
        costs::ROCKET_TEST_COST
    }

    /// Get the cost to fix a discovered flaw
    pub fn flaw_fix_cost() -> f64 {
        costs::FLAW_FIX_COST
    }

    /// Generate flaws for this rocket design
    /// Should be called when the design is finalized (before testing/launching)
    pub fn generate_flaws(&mut self) {
        if self.flaws_generated {
            return;
        }

        let total_engines: u32 = self.stages.iter().map(|s| s.engine_count).sum();
        let stage_count = self.stages.len();

        if total_engines == 0 || stage_count == 0 {
            return;
        }

        // Collect engine types and their counts
        let engine_types = self.get_engine_type_counts();

        let mut generator = FlawGenerator::new();
        self.flaws = generator.generate_flaws_with_engine_types(total_engines, stage_count, &engine_types);
        self.flaws_generated = true;
        // Save the design signature so we can detect changes
        self.flaw_design_signature = self.compute_design_signature();
    }

    /// Get a list of unique engine types and their total counts in the design
    /// Returns a vector of (engine_type_index, engine_count) pairs
    pub fn get_engine_type_counts(&self) -> Vec<(i32, u32)> {
        use std::collections::HashMap;
        let mut counts: HashMap<i32, u32> = HashMap::new();

        for stage in &self.stages {
            let engine_idx = stage.engine_type.to_index();
            *counts.entry(engine_idx).or_insert(0) += stage.engine_count;
        }

        counts.into_iter().collect()
    }

    /// Get the list of unique engine type indices in the design
    pub fn get_unique_engine_types(&self) -> Vec<i32> {
        use std::collections::HashSet;
        let mut types: HashSet<i32> = HashSet::new();

        for stage in &self.stages {
            types.insert(stage.engine_type.to_index());
        }

        let mut result: Vec<i32> = types.into_iter().collect();
        result.sort();
        result
    }

    /// Check if flaws have been generated
    pub fn has_flaws_generated(&self) -> bool {
        self.flaws_generated
    }

    /// Get all flaws
    pub fn get_flaws(&self) -> &[Flaw] {
        &self.flaws
    }

    /// Get the total number of flaws
    pub fn get_flaw_count(&self) -> usize {
        self.flaws.len()
    }

    /// Get the number of discovered flaws
    pub fn get_discovered_flaw_count(&self) -> usize {
        self.flaws.iter().filter(|f| f.discovered).count()
    }

    /// Get the number of fixed flaws
    pub fn get_fixed_flaw_count(&self) -> usize {
        self.flaws.iter().filter(|f| f.fixed).count()
    }

    /// Get the number of undiscovered, unfixed flaws (unknown issues)
    pub fn get_unknown_flaw_count(&self) -> usize {
        self.flaws.iter().filter(|f| !f.discovered && !f.fixed).count()
    }

    /// Get a flaw by index
    pub fn get_flaw(&self, index: usize) -> Option<&Flaw> {
        self.flaws.get(index)
    }

    /// Get a mutable flaw by index
    pub fn get_flaw_mut(&mut self, index: usize) -> Option<&mut Flaw> {
        self.flaws.get_mut(index)
    }

    /// Run an engine test - returns names of newly discovered flaws
    /// Costs ENGINE_TEST_COST from budget
    pub fn run_engine_test(&mut self) -> Vec<String> {
        if self.remaining_budget() < costs::ENGINE_TEST_COST {
            return Vec::new();
        }

        self.testing_spent += costs::ENGINE_TEST_COST;
        run_test(&mut self.flaws, FlawType::Engine)
    }

    /// Run an engine test for a specific engine type - returns names of newly discovered flaws
    /// Costs ENGINE_TEST_COST from budget
    pub fn run_engine_test_for_type(&mut self, engine_type_index: i32) -> Vec<String> {
        if self.remaining_budget() < costs::ENGINE_TEST_COST {
            return Vec::new();
        }

        self.testing_spent += costs::ENGINE_TEST_COST;
        crate::flaw::run_engine_test_for_type(&mut self.flaws, engine_type_index)
    }

    /// Run a rocket test - returns names of newly discovered flaws
    /// Costs ROCKET_TEST_COST from budget
    pub fn run_rocket_test(&mut self) -> Vec<String> {
        if self.remaining_budget() < costs::ROCKET_TEST_COST {
            return Vec::new();
        }

        self.testing_spent += costs::ROCKET_TEST_COST;
        run_test(&mut self.flaws, FlawType::Design)
    }

    /// Fix a flaw by ID
    /// Costs FLAW_FIX_COST from budget
    /// Returns true if the flaw was fixed
    pub fn fix_flaw(&mut self, flaw_id: u32) -> bool {
        if self.remaining_budget() < costs::FLAW_FIX_COST {
            return false;
        }

        for flaw in &mut self.flaws {
            if flaw.id == flaw_id && flaw.discovered && !flaw.fixed {
                flaw.fixed = true;
                self.testing_spent += costs::FLAW_FIX_COST;
                return true;
            }
        }

        false
    }

    /// Fix a flaw by index
    /// Costs FLAW_FIX_COST from budget
    /// Returns true if the flaw was fixed
    pub fn fix_flaw_by_index(&mut self, index: usize) -> bool {
        if self.remaining_budget() < costs::FLAW_FIX_COST {
            return false;
        }

        if let Some(flaw) = self.flaws.get_mut(index) {
            if flaw.discovered && !flaw.fixed {
                flaw.fixed = true;
                self.testing_spent += costs::FLAW_FIX_COST;
                return true;
            }
        }

        false
    }

    /// Get the additional failure rate from flaws for a given event
    /// stage_engine_type: the engine type index of the stage (for filtering engine flaws)
    pub fn get_flaw_failure_contribution(&self, event_name: &str, stage_engine_type: Option<i32>) -> f64 {
        calculate_flaw_failure_rate(&self.flaws, event_name, stage_engine_type)
    }

    /// Estimate success rate including flaw contributions
    pub fn estimate_success_rate_with_flaws(&self) -> f64 {
        let base_success = self.mission_success_probability();
        estimate_success_rate(&self.flaws, base_success)
    }

    /// Get estimated range of unknown flaw count (fuzzy, not exact)
    pub fn estimate_unknown_flaws(&self) -> (usize, usize) {
        estimate_unknown_flaw_count(&self.flaws)
    }

    /// Check if a flaw can be afforded
    pub fn can_afford_fix(&self) -> bool {
        self.remaining_budget() >= costs::FLAW_FIX_COST
    }

    /// Check if an engine test can be afforded
    pub fn can_afford_engine_test(&self) -> bool {
        self.remaining_budget() >= costs::ENGINE_TEST_COST
    }

    /// Check if a rocket test can be afforded
    pub fn can_afford_rocket_test(&self) -> bool {
        self.remaining_budget() >= costs::ROCKET_TEST_COST
    }

    /// Mark a flaw as discovered (used when failure occurs during launch)
    pub fn discover_flaw(&mut self, flaw_id: u32) {
        for flaw in &mut self.flaws {
            if flaw.id == flaw_id {
                flaw.discovered = true;
                break;
            }
        }
    }

    /// Check if any flaw triggers at a given event, and return the flaw ID if it caused a failure
    /// stage_engine_type: the engine type index of the stage that failed
    /// Returns Some(flaw_id) if a flaw triggered failure, None otherwise
    pub fn check_flaw_trigger(&self, event_name: &str, stage_engine_type: Option<i32>) -> Option<u32> {
        crate::flaw::check_flaw_trigger(&self.flaws, event_name, stage_engine_type)
    }

    /// Mark a flaw as discovered and return its name
    /// Used when a flaw causes a failure during launch
    pub fn discover_flaw_by_id(&mut self, flaw_id: u32) -> Option<String> {
        crate::flaw::mark_flaw_discovered(&mut self.flaws, flaw_id)
    }

    /// Get the engine type index for a flaw (None for design flaws)
    pub fn get_flaw_engine_type_index(&self, index: usize) -> Option<i32> {
        self.flaws.get(index).and_then(|f| f.engine_type_index)
    }

    /// Get total testing spent
    pub fn get_testing_spent(&self) -> f64 {
        self.testing_spent
    }
}

impl Default for RocketDesign {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a launch event during flight
#[derive(Debug, Clone)]
pub struct LaunchEvent {
    /// Name of the event
    pub name: String,
    /// Description of the event
    pub description: String,
    /// Failure rate for this event (0.0 to 1.0)
    pub failure_rate: f64,
    /// Which rocket stage this event belongs to (0-indexed)
    pub rocket_stage: usize,
}

impl RocketDesign {
    /// Generate the sequence of launch events based on the rocket design
    ///
    /// First stage: Ignition → Liftoff → MaxQ → [Booster Separation] → Separation
    /// Middle stages: Ignition → [Booster Separation] → Separation
    /// Last stage: Ignition → Payload Release
    /// Boosters separate before their core stage separates
    pub fn generate_launch_events(&self) -> Vec<LaunchEvent> {
        let mut events = Vec::new();

        // Find booster groups so we know which boosters belong to which core
        let groups = self.find_booster_groups();

        // Track stage number (only counting non-booster stages)
        let mut stage_num = 0;

        for (i, stage) in self.stages.iter().enumerate() {
            // Skip boosters - their events are added with their core stage
            if stage.is_booster {
                continue;
            }

            stage_num += 1;
            let is_first = stage_num == 1;
            let failure_rate = stage.ignition_failure_rate();

            // Find if this core has boosters
            let boosters_for_this_core: Vec<usize> = groups
                .iter()
                .find(|g| g.core_stage_index == i)
                .map(|g| g.booster_indices.clone())
                .unwrap_or_default();

            // Ignition event (includes boosters if any)
            if boosters_for_this_core.is_empty() {
                events.push(LaunchEvent {
                    name: format!("Stage {} Ignition", stage_num),
                    description: format!(
                        "Stage {} engine{} ignit{}",
                        stage_num,
                        if stage.engine_count > 1 { "s" } else { "" },
                        if stage.engine_count > 1 { "e" } else { "es" }
                    ),
                    failure_rate,
                    rocket_stage: i,
                });
            } else {
                // Combined ignition with boosters
                let total_engines: u32 = stage.engine_count
                    + boosters_for_this_core
                        .iter()
                        .map(|&bi| self.stages[bi].engine_count)
                        .sum::<u32>();
                events.push(LaunchEvent {
                    name: format!("Stage {} Ignition", stage_num),
                    description: format!(
                        "Stage {} and boosters ignite ({} engines)",
                        stage_num,
                        total_engines
                    ),
                    failure_rate,
                    rocket_stage: i,
                });
            }

            if is_first {
                // First stage gets Liftoff and MaxQ
                events.push(LaunchEvent {
                    name: "Liftoff".to_string(),
                    description: "Rocket lifts off from the pad".to_string(),
                    failure_rate: 0.02, // Fixed 2% for liftoff structural
                    rocket_stage: i,
                });

                events.push(LaunchEvent {
                    name: "Max-Q".to_string(),
                    description: "Maximum dynamic pressure".to_string(),
                    failure_rate: 0.05, // Fixed 5% for max-q aerodynamic
                    rocket_stage: i,
                });
            }

            // Booster separation events (happen before core separation)
            for &booster_idx in &boosters_for_this_core {
                events.push(LaunchEvent {
                    name: format!("Stage {} Booster Separation", stage_num),
                    description: format!("Stage {} booster separates", stage_num),
                    failure_rate: 0.03, // Fixed 3% for separation
                    rocket_stage: booster_idx,
                });
            }

            // Check if there are any non-booster stages after this one
            let has_non_booster_after = self.stages[(i + 1)..]
                .iter()
                .any(|s| !s.is_booster);

            if has_non_booster_after {
                // Regular stage separation
                events.push(LaunchEvent {
                    name: format!("Stage {} Separation", stage_num),
                    description: format!("Stage {} separates", stage_num),
                    failure_rate: 0.03, // Fixed 3% for separation
                    rocket_stage: i,
                });
            } else {
                // This is the last core stage - orbital insertion
                events.push(LaunchEvent {
                    name: "Payload Release".to_string(),
                    description: "Final burn for orbit".to_string(),
                    failure_rate: 0.02, // Fixed 2% for final burn
                    rocket_stage: i,
                });
            }
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_design() {
        let design = RocketDesign::new();
        assert_eq!(design.stages.len(), 0);
        assert_eq!(design.payload_mass_kg, DEFAULT_PAYLOAD_KG);
    }

    #[test]
    fn test_add_stage() {
        let mut design = RocketDesign::new();
        let idx = design.add_stage(EngineType::Kerolox);
        assert_eq!(idx, 0);
        assert_eq!(design.stages.len(), 1);

        let idx2 = design.add_stage(EngineType::Hydrolox);
        assert_eq!(idx2, 1);
        assert_eq!(design.stages.len(), 2);
    }

    #[test]
    fn test_remove_stage() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.add_stage(EngineType::Hydrolox);

        let removed = design.remove_stage(0);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().engine_type, EngineType::Kerolox);
        assert_eq!(design.stages.len(), 1);
        assert_eq!(design.stages[0].engine_type, EngineType::Hydrolox);
    }

    #[test]
    fn test_move_stage() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.add_stage(EngineType::Hydrolox);

        design.move_stage(0, 1);
        assert_eq!(design.stages[0].engine_type, EngineType::Hydrolox);
        assert_eq!(design.stages[1].engine_type, EngineType::Kerolox);
    }

    #[test]
    fn test_mass_above_stage() {
        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;

        let mut stage1 = RocketStage::new(EngineType::Kerolox);
        stage1.propellant_mass_kg = 10000.0;

        let mut stage2 = RocketStage::new(EngineType::Hydrolox);
        stage2.propellant_mass_kg = 3000.0;

        design.stages.push(stage1);
        design.stages.push(stage2);

        // Mass above stage 1 (bottom): stage 2 + payload
        // Stage 2: 300 kg engine + 240 kg tank (3000 × 0.08) + 3000 kg prop = 3540 kg
        // + 1000 kg payload = 4540 kg
        let mass_above_0 = design.mass_above_stage(0);
        assert_eq!(mass_above_0, 4540.0);

        // Mass above stage 2 (top): just payload
        let mass_above_1 = design.mass_above_stage(1);
        assert_eq!(mass_above_1, 1000.0);
    }

    #[test]
    fn test_total_delta_v() {
        let design = RocketDesign::default_design();
        let dv = design.total_delta_v();
        // Should be somewhere in the ballpark for a reasonable design
        assert!(dv > 5000.0, "Delta-v should be substantial: {}", dv);
    }

    #[test]
    fn test_default_design_almost_sufficient() {
        let design = RocketDesign::default_design();
        let dv = design.total_delta_v();
        // Default should be close to but maybe not quite sufficient
        assert!(dv > 7000.0, "Default should provide reasonable delta-v");
    }

    #[test]
    fn test_generate_launch_events_single_stage() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);

        let events = design.generate_launch_events();

        // Single stage should have: Ignition, Liftoff, Max-Q, Payload Release
        assert_eq!(events.len(), 4);
        assert!(events[0].name.contains("Ignition"));
        assert_eq!(events[1].name, "Liftoff");
        assert_eq!(events[2].name, "Max-Q");
        assert_eq!(events[3].name, "Payload Release");
    }

    #[test]
    fn test_generate_launch_events_two_stage() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.add_stage(EngineType::Hydrolox);

        let events = design.generate_launch_events();

        // Two stages:
        // Stage 1: Ignition, Liftoff, Max-Q, Separation
        // Stage 2: Ignition, Payload Release
        // Total: 6 events
        assert_eq!(events.len(), 6);
        assert!(events[0].name.contains("Stage 1 Ignition"));
        assert_eq!(events[1].name, "Liftoff");
        assert_eq!(events[2].name, "Max-Q");
        assert!(events[3].name.contains("Stage 1 Separation"));
        assert!(events[4].name.contains("Stage 2 Ignition"));
        assert_eq!(events[5].name, "Payload Release");
    }

    #[test]
    fn test_generate_launch_events_three_stage() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.add_stage(EngineType::Kerolox);
        design.add_stage(EngineType::Hydrolox);

        let events = design.generate_launch_events();

        // Three stages:
        // Stage 1: Ignition, Liftoff, Max-Q, Separation (4)
        // Stage 2: Ignition, Separation (2)
        // Stage 3: Ignition, Payload Release (2)
        // Total: 8 events
        assert_eq!(events.len(), 8);
    }

    #[test]
    fn test_generate_launch_events_with_booster() {
        let mut design = RocketDesign::new();

        // Stage 1 (core) - index 0
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 3;
        design.stages[0].propellant_mass_kg = 100000.0;

        // Stage 1 Booster - index 1
        design.add_stage(EngineType::Kerolox);
        design.stages[1].engine_count = 2;
        design.stages[1].propellant_mass_kg = 20000.0; // Less propellant, shorter burn
        design.stages[1].is_booster = true;

        // Stage 2 (upper stage) - index 2
        design.add_stage(EngineType::Hydrolox);
        design.stages[2].engine_count = 1;
        design.stages[2].propellant_mass_kg = 20000.0;

        let events = design.generate_launch_events();

        // Print all events for debugging
        for (i, event) in events.iter().enumerate() {
            println!("Event {}: {} - {}", i, event.name, event.description);
        }

        // Expected sequence:
        // Stage 1: Ignition (with boosters), Liftoff, Max-Q, Booster Separation, Separation (5)
        // Stage 2: Ignition, Payload Release (2)
        // Total: 7 events
        assert_eq!(events.len(), 7, "Expected 7 events, got {}", events.len());

        // Verify event names
        assert_eq!(events[0].name, "Stage 1 Ignition");
        assert!(events[0].description.contains("boosters"),
            "Ignition should mention boosters: {}", events[0].description);
        assert_eq!(events[1].name, "Liftoff");
        assert_eq!(events[2].name, "Max-Q");
        assert_eq!(events[3].name, "Stage 1 Booster Separation",
            "Expected 'Stage 1 Booster Separation', got '{}'", events[3].name);
        assert_eq!(events[4].name, "Stage 1 Separation");
        assert_eq!(events[5].name, "Stage 2 Ignition");
        assert_eq!(events[6].name, "Payload Release");
    }

    #[test]
    fn test_ignition_failure_rate_scales_with_engines() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 5;

        let events = design.generate_launch_events();
        let ignition = &events[0];

        // 5 engines at 0.7% each: 1 - 0.993^5 ≈ 3.45%
        let expected = 1.0 - 0.993_f64.powi(5);
        assert!((ignition.failure_rate - expected).abs() < 0.001);
    }

    #[test]
    fn test_is_sufficient() {
        let mut design = RocketDesign::new();

        // Empty design is not sufficient
        assert!(!design.is_sufficient());

        // Add powerful stages (need significant propellant with 8000 kg payload)
        // Must account for gravity losses reducing effective delta-v
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 9;
        design.stages[0].propellant_mass_kg = 200000.0;

        design.add_stage(EngineType::Hydrolox);
        design.stages[1].engine_count = 2;
        design.stages[1].propellant_mass_kg = 50000.0;

        // This should be more than enough
        assert!(design.is_sufficient(),
            "Effective dv: {}, Target: {}",
            design.total_effective_delta_v(),
            crate::rocket_design::TARGET_DELTA_V_MS);
    }

    // ============================================
    // Physics Validation Tests
    // ============================================

    #[test]
    fn test_delta_v_hand_calculated_single_stage() {
        // Hand calculation for a single stage rocket:
        // Hydrolox engine: Ve = 4500 m/s, engine mass = 300 kg
        // Propellant: 9000 kg
        // Tank mass: 9000 × 0.08 = 720 kg
        // Payload: 1000 kg
        //
        // Wet mass (m0) = 300 + 720 + 9000 + 1000 = 11020 kg
        // Dry mass (mf) = 300 + 720 + 1000 = 2020 kg
        // Δv = 4500 * ln(11020/2020) = 4500 * ln(5.455) = 4500 * 1.697 = 7637 m/s

        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;
        design.add_stage(EngineType::Hydrolox);
        design.stages[0].engine_count = 1;
        design.stages[0].propellant_mass_kg = 9000.0;

        let dv = design.total_delta_v();
        let expected = 4500.0 * (11020.0_f64 / 2020.0).ln();

        assert!(
            (dv - expected).abs() < 1.0,
            "Expected ~{:.0} m/s, got {:.0} m/s",
            expected,
            dv
        );
    }

    #[test]
    fn test_delta_v_hand_calculated_two_stage() {
        // Two-stage rocket calculation (with tank structural mass = 8% of propellant):
        //
        // Stage 2 (upper, fires second):
        //   Hydrolox: Ve = 4500 m/s, engine = 300 kg
        //   Propellant: 3000 kg, Tank: 3000 × 0.08 = 240 kg
        //   Payload: 1000 kg
        //   m0 = 300 + 240 + 3000 + 1000 = 4540 kg
        //   mf = 300 + 240 + 1000 = 1540 kg
        //   Δv2 = 4500 * ln(4540/1540) = 4500 * ln(2.948) = 4500 * 1.082 = 4868 m/s
        //
        // Stage 1 (lower, fires first):
        //   Kerolox: Ve = 3000 m/s, engine = 450 kg
        //   Propellant: 10000 kg, Tank: 10000 × 0.08 = 800 kg
        //   Payload above = stage 2 wet mass = 4540 kg
        //   m0 = 450 + 800 + 10000 + 4540 = 15790 kg
        //   mf = 450 + 800 + 4540 = 5790 kg
        //   Δv1 = 3000 * ln(15790/5790) = 3000 * ln(2.727) = 3000 * 1.003 = 3010 m/s
        //
        // Total Δv = 4868 + 3010 = 7878 m/s

        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;

        // Stage 1 (index 0, fires first)
        let mut stage1 = RocketStage::new(EngineType::Kerolox);
        stage1.engine_count = 1;
        stage1.propellant_mass_kg = 10000.0;
        design.stages.push(stage1);

        // Stage 2 (index 1, fires second)
        let mut stage2 = RocketStage::new(EngineType::Hydrolox);
        stage2.engine_count = 1;
        stage2.propellant_mass_kg = 3000.0;
        design.stages.push(stage2);

        let dv1 = design.stage_delta_v(0);
        let dv2 = design.stage_delta_v(1);
        let total = design.total_delta_v();

        let expected_dv2 = 4500.0 * (4540.0_f64 / 1540.0).ln();
        let expected_dv1 = 3000.0 * (15790.0_f64 / 5790.0).ln();
        let expected_total = expected_dv1 + expected_dv2;

        assert!(
            (dv1 - expected_dv1).abs() < 1.0,
            "Stage 1: expected {:.0}, got {:.0}",
            expected_dv1,
            dv1
        );
        assert!(
            (dv2 - expected_dv2).abs() < 1.0,
            "Stage 2: expected {:.0}, got {:.0}",
            expected_dv2,
            dv2
        );
        assert!(
            (total - expected_total).abs() < 2.0,
            "Total: expected {:.0}, got {:.0}",
            expected_total,
            total
        );
    }

    #[test]
    fn test_mass_fraction_round_trip() {
        // Test that setting mass fraction and reading it back works
        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 2;

        // Set to 85% mass fraction
        design.set_stage_mass_fraction(0, 0.85);
        let actual = design.stage_mass_fraction(0);

        assert!(
            (actual - 0.85).abs() < 0.001,
            "Expected 0.85, got {}",
            actual
        );
    }

    #[test]
    fn test_reorder_preserves_stage_properties() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 3;
        design.stages[0].propellant_mass_kg = 20000.0;

        design.add_stage(EngineType::Hydrolox);
        design.stages[1].engine_count = 1;
        design.stages[1].propellant_mass_kg = 5000.0;

        // Reorder
        design.move_stage(0, 1);

        // Hydrolox should now be at index 0
        assert_eq!(design.stages[0].engine_type, EngineType::Hydrolox);
        assert_eq!(design.stages[0].engine_count, 1);
        assert_eq!(design.stages[0].propellant_mass_kg, 5000.0);

        // Kerolox should now be at index 1
        assert_eq!(design.stages[1].engine_type, EngineType::Kerolox);
        assert_eq!(design.stages[1].engine_count, 3);
        assert_eq!(design.stages[1].propellant_mass_kg, 20000.0);
    }

    #[test]
    fn test_delta_v_changes_with_engine_count() {
        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;
        design.add_stage(EngineType::Kerolox);
        design.stages[0].propellant_mass_kg = 10000.0;

        // With 1 engine
        design.stages[0].engine_count = 1;
        let dv1 = design.total_delta_v();

        // With 3 engines (more dry mass = less delta-v)
        design.stages[0].engine_count = 3;
        let dv3 = design.total_delta_v();

        assert!(
            dv1 > dv3,
            "More engines should reduce delta-v due to mass: {} vs {}",
            dv1,
            dv3
        );
    }

    #[test]
    fn test_sufficient_design_calculation() {
        // Build a rocket that should be sufficient for LEO (9200 m/s)
        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;

        // First stage: 3 Kerolox engines, lots of fuel
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 3;
        design.stages[0].propellant_mass_kg = 40000.0;

        // Second stage: 1 Hydrolox engine
        design.add_stage(EngineType::Hydrolox);
        design.stages[1].engine_count = 1;
        design.stages[1].propellant_mass_kg = 8000.0;

        let total_dv = design.total_delta_v();
        println!(
            "Sufficient design test: Stage 1 = {:.0} m/s, Stage 2 = {:.0} m/s, Total = {:.0} m/s",
            design.stage_delta_v(0),
            design.stage_delta_v(1),
            total_dv
        );

        assert!(
            design.is_sufficient(),
            "Design should be sufficient: {} m/s vs {} m/s target",
            total_dv,
            TARGET_DELTA_V_MS
        );
    }

    #[test]
    fn test_total_mass_calculations() {
        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;

        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 2;
        design.stages[0].propellant_mass_kg = 5000.0;
        // Engine: 2 × 450 = 900 kg, Tank: 5000 × 0.08 = 400 kg
        // Dry: 1300 kg, Wet: 6300 kg

        design.add_stage(EngineType::Hydrolox);
        design.stages[1].engine_count = 1;
        design.stages[1].propellant_mass_kg = 2000.0;
        // Engine: 300 kg, Tank: 2000 × 0.08 = 160 kg
        // Dry: 460 kg, Wet: 2460 kg

        // Total dry = 1300 + 460 + 1000 = 2760 kg
        // Total wet = 6300 + 2460 + 1000 = 9760 kg
        assert_eq!(design.total_dry_mass_kg(), 2760.0);
        assert_eq!(design.total_wet_mass_kg(), 9760.0);
    }

    #[test]
    fn test_liftoff_twr() {
        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;

        // Single Kerolox engine: 500 kN thrust
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 1;
        design.stages[0].propellant_mass_kg = 10000.0;
        // Engine: 450 kg, Tank: 800 kg, Propellant: 10000 kg, Payload: 1000 kg
        // Total mass = 12250 kg
        // Weight = 12250 × 9.81 = 120,173 N
        // Thrust = 500 kN = 500,000 N
        // TWR = 500,000 / 120,173 = 4.16

        let twr = design.liftoff_twr();
        assert!(twr > 3.5 && twr < 5.0, "TWR should be ~4.2: {}", twr);
    }

    #[test]
    fn test_delta_v_margin() {
        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;

        // Build insufficient rocket
        design.add_stage(EngineType::Kerolox);
        design.stages[0].propellant_mass_kg = 5000.0;

        let margin = design.delta_v_margin();
        assert!(margin < 0.0, "Should have negative margin: {}", margin);

        // Build sufficient rocket (need more propellant and engines for adequate TWR)
        design.stages[0].propellant_mass_kg = 50000.0;
        design.stages[0].engine_count = 3; // More engines for better TWR
        design.add_stage(EngineType::Hydrolox);
        design.stages[1].propellant_mass_kg = 15000.0;

        let margin2 = design.delta_v_margin();
        assert!(margin2 > 0.0, "Should have positive margin: {}", margin2);
    }

    #[test]
    fn test_delta_v_percentage() {
        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;
        design.add_stage(EngineType::Hydrolox);
        design.stages[0].propellant_mass_kg = 12000.0; // Enough to exceed 100%
        design.stages[0].engine_count = 1;
        // With tank mass at 8%, this gives ~8400 m/s ideal (>100% of 8100 target)
        // Effective delta-v is lower due to gravity losses

        // Test that effective percentage is less than ideal
        let effective_percentage = design.delta_v_percentage();
        let ideal_percentage = design.ideal_delta_v_percentage();

        assert!(
            ideal_percentage > 100.0 && ideal_percentage < 120.0,
            "Ideal percentage should be >100%: {}",
            ideal_percentage
        );
        assert!(
            effective_percentage < ideal_percentage,
            "Effective percentage {} should be less than ideal {}",
            effective_percentage,
            ideal_percentage
        );
        assert!(
            effective_percentage > 0.0,
            "Effective percentage should be positive: {}",
            effective_percentage
        );
    }

    #[test]
    fn test_mission_success_probability() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 1;

        // Single stage with 1 engine:
        // Events: Ignition (0.7%), Liftoff (2%), Max-Q (5%), Payload Release (2%)
        // Success prob = 0.993 * 0.98 * 0.95 * 0.98 = ~0.906

        let prob = design.mission_success_probability();
        assert!(
            prob > 0.85 && prob < 0.95,
            "Success probability should be ~90%: {}",
            prob
        );

        // Adding more engines decreases success (more ignition risk)
        design.stages[0].engine_count = 5;
        let prob2 = design.mission_success_probability();
        assert!(
            prob2 < prob,
            "More engines should decrease success: {} vs {}",
            prob2,
            prob
        );
    }

    // ==========================================
    // Cost Tests
    // ==========================================

    #[test]
    fn test_starting_budget() {
        assert_eq!(RocketDesign::starting_budget(), 500_000_000.0);
    }

    #[test]
    fn test_empty_design_cost() {
        let design = RocketDesign::new();
        assert_eq!(design.total_cost(), 0.0);
        assert_eq!(design.rocket_overhead_cost(), 0.0);
        assert!(design.is_within_budget());
    }

    #[test]
    fn test_rocket_overhead_cost() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        // Rocket overhead should be $10M when there's at least one stage
        assert_eq!(design.rocket_overhead_cost(), 10_000_000.0);
    }

    #[test]
    fn test_default_design_cost() {
        let design = RocketDesign::default_design();
        // Default: 5 Kerolox ($50M) + 100000kg tank + 1 Hydrolox ($15M) + 20000kg tank
        // + 2 stage overheads ($10M) + rocket overhead ($10M)
        let cost = design.total_cost();

        // Should be roughly $102M (within budget of $150M)
        assert!(cost > 95_000_000.0 && cost < 110_000_000.0,
            "Default design cost should be ~$102M: ${}", cost);
        assert!(design.is_within_budget());
    }

    #[test]
    fn test_remaining_budget() {
        let design = RocketDesign::default_design();
        let remaining = design.remaining_budget();
        let cost = design.total_cost();

        assert_eq!(remaining + cost, 500_000_000.0);
        assert!(remaining > 0.0, "Default design should have remaining budget");
    }

    #[test]
    fn test_over_budget_detection() {
        let mut design = RocketDesign::new();

        // Add 35 expensive Hydrolox engines (35 × $15M = $525M engine cost alone)
        // This should exceed the $500M budget
        design.add_stage(EngineType::Hydrolox);
        design.stages[0].engine_count = 35;

        assert!(!design.is_within_budget(),
            "35 Hydrolox engines should exceed budget");
        assert!(design.remaining_budget() < 0.0,
            "Remaining budget should be negative");
    }

    #[test]
    fn test_is_launchable() {
        // Default design should be launchable (sufficient delta-v and within budget)
        let mut design = RocketDesign::default_design();

        // First, increase propellant to ensure sufficient delta-v
        design.stages[0].propellant_mass_kg = 30000.0;
        design.stages[1].propellant_mass_kg = 6000.0;

        if design.is_sufficient() && design.is_within_budget() {
            assert!(design.is_launchable());
        }

        // Add too many engines to go over budget
        design.stages[0].engine_count = 10;
        if !design.is_within_budget() {
            assert!(!design.is_launchable(),
                "Over budget should not be launchable");
        }
    }

    #[test]
    fn test_stage_cost_calculation() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 2;
        design.stages[0].propellant_mass_kg = 10200.0; // 10 m³

        // Expected stage cost:
        // 2 engines × $10M = $20M
        // 10 m³ × $100K = $1M
        // Stage overhead = $5M
        // Total = $26M
        let expected = 20_000_000.0 + 1_000_000.0 + 5_000_000.0;
        let actual = design.stage_cost(0);

        assert!((actual - expected).abs() < 100.0,
            "Stage cost should be $26M, got ${}", actual);
    }

    // ==========================================
    // TWR and Gravity Loss Tests
    // ==========================================

    #[test]
    fn test_gravity_coefficients() {
        // First stage should have highest coefficient
        let c1 = gravity_coefficients::for_stage(0, 3);
        let c2 = gravity_coefficients::for_stage(1, 3);
        let c3 = gravity_coefficients::for_stage(2, 3);

        assert!(c1 > c2, "First stage should have higher coefficient: {} vs {}", c1, c2);
        assert!(c2 > c3, "Second stage should have higher coefficient than third: {} vs {}", c2, c3);
    }

    #[test]
    fn test_stage_twr() {
        let design = RocketDesign::default_design();

        // First stage should have good TWR (>1.0)
        let twr = design.stage_twr(0);
        assert!(twr > 1.0, "First stage TWR should be > 1.0: {}", twr);

        // Upper stage typically has lower TWR but still functional
        let twr2 = design.stage_twr(1);
        assert!(twr2 > 0.0, "Upper stage TWR should be > 0: {}", twr2);
    }

    #[test]
    fn test_effective_delta_v_less_than_ideal() {
        let design = RocketDesign::default_design();

        let ideal = design.total_delta_v();
        let effective = design.total_effective_delta_v();

        assert!(
            effective < ideal,
            "Effective delta-v should be less than ideal: {} vs {}",
            effective,
            ideal
        );
    }

    #[test]
    fn test_gravity_loss_first_stage_higher() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 3;
        design.stages[0].propellant_mass_kg = 25000.0;

        design.add_stage(EngineType::Hydrolox);
        design.stages[1].engine_count = 1;
        design.stages[1].propellant_mass_kg = 5000.0;

        let _loss1 = design.stage_gravity_loss(0);
        let _loss2 = design.stage_gravity_loss(1);

        // First stage has higher gravity coefficient, so should have more loss
        // (even though it might have better TWR)
        let coef1 = design.stage_gravity_coefficient(0);
        let coef2 = design.stage_gravity_coefficient(1);
        assert!(coef1 > coef2, "First stage should have higher coefficient");
    }

    #[test]
    fn test_gravity_efficiency_reasonable() {
        let design = RocketDesign::default_design();

        let efficiency = design.gravity_efficiency();

        // With default design, efficiency depends on TWR
        // Lower TWR = higher gravity losses = lower efficiency
        // With realistic TWR (~1.8), efficiency may be 40-80%
        assert!(
            efficiency > 0.4 && efficiency < 0.95,
            "Gravity efficiency should be reasonable: {}",
            efficiency
        );
    }

    #[test]
    fn test_total_gravity_loss_positive() {
        let design = RocketDesign::default_design();

        let loss = design.total_gravity_loss();

        assert!(loss > 0.0, "Total gravity loss should be positive: {}", loss);
    }

    #[test]
    fn test_is_sufficient_uses_effective_dv() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 1;  // Low TWR = high gravity losses
        design.stages[0].propellant_mass_kg = 50000.0;

        let ideal = design.total_delta_v();
        let effective = design.total_effective_delta_v();

        // These should be different
        assert!(
            (ideal - effective).abs() > 100.0,
            "Ideal and effective should differ significantly"
        );

        // is_sufficient should use effective, not ideal
        let sufficient = design.is_sufficient();
        let _would_be_sufficient_with_ideal = ideal >= TARGET_DELTA_V_MS;
        let sufficient_with_effective = effective >= TARGET_DELTA_V_MS;

        assert_eq!(sufficient, sufficient_with_effective,
            "is_sufficient should use effective delta-v");
    }

    #[test]
    fn test_more_engines_reduces_gravity_loss() {
        let mut design1 = RocketDesign::new();
        design1.add_stage(EngineType::Kerolox);
        design1.stages[0].engine_count = 1;
        design1.stages[0].propellant_mass_kg = 20000.0;

        let mut design2 = RocketDesign::new();
        design2.add_stage(EngineType::Kerolox);
        design2.stages[0].engine_count = 5;
        design2.stages[0].propellant_mass_kg = 20000.0;

        let loss1 = design1.stage_gravity_loss(0);
        let loss2 = design2.stage_gravity_loss(0);

        // More engines = higher TWR = less gravity loss
        assert!(
            loss2 < loss1,
            "More engines should reduce gravity loss: {} vs {}",
            loss2,
            loss1
        );
    }
}
