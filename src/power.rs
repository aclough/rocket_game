//! Electrical power generation and storage on rocket stages.
//!
//! Phase 1: Battery, SolarPanel, RTG. Fuel cells (Phase 2) and reactors
//! (Phase 3) are stubbed in the enum but not surfaced in the UI yet.

use serde::{Deserialize, Serialize};

/// Solar irradiance at 1 AU, used as the reference for solar-panel rating.
pub const SOLAR_FLUX_1AU_W_M2: f64 = 1361.0;

/// Heat-rejection technology for a reactor's radiator. One variant for now;
/// the enum exists so future research can introduce better radiators
/// (heatpipe, pumped-fluid loops, droplet radiators…) without refactoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RadiatorKind {
    Standard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Radiator {
    pub kind: RadiatorKind,
    pub mass_kg: f64,
}

/// What kind of power source this is. Each variant carries its
/// kind-specific physics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PowerSourceKind {
    /// Pure storage — produces no power, only buffers it.
    Battery,
    /// Solar panel rated at `peak_w_at_1au` watts at Earth distance from
    /// the Sun. Output scales as 1/AU² with distance from the Sun.
    SolarPanel { peak_w_at_1au: f64 },
    /// Radioisotope thermoelectric generator — small constant trickle.
    Rtg { steady_w: f64 },
    /// Fuel cell — burns a tiny fraction of stage propellant for power.
    /// Phase 2.
    FuelCell { peak_w: f64, kg_per_kwd: f64 },
    /// Nuclear reactor — owns a cloned snapshot of the [`ReactorDesign`]
    /// the player researched. Mirrors how `Stage::engine` carries a
    /// cloned `EngineDesign`. The bundled radiator's heat-rejection
    /// efficiency comes from the design's `temperature_k`
    /// (Stefan-Boltzmann ∝ T⁴).
    Reactor {
        design: crate::reactor::ReactorDesign,
    },
}

/// A single power source on a stage. Mass is the total physical mass
/// (panel + structure, RTG + cask, reactor + radiator, etc.). Material
/// cost goes into the stage's bill of materials at build time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerSource {
    pub kind: PowerSourceKind,
    pub mass_kg: f64,
    /// Material cost (dollars). Labor is added by the manufacturing
    /// pipeline as part of the stage's build.
    #[serde(default)]
    pub material_cost: f64,
    /// Battery-only: stored energy in kilowatt-days. Source-instantiation
    /// time fills this to capacity for batteries; per-day flight ticks
    /// drain or recharge it from the demand/supply balance.
    #[serde(default)]
    pub stored_kwd: f64,
    /// Battery-only: maximum stored energy.
    #[serde(default)]
    pub capacity_kwd: f64,
}

impl PowerSource {
    /// Steady-state power output (watts) at the given distance from the
    /// Sun.
    ///
    /// For display and thrust derating this returns the rated output of
    /// every source — including fuel-cell peak power, since a fuel cell
    /// genuinely can supply that much continuously *as long as the
    /// stage's propellant lasts*. The per-day balance tick separately
    /// invokes fuel cells and deducts propellant for what they actually
    /// produced; batteries (zero-steady) are handled by the surge path.
    pub fn steady_output_w(&self, sun_distance_au: f64) -> f64 {
        match &self.kind {
            PowerSourceKind::Battery => 0.0,
            PowerSourceKind::SolarPanel { peak_w_at_1au } => {
                if sun_distance_au <= 0.0 { return 0.0; }
                peak_w_at_1au / (sun_distance_au * sun_distance_au)
            }
            PowerSourceKind::Rtg { steady_w } => *steady_w,
            PowerSourceKind::FuelCell { peak_w, .. } => *peak_w,
            PowerSourceKind::Reactor { design } => design.steady_w,
        }
    }

