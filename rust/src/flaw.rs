use rand::Rng;
use rand_distr::{Distribution, LogNormal};

use crate::engine_design::FuelType;
use crate::engineering_team::TESTING_WORK;

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
    /// Base chance to cause failure (0.005 - 1.0), drawn from global distribution
    pub failure_rate: f64,
    /// Discovery modifier for testing (0.1 - 1.0), drawn uniformly
    /// Higher means easier to discover during testing
    pub testing_modifier: f64,
    /// Which launch event type triggers this flaw
    pub trigger_event_type: FlawTrigger,
    /// Whether the flaw has been discovered (through testing or flight failure)
    pub discovered: bool,
    /// Whether the flaw has been fixed
    pub fixed: bool,
    /// For engine flaws: which engine design this flaw is associated with (index into company's engine_designs)
    /// None for design flaws
    pub engine_design_id: Option<usize>,
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

/// Category of engine flaws - determines which flaw template list to use
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FlawCategory {
    /// Liquid engine flaws (turbopumps, injectors, combustion chambers)
    LiquidEngine,
    /// Solid motor flaws (O-rings, grain cracks, nozzle erosion)
    SolidMotor,
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

    /// Convert trigger to an index for serialization
    pub fn to_index(&self) -> i32 {
        match self {
            FlawTrigger::Ignition => 0,
            FlawTrigger::Liftoff => 1,
            FlawTrigger::MaxQ => 2,
            FlawTrigger::Separation => 3,
            FlawTrigger::PayloadRelease => 4,
        }
    }

    /// Convert index back to trigger type
    pub fn from_index(index: i32) -> Option<FlawTrigger> {
        match index {
            0 => Some(FlawTrigger::Ignition),
            1 => Some(FlawTrigger::Liftoff),
            2 => Some(FlawTrigger::MaxQ),
            3 => Some(FlawTrigger::Separation),
            4 => Some(FlawTrigger::PayloadRelease),
            _ => None,
        }
    }
}

/// Template for generating flaws — just names, descriptions, and trigger types.
/// Numeric values (failure_rate, testing_modifier) are drawn from global distributions.
#[derive(Clone, Debug)]
pub struct FlawTemplate {
    pub name: &'static str,
    pub description: &'static str,
    pub flaw_type: FlawType,
    pub trigger_event_type: FlawTrigger,
}

