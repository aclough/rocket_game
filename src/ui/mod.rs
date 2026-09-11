pub mod draw;
pub mod keys;
pub mod next_steps;
mod designer;
mod editors;
mod modals;
mod planner;
mod tabs;

pub use designer::{DesignerMode, RocketDesignerState};
pub use planner::{DvPlannerState, PlanAction, PlannerSetupField, PlannerSetupState, PlannerSource};

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
use crate::project::{ProjectKind, ProjectRef};
use crate::rocket_project::RocketDesignStatus;
use crate::save;
use crate::stage::{Stage, StageId};

/// The Ready Rockets list in the order it is drawn: grouped by rocket
/// name, newest revision first, and build order within that.
///
/// `App::selected_item` indexes into *this* list on the Launches tab, so
/// the draw side and the launch key have to agree on the order — hence
/// one function rather than a sort in each. The underlying inventory is
/// left in build order, which is what the competitor's stock lookup and
/// the manufacturing pipeline expect.
pub(crate) fn ready_rockets_in_display_order(
    company: &crate::company::Company,
) -> Vec<&crate::manufacturing::InventoryRocket> {
    let mut rockets: Vec<_> = company.manufacturing.inventory.rockets.iter().collect();
    rockets.sort_by(|a, b| {
        a.rocket_name.cmp(&b.rocket_name)
            .then(b.revision.cmp(&a.revision))
            .then(a.item_id.0.cmp(&b.item_id.0))
    });
    rockets
}

/// What the help modal is describing, and what to return to on Esc.
#[derive(Debug, Clone)]
pub enum HelpScope {
    /// Opened from a normal tab view.
    Tab(usize),
    /// Opened from inside the rocket designer — its state rides along
    /// so Esc puts the player back where they were.
    RocketDesigner(Box<RocketDesignerState>),
}

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

/// How long the paused game waits for a keypress before looping. Long
/// enough that idling costs nothing, short enough that the UI still feels
/// immediate.
const PAUSED_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// How long to block waiting for input before the main loop comes round
/// again.
///
/// Paused, there is no tick to be late for, so wait the whole interval.
/// Running, wait only what is left until the next tick falls due.
///
/// Subtracting a stale `last_tick` in the paused case was a busy loop:
/// nothing resets it while the clock is stopped, so the elapsed time grows
/// past the interval, `saturating_sub` floors the timeout at zero, and the
/// loop spins at full speed redrawing — pinning a core and starving input
/// handling behind a redraw it does thousands of times a second.
fn input_timeout(
    speed: GameSpeed, tick_rate: Duration, since_last_tick: Duration,
) -> Duration {
    if speed == GameSpeed::Paused {
        tick_rate
    } else {
        tick_rate.saturating_sub(since_last_tick)
    }
}

