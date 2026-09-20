use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::balance;
use crate::engine::{Propulsion, EngineDesign, EngineCycle, EngineId, PropellantFraction, G0};
use crate::balance_config::NozzleConfig;
use crate::balance_config::{BalanceConfig, FlawsConfig};
use crate::flaw::FlawDomain;
use crate::project::{DesignProject, DesignStatus, Designable, Direction, Improvement, ImprovementId, ProjectKind, ProjectRef};
use crate::propellant::Propellant;
use crate::technology::TechDeficiencyKind;
use crate::third_party::ContractedEngineId;

pub use crate::project::WorkEvent;

/// A preset propellant combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropellantPreset {
    Kerolox,
    Hydrolox,
    Methalox,
    Hypergolic,
    Solid,
    /// Pure hydrogen heated by nuclear reactor (no oxidizer).
    Hydrogen,
    /// Xenon for electric propulsion (ion/Hall thrusters).
    Xenon,
    /// Photon pressure — no propellant, used with solar sails.
    Photon,
}

impl PropellantPreset {
    pub const ALL: &[PropellantPreset] = &[
        PropellantPreset::Kerolox,
        PropellantPreset::Hydrolox,
        PropellantPreset::Methalox,
        PropellantPreset::Hypergolic,
        PropellantPreset::Solid,
        PropellantPreset::Hydrogen,
        PropellantPreset::Xenon,
        PropellantPreset::Photon,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            PropellantPreset::Kerolox => "Kerolox",
            PropellantPreset::Hydrolox => "Hydrolox",
            PropellantPreset::Methalox => "Methalox",
            PropellantPreset::Hypergolic => "Hypergolic",
            PropellantPreset::Solid => "Solid",
            PropellantPreset::Hydrogen => "Hydrogen",
            PropellantPreset::Xenon => "Xenon",
            PropellantPreset::Photon => "Photon",
        }
    }

    /// The propellant mix for this preset.
    pub fn propellant_mix(&self) -> Vec<PropellantFraction> {
        match self {
            PropellantPreset::Kerolox => vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.73 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.27 },
            ],
            PropellantPreset::Hydrolox => vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.83 },
                PropellantFraction { propellant: Propellant::LH2, mass_fraction: 0.17 },
            ],
            PropellantPreset::Methalox => vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.78 },
                PropellantFraction { propellant: Propellant::Methane, mass_fraction: 0.22 },
            ],
            PropellantPreset::Hypergolic => vec![
                PropellantFraction { propellant: Propellant::NTO, mass_fraction: 0.57 },
                PropellantFraction { propellant: Propellant::UDMH, mass_fraction: 0.43 },
            ],
            PropellantPreset::Solid => vec![
                PropellantFraction { propellant: Propellant::SolidMix, mass_fraction: 1.0 },
            ],
            PropellantPreset::Hydrogen => vec![
                PropellantFraction { propellant: Propellant::LH2, mass_fraction: 1.0 },
            ],
            PropellantPreset::Xenon => vec![
                PropellantFraction { propellant: Propellant::Xenon, mass_fraction: 1.0 },
            ],
            PropellantPreset::Photon => vec![],
        }
    }

    /// The propellants in this preset (for complexity calculations).
    pub fn propellants(&self) -> Vec<Propellant> {
        self.propellant_mix().iter().map(|f| f.propellant).collect()
    }

    /// Which cycles are compatible with this propellant preset.
    pub fn compatible_cycles(&self) -> &[EngineCycle] {
        match self {
            PropellantPreset::Solid => &[EngineCycle::PressureFed],
            PropellantPreset::Hydrogen => &[EngineCycle::NuclearThermal],
            PropellantPreset::Xenon => &[EngineCycle::ElectricPropulsion],
            PropellantPreset::Photon => &[EngineCycle::SolarSail],
            _ => &[
                EngineCycle::PressureFed,
                EngineCycle::GasGenerator,
                EngineCycle::Expander,
                EngineCycle::StagedCombustion,
                EngineCycle::FullFlow,
            ],
        }
    }
}

/// Baseline engine parameters for a (cycle, propellant) combination at scale 1.0.
///
/// Realistic-ish figures inspired by real engines. Thrust and mass scale
/// linearly with the scale factor; Isp does not. The nozzle is not on
/// the baseline: `design` derives the sea-level bell from the chamber
/// pressure (19_NOZZLES.md §2) and `EngineDesign::with_vacuum_bell`
/// the long one from the throat that thrust and chamber pressure imply.
#[derive(Debug, Clone, Copy)]
pub struct EngineBaseline {
    /// Vacuum thrust of the sea-level bell at scale 1.0 (N).
    pub thrust_n: f64,
    /// Mass at scale 1.0 with the sea-level bell (kg).
    pub mass_kg: f64,
    /// Vacuum Isp of the *sea-level* bell (s): the recognisable figure
    /// (kerolox ~311, hydrolox ~420). The pad figure and the vacuum
    /// bell's are derived from it. 0 for a sail.
    pub isp_ref_s: f64,
    /// Chamber pressure (Pa); 0 for engines without a nozzle.
    pub chamber_pressure_pa: f64,
    /// Exhaust heat-capacity ratio; 0 for engines without a nozzle.
    pub gamma: f64,
    /// If true, this engine only exists in vacuum configuration.
    pub vacuum_only: bool,
    /// Electrical power draw at full thrust (watts). 0 for everything
    /// except `ElectricPropulsion`.
    pub power_draw_w: f64,
}

