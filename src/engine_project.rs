use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::balance;
use crate::engine::{EngineDesign, EngineCycle, EngineId, PropellantFraction, G0};
use crate::flaw::{self, Flaw, TESTING_CYCLE_WORK, FLAW_REVISION_WORK};
use crate::propellant::Propellant;
use crate::third_party::ContractedEngineId;

/// A preset propellant combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropellantPreset {
    Kerolox,
    Hydrolox,
    Methalox,
    Hypergolic,
    Solid,
}

impl PropellantPreset {
    pub const ALL: &[PropellantPreset] = &[
        PropellantPreset::Kerolox,
        PropellantPreset::Hydrolox,
        PropellantPreset::Methalox,
        PropellantPreset::Hypergolic,
        PropellantPreset::Solid,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            PropellantPreset::Kerolox => "Kerolox",
            PropellantPreset::Hydrolox => "Hydrolox",
            PropellantPreset::Methalox => "Methalox",
            PropellantPreset::Hypergolic => "Hypergolic",
            PropellantPreset::Solid => "Solid",
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
/// These represent realistic-ish performance inspired by real engines.
/// Thrust and mass scale linearly with the scale factor.
/// Isp is fixed (doesn't change with scale).
#[derive(Debug, Clone, Copy)]
pub struct EngineBaseline {
    /// Baseline thrust in Newtons at scale 1.0.
    pub thrust_n: f64,
    /// Baseline mass in kg at scale 1.0.
    pub mass_kg: f64,
    /// Specific impulse in seconds (vacuum).
    pub isp_vac_s: f64,
    /// Specific impulse in seconds (sea level, if applicable).
    pub isp_sl_s: f64,
    /// Exit pressure in Pa when optimized for vacuum.
    pub exit_pressure_vac_pa: f64,
    /// Exit pressure in Pa when optimized for sea level.
    pub exit_pressure_sl_pa: f64,
    /// If true, this engine can only be built in vacuum configuration.
    pub vacuum_only: bool,
}

/// Get the baseline engine parameters for a (cycle, propellant) combination.
///
/// These are the "middle of the range" values at scale 1.0.
/// Inspired by real engines but simplified for gameplay.
pub fn engine_baseline(cycle: EngineCycle, preset: PropellantPreset) -> Option<EngineBaseline> {
    // Solid engines can only be pressure-fed
    if preset == PropellantPreset::Solid && cycle != EngineCycle::PressureFed {
        return None;
    }

    // Base Isp values by propellant (vacuum), then cycle adjusts
    let (base_isp_vac, base_isp_sl) = match preset {
        PropellantPreset::Kerolox => (310.0, 270.0),
        PropellantPreset::Hydrolox => (440.0, 360.0),
        PropellantPreset::Methalox => (350.0, 305.0),
        PropellantPreset::Hypergolic => (290.0, 255.0),
        PropellantPreset::Solid => (265.0, 240.0),
    };

    // Cycle multipliers for Isp (relative to GasGenerator baseline)
    let isp_mult = match cycle {
        EngineCycle::PressureFed => 0.92,
        EngineCycle::GasGenerator => 1.00,
        EngineCycle::Expander => 1.04,
        EngineCycle::StagedCombustion => 1.06,
        EngineCycle::FullFlow => 1.08,
    };

    // Base thrust at scale 1.0 by propellant type
    let base_thrust = match preset {
        PropellantPreset::Kerolox => 900_000.0,      // ~Merlin-class
        PropellantPreset::Hydrolox => 110_000.0,      // ~RL-10-class
        PropellantPreset::Methalox => 700_000.0,      // ~Raptor-class
        PropellantPreset::Hypergolic => 45_000.0,     // ~AJ10-class
        PropellantPreset::Solid => 500_000.0,         // ~medium SRB
    };

    // Cycle multipliers for thrust (relative to GasGenerator)
    let thrust_mult = match cycle {
        EngineCycle::PressureFed => 0.60,
        EngineCycle::GasGenerator => 1.00,
        EngineCycle::Expander => 0.80,
        EngineCycle::StagedCombustion => 1.15,
        EngineCycle::FullFlow => 1.30,
    };

    // Thrust-to-weight ratio by cycle (higher = lighter for given thrust)
    // This gives us mass from thrust
    let twr = match cycle {
        EngineCycle::PressureFed => 40.0,    // simple, light
        EngineCycle::GasGenerator => 80.0,   // good TWR
        EngineCycle::Expander => 50.0,       // moderate
        EngineCycle::StagedCombustion => 70.0, // heavy but powerful
        EngineCycle::FullFlow => 60.0,       // heaviest complex cycle
    };

    let thrust = base_thrust * thrust_mult;
    let mass = thrust / (twr * G0);

    // Exit pressure depends on optimization:
    // Sea-level: ~80 kPa (near-optimal at 101 kPa ambient)
    // Vacuum: ~7 kPa (large nozzle, optimized for space)
    // Expander cycles always vacuum (low chamber pressure)
    let exit_pressure_sl = 80_000.0;
    let exit_pressure_vac = 7_000.0;

    Some(EngineBaseline {
        thrust_n: thrust,
        mass_kg: mass,
        isp_vac_s: base_isp_vac * isp_mult,
        isp_sl_s: base_isp_sl * isp_mult,
        exit_pressure_vac_pa: exit_pressure_vac,
        exit_pressure_sl_pa: exit_pressure_sl,
        vacuum_only: cycle == EngineCycle::Expander,
    })
}

/// Scale range for engine design. Player picks a value in [min_scale, max_scale].
pub const MIN_SCALE: f64 = 0.25;
pub const MAX_SCALE: f64 = 4.0;
pub const DEFAULT_SCALE: f64 = 1.0;
pub const SCALE_STEP: f64 = 0.25;

/// Status of an engine design project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineDesignStatus {
    InDesign { work_completed: f64, work_required: f64 },
    Testing { work_completed: f64 },
    /// Revising discovered flaws and actualizing improvements.
    /// Works through flaw indices first, then improvement indices.
    Revising {
        remaining_flaw_indices: Vec<usize>,
        remaining_improvement_indices: Vec<usize>,
        work_completed: f64,
    },
}

/// Unique identifier for an engine project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngineProjectId(pub u64);

/// Where an engine comes from — player-designed or contracted third-party.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EngineSource {
    PlayerDesign(EngineProjectId),
    Contracted(ContractedEngineId),
}

