use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::company::Company;
use crate::contract::ContractId;
use crate::engine::EngineId;
use crate::engine_project::{EngineProject, EngineSource};
use crate::flaw::{Flaw, FlawConsequence, FlawId, FlawPhase, FlawTrigger};
use crate::reactor::ReactorId;
use crate::rocket::{DesignPerformance, Rocket, RocketDesign};
use crate::rocket_project::RocketProjectId;
use crate::stage::Stage;
use crate::third_party::ContractedEngine;

// ── Flaw snapshots and rolls ─────────────────────────────────────────
//
// Flaws live on projects; vehicles are built from copies of those
// designs. When a vehicle fires a stage or sits through a day, the
// flaws that can bite it are looked up here — snapshotted so the roll
// can mutate the vehicle without holding a borrow on the company — and
// rolled by one function per kind of roll, whether the vehicle is on
// the pad, in transit, or parked.

/// The part a flaw lives on, and how to find it again to mark it
/// discovered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlawOwner {
    Engine { source: EngineSource, engine_id: EngineId },
    Rocket(RocketProjectId),
    Reactor(ReactorId),
}

/// A flaw snapshotted from its project.
#[derive(Debug, Clone)]
pub struct FlawRef {
    pub owner: FlawOwner,
    pub flaw_index: usize,
    pub id: FlawId,
    pub trigger: FlawTrigger,
    pub phase: FlawPhase,
    pub stage_group: Option<usize>,
    pub activation_chance: f64,
    pub daily_rate: f64,
    pub consequence: FlawConsequence,
    pub description: String,
}

impl FlawRef {
    fn from(owner: FlawOwner, flaw_index: usize, flaw: &crate::flaw::Flaw) -> Self {
        FlawRef {
            owner,
            flaw_index,
            id: flaw.id,
            trigger: flaw.trigger,
            phase: flaw.phase,
            stage_group: flaw.stage_group,
            activation_chance: flaw.activation_chance,
            daily_rate: flaw.daily_rate(),
            consequence: flaw.consequence.clone(),
            description: flaw.description.clone(),
        }
    }
}

/// Every flaw the fleet can suffer, by the kind of part it lives on.
pub struct FlawTables {
    /// Player-designed engines first, then contracted ones; every flaw.
    pub engines: Vec<FlawRef>,
    /// Rocket projects' endurance (`PerDay`) flaws only — their
    /// `PerFlight` flaws roll on the pad from the inventory item's own
    /// snapshot.
    pub rockets: Vec<FlawRef>,
    /// Reactor projects; every flaw, both triggers.
    pub reactors: Vec<FlawRef>,
}

impl FlawTables {
    pub fn snapshot(company: &Company) -> Self {
        FlawTables {
            engines: engine_flaw_table(&company.engine_projects, &company.contracted_engines),
            rockets: company.rocket_projects.iter()
                .flat_map(|rp| rp.flaws.iter().enumerate()
                    .filter(|(_, f)| f.trigger == FlawTrigger::PerDay)
                    .map(move |(fi, f)| FlawRef::from(FlawOwner::Rocket(rp.project_id), fi, f)))
                .collect(),
            reactors: company.reactor_projects.iter()
                .flat_map(|rp| rp.flaws.iter().enumerate()
                    .map(move |(fi, f)| FlawRef::from(FlawOwner::Reactor(rp.design.id), fi, f)))
                .collect(),
        }
    }
}

/// The engine flaw table on its own, for the launch sim's callers that
/// hold the two lists rather than a company.
pub fn engine_flaw_table(
    engine_projects: &[EngineProject], contracted_engines: &[ContractedEngine],
) -> Vec<FlawRef> {
    let mut table = Vec::new();
    for ep in engine_projects {
        let owner = FlawOwner::Engine {
            source: EngineSource::PlayerDesign(ep.project_id), engine_id: ep.design.id,
        };
        table.extend(ep.flaws.iter().enumerate().map(|(fi, f)| FlawRef::from(owner, fi, f)));
    }
    for ce in contracted_engines {
        let owner = FlawOwner::Engine {
            source: EngineSource::Contracted(ce.id), engine_id: ce.design.id,
        };
        table.extend(ce.flaws.iter().enumerate().map(|(fi, f)| FlawRef::from(owner, fi, f)));
    }
    table
}

/// What one round of flaw rolls did to a vehicle.
#[derive(Debug, Default)]
pub struct FlawRoll {
    /// Every flaw that fired, in order.
    pub activations: Vec<FlawActivation>,
    /// The same flaws, as (owner, index, id) for marking them
    /// discovered. Live tables are addressed by index; a vehicle's
    /// snapshot may be older than its project, so rocket flaws are
    /// found again by id.
    pub discoveries: Vec<(FlawOwner, usize, FlawId)>,
    /// Set when a `StageLoss` fired: the vehicle is gone and the roll
    /// stopped there, since nothing after the loss can be observed.
    pub lost: Option<String>,
}

/// A moment in a flight at which flaws roll, in the order
/// `ascent_events` walks them (22_FLAWS.md §3.12). Groups index the
/// design's `stage_groups`; players read them one-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlightEvent {
    Ignition { group: usize },
    MaxQ,
    Burn { group: usize },
    Separation { group: usize },
    PayloadDeploy,
}

impl FlightEvent {
    /// The flaw phase that rolls at this event.
    pub fn phase(self) -> FlawPhase {
        match self {
            FlightEvent::Ignition { .. } => FlawPhase::Ignition,
            FlightEvent::MaxQ => FlawPhase::MaxQ,
            FlightEvent::Burn { .. } => FlawPhase::Burn,
            FlightEvent::Separation { .. } => FlawPhase::Separation,
            FlightEvent::PayloadDeploy => FlawPhase::PayloadDeploy,
        }
    }

    /// The stage group whose hardware is on the line: max-Q is the
    /// first stage's moment, payload deploy the last's.
    pub fn group(self, design: &RocketDesign) -> usize {
        match self {
            FlightEvent::Ignition { group } | FlightEvent::Burn { group } | FlightEvent::Separation { group } => group,
            FlightEvent::MaxQ => 0,
            FlightEvent::PayloadDeploy => design.stage_groups.len().saturating_sub(1),
        }
    }

