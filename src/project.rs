//! The R&D project: one workflow shared by engine, rocket and reactor
//! designs.
//!
//! A project takes a design through `Proposed → InDesign → Testing →
//! Revising` (and back to Testing), accruing team-days of work, growing
//! flaws when the design completes, discovering them and the odd
//! improvement in testing, and burning them down in revisions. All of
//! that is the same for every kind of design; what differs — which flaw
//! pool a design draws from, whether it can be improved and how an
//! improvement lands on it, how a technology deficiency hits its stats —
//! is expressed once per design type through [`Designable`].
//!
//! `DesignProject<D>` is the workflow; `EngineDesign`, `RocketDesign`
//! and `ReactorDesign` are the `D`s. The kind-specific constructors and
//! editors live next to their design (`engine_project.rs` etc.) as
//! inherent impls on the alias.

use std::fmt;
use std::hash::Hash;

use rand::Rng;
use rand::rngs::StdRng;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::balance_config::{BalanceConfig, FlawsConfig};
use crate::flaw::{self, Flaw, FlawDomain, FlawId};
use crate::technology::{TechDeficiencyId, TechDeficiencyKind, TechnologyId};

/// Identity of an improvement within its project. Allocated from the
/// project's own `next_improvement_id`: an improvement never leaves the
/// project that discovered it, so the id only has to be unique there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImprovementId(pub u64);

/// Whether a technology deficiency's stat effect is being put onto a
/// design (at design completion) or taken back off it (when a revision
/// solves the deficiency). The two are exact inverses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Apply,
    Revert,
}

/// The three kinds of design a project can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectKind {
    Engine,
    Rocket,
    Reactor,
}

impl ProjectKind {
    /// Capitalised noun for event text and pane headings.
    pub fn label(&self) -> &'static str {
        match self {
            ProjectKind::Engine => "Engine",
            ProjectKind::Rocket => "Rocket",
            ProjectKind::Reactor => "Reactor",
        }
    }

    /// Lower-case noun for mid-sentence use.
    pub fn noun(&self) -> &'static str {
        match self {
            ProjectKind::Engine => "engine",
            ProjectKind::Rocket => "rocket",
            ProjectKind::Reactor => "reactor",
        }
    }
}

/// Where a project is in its workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DesignStatus {
    /// Tentative — sketched inside an in-progress designer session but
    /// not yet committed. Accrues no work and is hidden from the panes.
    /// Promoted to `InDesign` when the session is finalised; deleted if
    /// it is cancelled. Rockets never construct this today; it is the
    /// hook if rocket drafts ever work like engine drafts.
    Proposed { work_required: f64 },
    InDesign { work_completed: f64, work_required: f64 },
    Testing { work_completed: f64 },
    /// Working through discovered flaws, then pending improvements, then
    /// unsolved tech deficiencies, at a fixed cost each. Queues hold ids,
    /// not vec positions, so removing a flaw mid-revision can't shift
    /// what the rest of the queue points at.
    Revising {
        remaining_flaw_ids: Vec<FlawId>,
        #[serde(default)]
        remaining_improvement_ids: Vec<ImprovementId>,
        #[serde(default)]
        remaining_tech_deficiency_ids: Vec<TechDeficiencyId>,
        work_completed: f64,
    },
}

impl DesignStatus {
    /// Short phase name for status lines, editors and reports.
    pub fn label(&self) -> &'static str {
        match self {
            DesignStatus::Proposed { .. } => "Proposed",
            DesignStatus::InDesign { .. } => "In Design",
            DesignStatus::Testing { .. } => "Testing",
            DesignStatus::Revising { .. } => "Revising",
        }
    }
}

/// A potential improvement found in testing, actualised by a revision.
/// `K` says what it improves — Isp/mass/thrust on an engine, power/mass
/// on a reactor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Improvement<K> {
    pub id: ImprovementId,
    pub description: String,
    pub kind: K,
    /// Whether this improvement has been actualized via revision.
    pub actualized: bool,
}

/// Improvement kind for designs that have no improvements (rockets).
/// Uninhabited, so a `Vec<Improvement<Never>>` is always empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Never {}