/// An engine design project with workflow state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineProject {
    pub project_id: EngineProjectId,
    pub design: EngineDesign,
    pub preset: PropellantPreset,
    pub scale: f64,
    pub status: EngineDesignStatus,
    pub flaws: Vec<Flaw>,
    pub revision: u32,
    pub teams_assigned: u32,
    pub complexity: u32,
    /// Cumulative engineering salary spent on this project (NRE).
    #[serde(default)]
    pub nre_cost: f64,
    /// Improvements discovered during testing. Pending ones need a revision to actualize.
    #[serde(default)]
    pub improvements: Vec<Improvement>,
}

impl EngineProject {
    /// Create a new engine project from player choices.
    pub fn new(
        project_id: EngineProjectId,
        engine_id: EngineId,
        name: String,
        cycle: EngineCycle,
        preset: PropellantPreset,
        scale: f64,
        use_vacuum_isp: bool,
    ) -> Option<Self> {
        let baseline = engine_baseline(cycle, preset)?;
        let propellants = preset.propellants();
        let complexity = balance::combined_complexity(cycle, &propellants);
        let effective = balance::effective_complexity(cycle, &propellants);
        let work_required = balance::design_work_required(effective);

        let thrust = baseline.thrust_n * scale;
        let mass = baseline.mass_kg * scale;
        // Expander cycles are always vacuum-optimized
        let use_vacuum = if baseline.vacuum_only { true } else { use_vacuum_isp };
        let isp = if use_vacuum { baseline.isp_vac_s } else { baseline.isp_sl_s };
        let exit_pressure = if use_vacuum { baseline.exit_pressure_vac_pa } else { baseline.exit_pressure_sl_pa };

        let design = EngineDesign {
            id: engine_id,
            name,
            cycle,
            thrust_n: thrust,
            mass_kg: mass,
            isp_s: isp,
            exit_pressure_pa: exit_pressure,
            needs_atmosphere: !use_vacuum,
            propellant_mix: preset.propellant_mix(),
        };

        Some(EngineProject {
            project_id,
            design,
            preset,
            scale,
            status: EngineDesignStatus::InDesign {
                work_completed: 0.0,
                work_required,
            },
            flaws: Vec::new(),
            revision: 0,
            teams_assigned: 0,
            complexity,
            nre_cost: 0.0,
            improvements: Vec::new(),
        })
    }