/// Liquid engine flaw templates - discovered by engine testing, trigger at ignition
/// Used for Kerolox and Hydrolox engines
pub const LIQUID_ENGINE_FLAW_TEMPLATES: &[FlawTemplate] = &[
    FlawTemplate {
        name: "Turbopump Bearing Defect",
        description: "Microscopic imperfections in turbopump bearings cause premature wear and potential seizure during high-speed operation.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Combustion Chamber Crack",
        description: "Hairline fractures in the combustion chamber wall can propagate under thermal stress, leading to catastrophic failure.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Fuel Injector Misalignment",
        description: "Slight misalignment in fuel injectors causes uneven combustion, hot spots, and potential burnthrough.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Gimbal Actuator Weakness",
        description: "Hydraulic actuators for engine gimbaling have insufficient strength for the required thrust vector control loads.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Propellant Valve Seal",
        description: "Main propellant valve seals degrade under cryogenic conditions, causing leaks and pressure loss.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Igniter Reliability Issue",
        description: "Redundant igniters have common-mode failure vulnerability under certain environmental conditions.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Turbine Blade Resonance",
        description: "Turbine blades resonate at certain RPM ranges, causing metal fatigue and eventual failure.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
];

/// Solid motor flaw templates - discovered by engine testing, trigger at ignition
/// Used for solid rocket motors
pub const SOLID_MOTOR_FLAW_TEMPLATES: &[FlawTemplate] = &[
    FlawTemplate {
        name: "O-Ring Seal Defect",
        description: "Field joint O-rings lose elasticity in cold conditions, allowing hot gas blow-by and joint failure.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Propellant Grain Crack",
        description: "Internal cracks in the solid propellant grain cause uneven burning and potential case burn-through.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Nozzle Throat Erosion",
        description: "Excessive erosion of the nozzle throat causes loss of chamber pressure and thrust reduction.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Case Insulation Failure",
        description: "Internal insulation fails to protect the motor case from combustion heat, causing structural failure.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
    FlawTemplate {
        name: "Igniter Squib Malfunction",
        description: "Pyrotechnic igniter fails to produce sufficient heat to reliably ignite the main propellant grain.",
        flaw_type: FlawType::Engine,
        trigger_event_type: FlawTrigger::Ignition,
    },
];

/// Design flaw templates - discovered by rocket testing, trigger at various phases
pub const DESIGN_FLAW_TEMPLATES: &[FlawTemplate] = &[
    FlawTemplate {
        name: "Structural Resonance",
        description: "Vehicle natural frequency matches aerodynamic buffet frequency during max-Q, causing destructive oscillations.",
        flaw_type: FlawType::Design,
        trigger_event_type: FlawTrigger::MaxQ,
    },
    FlawTemplate {
        name: "Stage Separation Bolt Defect",
        description: "Explosive bolts for stage separation have inconsistent charge, leading to asymmetric separation.",
        flaw_type: FlawType::Design,
        trigger_event_type: FlawTrigger::Separation,
    },
    FlawTemplate {
        name: "Guidance Software Bug",
        description: "Edge case in guidance algorithms causes incorrect attitude determination under specific orbital conditions.",
        flaw_type: FlawType::Design,
        trigger_event_type: FlawTrigger::PayloadRelease,
    },
    FlawTemplate {
        name: "Propellant Slosh Instability",
        description: "Propellant sloshing in partially-filled tanks couples with control system, causing loss of control.",
        flaw_type: FlawType::Design,
        trigger_event_type: FlawTrigger::MaxQ,
    },
    FlawTemplate {
        name: "Thermal Protection Gap",
        description: "Gaps in aerodynamic heating protection allow hot gases to damage structure during ascent.",
        flaw_type: FlawType::Design,
        trigger_event_type: FlawTrigger::MaxQ,
    },
    FlawTemplate {
        name: "Interstage Coupler Flaw",
        description: "Interstage structure has insufficient strength for the separation loads under all flight conditions.",
        flaw_type: FlawType::Design,
        trigger_event_type: FlawTrigger::Separation,
    },
    FlawTemplate {
        name: "Avionics Thermal Margin",
        description: "Flight computer cooling is inadequate for extended powered flight, causing thermal shutdown.",
        flaw_type: FlawType::Design,
        trigger_event_type: FlawTrigger::PayloadRelease,
    },
    FlawTemplate {
        name: "Fairing Separation Failure",
        description: "Payload fairing separation system has unreliable pyrotechnic actuators.",
        flaw_type: FlawType::Design,
        trigger_event_type: FlawTrigger::Separation,
    },
    FlawTemplate {
        name: "Liftoff Clamp Release",
        description: "Hold-down clamps release sequence has timing issues that can tip the vehicle.",
        flaw_type: FlawType::Design,
        trigger_event_type: FlawTrigger::Liftoff,
    },
    FlawTemplate {
        name: "Acoustic Vibration Damage",
        description: "Launch acoustic environment exceeds component qualification levels in some areas.",
        flaw_type: FlawType::Design,
        trigger_event_type: FlawTrigger::Liftoff,
    },
];

// ==========================================
// Technology Difficulty Mean Functions
// ==========================================

/// Mean failure rate for engine flaws, based on fuel type and scale.
/// Hydrolox is harder (0.35), Kerolox moderate (0.25), Solid easiest (0.15).
/// Scale has a mild effect: 1.0 + 0.1 * (scale - 1.0).
pub fn engine_failure_rate_mean(fuel_type: FuelType, scale: f64) -> f64 {
    let base = match fuel_type {
        FuelType::Kerolox => 0.25,
        FuelType::Hydrolox => 0.35,
        FuelType::Solid => 0.15,
    };
    let scale_mult = 1.0 + 0.1 * (scale - 1.0);
    base * scale_mult
}

/// Mean testing modifier for engine flaws, based on fuel type.
/// Higher means easier to discover. Solid easiest (0.65), Kerolox moderate (0.55), Hydrolox hardest (0.40).
/// Scale has no effect on discoverability.
pub fn engine_testing_modifier_mean(fuel_type: FuelType, _scale: f64) -> f64 {
    match fuel_type {
        FuelType::Kerolox => 0.55,
        FuelType::Hydrolox => 0.40,
        FuelType::Solid => 0.65,
    }
}

/// Mean failure rate for rocket design flaws, based on complexity.
/// More stages, fuel type diversity, and engines increase failure rates.
pub fn rocket_failure_rate_mean(stage_count: usize, unique_fuel_types: usize, total_engines: u32) -> f64 {
    let base = 0.25;
    let stage_mult = 1.0 + 0.1 * (stage_count as f64 - 1.0);
    let fuel_mult = 1.0 + 0.1 * (unique_fuel_types as f64 - 1.0);
    let engine_mult = 1.0 + 0.02 * (total_engines as f64 - 1.0);
    base * stage_mult * fuel_mult * engine_mult
}

/// Mean testing modifier for rocket design flaws, based on stage count.
/// More stages make testing harder (lower discoverability).
pub fn rocket_testing_modifier_mean(stage_count: usize) -> f64 {
    let base = 0.55;
    base / (1.0 + 0.1 * (stage_count as f64 - 1.0))
}

// ==========================================
// Testing Level Enum and Estimation
// ==========================================

/// Qualitative assessment of how well-tested a design is.
/// Shown to the player instead of exact success rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TestingLevel {
    Untested = 0,
    LightlyTested = 1,
    ModeratelyTested = 2,
    WellTested = 3,
    ThoroughlyTested = 4,
}

impl TestingLevel {
    /// Human-readable name for display
    pub fn name(&self) -> &'static str {
        match self {
            TestingLevel::Untested => "Untested",
            TestingLevel::LightlyTested => "Lightly Tested",
            TestingLevel::ModeratelyTested => "Moderately Tested",
            TestingLevel::WellTested => "Well Tested",
            TestingLevel::ThoroughlyTested => "Thoroughly Tested",
        }
    }

    /// Convert to integer index for GDScript interop
    pub fn to_index(&self) -> i32 {
        *self as i32
    }

    /// Convert from integer index
    pub fn from_index(i: i32) -> TestingLevel {
        match i {
            0 => TestingLevel::Untested,
            1 => TestingLevel::LightlyTested,
            2 => TestingLevel::ModeratelyTested,
            3 => TestingLevel::WellTested,
            4 => TestingLevel::ThoroughlyTested,
            _ => if i < 0 { TestingLevel::Untested } else { TestingLevel::ThoroughlyTested },
        }
    }
}

