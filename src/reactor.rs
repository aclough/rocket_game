//! Fission reactor design artefact (the "what" — physical parameters of
//! one reactor). The "how it was researched" lives in
//! [`crate::reactor_project::ReactorProject`].
//!
//! Reactors are anchored to a single continuous `scale` knob; `scale = 1.0`
//! corresponds to the historical Medium reactor preset
//! (50 kW, 1400 K, ~5000 kg). Scaling curves:
//!
//! * `steady_w` ∝ scale (linear)
//! * `mass_kg` ∝ scale^0.9 (mild economies of scale)
//! * `material_cost` ∝ scale^0.85
//! * `temperature_k` ∝ scale^0.05 (bigger reactors run slightly hotter,
//!   giving the bundled radiator a small additional efficiency win)
//!
//! Enrichment (LEU/MEU/HEU) multiplies the mass curve only; output power
//! and temperature don't depend on enrichment.

use serde::{Deserialize, Serialize};

use crate::power::{Radiator, RadiatorKind};

/// Unique identifier for a reactor design. Distinct from `EngineId` so a
/// reactor and an engine can share a numeric value without collisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReactorId(pub u64);

/// Fissile enrichment level. Higher enrichment → more compact reactor
/// (much better kg/kW). MEU and HEU are gated behind reputation
/// thresholds; LEU is always available. Phase 2b will surface the
/// picker in the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnrichmentLevel {
    /// Low-enriched uranium (<20% U-235). Civilian-power-reactor grade;
    /// heaviest mass-per-kW.
    Leu,
    /// Medium-enriched uranium (20–90%). Research / naval propulsion.
    Meu,
    /// Highly-enriched uranium (>90%). Kilopower / weapons-grade —
    /// best mass-per-kW.
    Heu,
}

impl EnrichmentLevel {
    pub fn display_name(self) -> &'static str {
        match self {
            EnrichmentLevel::Leu => "LEU",
            EnrichmentLevel::Meu => "MEU",
            EnrichmentLevel::Heu => "HEU",
        }
    }

    /// Multiplier on reactor mass (not radiator) — lower is better.
    pub fn mass_multiplier(self) -> f64 {
        match self {
            EnrichmentLevel::Leu => 1.0,
            EnrichmentLevel::Meu => 0.55,
            EnrichmentLevel::Heu => 0.30,
        }
    }
}

/// Snapshot of a reactor's physical parameters. Lives inside a
/// `PowerSourceKind::Reactor` once installed on a stage; identical to
/// how a `Stage::engine` carries a cloned `EngineDesign`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactorDesign {
    pub id: ReactorId,
    pub name: String,
    pub scale: f64,
    pub enrichment: EnrichmentLevel,
    /// Steady electrical output (watts).
    pub steady_w: f64,
    /// Core temperature (kelvin) — drives the bundled radiator's
    /// efficiency.
    pub temperature_k: f64,
    /// Reactor mass (kg), excluding the bundled radiator.
    pub reactor_mass_kg: f64,
    /// Radiator bundled with this reactor.
    pub radiator: Radiator,
    /// Total physical mass (reactor + radiator).
    pub mass_kg: f64,
    /// Material cost in dollars; labour is added at build time.
    pub material_cost: f64,
}

// ── Balance anchors ──────────────────────────────────────────────────
//
// Anchored to the previous "Medium" reactor preset so the scale = 1.0
// reactor is identical to what `PowerSource::new_reactor(Medium)` used
// to produce.

/// Reference output power at scale = 1.0 (watts).
pub const REF_STEADY_W: f64 = 50_000.0;
/// Reference core temperature at scale = 1.0 (kelvin).
pub const REF_TEMPERATURE_K: f64 = 1400.0;
/// Reference reactor mass at scale = 1.0 (kg), excluding radiator.
pub const REF_REACTOR_MASS_KG: f64 = 4_200.0;
/// Reference bundled radiator mass at scale = 1.0 (kg).
pub const REF_RADIATOR_MASS_KG: f64 = 800.0;
/// Reference material cost at scale = 1.0 (dollars).
pub const REF_MATERIAL_COST: f64 = 30_000_000.0;

/// Smallest physically meaningful scale. Reactors smaller than this
/// don't form a critical mass.
pub const MIN_SCALE: f64 = 0.05;
/// Reasonable upper bound on scale for UI clamping. Not a hard limit
/// on the data type — the editor uses this for the +/- step ceiling.
pub const MAX_SCALE: f64 = 20.0;
/// Editor step size for `scale`.
pub const SCALE_STEP: f64 = 0.05;
/// Default scale when the editor opens.
pub const DEFAULT_SCALE: f64 = 1.0;