    /// Apply one day of work. Returns any completed work events.
    pub fn apply_daily_work(&mut self, rng: &mut StdRng, next_flaw_id: &mut u64) -> Vec<WorkEvent> {
        if self.teams_assigned == 0 {
            return Vec::new();
        }
        let work = crate::team::effective_work_rate(self.teams_assigned);
        let mut events = Vec::new();

        match &mut self.status {
            EngineDesignStatus::InDesign { work_completed, work_required } => {
                *work_completed += work;
                if *work_completed >= *work_required {
                    // Design complete — generate flaws
                    let propellants = self.preset.propellants();
                    let eff = balance::effective_complexity(self.design.cycle, &propellants);
                    self.flaws = flaw::generate_flaws(eff, rng, next_flaw_id);
                    let flaw_count = self.flaws.len() as u32;
                    self.status = EngineDesignStatus::Testing { work_completed: 0.0 };
                    events.push(WorkEvent::DesignComplete { flaw_count });
                }
            }
            EngineDesignStatus::Testing { work_completed } => {
                *work_completed += work;
                // Check for testing cycle completion
                while *work_completed >= TESTING_CYCLE_WORK {
                    *work_completed -= TESTING_CYCLE_WORK;
                    let discovered = flaw::roll_discoveries_with_rng(&mut self.flaws, rng);
                    for idx in discovered {
                        events.push(WorkEvent::FlawDiscovered {
                            flaw_description: self.flaws[idx].description.clone(),
                        });
                    }
                    // Roll for improvement discovery
                    if rng.gen::<f64>() < IMPROVEMENT_DISCOVERY_CHANCE {
                        let improvement = generate_improvement(rng);
                        events.push(WorkEvent::ImprovementDiscovered {
                            description: format!("{}: {}", improvement.description, improvement.kind),
                        });
                        self.improvements.push(improvement);
                    }
                    events.push(WorkEvent::TestingCycleComplete);
                }
            }
            EngineDesignStatus::Revising { remaining_flaw_indices, remaining_improvement_indices, work_completed } => {
                *work_completed += work;
                // Process flaws first
                while *work_completed >= FLAW_REVISION_WORK && !remaining_flaw_indices.is_empty() {
                    *work_completed -= FLAW_REVISION_WORK;
                    let fi = remaining_flaw_indices.remove(0);
                    self.flaws.remove(fi);
                    events.push(WorkEvent::RevisionComplete);
                    for idx in remaining_flaw_indices.iter_mut() {
                        if *idx > fi {
                            *idx -= 1;
                        }
                    }
                }
                // Then actualize improvements
                while *work_completed >= FLAW_REVISION_WORK && !remaining_improvement_indices.is_empty() {
                    *work_completed -= FLAW_REVISION_WORK;
                    let ii = remaining_improvement_indices.remove(0);
                    if let Some(imp) = self.improvements.get_mut(ii) {
                        imp.actualized = true;
                        // Apply the improvement to the engine design
                        match &imp.kind {
                            ImprovementKind::Isp(frac) => {
                                self.design.isp_s *= 1.0 + frac;
                            }
                            ImprovementKind::Mass(frac) => {
                                self.design.mass_kg *= 1.0 - frac;
                            }
                            ImprovementKind::Thrust(frac) => {
                                self.design.thrust_n *= 1.0 + frac;
                            }
                        }
                        events.push(WorkEvent::ImprovementActualized {
                            description: format!("{}: {}", imp.description, imp.kind),
                        });
                    }
                }
                if remaining_flaw_indices.is_empty() && remaining_improvement_indices.is_empty() {
                    let leftover = *work_completed;
                    self.status = EngineDesignStatus::Testing { work_completed: leftover };
                }
            }
        }

        events
    }

    /// Start revising all discovered flaws and pending improvements.
    pub fn start_revision(&mut self) -> bool {
        if !matches!(self.status, EngineDesignStatus::Testing { .. }) {
            return false;
        }
        let flaw_indices: Vec<usize> = self.flaws.iter()
            .enumerate()
            .filter(|(_, f)| f.discovered)
            .map(|(i, _)| i)
            .collect();
        let improvement_indices: Vec<usize> = self.improvements.iter()
            .enumerate()
            .filter(|(_, imp)| !imp.actualized)
            .map(|(i, _)| i)
            .collect();
        if flaw_indices.is_empty() && improvement_indices.is_empty() {
            return false;
        }
        self.revision += 1;
        self.status = EngineDesignStatus::Revising {
            remaining_flaw_indices: flaw_indices,
            remaining_improvement_indices: improvement_indices,
            work_completed: 0.0,
        };
        true
    }

    /// Number of discovered flaws.
    pub fn discovered_flaw_count(&self) -> usize {
        self.flaws.iter().filter(|f| f.discovered).count()
    }

