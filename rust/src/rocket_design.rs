use crate::engine::EngineType;
use crate::stage::RocketStage;

/// Mission constants for LEO insertion
pub const TARGET_DELTA_V_MS: f64 = 9200.0; // 7800 velocity + 1200 gravity + 200 drag
pub const DEFAULT_PAYLOAD_KG: f64 = 1000.0;

/// A complete rocket design with multiple stages
#[derive(Debug, Clone)]
pub struct RocketDesign {
    /// Stages from bottom (first to fire) to top (last to fire)
    /// Index 0 is the first stage (bottom), highest index is last stage (top)
    pub stages: Vec<RocketStage>,
    /// Payload mass in kilograms
    pub payload_mass_kg: f64,

    // Future-proofing fields (from Rocket Tycoon 1.0 vision)

    /// Name of this rocket design
    pub name: String,
    /// Number of times this design has been launched (for reliability progression)
    pub launch_count: u32,
}

impl RocketDesign {
    /// Create a new empty rocket design
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            payload_mass_kg: DEFAULT_PAYLOAD_KG,
            name: "Unnamed Rocket".to_string(),
            launch_count: 0,
        }
    }

    /// Create a default two-stage rocket that's almost sufficient for LEO
    pub fn default_design() -> Self {
        let mut design = Self::new();
        design.name = "Default Rocket".to_string();

        // First stage: 3 Kerolox engines
        let mut stage1 = RocketStage::new(EngineType::Kerolox);
        stage1.engine_count = 3;
        stage1.propellant_mass_kg = 25000.0;

        // Second stage: 1 Hydrolox engine
        let mut stage2 = RocketStage::new(EngineType::Hydrolox);
        stage2.engine_count = 1;
        stage2.propellant_mass_kg = 5000.0;

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

    /// Calculate the mass above a given stage (payload + all upper stages)
    /// Stage 0 is the bottom, so it carries the most mass
    pub fn mass_above_stage(&self, stage_index: usize) -> f64 {
        let mut mass = self.payload_mass_kg;

        // Add mass of all stages above this one
        for i in (stage_index + 1)..self.stages.len() {
            mass += self.stages[i].wet_mass_kg();
        }

        mass
    }

    /// Calculate delta-v for a single stage
    pub fn stage_delta_v(&self, stage_index: usize) -> f64 {
        if stage_index >= self.stages.len() {
            return 0.0;
        }

        let payload = self.mass_above_stage(stage_index);
        self.stages[stage_index].delta_v(payload)
    }

    /// Calculate total delta-v for the entire rocket
    /// Stages fire from bottom (index 0) to top
    pub fn total_delta_v(&self) -> f64 {
        let mut total = 0.0;
        for i in 0..self.stages.len() {
            total += self.stage_delta_v(i);
        }
        total
    }

    /// Check if the design provides sufficient delta-v for LEO
    pub fn is_sufficient(&self) -> bool {
        self.total_delta_v() >= TARGET_DELTA_V_MS
    }

    /// Get the target delta-v
    pub fn target_delta_v(&self) -> f64 {
        TARGET_DELTA_V_MS
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
    pub fn liftoff_twr(&self) -> f64 {
        if self.stages.is_empty() {
            return 0.0;
        }

        let first_stage = &self.stages[0];
        let thrust_n = first_stage.total_thrust_kn() * 1000.0; // kN to N
        let weight_n = self.total_wet_mass_kg() * 9.81; // kg to N (Earth gravity)

        thrust_n / weight_n
    }

    /// Calculate how much delta-v margin we have (positive = excess, negative = shortfall)
    pub fn delta_v_margin(&self) -> f64 {
        self.total_delta_v() - TARGET_DELTA_V_MS
    }

    /// Calculate delta-v as a percentage of target (100% = exactly sufficient)
    pub fn delta_v_percentage(&self) -> f64 {
        if TARGET_DELTA_V_MS == 0.0 {
            return 0.0;
        }
        (self.total_delta_v() / TARGET_DELTA_V_MS) * 100.0
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
    /// First stage: Ignition → Liftoff → MaxQ → Separation
    /// Middle stages: Ignition → Separation
    /// Last stage: Ignition → Orbital Insertion
    pub fn generate_launch_events(&self) -> Vec<LaunchEvent> {
        let mut events = Vec::new();

        for (i, stage) in self.stages.iter().enumerate() {
            let is_first = i == 0;
            let is_last = i == self.stages.len() - 1;
            let failure_rate = stage.ignition_failure_rate();

            // Ignition event for all stages
            events.push(LaunchEvent {
                name: format!("Stage {} Ignition", i + 1),
                description: format!(
                    "Stage {} engine{} ignit{}",
                    i + 1,
                    if stage.engine_count > 1 { "s" } else { "" },
                    if stage.engine_count > 1 { "e" } else { "es" }
                ),
                failure_rate,
                rocket_stage: i,
            });

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

            if !is_last {
                // All stages except last get separation
                events.push(LaunchEvent {
                    name: format!("Stage {} Separation", i + 1),
                    description: format!("Stage {} separates", i + 1),
                    failure_rate: 0.03, // Fixed 3% for separation
                    rocket_stage: i,
                });
            } else {
                // Last stage gets orbital insertion
                events.push(LaunchEvent {
                    name: "Orbital Insertion".to_string(),
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
        // Stage 2: 300 kg engine + 3000 kg prop = 3300 kg + 1000 kg payload = 4300 kg
        let mass_above_0 = design.mass_above_stage(0);
        assert_eq!(mass_above_0, 4300.0);

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

        // Single stage should have: Ignition, Liftoff, Max-Q, Orbital Insertion
        assert_eq!(events.len(), 4);
        assert!(events[0].name.contains("Ignition"));
        assert_eq!(events[1].name, "Liftoff");
        assert_eq!(events[2].name, "Max-Q");
        assert_eq!(events[3].name, "Orbital Insertion");
    }

    #[test]
    fn test_generate_launch_events_two_stage() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.add_stage(EngineType::Hydrolox);

        let events = design.generate_launch_events();

        // Two stages:
        // Stage 1: Ignition, Liftoff, Max-Q, Separation
        // Stage 2: Ignition, Orbital Insertion
        // Total: 6 events
        assert_eq!(events.len(), 6);
        assert!(events[0].name.contains("Stage 1 Ignition"));
        assert_eq!(events[1].name, "Liftoff");
        assert_eq!(events[2].name, "Max-Q");
        assert!(events[3].name.contains("Stage 1 Separation"));
        assert!(events[4].name.contains("Stage 2 Ignition"));
        assert_eq!(events[5].name, "Orbital Insertion");
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
        // Stage 3: Ignition, Orbital Insertion (2)
        // Total: 8 events
        assert_eq!(events.len(), 8);
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

        // Add a powerful stage
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 5;
        design.stages[0].propellant_mass_kg = 50000.0;

        design.add_stage(EngineType::Hydrolox);
        design.stages[1].propellant_mass_kg = 10000.0;

        // This should be more than enough
        assert!(design.is_sufficient());
    }

    // ============================================
    // Physics Validation Tests
    // ============================================

    #[test]
    fn test_delta_v_hand_calculated_single_stage() {
        // Hand calculation for a single stage rocket:
        // Hydrolox engine: Ve = 4500 m/s, engine mass = 300 kg
        // Propellant: 9000 kg
        // Payload: 1000 kg
        //
        // Wet mass (m0) = 300 + 9000 + 1000 = 10300 kg
        // Dry mass (mf) = 300 + 1000 = 1300 kg
        // Δv = 4500 * ln(10300/1300) = 4500 * ln(7.923) = 4500 * 2.070 = 9315 m/s

        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;
        design.add_stage(EngineType::Hydrolox);
        design.stages[0].engine_count = 1;
        design.stages[0].propellant_mass_kg = 9000.0;

        let dv = design.total_delta_v();
        let expected = 4500.0 * (10300.0_f64 / 1300.0).ln();

        assert!(
            (dv - expected).abs() < 1.0,
            "Expected ~{:.0} m/s, got {:.0} m/s",
            expected,
            dv
        );
    }

    #[test]
    fn test_delta_v_hand_calculated_two_stage() {
        // Two-stage rocket calculation:
        //
        // Stage 2 (upper, fires second):
        //   Hydrolox: Ve = 4500 m/s, engine = 300 kg
        //   Propellant: 3000 kg
        //   Payload: 1000 kg
        //   m0 = 300 + 3000 + 1000 = 4300 kg
        //   mf = 300 + 1000 = 1300 kg
        //   Δv2 = 4500 * ln(4300/1300) = 4500 * ln(3.308) = 4500 * 1.196 = 5384 m/s
        //
        // Stage 1 (lower, fires first):
        //   Kerolox: Ve = 3000 m/s, engine = 450 kg
        //   Propellant: 10000 kg
        //   Payload above = stage 2 wet mass = 4300 kg
        //   m0 = 450 + 10000 + 4300 = 14750 kg
        //   mf = 450 + 4300 = 4750 kg
        //   Δv1 = 3000 * ln(14750/4750) = 3000 * ln(3.105) = 3000 * 1.133 = 3399 m/s
        //
        // Total Δv = 5384 + 3399 = 8783 m/s

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

        let expected_dv2 = 4500.0 * (4300.0_f64 / 1300.0).ln();
        let expected_dv1 = 3000.0 * (14750.0_f64 / 4750.0).ln();
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
        // Dry: 2 * 450 = 900 kg, Wet: 900 + 5000 = 5900 kg

        design.add_stage(EngineType::Hydrolox);
        design.stages[1].engine_count = 1;
        design.stages[1].propellant_mass_kg = 2000.0;
        // Dry: 300 kg, Wet: 300 + 2000 = 2300 kg

        // Total dry = 900 + 300 + 1000 = 2200 kg
        // Total wet = 5900 + 2300 + 1000 = 9200 kg
        assert_eq!(design.total_dry_mass_kg(), 2200.0);
        assert_eq!(design.total_wet_mass_kg(), 9200.0);
    }

    #[test]
    fn test_liftoff_twr() {
        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;

        // Single Kerolox engine: 1000 kN thrust
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 1;
        design.stages[0].propellant_mass_kg = 10000.0;
        // Wet mass = 450 + 10000 + 1000 = 11450 kg
        // Weight = 11450 * 9.81 = 112324.5 N
        // Thrust = 1000 * 1000 = 1000000 N
        // TWR = 1000000 / 112324.5 = 8.9

        let twr = design.liftoff_twr();
        assert!(twr > 8.0 && twr < 10.0, "TWR should be ~8.9: {}", twr);
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

        // Build sufficient rocket
        design.stages[0].propellant_mass_kg = 50000.0;
        design.add_stage(EngineType::Hydrolox);
        design.stages[1].propellant_mass_kg = 10000.0;

        let margin2 = design.delta_v_margin();
        assert!(margin2 > 0.0, "Should have positive margin: {}", margin2);
    }

    #[test]
    fn test_delta_v_percentage() {
        let mut design = RocketDesign::new();
        design.payload_mass_kg = 1000.0;
        design.add_stage(EngineType::Hydrolox);
        design.stages[0].propellant_mass_kg = 9000.0;
        // This gives ~9315 m/s, which is ~101% of 9200 target

        let percentage = design.delta_v_percentage();
        assert!(
            percentage > 100.0 && percentage < 110.0,
            "Percentage should be ~101%: {}",
            percentage
        );
    }

    #[test]
    fn test_mission_success_probability() {
        let mut design = RocketDesign::new();
        design.add_stage(EngineType::Kerolox);
        design.stages[0].engine_count = 1;

        // Single stage with 1 engine:
        // Events: Ignition (0.7%), Liftoff (2%), Max-Q (5%), Orbital Insertion (2%)
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
}
