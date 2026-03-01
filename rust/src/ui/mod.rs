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
use crate::engine_project::{EngineDesignStatus, EngineProjectId, PropellantPreset};
use crate::game_state::{GameSpeed, GameState};
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
    Events,
}

impl Tab {
    pub const ALL: &[Tab] = &[
        Tab::Overview, Tab::Teams, Tab::Engines,
        Tab::Rockets, Tab::Manufacturing, Tab::Events,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Teams => "Teams",
            Tab::Engines => "Engines",
            Tab::Rockets => "Rockets",
            Tab::Manufacturing => "Mfg",
            Tab::Events => "Events",
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
    /// Adding a stage to the rocket being designed.
    /// Shows list of completed engines to pick from.
    RocketSelectEngine {
        rocket_name: String,
        /// Stage groups built so far.
        stage_groups: Vec<Vec<Stage>>,
        /// Next stage ID to assign.
        next_stage_id: u64,
        selected: usize,
    },
    /// Configuring a stage: engine count and propellant mass.
    RocketConfigStage {
        rocket_name: String,
        stage_groups: Vec<Vec<Stage>>,
        next_stage_id: u64,
        engine_project_id: EngineProjectId,
        engine: EngineDesign,
        engine_count: u32,
        propellant_mass_kg: f64,
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
            KeyCode::Enter => {}

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
                self.input_mode = InputMode::EngineName { buffer: String::new() };
            }
            KeyCode::Char('b') => {
                // Buy third-party engine
                if !self.game.player_company.third_party_catalog.is_empty() {
                    self.input_mode = InputMode::SelectThirdParty { selected: 0 };
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                // Add team to selected project
                if self.game.player_company.add_team_to_project(self.selected_item) {
                    self.status_message = Some("Team assigned".into());
                } else {
                    self.status_message = Some("No unassigned teams".into());
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
                self.input_mode = InputMode::RocketName { buffer: String::new() };
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                if self.game.player_company.add_team_to_rocket_project(self.selected_item) {
                    self.status_message = Some("Team assigned".into());
                } else {
                    self.status_message = Some("No unassigned teams".into());
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
                } else {
                    self.status_message = Some("No unassigned mfg teams or order is waiting".into());
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

    fn handle_input_mode_key(&mut self, key: KeyCode) {
        match &mut self.input_mode {
            InputMode::Normal => unreachable!(),
            InputMode::EngineName { buffer } => {
                match key {
                    KeyCode::Esc => { self.input_mode = InputMode::Normal; }
                    KeyCode::Enter => {
                        if buffer.is_empty() {
                            self.status_message = Some("Name cannot be empty".into());
                            self.input_mode = InputMode::Normal;
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
                match key {
                    KeyCode::Esc => { self.input_mode = InputMode::Normal; }
                    KeyCode::Up => { if *selected > 0 { *selected -= 1; } }
                    KeyCode::Down => { if *selected + 1 < cycles.len() { *selected += 1; } }
                    KeyCode::Enter => {
                        let cycle = cycles[*selected];
                        let name = name.clone();
                        self.input_mode = InputMode::SelectPropellant {
                            name,
                            cycle,
                            selected: 0,
                        };
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
                    KeyCode::Esc => { self.input_mode = InputMode::Normal; }
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
                    KeyCode::Esc => { self.input_mode = InputMode::Normal; }
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
                        self.input_mode = InputMode::Normal;

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
                    KeyCode::Esc => { self.input_mode = InputMode::Normal; }
                    KeyCode::Up => { if *selected > 0 { *selected -= 1; } }
                    KeyCode::Down => { if *selected + 1 < catalog_len { *selected += 1; } }
                    KeyCode::Enter => {
                        let idx = *selected;
                        let date = self.game.date;
                        self.input_mode = InputMode::Normal;
                        if let Some(evt) = self.game.player_company.purchase_third_party(idx, date) {
                            self.game.event_log.push(self.game.date, evt);
                            self.status_message = Some("Engine purchased".into());
                        }
                    }
                    _ => {}
                }
            }
            InputMode::RocketName { buffer } => {
                match key {
                    KeyCode::Esc => { self.input_mode = InputMode::Normal; }
                    KeyCode::Enter => {
                        if buffer.is_empty() {
                            self.status_message = Some("Name cannot be empty".into());
                            self.input_mode = InputMode::Normal;
                        } else {
                            let name = buffer.clone();
                            self.input_mode = InputMode::RocketSelectEngine {
                                rocket_name: name,
                                stage_groups: Vec::new(),
                                next_stage_id: 1,
                                selected: 0,
                            };
                        }
                    }
                    KeyCode::Backspace => { buffer.pop(); }
                    KeyCode::Char(c) => { buffer.push(c); }
                    _ => {}
                }
            }
            InputMode::RocketSelectEngine { rocket_name, stage_groups, next_stage_id, selected } => {
                // Collect engines list from completed projects without borrowing self
                let engines: Vec<(EngineProjectId, EngineDesign)> =
                    self.game.player_company.engine_projects.iter()
                        .filter(|ep| matches!(ep.status, EngineDesignStatus::Testing { .. }))
                        .map(|ep| (ep.project_id, ep.design.clone()))
                        .collect();
                let num_engines = engines.len();
                match key {
                    KeyCode::Esc => { self.input_mode = InputMode::Normal; }
                    KeyCode::Up => { if *selected > 0 { *selected -= 1; } }
                    KeyCode::Down => { if *selected + 1 < num_engines { *selected += 1; } }
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        // Done adding stages — create the rocket project
                        if stage_groups.is_empty() {
                            self.status_message = Some("Must add at least one stage".into());
                        } else {
                            let rocket_name = rocket_name.clone();
                            let stage_groups = stage_groups.clone();
                            self.input_mode = InputMode::Normal;
                            self.create_rocket_project(rocket_name, stage_groups);
                        }
                    }
                    KeyCode::Enter => {
                        if num_engines == 0 {
                            self.status_message = Some("No completed engines available".into());
                        } else {
                            let (ep_id, engine) = engines[*selected].clone();
                            let rocket_name = rocket_name.clone();
                            let stage_groups = stage_groups.clone();
                            let next_stage_id = *next_stage_id;
                            self.input_mode = InputMode::RocketConfigStage {
                                rocket_name,
                                stage_groups,
                                next_stage_id,
                                engine_project_id: ep_id,
                                engine,
                                engine_count: 1,
                                propellant_mass_kg: 10_000.0,
                            };
                        }
                    }
                    _ => {}
                }
            }
            InputMode::RocketConfigStage {
                rocket_name, stage_groups, next_stage_id,
                engine_project_id: _, engine, engine_count, propellant_mass_kg,
            } => {
                match key {
                    KeyCode::Esc => {
                        // Go back to engine selection
                        let rocket_name = rocket_name.clone();
                        let stage_groups = stage_groups.clone();
                        let next_stage_id = *next_stage_id;
                        self.input_mode = InputMode::RocketSelectEngine {
                            rocket_name,
                            stage_groups,
                            next_stage_id,
                            selected: 0,
                        };
                    }
                    KeyCode::Up => {
                        *propellant_mass_kg = (*propellant_mass_kg + 5_000.0).min(500_000.0);
                    }
                    KeyCode::Down => {
                        *propellant_mass_kg = (*propellant_mass_kg - 5_000.0).max(1_000.0);
                    }
                    KeyCode::Right => {
                        *engine_count = (*engine_count + 1).min(9);
                    }
                    KeyCode::Left => {
                        *engine_count = (*engine_count).max(2) - 1;
                    }
                    KeyCode::Enter => {
                        // Build the stage and add it as a new stage group
                        let group_index = stage_groups.len();
                        let is_first_group = group_index == 0;
                        let propellant_mix: Vec<(crate::propellant::Propellant, f64)> =
                            engine.propellant_mix.iter()
                                .map(|f| (f.propellant, f.mass_fraction))
                                .collect();
                        let breakdown = structure::compute_structural_mass(
                            *propellant_mass_kg,
                            &propellant_mix,
                            engine,
                            *engine_count,
                            is_first_group,
                            true, // interstage for all except last (computed later)
                        );

                        let stage = Stage {
                            id: StageId(*next_stage_id),
                            name: format!("S{}", group_index + 1),
                            engine: engine.clone(),
                            engine_count: *engine_count,
                            propellant_mass_kg: *propellant_mass_kg,
                            structural_mass_kg: breakdown.total,
                            fairing: None,
                        };

                        let mut stage_groups = stage_groups.clone();
                        stage_groups.push(vec![stage]);
                        let next_stage_id = *next_stage_id + 1;
                        let rocket_name = rocket_name.clone();

                        self.status_message = Some(format!("Stage {} added", group_index + 1));
                        self.input_mode = InputMode::RocketSelectEngine {
                            rocket_name,
                            stage_groups,
                            next_stage_id,
                            selected: 0,
                        };
                    }
                    _ => {}
                }
            }
        }
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
                    Tab::Engines | Tab::Rockets | Tab::Manufacturing => {
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