    /// Total number of flaws (hidden from player — for testing only).
    pub fn total_flaw_count(&self) -> usize {
        self.flaws.len()
    }

    /// Testing level description based on work completed in testing.
    pub fn testing_level(&self) -> &'static str {
        let total_testing_work = match &self.status {
            EngineDesignStatus::Testing { work_completed } => *work_completed,
            _ => 0.0,
        };
        // Count completed cycles (approximate from total work spent in testing)
        // This is simplified — in practice we'd track cumulative testing work
        let cycles = (total_testing_work / TESTING_CYCLE_WORK) as u32;
        match cycles {
            0 => "Untested",
            1..=2 => "Lightly Tested",
            3..=5 => "Moderately Tested",
            6..=9 => "Well Tested",
            _ => "Thoroughly Tested",
        }
    }
}

/// A potential improvement discovered during testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Improvement {
    pub description: String,
    pub kind: ImprovementKind,
    /// Whether this improvement has been actualized via revision.
    pub actualized: bool,
}

/// What an improvement affects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImprovementKind {
    /// Increase Isp by this fraction (e.g. 0.02 = +2%).
    Isp(f64),
    /// Reduce mass by this fraction (e.g. 0.03 = -3%).
    Mass(f64),
    /// Increase thrust by this fraction (e.g. 0.02 = +2%).
    Thrust(f64),
}

impl std::fmt::Display for ImprovementKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImprovementKind::Isp(frac) => write!(f, "+{:.0}% Isp", frac * 100.0),
            ImprovementKind::Mass(frac) => write!(f, "-{:.0}% mass", frac * 100.0),
            ImprovementKind::Thrust(frac) => write!(f, "+{:.0}% thrust", frac * 100.0),
        }
    }
}

/// Chance per testing cycle to discover an improvement.
const IMPROVEMENT_DISCOVERY_CHANCE: f64 = 0.08;

/// Generate a random improvement.
fn generate_improvement(rng: &mut StdRng) -> Improvement {
    let roll: f64 = rng.gen();
    let (kind, description) = if roll < 0.40 {
        let frac = rng.gen_range(0.01..0.04);
        (ImprovementKind::Isp(frac), match rng.gen_range(0u32..3) {
            0 => "Optimized injector pattern",
            1 => "Improved propellant mixing efficiency",
            _ => "Better nozzle contour",
        })
    } else if roll < 0.70 {
        let frac = rng.gen_range(0.02..0.06);
        (ImprovementKind::Mass(frac), match rng.gen_range(0u32..3) {
            0 => "Lighter turbopump housing",
            1 => "Thinner chamber wall design",
            _ => "Reduced gimbal mechanism mass",
        })
    } else {
        let frac = rng.gen_range(0.01..0.04);
        (ImprovementKind::Thrust(frac), match rng.gen_range(0u32..3) {
            0 => "Higher chamber pressure achievable",
            1 => "Improved injector throughput",
            _ => "Better regenerative cooling allows hotter burn",
        })
    };

    Improvement {
        description: description.to_string(),
        kind,
        actualized: false,
    }
}

