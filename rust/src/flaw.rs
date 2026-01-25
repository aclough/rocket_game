use rand::Rng;

/// Represents a hidden defect in a rocket that can cause failures
#[derive(Clone, Debug)]
pub struct Flaw {
    /// Unique identifier for this flaw instance
    pub id: u32,
    /// Type of flaw (Engine or Design)
    pub flaw_type: FlawType,
    /// Human-readable name
    pub name: String,
    /// Detailed description of the flaw
    pub description: String,
    /// Base chance to cause failure (0.05 - 0.20)
    pub failure_rate: f64,
    /// Discovery modifier for testing (0.6 - 1.0)
    /// Higher means easier to discover during testing
    pub testing_modifier: f64,
    /// Which launch event type triggers this flaw
    pub trigger_event_type: FlawTrigger,
    /// Whether the flaw has been discovered (through testing or flight failure)
    pub discovered: bool,
    /// Whether the flaw has been fixed
    pub fixed: bool,
    /// For engine flaws: which engine type this flaw is associated with (index)
    /// None for design flaws
    pub engine_type_index: Option<i32>,
}

/// Type of flaw - determines what kind of testing discovers it
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlawType {
    /// Engine flaws are discovered by engine testing
    /// They trigger at ignition events
    Engine,
    /// Design flaws are discovered by rocket testing
    /// They trigger at various flight phases
    Design,
}

/// Which launch event triggers a flaw
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlawTrigger {
    /// Any ignition event (stage ignition)
    Ignition,
    /// Liftoff event
    Liftoff,
    /// Maximum dynamic pressure
    MaxQ,
    /// Any separation event (stage or booster)
    Separation,
    /// Final payload release (previously called orbital insertion)
    PayloadRelease,
}

impl FlawTrigger {
    /// Check if this trigger matches a given event name
    pub fn matches_event(&self, event_name: &str) -> bool {
        let event_lower = event_name.to_lowercase();
        match self {
            FlawTrigger::Ignition => event_lower.contains("ignition"),
            FlawTrigger::Liftoff => event_lower.contains("liftoff"),
            FlawTrigger::MaxQ => event_lower.contains("max-q") || event_lower.contains("max q"),
            FlawTrigger::Separation => event_lower.contains("separation"),
            FlawTrigger::PayloadRelease => {
                event_lower.contains("payload release") || event_lower.contains("orbital insertion")
            }
        }
    }
}

/// Template for generating flaws
#[derive(Clone, Debug)]
pub struct FlawTemplate {
    pub name: &'static str,
    pub description: &'static str,
    pub flaw_type: FlawType,
    pub failure_rate: f64,
    pub testing_modifier: f64,
    pub trigger_event_type: FlawTrigger,
}

/// Engine flaw templates - discovered by engine testing, trigger at ignition
pub const ENGINE_FLAW_TEMPLATES: &[FlawTemplate] = &[
    FlawTemplate {
        name: "Turbopump Bearing Defect",
        description: "Microscopic imperfections in turbopump bearings cause premature wear and potential seizure during high-speed operation.",
        flaw_type: FlawType::Engine,
        failure_rate: 0.12,
        testing_modifier: 0.9,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Combustion Chamber Crack",
        description: "Hairline fractures in the combustion chamber wall can propagate under thermal stress, leading to catastrophic failure.",
        flaw_type: FlawType::Engine,
        failure_rate: 0.15,
        testing_modifier: 0.7,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Fuel Injector Misalignment",
        description: "Slight misalignment in fuel injectors causes uneven combustion, hot spots, and potential burnthrough.",
        flaw_type: FlawType::Engine,
        failure_rate: 0.10,
        testing_modifier: 0.85,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Gimbal Actuator Weakness",
        description: "Hydraulic actuators for engine gimbaling have insufficient strength for the required thrust vector control loads.",
        flaw_type: FlawType::Engine,
        failure_rate: 0.08,
        testing_modifier: 0.95,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Propellant Valve Seal",
        description: "Main propellant valve seals degrade under cryogenic conditions, causing leaks and pressure loss.",
        flaw_type: FlawType::Engine,
        failure_rate: 0.10,
        testing_modifier: 0.8,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Igniter Reliability Issue",
        description: "Redundant igniters have common-mode failure vulnerability under certain environmental conditions.",
        flaw_type: FlawType::Engine,
        failure_rate: 0.09,
        testing_modifier: 0.88,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Turbine Blade Resonance",
        description: "Turbine blades resonate at certain RPM ranges, causing metal fatigue and eventual failure.",
        flaw_type: FlawType::Engine,
        failure_rate: 0.11,
        testing_modifier: 0.75,
        trigger_event_type: FlawTrigger::Ignition,
    },
];

