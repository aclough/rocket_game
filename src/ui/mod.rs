pub mod draw;

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen,
    disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;

use crate::engine::{EngineCycle, EngineDesign};
use crate::engine_project::{EngineDesignStatus, EngineSource, PropellantPreset};
use crate::game_state::{GameSpeed, GameState};
use crate::location::DELTA_V_MAP;
use crate::rocket_project::RocketDesignStatus;
use crate::save;
use crate::stage::{Stage, StageId};
use crate::structure;

/// Which pane has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPane {
    Sidebar,
    Content,
}

/// Available tabs in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Engines,
    Reactors,
    Rockets,
    Manufacturing,
    Contracts,
    Launches,
    Finance,
    Events,
}

impl Tab {
    pub const ALL: &[Tab] = &[
        Tab::Overview, Tab::Engines, Tab::Reactors,
        Tab::Rockets, Tab::Manufacturing, Tab::Contracts,
        Tab::Launches, Tab::Finance, Tab::Events,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Engines => "Engines",
            Tab::Reactors => "Reactors",
            Tab::Rockets => "Rockets",
            Tab::Manufacturing => "Mfg",
            Tab::Contracts => "Contracts",
            Tab::Launches => "Launches",
            Tab::Finance => "Finance",
            Tab::Events => "Events",
        }
    }

    /// Whether this tab uses a list-style selection (vs scrollable content).
    pub fn is_list_tab(&self) -> bool {
        matches!(self, Tab::Engines | Tab::Reactors | Tab::Rockets
            | Tab::Manufacturing | Tab::Contracts | Tab::Launches)
    }
}

/// Whether the rocket designer is creating a brand-new design or
/// modifying an existing project (post-Phase-3 tankage / power tweaks
/// to a rocket the player has already started building).
#[derive(Debug, Clone)]
pub enum DesignerMode {
    New,
    Modify {
        project_id: crate::rocket_project::RocketProjectId,
    },
}

/// Shared state for the rocket designer screen.
#[derive(Debug, Clone)]
pub struct RocketDesignerState {
    pub mode: DesignerMode,
    pub rocket_name: String,
    /// Stages, grouped — `stage_groups[gi][si]` is the `si`-th stage of
    /// group `gi`. **Index-aligned with `engine_sources`** (same outer
    /// and inner shape). When adding, removing, or replacing stages,
    /// always go through the lockstep methods on this struct
    /// (`push_new_group`, `push_to_group`, `insert_new_group_at`,
    /// `replace_stage`, `remove_group`, `remove_inner`) so the source
    /// list stays in sync. Field-level mutation of an existing Stage
    /// (engine count, propellant, power) is fine via direct access.
    pub stage_groups: Vec<Vec<Stage>>,
    /// Where each stage's engine came from (player project /
    /// contracted). Mirrors the shape of `stage_groups` — see the
    /// invariant note there.
    pub engine_sources: Vec<Vec<EngineSource>>,
    pub next_stage_id: u64,
    pub selected_group: usize,
    pub selected_inner: usize,
    pub payload_kg: f64,
    pub launch_from: &'static str,
    /// Reference-trajectory destination for live feasibility readout.
    /// Always set (defaults to LEO); the design-time mission scratchpad
    /// only displays the route — the destination isn't carried onto the
    /// resulting RocketProject.
    pub destination: &'static str,
    /// EngineProject ids that this designer session created (always in
    /// `Proposed` status). Used to clean up Proposed engines if the
    /// designer is cancelled, and to promote them to `InDesign` when
    /// the rocket is committed.
    pub created_engine_projects: Vec<crate::engine_project::EngineProjectId>,
}

impl RocketDesignerState {
    fn new(name: String) -> Self {
        Self {
            mode: DesignerMode::New,
            rocket_name: name,
            stage_groups: Vec::new(),
            engine_sources: Vec::new(),
            next_stage_id: 1,
            selected_group: 0,
            selected_inner: 0,
            payload_kg: 1000.0,
            launch_from: "earth_surface",
            destination: "leo",
            created_engine_projects: Vec::new(),
        }
    }

    /// Open the designer in `Modify` mode against an existing rocket
    /// project — pre-fills stages, name, and the mission scratchpad
    /// fields. EngineSources are recovered from the engine_id on each
    /// stage by looking up the company's engine roster.
    pub fn from_existing(
        project: &crate::rocket_project::RocketProject,
        company: &crate::game_state::Company,
    ) -> Self {
        let stage_groups = project.design.stage_groups.clone();
        let max_id = stage_groups.iter().flatten()
            .map(|s| s.id.0).max().unwrap_or(0);
        let engine_sources: Vec<Vec<EngineSource>> = stage_groups.iter()
            .map(|group| group.iter()
                .map(|stage| company.engine_source_for_id(stage.engine.id)
                    .unwrap_or(EngineSource::PlayerDesign(
                        crate::engine_project::EngineProjectId(0))))
                .collect())
            .collect();
        Self {
            mode: DesignerMode::Modify { project_id: project.project_id },
            rocket_name: project.design.name.clone(),
            stage_groups,
            engine_sources,
            next_stage_id: max_id + 1,
            selected_group: 0,
            selected_inner: 0,
            payload_kg: 1000.0,
            launch_from: "earth_surface",
            destination: "leo",
            created_engine_projects: Vec::new(),
        }
    }

    /// True when the designer is in Modify mode.
    pub fn is_modify(&self) -> bool {
        matches!(self.mode, DesignerMode::Modify { .. })
    }

    /// Total number of individual stages across all groups.
    fn total_stages(&self) -> usize {
        self.stage_groups.iter().map(|g| g.len()).sum()
    }

    /// True if any stage in the design uses a low-thrust engine.
    /// Low-thrust engines (ion drives) are restricted to single-stage
    /// designs — booster duty is handled by carrying the ion rocket as a
    /// payload on a separate chemical rocket.
    fn has_low_thrust_stage(&self) -> bool {
        self.stage_groups.iter().flatten()
            .any(|s| s.engine.is_low_thrust())
    }

    /// Whether the selection cursor is on the "add stage" slot.
    fn on_add_slot(&self) -> bool {
        self.selected_group >= self.stage_groups.len()
    }

    /// Flat index of the current selection (0-based across all inner stages).
    fn flat_index(&self) -> usize {
        let mut idx = 0;
        for gi in 0..self.selected_group.min(self.stage_groups.len()) {
            idx += self.stage_groups[gi].len();
        }
        if !self.on_add_slot() {
            idx += self.selected_inner;
        }
        idx
    }

    /// Set selection from a flat index. If flat >= total_stages(), selects add slot.
    fn select_flat(&mut self, flat: usize) {
        let total = self.total_stages();
        if flat >= total {
            self.selected_group = self.stage_groups.len();
            self.selected_inner = 0;
            return;
        }
        let mut remaining = flat;
        for (gi, group) in self.stage_groups.iter().enumerate() {
            if remaining < group.len() {
                self.selected_group = gi;
                self.selected_inner = remaining;
                return;
            }
            remaining -= group.len();
        }
    }

    /// Generate stage name for a stage at (group_index, inner_index).
    fn stage_name(group_index: usize, inner_index: usize, group_len: usize) -> String {
        if group_len == 1 {
            format!("S{}", group_index + 1)
        } else {
            let suffix = (b'a' + inner_index as u8) as char;
            format!("S{}{}", group_index + 1, suffix)
        }
    }

    // ── Lockstep mutators ────────────────────────────────────────────
    //
    // `stage_groups` and `engine_sources` are kept index-aligned (same
    // outer/inner shape) — every Stage in `stage_groups[gi][si]` has its
    // EngineSource at `engine_sources[gi][si]`. The methods below are
    // the *only* sites that change the cardinality of either Vec; read
    // access can use the fields directly. New mutation paths must use
    // these methods (or grow new ones that update both lists atomically).

    /// Append a new singleton group at the end of the layout.
    pub fn push_new_group(&mut self, stage: Stage, source: EngineSource) {
        self.stage_groups.push(vec![stage]);
        self.engine_sources.push(vec![source]);
    }

    /// Append a stage to an existing group (used for boosters).
    pub fn push_to_group(&mut self, gi: usize, stage: Stage, source: EngineSource) {
        self.stage_groups[gi].push(stage);
        self.engine_sources[gi].push(source);
    }

    /// Insert a new singleton group at position `gi`, shifting later
    /// groups down.
    pub fn insert_new_group_at(&mut self, gi: usize, stage: Stage, source: EngineSource) {
        self.stage_groups.insert(gi, vec![stage]);
        self.engine_sources.insert(gi, vec![source]);
    }

    /// Replace an existing stage's contents in place.
    pub fn replace_stage(&mut self, gi: usize, si: usize, stage: Stage, source: EngineSource) {
        self.stage_groups[gi][si] = stage;
        self.engine_sources[gi][si] = source;
    }

    /// Remove an entire group.
    pub fn remove_group(&mut self, gi: usize) {
        self.stage_groups.remove(gi);
        self.engine_sources.remove(gi);
    }

    /// Remove a single inner stage from a group.
    pub fn remove_inner(&mut self, gi: usize, si: usize) {
        self.stage_groups[gi].remove(si);
        self.engine_sources[gi].remove(si);
    }
}

/// Whether an engine uses solid propellant (propellant is not adjustable).
fn is_solid_engine(engine: &EngineDesign) -> bool {
    engine.propellant_mix.len() == 1
        && engine.propellant_mix[0].propellant == crate::propellant::Propellant::SolidMix
}

/// Burn-time used as a default when a stage is first created — the
/// initial propellant load is sized for this many seconds of full-thrust
/// firing. Easy starting point that the player can grow with `+`.
const NEW_STAGE_BURN_SECONDS: f64 = 120.0;
/// Step size for inline propellant adjustments (`+`/`-` in the
/// designer): ~10 seconds of burn time per press.
const PROPELLANT_STEP_BURN_SECONDS: f64 = 10.0;

/// Compute thrust-scaled propellant step size for inline adjustments.
/// Rounded to nearest 100 kg, min 100 kg.
fn propellant_step(engine: &EngineDesign, engine_count: u32) -> f64 {
    let raw = engine.mass_flow_rate() * engine_count as f64 * PROPELLANT_STEP_BURN_SECONDS;
    (raw / 100.0).round().max(1.0) * 100.0
}

/// Recompute structural masses for all stage groups based on their position.
/// Aero shell depends on being group 0 (exposed to airflow).
/// Interstage depends on whether the stage is the last group.
/// Refresh every player-designed stage engine from its source engine
/// project, then re-derive the per-stage propellant mass (using the
/// same `NEW_STAGE_BURN_SECONDS` formula as the engine picker) and
/// recompute structural masses (which depend on engine mass). Call
/// after any engine-editor mutation that touched a project's
/// `EngineDesign` so the rocket designer's thrust / Isp / power_draw_w
/// — and the propellant load that determines burn time — don't go
/// stale against the project's current numbers.
///
/// Mirrors the lockstep invariant: `stage_groups[gi][si]` and
/// `engine_sources[gi][si]` always describe the same stage, so we walk
/// them in parallel.
fn sync_stages_to_projects(state: &mut RocketDesignerState, company: &crate::game_state::Company) {
    for (group, sources) in state.stage_groups.iter_mut()
        .zip(state.engine_sources.iter())
    {
        for (stage, source) in group.iter_mut().zip(sources.iter()) {
            if let EngineSource::PlayerDesign(pid) = source {
                if let Some(ep) = company.engine_projects.iter()
                    .find(|ep| ep.project_id == *pid)
                {
                    stage.engine = ep.design.clone();
                    // Re-derive propellant mass the same way the engine
                    // picker does for a fresh stage, so swapping cycle
                    // (e.g. Kerolox → Ion) doesn't strand a kerolox-
                    // sized tank on an ion engine.
                    stage.propellant_mass_kg = stage.engine.mass_flow_rate()
                        * stage.engine_count as f64
                        * NEW_STAGE_BURN_SECONDS;
                }
            }
        }
    }
    recompute_structural_masses(&mut state.stage_groups);
}