/// Events generated by engine project work.
#[derive(Debug, Clone)]
pub enum WorkEvent {
    DesignComplete { flaw_count: u32 },
    TestingCycleComplete,
    FlawDiscovered { flaw_description: String },
    ImprovementDiscovered { description: String },
    RevisionComplete,
    ImprovementActualized { description: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn test_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    fn create_test_project() -> EngineProject {
        EngineProject::new(
            EngineProjectId(1),
            EngineId(1),
            "TestEngine".into(),
            EngineCycle::GasGenerator,
            PropellantPreset::Kerolox,
            1.0,
            true,
        ).unwrap()
    }

    #[test]
    fn test_baseline_exists_for_all_valid_combos() {
        for preset in PropellantPreset::ALL {
            for cycle in preset.compatible_cycles() {
                let b = engine_baseline(*cycle, *preset);
                assert!(b.is_some(), "Missing baseline for {:?}/{:?}", cycle, preset);
                let b = b.unwrap();
                assert!(b.thrust_n > 0.0);
                assert!(b.mass_kg > 0.0);
                assert!(b.isp_vac_s > 0.0);
                assert!(b.isp_sl_s > 0.0);
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
            EngineCycle::GasGenerator, PropellantPreset::Kerolox, 0.5, true,
        ).unwrap();
        let p2 = EngineProject::new(
            EngineProjectId(2), EngineId(2), "Big".into(),
            EngineCycle::GasGenerator, PropellantPreset::Kerolox, 2.0, true,
        ).unwrap();
        // Thrust and mass scale linearly
        assert!((p2.design.thrust_n / p1.design.thrust_n - 4.0).abs() < 0.01);
        assert!((p2.design.mass_kg / p1.design.mass_kg - 4.0).abs() < 0.01);
        // Isp doesn't change
        assert_eq!(p1.design.isp_s, p2.design.isp_s);
    }

    #[test]
    fn test_vacuum_vs_sea_level_isp() {
        let vac = EngineProject::new(
            EngineProjectId(1), EngineId(1), "Vac".into(),
            EngineCycle::GasGenerator, PropellantPreset::Kerolox, 1.0, true,
        ).unwrap();
        let sl = EngineProject::new(
            EngineProjectId(2), EngineId(2), "SL".into(),
            EngineCycle::GasGenerator, PropellantPreset::Kerolox, 1.0, false,
        ).unwrap();
        assert!(vac.design.isp_s > sl.design.isp_s);
    }

    #[test]
    fn test_design_completes_with_work() {
        let mut proj = create_test_project();
        proj.teams_assigned = 1;
        let mut rng = test_rng();
        let mut next_flaw_id = 0u64;

        let work_needed = match &proj.status {
            EngineDesignStatus::InDesign { work_required, .. } => *work_required,
            _ => panic!("should be InDesign"),
        };

        // Apply enough days
        let mut all_events = Vec::new();
        for _ in 0..(work_needed.ceil() as u32 + 1) {
            let events = proj.apply_daily_work(&mut rng, &mut next_flaw_id);
            all_events.extend(events);
        }

        // Should have completed design
        assert!(all_events.iter().any(|e| matches!(e, WorkEvent::DesignComplete { .. })));
        assert!(matches!(proj.status, EngineDesignStatus::Testing { .. }));
    }

    #[test]
    fn test_no_work_without_teams() {
        let mut proj = create_test_project();
        assert_eq!(proj.teams_assigned, 0);
        let mut rng = test_rng();
        let mut next_flaw_id = 0u64;

        for _ in 0..100 {
            let events = proj.apply_daily_work(&mut rng, &mut next_flaw_id);
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
        let mut id1 = 0u64;
        let mut id2 = 100u64;

        // After 10 days, proj2 should have more work done
        for _ in 0..10 {
            proj1.apply_daily_work(&mut rng1, &mut id1);
            proj2.apply_daily_work(&mut rng2, &mut id2);
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
        let mut next_flaw_id = 0u64;

        // Fast-forward to testing
        for _ in 0..300 {
            proj.apply_daily_work(&mut rng, &mut next_flaw_id);
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
            proj.apply_daily_work(&mut rng, &mut next_flaw_id);
        }

        assert_eq!(proj.flaws.len(), count_before - discovered_count);
        // Revision increments once per revision cycle, not per flaw
        assert_eq!(proj.revision, 1);
        assert!(matches!(proj.status, EngineDesignStatus::Testing { .. }));
    }

    #[test]
    fn test_testing_level() {
        let mut proj = create_test_project();
        proj.status = EngineDesignStatus::Testing { work_completed: 0.0 };
        assert_eq!(proj.testing_level(), "Untested");

        proj.status = EngineDesignStatus::Testing { work_completed: 60.0 };
        assert_eq!(proj.testing_level(), "Lightly Tested");

        proj.status = EngineDesignStatus::Testing { work_completed: 150.0 };
        assert_eq!(proj.testing_level(), "Moderately Tested");

        proj.status = EngineDesignStatus::Testing { work_completed: 250.0 };
        assert_eq!(proj.testing_level(), "Well Tested");

        proj.status = EngineDesignStatus::Testing { work_completed: 400.0 };
        assert_eq!(proj.testing_level(), "Thoroughly Tested");
    }

    #[test]
    fn test_hydrolox_higher_isp_than_kerolox() {
        let kero = engine_baseline(EngineCycle::GasGenerator, PropellantPreset::Kerolox).unwrap();
        let hydro = engine_baseline(EngineCycle::GasGenerator, PropellantPreset::Hydrolox).unwrap();
        assert!(hydro.isp_vac_s > kero.isp_vac_s);
    }

    #[test]
    fn test_complexity_stored_on_project() {
        let proj = create_test_project();
        // GG Kerolox: cycle=6, fuel=4 → max(6,4)=6
        assert_eq!(proj.complexity, 6);
    }
}