/// Design flaw templates - discovered by rocket testing, trigger at various phases
pub const DESIGN_FLAW_TEMPLATES: &[FlawTemplate] = &[
    FlawTemplate {
        name: "Structural Resonance",
        description: "Vehicle natural frequency matches aerodynamic buffet frequency during max-Q, causing destructive oscillations.",
        flaw_type: FlawType::Design,
        failure_rate: 0.15,
        testing_modifier: 0.7,
        trigger_event_type: FlawTrigger::MaxQ,
    },
    FlawTemplate {
        name: "Stage Separation Bolt Defect",
        description: "Explosive bolts for stage separation have inconsistent charge, leading to asymmetric separation.",
        flaw_type: FlawType::Design,
        failure_rate: 0.12,
        testing_modifier: 0.85,
        trigger_event_type: FlawTrigger::Separation,
    },
    FlawTemplate {
        name: "Guidance Software Bug",
        description: "Edge case in guidance algorithms causes incorrect attitude determination under specific orbital conditions.",
        flaw_type: FlawType::Design,
        failure_rate: 0.10,
        testing_modifier: 0.6,
        trigger_event_type: FlawTrigger::PayloadRelease,
    },
    FlawTemplate {
        name: "Propellant Slosh Instability",
        description: "Propellant sloshing in partially-filled tanks couples with control system, causing loss of control.",
        flaw_type: FlawType::Design,
        failure_rate: 0.08,
        testing_modifier: 0.75,
        trigger_event_type: FlawTrigger::MaxQ,
    },
    FlawTemplate {
        name: "Thermal Protection Gap",
        description: "Gaps in aerodynamic heating protection allow hot gases to damage structure during ascent.",
        flaw_type: FlawType::Design,
        failure_rate: 0.10,
        testing_modifier: 0.8,
        trigger_event_type: FlawTrigger::MaxQ,
    },
    FlawTemplate {
        name: "Interstage Coupler Flaw",
        description: "Interstage structure has insufficient strength for the separation loads under all flight conditions.",
        flaw_type: FlawType::Design,
        failure_rate: 0.12,
        testing_modifier: 0.9,
        trigger_event_type: FlawTrigger::Separation,
    },
    FlawTemplate {
        name: "Avionics Thermal Margin",
        description: "Flight computer cooling is inadequate for extended powered flight, causing thermal shutdown.",
        flaw_type: FlawType::Design,
        failure_rate: 0.07,
        testing_modifier: 0.65,
        trigger_event_type: FlawTrigger::PayloadRelease,
    },
    FlawTemplate {
        name: "Fairing Separation Failure",
        description: "Payload fairing separation system has unreliable pyrotechnic actuators.",
        flaw_type: FlawType::Design,
        failure_rate: 0.09,
        testing_modifier: 0.85,
        trigger_event_type: FlawTrigger::Separation,
    },
    FlawTemplate {
        name: "Liftoff Clamp Release",
        description: "Hold-down clamps release sequence has timing issues that can tip the vehicle.",
        flaw_type: FlawType::Design,
        failure_rate: 0.06,
        testing_modifier: 0.92,
        trigger_event_type: FlawTrigger::Liftoff,
    },
    FlawTemplate {
        name: "Acoustic Vibration Damage",
        description: "Launch acoustic environment exceeds component qualification levels in some areas.",
        flaw_type: FlawType::Design,
        failure_rate: 0.08,
        testing_modifier: 0.78,
        trigger_event_type: FlawTrigger::Liftoff,
    },
];

impl Flaw {
    /// Create a new flaw from a template with a unique ID
    pub fn from_template(template: &FlawTemplate, id: u32) -> Self {
        Self {
            id,
            flaw_type: template.flaw_type.clone(),
            name: template.name.to_string(),
            description: template.description.to_string(),
            failure_rate: template.failure_rate,
            testing_modifier: template.testing_modifier,
            trigger_event_type: template.trigger_event_type.clone(),
            discovered: false,
            fixed: false,
            engine_type_index: None,
        }
    }

    /// Create a new engine flaw from a template with a specific engine type
    pub fn from_template_with_engine(template: &FlawTemplate, id: u32, engine_type: i32) -> Self {
        Self {
            id,
            flaw_type: template.flaw_type.clone(),
            name: template.name.to_string(),
            description: template.description.to_string(),
            failure_rate: template.failure_rate,
            testing_modifier: template.testing_modifier,
            trigger_event_type: template.trigger_event_type.clone(),
            discovered: false,
            fixed: false,
            engine_type_index: Some(engine_type),
        }
    }

