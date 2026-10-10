use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::balance_config::FlawsConfig;

/// Unique identifier for a flaw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlawId(pub u64);

/// When a flaw can trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Default)]
pub enum FlawTrigger {
    /// Rolls once when the stage fires (existing behavior).
    #[default]
    PerFlight,
    /// Rolls every day in flight (endurance flaw).
    PerDay,
}


impl FlawTrigger {
    /// Reference mission duration in days for converting activation_chance to daily rate.
    const REFERENCE_DAYS: f64 = 365.0;
}

/// The moment in a flight a flaw gets its roll (22_FLAWS.md §3.12).
/// `Endurance` is the daily roll of a `PerDay` flaw; the rest are
/// events of an ascent or of a later burn, walked in order by
/// `launch::ascent_events`. A rocket flaw also names the stage group
/// whose hardware it lives on (`Flaw::stage_group`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum FlawPhase {
    /// Engines start: igniters, start transients, pressurisation.
    Ignition,
    /// Peak aerodynamic load on an atmospheric ascent. Never rolled
    /// leaving an airless body.
    MaxQ,
    /// The stage's engines running.
    #[default]
    Burn,
    /// This stage lets go of what is above it — interstage, pyros,
    /// residual thrust. The lower stage's hardware.
    Separation,
    /// The top stage releases the payload.
    PayloadDeploy,
    /// Wear: the daily `PerDay` roll.
    Endurance,
}

impl FlawPhase {
    pub fn word(self) -> &'static str {
        match self {
            FlawPhase::Ignition => "ignition",
            FlawPhase::MaxQ => "max-Q",
            FlawPhase::Burn => "burn",
            FlawPhase::Separation => "separation",
            FlawPhase::PayloadDeploy => "payload deploy",
            FlawPhase::Endurance => "endurance",
        }
    }
}

/// What happens when a flaw activates during flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlawConsequence {
    /// Fraction of thrust/isp lost (e.g. 0.05 = 5% loss).
    PerformanceDegradation(f64),
    /// The affected engine fails.
    EngineLoss,
    /// The entire stage fails.
    StageLoss,
}

impl FlawConsequence {
    /// Compact wording for engine and rocket flaw lists.
    pub fn short_label(&self) -> String {
        match self {
            FlawConsequence::PerformanceDegradation(frac) =>
                format!("{:.0}% perf loss", frac * 100.0),
            FlawConsequence::EngineLoss => "engine loss".to_string(),
            FlawConsequence::StageLoss => "stage loss".to_string(),
        }
    }

    /// Compact wording for reactor flaw lists, where "engine loss" means
    /// the reactor shuts down and degradation is lost power.
    pub fn reactor_short_label(&self) -> String {
        match self {
            FlawConsequence::PerformanceDegradation(frac) =>
                format!("{:.0}% power loss", frac * 100.0),
            FlawConsequence::EngineLoss => "reactor shutdown".to_string(),
            FlawConsequence::StageLoss => "stage loss".to_string(),
        }
    }
}

impl std::fmt::Display for FlawConsequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlawConsequence::PerformanceDegradation(frac) =>
                write!(f, "{:.0}% performance loss", frac * 100.0),
            FlawConsequence::EngineLoss => write!(f, "engine loss"),
            FlawConsequence::StageLoss => write!(f, "stage loss"),
        }
    }
}

/// Serde default for the per-project `auto_revise` flag. New projects
/// and projects loaded from pre-M5 saves both get it on.
pub fn auto_revise_default() -> bool {
    true
}

/// A latent defect in a design that may activate in flight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flaw {
    pub id: FlawId,
    pub description: String,
    pub consequence: FlawConsequence,
    /// Chance per flight (PerFlight) or cumulative over reference duration (PerDay).
    pub activation_chance: f64,
    /// Chance per testing cycle to discover this flaw.
    /// Computed as uniform(0,1)^N * sqrt(activation_chance), N being
    /// `FlawsConfig::flaw_discovery_exponent`.
    pub discovery_probability: f64,
    pub discovered: bool,
    /// When this flaw can trigger.
    #[serde(default)]
    pub trigger: FlawTrigger,
    /// The flight event this flaw rolls at. Pre-v6 saves get it from
    /// `assign_legacy_phase`.
    #[serde(default)]
    pub phase: FlawPhase,
    /// Rocket flaws: the stage group whose hardware this is, clamped to
    /// the design's groups when rolled. `None` for engine, reactor and
    /// endurance flaws.
    #[serde(default)]
    pub stage_group: Option<usize>,
}

impl Flaw {
    /// For PerDay flaws, convert activation_chance to a daily rate.
    /// For PerFlight flaws, returns activation_chance unchanged.
    pub fn daily_rate(&self) -> f64 {
        match self.trigger {
            FlawTrigger::PerFlight => self.activation_chance,
            FlawTrigger::PerDay => {
                // activation_chance = 1 - (1 - daily_rate)^365
                // daily_rate = 1 - (1 - activation_chance)^(1/365)
                1.0 - (1.0 - self.activation_chance).powf(1.0 / FlawTrigger::REFERENCE_DAYS)
            }
        }
    }

    /// Where and when it bites, for lists: "S2 separation" for a rocket
    /// flaw, the phase word alone for an engine's.
    pub fn phase_label(&self) -> String {
        match self.stage_group {
            Some(g) => format!("S{} {}", g + 1, self.phase.word()),
            None => self.phase.word().to_string(),
        }
    }

    /// Give a flaw from before phases existed its phase and, for a rocket
    /// flaw, its stage group. The phase comes from the description's pool
    /// entry when the text is still one of ours (it always was until the
    /// pools were retagged), else `Burn`; the group is drawn from `rng`
    /// (the world seed keyed by flaw id, so a save migrates the same way
    /// every time).
    pub fn assign_legacy_phase(&mut self, is_rocket: bool, stage_groups: usize, rng: &mut StdRng) {
        if self.trigger == FlawTrigger::PerDay {
            self.phase = FlawPhase::Endurance;
            self.stage_group = None;
            return;
        }
        self.phase = phase_for_description(&self.description).unwrap_or_default();
        self.stage_group = is_rocket.then(|| stage_group_for(self.phase, stage_groups, rng));
    }
}

