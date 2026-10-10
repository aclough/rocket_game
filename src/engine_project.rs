use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::balance;
use crate::nozzle;
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
            // Kerosene cokes and hypergolics decompose in a cooling
            // jacket hot enough to drive a turbine: no expanders.
            PropellantPreset::Kerolox | PropellantPreset::Hypergolic => &[
                EngineCycle::PressureFed,
                EngineCycle::GasGenerator,
                EngineCycle::StagedCombustion,
                EngineCycle::FullFlow,
            ],
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
/// The bell every nozzle engine's reference Isp is quoted at
/// (`EngineBaseline::isp_ref_s`): one expansion ratio for all cycles,
/// so the cycle table carries combustion efficiency only.
pub const REFERENCE_EXPANSION_RATIO: f64 = 20.0;

#[derive(Debug, Clone, Copy)]
pub struct EngineBaseline {
    /// Vacuum thrust of the sea-level bell at scale 1.0 (N).
    pub thrust_n: f64,
    /// Mass at scale 1.0 with the sea-level bell (kg).
    pub mass_kg: f64,
    /// Part of `mass_kg` that doesn't scale with size: turbomachinery's
    /// minimum practical size (`turbomachinery_mass_kg`). 0 for
    /// pressure-fed engines and the kinds without a pump.
    pub fixed_mass_kg: f64,
    /// How the rest of the mass grows with size:
    /// `(mass_kg − fixed_mass_kg) × scale^mass_size_exponent + fixed_mass_kg`.
    /// Below 1 for pump-fed liquid engines, so thrust-to-weight rises
    /// gently with size (`TWR_SIZE_EXPONENT`); 1 for kinds whose mass is
    /// simply proportional (solids, reactors, ion engines, sails).
    pub mass_size_exponent: f64,
    /// Vacuum Isp (s) through a bell of `REFERENCE_EXPANSION_RATIO`
    /// for engines with a nozzle, so cycles compare at the same bell;
    /// each design's own bells are derived from it. The engine's Isp
    /// for an ion drive, 0 for a sail.
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
        let nominal_thrust = self.thrust_n * scale;
        // An expander past its heat-transfer ceiling can't hold chamber
        // pressure: pc falls as (F / ceiling)^−p in the thrust F it
        // actually delivers. The same thrust then needs a bigger, heavier
        // chamber and throat, and a smaller bell (less Isp). Solving
        // F = nominal · (F / ceiling)^−p for F gives the factor below.
        let pressure_factor = if cycle == EngineCycle::Expander {
            let p = EXPANDER_CEILING_PRESSURE_EXPONENT;
            (nominal_thrust / EXPANDER_CEILING_THRUST_N).max(1.0).powf(-p / (1.0 + p))
        } else {
            1.0
        };
        let chamber_pressure_pa = self.chamber_pressure_pa * pressure_factor;
        let propulsion = match cycle {
            EngineCycle::ElectricPropulsion => Propulsion::Electric {
                // Power draw: scales with thrust for ion drives (~30 kW/N
                // ≈ NEXT thruster ratio).
                power_draw_w: self.power_draw_w * scale,
            },
            EngineCycle::SolarSail => Propulsion::Sail,
            _ => Propulsion::nozzle_for_exit_pressure(
                chamber_pressure_pa, cfg.sea_level_exit_pressure_pa, self.gamma, true,
            ),
        };
        let isp_s = match propulsion.bell() {
            Some((_, eps, gamma)) => {
                self.isp_ref_s * nozzle::thrust_coefficient_vacuum(eps, gamma)
                    / nozzle::thrust_coefficient_vacuum(REFERENCE_EXPANSION_RATIO, gamma)
            }
            None => self.isp_ref_s,
        };
        let design = EngineDesign {
            id,
            name,
            cycle,
            thrust_n: nominal_thrust * pressure_factor,
            mass_kg: (self.mass_kg - self.fixed_mass_kg) * scale.powf(self.mass_size_exponent)
                + self.fixed_mass_kg,
            isp_s,
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
/// coolants and have no expander rows (21_2 step 4). `None` for the
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
/// else the cycle's kerolox figure, else nothing (no nozzle). A family
/// since removed (a kerosene or hypergolic expander) takes the cycle's
/// hydrolox figure, so an old built engine keeps its bell.
pub fn chamber_pressure_for_legacy(cycle: EngineCycle, mix: &[crate::engine::PropellantFraction]) -> Option<(f64, f64)> {
    let fallback = || chamber_pressure_pa(cycle, PropellantPreset::Kerolox)
        .or_else(|| chamber_pressure_pa(cycle, PropellantPreset::Hydrolox));
    match preset_for_mix(mix) {
        Some(p) => chamber_pressure_pa(cycle, p).or_else(fallback).map(|pc| (pc, exhaust_gamma(p))),
        None => fallback().map(|pc| (pc, crate::engine::DEFAULT_GAMMA)),
    }
}

/// Thrust at which `first_production_twr_1mn` is quoted.
pub const TWR_REFERENCE_THRUST_N: f64 = 1_000_000.0;

/// How thrust-to-weight grows with thrust: TWR ∝ thrust^0.10, so a
/// 25 kN engine of a family is ~0.69× its 1 MN TWR and a 7 MN one
/// ~1.21×. Small pump-fed engines are heavier per kN (RL10, HM7B,
/// LE-5 at 33–59; F-1 82 at 6.8 MN), 21_ENGINES.md §3.3.3.
pub const TWR_SIZE_EXPONENT: f64 = 0.10;

/// Thrust-to-weight of a family's *first production* engine at 1 MN of
/// rated (vacuum, short-bell) thrust (21_ENGINES.md §3.3.2 and question
/// 9): where a new programme starts, with tier-2 uprates carrying it
/// toward the refined engines (Merlin 1D, NK-33, RD-275M, Raptor).
///
/// Fitted to the survey (21_1_ENGINE_SURVEY.md) on pad TWR, which is
/// ~0.87–0.90 of the rated figure here. Mass rises with chamber
/// pressure across families (§3.3.1, question 8): high-pressure staged
/// combustion is not lighter than a gas generator. Anchors:
/// - kerolox GG: Merlin 1C 68, RD-107A 79, RS-27A 79, F-1 82;
/// - kerolox SC: RD-180 71, RD-191 86, YF-100 64, SCE-200 69;
/// - hydrolox GG: Vulcain 2 53, RS-68 46, J-2 59 vac;
/// - hydrolox SC: RS-25 54, LE-7 50, RD-0120 45;
/// - hydrolox expander: RL10A-3 51, RL10A-4 59, LE-5A 50 (vac, small);
/// - methalox GG: Longyun-70 80; methalox FF: Raptor 1 89;
/// - hypergolic GG: Viking 5C 84, YF-20B 105; SC: RD-253 139, RD-0210 105;
/// - pressure-fed: Kestrel 60, AJ10-118K 47, Aestus 27 (all small).
///
/// Combinations nobody has flown (full-flow kerolox, hydrolox or
/// hypergolic; methalox expanders) sit beside their nearest real
/// relatives.
pub fn first_production_twr_1mn(cycle: EngineCycle, preset: PropellantPreset) -> f64 {
    use EngineCycle as C;
    use PropellantPreset as P;
    match (cycle, preset) {
        (C::PressureFed, P::Kerolox | P::Methalox) => 75.0,
        (C::PressureFed, P::Hydrolox) => 45.0,
        (C::PressureFed, _) => 42.0,
        (C::GasGenerator, P::Kerolox | P::Methalox) => 86.0,
        (C::GasGenerator, P::Hydrolox) => 63.0,
        (C::GasGenerator, _) => 105.0,
        (C::Expander, P::Hydrolox) => 65.0,
        (C::Expander, _) => 60.0,
        (C::StagedCombustion, P::Kerolox) => 74.0,
        (C::StagedCombustion, P::Methalox) => 78.0,
        (C::StagedCombustion, P::Hydrolox) => 55.0,
        (C::StagedCombustion, _) => 140.0,
        (C::FullFlow, P::Kerolox) => 82.0,
        (C::FullFlow, P::Methalox) => 92.0,
        (C::FullFlow, P::Hydrolox) => 60.0,
        (C::FullFlow, _) => 140.0,
        // Not liquid pump-fed engines: their baselines carry a mass.
        (C::NuclearThermal | C::ElectricPropulsion | C::SolarSail, _) => 0.0,
    }
}

/// Thrust (at the family's base chamber pressure) above which a closed
/// expander runs out of heat to drive its turbine (21_2_ENGINE_CYCLES.md
/// §B): jacket heat grows with wall area, ~F^⅔, while the turbine power
/// it must supply grows with F. RL10, Vinci and LE-5 all sit below it.
pub const EXPANDER_CEILING_THRUST_N: f64 = 250_000.0;
/// Past the ceiling, chamber pressure falls as (F / ceiling)^−this. The
/// mass penalty follows from it: the bigger chamber the same thrust
/// then needs (and so, with the size law, more mass per kN).
pub const EXPANDER_CEILING_PRESSURE_EXPONENT: f64 = 1.0 / 3.0;

/// Mass every pump-fed engine of a cycle carries whatever its size
/// (21_2_ENGINE_CYCLES.md §C): bearings, seals, valves, igniter, gas
/// generator or preburner — the parts with a minimum practical size.
/// It makes small turbopump engines heavy per kN, leaving the small end
/// to pressure-fed engines and expanders. Calibrated on the yardstick's
/// small engines: HM7B, S5.98M, Aestus II, RD-58M, RL10, LE-5B.
pub fn turbomachinery_mass_kg(cycle: EngineCycle) -> f64 {
    match cycle {
        EngineCycle::GasGenerator => 40.0,
        EngineCycle::Expander => 20.0,
        EngineCycle::StagedCombustion | EngineCycle::FullFlow => 60.0,
        EngineCycle::PressureFed
        | EngineCycle::NuclearThermal
        | EngineCycle::ElectricPropulsion
        | EngineCycle::SolarSail => 0.0,
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
            fixed_mass_kg: 0.0,
            mass_size_exponent: 1.0,
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
            fixed_mass_kg: 0.0,
            mass_size_exponent: 1.0,
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
            fixed_mass_kg: 0.0,
            mass_size_exponent: 1.0,
            // At the reference bell; the vacuum bell it always flies
            // lands near NERVA's ~850 s.
            isp_ref_s: 822.0,
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

    // Vacuum Isp by propellant for a closed cycle at the common
    // reference bell (`REFERENCE_EXPANSION_RATIO`), then the cycle
    // adjusts. Chosen so the gas generator's own short bell keeps its
    // recognisable figure: Merlin 1D 311, J-2 / Vulcain class ~420,
    // AJ10-class hypergolic ~285; the solid's keeps Castor-class ~262.
    // Methalox sits ~14 s above kerolox at the same bell: TQ-12's 337 s
    // on its larger bell is ~324 s on Merlin's, and Raptor 1 (350 s,
    // full-flow, 250 bar) is ~12 s over the RD-180 (338 s).
    let base_isp_ref = match preset {
        PropellantPreset::Kerolox => 316.7,
        PropellantPreset::Hydrolox => 424.6,
        PropellantPreset::Methalox => 330.5,
        PropellantPreset::Hypergolic => 295.4,
        PropellantPreset::Solid => 270.4,
        PropellantPreset::Hydrogen => unreachable!(),
        PropellantPreset::Xenon => unreachable!(),
        PropellantPreset::Photon => unreachable!(),
    };

    // Cycle efficiency at the same bell. Chamber pressure already sets
    // how far each cycle's bells expand (and so most of the spread);
    // this carries only what the bell doesn't: a gas generator dumps
    // its turbine exhaust at low Isp (HM7B 446 s against RL10A-4's
    // 451 s on the same ε ~84), and a pressure-fed chamber burns less
    // completely. Expanders, staged combustion and full-flow burn all
    // their propellant in the chamber.
    let isp_mult = match cycle {
        EngineCycle::PressureFed => 0.98,
        EngineCycle::GasGenerator => 0.98,
        EngineCycle::Expander => 1.00,
        EngineCycle::StagedCombustion => 1.00,
        EngineCycle::FullFlow => 1.00,
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

    let thrust = base_thrust * thrust_mult;
    // A solid motor's case is its tank; its mass stays in proportion.
    // A pump-fed engine's 1 MN mass is split into a fixed turbomachinery
    // part and the rest, which scales as size^0.9; the 1 MN anchor
    // stays where the TWR table puts it.
    let (mass, fixed_mass_kg, mass_size_exponent) = if preset == PropellantPreset::Solid {
        (thrust / (40.0 * G0), 0.0, 1.0)
    } else {
        let fixed = turbomachinery_mass_kg(cycle);
        let mass_1mn = TWR_REFERENCE_THRUST_N / (first_production_twr_1mn(cycle, preset) * G0);
        let exponent = 1.0 - TWR_SIZE_EXPONENT;
        let mass = (mass_1mn - fixed) * (thrust / TWR_REFERENCE_THRUST_N).powf(exponent) + fixed;
        (mass, fixed, exponent)
    };

    Some(EngineBaseline {
        thrust_n: thrust,
        mass_kg: mass,
        fixed_mass_kg,
        mass_size_exponent,
        isp_ref_s: base_isp_ref * isp_mult,
        chamber_pressure_pa: chamber_pressure_pa(cycle, preset)?,
        gamma: exhaust_gamma(preset),
        vacuum_only: cycle == EngineCycle::Expander,
        // Chemical engines don't draw electrical power.
        power_draw_w: 0.0,
    })
}

/// Scale range for engine design. Player picks a value in [min_scale, max_scale].
/// The family's default engine size, as a multiple of its baseline.
/// Constructors take a scale in this sense; the project stores the
/// resulting size (`EngineSpec::size`).
pub const DEFAULT_SCALE: f64 = 1.0;

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
    /// How big the engine is, in its kind's natural unit (`SizeUnit`):
    /// throat area for a bell, input power for an electric thruster,
    /// sail area for a sail. Fixed when the design is laid out;
    /// improvements move the engine's figures, not its size. Saves
    /// before v4 stored a `scale` multiplier instead (`save::migrate_json`).
    pub size: f64,
}

/// Solar radiation pressure on a sail at 1 AU (N/m²): ~9.1 µN/m² for a
/// perfect reflector, a little less for a real film.
pub const SAIL_THRUST_N_PER_M2_1AU: f64 = 9.0e-6;

/// What an engine's size is measured in, by propulsion kind
/// (21_ENGINES.md, question 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeUnit {
    /// A bell: throat area (m²). The editor shows it as rated thrust.
    ThroatArea,
    /// An electric thruster: input power (W).
    InputPower,
    /// A sail: area (m²).
    SailArea,
}

impl SizeUnit {
    /// The unit a family of `cycle` is sized in.
    pub fn of_cycle(cycle: EngineCycle) -> SizeUnit {
        match cycle {
            EngineCycle::ElectricPropulsion => SizeUnit::InputPower,
            EngineCycle::SolarSail => SizeUnit::SailArea,
            _ => SizeUnit::ThroatArea,
        }
    }

    /// The editor's range for the size's headline figure (`headline`):
    /// 10 kN–10 MN of thrust, 1 kW–1 MW of input power, 10 m²–1 km² of
    /// sail. Wide enough for Rutherford to RD-170 and a CubeSat sail to
    /// a mission-class one.
    pub fn headline_range(self) -> (f64, f64) {
        match self {
            SizeUnit::ThroatArea => (10_000.0, 10_000_000.0),
            SizeUnit::InputPower => (1_000.0, 1_000_000.0),
            SizeUnit::SailArea => (10.0, 1_000_000.0),
        }
    }
}

/// An engine design's size in its kind's natural unit (see `SizeUnit`).
/// A bell fixture with no nozzle data has a throat of 0.
pub fn natural_size(design: &EngineDesign) -> f64 {
    match design.propulsion {
        Propulsion::Nozzle { .. } => design.throat_area_m2(),
        Propulsion::Electric { power_draw_w } => power_draw_w,
        Propulsion::Sail => design.thrust_n / SAIL_THRUST_N_PER_M2_1AU,
    }
}

/// The figure the editor shows for an engine's size, in SI units: rated
/// thrust for a bell, input power for an electric thruster, area for a
/// sail. It is what the player recognises; the size behind it is what
/// is stored.
pub fn headline(design: &EngineDesign) -> f64 {
    match design.propulsion {
        Propulsion::Nozzle { .. } => design.thrust_n,
        Propulsion::Electric { power_draw_w } => power_draw_w,
        Propulsion::Sail => natural_size(design),
    }
}

/// The size of a (cycle, propellant) family's default engine — the one
/// `DEFAULT_SCALE` gives — in its natural unit.
pub fn default_size(cycle: EngineCycle, preset: PropellantPreset, cfg: &NozzleConfig) -> Option<f64> {
    let design = engine_baseline(cycle, preset)?
        .design(EngineId(0), String::new(), cycle, preset, DEFAULT_SCALE, false, cfg);
    Some(natural_size(&design))
}

/// The scale (multiple of the family default) whose canonical design
/// has `target` as its headline figure. Linear except where a
/// vacuum-only family's bell gain depends on the throat, so it corrects
/// a few times.
pub fn scale_for_headline(
    cycle: EngineCycle, preset: PropellantPreset, target: f64, cfg: &NozzleConfig,
) -> Option<f64> {
    let baseline = engine_baseline(cycle, preset)?;
    let at = |scale: f64| headline(&baseline.design(EngineId(0), String::new(), cycle, preset, scale, false, cfg));
    let mut scale = target / at(DEFAULT_SCALE);
    for _ in 0..6 {
        let got = at(scale);
        if got <= 0.0 {
            break;
        }
        scale *= target / got;
    }
    Some(scale)
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
            EngineImprovementKind::Uprate { kind, magnitude, .. } => match kind {
                UprateKind::ChamberPressure => {
                    // Same throat and bell, more pressure behind them.
                    self.thrust_n *= 1.0 + magnitude;
                    if let Some((pc, eps, gamma)) = self.propulsion.bell() {
                        self.set_nozzle(pc * (1.0 + magnitude), eps, gamma);
                    }
                }
                UprateKind::Turbomachinery => self.mass_kg *= 1.0 - magnitude,
                UprateKind::Injector => self.isp_s *= 1.0 + magnitude,
                // Build work is the project's, not the design's.
                UprateKind::Production => {}
            },
        }
    }

