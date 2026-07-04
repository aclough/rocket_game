# Phase 1: Game Loop, Time, Seed & Terminal UI

## Context

We have core data structures (propellant, engine, stage, rocket, location, game_state shell). Phase 1 builds the game loop backbone that everything else hangs off: time advances, money depletes, events fire, and the player sees it all in a ratatui terminal UI. No gameplay mechanics yet — just the skeleton that Phase 2 (engineering teams, engine design) will plug into.

---

## 1. Data Structures

### `rust/src/calendar.rs` — Date & Time

```rust
struct GameDate {
    year: u32,     // e.g. 2001
    month: u32,    // 1-12
    day: u32,      // 1-28/29/30/31 (real month lengths)
}
```

- `GameDate::next_day() -> GameDate` — advance by one day, rolling month/year correctly
- `GameDate::is_first_of_month() -> bool` — for salary triggers
- `GameDate::days_in_month() -> u32` — handles Feb/leap years
- `GameDate::display() -> String` — e.g. "Jan 15, 2001"
- `GameDate::day_of_year() -> u32` — for scheduling math
- Default start: **Jan 1, 2001**

### `rust/src/event.rs` — Event Queue

```rust
enum GameEvent {
    DayAdvanced,
    MonthStart,
    MoneyChanged { amount: f64, reason: String },
    GameStarted,
    // Future phases will add: LaunchResult, ContractOffered, FlawDiscovered, etc.
}

struct EventLog {
    events: VecDeque<(GameDate, GameEvent)>,  // timestamped
    max_size: usize,                          // ring buffer
}
```

- Events are informational records of what happened, stored for the UI to display
- `EventLog::push(date, event)`, `recent(n) -> &[(GameDate, GameEvent)]` — last N for the news ticker
- Events do NOT pause time — that's a UI concern (the UI decides whether to pause on interesting events)

### `rust/src/seed.rs` — Deterministic RNG

**Hash-keyed derivation** for world questions: hash the question string with the seed to produce a per-question sub-RNG. Each question always gets the same answer regardless of query order. Separate contingent RNG for non-deterministic events.

```rust
struct GameSeed {
    seed: u64,
    contingent_rng: StdRng,  // for in-game randomness (explosions, flaw rolls)
}
```

- `GameSeed::new(seed: u64)` — create contingent_rng from `seed.wrapping_add(1)`
- `GameSeed::world_query(question: &str) -> StdRng` — hash(seed, question) → deterministic sub-RNG. Order-independent, save-scum-proof.
- Dependency: `rand` crate, plus a hash function (SipHash via `std::hash` or `std::collections::hash_map::DefaultHasher`)

### Updates to `rust/src/game_state.rs`

```rust
enum GameSpeed { Paused, Normal, Fast, VeryFast }

struct GameState {
    date: GameDate,
    start_date: GameDate,
    player_company: Company,
    event_log: EventLog,
    seed: GameSeed,
    speed: GameSpeed,
}
```

- `GameState::advance_day()` — tick one day forward:
  1. Advance `date`
  2. If first of month: log `MonthStart` (no salaries yet, but the hook exists for Phase 2)
  3. Log `DayAdvanced`
  4. Return list of events generated this tick (for UI to decide if something is worth pausing for)
- `GameState::new(company_name, starting_money, seed)` — starting_money ~$200M default
- `GameState::elapsed_days() -> u32` — days since start

Company struct stays minimal — no new fields until Phase 2 adds teams.

---

## 2. Terminal UI

### Dependencies (Cargo.toml additions)

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"
rand = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

### File structure

```
src/
    ui/
        mod.rs          — App struct, main loop, input dispatch
        draw.rs         — all rendering (status bar, content, event feed, help bar)
    bin/
        main.rs         — entry point: parse args or prompt, create game, run App
```

### `App` struct (`ui/mod.rs`)

```rust
enum FocusedPane { Sidebar, Content }

struct App {
    game: GameState,
    running: bool,
    active_tab: usize,          // index into tab list
    focused_pane: FocusedPane,
    content_scroll: usize,      // scroll offset in content pane
    event_scroll: usize,        // scroll offset in event feed
}
```

### Navigation — Paradox-style

- **Space** — toggle pause/unpause
- **1/2/3** — set speed (Normal/Fast/VeryFast); also unpauses
- **Left/Right arrows** — switch focus between sidebar (tab list) and content pane; focused pane gets highlighted border
- **Up/Down arrows** — move selection within focused pane (sidebar: select tab; content: scroll)
- **Enter** — activate selected tab (when sidebar focused)
- **s** — save game
- **q** — quit

When unpaused, days tick automatically at the current speed. Tick rate: Normal=~4/sec, Fast=~15/sec, VeryFast=~60/sec (controlled by poll timeout).

### Screen layout

```
┌─────────────────────────────────────────────────────────────────┐
│ SpaceCorp      Jan 15, 2001      $200,000,000      ▶ Normal    │
├──────────────┬──────────────────────────────────────────────────┤
│ [Overview]   │                                                  │
│ [Events]     │  Company: SpaceCorp                              │
│              │  Founded: Jan 1, 2001                             │
│              │  Days elapsed: 14                                 │
│              │  Engine designs: 0                                │
│              │  Rocket designs: 0                                │
│              │                                                   │
├──────────────┴──────────────────────────────────────────────────┤
│ Jan 15: Day advanced                                            │
│ Jan  1: New month                                               │
│ Jan  1: SpaceCorp founded with $200,000,000                     │
├─────────────────────────────────────────────────────────────────┤
│ [Space] Pause  [1-3] Speed  [←→] Pane  [↑↓] Select  [Q] Quit  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. Save/Load — `rust/src/save.rs`

- Add `#[derive(Serialize, Deserialize)]` to all data types
- `save_game(state: &GameState, path: &Path) -> Result<()>`
- `load_game(path: &Path) -> Result<GameState>`
- Format: JSON (human-readable, easy to debug)
- UI: `s` saves to `~/.rocket_tycoon/saves/<company_name>.json`

---

## 4. Startup Flow

`main.rs` on launch:
1. Check for command-line args (load file, or seed)
2. If no args: prompt for company name (text input in terminal before TUI starts)
3. Create `GameState::new(name, 200_000_000.0, seed)`
4. Log `GameStarted` event
5. Enter TUI main loop

---

## 5. Implementation Order

1. **`calendar.rs`** — GameDate with full tests (date math, month lengths, leap years, display)
2. **`event.rs`** — GameEvent enum, EventLog with tests
3. **`seed.rs`** — GameSeed with hash-keyed world_query, determinism tests
4. **Update `game_state.rs`** — integrate calendar/events/seed, `advance_day()`, `GameSpeed`, tests
5. **Add serde derives** to all existing data types (propellant, engine, stage, rocket, location types)
6. **`save.rs`** — save/load JSON, tests
7. **Add dependencies** to Cargo.toml (ratatui, crossterm, rand, serde, serde_json)
8. **`ui/mod.rs`** — App struct, main loop, input handling, pane focus
9. **`ui/draw.rs`** — rendering: status bar, overview tab, events tab, help bar
10. **`src/bin/main.rs`** — entry point with startup flow
11. **Wire save/load into UI**

## 6. Verification

- `cargo test` — all 66 existing tests still pass + new tests for calendar, events, seed, save/load
- `cargo run` — launches TUI, Space pauses/unpauses, days tick at speed, arrow keys navigate panes
- Save game with `s`, quit with `q`, relaunch and load, verify state round-trips correctly
- `cargo build` — clean library build
