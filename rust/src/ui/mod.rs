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

use crate::engine::EngineCycle;
use crate::engine_project::{EngineDesignStatus, PropellantPreset};
use crate::game_state::{GameSpeed, GameState};
use crate::save;

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
    Events,
}

impl Tab {
    pub const ALL: &[Tab] = &[Tab::Overview, Tab::Teams, Tab::Engines, Tab::Events];

    pub fn name(&self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Teams => "Teams",
            Tab::Engines => "Engines",
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
            KeyCode::Char('t') => {
                // Start testing
                if self.selected_item < self.game.player_company.engine_projects.len() {
                    let project = &mut self.game.player_company.engine_projects[self.selected_item];
                    if project.start_testing() {
                        self.status_message = Some("Testing started".into());
                    }
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
            KeyCode::Char('c') => {
                // Mark complete
                if self.selected_item < self.game.player_company.engine_projects.len() {
                    let project = &mut self.game.player_company.engine_projects[self.selected_item];
                    if project.mark_complete() {
                        self.status_message = Some("Engine marked complete".into());
                    }
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
                    Tab::Engines => {
                        self.selected_item = self.selected_item.saturating_sub(1);
                    }
                    Tab::Events => {
                        self.content_scroll = self.content_scroll.saturating_sub(1);
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
                    _ => {
                        self.content_scroll += 1;
                    }
                }
            }
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
