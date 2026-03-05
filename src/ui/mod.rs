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
use crate::location;
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
    Teams,
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
        Tab::Overview, Tab::Teams, Tab::Engines,
        Tab::Rockets, Tab::Manufacturing, Tab::Contracts,
        Tab::Launches, Tab::Finance, Tab::Events,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Teams => "Teams",
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
    /// Selecting contract for a launch (or test launch).
    LaunchSelectContract {
        rocket_item_id: crate::manufacturing::InventoryItemId,
        selected: usize,  // 0..N = contracts, N = test launch
    },
    /// Showing launch result.
    LaunchResult {
        record: crate::launch::LaunchRecord,
    },
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
                self.game.advance_day();
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
            Tab::Teams => self.handle_teams_key(key),
            Tab::Engines => self.handle_engines_key(key),
            Tab::Rockets => self.handle_rockets_key(key),
            Tab::Manufacturing => self.handle_manufacturing_key(key),
            Tab::Contracts => self.handle_contracts_key(key),
            Tab::Launches => self.handle_launches_key(key),
            _ => {}
        }
    }

    fn handle_teams_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('h') => {
                let team_num = self.game.player_company.team_count() + 1;
                let name = format!("Team {}", team_num);
                if let Some(evt) = self.game.player_company.hire_team(name.clone()) {
                    self.game.event_log.push(self.game.date, evt);
                    self.status_message = Some(format!("Hired {}", name));
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
            KeyCode::Char('r') => {
                // Revise all discovered flaws
                if self.selected_item < self.game.player_company.engine_projects.len() {
                    let project = &mut self.game.player_company.engine_projects[self.selected_item];
                    if project.start_revision() {
                        let count = match &project.status {
                            EngineDesignStatus::Revising { remaining_indices, .. } => remaining_indices.len(),
                            _ => 0,
                        };
                        self.status_message = Some(format!("Revising {} flaw(s)", count));
                    }
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
            KeyCode::Char('l') | KeyCode::Enter => {
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

                // Enter launch modal — pick contract or test launch
                self.enter_modal(InputMode::LaunchSelectContract {
                    rocket_item_id: item_id,
                    selected: 0,
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
                let cycles = [
                    EngineCycle::PressureFed,
                    EngineCycle::GasGenerator,
                    EngineCycle::Expander,
                    EngineCycle::StagedCombustion,
                    EngineCycle::FullFlow,
                ];
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
                        self.input_mode = InputMode::SelectScale {
                            name,
                            cycle,
                            preset,
                            scale: crate::engine_project::DEFAULT_SCALE,
                            use_vacuum: true,
                        };
                    }
                    _ => {}
                }
            }
            InputMode::SelectScale { name, cycle, preset, scale, use_vacuum } => {
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
                    KeyCode::Char('v') => { *use_vacuum = !*use_vacuum; }
                    KeyCode::Enter => {
                        let name = name.clone();
                        let cycle = *cycle;
                        let preset = *preset;
                        let scale = *scale;
                        let use_vacuum = *use_vacuum;
                        self.exit_modal();

                        if let Some(evt) = self.game.player_company.start_engine_project(
                            name.clone(), cycle, preset, scale, use_vacuum,
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
            InputMode::LaunchSelectContract { rocket_item_id, selected } => {
                let rocket_item_id = *rocket_item_id;
                let selected = *selected;
                let num_contracts = self.game.player_company.active_contracts.len();
                let total_options = num_contracts + 1; // +1 for test launch
                match key {
                    KeyCode::Esc => { self.exit_modal(); }
                    KeyCode::Up => {
                        if selected > 0 {
                            if let InputMode::LaunchSelectContract { selected: s, .. } = &mut self.input_mode {
                                *s -= 1;
                            }
                        }
                    }
                    KeyCode::Down => {
                        if selected + 1 < total_options {
                            if let InputMode::LaunchSelectContract { selected: s, .. } = &mut self.input_mode {
                                *s += 1;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        if selected < num_contracts {
                            // Launch for this contract
                            let contract = &self.game.player_company.active_contracts[selected];
                            let destination = contract.destination.clone();
                            let payload_kg = contract.payload_kg;
                            if let Some((_events, record)) = self.game.launch_rocket(
                                rocket_item_id, Some(selected), &destination, payload_kg,
                            ) {
                                self.input_mode = InputMode::LaunchResult { record };
                            } else {
                                self.status_message = Some("Launch failed (rocket not found)".into());
                                self.exit_modal();
                            }
                        } else {
                            // Test launch to LEO with 0 payload
                            if let Some((_events, record)) = self.game.launch_rocket(
                                rocket_item_id, None, "leo", 0.0,
                            ) {
                                self.input_mode = InputMode::LaunchResult { record };
                            } else {
                                self.status_message = Some("Launch failed (rocket not found)".into());
                                self.exit_modal();
                            }
                        }
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