/// Modal input state for new engine design flow.
#[derive(Debug, Clone)]
pub enum InputMode {
    Normal,
    /// Keybinding reference. `scope` remembers what to describe: the
    /// tab that was open, or the rocket designer, whose state travels
    /// with the modal so Esc returns to it.
    Help {
        scope: HelpScope,
    },
    /// One-screen orientation, shown once at the start of a new game.
    /// Dismissed by any key and never shown again — it is not stored in
    /// the save, so it cannot reappear on load.
    Intro,
    /// Non-linear engine editor — edits an existing EngineProject in
    /// place. The cursor walks a fixed field list; each field has its
    /// own interaction (Left/Right cycles, +/- adjusts scale, Enter
    /// opens a text sub-modal for name or scale). Only opened from
    /// inside the rocket designer; the designer state travels with the
    /// editor and is restored on Esc.
    /// `state` is `Some` when opened from the rocket designer (Esc
    /// returns there, edits sync to the stage clone); `None` when opened
    /// standalone from the Engines pane (Esc cancels the draft, `d`
    /// commits it).
    EngineEditor {
        project_id: crate::engine_project::EngineProjectId,
        cursor: usize,
        state: Option<Box<RocketDesignerState>>,
    },
    /// Text-input sub-modal for the engine name.
    EngineEditorNameInput {
        project_id: crate::engine_project::EngineProjectId,
        cursor: usize,
        buffer: String,
        state: Option<Box<RocketDesignerState>>,
    },
    /// Numeric sub-modal for the engine scale.
    EngineEditorScaleInput {
        project_id: crate::engine_project::EngineProjectId,
        cursor: usize,
        buffer: String,
        state: Option<Box<RocketDesignerState>>,
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
    /// Entering a sealed bid (in $M) on an available solicitation.
    BidEntry { contract_index: usize, buffer: String },
    /// Editing standing per-market bid rules (enable + margin). The
    /// rule engine auto-bids marginal cost × (1 + margin) daily.
    BidRules { selected: usize },
    /// Browsing observed award outcomes (price-discovery history).
    AwardHistory { scroll: usize },
    /// Browsing anchor-customer programs; Enter/B on a soliciting one
    /// opens block-bid entry. Auto-opens when a liftable program is
    /// announced (the announcement pauses the game).
    Campaigns { selected: usize },
    /// Entering a sealed block bid (per-mission price in $M) on a
    /// soliciting campaign. Esc returns to the programs list.
    CampaignBidEntry {
        campaign_id: crate::contract::CampaignId,
        selected: usize,
        buffer: String,
    },
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
    /// "Are you sure?" before retiring a design. Holds the target's *id*
    /// rather than its pane row, so a day tick landing while the prompt
    /// is open can't redirect it at whatever slid into that slot.
    ///
    /// `effects` is the plan computed when the prompt opened — the same
    /// decisions `Company::retire` will apply, so what the player reads
    /// is what they get.
    ConfirmRetire {
        target: crate::company::ProjectRef,
        effects: crate::company::RetirementEffects,
    },
}

/// Which RocketDesignerState field a location picker should update.
#[derive(Debug, Clone, Copy)]
pub enum LocationPickerTarget {
    LaunchSite,
    MissionDestination,
}

/// The terminal every screen in the game draws to.
pub type Tui = Terminal<CrosstermBackend<io::Stdout>>;

/// Run `body` inside raw mode on the alternate screen, and restore the
/// terminal afterwards — on a clean return, an error, *or* a panic. A
/// panic is re-raised once the terminal is back, so callers that want
/// to report it can catch it outside this call and print to a screen
/// the player can see.
pub fn with_terminal<T>(body: impl FnOnce(&mut Tui) -> io::Result<T>) -> io::Result<T> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let caught = std::panic::catch_unwind(
        std::panic::AssertUnwindSafe(|| body(&mut terminal)),
    );

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    match caught {
        Ok(result) => result,
        Err(payload) => std::panic::resume_unwind(payload),
    }
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

/// Step through a slice of values, wrapping at either end. Direction
/// is forward when `forward` is true, backward otherwise.
fn wrap_cycle<T: Copy + PartialEq>(values: &[T], current: T, forward: bool) -> Option<T> {
    let i = values.iter().position(|v| *v == current)?;
    let n = values.len();
    if n == 0 { return None; }
    let next = if forward { (i + 1) % n } else { (i + n - 1) % n };
    Some(values[next])
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

    /// Same, but opens on the one-screen orientation. Used for a fresh
    /// company; loading a save skips it.
    pub fn new_game(game: GameState) -> Self {
        let mut app = App::new(game);
        app.input_mode = InputMode::Intro;
        app.game.speed = GameSpeed::Paused;
        app
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
        crate::report::install_hook();

        // `with_terminal` has already given the terminal back by the time
        // a panic reaches here, so the report below is printed somewhere
        // the player can actually read it. See `report.rs`.
        let caught = std::panic::catch_unwind(
            std::panic::AssertUnwindSafe(|| with_terminal(|terminal| self.main_loop(terminal))),
        );

        match caught {
            Ok(result) => result,
            Err(_) => {
                let panic_text = crate::report::take_panic_text()
                    .unwrap_or_else(|| "panic with no captured message".into());
                eprint!("{}", crate::report::handle_panic(&self.game, &panic_text));
                Err(io::Error::other("the game hit a bug and stopped"))
            }
        }
    }

    fn main_loop(&mut self, terminal: &mut Tui) -> io::Result<()> {
        let mut last_tick = Instant::now();

        while self.running {
            terminal.draw(|frame| draw::draw(frame, self))?;

            let tick_rate = if self.game.speed == GameSpeed::Paused {
                PAUSED_POLL_INTERVAL // Still responsive to input when paused
            } else {
                Duration::from_millis(self.game.speed.tick_ms())
            };

            let timeout = input_timeout(self.game.speed, tick_rate, last_tick.elapsed());

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

                // Autosave at the top of each month. Saves are ~0.2 MB
                // at eight game-years and the log is ring-buffered, so
                // a monthly write costs nothing worth counting.
                if day_events.iter().any(|e| matches!(
                    e, crate::event::GameEvent::MonthStart,
                )) {
                    self.autosave();
                }

                // Switch to Events tab on critical events
                if day_events.iter().any(|e| e.importance() == crate::event::EventImportance::Critical) {
                    if let Some(idx) = Tab::ALL.iter().position(|t| matches!(t, Tab::Events)) {
                        self.active_tab = idx;
                    }
                }
                // A liftable program announcement already paused the
                // game; open the programs modal on it so the block-bid
                // decision is one keypress away.
                if matches!(self.input_mode, InputMode::Normal) {
                    if let Some(crate::event::GameEvent::CampaignAnnounced { program, .. }) =
                        day_events.iter().find(|e| matches!(
                            e,
                            crate::event::GameEvent::CampaignAnnounced { liftable: true, .. },
                        ))
                    {
                        let selected = self.game.active_campaigns.iter()
                            .position(|c| c.name == *program)
                            .unwrap_or(0);
                        self.enter_modal(InputMode::Campaigns { selected });
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
            KeyCode::Char('q') => {
                // Autosave on the way out, so quitting is never the
                // thing that loses an hour of play.
                self.autosave();
                self.running = false;
            }
            KeyCode::F(12) => self.write_report(),
            KeyCode::Char('?') => {
                self.enter_modal(InputMode::Help {
                    scope: HelpScope::Tab(self.active_tab),
                });
            }
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
            Tab::Engines => self.handle_project_pane_key(ProjectKind::Engine, key),
            Tab::Reactors => self.handle_project_pane_key(ProjectKind::Reactor, key),
            Tab::Rockets => self.handle_project_pane_key(ProjectKind::Rocket, key),
            Tab::Manufacturing => self.handle_manufacturing_key(key),
            Tab::Contracts => self.handle_contracts_key(key),
            Tab::Launches => self.handle_launches_key(key),
            _ => {}
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
                        let max = self.game.player_company.visible_rocket_projects()
                            .count().saturating_sub(1);
                        if self.selected_item < max {
                            self.selected_item += 1;
                        }
                    }
                    Tab::Manufacturing => {
                        // The cursor walks the drawn tree, which holds one
                        // row per order — same length, different order.
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

    /// Write a rotating autosave, reporting failure rather than
    /// swallowing it — a player who thinks they're protected and isn't
    /// is worse off than one who knows.
    fn autosave(&mut self) {
        if let Err(e) = save::autosave(&self.game) {
            self.status_message = Some(format!("Autosave failed: {e}"));
        }
    }

    /// Write a session report (F12). Tells the player exactly where it
    /// went — a diagnostic they can't find is a diagnostic that doesn't
    /// exist.
    fn write_report(&mut self) {
        self.status_message = Some(match crate::report::write_session_report(&self.game) {
            Ok(path) => format!("Report written to {}", path.display()),
            Err(e) => format!("Could not write report: {e}"),
        });
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
mod loop_timing_tests {
    use super::*;

    /// The paused game must not busy-loop. `last_tick` is only reset when a
    /// day advances, so with the clock stopped it goes arbitrarily stale;
    /// subtracting it floored the poll timeout at zero and pinned a core.
    #[test]
    fn a_paused_game_waits_for_input_however_stale_the_clock_is() {
        let rate = PAUSED_POLL_INTERVAL;
        for stale in [
            Duration::from_millis(0),
            Duration::from_millis(99),
            Duration::from_millis(101),
            Duration::from_secs(60),
            Duration::from_secs(86_400),
        ] {
            assert_eq!(
                input_timeout(GameSpeed::Paused, rate, stale), rate,
                "paused, {stale:?} since the last tick: must still wait the \
                 full interval, not spin",
            );
        }
    }

    /// Running, the timeout is the time left until the tick falls due —
    /// that is what keeps the game to its chosen speed.
    #[test]
    fn a_running_game_waits_only_until_its_next_tick() {
        let rate = Duration::from_millis(250);
        assert_eq!(
            input_timeout(GameSpeed::Normal, rate, Duration::from_millis(0)),
            rate,
        );
        assert_eq!(
            input_timeout(GameSpeed::Normal, rate, Duration::from_millis(100)),
            Duration::from_millis(150),
        );
        // Overdue: don't wait at all, the tick is already late.
        assert_eq!(
            input_timeout(GameSpeed::Normal, rate, Duration::from_millis(400)),
            Duration::ZERO,
        );
    }
}