    /// Construct a default housekeeping battery for legacy stages.
    /// Sized to ~1 W per 10 kg of `dry_mass_kg` and a one-day reserve, so
    /// existing tests/saves keep working without authoring data.
    pub fn default_battery_for(dry_mass_kg: f64) -> Self {
        let housekeeping_w = dry_mass_kg * 0.1; // 1 W per 10 kg
        // capacity sized to one day at housekeeping load (kW × day)
        let capacity_kwd = (housekeeping_w / 1000.0) * 1.0;
        // Mass: lithium-ion ballpark ~250 Wh/kg ≈ 6 kWd/kg.
        let mass_kg = (capacity_kwd / 6.0).max(0.1);
        PowerSource {
            kind: PowerSourceKind::Battery,
            mass_kg,
            material_cost: 0.0,
            stored_kwd: capacity_kwd,
            capacity_kwd,
        }
    }

    /// Build a freshly-charged battery of a given capacity. Mass ratio
    /// roughly matches modern lithium-ion (250 Wh/kg ≈ 6 kWd/kg).
    pub fn new_battery(capacity_kwd: f64) -> Self {
        let mass_kg = (capacity_kwd / 6.0).max(0.1);
        // Material cost ~$50K per kWd (rough estimate, lithium pack scale).
        let material_cost = capacity_kwd * 50_000.0;
        PowerSource {
            kind: PowerSourceKind::Battery,
            mass_kg,
            material_cost,
            stored_kwd: capacity_kwd,
            capacity_kwd,
        }
    }

    /// Resize an existing solar panel in place: update the peak watts
    /// and recompute mass + material cost from the same sub-linear
    /// curve as `new_solar_panel`. No-op on non-solar sources.
    pub fn resize_solar_panel(&mut self, new_peak_w_at_1au: f64) {
        if let PowerSourceKind::SolarPanel { peak_w_at_1au } = &mut self.kind {
            *peak_w_at_1au = new_peak_w_at_1au.max(1.0);
            let resized = PowerSource::new_solar_panel(*peak_w_at_1au);
            self.mass_kg = resized.mass_kg;
            self.material_cost = resized.material_cost;
        }
    }

    /// Build a solar panel rated at the given peak watts at 1 AU. Mass and
    /// material cost scale sub-linearly with size (mild economies of
    /// scale): mass ∝ peak^0.9, cost ∝ peak^0.85. No upper bound.
    pub fn new_solar_panel(peak_w_at_1au: f64) -> Self {
        let p = peak_w_at_1au.max(1.0);
        // Reference: a 1 kW panel ≈ 5 kg, $100K. State-of-the-art space
        // panels are ~150 W/kg; the 5 kg/kW choice is conservative for
        // legacy/tycoon-game ranges. (Tunable in a balance pass.)
        let ref_w = 1000.0;
        let ref_mass = 5.0;
        let ref_cost = 100_000.0;
        let mass_kg = ref_mass * (p / ref_w).powf(0.9);
        let material_cost = ref_cost * (p / ref_w).powf(0.85);
        PowerSource {
            kind: PowerSourceKind::SolarPanel { peak_w_at_1au },
            mass_kg,
            material_cost,
            stored_kwd: 0.0,
            capacity_kwd: 0.0,
        }
    }

    /// Build a fixed-size space-rated fission reactor at one of the
    /// three legacy preset scales. Phase 1 keeps these so the existing
    /// power editor still works; Phase 2 will retire `new_reactor` in
    /// favour of installing player-designed reactors by id.
    ///
    /// Preset → scale mapping (anchored to the historical Medium
    /// preset = `scale = 1.0`):
    ///   Small  → scale 0.1   (~5 kW)
    ///   Medium → scale 1.0   (~50 kW)
    ///   Large  → scale 10.0  (~500 kW)
    pub fn new_reactor(class: ReactorClass) -> Self {
        use crate::reactor::{EnrichmentLevel, ReactorDesign, ReactorId};
        let (scale, label) = match class {
            ReactorClass::Small => (0.1, "Preset Small Reactor"),
            ReactorClass::Medium => (1.0, "Preset Medium Reactor"),
            ReactorClass::Large => (10.0, "Preset Large Reactor"),
        };
        // ReactorId(0) marks "preset, not from a player project".
        let design = ReactorDesign::new(ReactorId(0), label.into(), scale, EnrichmentLevel::Leu);
        let mass_kg = design.mass_kg;
        let material_cost = design.material_cost;
        PowerSource {
            kind: PowerSourceKind::Reactor { design },
            mass_kg,
            material_cost,
            stored_kwd: 0.0,
            capacity_kwd: 0.0,
        }
    }