    fn roll_uprate(&self, rng: &mut StdRng, id: ImprovementId, balance_cfg: &BalanceConfig) -> Option<EngineImprovement> {
        // Sails and ion engines have no chamber, turbopump or injector.
        if !self.propulsion.has_nozzle() {
            return None;
        }
        Some(generate_uprate(rng, self.cycle, id, &balance_cfg.uprates))
    }

    fn is_uprate(kind: &EngineImprovementKind) -> bool {
        matches!(kind, EngineImprovementKind::Uprate { .. })
    }

    fn improvement_side_effects(kind: &EngineImprovementKind) -> (u32, f64, f64) {
        match kind {
            EngineImprovementKind::Uprate { kind, magnitude, flaw_rolls, flaw_chance } => {
                let divisor = if *kind == UprateKind::Production { *magnitude } else { 1.0 };
                (*flaw_rolls, *flaw_chance, divisor)
            }
            _ => (0, 0.0, 1.0),
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
        let size = scale * default_size(cycle, preset, &balance_cfg.nozzle)?;
        Some(DesignProject::new_in_design(
            project_id, design, EngineSpec { preset, size }, complexity, work_required, None,
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
        let Some(base) = default_size(cycle, preset, &balance_cfg.nozzle) else {
            return false;
        };
        self.design = baseline.design(self.design.id, name, cycle, preset, scale, false, &balance_cfg.nozzle);
        self.spec = EngineSpec { preset, size: scale * base };
        // The propellant decides the technology: an engine edited to
        // methalox is a methalox engine and inherits that technology's
        // deficiencies when its design completes. (Every engine starts as
        // kerolox, so without this no edited engine ever did.)
        self.technology_id = crate::technology::technology_for_preset(preset);
        self.complexity = complexity;
        self.clamp_work_after_edit(work_required);
        true
    }

    /// Move a project whose (cycle, propellant) family no longer exists
    /// onto the gas generator of the same propellant (21_2 step 4: the
    /// kerosene and hypergolic expanders). The new design delivers the
    /// same vacuum thrust; actualized Isp and mass improvements are
    /// re-applied (thrust ones are already in that thrust). Flaws,
    /// improvements, work and status stay as they were. Returns whether
    /// anything changed; a project whose family exists is left alone.
    pub fn migrate_removed_family(&mut self, balance_cfg: &BalanceConfig) -> bool {
        let preset = self.spec.preset;
        if engine_baseline(self.design.cycle, preset).is_some() {
            return false;
        }
        let cycle = EngineCycle::GasGenerator;
        let (Some(baseline), Some(base)) = (
            engine_baseline(cycle, preset),
            default_size(cycle, preset, &balance_cfg.nozzle),
        ) else {
            return false;
        };
        let nozzle = &balance_cfg.nozzle;
        let target = self.design.thrust_n;
        let vacuum_thrust = |scale: f64| {
            baseline.design(self.design.id, String::new(), cycle, preset, scale, true, nozzle).thrust_n
        };
        let mut scale = target / vacuum_thrust(DEFAULT_SCALE);
        for _ in 0..6 {
            scale *= target / vacuum_thrust(scale);
        }
        let mut design = baseline.design(self.design.id, self.design.name.clone(), cycle, preset, scale, false, nozzle);
        for imp in self.improvements.iter().filter(|i| i.actualized) {
            match &imp.kind {
                EngineImprovementKind::Isp(_) | EngineImprovementKind::Mass(_)
                | EngineImprovementKind::Uprate { kind: UprateKind::Injector | UprateKind::Turbomachinery, .. } => {
                    design.apply_improvement(&imp.kind);
                }
                _ => {}
            }
        }
        self.design = design;
        self.spec = EngineSpec { preset, size: scale * base };
        self.complexity = balance::combined_complexity(cycle, &preset.propellants());
        true
    }

    /// The project's size as a multiple of its family's default
    /// (`DEFAULT_SCALE`), for the formulas that still take a scale.
    pub fn scale(&self, cfg: &NozzleConfig) -> f64 {
        default_size(self.design.cycle, self.spec.preset, cfg)
            .filter(|base| *base > 0.0)
            .map_or(DEFAULT_SCALE, |base| self.spec.size / base)
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
    /// A tier-2 uprate (21_ENGINES.md step 7): bigger, and risky —
    /// actualising it rolls `flaw_rolls` chances of `flaw_chance` to add
    /// a new hidden flaw. `magnitude` is a fraction, except for
    /// `Production`, where it divides build work.
    Uprate { kind: UprateKind, magnitude: f64, flaw_rolls: u32, flaw_chance: f64 },
}

/// What an uprate changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UprateKind {
    /// Chamber pressure up on the same hardware: thrust up by the same
    /// fraction at the same mass; pad Isp follows through the nozzle model.
    ChamberPressure,
    /// Lighter turbomachinery: mass down.
    Turbomachinery,
    /// Better injection and combustion: Isp up.
    Injector,
    /// A production-process change: build work divided.
    Production,
}

impl std::fmt::Display for EngineImprovementKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineImprovementKind::Isp(frac) => write!(f, "+{:.0}% Isp", frac * 100.0),
            EngineImprovementKind::Mass(frac) => write!(f, "-{:.0}% mass", frac * 100.0),
            EngineImprovementKind::Thrust(frac) => write!(f, "+{:.0}% thrust", frac * 100.0),
            EngineImprovementKind::Uprate { kind, magnitude, flaw_rolls, flaw_chance } => {
                let effect = match kind {
                    UprateKind::ChamberPressure => format!("+{:.0}% chamber pressure and thrust", magnitude * 100.0),
                    UprateKind::Turbomachinery => format!("-{:.0}% mass", magnitude * 100.0),
                    UprateKind::Injector => format!("+{:.0}% Isp", magnitude * 100.0),
                    UprateKind::Production => format!("build time ÷{:.1}", magnitude),
                };
                let expected = *flaw_rolls as f64 * flaw_chance;
                write!(f, "{effect} · may add up to {flaw_rolls} flaw{} ({expected:.2} expected)",
                    if *flaw_rolls == 1 { "" } else { "s" })
            }
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
        accepted: true,
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
        (_, K::Uprate { kind, .. }) => match (kind, pick) {
            (UprateKind::ChamberPressure, 0) => "Uprate: run the chamber hotter on flight-proven hardware",
            (UprateKind::ChamberPressure, _) => "Uprate: higher turbopump power level",
            (UprateKind::Turbomachinery, 0) => "Uprate: single-shaft turbopump redesign",
            (UprateKind::Turbomachinery, _) => "Uprate: lighter turbomachinery from flight data",
            (UprateKind::Injector, 0) => "Uprate: new injector pattern",
            (UprateKind::Injector, _) => "Uprate: combustion stability margin traded for efficiency",
            (UprateKind::Production, 0) => "Uprate: printed parts replace welded assemblies",
            (UprateKind::Production, _) => "Uprate: fewer parts, simpler tooling",
        },
    }
}

/// Roll a tier-2 uprate (21_ENGINES.md step 7) by the configured weights.
fn generate_uprate(
    rng: &mut StdRng, cycle: EngineCycle, id: ImprovementId, cfg: &crate::balance_config::UpratesConfig,
) -> EngineImprovement {
    let options = [
        (UprateKind::ChamberPressure, &cfg.chamber_pressure),
        (UprateKind::Turbomachinery, &cfg.turbomachinery),
        (UprateKind::Injector, &cfg.injector),
        (UprateKind::Production, &cfg.production),
    ];
    let total: f64 = options.iter().map(|(_, r)| r.weight).sum();
    let mut roll = rng.gen::<f64>() * total;
    let (kind, r) = options.iter()
        .find(|(_, r)| { roll -= r.weight; roll < 0.0 })
        .copied()
        .unwrap_or(options[options.len() - 1]);
    let magnitude = rng.gen_range(r.min..=r.max);
    let pick = rng.gen_range(0u32..2);
    let kind = EngineImprovementKind::Uprate {
        kind, magnitude, flaw_rolls: r.flaw_rolls, flaw_chance: r.flaw_chance,
    };
    let description = improvement_description(cycle, &kind, pick);
    EngineImprovement { id, description: description.to_string(), kind, actualized: false, accepted: false }
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

    /// The editor's list and the baseline table agree both ways: no
    /// family the editor can't offer has a baseline (21_2 step 4 took
    /// out the kerosene and hypergolic expanders).
    #[test]
    fn only_compatible_combos_have_baselines() {
        let cycles = [
            EngineCycle::PressureFed, EngineCycle::GasGenerator, EngineCycle::Expander,
            EngineCycle::StagedCombustion, EngineCycle::FullFlow, EngineCycle::NuclearThermal,
            EngineCycle::ElectricPropulsion, EngineCycle::SolarSail,
        ];
        for preset in PropellantPreset::ALL {
            for cycle in cycles {
                assert_eq!(
                    engine_baseline(cycle, *preset).is_some(),
                    preset.compatible_cycles().contains(&cycle),
                    "{cycle:?}/{preset:?}",
                );
            }
        }
        assert!(engine_baseline(EngineCycle::Expander, PropellantPreset::Kerolox).is_none());
        assert!(engine_baseline(EngineCycle::Expander, PropellantPreset::Hypergolic).is_none());
        assert!(engine_baseline(EngineCycle::Expander, PropellantPreset::Methalox).is_some());
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

    /// Past ~250 kN a closed expander loses chamber pressure: a 1 MN
    /// hydrolox expander is clearly worse than a gas generator of the
    /// same thrust, while a 110 kN one is at full pressure (21_2 §B).
    #[test]
    fn expanders_lose_pressure_past_the_heat_transfer_ceiling() {
        let cfg = bal().nozzle;
        let at = |cycle, thrust: f64| {
            let scale = scale_for_headline(cycle, PropellantPreset::Hydrolox, thrust, &cfg).unwrap();
            engine_baseline(cycle, PropellantPreset::Hydrolox).unwrap()
                .design(EngineId(0), String::new(), cycle, PropellantPreset::Hydrolox, scale, true, &cfg)
        };
        let base_pc = chamber_pressure_pa(EngineCycle::Expander, PropellantPreset::Hydrolox).unwrap();
        let small = at(EngineCycle::Expander, 110_000.0);
        assert_eq!(small.propulsion.bell().unwrap().0, base_pc);

        let big = at(EngineCycle::Expander, 1_000_000.0);
        let gg = at(EngineCycle::GasGenerator, 1_000_000.0);
        assert!((big.thrust_n / 1e6 - 1.0).abs() < 0.01);
        // pc falls as (F / 250 kN)^(-1/3), F the rated (short-bell)
        // thrust: ~890 kN behind this 1 MN vacuum figure, so ≈ 0.65.
        let pc_ratio = big.propulsion.bell().unwrap().0 / base_pc;
        assert!(pc_ratio > 0.62 && pc_ratio < 0.68, "pc ratio {pc_ratio}");
        let twr = |e: &EngineDesign| e.thrust_n / (e.mass_kg * G0);
        assert!(twr(&big) < 0.8 * twr(&gg), "expander TWR {} vs GG {}", twr(&big), twr(&gg));
        assert!(big.isp_s < gg.isp_s, "expander Isp {} vs GG {}", big.isp_s, gg.isp_s);
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
        // Thrust scales linearly; mass a little slower, so the bigger
        // engine has the better thrust-to-weight (TWR_SIZE_EXPONENT),
        // with the fixed turbomachinery mass not scaling at all.
        assert!((p2.design.thrust_n / p1.design.thrust_n - 4.0).abs() < 0.01);
        let fixed = turbomachinery_mass_kg(EngineCycle::GasGenerator);
        let mass_ratio = 4.0_f64.powf(1.0 - TWR_SIZE_EXPONENT);
        assert!(((p2.design.mass_kg - fixed) / (p1.design.mass_kg - fixed) - mass_ratio).abs() < 1e-9);
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
        // The sea-level bell keeps Merlin's figure; it is charged the
        // pad by the ascent, not here.
        assert!((sl.isp_s - 311.0).abs() < 0.5, "got {}", sl.isp_s);
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
            let events = proj.apply_daily_work(&mut rng, &mut next_flaw_id, &bal(), 0);
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
            let events = proj.apply_daily_work(&mut rng, &mut next_flaw_id, &bal(), 0);
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
            proj1.apply_daily_work(&mut rng1, &mut id1, &bal(), 0);
            proj2.apply_daily_work(&mut rng2, &mut id2, &bal(), 0);
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
            proj.apply_daily_work(&mut rng, &mut next_flaw_id, &bal(), 0);
        }

        // Manually add a discovered flaw for testing
        if proj.flaws.is_empty() || !proj.flaws.iter().any(|f| f.discovered) {
            // Force a discovered flaw
            proj.flaws.push(Flaw {
                id: crate::flaw::FlawId(999),
                description: "Test flaw".into(),
                consequence: crate::flaw::FlawConsequence::EngineLoss,
                activation_chance: 0.1,
                discovery_roll: 0.5,
                discovered: true,
                trigger: crate::flaw::FlawTrigger::PerFlight,
                phase: crate::flaw::FlawPhase::Burn, stage_group: None, exposure: crate::flaw::FlawExposure::Bench,
            });
        }

        let discovered_count = proj.flaws.iter().filter(|f| f.discovered).count();
        let count_before = proj.flaws.len();

        assert!(proj.start_revision());
        assert!(matches!(proj.status, EngineDesignStatus::Revising { .. }));

        // Work through all revisions (30 work units each, sqrt(4) = 2/day)
        for _ in 0..50 {
            proj.apply_daily_work(&mut rng, &mut next_flaw_id, &bal(), 0);
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

    /// Run testing cycles until something happens or `cycles` pass.
    fn test_cycles(p: &mut EngineProject, cfg: &BalanceConfig, cycles: u32) {
        let mut rng = test_rng();
        let mut next_flaw = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(100);
        p.teams_assigned = 1;
        let days = (cfg.work.testing_cycle_work / crate::team::effective_work_rate(1)).ceil() as u32 * cycles;
        for _ in 0..days {
            p.apply_daily_work(&mut rng, &mut next_flaw, cfg, 0);
        }
    }

    /// 21_ENGINES.md step 7: an uprate needs both a testing record and
    /// flight heritage; it waits for acceptance; a revision then applies
    /// it and rolls its flaws.
    #[test]
    fn uprates_need_heritage_wait_for_acceptance_and_risk_flaws() {
        let mut cfg = bal();
        cfg.uprates.chance_per_cycle = 1.0;
        cfg.flaws.improvement_discovery_chance = 0.0;
        // Only chamber-pressure uprates, each certain to add all 3 flaws.
        cfg.uprates.turbomachinery.weight = 0.0;
        cfg.uprates.injector.weight = 0.0;
        cfg.uprates.production.weight = 0.0;
        cfg.uprates.chamber_pressure.flaw_chance = 1.0;
        let mut p = create_test_project();
        p.status = EngineDesignStatus::Testing { work_completed: 0.0 };
        p.flaws.clear();
        p.cumulative_testing_work = cfg.work.testing_cycle_work * 20.0;

        // Thoroughly tested but never flown: nothing on offer.
        test_cycles(&mut p, &cfg, 3);
        assert_eq!(p.offered_uprates().count(), 0, "no uprate without flight heritage");

        p.flights = cfg.uprates.min_flights;
        test_cycles(&mut p, &cfg, 3);
        assert_eq!(p.offered_uprates().count(), 1, "one uprate on offer at a time");
        // An offered uprate is not something a revision will apply.
        assert!(!p.start_revision(), "nothing accepted, nothing to revise");

        let before = p.design.clone();
        let (pc_before, eps, _) = before.propulsion.bell().unwrap();
        let magnitude = match p.offered_uprates().next().unwrap().kind {
            EngineImprovementKind::Uprate { magnitude, .. } => magnitude,
            _ => unreachable!(),
        };
        assert_eq!(p.accept_uprates(), 1);
        assert!(p.start_revision());
        // Run the revision out (no flaws discovered, so testing resumes).
        let mut rng = test_rng();
        let mut next_flaw = crate::id::IdAllocator::<crate::flaw::FlawId>::starting_at(100);
        for _ in 0..200 {
            p.apply_daily_work(&mut rng, &mut next_flaw, &cfg, 0);
            if matches!(p.status, EngineDesignStatus::Testing { .. }) { break; }
        }
        assert!(p.improvements.iter().all(|i| i.actualized));
        assert_eq!(p.flaws.len(), 3, "three rolls at 100 % leave three hidden flaws");
        assert!(p.flaws.iter().all(|f| !f.discovered));
        // Chamber pressure: thrust and pressure up together, same bell,
        // same mass; the pad keeps a larger share of the vacuum thrust.
        assert!((p.design.thrust_n / before.thrust_n - (1.0 + magnitude)).abs() < 1e-9);
        let (pc_after, eps_after, _) = p.design.propulsion.bell().unwrap();
        assert!((pc_after / pc_before - (1.0 + magnitude)).abs() < 1e-9);
        assert_eq!(eps_after, eps);
        assert_eq!(p.design.mass_kg, before.mass_kg);
        assert!(p.design.isp_fraction_at(101_325.0) > before.isp_fraction_at(101_325.0));
    }

    /// A production uprate shortens every later build; it touches
    /// nothing on the design.
    #[test]
    fn a_production_uprate_divides_build_work() {
        let kind = EngineImprovementKind::Uprate {
            kind: UprateKind::Production, magnitude: 1.6, flaw_rolls: 1, flaw_chance: 0.25,
        };
        assert_eq!(EngineDesign::improvement_side_effects(&kind), (1, 0.25, 1.6));
        let mut d = create_test_project().design;
        let before = d.clone();
        d.apply_improvement(&kind);
        assert_eq!(d.thrust_n, before.thrust_n);
        assert_eq!(d.mass_kg, before.mass_kg);
        assert_eq!(d.isp_s, before.isp_s);
        // Tier-1 improvements carry no risk.
        assert_eq!(EngineDesign::improvement_side_effects(&EngineImprovementKind::Isp(0.02)), (0, 0.0, 1.0));
    }

    /// The propellant decides the technology: editing an engine to an
    /// experimental propellant attaches that technology (so its
    /// deficiencies land when the design completes), and editing it back
    /// takes it off.
    #[test]
    fn editing_the_propellant_sets_the_technology() {
        let mut p = create_test_project();
        assert_eq!(p.technology_id, None);
        assert!(p.apply_edit("E".into(), EngineCycle::GasGenerator, PropellantPreset::Methalox, 1.0, &bal()));
        assert_eq!(p.technology_id, Some(crate::technology::TECH_METHALOX));
        assert!(p.apply_edit("E".into(), EngineCycle::GasGenerator, PropellantPreset::Kerolox, 1.0, &bal()));
        assert_eq!(p.technology_id, None);
    }
}
