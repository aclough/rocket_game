//! The rocket designer: its state and the key handling for building a
//! vehicle stage by stage (17_REFACTOR.md E1). The sizing physics it
//! calls — tank autosizer, structural masses, propellant step — lives
//! in `stage.rs` (17_3_PHYSICS.md D5).

use super::*;
use super::text_field::{edit_text_field, FieldEdit, FieldKind};
use crate::stage::{
    autosize_propellant, propellant_step, recompute_structural_masses, MIN_AUTOSIZED_PROPELLANT,
};

/// What the designer has open over itself (17_4_UI.md step 5 / F6).
/// The design state stays on `InputMode::RocketDesigner`; these carry
/// only the sub-modal's own cursor and buffer.
#[derive(Debug, Clone)]
pub enum DesignerSubMode {
    /// Nothing: keys go to the designer.
    Main,
    /// The engine list for a stage slot.
    PickEngine(EnginePick),
    /// Typing the payload mass.
    PayloadInput { buffer: String },
    /// Picking the launch site or the mission destination.
    LocationPicker {
        target: LocationPickerTarget,
        locations: Vec<(&'static str, &'static str)>,
        selected: usize,
    },
    /// The per-stage power-source editor. Cursor walks a merged list of
    /// equipped sources then presets to add; Space adds, X/Del removes.
    PowerEditor { group_index: usize, stage_index: usize, cursor: usize },
    /// The designer's key reference.
    Help,
}

impl DesignerSubMode {
    /// The engine picker for `slot`, cursor at the top.
    pub fn pick(slot: PickSlot) -> DesignerSubMode {
        DesignerSubMode::PickEngine(EnginePick { slot, selected: 0 })
    }
}

/// Where a picked engine goes. Group 0 is the bottom of the stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickSlot {
    /// A new group on top of the stack.
    Append,
    /// A new group at index `gi`; the groups from there up move up one.
    InsertAt(usize),
    /// A parallel (booster) stage in group `gi`.
    BoosterFor(usize),
    /// Replace stage `si` of group `gi`.
    Replace(usize, usize),
}

impl PickSlot {
    /// The group the engine will sit in, given the stack's current
    /// group count — the bottom group gets the sea-level bell.
    pub fn group_index(self, group_count: usize) -> usize {
        match self {
            PickSlot::Append => group_count,
            PickSlot::InsertAt(gi) | PickSlot::BoosterFor(gi) | PickSlot::Replace(gi, _) => gi,
        }
    }
}