/// Convert a coverage ratio to a TestingLevel
fn coverage_to_testing_level(coverage: f64) -> TestingLevel {
    if coverage < 0.1 {
        TestingLevel::Untested
    } else if coverage < 0.4 {
        TestingLevel::LightlyTested
    } else if coverage < 1.0 {
        TestingLevel::ModeratelyTested
    } else if coverage < 2.5 {
        TestingLevel::WellTested
    } else {
        TestingLevel::ThoroughlyTested
    }
}

/// Estimate the testing level of an engine based on technology and cumulative testing work.
pub fn engine_testing_level(fuel_type: FuelType, scale: f64, testing_work_completed: f64) -> TestingLevel {
    let expected_flaw_count = 3.5; // average of 3-4
    let fr_mean = engine_failure_rate_mean(fuel_type, scale);
    let tm_mean = engine_testing_modifier_mean(fuel_type, scale);
    let expected_work = expected_flaw_count * TESTING_WORK as f64 / (fr_mean * tm_mean);
    let coverage = testing_work_completed / expected_work;
    coverage_to_testing_level(coverage)
}

/// Estimate the testing level of a rocket design based on complexity and cumulative testing work.
pub fn rocket_testing_level(
    stage_count: usize,
    unique_fuel_types: usize,
    total_engines: u32,
    testing_work_completed: f64,
) -> TestingLevel {
    let expected_flaw_count = (3 + stage_count.min(3)) as f64;
    let fr_mean = rocket_failure_rate_mean(stage_count, unique_fuel_types, total_engines);
    let tm_mean = rocket_testing_modifier_mean(stage_count);
    let expected_work = expected_flaw_count * TESTING_WORK as f64 / (fr_mean * tm_mean);
    let coverage = testing_work_completed / expected_work;
    coverage_to_testing_level(coverage)
}

