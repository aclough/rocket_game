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
use crate::location::{self, DELTA_V_MAP};
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
    Rockets,
    Manufacturing,
    Contracts,
    Launches,
    Finance,
    Events,
}

impl Tab {
    pub const ALL: &[Tab] = &[
        Tab::Overview, Tab::Engines,
        Tab::Rockets, Tab::Manufacturing, Tab::Contracts,
        Tab::Launches, Tab::Finance, Tab::Events,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Engines => "Engines",
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
        matches!(self, Tab::Engines | Tab::Rockets | Tab::Manufacturing
            | Tab::Contracts | Tab::Launches)
    }
}

/// Shared state for the rocket designer screen.
#[derive(Debug, Clone)]
pub struct RocketDesignerState {
    pub rocket_name: String,
    pub stage_groups: Vec<Vec<Stage>>,
    pub engine_sources: Vec<Vec<EngineSource>>,
    pub next_stage_id: u64,
    pub selected_group: usize,
    pub selected_inner: usize,
    pub payload_kg: f64,
    pub launch_from: &'static str,
}

impl RocketDesignerState {
    fn new(name: String) -> Self {
        Self {
            rocket_name: name,
            stage_groups: Vec::new(),
            engine_sources: Vec::new(),
            next_stage_id: 1,
            selected_group: 0,
            selected_inner: 0,
            payload_kg: 1000.0,
            launch_from: "earth_surface",
        }
    }