impl EngineBaseline {
    /// The engine this baseline yields at `scale`, with the sea-level
    /// bell (expanded to `cfg.sea_level_exit_pressure_pa`) or, when
    /// `vacuum` or the baseline is vacuum-only, the long bell.
    #[allow(clippy::too_many_arguments)]
    pub fn design(
        &self, id: EngineId, name: String, cycle: EngineCycle, preset: PropellantPreset,
        scale: f64, vacuum: bool, cfg: &NozzleConfig,
    ) -> EngineDesign {
        let propulsion = match cycle {
            EngineCycle::ElectricPropulsion => Propulsion::Electric {
                // Power draw: scales with thrust for ion drives (~30 kW/N
                // ≈ NEXT thruster ratio).
                power_draw_w: self.power_draw_w * scale,
            },
            EngineCycle::SolarSail => Propulsion::Sail,
            _ => Propulsion::nozzle_for_exit_pressure(
                self.chamber_pressure_pa, cfg.sea_level_exit_pressure_pa, self.gamma, true,
            ),
        };
        let design = EngineDesign {
            id,
            name,
            cycle,
            thrust_n: self.thrust_n * scale,
            mass_kg: self.mass_kg * scale,
            isp_s: self.isp_ref_s,
            propellant_mix: preset.propellant_mix(),
            propulsion,
        };
        if self.vacuum_only || vacuum {
            design.with_vacuum_bell(cfg)
        } else {
            design
        }
    }
}

/// Chamber pressure (Pa) of a (cycle, propellant) family, from the real
/// engines of each kind (19_NOZZLES.md §5): Kestrel/AJ10 pressure-fed
/// at 9–10 bar, Merlin/F-1 gas generators at 70–97, RL10/Vinci
/// expanders at 44–60, RD-180/RS-25/BE-4 staged combustion at 134–257,
/// Raptor full-flow at 300. Kerosene and hypergolics are poor expander
/// coolants; those rows stay legal at low pressure. `None` for the
/// nozzle-less kinds and for combinations that don't exist.
pub fn chamber_pressure_pa(cycle: EngineCycle, preset: PropellantPreset) -> Option<f64> {
    use EngineCycle as C;
    use PropellantPreset as P;
    let bar = match (cycle, preset) {
        (C::PressureFed, P::Kerolox | P::Hydrolox | P::Methalox) => 15.0,
        (C::PressureFed, P::Hypergolic) => 10.0,
        (C::PressureFed, P::Solid) => 60.0,
        (C::GasGenerator, P::Kerolox | P::Methalox) => 90.0,
        (C::GasGenerator, P::Hydrolox) => 100.0,
        (C::GasGenerator, P::Hypergolic) => 60.0,
        (C::Expander, P::Kerolox | P::Hypergolic) => 40.0,
        (C::Expander, P::Hydrolox | P::Methalox) => 45.0,
        (C::StagedCombustion, P::Kerolox) => 250.0,
        (C::StagedCombustion, P::Hydrolox) => 200.0,
        (C::StagedCombustion, P::Methalox) => 135.0,
        (C::StagedCombustion, P::Hypergolic) => 150.0,
        (C::FullFlow, P::Kerolox | P::Hydrolox) => 250.0,
        (C::FullFlow, P::Methalox) => 300.0,
        (C::FullFlow, P::Hypergolic) => 200.0,
        // NERVA-class: ~30 bar.
        (C::NuclearThermal, P::Hydrogen) => 30.0,
        _ => return None,
    };
    Some(bar * 100_000.0)
}

/// Heat-capacity ratio of a propellant's exhaust. Hot hydrogen (NTR)
/// is closest to a diatomic gas; the chemical exhausts sit at 1.18–1.23.
pub fn exhaust_gamma(preset: PropellantPreset) -> f64 {
    match preset {
        PropellantPreset::Kerolox => 1.22,
        PropellantPreset::Hydrolox => 1.20,
        PropellantPreset::Methalox => 1.21,
        PropellantPreset::Hypergolic => 1.23,
        PropellantPreset::Solid => 1.18,
        PropellantPreset::Hydrogen => 1.40,
        PropellantPreset::Xenon | PropellantPreset::Photon => 0.0,
    }
}

/// The chamber pressure to assume for a design that predates nozzles
/// (`EngineDesignRepr`): its family's if the propellant mix names one,
/// else the cycle's kerolox figure, else nothing (no nozzle).
pub fn chamber_pressure_for_legacy(cycle: EngineCycle, mix: &[crate::engine::PropellantFraction]) -> Option<(f64, f64)> {
    match preset_for_mix(mix) {
        Some(p) => chamber_pressure_pa(cycle, p).map(|pc| (pc, exhaust_gamma(p))),
        None => chamber_pressure_pa(cycle, PropellantPreset::Kerolox)
            .map(|pc| (pc, crate::engine::DEFAULT_GAMMA)),
    }
}

