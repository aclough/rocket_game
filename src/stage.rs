use serde::{Serialize, Deserialize};

use crate::engine::EngineDesign;
use crate::power::{PowerSource, PowerSourceKind};

/// Unique identifier for a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StageId(pub u64);

/// A payload fairing that sits on top of a stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fairing {
    pub mass_kg: f64,
    pub diameter_m: f64,
}

/// A rocket stage: structural mass, engines, propellant, optional fairing,
/// and any power sources (batteries, panels, RTGs, etc.).
///
/// The stage holds a reference to its engine design (by clone) and the number of
/// engines of that type. It does NOT own fuel composition — that comes from the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    pub id: StageId,
    pub name: String,
    pub engine: EngineDesign,
    pub engine_count: u32,
    pub propellant_mass_kg: f64,
    pub structural_mass_kg: f64,
    pub fairing: Option<Fairing>,
    /// Power sources (batteries, solar panels, RTGs…) the player fitted to
    /// this stage. Empty is normal — and empty does not mean unpowered.
    /// Read it through [`Stage::effective_power_sources`] for anything
    /// physical; this field is only the explicit, editable list.
    #[serde(default)]
    pub power_sources: Vec<PowerSource>,
}

impl Stage {
    /// The stage's power sources as the physics sees them: the explicit
    /// list, or — when the player fitted nothing — the default battery
    /// every stage carries.
    ///
    /// Every mass, supply, capacity and drain calculation goes through
    /// here, so there is no such thing as a stage outside the power
    /// system. A stage used to be exempt when this list was empty, which
    /// made a design with no power sources immortal anywhere in the solar
    /// system.
    pub fn effective_power_sources(&self) -> std::borrow::Cow<'_, [PowerSource]> {
        if self.power_sources.is_empty() {
            std::borrow::Cow::Owned(vec![PowerSource::default_battery_for_stage(self)])
        } else {
            std::borrow::Cow::Borrowed(&self.power_sources)
        }
    }

    /// Dry mass: structural mass, all engines, the fairing if present, and
    /// the power sources.
    ///
    /// The default battery counts toward this — it is real kit, so it shows
    /// up in delta-v and thrust-to-weight like anything else bolted on.
    pub fn dry_mass_kg(&self) -> f64 {
        let engine_mass = self.engine.mass_kg * self.engine_count as f64;
        let fairing_mass = self.fairing.as_ref().map_or(0.0, |f| f.mass_kg);
        let power_mass: f64 = self.effective_power_sources().iter().map(|p| p.mass_kg).sum();
        self.structural_mass_kg + engine_mass + fairing_mass + power_mass
    }

    /// What one of this stage's power sources supplies at `sun_distance_au`:
    /// a fuel cell's peak if the stage's engine burns something it can run
    /// on (no solid, no xenon), otherwise the source's steady output.
    pub fn source_supply_w(&self, src: &PowerSource, sun_distance_au: f64) -> f64 {
        match src.kind {
            PowerSourceKind::FuelCell { peak_w, .. } => {
                if crate::power::fuel_cell_can_run_on(&self.engine) { peak_w } else { 0.0 }
            }
            _ => src.steady_output_w(sun_distance_au),
        }
    }

    /// Everything this stage's power sources supply at `sun_distance_au`.
    pub fn supply_w(&self, sun_distance_au: f64) -> f64 {
        self.effective_power_sources().iter()
            .map(|p| self.source_supply_w(p, sun_distance_au))
            .sum()
    }

    /// Battery capacity (kilowatt-days) aboard this stage, the default
    /// battery included when nothing was fitted.
    pub fn battery_capacity_kwd(&self) -> f64 {
        self.effective_power_sources().iter()
            .filter_map(|p| match p.kind {
                PowerSourceKind::Battery => Some(p.capacity_kwd),
                _ => None,
            })
            .sum()
    }

    /// Steady-state housekeeping draw in watts. Approximates ~1 W per 10 kg
    /// of dry mass (excluding power sources themselves so adding panels
    /// doesn't increase your own load).
    pub fn housekeeping_w(&self) -> f64 {
        let engine_mass = self.engine.mass_kg * self.engine_count as f64;
        let fairing_mass = self.fairing.as_ref().map_or(0.0, |f| f.mass_kg);
        let bus_mass = self.structural_mass_kg + engine_mass + fairing_mass;
        bus_mass * 0.1 // 1 W per 10 kg
    }

    /// Wet mass: dry mass + propellant.
    pub fn wet_mass_kg(&self) -> f64 {
        self.dry_mass_kg() + self.propellant_mass_kg
    }

    /// Total thrust from all engines on this stage (Newtons).
    pub fn total_thrust_n(&self) -> f64 {
        self.engine.thrust_n * self.engine_count as f64
    }

    /// Take the stage out of the vehicle: no engines, no thrust, no
    /// propellant. It stays in its group so stage indices keep lining
    /// up with a flying `Rocket`'s per-stage state.
    pub fn disable(&mut self) {
        self.engine_count = 0;
        self.engine.thrust_n = 0.0;
        self.engine.isp_s = 0.0;
        self.propellant_mass_kg = 0.0;
    }

    /// Propellant mass flow of the whole stage (all engines), kg/s.
    pub fn mass_flow_kg_s(&self) -> f64 {
        self.engine.mass_flow_rate() * self.engine_count as f64
    }

    /// Burn time in seconds (all propellant, all engines firing).
    pub fn burn_time_s(&self) -> f64 {
        let flow_rate = self.engine.mass_flow_rate() * self.engine_count as f64;
        if flow_rate <= 0.0 {
            return 0.0;
        }
        self.propellant_mass_kg / flow_rate
    }

    /// Delta-v this stage provides, given a payload mass sitting above it.
    /// Uses the Tsiolkovsky rocket equation: dv = Ve * ln(m0 / mf)
    /// where m0 = wet + payload, mf = dry + payload.
    pub fn delta_v(&self, payload_mass_kg: f64) -> f64 {
        let m0 = self.wet_mass_kg() + payload_mass_kg;
        let mf = self.dry_mass_kg() + payload_mass_kg;
        if mf <= 0.0 {
            return 0.0;
        }
        self.engine.exhaust_velocity() * (m0 / mf).ln()
    }
}