impl fmt::Display for Never {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

/// Spec for designs whose project carries no parameters beyond the
/// design itself. Flattens to nothing in a save.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NoSpec {}

/// What one day of work on a project produced. The company's research
/// tick turns these into `GameEvent`s.
#[derive(Debug, Clone)]
pub enum WorkEvent {
    DesignComplete,
    TestingCycleComplete,
    FlawDiscovered { flaw_description: String },
    ImprovementDiscovered { description: String },
    ImprovementActualized { description: String },
    RevisionComplete,
    /// A tech deficiency revision was attempted — the game state
    /// resolves it against the technology table.
    TechDeficiencyAttempted { deficiency_id: TechDeficiencyId },
}

/// What a design has to say about itself for the shared workflow to run
/// on it. Where the per-kind character lives: flaw pool and prevalence,
/// improvements, technology.
pub trait Designable: Clone + fmt::Debug {
    /// The project id type for this kind.
    type Id: Copy + Eq + Hash + fmt::Debug + Serialize + DeserializeOwned;
    /// Player choices the project keeps beside the design so the design
    /// can be re-derived (engine: preset and scale). [`NoSpec`] if none.
    type Spec: Clone + fmt::Debug + Serialize + DeserializeOwned;
    /// What an improvement changes on this design. [`Never`] if the
    /// kind has no improvements.
    type ImprovementKind: Clone + fmt::Debug + fmt::Display + Serialize + DeserializeOwned;

    const KIND: ProjectKind;

    fn name(&self) -> &str;

    /// Which description pools flaws are worded from, and whether the
    /// design can have endurance flaws.
    fn flaw_domain(&self) -> FlawDomain;

    /// The complexity the flaw count is drawn from when the design
    /// completes. Engines fold in the propellants' problem factors;
    /// rockets and reactors use the project's stored complexity.
    fn flaw_complexity(&self, spec: &Self::Spec, project_complexity: u32) -> u32;

    /// Per-testing-cycle chance of finding an improvement, before the
    /// decay for improvements already found. `None` = this kind has no
    /// improvements and draws no roll.
    fn improvement_chance(cfg: &FlawsConfig) -> Option<f64>;

    /// Roll one improvement. Only called when `improvement_chance` is
    /// `Some`.
    fn roll_improvement(&self, rng: &mut StdRng, id: ImprovementId) -> Improvement<Self::ImprovementKind>;

    /// Land an actualised improvement on the design's stats.
    fn apply_improvement(&mut self, kind: &Self::ImprovementKind);

    /// Apply or revert one technology deficiency's stat effect.
    /// Complexity penalties are handled by the project, not here.
    fn apply_deficiency(&mut self, kind: &TechDeficiencyKind, dir: Direction);
}

/// A design and its workflow state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "D: Designable + Serialize + DeserializeOwned")]
pub struct DesignProject<D: Designable> {
    pub project_id: D::Id,
    pub design: D,
    /// Kind-specific parameters, flattened so the save layout is the
    /// same as when they were plain fields.
    #[serde(flatten)]
    pub spec: D::Spec,
    pub status: DesignStatus,
    pub flaws: Vec<Flaw>,
    pub revision: u32,
    pub teams_assigned: u32,
    pub complexity: u32,
    /// Cumulative engineering salary spent on this project (NRE).
    #[serde(default)]
    pub nre_cost: f64,
    /// Improvements discovered during testing. Pending ones need a
    /// revision to actualize.
    #[serde(default)]
    pub improvements: Vec<Improvement<D::ImprovementKind>>,
    /// Allocator for `ImprovementId` on this project.
    #[serde(default)]
    pub next_improvement_id: u64,
    /// Cumulative work spent in testing (persists across revisions).
    #[serde(default)]
    pub cumulative_testing_work: f64,
    /// IDs of unsolved tech deficiencies on this design (references
    /// `Technology.deficiencies`).
    #[serde(default)]
    pub tech_deficiency_ids: Vec<TechDeficiencyId>,
    /// Which technology this design uses (if experimental).
    #[serde(default)]
    pub technology_id: Option<TechnologyId>,
    /// Automatically start a revision as soon as testing discovers a
    /// flaw. Default on: for a project you aren't yet mass-producing,
    /// revising promptly is what you'd do anyway. Turn it off on a
    /// design with a production run going — a revision bumps
    /// `revision`, which flows onto build orders and inventory and so
    /// partially resets the learning curve.
    #[serde(default = "crate::flaw::auto_revise_default")]
    pub auto_revise: bool,
    /// Retired by the player — hidden from its pane and from pickers,
    /// but still present so every id that refers to it keeps resolving
    /// (stages of rocket designs, inventory, cost history, flights).
    /// Deleting the project outright would dangle all of those.
    #[serde(default)]
    pub retired: bool,
}

impl<D: Designable> DesignProject<D> {
    /// A fresh project entering `InDesign` with nothing done yet.
    pub fn new_in_design(
        project_id: D::Id,
        design: D,
        spec: D::Spec,
        complexity: u32,
        work_required: f64,
        technology_id: Option<TechnologyId>,
    ) -> Self {
        DesignProject {
            project_id,
            design,
            spec,
            status: DesignStatus::InDesign { work_completed: 0.0, work_required },
            flaws: Vec::new(),
            revision: 0,
            teams_assigned: 0,
            complexity,
            nre_cost: 0.0,
            improvements: Vec::new(),
            next_improvement_id: 0,
            cumulative_testing_work: 0.0,
            tech_deficiency_ids: Vec::new(),
            technology_id,
            auto_revise: crate::flaw::auto_revise_default(),
            retired: false,
        }
    }