    /// Build a power source backed by a player-researched reactor
    /// design. Mass and material cost come from the design itself.
    pub fn from_reactor_design(design: crate::reactor::ReactorDesign) -> Self {
        let mass_kg = design.mass_kg;
        let material_cost = design.material_cost;
        PowerSource {
            kind: PowerSourceKind::Reactor { design },
            mass_kg,
            material_cost,
            stored_kwd: 0.0,
            capacity_kwd: 0.0,
        }
    }

    /// Build a fuel cell rated at `peak_w` watts. Burns the stage's own
    /// propellant at `kg_per_kwd` kg per kilowatt-day of output. Mass
    /// scales linearly at ~30 kg/kW (PEM-class), with material cost
    /// roughly $200K/kW.
    pub fn new_fuel_cell(peak_w: f64) -> Self {
        let p = peak_w.max(1.0);
        let mass_kg = (p / 1000.0) * 30.0;
        let material_cost = (p / 1000.0) * 200_000.0;
        PowerSource {
            kind: PowerSourceKind::FuelCell {
                peak_w: p,
                // ~5 kg of LOX/LH2 per kWd of output (≈Apollo-PEM
                // efficiency, generous for hydrocarbon mixtures).
                kg_per_kwd: 5.0,
            },
            mass_kg,
            material_cost,
            stored_kwd: 0.0,
            capacity_kwd: 0.0,
        }
    }

    /// Build a fixed-size RTG. Three real-world sizes (Cassini-class,
    /// MMRTG-class, small probe).
    pub fn new_rtg(class: RtgClass) -> Self {
        let (steady_w, mass_kg, material_cost) = match class {
            RtgClass::Small => (40.0, 12.0, 10_000_000.0),
            RtgClass::Mmrtg => (120.0, 45.0, 25_000_000.0),
            RtgClass::Cassini => (290.0, 56.0, 80_000_000.0),
        };
        PowerSource {
            kind: PowerSourceKind::Rtg { steady_w },
            mass_kg,
            material_cost,
            stored_kwd: 0.0,
            capacity_kwd: 0.0,
        }
    }
}

/// Discrete RTG sizes available to the player. (RTGs aren't continuously
/// scalable in the same way panels are — they're real engineered units.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RtgClass {
    /// ~40 W, small probe class.
    Small,
    /// ~120 W, MMRTG-class (Curiosity / Perseverance).
    Mmrtg,
    /// ~290 W, Cassini / Galileo class.
    Cassini,
}

impl RtgClass {
    pub fn display_name(self) -> &'static str {
        match self {
            RtgClass::Small => "Small RTG (40 W)",
            RtgClass::Mmrtg => "MMRTG (120 W)",
            RtgClass::Cassini => "Cassini-class RTG (290 W)",
        }
    }
}

/// Space-rated fission reactor sizes. Larger units run hotter and
/// achieve better mass-per-kW than smaller ones (better radiator
/// efficiency at higher temperature).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReactorClass {
    /// ~5 kW, Topaz-II class.
    Small,
    /// ~50 kW, SAFE-400 class.
    Medium,
    /// ~500 kW, SP-100 follow-on class.
    Large,
}