    /// Check if this flaw is active (not fixed) and should affect launches
    pub fn is_active(&self) -> bool {
        !self.fixed
    }

    /// Check if this flaw can cause failure at a given event
    pub fn can_trigger_at(&self, event_name: &str) -> bool {
        self.is_active() && self.trigger_event_type.matches_event(event_name)
    }

    /// Get the effective failure rate (0 if fixed)
    pub fn effective_failure_rate(&self) -> f64 {
        if self.fixed {
            0.0
        } else {
            self.failure_rate
        }
    }

    /// Calculate the probability of discovering this flaw during testing
    pub fn discovery_probability(&self) -> f64 {
        if self.discovered || self.fixed {
            0.0
        } else {
            self.failure_rate * self.testing_modifier
        }
    }
}

/// Generate a set of flaws for a rocket design
pub struct FlawGenerator {
    next_id: u32,
}

impl FlawGenerator {
    pub fn new() -> Self {
        Self { next_id: 1 }
    }

    /// Generate flaws for a rocket based on its configuration
    /// Returns a vector of flaws
    pub fn generate_flaws(&mut self, total_engines: u32, stage_count: usize) -> Vec<Flaw> {
        // Call the extended version with empty engine types (backward compatible)
        self.generate_flaws_with_engine_types(total_engines, stage_count, &[])
    }

    /// Generate flaws for a rocket with specific engine types
    /// engine_types is a list of (engine_type_index, engine_count) pairs
    pub fn generate_flaws_with_engine_types(
        &mut self,
        total_engines: u32,
        stage_count: usize,
        engine_types: &[(i32, u32)],
    ) -> Vec<Flaw> {
        let mut rng = rand::thread_rng();
        let mut flaws = Vec::new();

        // If we have engine type info, generate flaws per engine type
        if !engine_types.is_empty() {
            for &(engine_type, count) in engine_types {
                // 1-2 flaws per engine type, scaled by count
                let flaw_count = 1 + (count / 3).min(1) as usize;
                let templates = self.select_random_templates(
                    ENGINE_FLAW_TEMPLATES,
                    flaw_count,
                    &mut rng,
                );
                for template in templates {
                    flaws.push(Flaw::from_template_with_engine(template, self.next_id, engine_type));
                    self.next_id += 1;
                }
            }
        } else {
            // Fallback: generate engine flaws without type association
            let engine_flaw_count = 2 + (total_engines / 5).min(2) as usize;
            let engine_templates = self.select_random_templates(
                ENGINE_FLAW_TEMPLATES,
                engine_flaw_count,
                &mut rng,
            );
            for template in engine_templates {
                flaws.push(Flaw::from_template(template, self.next_id));
                self.next_id += 1;
            }
        }

        // Design flaws: 2-4 based on stage count
        // More stages = more potential design flaws
        let design_flaw_count = 2 + stage_count.min(2);
        let design_templates = self.select_random_templates(
            DESIGN_FLAW_TEMPLATES,
            design_flaw_count,
            &mut rng,
        );
        for template in design_templates {
            flaws.push(Flaw::from_template(template, self.next_id));
            self.next_id += 1;
        }

        flaws
    }

    /// Select random templates without replacement
    fn select_random_templates<'a>(
        &self,
        templates: &'a [FlawTemplate],
        count: usize,
        rng: &mut impl Rng,
    ) -> Vec<&'a FlawTemplate> {
        let count = count.min(templates.len());
        let mut indices: Vec<usize> = (0..templates.len()).collect();

        // Fisher-Yates shuffle for first `count` elements
        for i in 0..count {
            let j = rng.gen_range(i..templates.len());
            indices.swap(i, j);
        }

        indices[..count].iter().map(|&i| &templates[i]).collect()
    }
}

impl Default for FlawGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate the total failure contribution from flaws for a given event
/// stage_engine_type: the engine type index of the stage (for filtering engine flaws)
pub fn calculate_flaw_failure_rate(flaws: &[Flaw], event_name: &str, stage_engine_type: Option<i32>) -> f64 {
    flaws
        .iter()
        .filter(|f| {
            if !f.can_trigger_at(event_name) {
                return false;
            }
            // For engine flaws, only count if engine type matches the stage
            if f.flaw_type == FlawType::Engine {
                match (f.engine_type_index, stage_engine_type) {
                    (Some(flaw_engine), Some(stage_engine)) => flaw_engine == stage_engine,
                    _ => false,
                }
            } else {
                true
            }
        })
        .map(|f| f.effective_failure_rate())
        .sum()
}