    pub fn kind(&self) -> ProjectKind {
        D::KIND
    }

    pub fn is_proposed(&self) -> bool {
        matches!(self.status, DesignStatus::Proposed { .. })
    }

    /// Promote a `Proposed` project to `InDesign` with no work completed.
    /// No-op otherwise. Called when the parent designer session commits.
    pub fn promote_to_in_design(&mut self) {
        if let DesignStatus::Proposed { work_required } = self.status {
            self.status = DesignStatus::InDesign { work_completed: 0.0, work_required };
        }
    }

    /// After an edit changed `work_required`: store it, and clamp any
    /// progress so a now-cheaper design can't read as over-complete.
    /// Editors don't open on Testing, and Revising carries a fixed
    /// per-item cost rather than a total, so both keep their progress.
    pub fn clamp_work_after_edit(&mut self, work_required: f64) {
        match &mut self.status {
            DesignStatus::Proposed { work_required: wr } => *wr = work_required,
            DesignStatus::InDesign { work_completed, work_required: wr } => {
                *wr = work_required;
                if *work_completed > *wr {
                    *work_completed = *wr;
                }
            }
            DesignStatus::Testing { .. } | DesignStatus::Revising { .. } => {}
        }
    }

    /// Apply one day of work. Returns what it produced.
    ///
    /// Design completion generates the design's flaws. Each completed
    /// testing cycle rolls flaw discoveries and, for kinds that have
    /// them, one improvement roll (decaying with improvements already
    /// found — the low-hanging fruit runs out). Revision removes queued
    /// flaws, actualises queued improvements, and reports each queued
    /// tech-deficiency attempt for the game state to resolve, in that
    /// order and at `flaw_revision_work` each; when the queues are
    /// empty the project returns to Testing with its leftover progress.
    pub fn apply_daily_work(
        &mut self,
        rng: &mut StdRng,
        next_flaw_id: &mut u64,
        balance_cfg: &BalanceConfig,
    ) -> Vec<WorkEvent> {
        if self.teams_assigned == 0 {
            return Vec::new();
        }
        let work = crate::team::effective_work_rate(self.teams_assigned);
        let mut events = Vec::new();

        match &mut self.status {
            DesignStatus::Proposed { .. } => {
                // Tentative until the parent designer commits.
            }
            DesignStatus::InDesign { work_completed, work_required } => {
                *work_completed += work;
                if *work_completed >= *work_required {
                    let complexity = self.design.flaw_complexity(&self.spec, self.complexity);
                    self.flaws = flaw::generate_flaws(
                        self.design.flaw_domain(), complexity, rng, next_flaw_id, &balance_cfg.flaws,
                    );
                    self.status = DesignStatus::Testing { work_completed: 0.0 };
                    events.push(WorkEvent::DesignComplete);
                }
            }
            DesignStatus::Testing { work_completed } => {
                *work_completed += work;
                self.cumulative_testing_work += work;
                while *work_completed >= balance_cfg.work.testing_cycle_work {
                    *work_completed -= balance_cfg.work.testing_cycle_work;
                    let discovered = flaw::roll_discoveries_with_rng(&mut self.flaws, rng);
                    for idx in discovered {
                        events.push(WorkEvent::FlawDiscovered {
                            flaw_description: self.flaws[idx].description.clone(),
                        });
                    }
                    if let Some(base_chance) = D::improvement_chance(&balance_cfg.flaws) {
                        let chance = base_chance
                            * balance_cfg.flaws.improvement_decay.powi(self.improvements.len() as i32);
                        if rng.gen::<f64>() < chance {
                            let id = ImprovementId(self.next_improvement_id);
                            self.next_improvement_id += 1;
                            let improvement = self.design.roll_improvement(rng, id);
                            events.push(WorkEvent::ImprovementDiscovered {
                                description: format!("{}: {}", improvement.description, improvement.kind),
                            });
                            self.improvements.push(improvement);
                        }
                    }
                    events.push(WorkEvent::TestingCycleComplete);
                }
            }
            DesignStatus::Revising {
                remaining_flaw_ids,
                remaining_improvement_ids,
                remaining_tech_deficiency_ids,
                work_completed,
            } => {
                *work_completed += work;
                let cost = balance_cfg.work.flaw_revision_work;
                // Flaws first.
                while *work_completed >= cost && !remaining_flaw_ids.is_empty() {
                    *work_completed -= cost;
                    let fid = remaining_flaw_ids.remove(0);
                    self.flaws.retain(|f| f.id != fid);
                    events.push(WorkEvent::RevisionComplete);
                }
                // Then actualise improvements.
                while *work_completed >= cost && !remaining_improvement_ids.is_empty() {
                    *work_completed -= cost;
                    let iid = remaining_improvement_ids.remove(0);
                    if let Some(imp) = self.improvements.iter_mut().find(|imp| imp.id == iid) {
                        imp.actualized = true;
                        self.design.apply_improvement(&imp.kind);
                        events.push(WorkEvent::ImprovementActualized {
                            description: format!("{}: {}", imp.description, imp.kind),
                        });
                    }
                }
                // Then attempt tech deficiency fixes (resolved by the game state).
                while *work_completed >= cost && !remaining_tech_deficiency_ids.is_empty() {
                    *work_completed -= cost;
                    let def_id = remaining_tech_deficiency_ids.remove(0);
                    events.push(WorkEvent::TechDeficiencyAttempted { deficiency_id: def_id });
                }
                if remaining_flaw_ids.is_empty()
                    && remaining_improvement_ids.is_empty()
                    && remaining_tech_deficiency_ids.is_empty()
                {
                    let leftover = *work_completed;
                    self.status = DesignStatus::Testing { work_completed: leftover };
                }
            }
        }

        events
    }