/// The stage group a rocket flaw of `phase` lives on: max-Q is the
/// first stage's, payload deploy the last's, the rest anywhere.
fn stage_group_for(phase: FlawPhase, stage_groups: usize, rng: &mut StdRng) -> usize {
    let groups = stage_groups.max(1);
    match phase {
        FlawPhase::MaxQ => 0,
        FlawPhase::PayloadDeploy => groups - 1,
        _ => rng.gen_range(0..groups),
    }
}

/// The phase a pool entry with this exact text carries, if any pool has it.
pub fn phase_for_description(text: &str) -> Option<FlawPhase> {
    ALL_POOLS.iter()
        .flat_map(|pool| pool.degradation.iter().chain(pool.part_loss.iter()).chain(pool.stage_loss.iter()))
        .find(|e| e.text == text)
        .map(|e| e.phase)
}

const ALL_POOLS: [&FlawPool; 9] = [
    &ENGINE_FLAWS, &ENGINE_ENDURANCE_FLAWS, &ELECTRIC_FLAWS, &NUCLEAR_FLAWS, &SOLAR_SAIL_FLAWS,
    &ROCKET_FLAWS, &ROCKET_ENDURANCE_FLAWS, &REACTOR_FLAWS, &REACTOR_ENDURANCE_FLAWS,
];

/// Which kind of design a flaw belongs to. Selects the description
/// pools it is worded from and whether it can be an endurance (`PerDay`)
/// flaw at all — this, with the per-domain complexity that sets the
/// count, is where flaws differ between engines, rockets and reactors.
/// The probability model underneath is shared (`roll_flaw_core`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlawDomain {
    /// An engine; the cycle picks a specialised pool (electric,
    /// nuclear, solar sail) when known.
    Engine(Option<crate::engine::EngineCycle>),
    Rocket,
    Reactor,
}

impl FlawDomain {
    /// Fraction of generated flaws that are `PerDay`. `None` means the
    /// domain has no endurance flaws and draws no trigger roll.
    pub(crate) fn endurance_fraction(self, cfg: &FlawsConfig) -> Option<f64> {
        match self {
            FlawDomain::Engine(_) => None,
            FlawDomain::Rocket => Some(cfg.rocket_endurance_fraction),
            FlawDomain::Reactor => Some(cfg.reactor_endurance_fraction),
        }
    }
}

/// Generate the flaws of a newly completed design.
///
/// The count is drawn from a gaussian centred on `effective_complexity`
/// (stddev `cfg.count_stddev`), floored at zero. Each flaw then rolls
/// its trigger (domains with endurance flaws only), then the shared
/// probability core, then a description from the domain's pool.
pub fn generate_flaws(
    domain: FlawDomain,
    effective_complexity: u32,
    stage_groups: usize,
    rng: &mut StdRng,
    next_flaw_id: &mut crate::id::IdAllocator<FlawId>,
    cfg: &FlawsConfig,
) -> Vec<Flaw> {
    let mean = effective_complexity as f64;
    let count_f = gaussian_sample(mean, cfg.count_stddev, rng);
    let count = count_f.round().max(0.0) as u32;

    (0..count).map(|_| {
        let id = next_flaw_id.mint();
        let trigger = match domain.endurance_fraction(cfg) {
            Some(fraction) if rng.gen::<f64>() < fraction => FlawTrigger::PerDay,
            _ => FlawTrigger::PerFlight,
        };
        generate_single_flaw(domain, id, trigger, stage_groups, rng, cfg)
    }).collect()
}

/// Build one flaw: shared probability core, domain-worded description.
/// `PerDay` flaws read as gradual wear, `PerFlight` ones as events, so
/// what the player is asked to revise sounds like the part it lives on.
/// The description's pool entry sets the phase; a rocket flaw also
/// draws the stage group it lives on from the design's `stage_groups`.
pub fn generate_single_flaw(
    domain: FlawDomain, id: FlawId, trigger: FlawTrigger, stage_groups: usize,
    rng: &mut StdRng, cfg: &FlawsConfig,
) -> Flaw {
    let (consequence, activation_chance, discovery_probability) = roll_flaw_core(rng, cfg);
    let entry = FlawPool::for_flaw(domain, trigger).pick(&consequence, rng);
    let phase = if trigger == FlawTrigger::PerDay { FlawPhase::Endurance } else { entry.phase };
    let stage_group = (domain == FlawDomain::Rocket && trigger == FlawTrigger::PerFlight)
        .then(|| stage_group_for(phase, stage_groups, rng));
    Flaw {
        id,
        description: entry.text.to_string(),
        consequence,
        activation_chance,
        discovery_probability,
        discovered: false,
        trigger,
        phase,
        stage_group,
    }
}

/// Roll the domain-agnostic core of a flaw: its consequence, activation
/// chance, and discovery probability. Shared by engine, rocket, and
/// reactor flaw generation so the probability model stays in one place.
///
/// Consequence weighting: ~50% performance degradation, ~35% engine/part
/// loss, ~15% stage loss. Activation chance is random^2 (skewed low);
/// discovery probability = uniform(0,1)^N * sqrt(activation_chance),
/// where N is `flaw_discovery_exponent` (see FlawsConfig).
fn roll_flaw_core(rng: &mut StdRng, cfg: &FlawsConfig) -> (FlawConsequence, f64, f64) {
    let roll: f64 = rng.gen();
    let consequence = if roll < cfg.performance_degradation_weight {
        let degradation = rng.gen_range(cfg.degradation_min..cfg.degradation_max);
        FlawConsequence::PerformanceDegradation(degradation)
    } else if roll < cfg.performance_degradation_weight + cfg.engine_loss_weight {
        FlawConsequence::EngineLoss
    } else {
        FlawConsequence::StageLoss
    };

    let activation_chance: f64 = rng.gen::<f64>().powi(2);
    let uniform_roll: f64 = rng.gen();
    let discovery_probability =
        uniform_roll.powf(cfg.flaw_discovery_exponent) * activation_chance.sqrt();

    (consequence, activation_chance, discovery_probability)
}