    /// As the player reads it: "S1 ignition", "max-Q", "S2 separation".
    pub fn word(self) -> String {
        match self {
            FlightEvent::MaxQ => "max-Q".to_string(),
            FlightEvent::PayloadDeploy => "payload deploy".to_string(),
            other => format!("S{} {}", other.group_index() + 1, other.phase().word()),
        }
    }

    fn group_index(self) -> usize {
        match self {
            FlightEvent::Ignition { group } | FlightEvent::Burn { group } | FlightEvent::Separation { group } => group,
            FlightEvent::MaxQ | FlightEvent::PayloadDeploy => 0,
        }
    }
}

/// The events of a burn that fires `groups` in order: each group
/// ignites and burns; before an upper group ignites, the group below
/// it separates; the first group of an atmospheric ascent passes
/// max-Q after ignition. `groups` is what the first leg's burn fires
/// (`ascent_groups`), or the new groups of a later leg.
pub fn ascent_events(groups: &[usize], atmosphere: bool) -> Vec<FlightEvent> {
    let mut events = Vec::new();
    for (k, &group) in groups.iter().enumerate() {
        if group > 0 {
            events.push(FlightEvent::Separation { group: group - 1 });
        }
        events.push(FlightEvent::Ignition { group });
        if k == 0 && group == 0 && atmosphere {
            events.push(FlightEvent::MaxQ);
        }
        events.push(FlightEvent::Burn { group });
    }
    events
}

/// The stage groups a launch to `destination` fires before it is in
/// orbit: the first leg of the planned route, flown on a fresh copy of
/// the design. Everything here rolls on the pad as one timeline; a
/// group that first fires on a later leg rolls then.
pub fn ascent_groups(design: &RocketDesign, destination: &str, payload_kg: f64, from: &str) -> Vec<usize> {
    let fallback = || if design.stage_groups.is_empty() { Vec::new() } else { vec![0] };
    let Some((path, _)) = crate::location::DELTA_V_MAP
        .shortest_path_for_rocket(from, destination, design, payload_kg)
    else { return fallback() };
    if path.len() < 2 {
        return fallback();
    }
    let mut probe = design.instantiate(crate::rocket::RocketId(0), payload_kg);
    let route = crate::flight::build_route_for_rocket(&path, design, &probe, payload_kg);
    let Some(first) = route.first() else { return fallback() };
    let burn = probe.burn_sequential(design, first.delta_v_cost, from);
    let mut groups = burn.groups_burned;
    groups.sort_unstable();
    groups.dedup();
    if groups.is_empty() { fallback() } else { groups }
}

/// The flaws of one rocket design as a table, for the vehicle's own
/// snapshot (inventory item or flight) rather than the live project.
pub fn rocket_flaw_table(project_id: RocketProjectId, flaws: &[Flaw]) -> Vec<FlawRef> {
    flaws.iter().enumerate()
        .map(|(fi, f)| FlawRef::from(FlawOwner::Rocket(project_id), fi, f))
        .collect()
}

/// Roll everything that can bite at `event`: the engine flaws of that
/// phase on every stage of the event's group (each scaled by the
/// stage's engine count, `1 - (1-p)^n`), then the rocket flaws of that
/// phase whose stage group it is. A hit is applied to the stage; a
/// `StageLoss` ends the roll, since nothing after the loss can be
/// observed.
pub fn roll_event_flaws(
    rng: &mut StdRng,
    design: &mut RocketDesign,
    event: FlightEvent,
    engines: &[FlawRef],
    rockets: &[FlawRef],
) -> FlawRoll {
    let mut roll = FlawRoll::default();
    let phase = event.phase();
    let gi = event.group(design);
    let Some(group) = design.stage_groups.get(gi) else { return roll };
    let stage_count = group.len();
    if stage_count == 0 {
        return roll;
    }
    let last_group = design.stage_groups.len() - 1;

    for si in 0..stage_count {
        let (engine_id, engine_count, engine_name) = {
            let stage = &design.stage_groups[gi][si];
            (stage.engine.id, stage.engine_count, stage.engine.name.clone())
        };
        for flaw in engines.iter().filter(|f| f.phase == phase
            && matches!(f.owner, FlawOwner::Engine { engine_id: id, .. } if id == engine_id))
        {
            let effective_p = 1.0 - (1.0 - flaw.activation_chance).powi(engine_count as i32);
            if rng.gen::<f64>() < effective_p {
                roll.activations.push(FlawActivation {
                    flaw_description: flaw.description.clone(),
                    consequence: flaw.consequence.clone(),
                    source: engine_source(&engine_name),
                    event: Some(event),
                });
                roll.discoveries.push((flaw.owner, flaw.flaw_index, flaw.id));
                apply_consequence_to_stage(design, &flaw.consequence, gi, si);
                if matches!(flaw.consequence, FlawConsequence::StageLoss) {
                    roll.lost = Some(flaw.description.clone());
                    return roll;
                }
            }
        }
    }

    for flaw in rockets.iter().filter(|f| f.trigger == FlawTrigger::PerFlight && f.phase == phase
        && f.stage_group.map_or(gi == 0, |g| g.min(last_group) == gi))
    {
        if rng.gen::<f64>() >= flaw.activation_chance {
            continue;
        }
        let si = if stage_count > 1 { rng.gen_range(0..stage_count) } else { 0 };
        let source = rocket_source(&design.stage_groups[gi][si].name);
        roll.activations.push(FlawActivation {
            flaw_description: flaw.description.clone(),
            consequence: flaw.consequence.clone(),
            source,
            event: Some(event),
        });
        roll.discoveries.push((flaw.owner, flaw.flaw_index, flaw.id));
        apply_consequence_to_stage(design, &flaw.consequence, gi, si);
        if matches!(flaw.consequence, FlawConsequence::StageLoss) {
            roll.lost = Some(flaw.description.clone());
            return roll;
        }
    }
    roll
}

/// A random stage still attached to a flying vehicle, as (group, stage).
fn random_attached_stage(rng: &mut StdRng, design: &RocketDesign, rocket: &Rocket) -> Option<(usize, usize)> {
    let attached: Vec<(usize, usize)> = rocket.attached_stages(design)
        .map(|(gi, si, _, _)| (gi, si))
        .collect();
    if attached.is_empty() {
        return None;
    }
    Some(attached[rng.gen_range(0..attached.len())])
}

