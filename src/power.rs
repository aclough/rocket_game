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
    /// Nuclear reactor — large constant power. The radiator is rolled in
    /// for now (the UI shows reactor mass alone) but exposed so future
    /// work can let the player size it independently. Higher
    /// `temperature_k` ⇒ smaller radiator for the same heat rejection
    /// (Stefan-Boltzmann ∝ T⁴). Phase 3.
    Reactor {
        steady_w: f64,
        temperature_k: f64,
        radiator: Radiator,
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
    /// Sun. Batteries and fuel cells are not steady-state producers and
    /// return 0 here; battery surge handling lives in the per-day balance
    /// step rather than here.
    pub fn steady_output_w(&self, sun_distance_au: f64) -> f64 {
        match &self.kind {
            PowerSourceKind::Battery => 0.0,
            PowerSourceKind::SolarPanel { peak_w_at_1au } => {
                if sun_distance_au <= 0.0 { return 0.0; }
                peak_w_at_1au / (sun_distance_au * sun_distance_au)
            }
            PowerSourceKind::Rtg { steady_w } => *steady_w,
            PowerSourceKind::FuelCell { .. } => 0.0,
            PowerSourceKind::Reactor { steady_w, .. } => *steady_w,
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

/// Preset power sources surfaced in the rocket designer's power editor.
/// Each preset has a label and a way to construct the underlying
/// `PowerSource`. We keep this list short and human-friendly for the
/// first-cut UI; future work will let the player pick custom sizes.
pub struct PowerPreset {
    pub label: &'static str,
    pub build: fn() -> PowerSource,
}

pub fn power_presets() -> &'static [PowerPreset] {
    &[
        PowerPreset { label: "Small Battery (0.5 kWd)",
            build: || PowerSource::new_battery(0.5) },
        PowerPreset { label: "Medium Battery (2 kWd)",
            build: || PowerSource::new_battery(2.0) },
        PowerPreset { label: "Large Battery (10 kWd)",
            build: || PowerSource::new_battery(10.0) },
        PowerPreset { label: "Small Solar Panel (500 W @ 1 AU)",
            build: || PowerSource::new_solar_panel(500.0) },
        PowerPreset { label: "Medium Solar Panel (2 kW @ 1 AU)",
            build: || PowerSource::new_solar_panel(2_000.0) },
        PowerPreset { label: "Large Solar Panel (10 kW @ 1 AU)",
            build: || PowerSource::new_solar_panel(10_000.0) },
        PowerPreset { label: "Small RTG (40 W)",
            build: || PowerSource::new_rtg(RtgClass::Small) },
        PowerPreset { label: "MMRTG (120 W)",
            build: || PowerSource::new_rtg(RtgClass::Mmrtg) },
        PowerPreset { label: "Cassini-class RTG (290 W)",
            build: || PowerSource::new_rtg(RtgClass::Cassini) },
    ]
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
        PowerSourceKind::Reactor { steady_w, temperature_k, .. } => format!(
            "Reactor ({:.0} W, {} K, {:.1} kg)", steady_w, temperature_k, src.mass_kg),
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
}