/// The engine picker: which slot it is filling and where its cursor is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnginePick {
    pub slot: PickSlot,
    pub selected: usize,
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
    pub(super) fn new(name: String) -> Self {
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
    pub(super) fn total_stages(&self) -> usize {
        self.stage_groups.iter().map(|g| g.len()).sum()
    }

    /// True if any stage in the design uses a low-thrust engine.
    /// Low-thrust engines (ion drives) are restricted to single-stage
    /// designs — booster duty is handled by carrying the ion rocket as a
    /// payload on a separate chemical rocket.
    pub(super) fn has_low_thrust_stage(&self) -> bool {
        self.stage_groups.iter().flatten()
            .any(|s| s.engine.is_low_thrust())
    }

    /// Whether the selection cursor is on the "add stage" slot.
    pub(super) fn on_add_slot(&self) -> bool {
        self.selected_group >= self.stage_groups.len()
    }

    /// Flat index of the current selection (0-based across all inner stages).
    pub(super) fn flat_index(&self) -> usize {
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
    pub(super) fn select_flat(&mut self, flat: usize) {
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
    pub(super) fn stage_name(group_index: usize, inner_index: usize, group_len: usize) -> String {
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
/// Refresh every player-designed stage engine from its source engine
/// project, then re-solve every tank (the same way `autosize_propellant`
/// sizes a fresh stage) and recompute structural masses (which depend on
/// engine mass). Call
/// after any engine-editor mutation that touched a project's
/// `EngineDesign` so the rocket designer's thrust / Isp / power_draw_w
/// — and the propellant load that determines burn time — don't go
/// stale against the project's current numbers.
///
/// Mirrors the lockstep invariant: `stage_groups[gi][si]` and
/// `engine_sources[gi][si]` always describe the same stage, so we walk
/// them in parallel.
pub(super) fn sync_stages_to_projects(state: &mut RocketDesignerState, company: &crate::game_state::Company) {
    for (gi, (group, sources)) in state.stage_groups.iter_mut()
        .zip(state.engine_sources.iter())
        .enumerate()
    {
        for (stage, source) in group.iter_mut().zip(sources.iter()) {
            if let EngineSource::PlayerDesign(pid) = source {
                if let Some(ep) = company.engine_projects.iter()
                    .find(|ep| ep.project_id == *pid)
                {
                    // Keep the nozzle this stage was built around —
                    // editing the engine family must not silently swap
                    // an upper stage back to a sea-level bell. But only a
                    // family that *offered* a choice recorded one: if the
                    // stage's engine came from a vacuum-only family (the
                    // editor was paged through expander and back), its
                    // flag says nothing, and the stage gets the default
                    // for its position — sea level on the pad, vacuum
                    // above.
                    let want_vacuum = if crate::engine_project::design_has_nozzle_choice(&stage.engine) {
                        stage.engine.is_vacuum_variant()
                    } else {
                        gi > 0
                    };
                    stage.engine = ep.design_variant(want_vacuum);
                }
            }
        }
    }
    // Re-size every tank the same way the engine picker sizes a fresh
    // stage, so swapping cycle (e.g. Kerolox → Ion) doesn't strand a
    // kerolox-sized tank on an ion engine. Done in a second pass because
    // each stage is sized against the mass of the others, which the first
    // pass is still changing.
    resize_all_tanks(state);
}

/// Re-solve every tank in the stack, top-down.
///
/// Each stage is sized against the mass it has to lift, so the topmost
/// group — which lifts only the payload — is the one that can be solved
/// without knowing anything else. Working downwards from there, every
/// group already knows what sits above it by the time its turn comes.
/// Sizing bottom-up instead would have the booster guess at an upper
/// stage that hasn't been sized yet.
///
/// The repeat pass is for stages sharing a group: side boosters fire
/// alongside each other, so each one's answer moves the others'. Groups
/// alone would settle in a single sweep.
fn resize_all_tanks(state: &mut RocketDesignerState) {
    if let Some(top) = state.stage_groups.len().checked_sub(1) {
        resize_tanks_through(state, top);
    }
}

/// Re-solve the tanks of group `top` and every group beneath it.
///
/// Used when a stage is added or replaced: everything below it now has
/// more (or less) to lift and can no longer be at its target
/// thrust-to-weight, while everything *above* it is carrying exactly what
/// it was before. Leaving the upper groups alone keeps whatever the
/// player set there by hand — adding a booster is not a request to
/// redesign the spacecraft on top of it.
fn resize_tanks_through(state: &mut RocketDesignerState, top: usize) {
    /// Enough for the sizes to stop moving in practice; the loop exits
    /// early once they do.
    const MAX_PASSES: usize = 12;
    /// Settled when no tank moves by more than this in a whole pass.
    const SETTLED_KG: f64 = 1.0;

    let top = top.min(state.stage_groups.len().saturating_sub(1));
    for _ in 0..MAX_PASSES {
        let mut moved: f64 = 0.0;
        for gi in (0..=top).rev() {
            for si in 0..state.stage_groups[gi].len() {
                let sized = autosize_propellant(
                    &state.stage_groups, gi, si, state.payload_kg, state.launch_from,
                );
                moved = moved.max(
                    (sized - state.stage_groups[gi][si].propellant_mass_kg).abs(),
                );
                state.stage_groups[gi][si].propellant_mass_kg = sized;
                recompute_structural_masses(&mut state.stage_groups);
            }
        }
        if moved <= SETTLED_KG {
            break;
        }
    }
}

/// Rename every stage based on its position in `stage_groups`, using
/// the project's S1 / S1a / S1b conventions.
pub(super) fn rename_all_stages(stage_groups: &mut [Vec<Stage>]) {
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
    slot: PickSlot,
) {
    let engine_count = 1u32;
    // Placeholder — the real load is solved once the stage is in place and
    // the solver can see what it has to lift.
    let propellant_mass_kg = MIN_AUTOSIZED_PROPELLANT;
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

    match slot {
        PickSlot::Replace(gi, si) => {
            state.replace_stage(gi, si, stage, source);
            state.selected_group = gi;
            state.selected_inner = si;
        }
        PickSlot::BoosterFor(gi) => {
            state.push_to_group(gi, stage, source);
            state.selected_group = gi;
            state.selected_inner = state.stage_groups[gi].len() - 1;
        }
        PickSlot::InsertAt(gi) => {
            state.insert_new_group_at(gi, stage, source);
            state.selected_group = gi;
            state.selected_inner = 0;
        }
        PickSlot::Append => {
            state.push_new_group(stage, source);
            state.selected_group = state.stage_groups.len() - 1;
            state.selected_inner = 0;
        }
    }

    rename_all_stages(&mut state.stage_groups);
    recompute_structural_masses(&mut state.stage_groups);

    // Size the stage that was just placed and everything under it. A new
    // upper stage is new mass for the booster to lift, so the booster
    // can't still be at its target thrust-to-weight; stacking on top of a
    // rocket without re-solving what's underneath is how you get the
    // undersized-first-stage trap. Groups above are left as the player
    // had them — their own load hasn't changed.
    if !state.stage_groups.is_empty() {
        resize_tanks_through(state, state.selected_group);
    }
}

impl App {
    pub(super) fn handle_rocket_designer_key(&mut self, key: KeyCode, mut state: Box<RocketDesignerState>) {
        // Clear status message on any keypress in designer
        self.status_message = None;
        match key {
            KeyCode::Up => {
                let flat = state.flat_index();
                if flat > 0 {
                    state.select_flat(flat - 1);
                }
                self.input_mode = InputMode::designer(state);
            }
            KeyCode::Down => {
                let flat = state.flat_index();
                if flat < state.total_stages() {
                    state.select_flat(flat + 1);
                }
                self.input_mode = InputMode::designer(state);
            }
            KeyCode::Enter => {
                if state.is_modify() {
                    self.status_message = Some(
                        "Stage layout fixed in Modify mode — only propellant / power editable".into());
                    self.input_mode = InputMode::designer(state);
                } else if state.on_add_slot() {
                    // Same as 'a' — add stage at end
                    if state.has_low_thrust_stage() {
                        self.status_message = Some(
                            "Low-thrust designs must be single-stage — carry as payload instead".into());
                        self.input_mode = InputMode::designer(state);
                    } else {
                        self.input_mode = InputMode::designer_with(state, DesignerSubMode::pick(PickSlot::Append));
                    }
                } else {
                    // Edit the selected inner stage
                    let gi = state.selected_group;
                    let si = state.selected_inner;
                    self.input_mode = InputMode::designer_with(state, DesignerSubMode::pick(PickSlot::Replace(gi, si)));
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
                self.input_mode = InputMode::designer(state);
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
                self.input_mode = InputMode::designer(state);
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                // Increase propellant by thrust-scaled step (not for solid engines)
                if !state.on_add_slot() {
                    let gi = state.selected_group;
                    let si = state.selected_inner;
                    let stage = &mut state.stage_groups[gi][si];
                    if stage.engine.is_solid() {
                        self.status_message = Some("Solid propellant is not adjustable".into());
                    } else {
                        let step = propellant_step(&stage.engine, stage.engine_count);
                        stage.propellant_mass_kg = (stage.propellant_mass_kg + step).min(2_000_000.0);
                        recompute_structural_masses(&mut state.stage_groups);
                    }
                }
                self.input_mode = InputMode::designer(state);
            }
            KeyCode::Char('-') => {
                // Decrease propellant by thrust-scaled step (not for solid engines)
                if !state.on_add_slot() {
                    let gi = state.selected_group;
                    let si = state.selected_inner;
                    let stage = &mut state.stage_groups[gi][si];
                    if stage.engine.is_solid() {
                        self.status_message = Some("Solid propellant is not adjustable".into());
                    } else {
                        let step = propellant_step(&stage.engine, stage.engine_count);
                        stage.propellant_mass_kg = (stage.propellant_mass_kg - step).max(100.0);
                        recompute_structural_masses(&mut state.stage_groups);
                    }
                }
                self.input_mode = InputMode::designer(state);
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                // Add stage at end (new group)
                if state.is_modify() {
                    self.status_message = Some(
                        "Stage layout fixed in Modify mode".into());
                    self.input_mode = InputMode::designer(state);
                } else if state.has_low_thrust_stage() {
                    self.status_message = Some(
                        "Low-thrust designs must be single-stage — carry as payload instead".into());
                    self.input_mode = InputMode::designer(state);
                } else {
                    self.input_mode = InputMode::designer_with(state, DesignerSubMode::pick(PickSlot::Append));
                }
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                // Open the power-source editor for the currently-selected
                // stage. No-op when on the "add stage" sentinel slot.
                if !state.on_add_slot() {
                    let group_index = state.selected_group;
                    let stage_index = state.selected_inner;
                    self.input_mode = InputMode::designer_with(
                        state, DesignerSubMode::PowerEditor { group_index, stage_index, cursor: 0 },
                    );
                } else {
                    self.input_mode = InputMode::designer(state);
                }
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                // Insert stage before selected group
                if state.is_modify() {
                    self.status_message = Some(
                        "Stage layout fixed in Modify mode".into());
                    self.input_mode = InputMode::designer(state);
                } else if !state.on_add_slot() {
                    if state.has_low_thrust_stage() {
                        self.status_message = Some(
                            "Low-thrust designs must be single-stage — carry as payload instead".into());
                        self.input_mode = InputMode::designer(state);
                    } else {
                        let idx = state.selected_group;
                        self.input_mode = InputMode::designer_with(state, DesignerSubMode::pick(PickSlot::InsertAt(idx)));
                    }
                } else {
                    self.input_mode = InputMode::designer(state);
                }
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                // Add booster (parallel stage) to current group
                if state.is_modify() {
                    self.status_message = Some(
                        "Stage layout fixed in Modify mode".into());
                    self.input_mode = InputMode::designer(state);
                } else if !state.on_add_slot() {
                    if state.has_low_thrust_stage() {
                        self.status_message = Some(
                            "Low-thrust designs must be single-stage — carry as payload instead".into());
                        self.input_mode = InputMode::designer(state);
                    } else {
                        let gi = state.selected_group;
                        self.input_mode = InputMode::designer_with(state, DesignerSubMode::pick(PickSlot::BoosterFor(gi)));
                    }
                } else {
                    self.input_mode = InputMode::designer(state);
                }
            }
            KeyCode::Char('?') => {
                self.input_mode = InputMode::designer_with(state, DesignerSubMode::Help);
            }
            KeyCode::Char('v') | KeyCode::Char('V') => {
                // Toggle the selected stage between the sea-level and
                // vacuum bell of its engine family. Both come from one
                // engine project — the nozzle is fitted at stage
                // integration, so there is nothing extra to develop.
                if state.on_add_slot() || state.stage_groups.is_empty() {
                    self.input_mode = InputMode::designer(state);
                    return;
                }
                let gi = state.selected_group;
                let si = state.selected_inner;
                let source = state.engine_sources[gi][si];
                match source {
                    EngineSource::PlayerDesign(pid) => {
                        let want_vacuum = !state.stage_groups[gi][si]
                            .engine.is_vacuum_variant();
                        let swapped = self.game.player_company
                            .find_engine_project(pid)
                            .filter(|ep| ep.has_nozzle_choice())
                            .map(|ep| ep.design_variant(want_vacuum));
                        match swapped {
                            Some(engine) => {
                                state.stage_groups[gi][si].engine = engine;
                                recompute_structural_masses(&mut state.stage_groups);
                                self.status_message = Some(format!(
                                    "{} nozzle fitted",
                                    if want_vacuum { "Vacuum" } else { "Sea-level" },
                                ));
                            }
                            None => {
                                self.status_message = Some(
                                    "This engine only works in vacuum".into());
                            }
                        }
                    }
                    EngineSource::Contracted(_) => {
                        self.status_message = Some(
                            "Third-party engines come with a fixed nozzle".into());
                    }
                }
                self.input_mode = InputMode::designer(state);
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                // Remove selected inner stage
                if state.is_modify() {
                    self.status_message = Some(
                        "Stage layout fixed in Modify mode".into());
                    self.input_mode = InputMode::designer(state);
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
                self.input_mode = InputMode::designer(state);
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                // Set payload
                let buffer = format!("{}", state.payload_kg as u64);
                self.input_mode = InputMode::designer_with(
                    state, DesignerSubMode::PayloadInput { buffer },
                );
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                // Pick launch site
                let locations: Vec<(&'static str, &'static str)> = DELTA_V_MAP.locations().iter()
                    .map(|loc| (loc.id, loc.display_name))
                    .collect();
                let selected = locations.iter().position(|(id, _)| *id == state.launch_from).unwrap_or(0);
                self.input_mode = InputMode::designer_with(state, DesignerSubMode::LocationPicker {
                    target: LocationPickerTarget::LaunchSite, locations, selected,
                });
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
                // Pick mission destination
                let locations: Vec<(&'static str, &'static str)> = DELTA_V_MAP.locations().iter()
                    .map(|loc| (loc.id, loc.display_name))
                    .collect();
                let selected = locations.iter().position(|(id, _)| *id == state.destination).unwrap_or(0);
                self.input_mode = InputMode::designer_with(state, DesignerSubMode::LocationPicker {
                    target: LocationPickerTarget::MissionDestination, locations, selected,
                });
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // Done — finalize design
                if state.stage_groups.is_empty() {
                    self.status_message = Some("Must add at least one stage".into());
                    self.input_mode = InputMode::designer(state);
                } else if let DesignerMode::Modify { project_id } = state.mode {
                    // Modify mode: rewrite the existing project's
                    // stages and roll for a new flaw.
                    let stage_groups = state.stage_groups.clone();
                    self.exit_modal();
                    if let Some(evt) = self.game.apply_rocket_modification(project_id, stage_groups) {
                        let summary = format!("{}", evt);
                        self.game.log(evt);
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
                                .promote_proposed(ProjectRef::Engine(*id))
                            {
                                self.game.log(crate::event::GameEvent::project(
                                    crate::project::ProjectKind::Engine, engine_name,
                                    crate::event::ProjectEvent::DesignStarted,
                                ));
                            }
                        } else {
                            self.game.player_company.delete_proposed(ProjectRef::Engine(*id));
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
                    self.game.player_company.delete_proposed(ProjectRef::Engine(id));
                }
                self.status_message = Some("Rocket design cancelled".into());
            }
            _ => {
                self.input_mode = InputMode::designer(state);
            }
        }
    }

    pub(super) fn handle_rocket_pick_engine_key(
        &mut self,
        key: KeyCode,
        mut state: Box<RocketDesignerState>,
        mut pick: EnginePick,
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
                self.input_mode = InputMode::designer(state);
            }
            KeyCode::Up => {
                cursor_up(&mut pick.selected);
                self.input_mode = InputMode::designer_with(state, DesignerSubMode::PickEngine(pick));
            }
            KeyCode::Down => {
                cursor_down(&mut pick.selected, total_rows);
                self.input_mode = InputMode::designer_with(state, DesignerSubMode::PickEngine(pick));
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                // Open the engine editor on the highlighted player engine.
                // Only editable while Proposed, InDesign, or Revising.
                if pick.selected < num_engines {
                    if let EngineSource::PlayerDesign(pid) = engines[pick.selected].0 {
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
                                project_id: pid, cursor: 0, state: Some(state),
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
                self.input_mode = InputMode::designer_with(state, DesignerSubMode::PickEngine(pick));
            }
            KeyCode::Enter => {
                if pick.selected == new_engine_idx {
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
                    let tech_id = crate::technology::technology_for_preset(preset);
                    let project_id = match self.game.player_company
                        .start_proposed_engine_project(
                            default_name, cycle, preset, scale, tech_id,
                            &self.game.balance,
                        )
                    {
                        Some(id) => id,
                        None => {
                            self.status_message = Some("Failed to create engine".into());
                            self.input_mode = InputMode::designer_with(state, DesignerSubMode::PickEngine(pick));
                            return;
                        }
                    };
                    state.created_engine_projects.push(project_id);
                    // Apply the engine to the target stage now so the
                    // player sees its effect as they edit. Re-use the
                    // same plumbing as the engine-pick branch by
                    // looking the engine back up.
                    let group_index = pick.slot.group_index(state.stage_groups.len());
                    let engine = self.game.player_company
                        .find_engine_project(project_id)
                        .map(|ep| ep.design_variant(group_index > 0));
                    if let Some(engine) = engine {
                        apply_picked_engine_to_designer(
                            &mut state, EngineSource::PlayerDesign(project_id), engine, pick.slot,
                        );
                    }
                    self.input_mode = InputMode::EngineEditor {
                        project_id, cursor: 0, state: Some(state),
                    };
                } else if num_engines == 0 {
                    self.status_message = Some("No engines available".into());
                    self.input_mode = InputMode::designer_with(state, DesignerSubMode::PickEngine(pick));
                } else {
                    let (source, engine) = engines[pick.selected].clone();
                    // Enforce: low-thrust engines may only appear in a
                    // single-stage design. The 'a'/'i'/'b'/Enter gates
                    // already block adding to a low-thrust design; this
                    // also catches editing a stage in a multi-stage
                    // design to a low-thrust engine.
                    let other_stages = state.total_stages()
                        .saturating_sub(usize::from(matches!(pick.slot, PickSlot::Replace(..))));
                    if engine.is_low_thrust() && other_stages > 0 {
                        self.status_message = Some(
                            "Low-thrust engines must be in a single-stage design".into());
                        self.input_mode = InputMode::designer(state);
                        return;
                    }
                    // Bottom group flies the sea-level bell, upper
                    // stages the vacuum one; [V] overrides per stage.
                    let group_index = pick.slot.group_index(state.stage_groups.len());
                    let engine = self.engine_for_placement(
                        source, engine, group_index);
                    apply_picked_engine_to_designer(&mut state, source, engine, pick.slot);
                    self.input_mode = InputMode::designer(state);
                }
            }
            _ => {
                self.input_mode = InputMode::designer_with(state, DesignerSubMode::PickEngine(pick));
            }
        }
    }

    pub(super) fn handle_rocket_payload_input_key(
        &mut self,
        key: KeyCode,
        mut state: Box<RocketDesignerState>,
        mut buffer: String,
    ) {
        match edit_text_field(key, &mut buffer, FieldKind::Number) {
            FieldEdit::Continue => {
                self.input_mode = InputMode::designer_with(state, DesignerSubMode::PayloadInput { buffer });
            }
            FieldEdit::Commit => {
                if let Ok(val) = buffer.parse::<f64>() {
                    state.payload_kg = val.max(0.0);
                }
                self.input_mode = InputMode::designer(state);
            }
            FieldEdit::Cancel => {
                self.input_mode = InputMode::designer(state);
            }
        }
    }

    pub(super) fn handle_rocket_designer_location_picker_key(
        &mut self,
        key: KeyCode,
        mut state: Box<RocketDesignerState>,
        target: LocationPickerTarget,
        locations: Vec<(&'static str, &'static str)>,
        mut selected: usize,
    ) {
        match key {
            KeyCode::Esc => {
                self.input_mode = InputMode::designer(state);
            }
            KeyCode::Up => {
                cursor_up(&mut selected);
                self.input_mode = InputMode::designer_with(state, DesignerSubMode::LocationPicker { target, locations, selected });
            }
            KeyCode::Down => {
                cursor_down(&mut selected, locations.len());
                self.input_mode = InputMode::designer_with(state, DesignerSubMode::LocationPicker { target, locations, selected });
            }
            KeyCode::Enter => {
                if let Some((id, _)) = locations.get(selected) {
                    match target {
                        LocationPickerTarget::LaunchSite => state.launch_from = id,
                        LocationPickerTarget::MissionDestination => state.destination = id,
                    }
                }
                self.input_mode = InputMode::designer(state);
            }
            _ => {
                self.input_mode = InputMode::designer_with(state, DesignerSubMode::LocationPicker { target, locations, selected });
            }
        }
    }

    /// The nozzle a newly placed stage should fly. The bottom group
    /// lights on the pad and gets the sea-level bell; anything stacked
    /// above it ignites in near-vacuum and gets the vacuum bell. The
    /// player can override per stage with [V]. Third-party engines are
    /// taken as delivered.
    pub(super) fn engine_for_placement(
        &self,
        source: EngineSource,
        picked: EngineDesign,
        group_index: usize,
    ) -> EngineDesign {
        match source {
            EngineSource::PlayerDesign(pid) => self.game.player_company
                .find_engine_project(pid)
                .map(|ep| ep.design_variant(group_index > 0))
                .unwrap_or(picked),
            EngineSource::Contracted(_) => picked,
        }
    }

    /// Build the list of engines pickable in the rocket designer.
    /// Includes every player engine project (regardless of design /
    /// testing / revising status) so rocket designers can be started in
    /// parallel with engine design — both can sit in `InDesign` while
    /// teams work on each. Manufacturing still gates on the rocket
    /// reaching `Testing`; by that point the engine has typically caught
    /// up, but the design phase can run concurrently.
    ///
    /// Retired engines are left out: the point of retiring one is to
    /// stop reaching for it. Rocket designs that already use it keep
    /// working, since their stages hold a cloned `EngineDesign`.
    pub fn available_engines(&self) -> Vec<(EngineSource, EngineDesign)> {
        let mut engines: Vec<(EngineSource, EngineDesign)> = Vec::new();
        for ep in &self.game.player_company.engine_projects {
            if ep.retired {
                continue;
            }
            engines.push((EngineSource::PlayerDesign(ep.project_id), ep.design.clone()));
        }
        for ce in &self.game.player_company.contracted_engines {
            engines.push((EngineSource::Contracted(ce.id), ce.design.clone()));
        }
        engines
    }

    /// Create a rocket project from the designer flow.
    pub(super) fn create_rocket_project(&mut self, name: String, stage_groups: Vec<Vec<Stage>>) {
        if let Some(evt) = self.game.player_company
            .start_rocket_project(name.clone(), stage_groups, &self.game.balance)
        {
            self.game.log(evt);
            self.status_message = Some(format!("Started rocket design: {}", name));
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
            &crate::balance_config::BalanceConfig::default(),
        );
        // Player designs a kerolox engine.
        let ep = EngineProject::new(
            EngineProjectId(1), EngineId(1), "E1".into(),
            EngineCycle::GasGenerator, PropellantPreset::Kerolox,
            1.0,
            &crate::balance_config::BalanceConfig::default(),
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
            &crate::balance_config::BalanceConfig::default(),
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
        // The tank is re-solved rather than left at its kerolox size. This
        // stage sits in the first-stage slot, and a 1 N ion engine cannot
        // lift anything at all, so the solver bottoms out — which is the
        // honest answer, and the designer's TWR column says why. What
        // matters is that 40 tonnes of xenon didn't survive the swap.
        let ion_prop = state.stage_groups[0][0].propellant_mass_kg;
        assert_eq!(ion_prop, MIN_AUTOSIZED_PROPELLANT,
            "an ion engine in the booster slot can lift nothing, so its tank \
             should bottom out; got {ion_prop} kg (was {kerolox_prop} kg)");
    }
}

#[cfg(test)]
mod autosize_tests {
    use super::*;
    use crate::stage::{LOW_THRUST_DV_TARGET, TARGET_LIFTOFF_TWR, TARGET_STAGE_TWR};
    use crate::engine::{EngineId, PropellantFraction};
    use crate::rocket::RocketDesign;
    use crate::propellant::Propellant;

    fn engine(id: u64, thrust: f64, isp: f64, mass: f64, cycle: EngineCycle,
              prop: Propellant) -> EngineDesign {
        EngineDesign {
            id: EngineId(id), name: format!("E{id}"), cycle,
            thrust_n: thrust, mass_kg: mass, isp_s: isp,
            exit_pressure_pa: 70_000.0, needs_atmosphere: false, power_draw_w: 0.0,
            propellant_mix: vec![PropellantFraction { propellant: prop, mass_fraction: 1.0 }],
        }
    }

    fn stack(engines: Vec<EngineDesign>, payload: f64) -> RocketDesignerState {
        let n = engines.len();
        let mut st = RocketDesignerState::new("T".into());
        st.payload_kg = payload;
        for (i, e) in engines.into_iter().enumerate() {
            st.stage_groups.push(vec![Stage {
                id: StageId(i as u64 + 1), name: format!("S{}", i + 1),
                engine: e, engine_count: 1,
                propellant_mass_kg: 100.0, structural_mass_kg: 0.0,
                fairing: None, power_sources: Vec::new(),
            }]);
            st.engine_sources.push(vec![EngineSource::PlayerDesign(
                crate::engine_project::EngineProjectId(i as u64 + 1))]);
        }
        let _ = n;
        resize_all_tanks(&mut st);
        st
    }

    /// Liftoff thrust-to-weight is the whole point of the booster rule.
    /// The old flat burn-time default let a stage come out at any TWR at
    /// all — including below 1, where the rocket doesn't leave the pad.
    #[test]
    fn the_booster_is_sized_to_leave_the_pad() {
        let kerolox = |id, t| engine(id, t, 300.0, 1_500.0, EngineCycle::GasGenerator, Propellant::RP1);
        let hydrolox = |id, t| engine(id, t, 440.0, 800.0, EngineCycle::Expander, Propellant::LH2);

        for thrust in [1_000_000.0, 5_000_000.0, 20_000_000.0] {
            let st = stack(
                vec![kerolox(1, thrust), hydrolox(2, thrust / 10.0)], 2_000.0,
            );
            let design = RocketDesign {
                id: crate::rocket::RocketDesignId(1), name: "d".into(),
                stage_groups: st.stage_groups.clone(),
            };
            let twr = thrust / ((design.total_mass_kg() + st.payload_kg) * 9.81);
            assert!((twr - TARGET_LIFTOFF_TWR).abs() < 0.05,
                "a {thrust:.0} N booster should be sized to lift off at \
                 {TARGET_LIFTOFF_TWR}, got {twr:.2}");
        }
    }

    /// Every stage above the first is sized to keep accelerating at its
    /// own ignition, which is a property of that stage and what it lifts —
    /// not of the mission, and not of the designer's payload field.
    #[test]
    fn upper_stages_are_sized_to_keep_accelerating() {
        let st = stack(vec![
            engine(1, 5_000_000.0, 300.0, 1_500.0, EngineCycle::GasGenerator, Propellant::RP1),
            engine(2, 500_000.0, 440.0, 800.0, EngineCycle::Expander, Propellant::LH2),
            engine(3, 60_000.0, 440.0, 300.0, EngineCycle::Expander, Propellant::LH2),
        ], 2_000.0);
        let design = RocketDesign {
            id: crate::rocket::RocketDesignId(1), name: "d".into(),
            stage_groups: st.stage_groups.clone(),
        };
        let stats = crate::rocket::compute_stage_stats(&design, st.payload_kg, "earth_surface");
        assert!((stats[0].twr - TARGET_LIFTOFF_TWR).abs() < 0.05,
            "first stage should lift off at {TARGET_LIFTOFF_TWR}, got {:.2}", stats[0].twr);
        for (gi, s) in stats.iter().enumerate().skip(1) {
            assert!((s.twr - TARGET_STAGE_TWR).abs() < 0.05,
                "stage {gi} should ignite at {TARGET_STAGE_TWR}, got {:.2}", s.twr);
        }
    }

    /// Stacking a new stage on a rocket changes what everything under it
    /// has to lift, so those tanks can't still be at their target
    /// thrust-to-weight. Sizing only the stage you just placed is how the
    /// booster ends up undersized without anything saying so.
    #[test]
    fn adding_a_stage_re_solves_what_now_has_to_lift_it() {
        let mut st = stack(vec![
            engine(1, 5_000_000.0, 300.0, 1_500.0, EngineCycle::GasGenerator, Propellant::RP1),
        ], 2_000.0);
        let booster_alone = st.stage_groups[0][0].propellant_mass_kg;

        apply_picked_engine_to_designer(
            &mut st,
            EngineSource::PlayerDesign(crate::engine_project::EngineProjectId(2)),
            engine(2, 500_000.0, 440.0, 800.0, EngineCycle::Expander, Propellant::LH2),
            PickSlot::Append,
        );
        assert_eq!(st.stage_groups.len(), 2, "the new stage should be its own group");

        let booster_now = st.stage_groups[0][0].propellant_mass_kg;
        assert!(booster_now < booster_alone,
            "the booster now lifts an upper stage too, so its tank has to give \
             way for it; went {booster_alone:.0} -> {booster_now:.0} kg");

        let design = RocketDesign {
            id: crate::rocket::RocketDesignId(1), name: "d".into(),
            stage_groups: st.stage_groups.clone(),
        };
        let stats = crate::rocket::compute_stage_stats(&design, st.payload_kg, "earth_surface");
        assert!((stats[0].twr - TARGET_LIFTOFF_TWR).abs() < 0.05,
            "the booster should be back at {TARGET_LIFTOFF_TWR} carrying the new \
             stage, got {:.2}", stats[0].twr);
    }

    /// The converse: sliding a booster in underneath changes nothing about
    /// what the stages above it carry, so their tanks are left alone —
    /// including any the player sized by hand.
    #[test]
    fn adding_a_booster_underneath_leaves_the_stack_above_alone() {
        let mut st = stack(vec![
            engine(1, 1_000_000.0, 300.0, 1_500.0, EngineCycle::GasGenerator, Propellant::RP1),
        ], 2_000.0);
        // Something deliberately not what the solver would have chosen.
        st.stage_groups[0][0].propellant_mass_kg = 12_345.0;
        recompute_structural_masses(&mut st.stage_groups);

        apply_picked_engine_to_designer(
            &mut st,
            EngineSource::PlayerDesign(crate::engine_project::EngineProjectId(2)),
            engine(2, 9_000_000.0, 300.0, 3_000.0, EngineCycle::GasGenerator, Propellant::RP1),
            PickSlot::InsertAt(0),
        );
        assert_eq!(st.stage_groups.len(), 2, "the booster should be its own group");
        assert_eq!(st.stage_groups[1][0].propellant_mass_kg, 12_345.0,
            "the stage above carries the same payload it always did, so its \
             hand-set tank should survive");
    }

    /// Changing the designer's payload field must not silently redesign
    /// the whole vehicle. It defaults to a nominal test mass, and a stage
    /// sized against a mission delta-v would move every time it changed.
    #[test]
    fn tank_sizes_barely_move_with_the_payload_field() {
        let build = |payload| {
            let st = stack(vec![
                engine(1, 5_000_000.0, 300.0, 1_500.0, EngineCycle::GasGenerator, Propellant::RP1),
                engine(2, 500_000.0, 440.0, 800.0, EngineCycle::Expander, Propellant::LH2),
            ], payload);
            st.stage_groups[0][0].propellant_mass_kg
        };
        let light = build(1_000.0);
        let heavy = build(10_000.0);
        // 9 t more payload displaces at most its own mass from the tank —
        // it does not rescale the vehicle.
        assert!((light - heavy).abs() < 20_000.0,
            "booster tank moved {:.0} kg for a 9 t payload change ({light:.0} -> {heavy:.0})",
            (light - heavy).abs());
    }

    /// An ion stage was the worst case for the old rule: 120 seconds of
    /// its mass flow is a few grams of xenon. It never flies an ascent, so
    /// it's sized for the spiral it will actually fly.
    #[test]
    fn an_ion_stage_is_sized_for_its_spiral_not_for_two_minutes_of_burn() {
        let st = stack(vec![
            engine(1, 1_000_000.0, 300.0, 1_500.0, EngineCycle::GasGenerator, Propellant::RP1),
            engine(2, 1.0, 3500.0, 200.0, EngineCycle::ElectricPropulsion, Propellant::Xenon),
        ], 500.0);
        let ion = &st.stage_groups[1][0];
        // Two minutes of a 1 N ion engine is under a gram.
        let two_minutes = ion.engine.mass_flow_rate() * 120.0;
        assert!(two_minutes < 1.0, "test premise: {two_minutes} kg should be negligible");
        assert!(ion.propellant_mass_kg > two_minutes * 1000.0,
            "ion tank should be sized for its mission, not its burn clock; got {} kg",
            ion.propellant_mass_kg);
        // It delivers roughly the spiral it was sized for.
        let dv = ion.delta_v(st.payload_kg);
        assert!(dv >= LOW_THRUST_DV_TARGET,
            "ion stage should deliver at least {LOW_THRUST_DV_TARGET} m/s, got {dv:.0}");
    }

    /// An engine that cannot lift what is already stacked above it has no
    /// good answer, and inventing one would be worse than admitting it.
    #[test]
    fn an_engine_that_cannot_lift_its_stack_bottoms_out() {
        // 1 kN of thrust under 5 t of payload cannot reach TWR 1.3 with an
        // empty tank, let alone a full one. (A *low-thrust* engine is a
        // different case — it's sized for its spiral, since it was never
        // going to fly an ascent.)
        let st = stack(vec![
            engine(1, 1_000.0, 300.0, 500.0, EngineCycle::GasGenerator, Propellant::RP1),
        ], 5_000.0);
        assert_eq!(st.stage_groups[0][0].propellant_mass_kg, MIN_AUTOSIZED_PROPELLANT,
            "a 1 kN engine under 5 t of payload should bottom out, not guess");
    }
}

#[cfg(test)]
mod nozzle_variant_tests {
    use super::*;
    use crate::engine_project::EngineProject;
    use crate::engine::EngineId;
    use crate::engine_project::EngineProjectId;

    fn app_with_engine() -> (App, EngineProjectId) {
        let mut game = crate::game_state::GameState::new("Nozzle Test".into(), 5);
        let ep = EngineProject::new(
            EngineProjectId(1), EngineId(1), "Family".into(),
            EngineCycle::GasGenerator, PropellantPreset::Kerolox, 1.0,
            &game.balance,
        ).unwrap();
        let pid = ep.project_id;
        game.player_company.engine_projects.push(ep);
        (App::new(game), pid)
    }

    /// Place two stages from the *same* engine project and confirm the
    /// bottom one gets the sea-level bell and the upper one the vacuum
    /// bell — the default that makes the common two-stage design right
    /// without the player knowing the rule.
    #[test]
    fn placement_defaults_sea_level_below_vacuum_above() {
        let (app, pid) = app_with_engine();
        let ep = app.game.player_company.find_engine_project(pid).unwrap();
        let picked = ep.design.clone();
        let source = EngineSource::PlayerDesign(pid);

        let first = app.engine_for_placement(source, picked.clone(), 0);
        let second = app.engine_for_placement(source, picked, 1);

        assert!(!first.is_vacuum_variant(), "bottom group lights at sea level");
        assert!(second.is_vacuum_variant(), "stages above it fly vacuum bells");
        assert!(second.isp_s > first.isp_s);
    }

    /// Paging the engine editor's cycle through a vacuum-only family and
    /// back must not leave the first stage with a vacuum bell: the
    /// expander step overwrites the stage engine's flag, so the sync
    /// falls back to the stage's position rather than trusting it.
    #[test]
    fn paging_through_a_vacuum_only_cycle_does_not_strand_the_first_stage_in_vacuum() {
        let (mut app, pid) = app_with_engine();
        let mut state = Box::new(RocketDesignerState::new("Two Stage".into()));
        {
            let ep = app.game.player_company.find_engine_project(pid).unwrap();
            for gi in 0..2 {
                let stage = Stage {
                    id: StageId(gi as u64 + 1), name: format!("S{}", gi + 1),
                    engine: ep.design_variant(gi > 0), engine_count: 1,
                    propellant_mass_kg: 10_000.0, structural_mass_kg: 1_000.0,
                    fairing: None, power_sources: Vec::new(),
                };
                state.push_new_group(stage, EngineSource::PlayerDesign(pid));
            }
        }
        assert!(!state.stage_groups[0][0].engine.is_vacuum_variant());

        // Editor: GasGenerator -> Expander (vacuum only) ...
        let balance = app.game.balance.clone();
        app.game.player_company.find_engine_project_mut(pid).unwrap()
            .apply_edit("Family".into(), EngineCycle::Expander, PropellantPreset::Kerolox, 1.0, &balance);
        sync_stages_to_projects(&mut state, &app.game.player_company);
        assert!(state.stage_groups[0][0].engine.is_vacuum_variant(),
            "an expander family has only the vacuum form");

        // ... and back to GasGenerator.
        app.game.player_company.find_engine_project_mut(pid).unwrap()
            .apply_edit("Family".into(), EngineCycle::GasGenerator, PropellantPreset::Kerolox, 1.0, &balance);
        sync_stages_to_projects(&mut state, &app.game.player_company);
        assert!(!state.stage_groups[0][0].engine.is_vacuum_variant(),
            "back on a family with a choice, the first stage is sea-level again");
        assert!(state.stage_groups[1][0].engine.is_vacuum_variant(),
            "and the upper stage keeps its vacuum bell");

        // A deliberate vacuum bell on the first stage survives an edit
        // that stays within a family with a choice.
        {
            let ep = app.game.player_company.find_engine_project(pid).unwrap();
            state.stage_groups[0][0].engine = ep.design_variant(true);
        }
        app.game.player_company.find_engine_project_mut(pid).unwrap()
            .apply_edit("Family".into(), EngineCycle::GasGenerator, PropellantPreset::Kerolox, 1.5, &balance);
        sync_stages_to_projects(&mut state, &app.game.player_company);
        assert!(state.stage_groups[0][0].engine.is_vacuum_variant(),
            "a chosen vacuum bell is kept across a scale edit");
    }

    /// [V] swaps the selected stage's bell, and only that stage's.
    #[test]
    fn v_toggles_only_the_selected_stage() {
        let (mut app, pid) = app_with_engine();
        let ep = app.game.player_company.find_engine_project(pid).unwrap();
        let mut state = Box::new(RocketDesignerState::new("Two Stage".into()));
        for gi in 0..2 {
            let engine = ep.design_variant(gi > 0);
            let stage = Stage {
                id: StageId(gi as u64 + 1),
                name: format!("S{}", gi + 1),
                engine,
                engine_count: 1,
                propellant_mass_kg: 10_000.0,
                structural_mass_kg: 1_000.0,
                fairing: None,
                power_sources: Vec::new(),
            };
            state.push_new_group(stage, EngineSource::PlayerDesign(pid));
        }
        state.selected_group = 0;
        state.selected_inner = 0;
        app.input_mode = InputMode::designer(state);

        app.handle_key(KeyCode::Char('v'));

        let state = match &app.input_mode {
            InputMode::RocketDesigner { state, .. } => state,
            other => panic!("should stay in the designer, got {other:?}"),
        };
        assert!(state.stage_groups[0][0].engine.is_vacuum_variant(),
            "the selected stage should have swapped to the vacuum bell");
        assert!(state.stage_groups[1][0].engine.is_vacuum_variant(),
            "the untouched upper stage keeps its vacuum bell");

        // And back again.
        app.handle_key(KeyCode::Char('v'));
        let state = match &app.input_mode {
            InputMode::RocketDesigner { state, .. } => state,
            other => panic!("should stay in the designer, got {other:?}"),
        };
        assert!(!state.stage_groups[0][0].engine.is_vacuum_variant(),
            "[V] should toggle back");
    }

    /// Editing the engine family (here: rescaling it) must not silently
    /// revert an upper stage to the sea-level bell.
    #[test]
    fn editing_the_engine_preserves_each_stage_nozzle() {
        let (mut app, pid) = app_with_engine();
        let ep = app.game.player_company.find_engine_project(pid).unwrap();
        let mut state = RocketDesignerState::new("Keeps Nozzles".into());
        for gi in 0..2 {
            let stage = Stage {
                id: StageId(gi as u64 + 1),
                name: format!("S{}", gi + 1),
                engine: ep.design_variant(gi > 0),
                engine_count: 1,
                propellant_mass_kg: 10_000.0,
                structural_mass_kg: 1_000.0,
                fairing: None,
                power_sources: Vec::new(),
            };
            state.push_new_group(stage, EngineSource::PlayerDesign(pid));
        }

        app.apply_engine_scale(pid, 2.0);
        sync_stages_to_projects(&mut state, &app.game.player_company);

        assert!(!state.stage_groups[0][0].engine.is_vacuum_variant(),
            "booster keeps its sea-level bell across an engine edit");
        assert!(state.stage_groups[1][0].engine.is_vacuum_variant(),
            "upper stage keeps its vacuum bell across an engine edit");
        // The edit did land: both stages now carry the rescaled engine.
        let scaled = app.game.player_company.find_engine_project(pid)
            .unwrap().design.thrust_n;
        for gi in 0..2 {
            assert_eq!(state.stage_groups[gi][0].engine.thrust_n, scaled,
                "stage {gi} should pick up the rescaled thrust");
        }
    }

    /// Third-party engines arrive with a fixed bell — [V] declines
    /// rather than silently doing nothing.
    #[test]
    fn v_refuses_on_third_party_engines() {
        let (mut app, _pid) = app_with_engine();
        let engine = crate::engine::EngineDesign {
            id: EngineId(99), name: "NK-33".into(),
            cycle: EngineCycle::StagedCombustion,
            thrust_n: 1_500_000.0, mass_kg: 1_200.0, isp_s: 297.0,
            exit_pressure_pa: 70_000.0, needs_atmosphere: true,
            propellant_mix: PropellantPreset::Kerolox.propellant_mix(),
            power_draw_w: 0.0,
        };
        let mut state = Box::new(RocketDesignerState::new("Bought".into()));
        state.push_new_group(
            Stage {
                id: StageId(1), name: "S1".into(), engine,
                engine_count: 1, propellant_mass_kg: 10_000.0,
                structural_mass_kg: 1_000.0, fairing: None,
                power_sources: Vec::new(),
            },
            EngineSource::Contracted(crate::third_party::ContractedEngineId(1)),
        );
        app.input_mode = InputMode::designer(state);

        app.handle_key(KeyCode::Char('v'));

        let state = match &app.input_mode {
            InputMode::RocketDesigner { state, .. } => state,
            other => panic!("should stay in the designer, got {other:?}"),
        };
        assert!(!state.stage_groups[0][0].engine.is_vacuum_variant(),
            "a third-party engine keeps the bell it was sold with");
        assert!(
            app.status_message.as_deref().is_some_and(|m| m.contains("fixed nozzle")),
            "the player should be told why nothing happened, got {:?}",
            app.status_message,
        );
    }
}