/// Run a test and return which flaws were discovered
pub fn run_test(flaws: &mut [Flaw], flaw_type: FlawType) -> Vec<String> {
    let mut rng = rand::thread_rng();
    let mut discovered = Vec::new();

    for flaw in flaws.iter_mut() {
        if flaw.flaw_type == flaw_type && !flaw.discovered && !flaw.fixed {
            let discovery_chance = flaw.discovery_probability();
            let roll: f64 = rng.gen();
            if roll < discovery_chance {
                flaw.discovered = true;
                discovered.push(flaw.name.clone());
            }
        }
    }

    discovered
}

/// Run an engine test for a specific engine type
/// Returns names of flaws discovered
pub fn run_engine_test_for_type(flaws: &mut [Flaw], engine_type_index: i32) -> Vec<String> {
    let mut rng = rand::thread_rng();
    let mut discovered = Vec::new();

    for flaw in flaws.iter_mut() {
        // Only test engine flaws for the specified engine type
        if flaw.flaw_type == FlawType::Engine
            && flaw.engine_type_index == Some(engine_type_index)
            && !flaw.discovered
            && !flaw.fixed
        {
            let discovery_chance = flaw.discovery_probability();
            let roll: f64 = rng.gen();
            if roll < discovery_chance {
                flaw.discovered = true;
                discovered.push(flaw.name.clone());
            }
        }
    }

    discovered
}

/// Mark a specific flaw as discovered (e.g., after a launch failure)
/// Returns the flaw name if found
pub fn mark_flaw_discovered(flaws: &mut [Flaw], flaw_id: u32) -> Option<String> {
    for flaw in flaws.iter_mut() {
        if flaw.id == flaw_id && !flaw.discovered {
            flaw.discovered = true;
            return Some(flaw.name.clone());
        }
    }
    None
}

/// Find which flaw caused a failure at the given event
/// Called AFTER a failure has already been determined
/// Picks a flaw weighted by failure rate (higher rate = more likely to be the cause)
/// stage_engine_type: the engine type index of the stage that failed (for filtering engine flaws)
/// Returns the flaw ID of the responsible flaw, or None if no flaws could have triggered
pub fn check_flaw_trigger(flaws: &[Flaw], event_name: &str, stage_engine_type: Option<i32>) -> Option<u32> {
    let mut rng = rand::thread_rng();

    // Get all active flaws that can trigger at this event, with their effective rates
    // For engine flaws, only include if the engine type matches the stage's engine type
    let triggerable: Vec<(&Flaw, f64)> = flaws
        .iter()
        .filter(|f| {
            if !f.can_trigger_at(event_name) {
                return false;
            }
            // For engine flaws, check that the engine type matches
            if f.flaw_type == FlawType::Engine {
                match (f.engine_type_index, stage_engine_type) {
                    (Some(flaw_engine), Some(stage_engine)) => flaw_engine == stage_engine,
                    _ => false, // Engine flaw without type info or stage without type info
                }
            } else {
                // Design flaws can trigger on any stage
                true
            }
        })
        .map(|f| (f, f.effective_failure_rate()))
        .filter(|(_, rate)| *rate > 0.0)
        .collect();

    if triggerable.is_empty() {
        return None;
    }

    // Calculate total failure rate from flaws
    let total_rate: f64 = triggerable.iter().map(|(_, rate)| rate).sum();

    if total_rate <= 0.0 {
        return None;
    }

    // Pick a flaw weighted by its contribution to the total failure rate
    let roll: f64 = rng.gen::<f64>() * total_rate;
    let mut cumulative = 0.0;

    for (flaw, rate) in &triggerable {
        cumulative += rate;
        if roll < cumulative {
            return Some(flaw.id);
        }
    }

    // Fallback: return the last flaw (shouldn't happen due to floating point)
    triggerable.last().map(|(f, _)| f.id)
}

/// Estimate the success rate based on active flaws
/// This is a rough estimate shown to the player
pub fn estimate_success_rate(flaws: &[Flaw], base_success_rate: f64) -> f64 {
    // Sum up failure rates from unfixed flaws
    let total_flaw_failure: f64 = flaws
        .iter()
        .filter(|f| !f.fixed)
        .map(|f| f.failure_rate)
        .sum();

    // Convert to success rate (simplified model)
    // Each flaw is roughly independent
    let flaw_success_rate = 1.0 - total_flaw_failure.min(0.95);
    base_success_rate * flaw_success_rate
}