/// The words a flaw is described with: six per consequence, drawn
/// uniformly. One pool per (domain, trigger) — and per engine cycle for
/// the cycles whose failure modes are nothing like a chemical engine's.
///
/// Every entry names a *property of the design*, not an event: a flaw is
/// latent, built in and carried until revised, and the trigger decides
/// when it bites. Endurance (`PerDay`) pools read as wear and drift; the
/// rest as things that go wrong during a burn.
#[derive(Debug, Clone, Copy)]
pub struct FlawPool {
    /// `PerformanceDegradation`: the part works worse.
    pub degradation: [FlawEntry; 6],
    /// `EngineLoss`: the part is lost (an engine shuts down, a reactor
    /// SCRAMs) but the stage survives.
    pub part_loss: [FlawEntry; 6],
    /// `StageLoss`: the whole stage is lost.
    pub stage_loss: [FlawEntry; 6],
}

/// One way a flaw can be worded, and the flight event those words
/// happen at: "Separation pyrotechnics fire out of sequence" can only
/// bite at separation.
#[derive(Debug, Clone, Copy)]
pub struct FlawEntry {
    pub text: &'static str,
    pub phase: FlawPhase,
}

const fn e(text: &'static str, phase: FlawPhase) -> FlawEntry {
    FlawEntry { text, phase }
}

impl FlawPool {
    /// One entry for `consequence`, drawn uniformly.
    fn pick(&self, consequence: &FlawConsequence, rng: &mut StdRng) -> &FlawEntry {
        let entries = match consequence {
            FlawConsequence::PerformanceDegradation(_) => &self.degradation,
            FlawConsequence::EngineLoss => &self.part_loss,
            FlawConsequence::StageLoss => &self.stage_loss,
        };
        &entries[rng.gen_range(0..entries.len())]
    }

    /// The pool a flaw of `domain` with `trigger` is worded from.
    fn for_flaw(domain: FlawDomain, trigger: FlawTrigger) -> &'static FlawPool {
        use crate::engine::EngineCycle;
        match (domain, trigger) {
            (FlawDomain::Rocket, FlawTrigger::PerDay) => &ROCKET_ENDURANCE_FLAWS,
            (FlawDomain::Rocket, FlawTrigger::PerFlight) => &ROCKET_FLAWS,
            (FlawDomain::Reactor, FlawTrigger::PerDay) => &REACTOR_ENDURANCE_FLAWS,
            (FlawDomain::Reactor, FlawTrigger::PerFlight) => &REACTOR_FLAWS,
            (FlawDomain::Engine(Some(EngineCycle::SolarSail)), FlawTrigger::PerFlight) => &SOLAR_SAIL_FLAWS,
            (FlawDomain::Engine(Some(EngineCycle::ElectricPropulsion)), FlawTrigger::PerFlight) => &ELECTRIC_FLAWS,
            (FlawDomain::Engine(Some(EngineCycle::NuclearThermal)), FlawTrigger::PerFlight) => &NUCLEAR_FLAWS,
            (FlawDomain::Engine(_), FlawTrigger::PerFlight) => &ENGINE_FLAWS,
            (FlawDomain::Engine(_), FlawTrigger::PerDay) => &ENGINE_ENDURANCE_FLAWS,
        }
    }
}

/// Chemical engines during a burn.
pub const ENGINE_FLAWS: FlawPool = FlawPool {
    degradation: [
        e("Turbopump seal leak", FlawPhase::Burn),
        e("Injector pattern inefficiency", FlawPhase::Burn),
        e("Nozzle cooling channel restriction", FlawPhase::Burn),
        e("Valve response lag", FlawPhase::Ignition),
        e("Combustion instability at partial throttle", FlawPhase::Burn),
        e("Propellant feed pressure oscillation", FlawPhase::Burn),
    ],
    part_loss: [
        e("Turbopump bearing fatigue", FlawPhase::Burn),
        e("Combustion chamber hot spot", FlawPhase::Burn),
        e("Igniter reliability issue", FlawPhase::Ignition),
        e("Oxidizer-rich preburner instability", FlawPhase::Ignition),
        e("Thermal stress cracking in nozzle", FlawPhase::Burn),
        e("Main injector face erosion", FlawPhase::Burn),
    ],
    stage_loss: [
        e("Propellant feed line vibration failure", FlawPhase::Burn),
        e("Stage separation bolt stress fracture", FlawPhase::Burn),
        e("Thrust structure resonance mode", FlawPhase::Burn),
        e("Ullage gas contamination risk", FlawPhase::Ignition),
        e("Inter-stage electrical harness fault", FlawPhase::Burn),
        e("Catastrophic combustion instability", FlawPhase::Burn),
    ],
};

/// Engines and their plumbing over a long mission.
pub const ENGINE_ENDURANCE_FLAWS: FlawPool = FlawPool {
    degradation: [
        e("Thermal cycling degradation", FlawPhase::Endurance),
        e("Sensor drift accumulation", FlawPhase::Endurance),
        e("Propellant line seal wear", FlawPhase::Endurance),
        e("Attitude control thruster fouling", FlawPhase::Endurance),
        e("Radiator coating degradation", FlawPhase::Endurance),
        e("Reaction wheel bearing wear", FlawPhase::Endurance),
    ],
    part_loss: [
        e("Turbopump bearing wear", FlawPhase::Endurance),
        e("Igniter electrode erosion", FlawPhase::Endurance),
        e("Fuel valve seat degradation", FlawPhase::Endurance),
        e("Oxidizer seal embrittlement", FlawPhase::Endurance),
        e("Engine controller memory corruption", FlawPhase::Endurance),
        e("Regenerative cooling tube fatigue", FlawPhase::Endurance),
    ],
    stage_loss: [
        e("Avionics thermal failure", FlawPhase::Endurance),
        e("Battery capacity degradation", FlawPhase::Endurance),
        e("Structural fatigue crack propagation", FlawPhase::Endurance),
        e("Guidance computer memory fault", FlawPhase::Endurance),
        e("Wiring harness insulation breakdown", FlawPhase::Endurance),
        e("Pressurization system leak", FlawPhase::Endurance),
    ],
};