/// The preset whose propellant mix this design carries, if any — the
/// way back from a built engine to the family it came from.
pub fn preset_for_mix(mix: &[crate::engine::PropellantFraction]) -> Option<PropellantPreset> {
    [
        PropellantPreset::Kerolox, PropellantPreset::Hydrolox, PropellantPreset::Methalox,
        PropellantPreset::Hypergolic, PropellantPreset::Solid, PropellantPreset::Hydrogen,
        PropellantPreset::Xenon, PropellantPreset::Photon,
    ]
    .into_iter()
    .find(|p| p.propellant_mix() == mix)
}

/// Whether a built engine's family offered a sea-level bell at all: false
/// for expander, electric, solar-sail and nuclear-thermal families, whose
/// `is_vacuum_variant` says nothing about what anyone chose.
pub fn design_has_nozzle_choice(design: &EngineDesign) -> bool {
    preset_for_mix(&design.propellant_mix)
        .and_then(|preset| engine_baseline(design.cycle, preset))
        .is_some_and(|b| !b.vacuum_only)
}

/// Get the baseline engine parameters for a (cycle, propellant) combination.
///
/// Returns `None` for invalid combinations (e.g., Solid + GasGenerator,
/// or Xenon with anything other than ElectricPropulsion).
///
/// These are the "middle of the range" values at scale 1.0.
/// Inspired by real engines but simplified for gameplay.
pub fn engine_baseline(cycle: EngineCycle, preset: PropellantPreset) -> Option<EngineBaseline> {
    // Electric propulsion: completely different from chemical engines
    if cycle == EngineCycle::ElectricPropulsion {
        if preset != PropellantPreset::Xenon {
            return None;
        }
        return Some(EngineBaseline {
            thrust_n: 1.0,               // 1 Newton — very low thrust
            // Mass cut from 50 kg to 35 kg now that the engine no longer
            // implicitly carries its own power supply (panels are
            // provisioned separately on the stage).
            mass_kg: 35.0,
            isp_ref_s: 3000.0,           // very high Isp
            chamber_pressure_pa: 0.0,    // no nozzle the air can act on
            gamma: 0.0,
            vacuum_only: true,
            // ~30 kW per Newton of thrust — NEXT-thruster scale.
            power_draw_w: 30_000.0,
        });
    }

    // Solar sail: thrust from solar radiation pressure, no propellant
    if cycle == EngineCycle::SolarSail {
        if preset != PropellantPreset::Photon {
            return None;
        }
        return Some(EngineBaseline {
            thrust_n: 0.01,              // 10 millinewtons at 1 AU, scale 1.0
            mass_kg: 100.0,              // sail + structure
            isp_ref_s: 0.0,              // not meaningful for sails
            chamber_pressure_pa: 0.0,
            gamma: 0.0,
            vacuum_only: true,
            // Solar sails get thrust from photons, not electricity. A
            // future "magnetic sail" variant might draw power.
            power_draw_w: 0.0,
        });
    }

    // Nuclear thermal: completely different from chemical engines
    if cycle == EngineCycle::NuclearThermal {
        if preset != PropellantPreset::Hydrogen {
            return None;
        }
        return Some(EngineBaseline {
            thrust_n: 330_000.0,          // ~NERVA class (~73 klbf)
            mass_kg: 10_000.0,            // very heavy (reactor + shielding)
            // Sea-level-bell reference; the vacuum bell it always flies
            // lands near NERVA's ~850 s.
            isp_ref_s: 785.0,
            chamber_pressure_pa: chamber_pressure_pa(cycle, preset)?,
            gamma: exhaust_gamma(preset),
            vacuum_only: true,
            power_draw_w: 0.0,
        });
    }

    // Solid engines can only be pressure-fed
    if preset == PropellantPreset::Solid && cycle != EngineCycle::PressureFed {
        return None;
    }
    // Nuclear H2 only works with NuclearThermal cycle
    if preset == PropellantPreset::Hydrogen {
        return None;
    }
    // Xenon only works with ElectricPropulsion cycle
    if preset == PropellantPreset::Xenon {
        return None;
    }
    // Photon only works with SolarSail cycle
    if preset == PropellantPreset::Photon {
        return None;
    }

    // Vacuum Isp of the sea-level bell by propellant (gas-generator
    // reference), then the cycle adjusts. Merlin 1D 311, J-2 / Vulcain
    // class ~420, Raptor 350, AJ10-class hypergolic ~285, Castor-class
    // solid ~285.
    let base_isp_ref = match preset {
        PropellantPreset::Kerolox => 311.0,
        PropellantPreset::Hydrolox => 420.0,
        PropellantPreset::Methalox => 350.0,
        PropellantPreset::Hypergolic => 285.0,
        PropellantPreset::Solid => 285.0,
        PropellantPreset::Hydrogen => unreachable!(),
        PropellantPreset::Xenon => unreachable!(),
        PropellantPreset::Photon => unreachable!(),
    };

    // Cycle multipliers for the sea-level-bell reference Isp (relative
    // to GasGenerator). The expander's low chamber pressure gives it a
    // poor short bell; its long bell (the only one it flies) then gains
    // the most, landing hydrolox at RL10's ~465 s.
    let isp_mult = match cycle {
        EngineCycle::PressureFed => 0.92,
        EngineCycle::GasGenerator => 1.00,
        EngineCycle::Expander => 0.97,
        EngineCycle::StagedCombustion => 1.06,
        EngineCycle::FullFlow => 1.08,
        EngineCycle::NuclearThermal => unreachable!(),
        EngineCycle::ElectricPropulsion => unreachable!(),
        EngineCycle::SolarSail => unreachable!(),
    };

    // Base thrust at scale 1.0 by propellant type
    let base_thrust = match preset {
        PropellantPreset::Kerolox => 900_000.0,      // ~Merlin-class
        PropellantPreset::Hydrolox => 110_000.0,      // ~RL-10-class
        PropellantPreset::Methalox => 700_000.0,      // ~Raptor-class
        PropellantPreset::Hypergolic => 45_000.0,     // ~AJ10-class
        PropellantPreset::Solid => 500_000.0,         // ~medium SRB
        PropellantPreset::Hydrogen => unreachable!(),
        PropellantPreset::Xenon => unreachable!(),
        PropellantPreset::Photon => unreachable!(),
    };

    // Cycle multipliers for thrust (relative to GasGenerator)
    let thrust_mult = match cycle {
        EngineCycle::PressureFed => 0.60,
        EngineCycle::GasGenerator => 1.00,
        EngineCycle::Expander => 0.80,
        EngineCycle::StagedCombustion => 1.15,
        EngineCycle::FullFlow => 1.30,
        EngineCycle::NuclearThermal => unreachable!(),
        EngineCycle::ElectricPropulsion => unreachable!(),
        EngineCycle::SolarSail => unreachable!(),
    };

    // Thrust-to-weight ratio by cycle (higher = lighter for given thrust)
    // This gives us mass from thrust
    let twr = match cycle {
        EngineCycle::PressureFed => 40.0,    // simple, light
        EngineCycle::GasGenerator => 80.0,   // good TWR
        EngineCycle::Expander => 50.0,       // moderate
        EngineCycle::StagedCombustion => 70.0, // heavy but powerful
        EngineCycle::FullFlow => 60.0,       // heaviest complex cycle
        EngineCycle::NuclearThermal => unreachable!(),
        EngineCycle::ElectricPropulsion => unreachable!(),
        EngineCycle::SolarSail => unreachable!(),
    };

    let thrust = base_thrust * thrust_mult;
    let mass = thrust / (twr * G0);

    Some(EngineBaseline {
        thrust_n: thrust,
        mass_kg: mass,
        isp_ref_s: base_isp_ref * isp_mult,
        chamber_pressure_pa: chamber_pressure_pa(cycle, preset)?,
        gamma: exhaust_gamma(preset),
        vacuum_only: cycle == EngineCycle::Expander,
        // Chemical engines don't draw electrical power.
        power_draw_w: 0.0,
    })
}