    /// Start revising every discovered flaw, pending improvement and
    /// unsolved tech deficiency. Testing-only; false if not in Testing
    /// or there is nothing to revise.
    pub fn start_revision(&mut self) -> bool {
        if !matches!(self.status, DesignStatus::Testing { .. }) {
            return false;
        }
        let flaw_ids: Vec<FlawId> = self.flaws.iter()
            .filter(|f| f.discovered)
            .map(|f| f.id)
            .collect();
        let improvement_ids: Vec<ImprovementId> = self.improvements.iter()
            .filter(|imp| !imp.actualized)
            .map(|imp| imp.id)
            .collect();
        let tech_def_ids = self.tech_deficiency_ids.clone();
        if flaw_ids.is_empty() && improvement_ids.is_empty() && tech_def_ids.is_empty() {
            return false;
        }
        self.revision += 1;
        self.status = DesignStatus::Revising {
            remaining_flaw_ids: flaw_ids,
            remaining_improvement_ids: improvement_ids,
            remaining_tech_deficiency_ids: tech_def_ids,
            work_completed: 0.0,
        };
        true
    }

    /// Number of discovered flaws.
    pub fn discovered_flaw_count(&self) -> usize {
        self.flaws.iter().filter(|f| f.discovered).count()
    }

    /// Total number of flaws, hidden ones included (for tests and reports).
    pub fn total_flaw_count(&self) -> usize {
        self.flaws.len()
    }

    /// Improvements found but not yet actualised.
    pub fn pending_improvement_count(&self) -> usize {
        self.improvements.iter().filter(|imp| !imp.actualized).count()
    }

    /// Testing level description based on cumulative work in testing.
    pub fn testing_level(&self, balance_cfg: &BalanceConfig) -> &'static str {
        let cycles = (self.cumulative_testing_work / balance_cfg.work.testing_cycle_work) as u32;
        match cycles {
            0 => "Untested",
            1..=2 => "Lightly Tested",
            3..=5 => "Moderately Tested",
            6..=9 => "Well Tested",
            _ => "Thoroughly Tested",
        }
    }
}