/// Preset power sources surfaced in the rocket designer's power editor.
/// Each preset has a label, a constructor for the underlying
/// `PowerSource`, and an optional technology gate — presets whose
/// `tech_required` isn't unlocked are filtered out of the editor.
///
/// `auto_size_solar` is a marker the editor uses to know "ignore the
/// build closure and instead size the solar panel to the current stage's
/// demand." Set on the lone solar-panel preset.
pub struct PowerPreset {
    pub label: &'static str,
    pub build: fn() -> PowerSource,
    pub tech_required: Option<crate::technology::TechnologyId>,
    pub auto_size_solar: bool,
}

pub fn power_presets() -> &'static [PowerPreset] {
    &[
        PowerPreset { label: "Small Battery (0.5 kWd)",
            build: || PowerSource::new_battery(0.5),
            tech_required: None, auto_size_solar: false },
        PowerPreset { label: "Medium Battery (2 kWd)",
            build: || PowerSource::new_battery(2.0),
            tech_required: None, auto_size_solar: false },
        PowerPreset { label: "Large Battery (10 kWd)",
            build: || PowerSource::new_battery(10.0),
            tech_required: None, auto_size_solar: false },
        // Solar panels are sized by the editor to cover the current
        // stage's demand at 1 AU; the player tunes them up/down with
        // +/- once installed. The `build` here is a sentinel that
        // returns a placeholder; the editor swaps it for a properly
        // sized panel at add time.
        PowerPreset { label: "Solar Panel (auto-sized to stage demand)",
            build: || PowerSource::new_solar_panel(1.0),
            tech_required: None, auto_size_solar: true },
        PowerPreset { label: "Small RTG (40 W)",
            build: || PowerSource::new_rtg(RtgClass::Small),
            tech_required: None, auto_size_solar: false },
        PowerPreset { label: "MMRTG (120 W)",
            build: || PowerSource::new_rtg(RtgClass::Mmrtg),
            tech_required: None, auto_size_solar: false },
        PowerPreset { label: "Cassini-class RTG (290 W)",
            build: || PowerSource::new_rtg(RtgClass::Cassini),
            tech_required: None, auto_size_solar: false },
        PowerPreset { label: "Small Fuel Cell (1 kW)",
            build: || PowerSource::new_fuel_cell(1_000.0),
            tech_required: None, auto_size_solar: false },
        PowerPreset { label: "Medium Fuel Cell (5 kW)",
            build: || PowerSource::new_fuel_cell(5_000.0),
            tech_required: None, auto_size_solar: false },
        PowerPreset { label: "Large Fuel Cell (20 kW)",
            build: || PowerSource::new_fuel_cell(20_000.0),
            tech_required: None, auto_size_solar: false },
        PowerPreset { label: "Small Reactor (5 kW)",
            build: || PowerSource::new_reactor(ReactorClass::Small),
            tech_required: Some(crate::technology::TECH_FISSION_REACTOR), auto_size_solar: false },
        PowerPreset { label: "Medium Reactor (50 kW)",
            build: || PowerSource::new_reactor(ReactorClass::Medium),
            tech_required: Some(crate::technology::TECH_FISSION_REACTOR), auto_size_solar: false },
        PowerPreset { label: "Large Reactor (500 kW)",
            build: || PowerSource::new_reactor(ReactorClass::Large),
            tech_required: Some(crate::technology::TECH_FISSION_REACTOR), auto_size_solar: false },
    ]
}

/// True iff this preset's tech requirement (if any) is unlocked in
/// `technologies`. Presets with no gate always pass.
pub fn preset_available(
    preset: &PowerPreset,
    technologies: &[crate::technology::Technology],
) -> bool {
    match preset.tech_required {
        None => true,
        Some(req) => technologies.iter().any(|t| t.id == req && t.unlocked),
    }
}