/// Scale range for engine design. Player picks a value in [min_scale, max_scale].
pub const MIN_SCALE: f64 = 0.25;
pub const MAX_SCALE: f64 = 4.0;
pub const DEFAULT_SCALE: f64 = 1.0;
pub const SCALE_STEP: f64 = 0.25;

/// Status of an engine design project — the shared [`DesignStatus`].
pub type EngineDesignStatus = DesignStatus;

/// Unique identifier for an engine project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngineProjectId(pub u64);

/// Where an engine comes from — player-designed or contracted third-party.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EngineSource {
    PlayerDesign(EngineProjectId),
    Contracted(ContractedEngineId),
}

/// The player's choices an engine is derived from, kept beside the
/// design so the editor and `design_variant` can re-derive it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSpec {
    pub preset: PropellantPreset,
    pub scale: f64,
}

/// An engine design project with workflow state.
pub type EngineProject = DesignProject<EngineDesign>;

impl Designable for EngineDesign {
    type Id = EngineProjectId;
    fn project_ref(id: Self::Id) -> ProjectRef {
        ProjectRef::Engine(id)
    }
    type Spec = EngineSpec;
    type ImprovementKind = EngineImprovementKind;
    const KIND: ProjectKind = ProjectKind::Engine;

    fn name(&self) -> &str {
        &self.name
    }

    fn flaw_domain(&self) -> FlawDomain {
        FlawDomain::Engine(Some(self.cycle))
    }

    /// Effective complexity folds the propellants' problem factors into
    /// the cycle's; the project's stored complexity is the plain
    /// combined figure used for work and NRE.
    fn flaw_complexity(&self, spec: &EngineSpec, _project_complexity: u32) -> u32 {
        balance::effective_complexity(self.cycle, &spec.preset.propellants())
    }

    fn improvement_chance(cfg: &FlawsConfig) -> Option<f64> {
        Some(cfg.improvement_discovery_chance)
    }

    fn roll_improvement(&self, rng: &mut StdRng, id: ImprovementId, balance_cfg: &BalanceConfig) -> EngineImprovement {
        generate_improvement(rng, self.cycle, id, &balance_cfg.improvements)
    }

    fn apply_improvement(&mut self, kind: &EngineImprovementKind) {
        match kind {
            EngineImprovementKind::Isp(frac) => self.isp_s *= 1.0 + frac,
            EngineImprovementKind::Mass(frac) => self.mass_kg *= 1.0 - frac,
            EngineImprovementKind::Thrust(frac) => self.thrust_n *= 1.0 + frac,
        }
    }

    fn apply_deficiency(&mut self, kind: &TechDeficiencyKind, dir: Direction) {
        EngineDesign::apply_deficiency(self, kind, dir)
    }
}