impl Flaw {
    /// Generate a randomized failure rate from a log-normal distribution.
    /// The `mean` parameter sets the expected value of the distribution.
    /// mu is computed as ln(mean) - sigma²/2 so E[X] = mean.
    /// sigma = 0.8 gives wide spread for varied gameplay.
    /// Clamped to [0.5%, 100%].
    fn randomize_failure_rate(mean: f64) -> f64 {
        let mut rng = rand::thread_rng();

        let sigma = 0.8;
        let mu = mean.ln() - sigma * sigma / 2.0;

        if let Ok(dist) = LogNormal::new(mu, sigma) {
            dist.sample(&mut rng).clamp(0.005, 1.0)
        } else {
            mean
        }
    }

    /// Generate a randomized testing modifier from a uniform distribution.
    /// Centered on `mean` with width 0.5, clamped to [0.05, 1.0].
    fn randomize_testing_modifier(mean: f64) -> f64 {
        let mut rng = rand::thread_rng();
        let low = (mean - 0.25).max(0.05);
        let high = (mean + 0.25).min(1.0);
        rng.gen_range(low..=high)
    }

    /// Create a new flaw from a template with a unique ID.
    /// Failure rate and testing modifier are drawn from distributions centered on the given means.
    pub fn from_template(template: &FlawTemplate, id: u32, fr_mean: f64, tm_mean: f64) -> Self {
        Self {
            id,
            flaw_type: template.flaw_type.clone(),
            name: template.name.to_string(),
            description: template.description.to_string(),
            failure_rate: Self::randomize_failure_rate(fr_mean),
            testing_modifier: Self::randomize_testing_modifier(tm_mean),
            trigger_event_type: template.trigger_event_type.clone(),
            discovered: false,
            fixed: false,
            engine_design_id: None,
        }
    }