/// One day of a vehicle's endurance flaws: each `PerDay` flaw of its
/// rocket design fires at its daily rate, on a random attached stage.
/// Stops at a `StageLoss`. Shared by flights in transit and parked
/// spacecraft, so a vehicle ages the same way wherever it is.
pub fn roll_endurance_flaws(
    rng: &mut StdRng,
    design: &mut RocketDesign,
    rocket: &Rocket,
    project_id: RocketProjectId,
    table: &[FlawRef],
) -> FlawRoll {
    let mut roll = FlawRoll::default();
    for flaw in table.iter().filter(|f| f.owner == FlawOwner::Rocket(project_id)) {
        if rng.gen::<f64>() >= flaw.daily_rate {
            continue;
        }
        let Some((gi, si)) = random_attached_stage(rng, design, rocket) else { continue };
        let source = rocket_source(&design.stage_groups[gi][si].name);
        apply_consequence_to_stage(design, &flaw.consequence, gi, si);
        roll.activations.push(FlawActivation {
            flaw_description: flaw.description.clone(),
            consequence: flaw.consequence.clone(),
            source,
            event: None,
        });
        roll.discoveries.push((flaw.owner, flaw.flaw_index, flaw.id));
        if matches!(flaw.consequence, FlawConsequence::StageLoss) {
            roll.lost = Some(flaw.description.clone());
            break;
        }
    }
    roll
}

/// Engines destroyed by flow separation when a stage fires into
/// `ambient` pressure with a nozzle built for less.
#[derive(Debug, Clone, Copy)]
pub struct OverexpansionLoss {
    pub engines_lost: u32,
    pub engines_before: u32,
    pub exit_pressure_pa: f64,
}

/// Roll each of the stage's engines independently at its overexpansion
/// destruction risk and take the losses off the stage — all of them
/// gone disables it. `None` when the nozzle is safely matched.
pub fn roll_overexpansion(
    rng: &mut StdRng, stage: &mut Stage, ambient_pa: f64, nozzle_cfg: &crate::balance_config::NozzleConfig,
) -> Option<OverexpansionLoss> {
    let risk = stage.engine.overexpansion_destruction_risk(ambient_pa, nozzle_cfg);
    if risk <= 0.0 {
        return None;
    }
    let engines_before = stage.engine_count;
    let engines_lost = (0..engines_before).filter(|_| rng.gen::<f64>() < risk).count() as u32;
    if engines_lost == 0 {
        return None;
    }
    let exit_pressure_pa = stage.engine.exit_pressure_pa();
    if engines_lost >= engines_before {
        stage.disable();
    } else {
        stage.engine_count -= engines_lost;
    }
    Some(OverexpansionLoss { engines_lost, engines_before, exit_pressure_pa })
}

/// Record of a flaw that activated during a launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlawActivation {
    pub flaw_description: String,
    pub consequence: FlawConsequence,
    /// Whose flaw it was, as the player reads it: "Engine Merlin",
    /// "Rocket Upgoer 5 S1", "Reactor Kilopower". (Saved as
    /// `engine_name` before rocket and reactor flaws stopped borrowing
    /// the first stage's engine name.)
    #[serde(alias = "engine_name")]
    pub source: String,
    /// The flight event it fired at, when it fired at one: the launch
    /// popup lists the ascent in this order. `None` for endurance and
    /// reactor rolls, and for records from before events existed.
    #[serde(default)]
    pub event: Option<FlightEvent>,
}

/// `FlawActivation::source` for an engine flaw.
pub fn engine_source(engine_name: &str) -> String {
    format!("Engine {engine_name}")
}

/// `FlawActivation::source` for a rocket flaw on a stage.
pub fn rocket_source(stage_name: &str) -> String {
    format!("Rocket {stage_name}")
}

/// Record of a launch attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchRecord {
    pub launch_date: GameDate,
    pub rocket_name: String,
    pub contract_id: Option<ContractId>,
    pub destination: String,
    pub payload_kg: f64,
    pub outcome: LaunchOutcome,
    pub flaws_activated: Vec<FlawActivation>,
}

/// Outcome of a launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LaunchOutcome {
    Success,
    PartialFailure { reason: String },
    Failure { reason: String },
}

/// Result of simulating a launch, before applying to game state.
pub struct LaunchSimResult {
    pub outcome: LaunchOutcome,
    pub flaws_activated: Vec<FlawActivation>,
    /// The design after flaw degradation (what's actually flying).
    pub degraded_design: RocketDesign,
    /// Engine flaws that fired, to mark discovered on their owners.
    pub engine_discoveries: Vec<(FlawOwner, usize)>,
    /// Rocket flaws that fired, by id, to mark discovered on the project.
    pub rocket_flaw_discoveries: Vec<FlawId>,
    /// The stage groups the ascent fired — every group the first leg
    /// burns — so the flight loop does not roll them again.
    pub flaw_rolled_groups: std::collections::HashSet<usize>,
}