/// Ion and Hall thrusters: grids, cathodes, xenon feed, the PPU.
pub const ELECTRIC_FLAWS: FlawPool = FlawPool {
    degradation: [
        e("Ion grid erosion rate higher than expected", FlawPhase::Burn),
        e("Beam neutralizer current drift", FlawPhase::Burn),
        e("Discharge chamber magnetic field asymmetry", FlawPhase::Burn),
        e("Xenon flow controller calibration offset", FlawPhase::Burn),
        e("Thruster plume divergence angle excessive", FlawPhase::Burn),
        e("Power processing unit efficiency loss", FlawPhase::Burn),
    ],
    part_loss: [
        e("Grid short circuit from sputtered material", FlawPhase::Burn),
        e("Cathode heater element failure", FlawPhase::Burn),
        e("Xenon isolator valve seizure", FlawPhase::Burn),
        e("High-voltage breakdown in PPU", FlawPhase::Burn),
        e("Discharge chamber wall sputter-through", FlawPhase::Burn),
        e("Neutralizer keeper electrode erosion", FlawPhase::Burn),
    ],
    stage_loss: [
        e("Xenon tank pressure regulator failure", FlawPhase::Burn),
        e("Solar array connection arc fault", FlawPhase::Burn),
        e("Thruster gimbal mechanism binding", FlawPhase::Burn),
        e("Power bus overcurrent shutdown", FlawPhase::Burn),
        e("Propellant management unit leak", FlawPhase::Burn),
        e("Electromagnetic interference with avionics", FlawPhase::Burn),
    ],
};

/// Nuclear-thermal engines: fuel elements, drums, hydrogen everywhere.
pub const NUCLEAR_FLAWS: FlawPool = FlawPool {
    degradation: [
        e("Fuel element hydrogen corrosion", FlawPhase::Burn),
        e("Reactor power distribution imbalance", FlawPhase::Burn),
        e("Turbopump hydrogen bearing wear", FlawPhase::Burn),
        e("Nozzle skirt hydrogen embrittlement", FlawPhase::Burn),
        e("Moderator element swelling", FlawPhase::Burn),
        e("Reflector drum actuator lag", FlawPhase::Burn),
    ],
    part_loss: [
        e("Fuel element mid-section break", FlawPhase::Burn),
        e("Control drum servo mechanism failure", FlawPhase::Burn),
        e("Reactor thermal runaway risk", FlawPhase::Burn),
        e("Hydrogen leak in reactor pressure vessel", FlawPhase::Burn),
        e("Neutron poison buildup in fuel elements", FlawPhase::Burn),
        e("Turbopump seal failure from radiation damage", FlawPhase::Burn),
    ],
    stage_loss: [
        e("Radiation shielding structural failure", FlawPhase::Burn),
        e("Reactor SCRAM system false trigger", FlawPhase::Burn),
        e("Hydrogen tank embrittlement fracture", FlawPhase::Burn),
        e("Reactor coolant channel blockage", FlawPhase::Burn),
        e("Uncontrolled criticality excursion risk", FlawPhase::Burn),
        e("Nozzle detachment from thermal cycling", FlawPhase::Burn),
    ],
};

/// Solar sails: film, booms, deployment.
pub const SOLAR_SAIL_FLAWS: FlawPool = FlawPool {
    degradation: [
        e("Sail reflectivity degradation", FlawPhase::Burn),
        e("Micrometeorite puncture damage", FlawPhase::Burn),
        e("Sail deployment mechanism binding", FlawPhase::Ignition),
        e("Attitude control vane misalignment", FlawPhase::Burn),
        e("Sail surface wrinkling", FlawPhase::Burn),
        e("Solar radiation pressure modeling error", FlawPhase::Burn),
    ],
    part_loss: [
        e("Sail boom structural failure", FlawPhase::Burn),
        e("Complete sail deployment failure", FlawPhase::Ignition),
        e("Sail tearing from thermal stress", FlawPhase::Burn),
        e("Attitude control system failure", FlawPhase::Burn),
        e("Sail furling mechanism jam", FlawPhase::Burn),
        e("Boom hinge seizure", FlawPhase::Burn),
    ],
    stage_loss: [
        e("Sail catastrophic tear propagation", FlawPhase::Burn),
        e("Boom collapse from impact", FlawPhase::Burn),
        e("Sail jettison mechanism malfunction", FlawPhase::Burn),
        e("Thermal deformation beyond recovery", FlawPhase::Burn),
        e("Complete attitude loss from sail asymmetry", FlawPhase::Burn),
        e("Sail connection point failure", FlawPhase::Burn),
    ],
};