    /// Create a new engine flaw from a template with a specific engine design.
    /// Failure rate and testing modifier are drawn from distributions centered on the given means.
    pub fn from_template_with_engine(template: &FlawTemplate, id: u32, engine_design_id: usize, fr_mean: f64, tm_mean: f64) -> Self {
        Self {
            id,
            flaw_type: template.flaw_type.clone(),
            name: template.name.to_string(),
            description: template.description.to_string(),
            failure_rate: Self::randomize_failure_rate(fr_mean),
            testing_modifier: Self::randomize_testing_modifier(tm_mean),
            trigger_event_type: template.trigger_event_type.clone(),
            discovered: false,
            fixed: false,
            engine_design_id: Some(engine_design_id),
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
#[derive(Debug, Clone)]
pub struct FlawGenerator {
    next_id: u32,
}

impl FlawGenerator {
    pub fn new() -> Self {
        Self { next_id: 1 }
    }

    /// Get the flaw templates for a given category
    pub fn templates_for_category(category: FlawCategory) -> &'static [FlawTemplate] {
        match category {
            FlawCategory::LiquidEngine => LIQUID_ENGINE_FLAW_TEMPLATES,
            FlawCategory::SolidMotor => SOLID_MOTOR_FLAW_TEMPLATES,
        }
    }

    /// Generate engine flaws for a specific engine design with the given flaw category.
    /// Fixed count per engine design (not scaled by usage in rockets).
    /// Called when an engine design is first submitted for refining.
    /// Flaw severity depends on fuel_type and scale via technology difficulty means.
    pub fn generate_engine_flaws_for_type_with_category(
        &mut self,
        engine_design_id: usize,
        category: FlawCategory,
        fuel_type: FuelType,
        scale: f64,
    ) -> Vec<Flaw> {
        let mut rng = rand::thread_rng();

        let templates = Self::templates_for_category(category);

        let fr_mean = engine_failure_rate_mean(fuel_type, scale);
        let tm_mean = engine_testing_modifier_mean(fuel_type, scale);

        // Fixed 3-4 flaws per engine design (with log-normal distribution for varied severity)
        let flaw_count = 3 + rng.gen_range(0..2);
        let selected = self.select_random_templates(templates, flaw_count, &mut rng);

        selected
            .into_iter()
            .map(|template| {
                let flaw = Flaw::from_template_with_engine(template, self.next_id, engine_design_id, fr_mean, tm_mean);
                self.next_id += 1;
                flaw
            })
            .collect()
    }

    /// Generate engine flaws for a specific engine design (defaults to LiquidEngine/Kerolox at scale 1.0).
    /// Fixed count per engine design (not scaled by usage in rockets).
    /// Called when an engine design is first submitted for refining.
    pub fn generate_engine_flaws_for_type(&mut self, engine_design_id: usize) -> Vec<Flaw> {
        self.generate_engine_flaws_for_type_with_category(engine_design_id, FlawCategory::LiquidEngine, FuelType::Kerolox, 1.0)
    }

    /// Generate only design flaws for a rocket (engine flaws are on EngineDesign now).
    /// Called when a rocket design is created.
    /// Flaw severity depends on stage_count, unique_fuel_types, and total_engines.
    pub fn generate_design_flaws(
        &mut self,
        stage_count: usize,
        unique_fuel_types: usize,
        total_engines: u32,
    ) -> Vec<Flaw> {
        let mut rng = rand::thread_rng();

        let fr_mean = rocket_failure_rate_mean(stage_count, unique_fuel_types, total_engines);
        let tm_mean = rocket_testing_modifier_mean(stage_count);

        // Design flaws: 3-6 based on stage count
        // More flaws with long tail distribution for varied gameplay
        let design_flaw_count = 3 + stage_count.min(3);
        let design_templates = self.select_random_templates(
            DESIGN_FLAW_TEMPLATES,
            design_flaw_count,
            &mut rng,
        );

        design_templates
            .into_iter()
            .map(|template| {
                let flaw = Flaw::from_template(template, self.next_id, fr_mean, tm_mean);
                self.next_id += 1;
                flaw
            })
            .collect()
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
/// stage_engine_design_id: the engine design ID of the stage (for filtering engine flaws)
pub fn calculate_flaw_failure_rate(flaws: &[Flaw], event_name: &str, stage_engine_design_id: Option<usize>) -> f64 {
    flaws
        .iter()
        .filter(|f| {
            if !f.can_trigger_at(event_name) {
                return false;
            }
            // For engine flaws, only count if engine design matches the stage
            if f.flaw_type == FlawType::Engine {
                match (f.engine_design_id, stage_engine_design_id) {
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

/// Run an engine test for a specific engine design
/// Returns names of flaws discovered
pub fn run_engine_test_for_type(flaws: &mut [Flaw], engine_design_id: usize) -> Vec<String> {
    let mut rng = rand::thread_rng();
    let mut discovered = Vec::new();

    for flaw in flaws.iter_mut() {
        // Only test engine flaws for the specified engine design
        if flaw.flaw_type == FlawType::Engine
            && flaw.engine_design_id == Some(engine_design_id)
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
/// stage_engine_design_id: the engine design ID of the stage that failed (for filtering engine flaws)
/// Returns the flaw ID of the responsible flaw, or None if no flaws could have triggered
pub fn check_flaw_trigger(flaws: &[Flaw], event_name: &str, stage_engine_design_id: Option<usize>) -> Option<u32> {
    let mut rng = rand::thread_rng();

    // Get all active flaws that can trigger at this event, with their effective rates
    // For engine flaws, only include if the engine design matches the stage's engine design
    let triggerable: Vec<(&Flaw, f64)> = flaws
        .iter()
        .filter(|f| {
            if !f.can_trigger_at(event_name) {
                return false;
            }
            // For engine flaws, check that the engine design matches
            if f.flaw_type == FlawType::Engine {
                match (f.engine_design_id, stage_engine_design_id) {
                    (Some(flaw_engine), Some(stage_engine)) => flaw_engine == stage_engine,
                    _ => false, // Engine flaw without design info or stage without design info
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flaw_from_template() {
        let template = &LIQUID_ENGINE_FLAW_TEMPLATES[0];
        let flaw = Flaw::from_template(template, 1, 0.25, 0.55);

        assert_eq!(flaw.id, 1);
        assert_eq!(flaw.flaw_type, FlawType::Engine);
        assert_eq!(flaw.name, template.name);
        assert!(!flaw.discovered);
        assert!(!flaw.fixed);
    }

    #[test]
    fn test_flaw_is_active() {
        let mut flaw = Flaw::from_template(&LIQUID_ENGINE_FLAW_TEMPLATES[0], 1, 0.25, 0.55);

        assert!(flaw.is_active());

        flaw.fixed = true;
        assert!(!flaw.is_active());
    }

    #[test]
    fn test_flaw_trigger_matches() {
        let ignition_flaw = Flaw::from_template(&LIQUID_ENGINE_FLAW_TEMPLATES[0], 1, 0.25, 0.55);
        assert!(ignition_flaw.can_trigger_at("Stage 1 Ignition"));
        assert!(!ignition_flaw.can_trigger_at("Liftoff"));

        let maxq_flaw = Flaw::from_template(&DESIGN_FLAW_TEMPLATES[0], 2, 0.25, 0.55);
        assert!(maxq_flaw.can_trigger_at("Max-Q"));
        assert!(!maxq_flaw.can_trigger_at("Stage 1 Ignition"));
    }

    #[test]
    fn test_effective_failure_rate() {
        let mut flaw = Flaw::from_template(&LIQUID_ENGINE_FLAW_TEMPLATES[0], 1, 0.25, 0.55);

        let original_rate = flaw.failure_rate;
        assert!(flaw.effective_failure_rate() > 0.0);
        assert_eq!(flaw.effective_failure_rate(), original_rate);

        flaw.fixed = true;
        assert_eq!(flaw.effective_failure_rate(), 0.0);
    }

    #[test]
    fn test_discovery_probability() {
        let mut flaw = Flaw::from_template(&LIQUID_ENGINE_FLAW_TEMPLATES[0], 1, 0.25, 0.55);

        let prob = flaw.discovery_probability();
        assert!(prob > 0.0);
        assert!(prob <= flaw.failure_rate); // Can't be more than failure rate

        flaw.discovered = true;
        assert_eq!(flaw.discovery_probability(), 0.0);
    }

    #[test]
    fn test_flaw_generator() {
        let mut generator = FlawGenerator::new();

        // Generate engine flaws for a specific engine type
        let engine_flaws = generator.generate_engine_flaws_for_type(0);

        // Generate design flaws for a 2-stage rocket (1 fuel type, 6 engines)
        let design_flaws = generator.generate_design_flaws(2, 1, 6);

        // Should generate the expected counts
        assert!(engine_flaws.len() >= 3); // 3-4 per engine type
        assert!(design_flaws.len() >= 3); // 3+ based on stage count

        // All engine flaws should be of type Engine
        assert!(engine_flaws.iter().all(|f| f.flaw_type == FlawType::Engine));

        // All design flaws should be of type Design
        assert!(design_flaws.iter().all(|f| f.flaw_type == FlawType::Design));

        // All flaws should have unique IDs
        let all_flaws: Vec<&Flaw> = engine_flaws.iter().chain(design_flaws.iter()).collect();
        let ids: Vec<u32> = all_flaws.iter().map(|f| f.id).collect();
        let unique_ids: std::collections::HashSet<u32> = ids.iter().cloned().collect();
        assert_eq!(ids.len(), unique_ids.len());
    }

    #[test]
    fn test_calculate_flaw_failure_rate() {
        let mut generator = FlawGenerator::new();

        // Generate engine flaws for engine type 0
        let engine_flaws = generator.generate_engine_flaws_for_type(0);

        // Get ignition failure rate for engine type 0
        let ignition_rate = calculate_flaw_failure_rate(&engine_flaws, "Stage 1 Ignition", Some(0));

        // Should be sum of all engine flaw failure rates for engine type 0
        let expected: f64 = engine_flaws
            .iter()
            .filter(|f| f.flaw_type == FlawType::Engine && f.engine_design_id == Some(0))
            .map(|f| f.failure_rate)
            .sum();

        assert!((ignition_rate - expected).abs() < 0.001);
    }

    // ==========================================
    // Technology Difficulty Mean Tests
    // ==========================================

    #[test]
    fn test_engine_failure_rate_means() {
        let kerolox = engine_failure_rate_mean(FuelType::Kerolox, 1.0);
        let hydrolox = engine_failure_rate_mean(FuelType::Hydrolox, 1.0);
        let solid = engine_failure_rate_mean(FuelType::Solid, 1.0);

        assert_eq!(kerolox, 0.25);
        assert_eq!(hydrolox, 0.35);
        assert_eq!(solid, 0.15);
        assert!(solid < kerolox);
        assert!(kerolox < hydrolox);
    }

    #[test]
    fn test_engine_testing_modifier_means() {
        let kerolox = engine_testing_modifier_mean(FuelType::Kerolox, 1.0);
        let hydrolox = engine_testing_modifier_mean(FuelType::Hydrolox, 1.0);
        let solid = engine_testing_modifier_mean(FuelType::Solid, 1.0);

        assert_eq!(kerolox, 0.55);
        assert_eq!(hydrolox, 0.40);
        assert_eq!(solid, 0.65);
        assert!(hydrolox < kerolox);
        assert!(kerolox < solid);
    }

    #[test]
    fn test_scale_effect_mild() {
        let scale_1 = engine_failure_rate_mean(FuelType::Kerolox, 1.0);
        let scale_4 = engine_failure_rate_mean(FuelType::Kerolox, 4.0);

        // At scale 4.0: multiplier = 1.0 + 0.1 * (4-1) = 1.3
        let ratio = scale_4 / scale_1;
        assert!((ratio - 1.3).abs() < 0.01, "Scale effect should be ~1.3x at scale 4.0, got {}", ratio);
    }

    #[test]
    fn test_rocket_failure_rate_mean() {
        let base = rocket_failure_rate_mean(1, 1, 1);
        let more_stages = rocket_failure_rate_mean(3, 1, 1);
        let more_engines = rocket_failure_rate_mean(1, 1, 10);
        let more_fuels = rocket_failure_rate_mean(1, 3, 1);

        assert!(more_stages > base, "More stages should increase failure rate mean");
        assert!(more_engines > base, "More engines should increase failure rate mean");
        assert!(more_fuels > base, "More fuel types should increase failure rate mean");
    }

    #[test]
    fn test_randomize_failure_rate_parameterized() {
        // Run many samples and verify higher mean produces higher average
        let n = 1000;
        let mut sum_low = 0.0;
        let mut sum_high = 0.0;

        for _ in 0..n {
            sum_low += Flaw::randomize_failure_rate(0.15);
            sum_high += Flaw::randomize_failure_rate(0.35);
        }

        let avg_low = sum_low / n as f64;
        let avg_high = sum_high / n as f64;

        assert!(avg_high > avg_low, "Higher mean ({}) should produce higher average ({}) than lower mean ({}), avg ({})",
            0.35, avg_high, 0.15, avg_low);
    }

    // ==========================================
    // Testing Level Tests
    // ==========================================

    #[test]
    fn test_testing_level_thresholds() {
        assert_eq!(coverage_to_testing_level(0.0), TestingLevel::Untested);
        assert_eq!(coverage_to_testing_level(0.05), TestingLevel::Untested);
        assert_eq!(coverage_to_testing_level(0.1), TestingLevel::LightlyTested);
        assert_eq!(coverage_to_testing_level(0.3), TestingLevel::LightlyTested);
        assert_eq!(coverage_to_testing_level(0.4), TestingLevel::ModeratelyTested);
        assert_eq!(coverage_to_testing_level(0.99), TestingLevel::ModeratelyTested);
        assert_eq!(coverage_to_testing_level(1.0), TestingLevel::WellTested);
        assert_eq!(coverage_to_testing_level(2.0), TestingLevel::WellTested);
        assert_eq!(coverage_to_testing_level(2.5), TestingLevel::ThoroughlyTested);
        assert_eq!(coverage_to_testing_level(10.0), TestingLevel::ThoroughlyTested);
    }

    #[test]
    fn test_engine_testing_level_progression() {
        // 0 work = Untested
        let level_0 = engine_testing_level(FuelType::Kerolox, 1.0, 0.0);
        assert_eq!(level_0, TestingLevel::Untested);

        // Much work = Thoroughly Tested
        let level_many = engine_testing_level(FuelType::Kerolox, 1.0, 10000.0);
        assert_eq!(level_many, TestingLevel::ThoroughlyTested);

        // Monotonically increasing
        let levels: Vec<TestingLevel> = [0.0, 5.0, 20.0, 50.0, 200.0]
            .iter()
            .map(|&w| engine_testing_level(FuelType::Kerolox, 1.0, w))
            .collect();
        for i in 1..levels.len() {
            assert!(levels[i] >= levels[i-1], "Testing level should not decrease with more testing work");
        }
    }

    #[test]
    fn test_rocket_testing_level() {
        // 0 work = Untested
        let level_0 = rocket_testing_level(2, 1, 6, 0.0);
        assert_eq!(level_0, TestingLevel::Untested);

        // Much work = Thoroughly Tested
        let level_many = rocket_testing_level(2, 1, 6, 10000.0);
        assert_eq!(level_many, TestingLevel::ThoroughlyTested);
    }

    #[test]
    fn test_testing_level_names() {
        assert_eq!(TestingLevel::Untested.name(), "Untested");
        assert_eq!(TestingLevel::LightlyTested.name(), "Lightly Tested");
        assert_eq!(TestingLevel::ModeratelyTested.name(), "Moderately Tested");
        assert_eq!(TestingLevel::WellTested.name(), "Well Tested");
        assert_eq!(TestingLevel::ThoroughlyTested.name(), "Thoroughly Tested");
    }

    #[test]
    fn test_testing_level_index_roundtrip() {
        for i in 0..=4 {
            let level = TestingLevel::from_index(i);
            assert_eq!(level.to_index(), i);
        }
    }
}