/// Simulate a launch. This does not modify any state — it returns a result
/// that the caller applies.
///
/// The ascent is one timeline: the stage groups the first leg of the
/// route fires (`ascent_groups`), walked event by event
/// (`ascent_events`), each rolling its engine and rocket flaws. The
/// first ignition also faces flow separation at pad pressure. A stage
/// loss ends the walk. The degraded design is then judged against the
/// route's delta-v. Groups that first fire on a later leg roll in the
/// flight loop when they do.
#[allow(clippy::too_many_arguments)] // the three flaw sources plus the balance cut; a bundle struct would just rename them
pub fn simulate_launch(
    design: &RocketDesign,
    destination: &str,
    payload_kg: f64,
    engine_projects: &[EngineProject],
    rocket_flaws: &[crate::flaw::Flaw],
    contracted_engines: &[ContractedEngine],
    rng: &mut StdRng,
    flight_cfg: &crate::balance_config::FlightConfig,
    nozzle_cfg: &crate::balance_config::NozzleConfig,
) -> LaunchSimResult {
    let from = "earth_surface";
    let required_dv = crate::location::DELTA_V_MAP
        .shortest_path_for_rocket(from, destination, design, payload_kg)
        .map(|(_, dv)| dv)
        .unwrap_or(f64::INFINITY);

    let groups = ascent_groups(design, destination, payload_kg, from);
    let ambient = crate::location::DELTA_V_MAP.surface_properties(from)
        .filter(|p| p.has_atmosphere)
        .map_or(0.0, |p| p.ambient_pressure_pa);

    let engines = engine_flaw_table(engine_projects, contracted_engines);
    // The snapshot's owner id is a placeholder: discoveries leave here
    // by flaw id and the caller knows whose rocket it is.
    let rockets = rocket_flaw_table(RocketProjectId(0), rocket_flaws);

    let mut degraded = design.clone();
    let mut activations: Vec<FlawActivation> = Vec::new();
    let mut engine_discoveries: Vec<(FlawOwner, usize)> = Vec::new();
    let mut rocket_flaw_discoveries: Vec<FlawId> = Vec::new();

    for event in ascent_events(&groups, ambient > 0.0) {
        let roll = roll_event_flaws(rng, &mut degraded, event, &engines, &rockets);
        activations.extend(roll.activations);
        for (owner, fi, id) in roll.discoveries {
            match owner {
                FlawOwner::Engine { .. } => engine_discoveries.push((owner, fi)),
                FlawOwner::Rocket(_) => rocket_flaw_discoveries.push(id),
                FlawOwner::Reactor(_) => {}
            }
        }
        if roll.lost.is_some() {
            break;
        }
        // Flow separation: the first group fires into pad pressure.
        if event == (FlightEvent::Ignition { group: 0 }) && ambient > 0.0 {
            for stage in degraded.stage_groups[0].iter_mut() {
                let source = engine_source(&stage.engine.name);
                let Some(loss) = roll_overexpansion(rng, stage, ambient, nozzle_cfg) else { continue };
                let total = loss.engines_lost >= loss.engines_before;
                activations.push(FlawActivation {
                    flaw_description: if total {
                        format!(
                            "All {} engine(s) destroyed by flow separation (exit {:.0} kPa at {:.0} kPa ambient)",
                            loss.engines_before, loss.exit_pressure_pa / 1000.0, ambient / 1000.0,
                        )
                    } else {
                        format!(
                            "{} of {} engine(s) destroyed by flow separation (exit {:.0} kPa at {:.0} kPa ambient)",
                            loss.engines_lost, loss.engines_before,
                            loss.exit_pressure_pa / 1000.0, ambient / 1000.0,
                        )
                    },
                    consequence: if total { FlawConsequence::StageLoss } else { FlawConsequence::EngineLoss },
                    source,
                    event: Some(event),
                });
            }
        }
    }

    // Judge the vehicle as it now is the way the planner judged the
    // design: usable delta-v is the vacuum figure less the ascent's
    // gravity loss (see `DesignPerformance`). The sea-level Isp penalty
    // is *not* pre-applied to the design here — the flight's first burn
    // charges it, once, in `Rocket::burn_group`; scaling the design as
    // well used to charge it twice.
    let degraded_dv = DesignPerformance::compute(&degraded, payload_kg, "earth_surface")
        .total_planner_dv();

    // Determine outcome. Anything short of nominal names what went wrong:
    // a shortfall with no attribution tells the player nothing they can act
    // on, and the reason built here is the one carried all the way to the
    // arrival event and the launch record.
    let outcome = if degraded_dv >= required_dv {
        LaunchOutcome::Success
    } else {
        let shortfall = ((1.0 - degraded_dv / required_dv) * 100.0).round();
        let reason = describe_shortfall(&activations, shortfall);
        if degraded_dv >= required_dv * flight_cfg.partial_failure_cut {
            LaunchOutcome::PartialFailure { reason }
        } else {
            LaunchOutcome::Failure { reason }
        }
    };

    LaunchSimResult {
        outcome,
        flaws_activated: activations,
        degraded_design: degraded,
        engine_discoveries,
        rocket_flaw_discoveries,
        flaw_rolled_groups: groups.into_iter().collect(),
    }
}

/// Rank an activation by how much of the shortfall it plausibly explains,
/// so the launch report leads with the thing worth looking at rather than
/// whichever flaw happened to roll first.
fn severity(activation: &FlawActivation) -> f64 {
    match activation.consequence {
        FlawConsequence::StageLoss => f64::INFINITY,
        FlawConsequence::EngineLoss => 1.0e6,
        FlawConsequence::PerformanceDegradation(frac) => frac,
    }
}

/// Build the reason text for a launch that came up short, naming its most
/// severe cause. With nothing activated the vehicle simply never had the
/// delta-v, which is a design problem and worth saying so plainly.
fn describe_shortfall(activations: &[FlawActivation], shortfall: f64) -> String {
    let primary = activations.iter().max_by(|a, b| {
        severity(a).partial_cmp(&severity(b)).unwrap_or(std::cmp::Ordering::Equal)
    });
    match primary {
        Some(cause) if activations.len() > 1 => format!(
            "{} (+{} more) — {}% delta-v shortfall",
            cause.flaw_description,
            activations.len() - 1,
            shortfall,
        ),
        Some(cause) => format!(
            "{} — {}% delta-v shortfall",
            cause.flaw_description, shortfall,
        ),
        None => format!(
            "Design {}% short of the required delta-v (no flaws activated)",
            shortfall,
        ),
    }
}

/// Apply a flaw consequence to a specific stage in a cloned design.
/// PerformanceDegradation and EngineLoss are scoped to the specific stage.
/// StageLoss removes the entire stage from its group.
pub fn apply_consequence_to_stage(
    design: &mut RocketDesign,
    consequence: &FlawConsequence,
    group_index: usize,
    stage_index: usize,
) {
    if group_index >= design.stage_groups.len() {
        return;
    }
    let group = &mut design.stage_groups[group_index];
    if stage_index >= group.len() {
        return;
    }
    match consequence {
        FlawConsequence::PerformanceDegradation(frac) => {
            // Reduce thrust and Isp only on this specific stage
            group[stage_index].engine.thrust_n *= 1.0 - frac;
            group[stage_index].engine.isp_s *= 1.0 - frac;
        }
        FlawConsequence::EngineLoss => {
            // Remove one engine from this specific stage. A stage with no
            // engine left is dead weight: without this the planner's
            // mass-ratio delta-v still counted it, the pad judged the
            // launch sound, and the flight loop stranded it on a burn
            // that produced no thrust.
            if group[stage_index].engine_count > 0 {
                group[stage_index].engine_count -= 1;
            }
            if group[stage_index].engine_count == 0 {
                group[stage_index].disable();
            }
        }
        FlawConsequence::StageLoss => group[stage_index].disable(),
    }
}