/// The vehicle rather than its engines: tankage, structure, separation,
/// avionics, plumbing. An engine project already owns injectors and
/// turbopumps, so a rocket project that borrowed those words read as if
/// you were revising somebody else's work.
pub const ROCKET_FLAWS: FlawPool = FlawPool {
    degradation: [
        e("Tank baffle slosh damping insufficient", FlawPhase::Burn),
        e("Aerodynamic fairing drag higher than modelled", FlawPhase::MaxQ),
        e("Guidance loop overcorrects in high winds", FlawPhase::MaxQ),
        e("Stage mass over budget after assembly", FlawPhase::Burn),
        e("Thrust vector alignment out of tolerance", FlawPhase::Burn),
        e("Residual propellant trapped at tank sump", FlawPhase::Burn),
    ],
    part_loss: [
        e("Propellant feed starves the outboard engine", FlawPhase::Burn),
        e("Engine bay overheats without purge flow", FlawPhase::Burn),
        e("Gimbal actuator mount flexes under load", FlawPhase::MaxQ),
        e("Pogo suppressor undersized for this stage", FlawPhase::Burn),
        e("Engine mount bolt preload inconsistent", FlawPhase::Ignition),
        e("Feed line collapses under transient pressure", FlawPhase::Ignition),
    ],
    stage_loss: [
        e("Interstage buckles under max-Q loading", FlawPhase::MaxQ),
        e("Separation pyrotechnics fire out of sequence", FlawPhase::Separation),
        e("Common bulkhead weld porosity", FlawPhase::Burn),
        e("Payload fairing fails to jettison cleanly", FlawPhase::Separation),
        e("Tank pressurisation regulator runs away", FlawPhase::Ignition),
        e("Flight computer resets during staging transient", FlawPhase::Separation),
    ],
};

/// The airframe and its systems wearing and drifting in transit —
/// "no micrometeoroid shielding", not "pitting weakened a tank wall".
pub const ROCKET_ENDURANCE_FLAWS: FlawPool = FlawPool {
    degradation: [
        e("Tank insulation degrades, boiloff climbs", FlawPhase::Endurance),
        e("Star tracker alignment drifts with thermal cycling", FlawPhase::Endurance),
        e("Attitude control propellant leaks past a seat", FlawPhase::Endurance),
        e("Solar array hinge stiffens, pointing lags", FlawPhase::Endurance),
        e("Thermal coating erodes under UV", FlawPhase::Endurance),
        e("Reaction wheel imbalance grows with hours", FlawPhase::Endurance),
    ],
    part_loss: [
        e("Restart accumulator loses pressure over days", FlawPhase::Endurance),
        e("Engine bay heater fails, propellant lines chill", FlawPhase::Endurance),
        e("Ullage motor propellant slowly vents", FlawPhase::Endurance),
        e("Gimbal actuator lubricant migrates in vacuum", FlawPhase::Endurance),
        e("Feed line bellows fatigues on each thermal cycle", FlawPhase::Endurance),
        e("Engine controller watchdog trips intermittently", FlawPhase::Endurance),
    ],
    stage_loss: [
        e("No micrometeoroid shielding over the tank wall", FlawPhase::Endurance),
        e("Battery cell imbalance goes uncorrected", FlawPhase::Endurance),
        e("Harness insulation embrittles and shorts", FlawPhase::Endurance),
        e("Pressurant slowly leaks past a check valve", FlawPhase::Endurance),
        e("Structural adhesive creeps under sustained load", FlawPhase::Endurance),
        e("Flight computer accumulates uncorrected bit flips", FlawPhase::Endurance),
    ],
};

/// Reactors at power. Degradation reads as lost output; part loss as a
/// shutdown (the reactor trips, not the stage).
pub const REACTOR_FLAWS: FlawPool = FlawPool {
    degradation: [
        e("Coolant loop flow restriction", FlawPhase::Burn),
        e("Radiator fin degradation reduces heat rejection", FlawPhase::Burn),
        e("Control drum drift derates output", FlawPhase::Burn),
        e("Fuel element swelling reduces thermal transfer", FlawPhase::Burn),
        e("Thermoelectric converter efficiency loss", FlawPhase::Burn),
        e("Partial coolant channel blockage", FlawPhase::Burn),
    ],
    part_loss: [
        e("Control drum actuator seizure triggers SCRAM", FlawPhase::Burn),
        e("Coolant pump failure forces reactor shutdown", FlawPhase::Burn),
        e("Fuel element cladding breach", FlawPhase::Burn),
        e("Reactor overheats and trips offline", FlawPhase::Burn),
        e("Neutron poison buildup stalls the core", FlawPhase::Burn),
        e("Primary coolant loop leak", FlawPhase::Burn),
    ],
    stage_loss: [
        e("Reactor pressure vessel rupture", FlawPhase::Burn),
        e("Uncontrolled criticality excursion", FlawPhase::Burn),
        e("Radiation shielding structural failure", FlawPhase::Burn),
        e("Coolant flash-boil breaches the stage", FlawPhase::Burn),
        e("Thermal runaway destroys the stage", FlawPhase::Burn),
        e("Reactor debris severs stage structure", FlawPhase::Burn),
    ],
};

/// Reactors over a long mission: burnup, fouling, embrittlement, creep.
pub const REACTOR_ENDURANCE_FLAWS: FlawPool = FlawPool {
    degradation: [
        e("Radiator coating erosion degrades heat rejection", FlawPhase::Endurance),
        e("Fuel burnup lowers reactivity over time", FlawPhase::Endurance),
        e("Neutron embrittlement of core structure", FlawPhase::Endurance),
        e("Coolant loop fouling accumulates", FlawPhase::Endurance),
        e("Thermoelectric junction degradation", FlawPhase::Endurance),
        e("Control drum bearing wear derates output", FlawPhase::Endurance),
    ],
    part_loss: [
        e("Fuel cladding creep-ruptures after prolonged heat", FlawPhase::Endurance),
        e("Coolant pump bearing wears out and seizes", FlawPhase::Endurance),
        e("Cumulative xenon poisoning stalls the core", FlawPhase::Endurance),
        e("Control-drum actuator fails from thermal cycling", FlawPhase::Endurance),
        e("Primary loop develops a slow coolant leak", FlawPhase::Endurance),
        e("Reactor trips offline on degraded shielding sensors", FlawPhase::Endurance),
    ],
    stage_loss: [
        e("Coolant embrittlement leads to pressure-vessel failure", FlawPhase::Endurance),
        e("Long-term radiation damage collapses the structure", FlawPhase::Endurance),
        e("Cumulative thermal fatigue cracks the reactor mount", FlawPhase::Endurance),
        e("Shielding degradation triggers a runaway excursion", FlawPhase::Endurance),
        e("Radiator manifold fatigue ruptures the coolant loop", FlawPhase::Endurance),
        e("Structural creep severs the stage under load", FlawPhase::Endurance),
    ],
};