/// Synthesize a SolarPanel sized to cover this stage's full electrical
/// demand at 1 AU (housekeeping + engine.power_draw_w × engine_count).
/// Used at design time when a new stage is created and as the
/// "Solar Panel (auto-sized)" preset in the editor.
///
/// The returned `PowerSource` is just a regular `SolarPanel` — there's
/// nothing special tagging it as "auto" once it's in the stage's
/// `power_sources` vec. The player adjusts its size from there via the
/// editor's +/- controls.
pub fn solar_panel_for_stage_demand(stage: &crate::stage::Stage) -> PowerSource {
    let demand = stage.housekeeping_w()
        + stage.engine.power_draw_w * stage.engine_count as f64;
    PowerSource::new_solar_panel(demand.max(1.0))
}

/// True if the engine's propellant mix is something a fuel cell can
/// burn (any liquid hydrocarbon or hydrolox). Solid and ion stages can
/// have a fuel cell installed but it won't produce anything.
pub fn fuel_cell_can_run_on(engine: &crate::engine::EngineDesign) -> bool {
    use crate::propellant::Propellant;
    !engine.propellant_mix.iter().any(|f| matches!(
        f.propellant,
        Propellant::SolidMix | Propellant::Xenon,
    ))
}

/// Short summary label for the equipped-list display.
pub fn source_summary(src: &PowerSource) -> String {
    match &src.kind {
        PowerSourceKind::Battery => format!(
            "Battery ({:.2} kWd, {:.1} kg)", src.capacity_kwd, src.mass_kg),
        PowerSourceKind::SolarPanel { peak_w_at_1au } => format!(
            "Solar Panel ({:.0} W, {:.1} kg)", peak_w_at_1au, src.mass_kg),
        PowerSourceKind::Rtg { steady_w } => format!(
            "RTG ({:.0} W, {:.1} kg)", steady_w, src.mass_kg),
        PowerSourceKind::FuelCell { peak_w, .. } => format!(
            "Fuel Cell ({:.0} W, {:.1} kg)", peak_w, src.mass_kg),
        PowerSourceKind::Reactor { design } => format!(
            "Reactor ({:.0} W, {:.0} K, {:.1} kg)",
            design.steady_w, design.temperature_k, src.mass_kg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solar_panel_falls_off_with_distance() {
        let p = PowerSource::new_solar_panel(1000.0);
        let earth = p.steady_output_w(1.0);
        let mars = p.steady_output_w(1.52);
        let jupiter = p.steady_output_w(5.2);
        assert!((earth - 1000.0).abs() < 1.0);
        // Mars: 1000 / 1.52² ≈ 433 W
        assert!((mars - 1000.0 / (1.52 * 1.52)).abs() < 1.0);
        // Jupiter is much weaker — about 37 W from a 1 kW panel.
        assert!(jupiter < 50.0);
        assert!(earth > mars && mars > jupiter);
    }

    #[test]
    fn rtg_steady_output_does_not_depend_on_distance() {
        let r = PowerSource::new_rtg(RtgClass::Mmrtg);
        assert!((r.steady_output_w(1.0) - 120.0).abs() < 1.0);
        assert!((r.steady_output_w(10.0) - 120.0).abs() < 1.0);
    }

    #[test]
    fn battery_produces_no_steady_power_but_has_capacity() {
        let b = PowerSource::new_battery(2.0); // 2 kWd
        assert_eq!(b.steady_output_w(1.0), 0.0);
        assert!((b.capacity_kwd - 2.0).abs() < 1e-9);
        assert!((b.stored_kwd - 2.0).abs() < 1e-9);
    }

    #[test]
    fn solar_panel_mass_and_cost_scale_sublinearly() {
        let small = PowerSource::new_solar_panel(1000.0);
        let big = PowerSource::new_solar_panel(10_000.0);
        // Mass ratio should be < 10× for 10× the power (sub-linear).
        let mass_ratio = big.mass_kg / small.mass_kg;
        assert!(mass_ratio < 10.0, "expected sub-linear, got {}", mass_ratio);
        // But still bigger.
        assert!(mass_ratio > 1.0);
    }

    #[test]
    fn reactor_output_is_constant_with_sun_distance() {
        let r = PowerSource::new_reactor(ReactorClass::Medium);
        assert!((r.steady_output_w(1.0) - 50_000.0).abs() < 1.0);
        assert!((r.steady_output_w(10.0) - 50_000.0).abs() < 1.0);
        // Total mass includes both reactor and radiator.
        if let PowerSourceKind::Reactor { design } = &r.kind {
            assert!(r.mass_kg > design.radiator.mass_kg,
                "total mass should include both reactor + radiator");
        } else {
            panic!("expected Reactor kind");
        }
    }

    #[test]
    fn larger_reactors_have_better_mass_per_kw() {
        // Higher-temp reactors radiate heat more efficiently → less
        // radiator mass per kW. Encoded in the bundled radiator mass.
        let small = PowerSource::new_reactor(ReactorClass::Small);
        let large = PowerSource::new_reactor(ReactorClass::Large);
        let small_kg_per_kw = small.mass_kg / 5.0;       // 5 kW class
        let large_kg_per_kw = large.mass_kg / 500.0;     // 500 kW class
        assert!(large_kg_per_kw < small_kg_per_kw,
            "expected larger reactor to have lower kg/kW: small={} large={}",
            small_kg_per_kw, large_kg_per_kw);
    }

    #[test]
    fn reactor_preset_locked_until_tech_unlocked() {
        use crate::technology::{Technology, TECH_FISSION_REACTOR};
        // Construct a technology list with the reactor tech LOCKED.
        let locked = vec![Technology {
            id: TECH_FISSION_REACTOR, name: "Fission Reactor".into(),
            description: "".into(),
            unlocked: false, difficulty: 2, deficiencies: vec![],
        }];
        let presets = power_presets();
        let reactor_preset = presets.iter()
            .find(|p| p.tech_required == Some(TECH_FISSION_REACTOR))
            .expect("at least one reactor preset");
        assert!(!preset_available(reactor_preset, &locked),
            "reactor should be unavailable when tech is locked");

        // Now unlock it.
        let unlocked = vec![Technology {
            id: TECH_FISSION_REACTOR, name: "Fission Reactor".into(),
            description: "".into(),
            unlocked: true, difficulty: 2, deficiencies: vec![],
        }];
        assert!(preset_available(reactor_preset, &unlocked));

        // Non-tech-gated presets always pass.
        let battery = presets.iter().find(|p| p.tech_required.is_none()).unwrap();
        assert!(preset_available(battery, &locked));
        assert!(preset_available(battery, &[]));
    }

    fn make_stage(power_draw_w: f64, engine_count: u32) -> crate::stage::Stage {
        use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
        use crate::propellant::Propellant;
        use crate::stage::{Stage, StageId};
        let engine = EngineDesign {
            id: EngineId(1), name: "E".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: 1.0, mass_kg: 100.0, isp_s: 300.0,
            exit_pressure_pa: 1.0, needs_atmosphere: false,
            propellant_mix: vec![PropellantFraction {
                propellant: Propellant::LOX, mass_fraction: 1.0,
            }],
            power_draw_w,
        };
        Stage {
            id: StageId(1), name: "S".into(),
            engine, engine_count,
            propellant_mass_kg: 1000.0, structural_mass_kg: 200.0,
            fairing: None,
            power_sources: Vec::new(),
        }
    }

    #[test]
    fn auto_sized_solar_covers_chemical_stage_housekeeping() {
        // No engine power draw → panel covers just housekeeping.
        let stage = make_stage(0.0, 1);
        let panel = solar_panel_for_stage_demand(&stage);
        let expected = stage.housekeeping_w();
        match panel.kind {
            PowerSourceKind::SolarPanel { peak_w_at_1au } => {
                assert!((peak_w_at_1au - expected).abs() < 1e-6,
                    "expected {} W, got {}", expected, peak_w_at_1au);
            }
            _ => panic!("expected SolarPanel"),
        }
    }

    #[test]
    fn auto_sized_solar_covers_ion_stage_with_engine_draw() {
        // Ion-class power draw: panel covers housekeeping + engine.
        let stage = make_stage(150_000.0, 1);
        let panel = solar_panel_for_stage_demand(&stage);
        let expected = stage.housekeeping_w() + 150_000.0;
        match panel.kind {
            PowerSourceKind::SolarPanel { peak_w_at_1au } => {
                assert!((peak_w_at_1au - expected).abs() < 1e-6,
                    "expected {} W, got {}", expected, peak_w_at_1au);
            }
            _ => panic!("expected SolarPanel"),
        }
        // Should produce enough at 1 AU to power the engine.
        assert!(panel.steady_output_w(1.0) >= 150_000.0);
    }

    #[test]
    fn auto_sized_solar_scales_with_engine_count() {
        // 4× engines → 4× engine demand.
        let stage = make_stage(10_000.0, 4);
        let panel = solar_panel_for_stage_demand(&stage);
        let expected = stage.housekeeping_w() + 40_000.0;
        match panel.kind {
            PowerSourceKind::SolarPanel { peak_w_at_1au } => {
                assert!((peak_w_at_1au - expected).abs() < 1e-6);
            }
            _ => panic!("expected SolarPanel"),
        }
    }

    #[test]
    fn resize_solar_panel_updates_mass_and_cost() {
        let mut p = PowerSource::new_solar_panel(1_000.0);
        let mass_1k = p.mass_kg;
        let cost_1k = p.material_cost;
        p.resize_solar_panel(10_000.0);
        // Sub-linear scaling: mass ratio < 10× for 10× the power.
        assert!(p.mass_kg > mass_1k);
        assert!(p.mass_kg < mass_1k * 10.0);
        assert!(p.material_cost > cost_1k);
        match p.kind {
            PowerSourceKind::SolarPanel { peak_w_at_1au } => {
                assert!((peak_w_at_1au - 10_000.0).abs() < 1e-6);
            }
            _ => panic!("resize should preserve variant"),
        }
    }

    #[test]
    fn resize_solar_panel_is_noop_on_other_kinds() {
        let mut b = PowerSource::new_battery(2.0);
        let mass = b.mass_kg;
        b.resize_solar_panel(10_000.0);
        // Battery is unchanged.
        assert!((b.mass_kg - mass).abs() < 1e-9);
        assert!(matches!(b.kind, PowerSourceKind::Battery));
    }

    #[test]
    fn solar_preset_is_marked_auto_size() {
        // Exactly one solar preset, flagged auto_size_solar = true.
        let solar: Vec<&PowerPreset> = power_presets().iter()
            .filter(|p| p.label.contains("Solar"))
            .collect();
        assert_eq!(solar.len(), 1, "expected one Solar preset");
        assert!(solar[0].auto_size_solar);
        // Non-solar presets are not auto-size.
        let non_solar: Vec<&PowerPreset> = power_presets().iter()
            .filter(|p| !p.label.contains("Solar"))
            .collect();
        assert!(non_solar.iter().all(|p| !p.auto_size_solar));
    }

    /// New games include the fission-reactor tech entry (locked by
    /// default). This is the path that lets a player eventually research
    /// reactors — if the entry is missing the unlock event can't fire.
    #[test]
    fn new_game_includes_fission_reactor_tech() {
        use crate::seed::GameSeed;
        let seed = GameSeed::new(42);
        let techs = crate::technology::generate_technologies(&seed);
        let reactor_tech = techs.iter()
            .find(|t| t.id == crate::technology::TECH_FISSION_REACTOR);
        let reactor_tech = reactor_tech.expect("fission reactor tech missing");
        assert!(!reactor_tech.unlocked,
            "reactor tech should start locked");
    }
}