/// Get the approximate count of unknown (undiscovered and unfixed) flaws
/// Returns a fuzzy estimate, not the exact count
pub fn estimate_unknown_flaw_count(flaws: &[Flaw]) -> (usize, usize) {
    let unknown_count = flaws
        .iter()
        .filter(|f| !f.discovered && !f.fixed)
        .count();

    // Return a range: (min, max) that includes the actual count
    // This gives the player a rough idea without exact information
    let min = if unknown_count > 2 { unknown_count - 2 } else { 0 };
    let max = unknown_count + 2;
    (min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flaw_from_template() {
        let template = &ENGINE_FLAW_TEMPLATES[0];
        let flaw = Flaw::from_template(template, 1);

        assert_eq!(flaw.id, 1);
        assert_eq!(flaw.flaw_type, FlawType::Engine);
        assert_eq!(flaw.name, template.name);
        assert!(!flaw.discovered);
        assert!(!flaw.fixed);
    }

    #[test]
    fn test_flaw_is_active() {
        let mut flaw = Flaw::from_template(&ENGINE_FLAW_TEMPLATES[0], 1);

        assert!(flaw.is_active());

        flaw.fixed = true;
        assert!(!flaw.is_active());
    }

    #[test]
    fn test_flaw_trigger_matches() {
        let ignition_flaw = Flaw::from_template(&ENGINE_FLAW_TEMPLATES[0], 1);
        assert!(ignition_flaw.can_trigger_at("Stage 1 Ignition"));
        assert!(!ignition_flaw.can_trigger_at("Liftoff"));

        let maxq_flaw = Flaw::from_template(&DESIGN_FLAW_TEMPLATES[0], 2);
        assert!(maxq_flaw.can_trigger_at("Max-Q"));
        assert!(!maxq_flaw.can_trigger_at("Stage 1 Ignition"));
    }

    #[test]
    fn test_effective_failure_rate() {
        let mut flaw = Flaw::from_template(&ENGINE_FLAW_TEMPLATES[0], 1);

        let original_rate = flaw.failure_rate;
        assert!(flaw.effective_failure_rate() > 0.0);
        assert_eq!(flaw.effective_failure_rate(), original_rate);

        flaw.fixed = true;
        assert_eq!(flaw.effective_failure_rate(), 0.0);
    }

    #[test]
    fn test_discovery_probability() {
        let mut flaw = Flaw::from_template(&ENGINE_FLAW_TEMPLATES[0], 1);

        let prob = flaw.discovery_probability();
        assert!(prob > 0.0);
        assert!(prob <= flaw.failure_rate); // Can't be more than failure rate

        flaw.discovered = true;
        assert_eq!(flaw.discovery_probability(), 0.0);
    }

    #[test]
    fn test_flaw_generator() {
        let mut generator = FlawGenerator::new();
        let flaws = generator.generate_flaws(5, 2);

        // Should generate some engine flaws and some design flaws
        let engine_flaws = flaws.iter().filter(|f| f.flaw_type == FlawType::Engine).count();
        let design_flaws = flaws.iter().filter(|f| f.flaw_type == FlawType::Design).count();

        assert!(engine_flaws >= 2);
        assert!(design_flaws >= 2);

        // All flaws should have unique IDs
        let ids: Vec<u32> = flaws.iter().map(|f| f.id).collect();
        let unique_ids: std::collections::HashSet<u32> = ids.iter().cloned().collect();
        assert_eq!(ids.len(), unique_ids.len());
    }

    #[test]
    fn test_calculate_flaw_failure_rate() {
        let mut generator = FlawGenerator::new();
        // Generate flaws with engine type info
        let flaws = generator.generate_flaws_with_engine_types(3, 2, &[(0, 3)]);

        // Get ignition failure rate for engine type 0
        let ignition_rate = calculate_flaw_failure_rate(&flaws, "Stage 1 Ignition", Some(0));

        // Should be sum of all engine flaw failure rates for engine type 0
        let expected: f64 = flaws
            .iter()
            .filter(|f| f.flaw_type == FlawType::Engine && f.engine_type_index == Some(0))
            .map(|f| f.failure_rate)
            .sum();

        assert!((ignition_rate - expected).abs() < 0.001);
    }

    #[test]
    fn test_estimate_success_rate() {
        let mut generator = FlawGenerator::new();
        let flaws = generator.generate_flaws(3, 2);

        let success = estimate_success_rate(&flaws, 0.9);

        // With flaws, success rate should be lower than base
        assert!(success < 0.9);
        assert!(success > 0.0);
    }
}