    /// Total number of individual stages across all groups.
    fn total_stages(&self) -> usize {
        self.stage_groups.iter().map(|g| g.len()).sum()
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
}

/// Whether an engine uses solid propellant (propellant is not adjustable).
fn is_solid_engine(engine: &EngineDesign) -> bool {
    engine.propellant_mix.len() == 1
        && engine.propellant_mix[0].propellant == crate::propellant::Propellant::SolidMix
}

/// Compute thrust-scaled propellant step size for inline adjustments.
/// ~10s of burn time, rounded to nearest 100 kg, min 100 kg.
fn propellant_step(engine: &EngineDesign, engine_count: u32) -> f64 {
    let raw = engine.mass_flow_rate() * engine_count as f64 * 10.0;
    (raw / 100.0).round().max(1.0) * 100.0
}

/// Recompute structural masses for all stage groups based on their position.
/// Aero shell depends on being group 0 (exposed to airflow).
/// Interstage depends on whether the stage is the last group.
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
    /// Typing engine name.
    EngineName { buffer: String },
    /// Selecting cycle type.
    SelectCycle { name: String, selected: usize },
    /// Selecting propellant preset.
    SelectPropellant { name: String, cycle: EngineCycle, selected: usize },
    /// Selecting scale.
    SelectScale {
        name: String,
        cycle: EngineCycle,
        preset: PropellantPreset,
        scale: f64,
        use_vacuum: bool,
        vacuum_only: bool,
    },
    /// Selecting from third-party catalog.
    SelectThirdParty { selected: usize },
    /// Typing rocket name.
    RocketName { buffer: String },
    /// Persistent rocket designer screen.
    RocketDesigner { state: Box<RocketDesignerState> },
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
                deploy_at: destination.clone(),
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
            Tab::Rockets => self.handle_rockets_key(key),
            Tab::Manufacturing => self.handle_manufacturing_key(key),
            Tab::Contracts => self.handle_contracts_key(key),
            Tab::Launches => self.handle_launches_key(key),
            _ => {}
        }
    }

    fn handle_engines_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('n') => {
                // Start new engine design flow
                self.enter_modal(InputMode::EngineName { buffer: String::new() });
            }
            KeyCode::Char('b') => {
                // Buy third-party engine
                if !self.game.player_company.third_party_catalog.is_empty() {
                    self.enter_modal(InputMode::SelectThirdParty { selected: 0 });
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                // Add team to selected project, or steal from busiest
                if self.game.player_company.add_team_to_project(self.selected_item) {
                    self.status_message = Some("Team assigned".into());
                } else if let Some(from) = self.game.player_company.steal_engineering_team_to_engine_project(self.selected_item) {
                    self.status_message = Some(format!("Team reassigned from {}", from));
                } else {
                    self.status_message = Some("No teams to reassign".into());
                }
            }
            KeyCode::Char('-') => {
                // Remove team from selected project
                if self.game.player_company.remove_team_from_project(self.selected_item) {
                    self.status_message = Some("Team removed".into());
                }
            }
            KeyCode::Char('o') => {
                // Order standalone engine build
                if let Some((cost, evt)) = self.game.player_company.order_engine_build(self.selected_item) {
                    self.game.event_log.push(self.game.date, evt);
                    self.status_message = Some(format!("Engine build ordered ({})", crate::ui::draw::format_money(cost)));
                } else {
                    self.status_message = Some("Must be in Testing to order build".into());
                }
            }
            KeyCode::Char('r') => {
                // Revise all discovered flaws and actualize pending improvements
                if self.selected_item < self.game.player_company.engine_projects.len() {
                    let project = &mut self.game.player_company.engine_projects[self.selected_item];
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
            KeyCode::Char('l') | KeyCode::Enter | KeyCode::Char('u') => {
                let persist = key == KeyCode::Char('u');
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
            InputMode::EngineName { buffer } => {
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Enter => {
                        if buffer.is_empty() {
                            self.status_message = Some("Name cannot be empty".into());
                            self.exit_modal();
                        } else {
                            let name = buffer.clone();
                            self.input_mode = InputMode::SelectCycle {
                                name,
                                selected: 0,
                            };
                        }
                    }
                    KeyCode::Backspace => { buffer.pop(); }
                    KeyCode::Char(c) => { buffer.push(c); }
                    _ => {}
                }
            }
            InputMode::SelectCycle { name, selected } => {
                let mut cycles = vec![
                    EngineCycle::PressureFed,
                    EngineCycle::GasGenerator,
                    EngineCycle::Expander,
                    EngineCycle::StagedCombustion,
                    EngineCycle::FullFlow,
                ];
                // Add NuclearThermal if the tech is unlocked
                if self.game.technologies.iter().any(|t|
                    t.id == crate::technology::TECH_NUCLEAR_THERMAL && t.unlocked
                ) {
                    cycles.push(EngineCycle::NuclearThermal);
                }
                cycles.push(EngineCycle::ElectricPropulsion);
                cycles.push(EngineCycle::SolarSail);
                let num_options = cycles.len() + 1; // +1 for Solid Rocket Motor
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => { if *selected > 0 { *selected -= 1; } }
                    KeyCode::Down => { if *selected + 1 < num_options { *selected += 1; } }
                    KeyCode::Enter => {
                        if *selected < cycles.len() {
                            let cycle = cycles[*selected];
                            let name = name.clone();
                            self.input_mode = InputMode::SelectPropellant {
                                name,
                                cycle,
                                selected: 0,
                            };
                        } else {
                            // Solid Rocket Motor — skip propellant selection
                            let name = name.clone();
                            self.input_mode = InputMode::SelectScale {
                                name,
                                cycle: EngineCycle::PressureFed,
                                preset: PropellantPreset::Solid,
                                scale: crate::engine_project::DEFAULT_SCALE,
                                use_vacuum: false,
                                vacuum_only: false,
                            };
                        }
                    }
                    _ => {}
                }
            }
            InputMode::SelectPropellant { name, cycle, selected } => {
                let cycle = *cycle;
                let presets: Vec<PropellantPreset> = PropellantPreset::ALL.iter()
                    .filter(|p| p.compatible_cycles().contains(&cycle))
                    .copied()
                    .collect();
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => { if *selected > 0 { *selected -= 1; } }
                    KeyCode::Down => { if *selected + 1 < presets.len() { *selected += 1; } }
                    KeyCode::Enter => {
                        let preset = presets[*selected];
                        let name = name.clone();
                        // Expander and Nuclear Thermal are always vacuum-optimized
                        let vacuum_only = matches!(cycle, EngineCycle::Expander | EngineCycle::NuclearThermal | EngineCycle::ElectricPropulsion | EngineCycle::SolarSail);
                        self.input_mode = InputMode::SelectScale {
                            name,
                            cycle,
                            preset,
                            scale: crate::engine_project::DEFAULT_SCALE,
                            use_vacuum: true,
                            vacuum_only,
                        };
                    }
                    _ => {}
                }
            }
            InputMode::SelectScale { name, cycle, preset, scale, use_vacuum, vacuum_only } => {
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up | KeyCode::Right => {
                        *scale = (*scale + crate::engine_project::SCALE_STEP)
                            .min(crate::engine_project::MAX_SCALE);
                    }
                    KeyCode::Down | KeyCode::Left => {
                        *scale = (*scale - crate::engine_project::SCALE_STEP)
                            .max(crate::engine_project::MIN_SCALE);
                    }
                    KeyCode::Char('v') if !*vacuum_only => { *use_vacuum = !*use_vacuum; }
                    KeyCode::Enter => {
                        let name = name.clone();
                        let cycle = *cycle;
                        let preset = *preset;
                        let scale = *scale;
                        let use_vacuum = *use_vacuum;
                        self.exit_modal();

                        let tech_id = crate::technology::technology_for_preset(preset);
                        if let Some(evt) = self.game.player_company.start_engine_project(
                            name.clone(), cycle, preset, scale, use_vacuum, tech_id,
                        ) {
                            self.game.event_log.push(self.game.date, evt);
                            self.status_message = Some(format!("Started design: {}", name));
                        } else {
                            self.status_message = Some("Invalid engine configuration".into());
                        }
                    }
                    _ => {}
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
            | InputMode::RocketPayloadInput { .. } => {
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
                if state.on_add_slot() {
                    // Same as 'a' — add stage at end
                    self.input_mode = InputMode::RocketPickEngine {
                        state,
                        target_index: None,
                        inner_index: None,
                        editing: false,
                        booster: false,
                        selected: 0,
                    };
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
                if !state.on_add_slot() {
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
                if !state.on_add_slot() {
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
                self.input_mode = InputMode::RocketPickEngine {
                    state,
                    target_index: None,
                    inner_index: None,
                    editing: false,
                    booster: false,
                    selected: 0,
                };
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                // Insert stage before selected group
                if !state.on_add_slot() {
                    let idx = state.selected_group;
                    self.input_mode = InputMode::RocketPickEngine {
                        state,
                        target_index: Some(idx),
                        inner_index: None,
                        editing: false,
                        booster: false,
                        selected: 0,
                    };
                } else {
                    self.input_mode = InputMode::RocketDesigner { state };
                }
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                // Add booster (parallel stage) to current group
                if !state.on_add_slot() {
                    let gi = state.selected_group;
                    self.input_mode = InputMode::RocketPickEngine {
                        state,
                        target_index: Some(gi),
                        inner_index: None,
                        editing: false,
                        booster: true,
                        selected: 0,
                    };
                } else {
                    self.input_mode = InputMode::RocketDesigner { state };
                }
            }
            KeyCode::Char('x') | KeyCode::Char('X') => {
                // Remove selected inner stage
                if !state.on_add_slot() && !state.stage_groups.is_empty() {
                    let gi = state.selected_group;
                    let si = state.selected_inner;
                    if state.stage_groups[gi].len() == 1 {
                        // Remove entire group
                        state.stage_groups.remove(gi);
                        state.engine_sources.remove(gi);
                        // Rename remaining stages
                        for (gj, group) in state.stage_groups.iter_mut().enumerate() {
                            let glen = group.len();
                            for (sj, stage) in group.iter_mut().enumerate() {
                                stage.name = RocketDesignerState::stage_name(gj, sj, glen);
                            }
                        }
                        recompute_structural_masses(&mut state.stage_groups);
                        // Adjust selection
                        if state.selected_group >= state.stage_groups.len() && state.selected_group > 0 {
                            state.selected_group -= 1;
                        }
                        state.selected_inner = 0;
                        self.status_message = Some(format!("Removed stage group {}", gi + 1));
                    } else {
                        // Remove just the inner stage
                        state.stage_groups[gi].remove(si);
                        state.engine_sources[gi].remove(si);
                        // Rename stages in this group
                        let glen = state.stage_groups[gi].len();
                        for (sj, stage) in state.stage_groups[gi].iter_mut().enumerate() {
                            stage.name = RocketDesignerState::stage_name(gi, sj, glen);
                        }
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
                // Cycle launch site
                let ids = location::surface_location_ids();
                let current_idx = ids.iter().position(|&id| id == state.launch_from).unwrap_or(0);
                state.launch_from = ids[(current_idx + 1) % ids.len()];
                self.input_mode = InputMode::RocketDesigner { state };
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // Done — finalize design
                if state.stage_groups.is_empty() {
                    self.status_message = Some("Must add at least one stage".into());
                    self.input_mode = InputMode::RocketDesigner { state };
                } else {
                    let name = state.rocket_name.clone();
                    let stage_groups = state.stage_groups.clone();
                    self.exit_modal();
                    self.create_rocket_project(name, stage_groups);
                }
            }
            KeyCode::Esc => {
                self.exit_modal();
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
        // Build combined engine list
        let engines = self.available_engines();
        let num_engines = engines.len();

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
                if selected + 1 < num_engines { selected += 1; }
                self.input_mode = InputMode::RocketPickEngine {
                    state, target_index, inner_index, editing, booster, selected,
                };
            }
            KeyCode::Enter => {
                if num_engines == 0 {
                    self.status_message = Some("No engines available".into());
                    self.input_mode = InputMode::RocketPickEngine {
                        state, target_index, inner_index, editing, booster, selected,
                    };
                } else {
                    let (source, engine) = engines[selected].clone();
                    let engine_count = 1u32;
                    // Initial propellant: ~120s burn time scaled to engine thrust
                    let propellant_mass_kg = engine.mass_flow_rate() * engine_count as f64 * 120.0;

                    let stage = Stage {
                        id: StageId(state.next_stage_id),
                        name: String::new(),
                        engine: engine.clone(),
                        engine_count,
                        propellant_mass_kg,
                        structural_mass_kg: 0.0,
                        fairing: None,
                    };
                    state.next_stage_id += 1;

                    match (editing, booster, inner_index, target_index) {
                        // Edit a specific inner stage
                        (true, _, Some(ii), Some(gi)) => {
                            state.stage_groups[gi][ii] = stage;
                            state.engine_sources[gi][ii] = source;
                            state.selected_group = gi;
                            state.selected_inner = ii;
                        }
                        // Add booster (parallel stage) to existing group
                        (false, true, _, Some(gi)) => {
                            state.stage_groups[gi].push(stage);
                            state.engine_sources[gi].push(source);
                            state.selected_group = gi;
                            state.selected_inner = state.stage_groups[gi].len() - 1;
                        }
                        // Insert new group before gi
                        (false, false, _, Some(gi)) => {
                            state.stage_groups.insert(gi, vec![stage]);
                            state.engine_sources.insert(gi, vec![source]);
                            state.selected_group = gi;
                            state.selected_inner = 0;
                        }
                        // Append new group at end
                        (false, false, _, None) => {
                            state.stage_groups.push(vec![stage]);
                            state.engine_sources.push(vec![source]);
                            state.selected_group = state.stage_groups.len() - 1;
                            state.selected_inner = 0;
                        }
                        _ => {}
                    }

                    // Rename all stages and recompute structural masses
                    for (gi, group) in state.stage_groups.iter_mut().enumerate() {
                        let glen = group.len();
                        for (si, stage) in group.iter_mut().enumerate() {
                            stage.name = RocketDesignerState::stage_name(gi, si, glen);
                        }
                    }
                    recompute_structural_masses(&mut state.stage_groups);

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

    /// Build the list of available engines (player Testing + contracted).
    pub fn available_engines(&self) -> Vec<(EngineSource, EngineDesign)> {
        let mut engines: Vec<(EngineSource, EngineDesign)> = Vec::new();
        for ep in &self.game.player_company.engine_projects {
            if matches!(ep.status, EngineDesignStatus::Testing { .. }) {
                engines.push((EngineSource::PlayerDesign(ep.project_id), ep.design.clone()));
            }
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
                        let max = self.game.player_company.engine_projects.len().saturating_sub(1);
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