// ─── Sizing ──────────────────────────────────────────────────────────
//
// How a stage's tank and structure are chosen for the stack around it.
// Pure functions over `Stage` lists: the rocket designer calls them as
// the player edits, and anything headless — a policy that designs its
// own vehicle — can too (17_3_PHYSICS.md D5).

/// Thrust-to-weight each stage group is sized for at its own ignition.
///
/// The first stage has to lift the stack off the pad, so it wants real
/// margin — real launchers leave the ground between about 1.2 and 1.5.
/// Everything above ignites already moving, so 1.0 is enough to keep it
/// accelerating, and anything more is engine it didn't need.
///
/// Sizing every stage against its own thrust deliberately keeps the
/// answer independent of the designer's payload field: that defaults to
/// a nominal test mass most players never touch, and a rule that solved
/// for a mission delta-v would quietly hang every tank in the vehicle off
/// a number nobody chose.
pub const TARGET_LIFTOFF_TWR: f64 = 1.2;

pub const TARGET_STAGE_TWR: f64 = 1.0;

/// Vacuum delta-v a low-thrust stage is sized towards.
///
/// Spiral transfers out of Earth orbit run roughly 3-8 km/s depending on
/// how far out they go; the middle of that band gives an ion stage a tank
/// that can do its job without dominating the vehicle it rides on.
pub const LOW_THRUST_DV_TARGET: f64 = 6_000.0;

/// Floor and ceiling on an auto-sized tank. The floor keeps a stage the
/// solver can't satisfy (an engine too weak to lift what's above it)
/// visible and editable rather than empty; the ceiling matches the cap
/// the `+` key already enforces.
pub const MIN_AUTOSIZED_PROPELLANT: f64 = 100.0;

pub const MAX_AUTOSIZED_PROPELLANT: f64 = 2_000_000.0;