/// Roll for flaw discovery during a testing cycle.
/// Returns indices of newly discovered flaws.
pub fn roll_discoveries_with_rng(flaws: &mut [Flaw], rng: &mut StdRng) -> Vec<usize> {
    let mut discovered = Vec::new();
    for (i, flaw) in flaws.iter_mut().enumerate() {
        if !flaw.discovered {
            let roll: f64 = rng.gen();
            if roll < flaw.discovery_probability {
                flaw.discovered = true;
                discovered.push(i);
            }
        }
    }
    discovered
}

/// Sample from a gaussian distribution using Box-Muller transform.
fn gaussian_sample(mean: f64, stddev: f64, rng: &mut StdRng) -> f64 {
    let u1: f64 = rng.gen();
    let u2: f64 = rng.gen();
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    mean + stddev * z
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{test_rng};
    use rand::SeedableRng;


    fn cfg() -> FlawsConfig {
        FlawsConfig::default()
    }

    #[test]
    fn test_generate_flaws_count_near_complexity() {
        // Run many times and check average is near effective_complexity
        let mut total = 0u32;
        let trials = 1000;
        for seed in 0..trials {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
            let flaws = generate_flaws(FlawDomain::Engine(None), 7, 0, &mut rng, &mut next_id, &cfg());
            total += flaws.len() as u32;
        }
        let avg = total as f64 / trials as f64;
        // Should be close to 7 (±1 is fine for 1000 trials)
        assert!((avg - 7.0).abs() < 1.0, "Average flaw count {} should be near 7", avg);
    }

    #[test]
    fn test_generate_flaws_can_be_zero() {
        // With low complexity, some runs should produce zero flaws
        let mut found_zero = false;
        for seed in 0..1000 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
            let flaws = generate_flaws(FlawDomain::Engine(None), 2, 0, &mut rng, &mut next_id, &cfg());
            if flaws.is_empty() {
                found_zero = true;
                break;
            }
        }
        assert!(found_zero, "Should sometimes generate zero flaws at low complexity");
    }

    #[test]
    fn test_flaw_ids_are_sequential() {
        let mut rng = test_rng();
        let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(10);
        let flaws = generate_flaws(FlawDomain::Engine(None), 6, 0, &mut rng, &mut next_id, &cfg());
        for (i, flaw) in flaws.iter().enumerate() {
            assert_eq!(flaw.id, FlawId(10 + i as u64));
        }
        assert_eq!(next_id.next_raw(), 10 + flaws.len() as u64);
    }

    #[test]
    fn test_flaws_start_undiscovered() {
        let mut rng = test_rng();
        let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
        let flaws = generate_flaws(FlawDomain::Engine(None), 8, 0, &mut rng, &mut next_id, &cfg());
        for flaw in &flaws {
            assert!(!flaw.discovered);
        }
    }

    #[test]
    fn test_activation_chance_in_range() {
        let mut rng = test_rng();
        let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
        let flaws = generate_flaws(FlawDomain::Engine(None), 9, 0, &mut rng, &mut next_id, &cfg());
        for flaw in &flaws {
            assert!(flaw.activation_chance >= 0.0, "activation_chance should be non-negative");
            assert!(flaw.activation_chance <= 1.0, "activation_chance should be <= 1");
        }
    }

    #[test]
    fn test_activation_chance_skewed_low() {
        // With random^2, most values should be below 0.5
        let mut rng = test_rng();
        let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
        let flaws = generate_flaws(FlawDomain::Engine(None), 100, 0, &mut rng, &mut next_id, &cfg());
        let below_half = flaws.iter().filter(|f| f.activation_chance < 0.5).count();
        assert!(
            below_half as f64 / flaws.len() as f64 > 0.6,
            "Most activation chances should be below 0.5 (got {}/{})",
            below_half, flaws.len(),
        );
    }

    #[test]
    fn test_discovery_probability_bounded_by_sqrt_activation() {
        let mut rng = test_rng();
        let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
        let flaws = generate_flaws(FlawDomain::Engine(None), 9, 0, &mut rng, &mut next_id, &cfg());
        for flaw in &flaws {
            assert!(
                flaw.discovery_probability <= flaw.activation_chance.sqrt() + 0.001,
                "discovery {} should be <= sqrt(activation {}) = {}",
                flaw.discovery_probability,
                flaw.activation_chance,
                flaw.activation_chance.sqrt()
            );
        }
    }

    #[test]
    fn test_roll_discoveries() {
        let mut rng = test_rng();
        let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
        let mut flaws = generate_flaws(FlawDomain::Engine(None), 8, 0, &mut rng, &mut next_id, &cfg());

        // Force high discovery probability on first flaw for testing
        if !flaws.is_empty() {
            flaws[0].discovery_probability = 0.99;
        }

        // Roll many times — should eventually discover the high-probability flaw
        let mut discovered_first = false;
        for seed in 0..100 {
            let mut roll_rng = StdRng::seed_from_u64(seed + 1000);
            let newly = roll_discoveries_with_rng(&mut flaws, &mut roll_rng);
            if newly.contains(&0) {
                discovered_first = true;
                break;
            }
            // Reset for next attempt
            if !discovered_first {
                flaws[0].discovered = false;
            }
        }
        assert!(discovered_first, "Should discover a flaw with 0.99 probability");
    }

    #[test]
    fn test_consequence_display() {
        assert_eq!(
            FlawConsequence::PerformanceDegradation(0.05).to_string(),
            "5% performance loss"
        );
        assert_eq!(FlawConsequence::EngineLoss.to_string(), "engine loss");
        assert_eq!(FlawConsequence::StageLoss.to_string(), "stage loss");
    }

    #[test]
    fn test_gaussian_distribution() {
        let mut rng = test_rng();
        let samples: Vec<f64> = (0..10000).map(|_| gaussian_sample(7.0, 1.5, &mut rng)).collect();
        let mean: f64 = samples.iter().sum::<f64>() / samples.len() as f64;
        assert!((mean - 7.0).abs() < 0.1, "Gaussian mean {} should be near 7.0", mean);
    }

    #[test]
    fn test_daily_rate_per_flight_unchanged() {
        let flaw = Flaw {
            id: FlawId(1), description: "test".into(),
            consequence: FlawConsequence::EngineLoss,
            activation_chance: 0.5,
            discovery_probability: 0.3,
            discovered: false,
            trigger: FlawTrigger::PerFlight,
            phase: crate::flaw::FlawPhase::Burn, stage_group: None,
        };
        assert_eq!(flaw.daily_rate(), 0.5);
    }

    #[test]
    fn test_daily_rate_per_day_conversion() {
        let flaw = Flaw {
            id: FlawId(1), description: "test".into(),
            consequence: FlawConsequence::EngineLoss,
            activation_chance: 0.30,
            discovery_probability: 0.3,
            discovered: false,
            trigger: FlawTrigger::PerDay,
            phase: crate::flaw::FlawPhase::Burn, stage_group: None,
        };
        let rate = flaw.daily_rate();
        // 1 - (1 - 0.30)^(1/365) ≈ 0.000977
        assert!(rate > 0.0009 && rate < 0.0011,
            "Daily rate should be ~0.097%/day, got {}", rate);

        // Verify: cumulative over 365 days should recover ~0.30
        let cumulative = 1.0 - (1.0 - rate).powi(365);
        assert!((cumulative - 0.30).abs() < 0.001,
            "Cumulative over 365 days should be ~0.30, got {}", cumulative);
    }

    #[test]
    fn test_rocket_flaws_have_per_day() {
        let mut rng = test_rng();
        let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
        let flaws = generate_flaws(FlawDomain::Rocket, 10, 3, &mut rng, &mut next_id, &cfg());
        let per_day_count = flaws.iter().filter(|f| f.trigger == FlawTrigger::PerDay).count();
        // With 30% chance and ~10 flaws, expect ~3 PerDay (allow 0-8 for randomness)
        assert!(per_day_count > 0, "Should have some PerDay flaws");
        assert!(per_day_count < flaws.len(), "Should have some PerFlight flaws too");
    }

    #[test]
    fn test_reactor_flaws_have_endurance_mix() {
        // Reactors run continuously, so ~30% of their flaws are PerDay
        // endurance flaws and the rest PerFlight. Aggregate over seeds so
        // the statistical split is reliable.
        let mut per_day = 0usize;
        let mut per_flight = 0usize;
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
            for flaw in generate_flaws(FlawDomain::Reactor, 10, 0, &mut rng, &mut next_id, &cfg()) {
                match flaw.trigger {
                    FlawTrigger::PerDay => per_day += 1,
                    FlawTrigger::PerFlight => per_flight += 1,
                }
            }
        }
        assert!(per_day > 0, "reactors should have some PerDay endurance flaws");
        assert!(per_flight > 0, "reactors should have some PerFlight flaws too");
        // Roughly 30% endurance — allow a wide band for randomness.
        let frac = per_day as f64 / (per_day + per_flight) as f64;
        assert!(frac > 0.15 && frac < 0.45, "endurance fraction {} should be ~0.30", frac);
    }

    #[test]
    fn test_reactor_flaws_count_near_complexity() {
        let mut total = 0u32;
        let trials = 1000;
        for seed in 0..trials {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
            let flaws = generate_flaws(FlawDomain::Reactor, 8, 0, &mut rng, &mut next_id, &cfg());
            total += flaws.len() as u32;
        }
        let avg = total as f64 / trials as f64;
        assert!((avg - 8.0).abs() < 1.0, "Average reactor flaw count {} should be near 8", avg);
    }

    #[test]
    fn test_reactor_flaws_ids_sequential_and_undiscovered() {
        let mut rng = test_rng();
        let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(5);
        let flaws = generate_flaws(FlawDomain::Reactor, 9, 0, &mut rng, &mut next_id, &cfg());
        for (i, flaw) in flaws.iter().enumerate() {
            assert_eq!(flaw.id, FlawId(5 + i as u64));
            assert!(!flaw.discovered);
        }
        assert_eq!(next_id.next_raw(), 5 + flaws.len() as u64);
    }

    #[test]
    fn test_engine_flaws_all_per_flight() {
        let mut rng = test_rng();
        let mut next_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
        let flaws = generate_flaws(FlawDomain::Engine(None), 10, 0, &mut rng, &mut next_id, &cfg());
        for flaw in &flaws {
            assert_eq!(flaw.trigger, FlawTrigger::PerFlight,
                "Engine flaws should all be PerFlight");
        }
    }
}