impl EngineProject {
    /// Create a new engine project from player choices.
    #[allow(clippy::too_many_arguments)] // constructor-style, callers read positionally with names at the call site
    pub fn new(
        project_id: EngineProjectId,
        engine_id: EngineId,
        name: String,
        cycle: EngineCycle,
        preset: PropellantPreset,
        scale: f64,
        balance_cfg: &BalanceConfig,
    ) -> Option<Self> {
        let baseline = engine_baseline(cycle, preset)?;
        let propellants = preset.propellants();
        let complexity = balance::combined_complexity(cycle, &propellants);
        let effective = balance::effective_complexity(cycle, &propellants);
        let work_required = balance_cfg.work.design_work_required(effective, scale);
        // The project's own `design` holds the canonical (sea-level)
        // form of the family; `design_variant` derives the nozzle a
        // given stage actually flies.
        let design = baseline.design(engine_id, name, cycle, preset, scale, false, &balance_cfg.nozzle);
        Some(DesignProject::new_in_design(
            project_id, design, EngineSpec { preset, scale }, complexity, work_required, None,
        ))
    }

    /// Create a tentative engine project in `Proposed` status. Used by
    /// the rocket designer to spawn a draft engine that can be iterated
    /// on; promoted to InDesign when the parent rocket is finalised.
    #[allow(clippy::too_many_arguments)] // constructor-style, callers read positionally with names at the call site
    pub fn new_proposed(
        project_id: EngineProjectId,
        engine_id: EngineId,
        name: String,
        cycle: EngineCycle,
        preset: PropellantPreset,
        scale: f64,
        balance_cfg: &BalanceConfig,
    ) -> Option<Self> {
        let mut p = Self::new(project_id, engine_id, name, cycle, preset, scale, balance_cfg)?;
        let work_required = match p.status {
            EngineDesignStatus::InDesign { work_required, .. } => work_required,
            _ => unreachable!(),
        };
        p.status = EngineDesignStatus::Proposed { work_required };
        Some(p)
    }

    /// Rebuild the design from a fresh set of player choices. Used by
    /// the engine editor for non-linear editing. Recomputes complexity
    /// and work_required; progress is clamped to the new work_required
    /// so a player can't appear to have over-completed a now-cheaper
    /// design.
    pub fn apply_edit(
        &mut self,
        name: String,
        cycle: EngineCycle,
        preset: PropellantPreset,
        scale: f64,
        balance_cfg: &BalanceConfig,
    ) -> bool {
        let Some(baseline) = engine_baseline(cycle, preset) else {
            return false;
        };
        let propellants = preset.propellants();
        let complexity = balance::combined_complexity(cycle, &propellants);
        let effective = balance::effective_complexity(cycle, &propellants);
        let work_required = balance_cfg.work.design_work_required(effective, scale);

        // Preserve engine id and re-derive everything else.
        self.design = baseline.design(self.design.id, name, cycle, preset, scale, false, &balance_cfg.nozzle);
        self.spec = EngineSpec { preset, scale };
        self.complexity = complexity;
        self.clamp_work_after_edit(work_required);
        true
    }

    /// The engine as flown with the given nozzle.
    ///
    /// An engine project designs a *family*, not a single unit: the
    /// chamber, turbopump, and plumbing are shared, and the nozzle is
    /// fitted per stage. Flaws, NRE, improvements, tech deficiencies,
    /// and the production learning curve all live on the project and
    /// so are shared by both variants — a bell extension is not a new
    /// engine programme.
    ///
    /// Baselines that only exist in vacuum (electric, solar sail, NTR)
    /// ignore `vacuum` and always return the vacuum form.
    pub fn design_variant(&self, vacuum: bool, cfg: &NozzleConfig) -> EngineDesign {
        // The project's design is the sea-level bell (or, for a
        // vacuum-only family, already the long bell); improvements and
        // deficiencies have been applied to it, so both variants carry
        // them.
        if vacuum && !self.design.is_vacuum_variant() {
            self.design.with_vacuum_bell(cfg)
        } else {
            self.design.clone()
        }
    }

    /// Whether this engine offers a nozzle choice at all. False for
    /// vacuum-only baselines, which have nothing to toggle.
    pub fn has_nozzle_choice(&self) -> bool {
        engine_baseline(self.design.cycle, self.spec.preset)
            .is_some_and(|b| !b.vacuum_only)
    }

}

/// A potential engine improvement discovered during testing. The
/// reactor counterpart is `reactor_project::ReactorImprovement`.
pub type EngineImprovement = Improvement<EngineImprovementKind>;

/// What an engine improvement affects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineImprovementKind {
    /// Increase Isp by this fraction (e.g. 0.02 = +2%).
    Isp(f64),
    /// Reduce mass by this fraction (e.g. 0.03 = -3%).
    Mass(f64),
    /// Increase thrust by this fraction (e.g. 0.02 = +2%).
    Thrust(f64),
}

impl std::fmt::Display for EngineImprovementKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineImprovementKind::Isp(frac) => write!(f, "+{:.0}% Isp", frac * 100.0),
            EngineImprovementKind::Mass(frac) => write!(f, "-{:.0}% mass", frac * 100.0),
            EngineImprovementKind::Thrust(frac) => write!(f, "+{:.0}% thrust", frac * 100.0),
        }
    }
}


