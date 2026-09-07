use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::company::Company;
use crate::contract::ContractId;
use crate::engine::EngineId;
use crate::engine_project::{EngineProject, EngineSource};
use crate::flaw::{FlawConsequence, FlawTrigger};
use crate::reactor::ReactorId;
use crate::rocket::{Rocket, RocketDesign};
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
    pub trigger: FlawTrigger,
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
            trigger: flaw.trigger,
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
    /// The same flaws, as (owner, index) for marking them discovered.
    pub discoveries: Vec<(FlawOwner, usize)>,
    /// Set when a `StageLoss` fired: the vehicle is gone and the roll
    /// stopped there, since nothing after the loss can be observed.
    pub lost: Option<String>,
}

/// Roll the engine flaws of `stages` as they fire. Each flaw's chance
/// is scaled by the stage's engine count (`1 - (1-p)^n`); a hit is
/// applied to that stage. Stops at the first `StageLoss`, so a firing
/// discovers at most one rocket-destroying flaw.
pub fn roll_engine_flaws(
    rng: &mut StdRng,
    design: &mut RocketDesign,
    stages: &[(usize, usize)],
    table: &[FlawRef],
) -> FlawRoll {
    let mut roll = FlawRoll::default();
    'stages: for &(gi, si) in stages {
        let Some(stage) = design.stage_groups.get(gi).and_then(|g| g.get(si)) else { continue };
        let (engine_id, engine_count, engine_name) =
            (stage.engine.id, stage.engine_count, stage.engine.name.clone());
        for flaw in table.iter().filter(|f| matches!(f.owner, FlawOwner::Engine { engine_id: id, .. } if id == engine_id)) {
            let effective_p = 1.0 - (1.0 - flaw.activation_chance).powi(engine_count as i32);
            if rng.gen::<f64>() < effective_p {
                roll.activations.push(FlawActivation {
                    flaw_description: flaw.description.clone(),
                    consequence: flaw.consequence.clone(),
                    engine_name: engine_name.clone(),
                });
                roll.discoveries.push((flaw.owner, flaw.flaw_index));
                apply_consequence_to_stage(design, &flaw.consequence, gi, si);
                if matches!(flaw.consequence, FlawConsequence::StageLoss) {
                    roll.lost = Some(flaw.description.clone());
                    break 'stages;
                }
            }
        }
    }
    roll
}