#[cfg(test)]
mod description_pool_tests {
    use super::*;
    use rand::SeedableRng;

    fn cfg() -> FlawsConfig {
        crate::balance_config::BalanceConfig::default().flaws
    }

    /// Every description a rocket project can produce, over enough rolls
    /// to reach every entry in both pools.
    fn all_rocket_descriptions() -> Vec<String> {
        let mut rng = StdRng::seed_from_u64(7);
        let cfg = cfg();
        let mut out = Vec::new();
        for i in 0..2_000u64 {
            let trigger = if i % 2 == 0 { FlawTrigger::PerFlight } else { FlawTrigger::PerDay };
            out.push(generate_single_flaw(FlawDomain::Rocket, FlawId(i), trigger, 3, &mut rng, &cfg).description);
        }
        out
    }

    fn all_engine_descriptions() -> Vec<String> {
        let mut rng = StdRng::seed_from_u64(7);
        let cfg = cfg();
        let mut out = Vec::new();
        for i in 0..2_000u64 {
            let trigger = if i % 2 == 0 { FlawTrigger::PerFlight } else { FlawTrigger::PerDay };
            out.push(generate_single_flaw(FlawDomain::Engine(None), FlawId(i), trigger, 0, &mut rng, &cfg).description);
        }
        out
    }