/// Step size for inline propellant adjustments (`+`/`-` in the
/// designer): ~10 seconds of burn time per press.
pub const PROPELLANT_STEP_BURN_SECONDS: f64 = 10.0;

/// Dry mass a stage would come to with a given propellant load.
///
/// Mirrors `recompute_structural_masses` followed by `Stage::dry_mass_kg`,
/// so the sizing solver prices its candidates exactly the way the designer
/// will price the answer — engines, fairing and power sources included,
/// not just the tank.
pub fn dry_mass_for(
    stage: &Stage, propellant_kg: f64, is_first: bool, has_interstage: bool,
) -> f64 {
    let mix: Vec<(crate::propellant::Propellant, f64)> = stage.engine.propellant_mix.iter()
        .map(|f| (f.propellant, f.mass_fraction))
        .collect();
    let structural = crate::structure::compute_structural_mass(
        propellant_kg, &mix, &stage.engine, stage.engine_count, is_first, has_interstage,
    ).total;
    let mut probe = stage.clone();
    probe.structural_mass_kg = structural;
    probe.dry_mass_kg()
}

/// Choose a propellant load for one stage, in the context of the stack
/// around it.
///
/// Every stage is sized by thrust-to-weight at its own ignition: whatever
/// tank leaves it at `TARGET_LIFTOFF_TWR` off the pad, or
/// `TARGET_STAGE_TWR` once it's already moving. Low-thrust stages are the
/// exception — a TWR target is meaningless for them — and are sized for
/// the spiral they fly instead.
///
/// A stage sized purely by burn time, as this used to be, can come out
/// too small to get what's above it moving. That doesn't show up as a
/// delta-v problem: it shows up as the next stage igniting slow and steep
/// and spending its propellant fighting gravity instead of going
/// sideways. Sizing each stage against what it actually has to lift puts
/// every handover somewhere the vehicle can recover from.
///
/// Solved by bisection because a tank pays for itself twice: propellant
/// mass, and the structure that has to hold it.
pub fn autosize_propellant(
    stage_groups: &[Vec<Stage>],
    group_index: usize,
    inner_index: usize,
    payload_kg: f64,
    launch_from: &str,
) -> f64 {
    let Some(stage) = stage_groups.get(group_index).and_then(|g| g.get(inner_index)) else {
        return MIN_AUTOSIZED_PROPELLANT;
    };
    let is_first = group_index == 0;
    let has_interstage = group_index + 1 < stage_groups.len();

    // Mass of every stage above this group, plus the payload. Fixed while
    // this tank is solved for.
    let mass_above: f64 = stage_groups[group_index + 1..].iter()
        .flat_map(|g| g.iter())
        .map(|s| s.wet_mass_kg())
        .sum::<f64>()
        + payload_kg;

    // Wet mass of this stage at a candidate propellant load.
    let wet_at = |p: f64| p + dry_mass_for(stage, p, is_first, has_interstage);

    let target = if stage.engine.is_low_thrust() {
        // Electric propulsion is *defined* by having a thrust-to-weight
        // far below 1 — the delta-v graph routes it from orbit and refuses
        // it the launch edge outright — so sizing it to a TWR target would
        // just floor every ion tank. Size it for the spiral it will
        // actually fly. This is also the case the old flat burn-time rule
        // got worst: 120 seconds of an ion engine's mass flow is grams.
        SizingTarget::DeltaV(LOW_THRUST_DV_TARGET)
    } else {
        // Siblings in the same group fire alongside this stage, so they
        // contribute both thrust and mass.
        let (sibling_thrust, sibling_mass): (f64, f64) = stage_groups[group_index].iter()
            .enumerate()
            .filter(|(si, _)| *si != inner_index)
            .fold((0.0, 0.0), |(t, m), (_, s)| (t + s.total_thrust_n(), m + s.wet_mass_kg()));
        let thrust = stage.total_thrust_n() + sibling_thrust;
        let gravity = crate::location::DELTA_V_MAP.surface_properties(launch_from)
            .map_or(9.81, |p| p.gravity_m_s2);
        if thrust <= 0.0 || gravity <= 0.0 {
            return MIN_AUTOSIZED_PROPELLANT;
        }
        // Ignition mass the target TWR allows, less everything that isn't
        // this stage. What's left is the budget for tank plus propellant.
        let twr = if is_first { TARGET_LIFTOFF_TWR } else { TARGET_STAGE_TWR };
        let budget = thrust / (twr * gravity) - mass_above - sibling_mass;
        SizingTarget::WetMass(budget)
    };

    let achieved = |p: f64| match target {
        SizingTarget::WetMass(_) => wet_at(p),
        SizingTarget::DeltaV(_) => {
            // This stage's own delta-v carrying everything above it —
            // the same expression `Stage::delta_v` evaluates.
            let dry = mass_above + dry_mass_for(stage, p, is_first, has_interstage);
            let wet = dry + p;
            if dry <= 0.0 || wet <= dry {
                0.0
            } else {
                stage.engine.exhaust_velocity() * (wet / dry).ln()
            }
        }
    };
    let goal = match target {
        SizingTarget::WetMass(m) => m,
        SizingTarget::DeltaV(dv) => dv,
    };

    // Both `achieved` functions rise monotonically with propellant, so
    // bisect. If even an empty tank overshoots — an engine too weak to
    // lift what's already above it — the floor is the honest answer, and
    // the designer's TWR column will say why.
    if achieved(MIN_AUTOSIZED_PROPELLANT) >= goal {
        return MIN_AUTOSIZED_PROPELLANT;
    }
    if achieved(MAX_AUTOSIZED_PROPELLANT) <= goal {
        return MAX_AUTOSIZED_PROPELLANT;
    }
    let (mut lo, mut hi) = (MIN_AUTOSIZED_PROPELLANT, MAX_AUTOSIZED_PROPELLANT);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if achieved(mid) <= goal { lo = mid } else { hi = mid }
    }
    let sized = lo;

    // Round *up* to 100 kg, the granularity `+`/`-` moves in. Rounding to
    // nearest would let the answer land under its own target — on a small
    // high-Isp stage the whole solution can be a couple of hundred kg, and
    // dropping to the lower step there costs a fifth of the delta-v.
    ((sized / 100.0).ceil() * 100.0)
        .clamp(MIN_AUTOSIZED_PROPELLANT, MAX_AUTOSIZED_PROPELLANT)
}