fn recompute_structural_masses(stage_groups: &mut [Vec<Stage>]) {
    let n = stage_groups.len();
    for (gi, group) in stage_groups.iter_mut().enumerate() {
        let is_first = gi == 0;
        let has_interstage = gi + 1 < n;
        for stage in group.iter_mut() {
            let propellant_mix: Vec<(crate::propellant::Propellant, f64)> =
                stage.engine.propellant_mix.iter()
                    .map(|f| (f.propellant, f.mass_fraction))
                    .collect();
            let breakdown = structure::compute_structural_mass(
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

/// Modal input state for new engine design flow.
#[derive(Debug, Clone)]
pub enum InputMode {
    Normal,
    /// Non-linear engine editor — edits an existing EngineProject in
    /// place. The cursor walks a fixed field list; each field has its
    /// own interaction (Left/Right cycles, +/- adjusts scale, Enter
    /// opens a text sub-modal for name or scale). Only opened from
    /// inside the rocket designer; the designer state travels with the
    /// editor and is restored on Esc.
    EngineEditor {
        project_id: crate::engine_project::EngineProjectId,
        cursor: usize,
        state: Box<RocketDesignerState>,
    },
    /// Text-input sub-modal for the engine name.
    EngineEditorNameInput {
        project_id: crate::engine_project::EngineProjectId,
        cursor: usize,
        buffer: String,
        state: Box<RocketDesignerState>,
    },
    /// Numeric sub-modal for the engine scale.
    EngineEditorScaleInput {
        project_id: crate::engine_project::EngineProjectId,
        cursor: usize,
        buffer: String,
        state: Box<RocketDesignerState>,
    },
    /// Standalone reactor editor — opened from the Reactors pane.
    /// Operates on an existing `ReactorProject` by id; the new-design
    /// flow seeds a `Proposed` project first so cancelling can delete it
    /// cleanly. Cursor: 0 = name, 1 = scale.
    ReactorEditor {
        project_id: crate::reactor_project::ReactorProjectId,
        cursor: usize,
    },
    ReactorEditorNameInput {
        project_id: crate::reactor_project::ReactorProjectId,
        cursor: usize,
        buffer: String,
    },
    ReactorEditorScaleInput {
        project_id: crate::reactor_project::ReactorProjectId,
        cursor: usize,
        buffer: String,
    },
    /// Selecting from third-party catalog.
    SelectThirdParty { selected: usize },
    /// Typing rocket name.
    RocketName { buffer: String },
    /// Persistent rocket designer screen.
    RocketDesigner { state: Box<RocketDesignerState> },
    /// Per-stage power-source editor opened from the rocket designer.
    /// Cursor walks a merged list of "equipped sources" then "presets to
    /// add"; Space adds a preset, X/Del removes an equipped source, Esc
    /// returns to the designer.
    PowerEditor {
        state: Box<RocketDesignerState>,
        group_index: usize,
        stage_index: usize,
        cursor: usize,
    },
    /// Picking an engine for a new or replacement stage.
    RocketPickEngine {
        state: Box<RocketDesignerState>,
        target_index: Option<usize>,   // group index
        inner_index: Option<usize>,    // inner stage index (for editing specific stage)
        editing: bool,
        booster: bool,                 // adding parallel stage to existing group
        selected: usize,
    },
    /// Typing a payload mass (kg).
    RocketPayloadInput {
        state: Box<RocketDesignerState>,
        buffer: String,
    },
    /// Picking a launch site or mission destination for the rocket
    /// designer. The picker lists every location in the delta-v map;
    /// `target` controls which RocketDesignerState field is updated on
    /// confirm.
    RocketDesignerLocationPicker {
        state: Box<RocketDesignerState>,
        target: LocationPickerTarget,
        locations: Vec<(&'static str, &'static str)>,
        selected: usize,
    },
    /// Building the launch manifest: pick contracts and/or inventory
    /// rockets (as Spacecraft payloads) to fly together. Empty manifest =
    /// test launch to LEO.
    LaunchManifest {
        rocket_item_id: crate::manufacturing::InventoryItemId,
        persist: bool,
        /// Parallel to `player_company.active_contracts`.
        contract_picks: Vec<bool>,
        /// Parallel to inventory rockets *excluding* the carrier (the
        /// rocket whose `rocket_item_id` matches the carrier's). The UI
        /// rebuilds this list to skip the carrier so the player can't pick
        /// it as its own payload.
        spacecraft_picks: Vec<bool>,
        /// Item ids for the rockets in `spacecraft_picks`, in order.
        spacecraft_item_ids: Vec<crate::manufacturing::InventoryItemId>,
        /// Row in the merged manifest (contracts then spacecraft).
        cursor: usize,
    },
    /// Showing launch result.
    LaunchResult {
        record: crate::launch::LaunchRecord,
    },
    /// Selecting which spacecraft to fly.
    FlySelectSpacecraft {
        selected: usize,
    },
    /// Step 1 of docking: pick the spacecraft that will become a payload.
    DockSelectSmall {
        selected: usize,
    },
    /// Step 2 of docking: pick the carrier to dock onto. `candidates` is
    /// the list of fleet indices (other spacecraft at the same location
    /// as the small one).
    DockSelectLarge {
        small_idx: usize,
        candidates: Vec<usize>,
        selected: usize,
    },
    /// Step 1 of undocking: pick which carrier to remove a payload from.
    /// Only carriers with at least one Spacecraft payload are considered.
    UndockSelectCarrier {
        candidates: Vec<usize>,
        selected: usize,
    },
    /// Step 2 of undocking: pick which payload (by index into
    /// carrier.payloads) to release. Only `Payload::Spacecraft` items
    /// are surfaced.
    UndockSelectPayload {
        carrier_idx: usize,
        payload_indices: Vec<usize>,
        selected: usize,
    },
    /// Selecting destination for a spacecraft flight.
    FlySelectDestination {
        spacecraft_index: usize,
        destinations: Vec<(String, String, f64)>, // (location_id, display_name, dv_cost)
        remaining_dv: f64,
        selected: usize,
    },
    /// Delta-v planner setup — choose design, payload, start location.
    PlannerSetup {
        state: Box<PlannerSetupState>,
    },
    /// Delta-v planner.
    DvPlanner {
        state: Box<DvPlannerState>,
    },
}

/// Which RocketDesignerState field a location picker should update.
#[derive(Debug, Clone, Copy)]
pub enum LocationPickerTarget {
    LaunchSite,
    MissionDestination,
}

/// An action in the delta-v planner.
#[derive(Debug, Clone)]
pub enum PlanAction {
    Leg { from: String, to: String, to_display: String, dv_cost: f64 },
    DropPayload { mass_dropped: f64 },
}

/// Source for the delta-v planner.
#[derive(Debug, Clone)]
pub enum PlannerSource {
    Design { project_index: usize },
    Spacecraft { spacecraft_index: usize },
}

/// Which field is active in the planner setup.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlannerSetupField {
    Design,
    Payload,
    Location,
}

/// State for the planner setup modal.
#[derive(Debug, Clone)]
pub struct PlannerSetupState {
    /// Indices of rocket projects that are in Testing status.
    pub eligible_projects: Vec<usize>,
    pub selected_project: usize,
    /// All locations from the delta-v map.
    pub locations: Vec<(&'static str, &'static str)>, // (id, display_name)
    pub selected_location: usize,
    pub payload_buffer: String,
    pub active_field: PlannerSetupField,
}

/// State for the delta-v planner modal.
#[derive(Debug, Clone)]
pub struct DvPlannerState {
    pub source: PlannerSource,
    pub rocket: crate::rocket::Rocket,
    pub design: crate::rocket::RocketDesign,
    pub current_location: String,
    pub actions: Vec<PlanAction>,
    /// Rocket snapshots before each action (for undo).
    pub snapshots: Vec<(crate::rocket::Rocket, f64)>, // (rocket_state, payload_at_that_point)
    /// Reachable destinations from current location.
    pub destinations: Vec<(String, String, f64)>, // (id, display, dv_cost)
    pub selected: usize,
    pub payload_kg: f64,
}

/// Application state wrapping the game and UI concerns.
pub struct App {
    pub game: GameState,
    pub running: bool,
    pub active_tab: usize,
    pub focused_pane: FocusedPane,
    pub content_scroll: usize,
    pub status_message: Option<String>,
    pub input_mode: InputMode,
    /// Selected item index in content pane (for engines list).
    pub selected_item: usize,
    /// Speed before entering a modal, so we can restore on exit.
    pub pre_modal_speed: Option<GameSpeed>,
}

/// Compute reachable destinations using the stage-aware path planner.
///
/// When `rocket` and `design` are both provided, plans from the spacecraft's
/// current state (partially-used stages, jettisoned lower stages) so the
/// reachability and dv numbers reflect what the rocket can still do.
/// Otherwise falls back to the abstract single-stage planner using
/// `rocket_mass`.
/// Rename every stage based on its position in `stage_groups`, using
/// the project's S1 / S1a / S1b conventions.
fn rename_all_stages(stage_groups: &mut [Vec<Stage>]) {
    for (gi, group) in stage_groups.iter_mut().enumerate() {
        let glen = group.len();
        for (si, stage) in group.iter_mut().enumerate() {
            stage.name = RocketDesignerState::stage_name(gi, si, glen);
        }
    }
}

/// Apply a picked engine to the rocket designer state — either by
/// editing an existing stage or by inserting a new one in the right
/// position. Renames stages and recomputes structural masses.
fn apply_picked_engine_to_designer(
    state: &mut RocketDesignerState,
    source: EngineSource,
    engine: EngineDesign,
    target_index: Option<usize>,
    inner_index: Option<usize>,
    editing: bool,
    booster: bool,
) {
    let engine_count = 1u32;
    let propellant_mass_kg = engine.mass_flow_rate() * engine_count as f64 * NEW_STAGE_BURN_SECONDS;
    let stage = Stage {
        id: StageId(state.next_stage_id),
        name: String::new(),
        engine,
        engine_count,
        propellant_mass_kg,
        structural_mass_kg: 0.0,
        fairing: None,
        power_sources: Vec::new(),
    };
    state.next_stage_id += 1;

    match (editing, booster, inner_index, target_index) {
        (true, _, Some(ii), Some(gi)) => {
            state.replace_stage(gi, ii, stage, source);
            state.selected_group = gi;
            state.selected_inner = ii;
        }
        (false, true, _, Some(gi)) => {
            state.push_to_group(gi, stage, source);
            state.selected_group = gi;
            state.selected_inner = state.stage_groups[gi].len() - 1;
        }
        (false, false, _, Some(gi)) => {
            state.insert_new_group_at(gi, stage, source);
            state.selected_group = gi;
            state.selected_inner = 0;
        }
        (false, false, _, None) => {
            state.push_new_group(stage, source);
            state.selected_group = state.stage_groups.len() - 1;
            state.selected_inner = 0;
        }
        _ => {}
    }

    rename_all_stages(&mut state.stage_groups);
    recompute_structural_masses(&mut state.stage_groups);
}

/// Engine cycles available to the player based on unlocked tech.
fn available_engine_cycles(game: &GameState) -> Vec<EngineCycle> {
    let mut cycles = vec![
        EngineCycle::PressureFed,
        EngineCycle::GasGenerator,
        EngineCycle::Expander,
        EngineCycle::StagedCombustion,
        EngineCycle::FullFlow,
    ];
    if game.technologies.iter().any(|t|
        t.id == crate::technology::TECH_NUCLEAR_THERMAL && t.unlocked
    ) {
        cycles.push(EngineCycle::NuclearThermal);
    }
    cycles.push(EngineCycle::ElectricPropulsion);
    cycles.push(EngineCycle::SolarSail);
    cycles
}

/// Step through a slice of values, wrapping at either end. Direction
/// is forward when `forward` is true, backward otherwise.
fn wrap_cycle<T: Copy + PartialEq>(values: &[T], current: T, forward: bool) -> Option<T> {
    let i = values.iter().position(|v| *v == current)?;
    let n = values.len();
    if n == 0 { return None; }
    let next = if forward { (i + 1) % n } else { (i + n - 1) % n };
    Some(values[next])
}

fn reachable_destinations_multistage(
    from: &str, remaining_dv: f64, rocket_mass: f64, _low_thrust: bool,
    rocket: Option<&crate::rocket::Rocket>,
    design: Option<&crate::rocket::RocketDesign>,
) -> Vec<(String, String, f64)> {
    let map = &crate::location::DELTA_V_MAP;
    let mut dests = Vec::new();

    for loc in map.locations() {
        if loc.id == from {
            continue;
        }

        let path = if let (Some(rocket), Some(design)) = (rocket, design) {
            map.shortest_path_for_rocket_state(from, loc.id, design, rocket)
        } else {
            // No rocket state — fall back to the abstract Dijkstra so the
            // UI can still surface destinations for empty/imaginary rockets.
            map.shortest_path(from, loc.id, rocket_mass)
        };

        if let Some((_, dv)) = path {
            if dv <= remaining_dv {
                dests.push((loc.id.to_string(), loc.display_name.to_string(), dv));
            }
        }
    }
    dests.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
    dests
}

impl App {
    pub fn new(game: GameState) -> Self {
        App {
            game,
            running: true,
            active_tab: 0,
            focused_pane: FocusedPane::Sidebar,
            content_scroll: 0,
            status_message: None,
            input_mode: InputMode::Normal,
            selected_item: 0,
            pre_modal_speed: None,
        }
    }

    /// Save current speed and pause the game when entering a modal.
    fn enter_modal(&mut self, mode: InputMode) {
        self.pre_modal_speed = Some(self.game.speed);
        self.game.speed = GameSpeed::Paused;
        self.input_mode = mode;
    }

    /// Restore the speed saved before entering a modal.
    fn exit_modal(&mut self) {
        self.input_mode = InputMode::Normal;
        if let Some(s) = self.pre_modal_speed.take() {
            self.game.speed = s;
        }
    }

    /// Assemble the launch manifest from the user's checks and submit it.
    /// All picked contracts must share a destination; the destination of
    /// the carrier flight is that shared destination (or LEO if the only
    /// picks are spacecraft / nothing). Spacecraft payloads are taken from
    /// inventory at submit time and packed with `deploy_at = destination`.
    fn submit_manifest_launch(
        &mut self,
        rocket_item_id: crate::manufacturing::InventoryItemId,
        persist: bool,
        contract_picks: Vec<bool>,
        spacecraft_picks: Vec<bool>,
        spacecraft_item_ids: Vec<crate::manufacturing::InventoryItemId>,
    ) {
        use crate::flight::Payload;

        // Determine destination from picked contracts (must agree). If no
        // contracts picked, default to LEO.
        let mut destination: Option<String> = None;
        for (i, picked) in contract_picks.iter().enumerate() {
            if !picked { continue; }
            let dest = self.game.player_company.active_contracts[i].destination.clone();
            match &destination {
                None => destination = Some(dest),
                Some(d) if d == &dest => {}
                Some(d) => {
                    self.status_message = Some(format!(
                        "Picked contracts have different destinations ({} vs {}). Untoggle one.",
                        d, dest,
                    ));
                    return;
                }
            }
        }
        let destination = destination.unwrap_or_else(|| "leo".to_string());

        // Build contract-delivery payloads.
        let mut payloads: Vec<Payload> = Vec::new();
        for (i, picked) in contract_picks.iter().enumerate() {
            if !picked { continue; }
            let c = &self.game.player_company.active_contracts[i];
            payloads.push(Payload::ContractDelivery {
                contract_id: c.id,
                payload_kg: c.payload_kg,
            });
        }

        // Take picked inventory rockets and pack as Spacecraft payloads.
        // We take in reverse so removing items from the inventory Vec
        // doesn't shift earlier indices (we look up by item_id, not index,
        // but extra-safe).
        for (i, picked) in spacecraft_picks.iter().enumerate() {
            if !picked { continue; }
            let item_id = spacecraft_item_ids[i];
            let inv_rocket = match self.game.player_company.manufacturing.inventory
                .take_rocket(item_id)
            {
                Some(r) => r,
                None => {
                    self.status_message = Some("Spacecraft payload no longer in inventory.".into());
                    return;
                }
            };
            // Pull the project's design and instantiate a fresh Rocket
            // with full propellant. Nested payload mass is 0 for now (no
            // recursive picking in this UI).
            let rp = self.game.player_company.rocket_projects.iter()
                .find(|rp| rp.project_id == inv_rocket.rocket_project_id);
            if rp.is_none() {
                self.status_message = Some("Payload rocket project not found.".into());
                return;
            }
            let design = rp.unwrap().design.clone();
            let rocket_id = crate::rocket::RocketId(self.game.next_rocket_id);
            self.game.next_rocket_id += 1;
            let rocket = design.instantiate(rocket_id, "earth_surface", 0.0);
            payloads.push(Payload::Spacecraft {
                deploy_at: Some(destination.clone()),
                design,
                rocket,
                nested_payloads: vec![],
                rocket_project_id: inv_rocket.rocket_project_id,
                name: inv_rocket.rocket_name.clone(),
            });
        }

        // No picks → test launch with zero mass.
        if payloads.is_empty() {
            payloads.push(Payload::TestMass { mass_kg: 0.0 });
        }

        match self.game.launch_rocket(rocket_item_id, &destination, payloads, persist) {
            Some((_events, Some(record))) => {
                self.input_mode = InputMode::LaunchResult { record };
            }
            Some((_events, None)) => {
                self.status_message = Some("Flight departed — in transit".into());
                self.exit_modal();
            }
            None => {
                self.status_message = Some("Launch failed (rocket not found)".into());
                self.exit_modal();
            }
        }
    }

    pub fn current_tab(&self) -> Tab {
        Tab::ALL[self.active_tab]
    }

    /// Run the main application loop.
    pub fn run(&mut self) -> io::Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.main_loop(&mut terminal);

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }

    fn main_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
        let mut last_tick = Instant::now();

        while self.running {
            terminal.draw(|frame| draw::draw(frame, self))?;

            let tick_rate = if self.game.speed == GameSpeed::Paused {
                Duration::from_millis(100) // Still responsive to input when paused
            } else {
                Duration::from_millis(self.game.speed.tick_ms())
            };

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());

            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key.code);
                    }
                }
            }

            // Auto-advance when not paused
            if self.game.speed != GameSpeed::Paused && last_tick.elapsed() >= tick_rate {
                let day_events = self.game.advance_day();
                // Switch to Events tab on critical events
                if day_events.iter().any(|e| e.importance() == crate::event::EventImportance::Critical) {
                    if let Some(idx) = Tab::ALL.iter().position(|t| matches!(t, Tab::Events)) {
                        self.active_tab = idx;
                    }
                }
                last_tick = Instant::now();
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyCode) {
        // Check if we're in an input mode first
        if !matches!(self.input_mode, InputMode::Normal) {
            self.handle_input_mode_key(key);
            return;
        }

        // Clear status message on any keypress
        self.status_message = None;

        match key {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char(' ') => self.game.toggle_pause(),
            KeyCode::Char('1') => self.game.set_speed(GameSpeed::Normal),
            KeyCode::Char('2') => self.game.set_speed(GameSpeed::Fast),
            KeyCode::Char('3') => self.game.set_speed(GameSpeed::VeryFast),
            KeyCode::Char('s') => self.save_game(),

            KeyCode::Left => self.focused_pane = FocusedPane::Sidebar,
            KeyCode::Right => self.focused_pane = FocusedPane::Content,

            KeyCode::Up => self.handle_up(),
            KeyCode::Down => self.handle_down(),

            // Tab-specific action keys work regardless of focused pane
            _ => {
                self.handle_tab_key(key);
            }
        }
    }

    fn handle_tab_key(&mut self, key: KeyCode) {
        match self.current_tab() {
            Tab::Engines => self.handle_engines_key(key),
            Tab::Reactors => self.handle_reactors_key(key),
            Tab::Rockets => self.handle_rockets_key(key),
            Tab::Manufacturing => self.handle_manufacturing_key(key),
            Tab::Contracts => self.handle_contracts_key(key),
            Tab::Launches => self.handle_launches_key(key),
            _ => {}
        }
    }

    /// Map the reactor-pane's visible selection (which hides Proposed
    /// drafts) back to the underlying `reactor_projects` index.
    fn reactor_pane_real_index(&self) -> Option<usize> {
        self.game.player_company.visible_reactor_projects()
            .nth(self.selected_item)
            .map(|(real_idx, _)| real_idx)
    }

    fn handle_reactors_key(&mut self, key: KeyCode) {
        use crate::reactor::{EnrichmentLevel, DEFAULT_SCALE};
        use crate::reactor_project::ReactorDesignStatus;

        let real_idx = self.reactor_pane_real_index();
        match key {
            KeyCode::Char('n') | KeyCode::Char('N') => {
                // Pick a fresh default name based on how many projects
                // exist; the player can rename inside the editor.
                let n = self.game.player_company.reactor_projects.len() + 1;
                let name = format!("Reactor Mk{}", n);
                let pid = self.game.player_company.start_proposed_reactor(
                    name, DEFAULT_SCALE, EnrichmentLevel::Leu,
                );
                self.enter_modal(InputMode::ReactorEditor { project_id: pid, cursor: 0 });
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                let idx = real_idx.unwrap_or(usize::MAX);
                if self.game.player_company.add_team_to_reactor_project(idx) {
                    self.status_message = Some("Team assigned".into());
                } else {
                    self.status_message = Some("No teams available".into());
                }
            }
            KeyCode::Char('-') => {
                let idx = real_idx.unwrap_or(usize::MAX);
                if self.game.player_company.remove_team_from_reactor_project(idx) {
                    self.status_message = Some("Team removed".into());
                }
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                // Re-open the editor on an InDesign reactor. Testing /
                // Revising / Proposed don't make sense here:
                // Testing is read-only; Revising is for Phase 3; the
                // pane hides Proposed.
                let idx = match real_idx { Some(i) => i, None => return };
                let project = &self.game.player_company.reactor_projects[idx];
                let pid = project.project_id;
                if matches!(project.status, ReactorDesignStatus::InDesign { .. }) {
                    self.enter_modal(InputMode::ReactorEditor { project_id: pid, cursor: 0 });
                } else {
                    self.status_message = Some(
                        "Editor only available on In Design reactors".into());
                }
            }
            _ => {}
        }
    }

    /// Map the engine-pane's visible selection (which hides Proposed
    /// projects) back to the underlying `engine_projects` index used by
    /// every project-action API.
    fn engine_pane_real_index(&self) -> Option<usize> {
        self.game.player_company.visible_engine_projects()
            .nth(self.selected_item)
            .map(|(real_idx, _)| real_idx)
    }

    fn handle_engines_key(&mut self, key: KeyCode) {
        // All engine-pane actions operate on the visible selection, so
        // resolve it to a real index up front. None = no engines, the
        // selection points past the end, or all hits are Proposed.
        let real_idx = self.engine_pane_real_index();
        match key {
            // Engine design no longer has its own entry point — players
            // design engines inside the rocket designer's "+ Design new
            // engine" picker entry, and the engine is promoted to InDesign
            // when the rocket is committed.
            KeyCode::Char('b') => {
                // Buy third-party engine
                if !self.game.player_company.third_party_catalog.is_empty() {
                    self.enter_modal(InputMode::SelectThirdParty { selected: 0 });
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                // Add team to selected project, or steal from busiest
                let idx = real_idx.unwrap_or(usize::MAX);
                if self.game.player_company.add_team_to_project(idx) {
                    self.status_message = Some("Team assigned".into());
                } else if let Some(from) = self.game.player_company.steal_engineering_team_to_engine_project(idx) {
                    self.status_message = Some(format!("Team reassigned from {}", from));
                } else {
                    self.status_message = Some("No teams to reassign".into());
                }
            }
            KeyCode::Char('-') => {
                // Remove team from selected project
                let idx = real_idx.unwrap_or(usize::MAX);
                if self.game.player_company.remove_team_from_project(idx) {
                    self.status_message = Some("Team removed".into());
                }
            }
            KeyCode::Char('o') => {
                // Order standalone engine build
                let idx = real_idx.unwrap_or(usize::MAX);
                if let Some((cost, evt)) = self.game.player_company.order_engine_build(idx) {
                    self.game.event_log.push(self.game.date, evt);
                    self.status_message = Some(format!("Engine build ordered ({})", crate::ui::draw::format_money(cost)));
                } else {
                    self.status_message = Some("Must be in Testing to order build".into());
                }
            }
            KeyCode::Char('r') => {
                // Revise all discovered flaws and actualize pending improvements
                if let Some(idx) = real_idx {
                    let project = &mut self.game.player_company.engine_projects[idx];
                    if project.start_revision() {
                        let (fc, ic) = match &project.status {
                            EngineDesignStatus::Revising { remaining_flaw_indices, remaining_improvement_indices, .. } =>
                                (remaining_flaw_indices.len(), remaining_improvement_indices.len()),
                            _ => (0, 0),
                        };
                        if ic > 0 {
                            self.status_message = Some(format!("Revising {} flaw(s), {} improvement(s)", fc, ic));
                        } else {
                            self.status_message = Some(format!("Revising {} flaw(s)", fc));
                        }
                    }
                }
            }
            KeyCode::Char('e') => {
                let team_num = self.game.player_company.team_count() + 1;
                let name = format!("Team {}", team_num);
                if let Some(evt) = self.game.player_company.hire_team(name.clone()) {
                    self.game.event_log.push(self.game.date, evt);
                    self.status_message = Some(format!("Hired {}", name));
                }
            }
            _ => {}
        }
    }

    fn handle_rockets_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('n') => {
                // Start new rocket design flow
                self.enter_modal(InputMode::RocketName { buffer: String::new() });
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if self.game.player_company.add_team_to_rocket_project(self.selected_item) {
                    self.status_message = Some("Team assigned".into());
                } else if let Some(from) = self.game.player_company.steal_engineering_team_to_rocket_project(self.selected_item) {
                    self.status_message = Some(format!("Team reassigned from {}", from));
                } else {
                    self.status_message = Some("No teams to reassign".into());
                }
            }
            KeyCode::Char('-') => {
                if self.game.player_company.remove_team_from_rocket_project(self.selected_item) {
                    self.status_message = Some("Team removed".into());
                }
            }
            KeyCode::Char('r') => {
                if self.selected_item < self.game.player_company.rocket_projects.len() {
                    let project = &mut self.game.player_company.rocket_projects[self.selected_item];
                    if project.start_revision() {
                        let count = match &project.status {
                            RocketDesignStatus::Revising { remaining_indices, .. } => remaining_indices.len(),
                            _ => 0,
                        };
                        self.status_message = Some(format!("Revising {} flaw(s)", count));
                    }
                }
            }
            KeyCode::Char('e') => {
                let team_num = self.game.player_company.team_count() + 1;
                let name = format!("Team {}", team_num);
                if let Some(evt) = self.game.player_company.hire_team(name.clone()) {
                    self.game.event_log.push(self.game.date, evt);
                    self.status_message = Some(format!("Hired {}", name));
                }
            }
            KeyCode::Char('o') => {
                // Order rocket build
                if let Some((cost, evt)) = self.game.player_company.order_rocket_build(self.selected_item) {
                    self.game.event_log.push(self.game.date, evt);
                    self.status_message = Some(format!("Build ordered ({})", crate::ui::draw::format_money(cost)));
                } else {
                    self.status_message = Some("Must be in Testing to order build".into());
                }
            }
            KeyCode::Char('M') => {
                // Modify the selected rocket project — opens the rocket
                // designer in Modify mode (only propellant + power
                // editable). Only allowed for InDesign / Testing.
                if self.selected_item >= self.game.player_company.rocket_projects.len() {
                    return;
                }
                let project = &self.game.player_company.rocket_projects[self.selected_item];
                match &project.status {
                    RocketDesignStatus::Revising { .. } => {
                        self.status_message = Some(
                            "Can't modify while revising — finish flaws first".into());
                        return;
                    }
                    _ => {}
                }
                let state = Box::new(RocketDesignerState::from_existing(
                    project, &self.game.player_company,
                ));
                self.enter_modal(InputMode::RocketDesigner { state });
            }
            KeyCode::Char('m') => {
                // Cycle auto-build target: 0 → 1 → 2 → 3 → 0
                if self.selected_item < self.game.player_company.rocket_projects.len() {
                    let project = &self.game.player_company.rocket_projects[self.selected_item];
                    if matches!(project.status, RocketDesignStatus::Testing { .. }) {
                        let pid = project.project_id;
                        let current = self.game.player_company.auto_build_targets
                            .get(&pid).copied().unwrap_or(0);
                        let next = if current >= 3 { 0 } else { current + 1 };
                        if next == 0 {
                            self.game.player_company.auto_build_targets.remove(&pid);
                            self.status_message = Some("Auto-build: off".into());
                        } else {
                            self.game.player_company.auto_build_targets.insert(pid, next);
                            self.status_message = Some(format!("Auto-build: {}", next));
                        }
                    } else {
                        self.status_message = Some("Must be in Testing to set auto-build".into());
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_manufacturing_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('b') => {
                // Buy floor space
                let cost = self.game.player_company.manufacturing.floor_space.order_expansion(1);
                self.game.player_company.money -= cost;
                self.status_message = Some(format!("Ordered 1 floor space unit ({})", crate::ui::draw::format_money(cost)));
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if self.game.player_company.add_team_to_manufacturing_order(self.selected_item) {
                    self.status_message = Some("Mfg team assigned".into());
                } else if let Some(from) = self.game.player_company.steal_manufacturing_team_to_order(self.selected_item) {
                    self.status_message = Some(format!("Mfg team reassigned from {}", from));
                } else {
                    self.status_message = Some("No mfg teams to reassign".into());
                }
            }
            KeyCode::Char('-') => {
                if self.game.player_company.remove_team_from_manufacturing_order(self.selected_item) {
                    self.status_message = Some("Mfg team removed".into());
                }
            }
            KeyCode::Char('m') => {
                let team_num = self.game.player_company.manufacturing_teams.len() + 1;
                let name = format!("Mfg Team {}", team_num);
                if let Some(evt) = self.game.player_company.hire_manufacturing_team(name.clone()) {
                    self.game.event_log.push(self.game.date, evt);
                    self.status_message = Some(format!("Hired {}", name));
                }
            }
            _ => {}
        }
    }

    fn handle_contracts_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('a') | KeyCode::Enter => {
                // Accept the selected contract (if it's in the available section)
                let avail_len = self.game.available_contracts.len();
                if self.selected_item < avail_len {
                    if let Some(evt) = self.game.accept_contract(self.selected_item) {
                        self.status_message = Some(format!("{}", evt));
                    }
                } else {
                    self.status_message = Some("Already accepted".into());
                }
            }
            _ => {}
        }
    }

    fn handle_launches_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('f') | KeyCode::Char('F') => {
                // Fly a spacecraft to a new destination
                if self.game.spacecraft.is_empty() {
                    self.status_message = Some("No spacecraft available".into());
                    return;
                }
                self.enter_modal(InputMode::FlySelectSpacecraft {
                    selected: 0,
                });
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // Dock one spacecraft onto another at the same location.
                if self.game.spacecraft.len() < 2 {
                    self.status_message = Some("Need at least two spacecraft to dock".into());
                    return;
                }
                self.enter_modal(InputMode::DockSelectSmall { selected: 0 });
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                // Undock a payload from a carrier.
                let candidates: Vec<usize> = self.game.spacecraft.iter().enumerate()
                    .filter(|(_, sc)| sc.payloads.iter().any(|p|
                        matches!(p, crate::flight::Payload::Spacecraft { .. })))
                    .map(|(i, _)| i)
                    .collect();
                if candidates.is_empty() {
                    self.status_message = Some("No spacecraft with docked payloads".into());
                    return;
                }
                self.enter_modal(InputMode::UndockSelectCarrier {
                    candidates, selected: 0,
                });
            }
            KeyCode::Char('p') => {
                // Open delta-v planner setup
                let eligible: Vec<usize> = self.game.player_company.rocket_projects.iter()
                    .enumerate()
                    .filter(|(_, rp)| matches!(rp.status, RocketDesignStatus::Testing { .. }))
                    .map(|(i, _)| i)
                    .collect();
                if eligible.is_empty() {
                    self.status_message = Some("No rocket design available".into());
                    return;
                }
                let locations: Vec<(&'static str, &'static str)> = DELTA_V_MAP.locations().iter()
                    .map(|loc| (loc.id, loc.display_name))
                    .collect();
                // Default to earth_surface
                let default_loc = locations.iter().position(|(id, _)| *id == "earth_surface").unwrap_or(0);
                self.enter_modal(InputMode::PlannerSetup {
                    state: Box::new(PlannerSetupState {
                        eligible_projects: eligible,
                        selected_project: 0,
                        locations,
                        selected_location: default_loc,
                        payload_buffer: "0".into(),
                        active_field: PlannerSetupField::Design,
                    }),
                });
            }
            KeyCode::Char('l') | KeyCode::Enter
            | KeyCode::Char('k') | KeyCode::Char('K') => {
                // 'k'/'K' = keep: the carrier becomes a Spacecraft at the
                // destination instead of being discarded on arrival.
                let persist = matches!(key, KeyCode::Char('k') | KeyCode::Char('K'));
                // Launch the selected rocket
                let rockets = &self.game.player_company.manufacturing.inventory.rockets;
                if self.selected_item >= rockets.len() {
                    self.status_message = Some("No rocket selected".into());
                    return;
                }
                let rocket = &rockets[self.selected_item];
                let item_id = rocket.item_id;
                let project_id = rocket.rocket_project_id;

                // Find the rocket project to get its design
                let rp = self.game.player_company.rocket_projects.iter()
                    .find(|rp| rp.project_id == project_id);
                if rp.is_none() {
                    self.status_message = Some("Rocket project not found".into());
                    return;
                }

                // Enter launch modal — assemble multi-payload manifest.
                let contract_picks = vec![false; self.game.player_company.active_contracts.len()];
                let spacecraft_item_ids: Vec<_> = self.game.player_company.manufacturing.inventory.rockets
                    .iter()
                    .filter(|r| r.item_id != item_id)
                    .map(|r| r.item_id)
                    .collect();
                let spacecraft_picks = vec![false; spacecraft_item_ids.len()];
                self.enter_modal(InputMode::LaunchManifest {
                    rocket_item_id: item_id,
                    persist,
                    contract_picks,
                    spacecraft_picks,
                    spacecraft_item_ids,
                    cursor: 0,
                });
            }
            _ => {}
        }
    }

    fn handle_input_mode_key(&mut self, key: KeyCode) {
        match &mut self.input_mode {
            InputMode::Normal => unreachable!(),
            InputMode::ReactorEditor { .. }
            | InputMode::ReactorEditorNameInput { .. }
            | InputMode::ReactorEditorScaleInput { .. } => {
                let old_mode = std::mem::replace(&mut self.input_mode, InputMode::Normal);
                match old_mode {
                    InputMode::ReactorEditor { project_id, cursor } => {
                        self.handle_reactor_editor_key(key, project_id, cursor);
                    }
                    InputMode::ReactorEditorNameInput { project_id, cursor, mut buffer } => {
                        match key {
                            KeyCode::Esc => {
                                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
                            }
                            KeyCode::Enter => {
                                let new_name = buffer.trim().to_string();
                                if !new_name.is_empty() {
                                    if let Some(rp) = self.game.player_company
                                        .find_reactor_project_mut(project_id)
                                    {
                                        rp.design.name = new_name;
                                    }
                                }
                                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
                            }
                            KeyCode::Backspace => {
                                buffer.pop();
                                self.input_mode = InputMode::ReactorEditorNameInput {
                                    project_id, cursor, buffer,
                                };
                            }
                            KeyCode::Char(c) => {
                                buffer.push(c);
                                self.input_mode = InputMode::ReactorEditorNameInput {
                                    project_id, cursor, buffer,
                                };
                            }
                            _ => {
                                self.input_mode = InputMode::ReactorEditorNameInput {
                                    project_id, cursor, buffer,
                                };
                            }
                        }
                    }
                    InputMode::ReactorEditorScaleInput { project_id, cursor, mut buffer } => {
                        match key {
                            KeyCode::Esc => {
                                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
                            }
                            KeyCode::Enter => {
                                if let Ok(parsed) = buffer.parse::<f64>() {
                                    let clamped = parsed
                                        .max(crate::reactor::MIN_SCALE)
                                        .min(crate::reactor::MAX_SCALE);
                                    self.apply_reactor_scale(project_id, clamped);
                                }
                                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
                            }
                            KeyCode::Backspace => {
                                buffer.pop();
                                self.input_mode = InputMode::ReactorEditorScaleInput {
                                    project_id, cursor, buffer,
                                };
                            }
                            KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                                buffer.push(c);
                                self.input_mode = InputMode::ReactorEditorScaleInput {
                                    project_id, cursor, buffer,
                                };
                            }
                            _ => {
                                self.input_mode = InputMode::ReactorEditorScaleInput {
                                    project_id, cursor, buffer,
                                };
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
            InputMode::EngineEditor { .. }
            | InputMode::EngineEditorNameInput { .. }
            | InputMode::EngineEditorScaleInput { .. } => {
                // Extract and dispatch separately to avoid holding a
                // mutable borrow on self.input_mode while calling self
                // methods.
                let old_mode = std::mem::replace(&mut self.input_mode, InputMode::Normal);
                match old_mode {
                    InputMode::EngineEditor { project_id, cursor, state } => {
                        self.handle_engine_editor_key(key, project_id, cursor, state);
                    }
                    InputMode::EngineEditorNameInput { project_id, cursor, mut buffer, mut state } => {
                        match key {
                            KeyCode::Esc => {
                                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
                            }
                            KeyCode::Enter => {
                                let new_name = buffer.trim().to_string();
                                if !new_name.is_empty() {
                                    if let Some(ep) = self.game.player_company
                                        .find_engine_project_mut(project_id)
                                    {
                                        ep.design.name = new_name;
                                    }
                                    sync_stages_to_projects(&mut state, &self.game.player_company);
                                }
                                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
                            }
                            KeyCode::Backspace => {
                                buffer.pop();
                                self.input_mode = InputMode::EngineEditorNameInput {
                                    project_id, cursor, buffer, state,
                                };
                            }
                            KeyCode::Char(c) => {
                                buffer.push(c);
                                self.input_mode = InputMode::EngineEditorNameInput {
                                    project_id, cursor, buffer, state,
                                };
                            }
                            _ => {
                                self.input_mode = InputMode::EngineEditorNameInput {
                                    project_id, cursor, buffer, state,
                                };
                            }
                        }
                    }
                    InputMode::EngineEditorScaleInput { project_id, cursor, mut buffer, mut state } => {
                        match key {
                            KeyCode::Esc => {
                                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
                            }
                            KeyCode::Enter => {
                                if let Ok(parsed) = buffer.parse::<f64>() {
                                    let clamped = parsed
                                        .max(crate::engine_project::MIN_SCALE)
                                        .min(crate::engine_project::MAX_SCALE);
                                    self.apply_engine_scale(project_id, clamped);
                                    sync_stages_to_projects(&mut state, &self.game.player_company);
                                }
                                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
                            }
                            KeyCode::Backspace => {
                                buffer.pop();
                                self.input_mode = InputMode::EngineEditorScaleInput {
                                    project_id, cursor, buffer, state,
                                };
                            }
                            KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                                buffer.push(c);
                                self.input_mode = InputMode::EngineEditorScaleInput {
                                    project_id, cursor, buffer, state,
                                };
                            }
                            _ => {
                                self.input_mode = InputMode::EngineEditorScaleInput {
                                    project_id, cursor, buffer, state,
                                };
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
            InputMode::SelectThirdParty { selected } => {
                let catalog_len = self.game.player_company.third_party_catalog.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => { if *selected > 0 { *selected -= 1; } }
                    KeyCode::Down => { if *selected + 1 < catalog_len { *selected += 1; } }
                    KeyCode::Enter => {
                        let idx = *selected;
                        let date = self.game.date;
                        self.exit_modal();
                        let seed_clone = self.game.seed.clone();
                        if let Some(evt) = self.game.player_company.contract_third_party(idx, date, &seed_clone) {
                            self.game.event_log.push(self.game.date, evt);
                            self.status_message = Some("Engine contracted".into());
                        }
                    }
                    _ => {}
                }
            }
            InputMode::RocketName { buffer } => {
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Enter => {
                        if buffer.is_empty() {
                            self.status_message = Some("Name cannot be empty".into());
                            self.exit_modal();
                        } else {
                            let name = buffer.clone();
                            self.input_mode = InputMode::RocketDesigner {
                                state: Box::new(RocketDesignerState::new(name)),
                            };
                        }
                    }
                    KeyCode::Backspace => { buffer.pop(); }
                    KeyCode::Char(c) => { buffer.push(c); }
                    _ => {}
                }
            }
            InputMode::RocketDesigner { .. }
            | InputMode::RocketPickEngine { .. }
            | InputMode::RocketPayloadInput { .. }
            | InputMode::RocketDesignerLocationPicker { .. }
            | InputMode::PowerEditor { .. } => {
                // Extract all data from the enum variant before calling handlers,
                // to avoid holding a mutable borrow on self.input_mode.
                let old_mode = std::mem::replace(&mut self.input_mode, InputMode::Normal);
                match old_mode {
                    InputMode::RocketDesigner { state } => {
                        self.handle_rocket_designer_key(key, state);
                    }
                    InputMode::RocketPickEngine { state, target_index, inner_index, editing, booster, selected } => {
                        self.handle_rocket_pick_engine_key(
                            key, state, target_index, inner_index, editing, booster, selected,
                        );
                    }
                    InputMode::RocketPayloadInput { state, buffer } => {
                        self.handle_rocket_payload_input_key(key, state, buffer);
                    }
                    InputMode::RocketDesignerLocationPicker { state, target, locations, selected } => {
                        self.handle_rocket_designer_location_picker_key(
                            key, state, target, locations, selected,
                        );
                    }
                    InputMode::PowerEditor { state, group_index, stage_index, cursor } => {
                        self.handle_power_editor_key(
                            key, state, group_index, stage_index, cursor,
                        );
                    }
                    _ => unreachable!(),
                }
            }
            InputMode::LaunchManifest {
                rocket_item_id, persist, contract_picks, spacecraft_picks,
                spacecraft_item_ids, cursor,
            } => {
                let rocket_item_id = *rocket_item_id;
                let persist = *persist;
                let num_contracts = contract_picks.len();
                let num_spacecraft = spacecraft_picks.len();
                let total_rows = num_contracts + num_spacecraft;
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => {
                        if *cursor > 0 { *cursor -= 1; }
                    }
                    KeyCode::Down => {
                        if *cursor + 1 < total_rows { *cursor += 1; }
                    }
                    KeyCode::Char(' ') => {
                        if *cursor < num_contracts {
                            contract_picks[*cursor] = !contract_picks[*cursor];
                        } else if *cursor - num_contracts < num_spacecraft {
                            let idx = *cursor - num_contracts;
                            spacecraft_picks[idx] = !spacecraft_picks[idx];
                        }
                    }
                    KeyCode::Enter => {
                        // Snapshot picks (we'll need to mutate game state).
                        let contract_picks = contract_picks.clone();
                        let spacecraft_picks = spacecraft_picks.clone();
                        let spacecraft_item_ids = spacecraft_item_ids.clone();
                        self.submit_manifest_launch(
                            rocket_item_id, persist,
                            contract_picks, spacecraft_picks, spacecraft_item_ids,
                        );
                    }
                    _ => {}
                }
            }
            InputMode::LaunchResult { .. } => {
                // Any key dismisses the result
                match key {
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char(_) => {
                        self.exit_modal();
                    }
                    _ => {}
                }
            }
            InputMode::FlySelectSpacecraft { selected } => {
                let selected = *selected;
                let num_spacecraft = self.game.spacecraft.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => {
                        if selected > 0 {
                            if let InputMode::FlySelectSpacecraft { selected: s } = &mut self.input_mode {
                                *s -= 1;
                            }
                        }
                    }
                    KeyCode::Down => {
                        if selected + 1 < num_spacecraft {
                            if let InputMode::FlySelectSpacecraft { selected: s } = &mut self.input_mode {
                                *s += 1;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        let sc = &self.game.spacecraft[selected];
                        let remaining_dv = sc.remaining_delta_v();
                        // Use the live sum of carried payload masses rather than
                        // the cached `payload_mass_kg`, which may be stale if
                        // payloads were detached on a previous flight.
                        let payload_mass: f64 = sc.payloads.iter().map(|p| p.mass_kg()).sum();
                        let rocket_mass = payload_mass + sc.design.total_mass_kg();
                        let low_thrust = sc.rocket.is_current_stage_low_thrust(&sc.design);
                        let destinations = reachable_destinations_multistage(
                            &sc.location, remaining_dv, rocket_mass, low_thrust,
                            Some(&sc.rocket), Some(&sc.design),
                        );
                        if destinations.is_empty() {
                            self.status_message = Some("No reachable destinations for this spacecraft".into());
                            return;
                        }
                        self.input_mode = InputMode::FlySelectDestination {
                            spacecraft_index: selected,
                            destinations,
                            remaining_dv,
                            selected: 0,
                        };
                    }
                    _ => {}
                }
            }
            InputMode::FlySelectDestination { spacecraft_index, destinations, selected, .. } => {
                let spacecraft_index = *spacecraft_index;
                let selected = *selected;
                let num_destinations = destinations.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => {
                        if selected > 0 {
                            if let InputMode::FlySelectDestination { selected: s, .. } = &mut self.input_mode {
                                *s -= 1;
                            }
                        }
                    }
                    KeyCode::Down => {
                        if selected + 1 < num_destinations {
                            if let InputMode::FlySelectDestination { selected: s, .. } = &mut self.input_mode {
                                *s += 1;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        if let InputMode::FlySelectDestination { destinations, .. } = &self.input_mode {
                            let dest_id = destinations[selected].0.clone();
                            self.game.fly_spacecraft(spacecraft_index, &dest_id);
                            self.status_message = Some("Spacecraft flight departed".into());
                            self.exit_modal();
                        }
                    }
                    _ => {}
                }
            }
            InputMode::DockSelectSmall { selected } => {
                let selected = *selected;
                let num = self.game.spacecraft.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => if let InputMode::DockSelectSmall { selected: s } = &mut self.input_mode {
                        if *s > 0 { *s -= 1; }
                    },
                    KeyCode::Down => if let InputMode::DockSelectSmall { selected: s } = &mut self.input_mode {
                        if *s + 1 < num { *s += 1; }
                    },
                    KeyCode::Enter => {
                        // Build candidate list: other spacecraft at the
                        // same location as the chosen "small" one.
                        let small_loc = &self.game.spacecraft[selected].location;
                        let candidates: Vec<usize> = self.game.spacecraft.iter().enumerate()
                            .filter(|(i, sc)| *i != selected && sc.location == *small_loc)
                            .map(|(i, _)| i)
                            .collect();
                        if candidates.is_empty() {
                            self.status_message = Some(
                                "No other spacecraft at this location".into());
                            self.exit_modal();
                            return;
                        }
                        self.input_mode = InputMode::DockSelectLarge {
                            small_idx: selected, candidates, selected: 0,
                        };
                    }
                    _ => {}
                }
            }
            InputMode::DockSelectLarge { small_idx, candidates, selected } => {
                let small_idx = *small_idx;
                let selected = *selected;
                let num = candidates.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => if let InputMode::DockSelectLarge { selected: s, .. } = &mut self.input_mode {
                        if *s > 0 { *s -= 1; }
                    },
                    KeyCode::Down => if let InputMode::DockSelectLarge { selected: s, .. } = &mut self.input_mode {
                        if *s + 1 < num { *s += 1; }
                    },
                    KeyCode::Enter => {
                        let large_idx = candidates[selected];
                        if self.game.dock_spacecraft(small_idx, large_idx) {
                            self.status_message = Some("Docked".into());
                        } else {
                            self.status_message = Some("Dock failed".into());
                        }
                        self.exit_modal();
                    }
                    _ => {}
                }
            }
            InputMode::UndockSelectCarrier { candidates, selected } => {
                let selected = *selected;
                let num = candidates.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => if let InputMode::UndockSelectCarrier { selected: s, .. } = &mut self.input_mode {
                        if *s > 0 { *s -= 1; }
                    },
                    KeyCode::Down => if let InputMode::UndockSelectCarrier { selected: s, .. } = &mut self.input_mode {
                        if *s + 1 < num { *s += 1; }
                    },
                    KeyCode::Enter => {
                        let carrier_idx = candidates[selected];
                        let payload_indices: Vec<usize> = self.game.spacecraft[carrier_idx]
                            .payloads.iter().enumerate()
                            .filter(|(_, p)| matches!(p, crate::flight::Payload::Spacecraft { .. }))
                            .map(|(i, _)| i)
                            .collect();
                        if payload_indices.is_empty() {
                            self.status_message = Some("No spacecraft payloads".into());
                            self.exit_modal();
                            return;
                        }
                        self.input_mode = InputMode::UndockSelectPayload {
                            carrier_idx, payload_indices, selected: 0,
                        };
                    }
                    _ => {}
                }
            }
            InputMode::UndockSelectPayload { carrier_idx, payload_indices, selected } => {
                let carrier_idx = *carrier_idx;
                let selected = *selected;
                let num = payload_indices.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => if let InputMode::UndockSelectPayload { selected: s, .. } = &mut self.input_mode {
                        if *s > 0 { *s -= 1; }
                    },
                    KeyCode::Down => if let InputMode::UndockSelectPayload { selected: s, .. } = &mut self.input_mode {
                        if *s + 1 < num { *s += 1; }
                    },
                    KeyCode::Enter => {
                        let payload_idx = payload_indices[selected];
                        if self.game.undock_payload(carrier_idx, payload_idx) {
                            self.status_message = Some("Undocked".into());
                        } else {
                            self.status_message = Some("Undock failed".into());
                        }
                        self.exit_modal();
                    }
                    _ => {}
                }
            }
            InputMode::PlannerSetup { state } => {
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Tab => {
                        state.active_field = match state.active_field {
                            PlannerSetupField::Design => PlannerSetupField::Payload,
                            PlannerSetupField::Payload => PlannerSetupField::Location,
                            PlannerSetupField::Location => PlannerSetupField::Design,
                        };
                    }
                    KeyCode::Up => {
                        match state.active_field {
                            PlannerSetupField::Design => {
                                if state.selected_project > 0 {
                                    state.selected_project -= 1;
                                }
                            }
                            PlannerSetupField::Location => {
                                if state.selected_location > 0 {
                                    state.selected_location -= 1;
                                }
                            }
                            PlannerSetupField::Payload => {}
                        }
                    }
                    KeyCode::Down => {
                        match state.active_field {
                            PlannerSetupField::Design => {
                                if state.selected_project + 1 < state.eligible_projects.len() {
                                    state.selected_project += 1;
                                }
                            }
                            PlannerSetupField::Location => {
                                if state.selected_location + 1 < state.locations.len() {
                                    state.selected_location += 1;
                                }
                            }
                            PlannerSetupField::Payload => {}
                        }
                    }
                    KeyCode::Char(c) if state.active_field == PlannerSetupField::Payload => {
                        if c.is_ascii_digit() || c == '.' {
                            state.payload_buffer.push(c);
                        }
                    }
                    KeyCode::Backspace if state.active_field == PlannerSetupField::Payload => {
                        state.payload_buffer.pop();
                    }
                    KeyCode::Enter => {
                        // Launch the planner with chosen parameters
                        let pi = state.eligible_projects[state.selected_project];
                        let rp = &self.game.player_company.rocket_projects[pi];
                        let payload_kg: f64 = state.payload_buffer.parse().unwrap_or(0.0);
                        let (start_id, _) = state.locations[state.selected_location];
                        let rocket = rp.design.instantiate(
                            crate::rocket::RocketId(0), start_id, payload_kg,
                        );
                        let remaining_dv = rocket.remaining_delta_v(&rp.design);
                        let rocket_mass = rp.design.total_mass_kg() + payload_kg;
                        let low_thrust = rocket.is_current_stage_low_thrust(&rp.design);
                        let destinations = reachable_destinations_multistage(
                            start_id, remaining_dv, rocket_mass, low_thrust,
                            Some(&rocket), Some(&rp.design),
                        );
                        self.input_mode = InputMode::DvPlanner {
                            state: Box::new(DvPlannerState {
                                source: PlannerSource::Design { project_index: pi },
                                rocket,
                                design: rp.design.clone(),
                                current_location: start_id.to_string(),
                                actions: vec![],
                                snapshots: vec![],
                                destinations,
                                selected: 0,
                                payload_kg,
                            }),
                        };
                    }
                    _ => {}
                }
            }
            InputMode::DvPlanner { state } => {
                let num_dests = state.destinations.len();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => {
                        if state.selected > 0 {
                            state.selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if state.selected + 1 < num_dests {
                            state.selected += 1;
                        }
                    }
                    KeyCode::Enter => {
                        // Select destination — simulate the burn
                        if state.selected < num_dests {
                            let (dest_id, dest_display, dv_cost) =
                                state.destinations[state.selected].clone();
                            let from = state.current_location.clone();

                            // Save snapshot for undo
                            state.snapshots.push((state.rocket.clone(), state.payload_kg));

                            // Burn propellant (account for atmospheric Isp penalty)
                            let ambient = crate::location::DELTA_V_MAP
                                .surface_properties(&from)
                                .filter(|p| p.has_atmosphere)
                                .map_or(0.0, |p| p.ambient_pressure_pa);
                            let _ = state.rocket.burn_sequential(&state.design, dv_cost, ambient);
                            state.rocket.location = dest_id.clone();
                            state.current_location = dest_id;

                            state.actions.push(PlanAction::Leg {
                                from,
                                to: state.current_location.clone(),
                                to_display: dest_display,
                                dv_cost,
                            });

                            // Recompute destinations
                            let remaining_dv = state.rocket.remaining_delta_v(&state.design);
                            let rocket_mass = state.design.total_mass_kg() + state.payload_kg;
                            let lt = state.rocket.is_current_stage_low_thrust(&state.design);
                            state.destinations = reachable_destinations_multistage(
                                &state.current_location, remaining_dv, rocket_mass, lt,
                                Some(&state.rocket), Some(&state.design),
                            );
                            state.selected = 0;
                        }
                    }
                    KeyCode::Char('d') => {
                        // Drop payload
                        if state.payload_kg > 0.0 {
                            let mass = state.payload_kg;
                            state.snapshots.push((state.rocket.clone(), state.payload_kg));
                            state.payload_kg = 0.0;
                            state.rocket.payload_mass_kg = 0.0;
                            state.actions.push(PlanAction::DropPayload { mass_dropped: mass });

                            // Recompute destinations with new mass
                            let remaining_dv = state.rocket.remaining_delta_v(&state.design);
                            let rocket_mass = state.design.total_mass_kg();
                            let lt = state.rocket.is_current_stage_low_thrust(&state.design);
                            state.destinations = reachable_destinations_multistage(
                                &state.current_location, remaining_dv, rocket_mass, lt,
                                Some(&state.rocket), Some(&state.design),
                            );
                            state.selected = state.selected.min(
                                state.destinations.len().saturating_sub(1),
                            );
                        }
                    }
                    KeyCode::Char('u') => {
                        // Undo last action
                        if let Some((prev_rocket, prev_payload)) = state.snapshots.pop() {
                            state.actions.pop();
                            state.rocket = prev_rocket;
                            state.payload_kg = prev_payload;
                            state.rocket.payload_mass_kg = prev_payload;
                            state.current_location = state.rocket.location.clone();

                            let remaining_dv = state.rocket.remaining_delta_v(&state.design);
                            let rocket_mass = state.design.total_mass_kg() + state.payload_kg;
                            let lt = state.rocket.is_current_stage_low_thrust(&state.design);
                            state.destinations = reachable_destinations_multistage(
                                &state.current_location, remaining_dv, rocket_mass, lt,
                                Some(&state.rocket), Some(&state.design),
                            );
                            state.selected = state.selected.min(
                                state.destinations.len().saturating_sub(1),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_rocket_designer_key(&mut self, key: KeyCode, mut state: Box<RocketDesignerState>) {
        // Clear status message on any keypress in designer
        self.status_message = None;
        match key {
            KeyCode::Up => {
                let flat = state.flat_index();
                if flat > 0 {
                    state.select_flat(flat - 1);
                }
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Down => {
                let flat = state.flat_index();
                if flat < state.total_stages() {
                    state.select_flat(flat + 1);
                }
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Enter => {
                if state.is_modify() {
                    self.status_message = Some(
                        "Stage layout fixed in Modify mode — only propellant / power editable".into());
                    self.input_mode = InputMode::RocketDesigner { state };
                } else if state.on_add_slot() {
                    // Same as 'a' — add stage at end
                    if state.has_low_thrust_stage() {
                        self.status_message = Some(
                            "Low-thrust designs must be single-stage — carry as payload instead".into());
                        self.input_mode = InputMode::RocketDesigner { state };
                    } else {
                        self.input_mode = InputMode::RocketPickEngine {
                            state,
                            target_index: None,
                            inner_index: None,
                            editing: false,
                            booster: false,
                            selected: 0,
                        };
                    }
                } else {
                    // Edit the selected inner stage
                    let gi = state.selected_group;
                    let si = state.selected_inner;
                    self.input_mode = InputMode::RocketPickEngine {
                        target_index: Some(gi),
                        inner_index: Some(si),
                        editing: true,
                        booster: false,
                        selected: 0,
                        state,
                    };
                }
            }
            KeyCode::Left => {
                // Decrease engine count on selected inner stage
                if state.is_modify() {
                    self.status_message = Some(
                        "Engine count fixed in Modify mode".into());
                } else if !state.on_add_slot() {
                    let gi = state.selected_group;
                    let si = state.selected_inner;
                    let stage = &mut state.stage_groups[gi][si];
                    if stage.engine_count > 1 {
                        let old_count = stage.engine_count;
                        stage.engine_count -= 1;
                        stage.propellant_mass_kg *= stage.engine_count as f64 / old_count as f64;
                        recompute_structural_masses(&mut state.stage_groups);
                    }
                }
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Right => {
                // Increase engine count on selected inner stage
                if state.is_modify() {
                    self.status_message = Some(
                        "Engine count fixed in Modify mode".into());
                } else if !state.on_add_slot() {
                    let gi = state.selected_group;
                    let si = state.selected_inner;
                    let stage = &mut state.stage_groups[gi][si];
                    if stage.engine_count < 9 {
                        let old_count = stage.engine_count;
                        stage.engine_count += 1;
                        stage.propellant_mass_kg *= stage.engine_count as f64 / old_count as f64;
                        recompute_structural_masses(&mut state.stage_groups);
                    }
                }
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                // Increase propellant by thrust-scaled step (not for solid engines)
                if !state.on_add_slot() {
                    let gi = state.selected_group;
                    let si = state.selected_inner;
                    let stage = &mut state.stage_groups[gi][si];
                    if is_solid_engine(&stage.engine) {
                        self.status_message = Some("Solid propellant is not adjustable".into());
                    } else {
                        let step = propellant_step(&stage.engine, stage.engine_count);
                        stage.propellant_mass_kg = (stage.propellant_mass_kg + step).min(2_000_000.0);
                        recompute_structural_masses(&mut state.stage_groups);
                    }
                }
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Char('-') => {
                // Decrease propellant by thrust-scaled step (not for solid engines)
                if !state.on_add_slot() {
                    let gi = state.selected_group;
                    let si = state.selected_inner;
                    let stage = &mut state.stage_groups[gi][si];
                    if is_solid_engine(&stage.engine) {
                        self.status_message = Some("Solid propellant is not adjustable".into());
                    } else {
                        let step = propellant_step(&stage.engine, stage.engine_count);
                        stage.propellant_mass_kg = (stage.propellant_mass_kg - step).max(100.0);
                        recompute_structural_masses(&mut state.stage_groups);
                    }
                }
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                // Add stage at end (new group)
                if state.is_modify() {
                    self.status_message = Some(
                        "Stage layout fixed in Modify mode".into());
                    self.input_mode = InputMode::RocketDesigner { state };
                } else if state.has_low_thrust_stage() {
                    self.status_message = Some(
                        "Low-thrust designs must be single-stage — carry as payload instead".into());
                    self.input_mode = InputMode::RocketDesigner { state };
                } else {
                    self.input_mode = InputMode::RocketPickEngine {
                        state,
                        target_index: None,
                        inner_index: None,
                        editing: false,
                        booster: false,
                        selected: 0,
                    };
                }
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                // Open the power-source editor for the currently-selected
                // stage. No-op when on the "add stage" sentinel slot.
                if !state.on_add_slot() {
                    let group_index = state.selected_group;
                    let stage_index = state.selected_inner;
                    self.input_mode = InputMode::PowerEditor {
                        state, group_index, stage_index, cursor: 0,
                    };
                } else {
                    self.input_mode = InputMode::RocketDesigner { state };
                }
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                // Insert stage before selected group
                if state.is_modify() {
                    self.status_message = Some(
                        "Stage layout fixed in Modify mode".into());
                    self.input_mode = InputMode::RocketDesigner { state };
                } else if !state.on_add_slot() {
                    if state.has_low_thrust_stage() {
                        self.status_message = Some(
                            "Low-thrust designs must be single-stage — carry as payload instead".into());
                        self.input_mode = InputMode::RocketDesigner { state };
                    } else {
                        let idx = state.selected_group;
                        self.input_mode = InputMode::RocketPickEngine {
                            state,
                            target_index: Some(idx),
                            inner_index: None,
                            editing: false,
                            booster: false,
                            selected: 0,
                        };
                    }
                } else {
                    self.input_mode = InputMode::RocketDesigner { state };
                }
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                // Add booster (parallel stage) to current group
                if state.is_modify() {
                    self.status_message = Some(
                        "Stage layout fixed in Modify mode".into());
                    self.input_mode = InputMode::RocketDesigner { state };
                } else if !state.on_add_slot() {
                    if state.has_low_thrust_stage() {
                        self.status_message = Some(
                            "Low-thrust designs must be single-stage — carry as payload instead".into());
                        self.input_mode = InputMode::RocketDesigner { state };
                    } else {
                        let gi = state.selected_group;
                        self.input_mode = InputMode::RocketPickEngine {
                            state,
                            target_index: Some(gi),
                            inner_index: None,
                            editing: false,
                            booster: true,
                            selected: 0,
                        };
                    }
                } else {
                    self.input_mode = InputMode::RocketDesigner { state };
                }
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                // Remove selected inner stage
                if state.is_modify() {
                    self.status_message = Some(
                        "Stage layout fixed in Modify mode".into());
                    self.input_mode = InputMode::RocketDesigner { state };
                    return;
                }
                if !state.on_add_slot() && !state.stage_groups.is_empty() {
                    let gi = state.selected_group;
                    let si = state.selected_inner;
                    if state.stage_groups[gi].len() == 1 {
                        // Remove entire group
                        state.remove_group(gi);
                        rename_all_stages(&mut state.stage_groups);
                        recompute_structural_masses(&mut state.stage_groups);
                        // Adjust selection
                        if state.selected_group >= state.stage_groups.len() && state.selected_group > 0 {
                            state.selected_group -= 1;
                        }
                        state.selected_inner = 0;
                        self.status_message = Some(format!("Removed stage group {}", gi + 1));
                    } else {
                        // Remove just the inner stage
                        state.remove_inner(gi, si);
                        rename_all_stages(&mut state.stage_groups);
                        recompute_structural_masses(&mut state.stage_groups);
                        if state.selected_inner >= state.stage_groups[gi].len() {
                            state.selected_inner = state.stage_groups[gi].len() - 1;
                        }
                        self.status_message = Some("Removed booster stage".into());
                    }
                }
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                // Set payload
                self.input_mode = InputMode::RocketPayloadInput {
                    buffer: format!("{}", state.payload_kg as u64),
                    state,
                };
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                // Pick launch site
                let locations: Vec<(&'static str, &'static str)> = DELTA_V_MAP.locations().iter()
                    .map(|loc| (loc.id, loc.display_name))
                    .collect();
                let selected = locations.iter().position(|(id, _)| *id == state.launch_from).unwrap_or(0);
                self.input_mode = InputMode::RocketDesignerLocationPicker {
                    state, target: LocationPickerTarget::LaunchSite, locations, selected,
                };
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
                // Pick mission destination
                let locations: Vec<(&'static str, &'static str)> = DELTA_V_MAP.locations().iter()
                    .map(|loc| (loc.id, loc.display_name))
                    .collect();
                let selected = locations.iter().position(|(id, _)| *id == state.destination).unwrap_or(0);
                self.input_mode = InputMode::RocketDesignerLocationPicker {
                    state, target: LocationPickerTarget::MissionDestination, locations, selected,
                };
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // Done — finalize design
                if state.stage_groups.is_empty() {
                    self.status_message = Some("Must add at least one stage".into());
                    self.input_mode = InputMode::RocketDesigner { state };
                } else if let DesignerMode::Modify { project_id } = state.mode {
                    // Modify mode: rewrite the existing project's
                    // stages and roll for a new flaw.
                    let stage_groups = state.stage_groups.clone();
                    self.exit_modal();
                    if let Some(evt) = self.game.apply_rocket_modification(project_id, stage_groups) {
                        let summary = format!("{}", evt);
                        self.game.event_log.push(self.game.date, evt);
                        self.status_message = Some(summary);
                    }
                } else {
                    let name = state.rocket_name.clone();
                    let stage_groups = state.stage_groups.clone();
                    // Promote any Proposed engines this session created
                    // that are actually referenced by a stage. Anything
                    // created but unreferenced (e.g. the player started
                    // designing an engine, then replaced its stage with
                    // a different engine) is cleaned up.
                    let referenced: std::collections::HashSet<crate::engine_project::EngineProjectId> =
                        state.engine_sources.iter().flatten()
                            .filter_map(|s| match s {
                                EngineSource::PlayerDesign(id) => Some(*id),
                                _ => None,
                            })
                            .collect();
                    let created = state.created_engine_projects.clone();
                    self.exit_modal();
                    for id in &created {
                        if referenced.contains(id) {
                            if let Some(engine_name) = self.game.player_company
                                .promote_proposed_engine(*id)
                            {
                                self.game.event_log.push(
                                    self.game.date,
                                    crate::event::GameEvent::EngineDesignStarted {
                                        engine_name,
                                    },
                                );
                            }
                        } else {
                            self.game.player_company.delete_proposed_engine(*id);
                        }
                    }
                    self.create_rocket_project(name, stage_groups);
                }
            }
            KeyCode::Esc => {
                // Cancelled — delete any Proposed engines we created.
                let created = state.created_engine_projects.clone();
                self.exit_modal();
                for id in created {
                    self.game.player_company.delete_proposed_engine(id);
                }
                self.status_message = Some("Rocket design cancelled".into());
            }
            _ => {
                self.input_mode = InputMode::RocketDesigner { state };
            }
        }
    }

    fn handle_rocket_pick_engine_key(
        &mut self,
        key: KeyCode,
        mut state: Box<RocketDesignerState>,
        target_index: Option<usize>,
        inner_index: Option<usize>,
        editing: bool,
        booster: bool,
        mut selected: usize,
    ) {
        // Build combined engine list. The picker shows engines plus a
        // trailing "+ Design new engine…" row that opens the standard
        // engine wizard and returns here when finished.
        let engines = self.available_engines();
        let num_engines = engines.len();
        let new_engine_idx = num_engines;
        let total_rows = num_engines + 1; // +1 for "Design new engine" entry

        match key {
            KeyCode::Esc => {
                // Back to designer
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Up => {
                if selected > 0 { selected -= 1; }
                self.input_mode = InputMode::RocketPickEngine {
                    state, target_index, inner_index, editing, booster, selected,
                };
            }
            KeyCode::Down => {
                if selected + 1 < total_rows { selected += 1; }
                self.input_mode = InputMode::RocketPickEngine {
                    state, target_index, inner_index, editing, booster, selected,
                };
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                // Open the engine editor on the highlighted player engine.
                // Only editable while Proposed, InDesign, or Revising.
                if selected < num_engines {
                    if let EngineSource::PlayerDesign(pid) = engines[selected].0 {
                        let editable = self.game.player_company
                            .find_engine_project(pid)
                            .map(|ep| matches!(
                                ep.status,
                                EngineDesignStatus::Proposed { .. }
                                | EngineDesignStatus::InDesign { .. }
                                | EngineDesignStatus::Revising { .. }
                            ))
                            .unwrap_or(false);
                        if editable {
                            self.input_mode = InputMode::EngineEditor {
                                project_id: pid, cursor: 0, state,
                            };
                            return;
                        } else {
                            self.status_message = Some(
                                "Engine in Testing — wait or revise to edit".into());
                        }
                    } else {
                        self.status_message = Some(
                            "Can't edit a third-party engine".into());
                    }
                }
                self.input_mode = InputMode::RocketPickEngine {
                    state, target_index, inner_index, editing, booster, selected,
                };
            }
            KeyCode::Enter => {
                if selected == new_engine_idx {
                    // "Design new engine…" — create a Proposed engine
                    // with sensible defaults, apply it to the target
                    // stage as if the player had just picked it, then
                    // jump straight into the editor for tweaking. The
                    // engine remains Proposed until the rocket is
                    // committed; if the designer is cancelled, the
                    // Proposed engine is cleaned up.
                    let default_name = format!("{}-engine-{}",
                        state.rocket_name,
                        state.created_engine_projects.len() + 1);
                    let cycle = EngineCycle::GasGenerator;
                    let preset = PropellantPreset::Kerolox;
                    let scale = crate::engine_project::DEFAULT_SCALE;
                    let use_vacuum = false;
                    let tech_id = crate::technology::technology_for_preset(preset);
                    let project_id = match self.game.player_company
                        .start_proposed_engine_project(
                            default_name, cycle, preset, scale, use_vacuum, tech_id,
                        )
                    {
                        Some(id) => id,
                        None => {
                            self.status_message = Some("Failed to create engine".into());
                            self.input_mode = InputMode::RocketPickEngine {
                                state, target_index, inner_index, editing, booster, selected,
                            };
                            return;
                        }
                    };
                    state.created_engine_projects.push(project_id);
                    // Apply the engine to the target stage now so the
                    // player sees its effect as they edit. Re-use the
                    // same plumbing as the engine-pick branch by
                    // looking the engine back up.
                    let engine = self.game.player_company
                        .find_engine_project(project_id)
                        .map(|ep| ep.design.clone());
                    if let Some(engine) = engine {
                        apply_picked_engine_to_designer(
                            &mut state, EngineSource::PlayerDesign(project_id),
                            engine, target_index, inner_index, editing, booster,
                        );
                    }
                    self.input_mode = InputMode::EngineEditor {
                        project_id, cursor: 0, state,
                    };
                } else if num_engines == 0 {
                    self.status_message = Some("No engines available".into());
                    self.input_mode = InputMode::RocketPickEngine {
                        state, target_index, inner_index, editing, booster, selected,
                    };
                } else {
                    let (source, engine) = engines[selected].clone();
                    // Enforce: low-thrust engines may only appear in a
                    // single-stage design. The 'a'/'i'/'b'/Enter gates
                    // already block adding to a low-thrust design; this
                    // also catches editing a stage in a multi-stage
                    // design to a low-thrust engine.
                    let other_stages = state.total_stages()
                        .saturating_sub(if editing { 1 } else { 0 });
                    if engine.is_low_thrust() && other_stages > 0 {
                        self.status_message = Some(
                            "Low-thrust engines must be in a single-stage design".into());
                        self.input_mode = InputMode::RocketDesigner { state };
                        return;
                    }
                    apply_picked_engine_to_designer(
                        &mut state, source, engine,
                        target_index, inner_index, editing, booster,
                    );
                    self.input_mode = InputMode::RocketDesigner { state };
                }
            }
            _ => {
                self.input_mode = InputMode::RocketPickEngine {
                    state, target_index, inner_index, editing, booster, selected,
                };
            }
        }
    }


    fn handle_rocket_payload_input_key(
        &mut self,
        key: KeyCode,
        mut state: Box<RocketDesignerState>,
        mut buffer: String,
    ) {
        match key {
            KeyCode::Esc => {
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Enter => {
                if let Ok(val) = buffer.parse::<f64>() {
                    state.payload_kg = val.max(0.0);
                }
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Backspace => {
                buffer.pop();
                self.input_mode = InputMode::RocketPayloadInput { state, buffer };
            }
            KeyCode::Char(c) if c.is_ascii_digit() || c == '.' => {
                buffer.push(c);
                self.input_mode = InputMode::RocketPayloadInput { state, buffer };
            }
            _ => {
                self.input_mode = InputMode::RocketPayloadInput { state, buffer };
            }
        }
    }

    fn handle_rocket_designer_location_picker_key(
        &mut self,
        key: KeyCode,
        mut state: Box<RocketDesignerState>,
        target: LocationPickerTarget,
        locations: Vec<(&'static str, &'static str)>,
        mut selected: usize,
    ) {
        match key {
            KeyCode::Esc => {
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Up => {
                if selected > 0 { selected -= 1; }
                self.input_mode = InputMode::RocketDesignerLocationPicker {
                    state, target, locations, selected,
                };
            }
            KeyCode::Down => {
                if selected + 1 < locations.len() { selected += 1; }
                self.input_mode = InputMode::RocketDesignerLocationPicker {
                    state, target, locations, selected,
                };
            }
            KeyCode::Enter => {
                if let Some((id, _)) = locations.get(selected) {
                    match target {
                        LocationPickerTarget::LaunchSite => state.launch_from = id,
                        LocationPickerTarget::MissionDestination => state.destination = id,
                    }
                }
                self.input_mode = InputMode::RocketDesigner { state };
            }
            _ => {
                self.input_mode = InputMode::RocketDesignerLocationPicker {
                    state, target, locations, selected,
                };
            }
        }
    }

    /// Snapshot of an engine project for editor display + mutation.
    fn editor_snapshot(&self, project_id: crate::engine_project::EngineProjectId)
        -> Option<(String, EngineCycle, PropellantPreset, f64, bool, bool)>
    {
        let ep = self.game.player_company.find_engine_project(project_id)?;
        let baseline = crate::engine_project::engine_baseline(ep.design.cycle, ep.preset)?;
        Some((
            ep.design.name.clone(),
            ep.design.cycle,
            ep.preset,
            ep.scale,
            !ep.design.needs_atmosphere,
            baseline.vacuum_only,
        ))
    }

    /// Apply an arbitrary scale to the engine project, rebuilding its
    /// design through `apply_edit`.
    fn apply_engine_scale(&mut self, project_id: crate::engine_project::EngineProjectId, scale: f64) {
        let snap = match self.editor_snapshot(project_id) {
            Some(s) => s, None => return,
        };
        let (name, cycle, preset, _, use_vacuum, _) = snap;
        if let Some(ep) = self.game.player_company.find_engine_project_mut(project_id) {
            ep.apply_edit(name, cycle, preset, scale, use_vacuum);
        }
    }

    /// Apply a new `scale` to a reactor project, preserving its name
    /// and enrichment. Snapshots the design first so we can rebuild
    /// without taking two overlapping borrows on the project.
    fn apply_reactor_scale(
        &mut self,
        project_id: crate::reactor_project::ReactorProjectId,
        scale: f64,
    ) {
        let snap = match self.game.player_company.find_reactor_project(project_id) {
            Some(rp) => (rp.design.name.clone(), rp.design.enrichment),
            None => return,
        };
        let (name, enrichment) = snap;
        if let Some(rp) = self.game.player_company.find_reactor_project_mut(project_id) {
            rp.apply_edit(name, scale, enrichment);
        }
    }

    /// Apply a new enrichment to a reactor project, preserving its
    /// name and scale.
    fn apply_reactor_enrichment(
        &mut self,
        project_id: crate::reactor_project::ReactorProjectId,
        enrichment: crate::reactor::EnrichmentLevel,
    ) {
        let snap = match self.game.player_company.find_reactor_project(project_id) {
            Some(rp) => (rp.design.name.clone(), rp.design.scale),
            None => return,
        };
        let (name, scale) = snap;
        if let Some(rp) = self.game.player_company.find_reactor_project_mut(project_id) {
            rp.apply_edit(name, scale, enrichment);
        }
    }

    /// Reactor editor key handler. Cursor: 0 = Name, 1 = Scale,
    /// 2 = Enrichment. Left/Right adjusts the scalar on Scale (×√2)
    /// or cycles through reputation-unlocked enrichments. Enter opens
    /// a sub-modal on Name/Scale. D promotes a Proposed reactor to
    /// InDesign and closes the modal; Esc closes — deleting the
    /// project if it was still Proposed.
    fn handle_reactor_editor_key(
        &mut self,
        key: KeyCode,
        project_id: crate::reactor_project::ReactorProjectId,
        mut cursor: usize,
    ) {
        use crate::reactor::available_enrichments;
        use crate::reactor_project::ReactorDesignStatus;
        const ROW_COUNT: usize = 3; // Name, Scale, Enrichment

        // If the project disappeared underneath us, bail cleanly.
        let snap = match self.game.player_company.find_reactor_project(project_id) {
            Some(rp) => (rp.design.name.clone(), rp.design.scale, rp.design.enrichment,
                         matches!(rp.status, ReactorDesignStatus::Proposed { .. })),
            None => {
                self.exit_modal();
                return;
            }
        };
        let (name, scale, enrichment, is_proposed) = snap;

        if cursor >= ROW_COUNT { cursor = ROW_COUNT - 1; }

        match key {
            KeyCode::Esc => {
                // Cancel: drop a draft we created; leave real work
                // alone. Proposed reactors only ever exist for the
                // lifetime of the editor session that birthed them.
                if is_proposed {
                    self.game.player_company.delete_proposed_reactor(project_id);
                }
                self.exit_modal();
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // Done: promote Proposed → InDesign and log the event,
                // then close. No-op for projects already past Proposed
                // (they only land here via the "edit existing" path).
                if let Some(rname) = self.game.player_company.promote_proposed_reactor(project_id) {
                    let evt = crate::event::GameEvent::ReactorDesignStarted {
                        reactor_name: rname,
                    };
                    self.game.event_log.push(self.game.date, evt);
                }
                self.exit_modal();
            }
            KeyCode::Up => {
                if cursor > 0 { cursor -= 1; }
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
            }
            KeyCode::Down => {
                if cursor + 1 < ROW_COUNT { cursor += 1; }
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
            }
            KeyCode::Enter if cursor == 0 => {
                self.input_mode = InputMode::ReactorEditorNameInput {
                    project_id, cursor, buffer: name,
                };
            }
            KeyCode::Enter if cursor == 1 => {
                self.input_mode = InputMode::ReactorEditorScaleInput {
                    project_id, cursor, buffer: format!("{:.2}", scale),
                };
            }
            KeyCode::Right if cursor == 1 => {
                let new_scale = (scale * std::f64::consts::SQRT_2)
                    .min(crate::reactor::MAX_SCALE);
                self.apply_reactor_scale(project_id, new_scale);
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
            }
            KeyCode::Left if cursor == 1 => {
                let new_scale = (scale / std::f64::consts::SQRT_2)
                    .max(crate::reactor::MIN_SCALE);
                self.apply_reactor_scale(project_id, new_scale);
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
            }
            KeyCode::Left | KeyCode::Right if cursor == 2 => {
                // Cycle through reputation-unlocked enrichments. The
                // current enrichment is always considered "in the list"
                // even if reputation has since fallen below the gate,
                // so a player who built an HEU reactor doesn't get the
                // editor refusing to display HEU when re-opened later.
                let reputation = self.game.player_company.reputation.total();
                let mut levels = available_enrichments(reputation);
                if !levels.contains(&enrichment) {
                    levels.push(enrichment);
                    levels.sort_by_key(|e| *e as u32);
                }
                if levels.len() > 1 {
                    let next = wrap_cycle(&levels, enrichment, matches!(key, KeyCode::Right))
                        .unwrap_or(enrichment);
                    self.apply_reactor_enrichment(project_id, next);
                }
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
            }
            _ => {
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
            }
        }
    }

    /// Engine editor key handler. Cursor walks: 0=Name, 1=Cycle,
    /// 2=Preset, 3=Scale, 4=Vacuum (when not vacuum-only).
    /// Left/Right cycles values on Cycle/Preset; +/- adjusts Scale by
    /// ×√2 (and clamps to [MIN_SCALE, MAX_SCALE]); Space toggles Vacuum;
    /// Enter on Name/Scale opens a text/number sub-modal.
    fn handle_engine_editor_key(
        &mut self,
        key: KeyCode,
        project_id: crate::engine_project::EngineProjectId,
        mut cursor: usize,
        mut state: Box<RocketDesignerState>,
    ) {
        let snap = match self.editor_snapshot(project_id) {
            Some(s) => s,
            None => {
                // Project disappeared (shouldn't happen) — bail back to designer.
                self.input_mode = InputMode::RocketDesigner { state };
                return;
            }
        };
        let (name, cycle, preset, scale, use_vacuum, vacuum_only) = snap;
        // Number of editable rows: hide the vacuum toggle when fixed.
        let row_count = if vacuum_only { 4 } else { 5 };
        if cursor >= row_count { cursor = row_count - 1; }

        match key {
            KeyCode::Esc => {
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Up => {
                if cursor > 0 { cursor -= 1; }
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Down => {
                if cursor + 1 < row_count { cursor += 1; }
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Enter if cursor == 0 => {
                self.input_mode = InputMode::EngineEditorNameInput {
                    project_id, cursor, buffer: name, state,
                };
            }
            KeyCode::Enter if cursor == 3 => {
                self.input_mode = InputMode::EngineEditorScaleInput {
                    project_id, cursor, buffer: format!("{:.2}", scale), state,
                };
            }
            KeyCode::Left | KeyCode::Right if cursor == 1 => {
                let cycles = available_engine_cycles(&self.game);
                let next = wrap_cycle(&cycles, cycle, matches!(key, KeyCode::Right))
                    .unwrap_or(cycle);
                // Keep current preset if it's still compatible with the
                // new cycle; otherwise pick the first compatible preset.
                let new_preset = if preset.compatible_cycles().contains(&next) {
                    preset
                } else {
                    PropellantPreset::ALL.iter()
                        .copied()
                        .find(|p| p.compatible_cycles().contains(&next))
                        .unwrap_or(preset)
                };
                let new_vacuum = if matches!(next,
                    EngineCycle::Expander | EngineCycle::NuclearThermal
                    | EngineCycle::ElectricPropulsion | EngineCycle::SolarSail) {
                    true
                } else { use_vacuum };
                if let Some(ep) = self.game.player_company.find_engine_project_mut(project_id) {
                    ep.apply_edit(name, next, new_preset, scale, new_vacuum);
                }
                sync_stages_to_projects(&mut state, &self.game.player_company);
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Left | KeyCode::Right if cursor == 2 => {
                let presets: Vec<PropellantPreset> = PropellantPreset::ALL.iter()
                    .filter(|p| p.compatible_cycles().contains(&cycle))
                    .copied()
                    .collect();
                let next = wrap_cycle(&presets, preset, matches!(key, KeyCode::Right))
                    .unwrap_or(preset);
                if let Some(ep) = self.game.player_company.find_engine_project_mut(project_id) {
                    ep.apply_edit(name, cycle, next, scale, use_vacuum);
                }
                sync_stages_to_projects(&mut state, &self.game.player_company);
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Right if cursor == 3 => {
                let new_scale = (scale * std::f64::consts::SQRT_2)
                    .min(crate::engine_project::MAX_SCALE);
                self.apply_engine_scale(project_id, new_scale);
                sync_stages_to_projects(&mut state, &self.game.player_company);
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Left if cursor == 3 => {
                let new_scale = (scale / std::f64::consts::SQRT_2)
                    .max(crate::engine_project::MIN_SCALE);
                self.apply_engine_scale(project_id, new_scale);
                sync_stages_to_projects(&mut state, &self.game.player_company);
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Left | KeyCode::Right if cursor == 4 && !vacuum_only => {
                if let Some(ep) = self.game.player_company.find_engine_project_mut(project_id) {
                    ep.apply_edit(name, cycle, preset, scale, !use_vacuum);
                }
                sync_stages_to_projects(&mut state, &self.game.player_company);
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            _ => {
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
        }
    }

    /// Power-source editor key handler. The cursor walks a merged list:
    /// rows 0..N for currently-equipped sources, then rows N..N+P for
    /// the preset-add menu. Space adds a preset; X/Del removes an
    /// equipped source. Esc returns to the designer.
    fn handle_power_editor_key(
        &mut self,
        key: KeyCode,
        mut state: Box<RocketDesignerState>,
        group_index: usize,
        stage_index: usize,
        mut cursor: usize,
    ) {
        // Sanity-bound the indices in case the design changed underneath
        // us (shouldn't, but be defensive).
        let stage = state.stage_groups
            .get(group_index)
            .and_then(|g| g.get(stage_index));
        if stage.is_none() {
            self.input_mode = InputMode::RocketDesigner { state };
            return;
        }
        let n_equipped = state.stage_groups[group_index][stage_index]
            .power_sources.len();
        // Reactor designs the player has researched at least to
        // Testing. Snapshot the design now so the cursor-region math
        // and the install step agree (no second borrow on Company).
        let player_reactor_designs: Vec<crate::reactor::ReactorDesign> =
            self.game.player_company.installable_reactor_projects()
                .map(|rp| rp.design.clone())
                .collect();
        let n_reactors = player_reactor_designs.len();
        // Filter the preset catalog to only those whose tech is unlocked.
        let available_presets: Vec<&crate::power::PowerPreset> =
            crate::power::power_presets().iter()
                .filter(|p| crate::power::preset_available(p, &self.game.technologies))
                .collect();
        let n_total = n_equipped + n_reactors + available_presets.len();
        // Cursor regions:
        //   [0, n_equipped)                          equipped sources
        //   [n_equipped, n_equipped + n_reactors)    player reactors
        //   [..n_total)                              presets
        let reactor_start = n_equipped;
        let preset_start = n_equipped + n_reactors;

        match key {
            KeyCode::Esc => {
                self.input_mode = InputMode::RocketDesigner { state };
                return;
            }
            KeyCode::Up => {
                if cursor > 0 { cursor -= 1; }
            }
            KeyCode::Down => {
                if cursor + 1 < n_total { cursor += 1; }
            }
            KeyCode::Char(' ') => {
                if cursor >= preset_start {
                    let pi = cursor - preset_start;
                    let preset = available_presets[pi];
                    // Solar-panel preset: size to the current stage's
                    // demand instead of using the placeholder closure.
                    let new_src = if preset.auto_size_solar {
                        crate::power::solar_panel_for_stage_demand(
                            &state.stage_groups[group_index][stage_index],
                        )
                    } else {
                        (preset.build)()
                    };
                    state.stage_groups[group_index][stage_index]
                        .power_sources.push(new_src);
                    cursor = n_equipped;
                } else if cursor >= reactor_start {
                    let ri = cursor - reactor_start;
                    let design = player_reactor_designs[ri].clone();
                    let new_src = crate::power::PowerSource::from_reactor_design(design);
                    state.stage_groups[group_index][stage_index]
                        .power_sources.push(new_src);
                    cursor = n_equipped;
                }
            }
            KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Delete => {
                if cursor < n_equipped {
                    state.stage_groups[group_index][stage_index]
                        .power_sources.remove(cursor);
                    let new_n_equipped = state.stage_groups[group_index][stage_index]
                        .power_sources.len();
                    if cursor >= new_n_equipped && cursor > 0 {
                        cursor -= 1;
                    }
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                // Resize a solar panel up by √2 (two presses = 2×).
                if cursor < n_equipped {
                    let src = &mut state.stage_groups[group_index][stage_index]
                        .power_sources[cursor];
                    if let crate::power::PowerSourceKind::SolarPanel { peak_w_at_1au } = src.kind {
                        src.resize_solar_panel(peak_w_at_1au * std::f64::consts::SQRT_2);
                    }
                }
            }
            KeyCode::Char('-') | KeyCode::Char('_') => {
                // Resize a solar panel down by 1/√2 (symmetric with +).
                if cursor < n_equipped {
                    let src = &mut state.stage_groups[group_index][stage_index]
                        .power_sources[cursor];
                    if let crate::power::PowerSourceKind::SolarPanel { peak_w_at_1au } = src.kind {
                        src.resize_solar_panel(
                            (peak_w_at_1au / std::f64::consts::SQRT_2).max(1.0),
                        );
                    }
                }
            }
            _ => {}
        }
        self.input_mode = InputMode::PowerEditor {
            state, group_index, stage_index, cursor,
        };
    }

    /// Build the list of engines pickable in the rocket designer.
    /// Includes every player engine project (regardless of design /
    /// testing / revising status) so rocket designers can be started in
    /// parallel with engine design — both can sit in `InDesign` while
    /// teams work on each. Manufacturing still gates on the rocket
    /// reaching `Testing`; by that point the engine has typically caught
    /// up, but the design phase can run concurrently.
    pub fn available_engines(&self) -> Vec<(EngineSource, EngineDesign)> {
        let mut engines: Vec<(EngineSource, EngineDesign)> = Vec::new();
        for ep in &self.game.player_company.engine_projects {
            engines.push((EngineSource::PlayerDesign(ep.project_id), ep.design.clone()));
        }
        for ce in &self.game.player_company.contracted_engines {
            engines.push((EngineSource::Contracted(ce.id), ce.design.clone()));
        }
        engines
    }

    fn handle_up(&mut self) {
        match self.focused_pane {
            FocusedPane::Sidebar => {
                if self.active_tab > 0 {
                    self.active_tab -= 1;
                    self.content_scroll = 0;
                    self.selected_item = 0;
                }
            }
            FocusedPane::Content => {
                match self.current_tab() {
                    tab if tab.is_list_tab() => {
                        self.selected_item = self.selected_item.saturating_sub(1);
                    }
                    _ => {
                        self.content_scroll = self.content_scroll.saturating_sub(1);
                    }
                }
            }
        }
    }

    fn handle_down(&mut self) {
        match self.focused_pane {
            FocusedPane::Sidebar => {
                if self.active_tab + 1 < Tab::ALL.len() {
                    self.active_tab += 1;
                    self.content_scroll = 0;
                    self.selected_item = 0;
                }
            }
            FocusedPane::Content => {
                match self.current_tab() {
                    Tab::Engines => {
                        // Bound by the visible (non-Proposed) count, since
                        // selected_item indexes the displayed list.
                        let max = self.game.player_company.visible_engine_projects()
                            .count().saturating_sub(1);
                        if self.selected_item < max {
                            self.selected_item += 1;
                        }
                    }
                    Tab::Reactors => {
                        let max = self.game.player_company.visible_reactor_projects()
                            .count().saturating_sub(1);
                        if self.selected_item < max {
                            self.selected_item += 1;
                        }
                    }
                    Tab::Rockets => {
                        let max = self.game.player_company.rocket_projects.len().saturating_sub(1);
                        if self.selected_item < max {
                            self.selected_item += 1;
                        }
                    }
                    Tab::Manufacturing => {
                        let max = self.game.player_company.manufacturing.orders.len().saturating_sub(1);
                        if self.selected_item < max {
                            self.selected_item += 1;
                        }
                    }
                    Tab::Contracts => {
                        let avail = self.game.available_contracts.len();
                        let accepted = self.game.player_company.active_contracts.len();
                        let max = (avail + accepted).saturating_sub(1);
                        if self.selected_item < max {
                            self.selected_item += 1;
                        }
                    }
                    Tab::Launches => {
                        let max = self.game.player_company.manufacturing.inventory.rockets.len().saturating_sub(1);
                        if self.selected_item < max {
                            self.selected_item += 1;
                        }
                    }
                    _ => {
                        self.content_scroll += 1;
                    }
                }
            }
        }
    }

    /// Create a rocket project from the designer flow.
    fn create_rocket_project(&mut self, name: String, stage_groups: Vec<Vec<Stage>>) {
        use crate::rocket::{RocketDesign, RocketDesignId};

        let design_id = RocketDesignId(self.game.player_company.next_rocket_project_id);
        let design = RocketDesign {
            id: design_id,
            name: name.clone(),
            stage_groups,
        };

        if let Some(evt) = self.game.player_company.start_rocket_project(design) {
            self.game.event_log.push(self.game.date, evt);
            self.status_message = Some(format!("Started rocket design: {}", name));
        }
    }

    fn save_game(&mut self) {
        let path = save::save_path(&self.game.player_company.name);
        match save::save_game(&self.game, &path) {
            Ok(()) => {
                self.status_message = Some(format!("Saved to {}", path.display()));
            }
            Err(e) => {
                self.status_message = Some(format!("Save failed: {}", e));
            }
        }
    }
}

#[cfg(test)]
mod sync_tests {
    use super::*;
    use crate::engine::EngineId;
    use crate::engine_project::{EngineProject, EngineProjectId, PropellantPreset};
    use crate::stage::StageId;

    /// The original bug: opening "+ Design new engine…" in the rocket
    /// designer creates a kerolox engine snapshot on the stage. Switching
    /// the cycle to ElectricPropulsion inside the editor updates the
    /// engine project, but until sync_stages_to_projects runs, the
    /// stage's `engine` clone is stale — so the simulation sees the
    /// kerolox thrust + zero power_draw_w and the player gets a wildly
    /// wrong TWR. This test pins that the sync helper fixes it.
    #[test]
    fn sync_refreshes_stage_engine_after_project_edit() {
        let mut company = crate::game_state::Company::new(
            "Test".into(), 10_000_000.0, &crate::seed::GameSeed::new(1),
        );
        // Player designs a kerolox engine.
        let ep = EngineProject::new(
            EngineProjectId(1), EngineId(1), "E1".into(),
            EngineCycle::GasGenerator, PropellantPreset::Kerolox,
            1.0, false,
        ).unwrap();
        company.engine_projects.push(ep);

        // Simulate the rocket designer having installed a stage with a
        // clone of the kerolox design.
        let stage = Stage {
            id: StageId(1),
            name: "S1".into(),
            engine: company.engine_projects[0].design.clone(),
            engine_count: 1,
            propellant_mass_kg: 40_000.0,
            structural_mass_kg: 100.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        let mut state = RocketDesignerState {
            mode: DesignerMode::New,
            rocket_name: "R1".into(),
            stage_groups: vec![vec![stage]],
            engine_sources: vec![vec![EngineSource::PlayerDesign(EngineProjectId(1))]],
            next_stage_id: 2,
            selected_group: 0,
            selected_inner: 0,
            payload_kg: 0.0,
            launch_from: "lc-39",
            destination: "leo",
            created_engine_projects: Vec::new(),
        };

        // Player opens the editor, switches cycle to ElectricPropulsion.
        company.engine_projects[0].apply_edit(
            "E1".into(),
            EngineCycle::ElectricPropulsion,
            PropellantPreset::Xenon,
            1.0,
            true,
        );

        // Before sync: stage still has kerolox numbers.
        assert!(state.stage_groups[0][0].engine.thrust_n > 100_000.0,
            "stage engine should still be kerolox-sized before sync");
        assert_eq!(state.stage_groups[0][0].engine.power_draw_w, 0.0,
            "stage engine should have zero power draw before sync");
        let kerolox_prop = state.stage_groups[0][0].propellant_mass_kg;

        sync_stages_to_projects(&mut state, &company);

        // After sync: stage reflects the ion design.
        assert!(state.stage_groups[0][0].engine.thrust_n < 100.0,
            "stage engine thrust should be ~1 N (ion) after sync, got {}",
            state.stage_groups[0][0].engine.thrust_n);
        assert!(state.stage_groups[0][0].engine.power_draw_w > 1000.0,
            "stage engine power_draw_w should reflect ion engine after sync, got {}",
            state.stage_groups[0][0].engine.power_draw_w);
        // Propellant should drop drastically — ion engines have a tiny
        // mass flow rate, so a 120-second burn needs grams, not tonnes.
        let ion_prop = state.stage_groups[0][0].propellant_mass_kg;
        assert!(ion_prop < 1.0,
            "ion-engine propellant should be << 1 kg for a 120 s burn, got {} kg (was {} kg)",
            ion_prop, kerolox_prop);
    }
}