    /// A rocket project owns tankage, structure, separation and avionics;
    /// an engine project owns injectors and turbopumps. Revising a rocket
    /// used to hand you "Injector pattern inefficiency", which reads as
    /// somebody else's work.
    #[test]
    fn rocket_flaws_never_borrow_engine_internals() {
        let rocket = all_rocket_descriptions();
        let engine = all_engine_descriptions();

        for d in &rocket {
            assert!(!engine.contains(d),
                "rocket flaw {d:?} is also in the engine pool");
        }
        // And the obvious engine-internal words stay out of it.
        for word in ["Injector", "injector", "Turbopump", "turbopump",
                     "Combustion chamber", "preburner", "Nozzle"] {
            assert!(!rocket.iter().any(|d| d.contains(word)),
                "a rocket flaw mentions {word:?}");
        }
    }

    /// Both triggers draw from the rocket pools — the endurance half is
    /// the one a player sees most, since it is what strands a craft.
    #[test]
    fn both_rocket_triggers_have_their_own_pool() {
        let mut rng = StdRng::seed_from_u64(11);
        let cfg = cfg();
        let per_flight: Vec<String> = (0..300u64)
            .map(|i| generate_single_flaw(FlawDomain::Rocket, FlawId(i), FlawTrigger::PerFlight, 3, &mut rng, &cfg).description)
            .collect();
        let per_day: Vec<String> = (0..300u64)
            .map(|i| generate_single_flaw(FlawDomain::Rocket, FlawId(i), FlawTrigger::PerDay, 3, &mut rng, &cfg).description)
            .collect();

        assert!(per_flight.iter().any(|d| d.contains("Interstage")
            || d.contains("separation") || d.contains("Separation")),
            "per-flight flaws are staging and burn events");
        assert!(per_day.iter().any(|d| d.contains("boiloff")
            || d.contains("drifts") || d.contains("erodes")),
            "per-day flaws are wear and drift");
        assert!(per_flight.iter().all(|d| !per_day.contains(d)),
            "the two pools stay distinct");
    }

    /// Only the description changed: the probability model is still shared
    /// with engine and reactor flaws.
    #[test]
    fn rocket_flaws_keep_the_shared_probability_core() {
        let cfg = cfg();
        let mut a = StdRng::seed_from_u64(3);
        let mut b = StdRng::seed_from_u64(3);
        let rocket = generate_single_flaw(FlawDomain::Rocket, FlawId(1), FlawTrigger::PerFlight, 3, &mut a, &cfg);
        let engine = generate_single_flaw(FlawDomain::Engine(None), FlawId(1), FlawTrigger::PerFlight, 0, &mut b, &cfg);

        // FlawConsequence has no PartialEq; its Display is faithful enough.
        assert_eq!(rocket.consequence.to_string(), engine.consequence.to_string());
        assert_eq!(rocket.activation_chance, engine.activation_chance);
        assert_eq!(rocket.discovery_probability, engine.discovery_probability);
        assert_ne!(rocket.description, engine.description, "but the words differ");
    }

    /// A rocket flaw is pinned to a stage group and a flight event at
    /// generation; max-Q belongs to the first stage, wear to no stage.
    #[test]
    fn rocket_flaws_live_on_a_stage_at_an_event() {
        let cfg = cfg();
        let mut saw_max_q = false;
        for seed in 0..40u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut next_id = crate::id::IdAllocator::<FlawId>::starting_at(0);
            for f in generate_flaws(FlawDomain::Rocket, 8, 3, &mut rng, &mut next_id, &cfg) {
                match f.trigger {
                    FlawTrigger::PerDay => {
                        assert_eq!(f.phase, FlawPhase::Endurance);
                        assert_eq!(f.stage_group, None);
                    }
                    FlawTrigger::PerFlight => {
                        assert_ne!(f.phase, FlawPhase::Endurance, "{}", f.description);
                        let g = f.stage_group.expect("a rocket flaw names its stage");
                        assert!(g < 3, "group {g} on a three-group design");
                        if f.phase == FlawPhase::MaxQ {
                            saw_max_q = true;
                            assert_eq!(g, 0, "max-Q is the first stage's moment");
                        }
                    }
                }
            }
            for f in generate_flaws(FlawDomain::Engine(None), 8, 0, &mut rng, &mut next_id, &cfg) {
                assert_eq!(f.stage_group, None, "engine flaws are not pinned to a stage");
            }
        }
        assert!(saw_max_q, "the rocket pool has max-Q entries");
    }

    /// A flaw from before phases existed gets the phase its own words
    /// carry in the pools, and a rocket flaw draws a stage.
    #[test]
    fn legacy_flaws_take_the_phase_of_their_pool_entry() {
        let mut flaw = Flaw {
            id: FlawId(7),
            description: "Separation pyrotechnics fire out of sequence".into(),
            consequence: FlawConsequence::StageLoss,
            activation_chance: 0.2, discovery_probability: 0.1, discovered: false,
            trigger: FlawTrigger::PerFlight,
            phase: FlawPhase::Burn, stage_group: None,
        };
        let mut rng = StdRng::seed_from_u64(1);
        flaw.assign_legacy_phase(true, 2, &mut rng);
        assert_eq!(flaw.phase, FlawPhase::Separation);
        assert!(matches!(flaw.stage_group, Some(g) if g < 2));

        flaw.description = "Something the pools never said".into();
        flaw.assign_legacy_phase(false, 0, &mut rng);
        assert_eq!(flaw.phase, FlawPhase::Burn);
        assert_eq!(flaw.stage_group, None);

        flaw.trigger = FlawTrigger::PerDay;
        flaw.assign_legacy_phase(true, 2, &mut rng);
        assert_eq!(flaw.phase, FlawPhase::Endurance);
        assert_eq!(flaw.stage_group, None);

        assert_eq!(phase_for_description("Igniter reliability issue"), Some(FlawPhase::Ignition));
    }
}