/// A random stage still attached to a flying vehicle, as (group, stage).
fn random_attached_stage(rng: &mut StdRng, design: &RocketDesign, rocket: &Rocket) -> Option<(usize, usize)> {
    let attached: Vec<(usize, usize)> = design.stage_groups.iter()
        .enumerate()
        .flat_map(|(gi, group)| {
            let stage_states = &rocket.stage_states;
            group.iter().enumerate()
                .filter(move |(si, _)| {
                    stage_states.get(gi)
                        .and_then(|g| g.get(*si))
                        .is_some_and(|ss| ss.attached)
                })
                .map(move |(si, _)| (gi, si))
        })
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
        apply_consequence_to_stage(design, &flaw.consequence, gi, si);
        roll.activations.push(FlawActivation {
            flaw_description: flaw.description.clone(),
            consequence: flaw.consequence.clone(),
            engine_name: String::new(),
        });
        roll.discoveries.push((flaw.owner, flaw.flaw_index));
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
pub fn roll_overexpansion(rng: &mut StdRng, stage: &mut Stage, ambient_pa: f64) -> Option<OverexpansionLoss> {
    let risk = stage.engine.overexpansion_destruction_risk(ambient_pa);
    if risk <= 0.0 {
        return None;
    }
    let engines_before = stage.engine_count;
    let engines_lost = (0..engines_before).filter(|_| rng.gen::<f64>() < risk).count() as u32;
    if engines_lost == 0 {
        return None;
    }
    let exit_pressure_pa = stage.engine.exit_pressure_pa;
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
    pub engine_name: String,
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
    /// Indices of flaws to mark as discovered on rocket projects.
    pub rocket_flaw_discoveries: Vec<usize>,
    /// Which stage groups had flaws rolled during the launch sim.
    pub flaw_rolled_groups: std::collections::HashSet<usize>,
}

/// Simulate a launch. This does not modify any state — it returns a result
/// that the caller applies.
///
/// The simulation:
/// 1. Rolls activation for each flaw (engine projects + rocket project + contracted engines)
/// 2. Applies consequences to a cloned design
/// 3. Computes delta-v with degraded performance
/// 4. Compares to required delta-v for the destination
pub fn simulate_launch(
    design: &RocketDesign,
    destination: &str,
    payload_kg: f64,
    engine_projects: &[EngineProject],
    rocket_flaws: &[crate::flaw::Flaw],
    contracted_engines: &[ContractedEngine],
    rng: &mut StdRng,
) -> LaunchSimResult {
    let mut rocket_flaw_discoveries: Vec<usize> = Vec::new();

    // Compute required delta-v for the destination using the stage-aware
    // planner (so e.g. an ion upper stage uses spiral dv on transfers).
    let required_dv = crate::location::DELTA_V_MAP
        .shortest_path_for_rocket("earth_surface", destination, design, payload_kg)
        .map(|(_, dv)| dv)
        .unwrap_or(f64::INFINITY);

    // Only roll flaws for the first stage group (group 0) at launch.
    // Upper stage flaws are rolled mid-flight when those stages actually fire.
    let groups_needed: usize = if design.stage_groups.is_empty() { 0 } else { 1 };

    // Clone the design so we can degrade it
    let mut degraded = design.clone();

    // Engine flaws on the firing groups. Reactor flaws are NOT rolled
    // here: a reactor runs from flight start, so its flaws roll in the
    // flight loop (once at flight start for PerFlight, daily for PerDay)
    // rather than at stage ignition.
    let firing: Vec<(usize, usize)> = (0..groups_needed)
        .flat_map(|gi| (0..design.stage_groups[gi].len()).map(move |si| (gi, si)))
        .collect();
    let table = engine_flaw_table(engine_projects, contracted_engines);
    let engine_roll = roll_engine_flaws(rng, &mut degraded, &firing, &table);
    let mut activations = engine_roll.activations;
    let engine_discoveries = engine_roll.discoveries;
    // Once a StageLoss activates the vehicle is gone — nothing after the
    // loss can be observed, so a launch discovers at most one
    // rocket-destroying flaw and rolls no overexpansion risk.
    let mut vehicle_lost = engine_roll.lost.is_some();

    // Roll rocket project flaws — only target groups that will fire
    if !vehicle_lost {
        for (fi, flaw) in rocket_flaws.iter().enumerate() {
            if rng.gen::<f64>() < flaw.activation_chance {
                // Pick a random stage group among those that will fire
                if groups_needed > 0 {
                    let gi = rng.gen_range(0..groups_needed);
                    let engine_name = degraded.stage_groups.get(gi)
                        .and_then(|g| g.first())
                        .map(|s| s.engine.name.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    activations.push(FlawActivation {
                        flaw_description: flaw.description.clone(),
                        consequence: flaw.consequence.clone(),
                        engine_name,
                    });
                    // Pick a random stage within the group
                    let si = if !degraded.stage_groups[gi].is_empty() {
                        rng.gen_range(0..degraded.stage_groups[gi].len())
                    } else { 0 };
                    apply_consequence_to_stage(&mut degraded, &flaw.consequence, gi, si);
                    if matches!(flaw.consequence, FlawConsequence::StageLoss) {
                        rocket_flaw_discoveries.push(fi);
                        vehicle_lost = true;
                        break;
                    }
                }
                rocket_flaw_discoveries.push(fi);
            }
        }
    }

    // Check overexpansion destruction risk for first stage group
    // (burning at sea level, 101325 Pa)
    let ambient = 101_325.0_f64;
    if groups_needed > 0 && !vehicle_lost {
        for stage in degraded.stage_groups[0].iter_mut() {
            let engine_name = stage.engine.name.clone();
            let Some(loss) = roll_overexpansion(rng, stage, ambient) else { continue };
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
                engine_name,
            });
        }
    }

    // Apply Isp penalty for overexpansion on first stage group (sea level).
    // Deliberately *not* recorded as an activation: this is nozzle geometry,
    // not a flaw, and the designer's "Eff dV" column already shows the
    // sea-level figure, so it is visible before the player commits.
    if !degraded.stage_groups.is_empty() {
        for stage in degraded.stage_groups[0].iter_mut() {
            let frac = stage.engine.isp_fraction_at(ambient);
            if frac < 1.0 {
                stage.engine.isp_s *= frac;
                stage.engine.thrust_n *= frac;
            }
        }
    }

    // Compute degraded delta-v
    let degraded_dv = degraded.total_delta_v(payload_kg);

    // Determine outcome. Anything short of nominal names what went wrong:
    // a shortfall with no attribution tells the player nothing they can act
    // on, and the reason built here is the one carried all the way to the
    // arrival event and the launch record.
    let outcome = if degraded_dv >= required_dv {
        LaunchOutcome::Success
    } else {
        let shortfall = ((1.0 - degraded_dv / required_dv) * 100.0).round();
        let reason = describe_shortfall(&activations, shortfall);
        if degraded_dv >= required_dv * 0.95 {
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
        flaw_rolled_groups: (0..groups_needed).collect(),
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
            // Remove one engine from this specific stage
            if group[stage_index].engine_count > 0 {
                group[stage_index].engine_count -= 1;
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
    use crate::engine::{EngineDesign, EngineCycle, PropellantFraction};
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
            thrust_n: 1_000_000.0,
            isp_s: 300.0,
            exit_pressure_pa: 100_000.0,
            needs_atmosphere: false,
            mass_kg: 1000.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.6 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.4 },
            ],
            power_draw_w: 0.0,
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
            discovery_probability: 0.5,
            discovered: false, trigger: FlawTrigger::PerFlight,
        };
        let ep1 = make_engine_project(1, vec![flaw]);
        let ep2 = make_engine_project(2, vec![]);
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = simulate_launch(
            &design, "leo", 0.0,
            &[ep1, ep2], &rp.flaws, &[], &mut rng,
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
            engine_name: "Lifter".into(),
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
                engine_name: "Lifter".into(),
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
            discovery_probability: 0.5,
            discovered: false, trigger: FlawTrigger::PerFlight,
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
            discovery_probability: 0.5,
            discovered: false, trigger: FlawTrigger::PerFlight,
        };
        let ep1 = make_engine_project(1, vec![flaw]);
        let ep2 = make_engine_project(2, vec![]);
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        // With a heavy payload, losing a stage should cause failure
        let result = simulate_launch(
            &design, "gto", 5000.0,
            &[ep1, ep2], &rp.flaws, &[], &mut rng,
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
            discovery_probability: 0.5,
            discovered: false, trigger: FlawTrigger::PerFlight,
        };
        let rp = make_rocket_project(design.clone(), vec![flaw]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = simulate_launch(
            &design, "leo", 0.0,
            &[ep1, ep2], &rp.flaws, &[], &mut rng,
        );

        assert_eq!(result.flaws_activated.len(), 1);
        assert_eq!(result.rocket_flaw_discoveries.len(), 1);
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
            discovery_probability: 0.5,
            discovered: false, trigger: FlawTrigger::PerFlight,
        };
        let ep1 = make_engine_project(1, vec![flaw]);
        let ep2 = make_engine_project(2, vec![]);
        let rp = make_rocket_project(design.clone(), vec![]);
        let mut rng = StdRng::seed_from_u64(42);

        let result = simulate_launch(
            &design, "leo", 0.0,
            &[ep1, ep2], &rp.flaws, &[], &mut rng,
        );

        assert!(result.flaws_activated.is_empty());
        assert!(matches!(result.outcome, LaunchOutcome::Success));
    }
}