/// Generate a random improvement appropriate for the engine cycle.
/// Roll one testing-cycle improvement for an engine of `cycle`: which
/// kind, by the family's weights, how big, from that kind's range, and
/// a description that fits the family (`ImprovementsConfig`).
fn generate_improvement(
    rng: &mut StdRng, cycle: EngineCycle, id: ImprovementId,
    cfg: &crate::balance_config::ImprovementsConfig,
) -> EngineImprovement {
    let family = match cycle {
        EngineCycle::SolarSail => &cfg.solar_sail,
        EngineCycle::ElectricPropulsion => &cfg.electric,
        EngineCycle::NuclearThermal => &cfg.nuclear_thermal,
        _ => &cfg.chemical,
    };
    let roll: f64 = rng.gen();
    let kind_roll = if roll < family.isp.weight {
        &family.isp
    } else if roll < family.isp.weight + family.mass.weight {
        &family.mass
    } else {
        &family.thrust
    };
    let frac = rng.gen_range(kind_roll.min..kind_roll.max);
    let pick = rng.gen_range(0u32..3);
    let kind = if std::ptr::eq(kind_roll, &family.isp) {
        EngineImprovementKind::Isp(frac)
    } else if std::ptr::eq(kind_roll, &family.mass) {
        EngineImprovementKind::Mass(frac)
    } else {
        EngineImprovementKind::Thrust(frac)
    };
    let description = improvement_description(cycle, &kind, pick);

    EngineImprovement {
        id,
        description: description.to_string(),
        kind,
        actualized: false,
    }
}