impl ReactorDesign {
    /// Build a reactor design from a scale + enrichment knob. Re-derives
    /// every physical field from the scaling curves above; callers that
    /// later mutate the design should go through `apply_edit` so the
    /// derived fields stay consistent.
    pub fn new(
        id: ReactorId,
        name: String,
        scale: f64,
        enrichment: EnrichmentLevel,
    ) -> Self {
        let s = scale.max(MIN_SCALE);
        let steady_w = REF_STEADY_W * s;
        let temperature_k = REF_TEMPERATURE_K * s.powf(0.05);
        let reactor_mass_kg = REF_REACTOR_MASS_KG * s.powf(0.9) * enrichment.mass_multiplier();
        let radiator_mass_kg = REF_RADIATOR_MASS_KG * s.powf(0.9);
        let material_cost = REF_MATERIAL_COST * s.powf(0.85);
        let radiator = Radiator {
            kind: RadiatorKind::Standard,
            mass_kg: radiator_mass_kg,
        };
        ReactorDesign {
            id,
            name,
            scale: s,
            enrichment,
            steady_w,
            temperature_k,
            reactor_mass_kg,
            radiator,
            mass_kg: reactor_mass_kg + radiator_mass_kg,
            material_cost,
        }
    }

    /// Re-derive every physical field from a fresh (scale, enrichment)
    /// pair while preserving the id and name. Used by the editor when
    /// the player tweaks the design without committing a new project.
    pub fn apply_edit(&mut self, name: String, scale: f64, enrichment: EnrichmentLevel) {
        let updated = ReactorDesign::new(self.id, name, scale, enrichment);
        *self = updated;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_scale_matches_legacy_medium_reactor() {
        // Anchor: scale=1.0 LEU is the old Medium preset.
        let r = ReactorDesign::new(ReactorId(1), "M".into(), 1.0, EnrichmentLevel::Leu);
        assert!((r.steady_w - 50_000.0).abs() < 1.0);
        assert!((r.temperature_k - 1400.0).abs() < 1.0);
        // Total mass should be 4200 + 800 = 5000 kg.
        assert!((r.mass_kg - 5_000.0).abs() < 1.0);
        assert!((r.material_cost - 30_000_000.0).abs() < 1.0);
    }

    #[test]
    fn steady_power_is_linear_in_scale() {
        let one = ReactorDesign::new(ReactorId(1), "a".into(), 1.0, EnrichmentLevel::Leu);
        let ten = ReactorDesign::new(ReactorId(2), "b".into(), 10.0, EnrichmentLevel::Leu);
        assert!((ten.steady_w / one.steady_w - 10.0).abs() < 1e-9);
    }

    #[test]
    fn mass_is_sublinear_in_scale() {
        let one = ReactorDesign::new(ReactorId(1), "a".into(), 1.0, EnrichmentLevel::Leu);
        let ten = ReactorDesign::new(ReactorId(2), "b".into(), 10.0, EnrichmentLevel::Leu);
        let ratio = ten.mass_kg / one.mass_kg;
        assert!(ratio > 1.0 && ratio < 10.0, "expected sub-linear mass scaling, got {}", ratio);
    }

    #[test]
    fn temperature_rises_mildly_with_scale() {
        let one = ReactorDesign::new(ReactorId(1), "a".into(), 1.0, EnrichmentLevel::Leu);
        let ten = ReactorDesign::new(ReactorId(2), "b".into(), 10.0, EnrichmentLevel::Leu);
        assert!(ten.temperature_k > one.temperature_k);
        // Should be << 2× — temperature exponent is 0.05, so 10× scale
        // gives only ~12% temperature lift.
        assert!(ten.temperature_k / one.temperature_k < 1.2);
    }

    #[test]
    fn enrichment_reduces_reactor_mass_only() {
        let leu = ReactorDesign::new(ReactorId(1), "a".into(), 1.0, EnrichmentLevel::Leu);
        let meu = ReactorDesign::new(ReactorId(2), "b".into(), 1.0, EnrichmentLevel::Meu);
        let heu = ReactorDesign::new(ReactorId(3), "c".into(), 1.0, EnrichmentLevel::Heu);
        // Reactor mass drops; radiator mass unchanged.
        assert!(meu.reactor_mass_kg < leu.reactor_mass_kg);
        assert!(heu.reactor_mass_kg < meu.reactor_mass_kg);
        assert!((meu.radiator.mass_kg - leu.radiator.mass_kg).abs() < 1e-9);
        assert!((heu.radiator.mass_kg - leu.radiator.mass_kg).abs() < 1e-9);
        // Steady power and temperature unaffected by enrichment.
        assert!((meu.steady_w - leu.steady_w).abs() < 1e-9);
        assert!((heu.temperature_k - leu.temperature_k).abs() < 1e-9);
    }

    #[test]
    fn scale_is_clamped_to_minimum() {
        let tiny = ReactorDesign::new(ReactorId(1), "a".into(), 0.001, EnrichmentLevel::Leu);
        assert!((tiny.scale - MIN_SCALE).abs() < 1e-9);
    }

    #[test]
    fn apply_edit_re_derives_all_fields() {
        let mut r = ReactorDesign::new(ReactorId(7), "orig".into(), 1.0, EnrichmentLevel::Leu);
        let before_id = r.id;
        r.apply_edit("renamed".into(), 4.0, EnrichmentLevel::Heu);
        assert_eq!(r.id, before_id); // id preserved
        assert_eq!(r.name, "renamed");
        assert_eq!(r.enrichment, EnrichmentLevel::Heu);
        assert!((r.steady_w - 4.0 * REF_STEADY_W).abs() < 1.0);
    }
}