/// What `autosize_propellant` is solving for, so the bisection can share
/// one loop across the two rules.
enum SizingTarget {
    /// Total wet mass this stage should come to.
    WetMass(f64),
    /// Delta-v this stage should deliver.
    DeltaV(f64),
}

/// Compute thrust-scaled propellant step size for inline adjustments.
/// Rounded to nearest 100 kg, min 100 kg.
pub fn propellant_step(engine: &EngineDesign, engine_count: u32) -> f64 {
    let raw = engine.mass_flow_rate() * engine_count as f64 * PROPELLANT_STEP_BURN_SECONDS;
    (raw / 100.0).round().max(1.0) * 100.0
}

/// Recompute structural masses for all stage groups based on their
/// position: the aero shell depends on being group 0 (exposed to
/// airflow), the interstage on whether the group is the last.
pub fn recompute_structural_masses(stage_groups: &mut [Vec<Stage>]) {
    let n = stage_groups.len();
    for (gi, group) in stage_groups.iter_mut().enumerate() {
        let is_first = gi == 0;
        let has_interstage = gi + 1 < n;
        for stage in group.iter_mut() {
            let propellant_mix: Vec<(crate::propellant::Propellant, f64)> =
                stage.engine.propellant_mix.iter()
                    .map(|f| (f.propellant, f.mass_fraction))
                    .collect();
            let breakdown = crate::structure::compute_structural_mass(
                stage.propellant_mass_kg,
                &propellant_mix,
                &stage.engine,
                stage.engine_count,
                is_first,
                has_interstage,
            );
            stage.structural_mass_kg = breakdown.total;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::*;
    use crate::propellant::Propellant;

    fn test_engine() -> EngineDesign {
        EngineDesign {
            id: EngineId(1),
            name: "TestEngine".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 1_000_000.0,
            mass_kg: 500.0,
            isp_s: 300.0,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
            propulsion: Propulsion::nozzle(9_000_000.0, 10.96, 1.2, false),
        }
    }

    fn test_stage() -> Stage {
        Stage {
            id: StageId(1),
            name: "S1".into(),
            engine: test_engine(),
            engine_count: 1,
            propellant_mass_kg: 20_000.0,
            structural_mass_kg: 1_500.0,
            fairing: None,
            power_sources: Vec::new(),
        }
    }

    /// Mass of the battery a bare stage carries by default. Every dry-mass
    /// figure below includes it: the stage fits no power sources of its own,
    /// so `effective_power_sources` hands it the default kit, and that kit
    /// weighs something.
    fn default_battery_mass(s: &Stage) -> f64 {
        PowerSource::default_battery_for_stage(s).mass_kg
    }

    #[test]
    fn test_dry_mass_no_fairing() {
        let s = test_stage();
        // structural 1500 + 1 engine * 500 = 2000, + the default battery
        assert_eq!(s.dry_mass_kg(), 2000.0 + default_battery_mass(&s));
    }

    #[test]
    fn test_dry_mass_with_fairing() {
        let mut s = test_stage();
        s.fairing = Some(Fairing { mass_kg: 200.0, diameter_m: 4.0 });
        assert_eq!(s.dry_mass_kg(), 2200.0 + default_battery_mass(&s));
    }

    #[test]
    fn test_wet_mass() {
        let s = test_stage();
        assert_eq!(s.wet_mass_kg(), 22_000.0 + default_battery_mass(&s));
    }

    #[test]
    fn test_multi_engine_thrust() {
        let mut s = test_stage();
        s.engine_count = 9;
        assert_eq!(s.total_thrust_n(), 9_000_000.0);
    }

    #[test]
    fn test_multi_engine_dry_mass() {
        let mut s = test_stage();
        s.engine_count = 3;
        // 1500 + 3*500 = 3000, + the default battery
        assert_eq!(s.dry_mass_kg(), 3000.0 + default_battery_mass(&s));
    }

    #[test]
    fn test_burn_time() {
        let s = test_stage();
        let ve = s.engine.exhaust_velocity(); // 300 * 9.80665 ≈ 2941.995
        let flow = s.engine.thrust_n / ve; // 1e6 / 2942 ≈ 339.9
        let expected = 20_000.0 / flow;
        assert!((s.burn_time_s() - expected).abs() < 0.1, "got {}", s.burn_time_s());
    }

    #[test]
    fn test_delta_v_no_payload() {
        let s = test_stage();
        let ve = s.engine.exhaust_velocity();
        // Against the stage's real masses, not the bare 2000/22000: the
        // default battery is part of the dry mass it has to haul.
        let expected = ve * (s.wet_mass_kg() / s.dry_mass_kg()).ln();
        let dv = s.delta_v(0.0);
        assert!((dv - expected).abs() < 1.0, "expected {}, got {}", expected, dv);
        assert!(
            s.dry_mass_kg() > 2_000.0,
            "premise: the default battery is real mass, got {}", s.dry_mass_kg(),
        );
    }

    #[test]
    fn test_delta_v_with_payload() {
        let s = test_stage();
        let ve = s.engine.exhaust_velocity();
        let payload = 5_000.0;
        let expected = ve
            * ((s.wet_mass_kg() + payload) / (s.dry_mass_kg() + payload)).ln();
        let dv = s.delta_v(payload);
        assert!((dv - expected).abs() < 1.0, "expected {}, got {}", expected, dv);
    }

    #[test]
    fn test_more_payload_less_delta_v() {
        let s = test_stage();
        let dv_light = s.delta_v(1_000.0);
        let dv_heavy = s.delta_v(10_000.0);
        assert!(dv_light > dv_heavy);
    }
}