/// The story behind an improvement, by engine family and kind.
fn improvement_description(cycle: EngineCycle, kind: &EngineImprovementKind, pick: u32) -> &'static str {
    use EngineImprovementKind as K;
    match (cycle, kind) {
        (EngineCycle::SolarSail, K::Mass(_)) => match pick {
            0 => "Lighter boom material",
            1 => "Thinner sail substrate",
            _ => "Optimized deployment mechanism",
        },
        (EngineCycle::SolarSail, _) => match pick {
            0 => "Higher reflectivity coating",
            1 => "Improved sail flatness",
            _ => "Better attitude control vane geometry",
        },
        (EngineCycle::ElectricPropulsion, K::Isp(_)) => match pick {
            0 => "Optimized ion grid spacing",
            1 => "Improved beam focusing",
            _ => "Better discharge chamber geometry",
        },
        (EngineCycle::ElectricPropulsion, K::Mass(_)) => match pick {
            0 => "Lighter power processing unit",
            1 => "Reduced thruster head mass",
            _ => "Compact xenon feed system",
        },
        (EngineCycle::ElectricPropulsion, K::Thrust(_)) => match pick {
            0 => "Higher discharge current achievable",
            1 => "Improved ion extraction efficiency",
            _ => "Better magnetic field confinement",
        },
        (EngineCycle::NuclearThermal, K::Isp(_)) => match pick {
            0 => "Higher reactor operating temperature",
            1 => "Improved fuel element heat transfer",
            _ => "Better hydrogen flow distribution",
        },
        (EngineCycle::NuclearThermal, K::Mass(_)) => match pick {
            0 => "Lighter radiation shielding",
            1 => "Compact reactor core design",
            _ => "Reduced turbopump mass",
        },
        (EngineCycle::NuclearThermal, K::Thrust(_)) => match pick {
            0 => "Higher reactor power output",
            1 => "Improved propellant heating efficiency",
            _ => "Better nozzle thermal management",
        },
        (_, K::Isp(_)) => match pick {
            0 => "Optimized injector pattern",
            1 => "Improved propellant mixing efficiency",
            _ => "Better nozzle contour",
        },
        (_, K::Mass(_)) => match pick {
            0 => "Lighter turbopump housing",
            1 => "Thinner chamber wall design",
            _ => "Reduced gimbal mechanism mass",
        },
        (_, K::Thrust(_)) => match pick {
            0 => "Higher chamber pressure achievable",
            1 => "Improved injector throughput",
            _ => "Better regenerative cooling allows hotter burn",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{bal, test_rng};
    use crate::flaw::Flaw;



    fn create_test_project() -> EngineProject {
        EngineProject::new(
            EngineProjectId(1),
            EngineId(1),
            "TestEngine".into(),
            EngineCycle::GasGenerator,
            PropellantPreset::Kerolox,
            1.0,
            &bal(),
        ).unwrap()
    }

    #[test]
    fn test_baseline_exists_for_all_valid_combos() {
        for preset in PropellantPreset::ALL {
            for cycle in preset.compatible_cycles() {
                let b = engine_baseline(*cycle, *preset);
                assert!(b.is_some(), "Missing baseline for {:?}/{:?}", cycle, preset);
                let b = b.unwrap();
                // Solar sails have ~0 thrust and 0 Isp; skip those assertions
                if b.isp_ref_s > 0.0 {
                    assert!(b.thrust_n > 0.0);
                }
                assert!(b.mass_kg > 0.0);
                if *cycle != EngineCycle::SolarSail {
                    assert!(b.isp_ref_s > 0.0);
                }
                // Everything that exhausts through a bell has a chamber pressure and gamma.
                let has_nozzle = !matches!(cycle, EngineCycle::ElectricPropulsion | EngineCycle::SolarSail);
                assert_eq!(b.chamber_pressure_pa > 0.0, has_nozzle, "{cycle:?}/{preset:?}");
                assert_eq!(b.gamma > 1.0, has_nozzle, "{cycle:?}/{preset:?}");
            }
        }
    }

    #[test]
    fn test_solid_only_pressure_fed() {
        assert!(engine_baseline(EngineCycle::GasGenerator, PropellantPreset::Solid).is_none());
        assert!(engine_baseline(EngineCycle::PressureFed, PropellantPreset::Solid).is_some());
    }

    #[test]
    fn test_new_project_is_in_design() {
        let proj = create_test_project();
        assert!(matches!(proj.status, EngineDesignStatus::InDesign { .. }));
        assert_eq!(proj.teams_assigned, 0);
        assert_eq!(proj.revision, 0);
        assert!(proj.flaws.is_empty());
    }

    #[test]
    fn test_scale_affects_thrust_and_mass() {
        let p1 = EngineProject::new(
            EngineProjectId(1), EngineId(1), "Small".into(),
            EngineCycle::GasGenerator, PropellantPreset::Kerolox, 0.5, &bal(),
        ).unwrap();
        let p2 = EngineProject::new(
            EngineProjectId(2), EngineId(2), "Big".into(),
            EngineCycle::GasGenerator, PropellantPreset::Kerolox, 2.0, &bal(),
        ).unwrap();
        // Thrust and mass scale linearly
        assert!((p2.design.thrust_n / p1.design.thrust_n - 4.0).abs() < 0.01);
        assert!((p2.design.mass_kg / p1.design.mass_kg - 4.0).abs() < 0.01);
        // Isp doesn't change
        assert_eq!(p1.design.isp_s, p2.design.isp_s);
    }

    #[test]
    fn test_vacuum_vs_sea_level_isp() {
        // M5 Task 1: one project, two nozzles. The vacuum bell trades a
        // lower exit pressure for more Isp; both come from the same
        // engine programme, so everything else is identical.
        let p = EngineProject::new(
            EngineProjectId(1), EngineId(1), "Family".into(),
            EngineCycle::GasGenerator, PropellantPreset::Kerolox, 1.0, &bal(),
        ).unwrap();
        assert!(p.has_nozzle_choice(), "a kerolox gas generator has a choice");

        let cfg = crate::balance_config::NozzleConfig::default();
        let vac = p.design_variant(true, &cfg);
        let sl = p.design_variant(false, &cfg);
        assert!(vac.isp_s > sl.isp_s, "vacuum bell should have higher Isp");
        assert!(vac.exit_pressure_pa() < sl.exit_pressure_pa(),
            "vacuum bell expands further");
        let (vac_pc, vac_eps, _) = vac.propulsion.bell().unwrap();
        let (sl_pc, sl_eps, _) = sl.propulsion.bell().unwrap();
        assert!(vac_eps > sl_eps);
        assert!(vac.is_vacuum_variant() && !sl.is_vacuum_variant());
        // Same chamber: chamber pressure and mass flow are shared, so
        // thrust rises with Isp; the long bell weighs more.
        assert_eq!(vac_pc, sl_pc);
        let gain = vac.isp_s / sl.isp_s;
        assert!(gain > 1.05 && gain < 1.10, "kerolox gas generator: 3 m bell gains ~7 %, got {gain}");
        assert!((vac.thrust_n / sl.thrust_n - gain).abs() < 1e-9, "thrust gain equals Isp gain");
        assert!((vac.mass_flow_rate() - sl.mass_flow_rate()).abs() < 1e-6, "same mass flow");
        assert!(vac.mass_kg > sl.mass_kg && vac.mass_kg < sl.mass_kg * 1.4,
            "bell extension adds mass: {} vs {}", vac.mass_kg, sl.mass_kg);
        assert_eq!(vac.id, sl.id);
        // The sea-level bell is the reference figure; it is charged the
        // pad by the ascent, not here.
        assert!((sl.isp_s - 311.0).abs() < 1e-9);
        let pad = sl.isp_fraction_at(101_325.0);
        assert!(pad > 0.85 && pad < 0.90, "90 bar sea-level bell keeps ~87 % at the pad, got {pad}");
        // The project's canonical design is the sea-level form.
        assert!(!p.design.is_vacuum_variant());
    }

    #[test]
    fn test_vacuum_only_engines_offer_no_nozzle_choice() {
        let p = EngineProject::new(
            EngineProjectId(1), EngineId(1), "Ion".into(),
            EngineCycle::ElectricPropulsion, PropellantPreset::Xenon, 1.0, &bal(),
        ).unwrap();
        assert!(!p.has_nozzle_choice());
        // Asking for the sea-level form still yields the vacuum one.
        let cfg = crate::balance_config::NozzleConfig::default();
        assert_eq!(p.design_variant(false, &cfg).isp_s, p.design_variant(true, &cfg).isp_s);
        assert!(p.design_variant(false, &cfg).is_vacuum_variant());
        assert!(!p.design.has_nozzle(), "an ion thruster has no bell the air acts on");
    }

    #[test]
    fn test_design_completes_with_work() {
        let mut proj = create_test_project();
        proj.teams_assigned = 1;
        let mut rng = test_rng();
        let mut next_flaw_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);

        let work_needed = match &proj.status {
            EngineDesignStatus::InDesign { work_required, .. } => *work_required,
            _ => panic!("should be InDesign"),
        };

        // Apply enough days
        let mut all_events = Vec::new();
        for _ in 0..(work_needed.ceil() as u32 + 1) {
            let events = proj.apply_daily_work(&mut rng, &mut next_flaw_id, &bal());
            all_events.extend(events);
        }

        // Should have completed design
        assert!(all_events.iter().any(|e| matches!(e, WorkEvent::DesignComplete)));
        assert!(matches!(proj.status, EngineDesignStatus::Testing { .. }));
    }

    #[test]
    fn test_no_work_without_teams() {
        let mut proj = create_test_project();
        assert_eq!(proj.teams_assigned, 0);
        let mut rng = test_rng();
        let mut next_flaw_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);

        for _ in 0..100 {
            let events = proj.apply_daily_work(&mut rng, &mut next_flaw_id, &bal());
            assert!(events.is_empty());
        }
        // Should still be in design at 0 work
        match &proj.status {
            EngineDesignStatus::InDesign { work_completed, .. } => {
                assert_eq!(*work_completed, 0.0);
            }
            _ => panic!("should still be InDesign"),
        }
    }

    #[test]
    fn test_more_teams_faster() {
        let mut proj1 = create_test_project();
        proj1.teams_assigned = 1;
        let mut proj2 = create_test_project();
        proj2.teams_assigned = 4;

        let mut rng1 = test_rng();
        let mut rng2 = test_rng();
        let mut id1 = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);
        let mut id2 = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(100);

        // After 10 days, proj2 should have more work done
        for _ in 0..10 {
            proj1.apply_daily_work(&mut rng1, &mut id1, &bal());
            proj2.apply_daily_work(&mut rng2, &mut id2, &bal());
        }

        let work1 = match &proj1.status {
            EngineDesignStatus::InDesign { work_completed, .. } => *work_completed,
            _ => f64::INFINITY,
        };
        let work2 = match &proj2.status {
            EngineDesignStatus::InDesign { work_completed, .. } => *work_completed,
            _ => f64::INFINITY,
        };
        assert!(work2 > work1, "4 teams should do more work than 1 team");
        // 4 teams = sqrt(4) = 2x rate
        assert!((work2 / work1 - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_revision_removes_flaw() {
        let mut proj = create_test_project();
        proj.teams_assigned = 4;
        let mut rng = test_rng();
        let mut next_flaw_id = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(0);

        // Fast-forward to testing
        for _ in 0..300 {
            proj.apply_daily_work(&mut rng, &mut next_flaw_id, &bal());
        }

        // Manually add a discovered flaw for testing
        if proj.flaws.is_empty() || !proj.flaws.iter().any(|f| f.discovered) {
            // Force a discovered flaw
            proj.flaws.push(Flaw {
                id: crate::flaw::FlawId(999),
                description: "Test flaw".into(),
                consequence: crate::flaw::FlawConsequence::EngineLoss,
                activation_chance: 0.1,
                discovery_probability: 0.5,
                discovered: true,
                trigger: crate::flaw::FlawTrigger::PerFlight,
            });
        }

        let discovered_count = proj.flaws.iter().filter(|f| f.discovered).count();
        let count_before = proj.flaws.len();

        assert!(proj.start_revision());
        assert!(matches!(proj.status, EngineDesignStatus::Revising { .. }));

        // Work through all revisions (30 work units each, sqrt(4) = 2/day)
        for _ in 0..50 {
            proj.apply_daily_work(&mut rng, &mut next_flaw_id, &bal());
        }

        assert_eq!(proj.flaws.len(), count_before - discovered_count);
        // Revision increments once per revision cycle, not per flaw
        assert_eq!(proj.revision, 1);
        assert!(matches!(proj.status, EngineDesignStatus::Testing { .. }));
    }

    #[test]
    fn test_testing_level() {
        let mut proj = create_test_project();
        proj.cumulative_testing_work = 0.0;
        assert_eq!(proj.testing_level(&bal()), "Untested");

        proj.cumulative_testing_work = 60.0;
        assert_eq!(proj.testing_level(&bal()), "Lightly Tested");

        proj.cumulative_testing_work = 150.0;
        assert_eq!(proj.testing_level(&bal()), "Moderately Tested");

        proj.cumulative_testing_work = 250.0;
        assert_eq!(proj.testing_level(&bal()), "Well Tested");

        proj.cumulative_testing_work = 400.0;
        assert_eq!(proj.testing_level(&bal()), "Thoroughly Tested");
    }

    #[test]
    fn test_hydrolox_higher_isp_than_kerolox() {
        let kero = engine_baseline(EngineCycle::GasGenerator, PropellantPreset::Kerolox).unwrap();
        let hydro = engine_baseline(EngineCycle::GasGenerator, PropellantPreset::Hydrolox).unwrap();
        assert!(hydro.isp_ref_s > kero.isp_ref_s);
    }

    #[test]
    fn test_complexity_stored_on_project() {
        let proj = create_test_project();
        // GG Kerolox: cycle=6, fuel=4 → max(6,4)=6
        assert_eq!(proj.complexity, 6);
    }
}