/// Apply a flaw consequence to a specific reactor on a stage.
///
/// `PerformanceDegradation(f)` scales the reactor's steady output by
/// `1-f`; `EngineLoss` ("reactor shutdown") zeroes it; `StageLoss`
/// disables the whole stage. Reduced power output cascades through the
/// flight power model on its own — `run_daily_power_tick` strands the
/// flight on brownout, and `group_effective_thrust_n` derates any
/// electric engine drawing that power.
pub fn apply_reactor_consequence_to_stage(
    design: &mut RocketDesign,
    consequence: &FlawConsequence,
    group_index: usize,
    stage_index: usize,
    reactor_id: ReactorId,
) {
    let group = match design.stage_groups.get_mut(group_index) {
        Some(g) => g,
        None => return,
    };
    let stage = match group.get_mut(stage_index) {
        Some(s) => s,
        None => return,
    };
    match consequence {
        FlawConsequence::PerformanceDegradation(frac) => {
            for src in stage.power_sources.iter_mut() {
                if let crate::power::PowerSourceKind::Reactor { design: rd } = &mut src.kind {
                    if rd.id == reactor_id {
                        rd.steady_w *= 1.0 - frac;
                    }
                }
            }
        }
        FlawConsequence::EngineLoss => {
            // Reactor shutdown — zero its output. The power cascade does
            // the rest (brownout stranding / electric-thrust derating).
            for src in stage.power_sources.iter_mut() {
                if let crate::power::PowerSourceKind::Reactor { design: rd } = &mut src.kind {
                    if rd.id == reactor_id {
                        rd.steady_w = 0.0;
                    }
                }
            }
        }
        FlawConsequence::StageLoss => stage.disable(),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use crate::engine::{EngineDesign, Propulsion, EngineCycle, PropellantFraction};
    use crate::propellant::Propellant;
    use crate::rocket::{RocketDesign, RocketDesignId};
    use crate::stage::{Stage, StageId};
    use crate::engine_project::{EngineProject, EngineProjectId, PropellantPreset};
    use crate::rocket_project::{RocketProject, RocketProjectId};
    use crate::flaw::{Flaw, FlawId, FlawTrigger};

    fn make_engine(id: u64) -> EngineDesign {
        EngineDesign {
            id: EngineId(id),
            name: format!("TestEngine{}", id),
            cycle: EngineCycle::GasGenerator,
            // 2.5 MN on a 53 t stage: liftoff TWR 2.4. At the original
            // 1 MN the fixture could not leave the pad (TWR 0.96) and only
            // "reached LEO" through the old ascent model's level-at-11-km
            // handover (17_3_PHYSICS.md D7).
            thrust_n: 2_500_000.0,
            isp_s: 300.0,
            mass_kg: 1000.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.6 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.4 },
            ],
            propulsion: Propulsion::nozzle(9_000_000.0, 10.96, 1.2, false),
        }
    }

    fn make_stage(engine_id: u64) -> Stage {
        Stage {
            id: StageId(engine_id),
            name: format!("S{}", engine_id),
            engine: make_engine(engine_id),
            engine_count: 1,
            propellant_mass_kg: 50_000.0,
            structural_mass_kg: 2_000.0,
            fairing: None,
            power_sources: Vec::new(),
        }
    }

    fn make_design() -> RocketDesign {
        RocketDesign {
            id: RocketDesignId(1),
            name: "TestRocket".into(),
            stage_groups: vec![
                vec![make_stage(1)],
                vec![make_stage(2)],
            ],
        }
    }

    fn make_engine_project(id: u64, flaws: Vec<Flaw>) -> EngineProject {
        let mut ep = EngineProject::new(
            EngineProjectId(id),
            EngineId(id),
            format!("TestEngine{}", id),
            EngineCycle::GasGenerator,
            PropellantPreset::Kerolox,
            1.0,
            &crate::balance_config::BalanceConfig::default(),
        ).unwrap();
        ep.flaws = flaws;
        ep
    }

    fn make_rocket_project(design: RocketDesign, flaws: Vec<Flaw>) -> RocketProject {
        let mut rp = RocketProject::new(RocketProjectId(1), design, &crate::balance_config::BalanceConfig::default());
        rp.flaws = flaws;
        rp
    }

    /// The sea-level nozzle penalty is paid once, by the first burn.
    /// The pad simulation used to scale the design's Isp and thrust by
    /// the penalty *and* hand that design to a flight whose first burn
    /// applied it again, so a sea-level stage flew at the penalty
    /// squared.
    #[test]
    fn overexpansion_is_charged_once_across_launch_and_first_leg() {
        use crate::engine::{EngineCycle, EngineDesign, Propulsion, EngineId, PropellantFraction, G0};
        use crate::propellant::Propellant;
        use crate::rocket::{RocketDesignId, RocketId};
        use crate::stage::{Stage, StageId};

        let ambient = 101_325.0;
        let engine = |id: u64, thrust: f64, isp: f64, exit_pa: f64| EngineDesign {
            id: EngineId(id), name: format!("E{id}"), cycle: EngineCycle::GasGenerator,
            thrust_n: thrust, mass_kg: 500.0, isp_s: isp,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
            propulsion: Propulsion::nozzle(9_000_000.0, crate::nozzle::expansion_ratio_for_exit_pressure(9_000_000.0, exit_pa, 1.2), 1.2, false),
        };
        let stage = |id: u64, e: EngineDesign, prop: f64, dry: f64| Stage {
            id: StageId(id), name: format!("S{id}"), engine: e, engine_count: 1,
            propellant_mass_kg: prop, structural_mass_kg: dry, fairing: None,
            power_sources: Vec::new(),
        };
        // A first stage whose nozzle is built for altitude: 40 kPa exit at
        // a 101 kPa pad is well into the penalty.
        let e1 = engine(1, 1_000_000.0, 280.0, 40_000.0);
        let frac = e1.isp_fraction_at(ambient);
        assert!(frac < 0.95, "fixture premise: the nozzle is penalised at sea level");
        let design = RocketDesign {
            id: RocketDesignId(1), name: "Probe".into(),
            stage_groups: vec![
                vec![stage(1, e1.clone(), 50_000.0, 3_000.0)],
                vec![stage(2, engine(2, 200_000.0, 340.0, 70_000.0), 10_000.0, 500.0)],
            ],
        };
        let mut rng = StdRng::seed_from_u64(1);
        let sim = simulate_launch(&design, "leo", 0.0, &[], &[], &[], &mut rng, &crate::balance_config::FlightConfig::default(), &crate::balance_config::NozzleConfig::default());

        // The vehicle handed to the flight is the design as built.
        assert_eq!(sim.degraded_design.stage_groups[0][0].engine.isp_s, e1.isp_s,
            "no flaws fired, so the flight's design carries the nominal Isp");

        // The first burn pays the penalty exactly once, at the fraction the
        // ascent averaged over the climb: less than the pad figure, since
        // the air thins as the propellant goes, and less than 1.
        let averaged = DesignPerformance::compute(&sim.degraded_design, 0.0, "earth_surface")
            .ascent.isp_fraction_by_group[0];
        assert!(frac < averaged && averaged < 1.0,
            "the climb averages the penalty: pad {frac:.4} < flown {averaged:.4} < 1");
        let mut rocket = sim.degraded_design.instantiate(RocketId(1), 0.0);
        let m0: f64 = sim.degraded_design.stage_groups.iter().flatten().map(|s| s.wet_mass_kg()).sum();
        let result = rocket.burn_sequential(&sim.degraded_design, 1_000.0, "earth_surface");
        let prop_used = 50_000.0 - rocket.stage_states[0][0].propellant_remaining_kg;
        let implied_ve = result.dv_achieved / (m0 / (m0 - prop_used)).ln();
        let ratio = implied_ve / (e1.isp_s * G0);
        assert!((ratio - averaged).abs() < 1e-6,
            "exhaust velocity should be nominal × {averaged:.4} (the ascent-averaged fraction), \
             got × {ratio:.4}; the pad fraction is {frac:.4} and frac² would be {:.4}", frac * frac);
    }

    #[test]
    fn test_launch_no_flaws_success() {
        let design = make_design();
        let ep1 = make_engine_project(1, vec![]);
        let ep2 = make_engine_project(2, vec![]);
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = simulate_launch(
            &design, "leo", 0.0,
            &[ep1, ep2], &rp.flaws, &[], &mut rng,
            &crate::balance_config::FlightConfig::default(),
            &crate::balance_config::NozzleConfig::default(),
        );

        assert!(matches!(result.outcome, LaunchOutcome::Success));
        assert!(result.flaws_activated.is_empty());
    }

    #[test]
    fn test_launch_with_guaranteed_flaw() {
        let design = make_design();
        let flaw = Flaw {
            id: FlawId(1),
            description: "Turbopump seal failure".into(),
            consequence: FlawConsequence::PerformanceDegradation(0.5),
            activation_chance: 1.0, // guaranteed activation
            discovery_roll: 0.5,
            discovered: false, trigger: FlawTrigger::PerFlight,
            phase: crate::flaw::FlawPhase::Burn, stage_group: None, exposure: crate::flaw::FlawExposure::Bench,
        };
        let ep1 = make_engine_project(1, vec![flaw]);
        let ep2 = make_engine_project(2, vec![]);
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = simulate_launch(
            &design, "leo", 0.0,
            &[ep1, ep2], &rp.flaws, &[], &mut rng,
            &crate::balance_config::FlightConfig::default(),
            &crate::balance_config::NozzleConfig::default(),
        );

        assert_eq!(result.flaws_activated.len(), 1);
        assert_eq!(result.flaws_activated[0].flaw_description, "Turbopump seal failure");
        // Should have discovered the flaw
        assert_eq!(result.engine_discoveries.len(), 1);
    }

    /// A launch that comes up short has to say *what* came up short. The
    /// old text was a bare percentage, or "degraded performance" when the
    /// list happened to be empty — neither tells the player what to fix.
    #[test]
    fn shortfall_reasons_name_their_cause() {
        let degradation = |desc: &str, frac: f64| FlawActivation {
            flaw_description: desc.into(),
            consequence: FlawConsequence::PerformanceDegradation(frac),
            source: "Engine Lifter".into(),
            event: None,
        };

        // Nothing activated: the vehicle simply never had the delta-v.
        let reason = describe_shortfall(&[], 4.0);
        assert!(reason.contains("Design 4% short"), "got {reason:?}");
        assert!(!reason.contains("degraded performance"), "got {reason:?}");

        // One cause: named, with the shortfall alongside it.
        let reason = describe_shortfall(&[degradation("Turbopump vibration", 0.05)], 3.0);
        assert_eq!(reason, "Turbopump vibration — 3% delta-v shortfall");

        // Several: lead with the worst, and admit there were others.
        let reason = describe_shortfall(&[
            degradation("Minor chatter", 0.01),
            degradation("Turbopump vibration", 0.20),
        ], 3.0);
        assert!(reason.starts_with("Turbopump vibration"), "got {reason:?}");
        assert!(reason.contains("+1 more"), "got {reason:?}");

        // Losing hardware outranks any performance hit, however large.
        let reason = describe_shortfall(&[
            degradation("Minor chatter", 0.9),
            FlawActivation {
                flaw_description: "Engine shutdown".into(),
                consequence: FlawConsequence::EngineLoss,
                source: "Engine Lifter".into(),
                event: None,
            },
        ], 3.0);
        assert!(reason.starts_with("Engine shutdown"), "got {reason:?}");
    }

    /// The reason the sim computes is the one the player must eventually
    /// read, so it has to name the flaw rather than restate the arithmetic.
    #[test]
    fn a_guaranteed_flaw_is_named_in_the_outcome_reason() {
        let design = make_design();
        let flaw = Flaw {
            id: FlawId(1),
            description: "Turbopump seal failure".into(),
            consequence: FlawConsequence::PerformanceDegradation(0.5),
            activation_chance: 1.0,
            discovery_roll: 0.5,
            discovered: false, trigger: FlawTrigger::PerFlight,
            phase: crate::flaw::FlawPhase::Burn, stage_group: None, exposure: crate::flaw::FlawExposure::Bench,
        };
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        const PAYLOAD_KG: f64 = 5_000.0;
        // Heavy enough that the first stage losing half its performance
        // actually costs the vehicle orbit rather than eating into margin.
        let result = simulate_launch(
            &design, "leo", PAYLOAD_KG,
            &[make_engine_project(1, vec![flaw]), make_engine_project(2, vec![])],
            &rp.flaws, &[], &mut rng,
            &crate::balance_config::FlightConfig::default(),
            &crate::balance_config::NozzleConfig::default(),
        );

        let reason = match &result.outcome {
            LaunchOutcome::PartialFailure { reason } | LaunchOutcome::Failure { reason } => reason,
            LaunchOutcome::Success => panic!("a 50% performance loss should not fly nominally"),
        };
        assert!(reason.contains("Turbopump seal failure"), "got {reason:?}");
    }

    #[test]
    fn test_launch_stage_loss_causes_failure() {
        let design = make_design();
        let flaw = Flaw {
            id: FlawId(1),
            description: "Structural failure".into(),
            consequence: FlawConsequence::StageLoss,
            activation_chance: 1.0,
            discovery_roll: 0.5,
            discovered: false, trigger: FlawTrigger::PerFlight,
            phase: crate::flaw::FlawPhase::Burn, stage_group: None, exposure: crate::flaw::FlawExposure::Bench,
        };
        let ep1 = make_engine_project(1, vec![flaw]);
        let ep2 = make_engine_project(2, vec![]);
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        // With a heavy payload, losing a stage should cause failure
        let result = simulate_launch(
            &design, "gto", 5000.0,
            &[ep1, ep2], &rp.flaws, &[], &mut rng,
            &crate::balance_config::FlightConfig::default(),
            &crate::balance_config::NozzleConfig::default(),
        );

        // Should be failure or partial failure (not success)
        assert!(!matches!(result.outcome, LaunchOutcome::Success));
    }

    #[test]
    fn test_launch_rocket_flaw_activates() {
        let design = make_design();
        let ep1 = make_engine_project(1, vec![]);
        let ep2 = make_engine_project(2, vec![]);
        let flaw = Flaw {
            id: FlawId(1),
            description: "Separation failure".into(),
            consequence: FlawConsequence::StageLoss,
            activation_chance: 1.0,
            discovery_roll: 0.5,
            discovered: false, trigger: FlawTrigger::PerFlight,
            phase: crate::flaw::FlawPhase::Burn, stage_group: None, exposure: crate::flaw::FlawExposure::Bench,
        };
        let rp = make_rocket_project(design.clone(), vec![flaw]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = simulate_launch(
            &design, "leo", 0.0,
            &[ep1, ep2], &rp.flaws, &[], &mut rng,
            &crate::balance_config::FlightConfig::default(),
            &crate::balance_config::NozzleConfig::default(),
        );

        assert_eq!(result.flaws_activated.len(), 1);
        assert_eq!(result.rocket_flaw_discoveries.len(), 1);
        // A rocket flaw names the stage it struck, not its engine.
        let stage = &design.stage_groups[0][0].name;
        assert_eq!(result.flaws_activated[0].source, format!("Rocket {stage}"));
    }

    /// An endurance flaw's chance is over a year of service; it rolls
    /// daily in flight, never as a one-shot on the pad. Even a certain
    /// one stays quiet at launch.
    #[test]
    fn endurance_rocket_flaws_do_not_roll_on_the_pad() {
        let design = make_design();
        let ep1 = make_engine_project(1, vec![]);
        let ep2 = make_engine_project(2, vec![]);
        let flaw = Flaw {
            id: FlawId(1),
            description: "Ullage motor propellant slowly vents".into(),
            consequence: FlawConsequence::EngineLoss,
            activation_chance: 1.0,
            discovery_roll: 0.5,
            discovered: false, trigger: FlawTrigger::PerDay,
            phase: crate::flaw::FlawPhase::Burn, stage_group: None, exposure: crate::flaw::FlawExposure::Bench,
        };
        let rp = make_rocket_project(design.clone(), vec![flaw]);
        for seed in 0..20 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = simulate_launch(
                &design, "leo", 0.0,
                &[ep1.clone(), ep2.clone()], &rp.flaws, &[], &mut rng,
                &crate::balance_config::FlightConfig::default(),
                &crate::balance_config::NozzleConfig::default(),
            );
            assert!(result.flaws_activated.is_empty(), "seed {seed}: {:?}", result.flaws_activated);
            assert!(result.rocket_flaw_discoveries.is_empty());
        }
    }

    /// Launch records saved before `source` read `engine_name`.
    #[test]
    fn an_old_activation_record_still_loads() {
        let old = r#"{"flaw_description":"Turbopump seal failure","consequence":"EngineLoss","engine_name":"Flamey"}"#;
        let a: FlawActivation = serde_json::from_str(old).unwrap();
        assert_eq!(a.source, "Flamey");
    }

    fn reactor_stage(engine_id: u64, reactor_id: u64) -> Stage {
        use crate::power::PowerSource;
        use crate::reactor::{EnrichmentLevel, ReactorDesign, ReactorId};
        let design = ReactorDesign::new(ReactorId(reactor_id), "R".into(), 1.0, EnrichmentLevel::Leu, &crate::balance_config::CostsConfig::default());
        let mut stage = make_stage(engine_id);
        stage.power_sources = vec![PowerSource::from_reactor_design(design)];
        stage
    }

    #[test]
    fn test_apply_reactor_consequence() {
        use crate::power::PowerSourceKind;
        use crate::reactor::ReactorId;
        let mut design = RocketDesign {
            id: RocketDesignId(1),
            name: "R".into(),
            stage_groups: vec![vec![reactor_stage(1, 50)]],
        };
        let steady_before = match &design.stage_groups[0][0].power_sources[0].kind {
            PowerSourceKind::Reactor { design } => design.steady_w,
            _ => panic!("expected reactor"),
        };
        assert!(steady_before > 0.0);

        // PerformanceDegradation scales the output.
        apply_reactor_consequence_to_stage(
            &mut design, &FlawConsequence::PerformanceDegradation(0.25), 0, 0, ReactorId(50),
        );
        let steady_after = match &design.stage_groups[0][0].power_sources[0].kind {
            PowerSourceKind::Reactor { design } => design.steady_w,
            _ => unreachable!(),
        };
        assert!((steady_after - steady_before * 0.75).abs() < 1.0);

        // EngineLoss (shutdown) zeroes the output.
        apply_reactor_consequence_to_stage(
            &mut design, &FlawConsequence::EngineLoss, 0, 0, ReactorId(50),
        );
        let steady_dead = match &design.stage_groups[0][0].power_sources[0].kind {
            PowerSourceKind::Reactor { design } => design.steady_w,
            _ => unreachable!(),
        };
        assert_eq!(steady_dead, 0.0);

        // StageLoss zeroes the whole stage.
        apply_reactor_consequence_to_stage(
            &mut design, &FlawConsequence::StageLoss, 0, 0, ReactorId(50),
        );
        assert_eq!(design.stage_groups[0][0].engine_count, 0);
        assert_eq!(design.stage_groups[0][0].propellant_mass_kg, 0.0);
    }

    #[test]
    fn test_zero_activation_chance_never_fires() {
        let design = make_design();
        let flaw = Flaw {
            id: FlawId(1),
            description: "Hidden flaw".into(),
            consequence: FlawConsequence::StageLoss,
            activation_chance: 0.0,
            discovery_roll: 0.5,
            discovered: false, trigger: FlawTrigger::PerFlight,
            phase: crate::flaw::FlawPhase::Burn, stage_group: None, exposure: crate::flaw::FlawExposure::Bench,
        };
        let ep1 = make_engine_project(1, vec![flaw]);
        let ep2 = make_engine_project(2, vec![]);
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = simulate_launch(
            &design, "leo", 0.0,
            &[ep1, ep2], &rp.flaws, &[], &mut rng,
            &crate::balance_config::FlightConfig::default(),
            &crate::balance_config::NozzleConfig::default(),
        );

        assert!(result.flaws_activated.is_empty());
        assert!(matches!(result.outcome, LaunchOutcome::Success));
    }

    #[test]
    fn ascent_events_walk_the_stack_in_order() {
        use FlightEvent::*;
        assert_eq!(
            ascent_events(&[0, 1], true),
            vec![Ignition { group: 0 }, MaxQ, Burn { group: 0 }, Separation { group: 0 }, Ignition { group: 1 }, Burn { group: 1 }],
        );
        // Leaving an airless body there is no max-Q.
        assert_eq!(
            ascent_events(&[0], false),
            vec![Ignition { group: 0 }, Burn { group: 0 }],
        );
        // A group that first fires on a later leg: the one below it lets go first.
        assert_eq!(
            ascent_events(&[2], false),
            vec![Separation { group: 1 }, Ignition { group: 2 }, Burn { group: 2 }],
        );
        assert_eq!(Separation { group: 1 }.word(), "S2 separation");
        assert_eq!(MaxQ.word(), "max-Q");
    }

    fn rocket_flaw(id: u64, phase: crate::flaw::FlawPhase, group: usize) -> Flaw {
        Flaw {
            id: FlawId(id),
            description: format!("rocket flaw {id}"),
            consequence: FlawConsequence::PerformanceDegradation(0.01),
            activation_chance: 1.0, discovery_roll: 0.5, discovered: false,
            trigger: FlawTrigger::PerFlight,
            phase, stage_group: Some(group), exposure: crate::flaw::FlawExposure::Bench,
        }
    }

    /// Each rocket flaw rolls at its own event on its own stage, and
    /// only when the ascent reaches that event: a separation flaw on
    /// the top stage never fires on the way up, and max-Q flaws need
    /// an atmosphere. Engine flaws likewise roll at their phase.
    #[test]
    fn flaws_roll_only_at_their_event_and_stage() {
        use crate::flaw::FlawPhase;
        let design = make_design();
        let rockets = rocket_flaw_table(RocketProjectId(1), &[
            rocket_flaw(1, FlawPhase::Separation, 0),
            rocket_flaw(2, FlawPhase::Separation, 1),
            rocket_flaw(3, FlawPhase::MaxQ, 0),
            rocket_flaw(4, FlawPhase::Ignition, 1),
        ]);
        let mut igniter = rocket_flaw(5, FlawPhase::Ignition, 0);
        igniter.description = "igniter".into();
        igniter.stage_group = None;
        let engines = engine_flaw_table(&[make_engine_project(1, vec![igniter])], &[]);

        let fired = |atmosphere: bool| -> Vec<(String, FlightEvent)> {
            let mut rng = StdRng::seed_from_u64(1);
            let mut degraded = design.clone();
            let mut out = Vec::new();
            for event in ascent_events(&[0, 1], atmosphere) {
                let roll = roll_event_flaws(&mut rng, &mut degraded, event, &engines, &rockets);
                out.extend(roll.activations.into_iter().map(|a| (a.flaw_description, a.event.unwrap())));
            }
            out
        };
        assert_eq!(fired(true), vec![
            ("igniter".to_string(), FlightEvent::Ignition { group: 0 }),
            ("rocket flaw 3".to_string(), FlightEvent::MaxQ),
            ("rocket flaw 1".to_string(), FlightEvent::Separation { group: 0 }),
            ("rocket flaw 4".to_string(), FlightEvent::Ignition { group: 1 }),
        ]);
        assert!(!fired(false).iter().any(|(d, _)| d == "rocket flaw 3"), "no max-Q without an atmosphere");
    }

    /// A stage group beyond the design (the design was modified after
    /// the flaw was pinned) rolls on the last group rather than never.
    #[test]
    fn a_stage_group_past_the_design_clamps_to_the_last() {
        use crate::flaw::FlawPhase;
        let design = make_design();
        let rockets = rocket_flaw_table(RocketProjectId(1), &[rocket_flaw(1, FlawPhase::Burn, 5)]);
        let mut rng = StdRng::seed_from_u64(1);
        let mut degraded = design.clone();
        let roll = roll_event_flaws(&mut rng, &mut degraded, FlightEvent::Burn { group: 1 }, &[], &rockets);
        assert_eq!(roll.activations.len(), 1);
        assert_eq!(roll.discoveries[0].2, FlawId(1));
    }
}
