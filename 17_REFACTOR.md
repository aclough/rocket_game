# Refactoring Opportunities

> Survey of the codebase as of commit `b826426` (2026-09-07). Grouped by
> theme, ranked within each group. Nothing here has been implemented.
> Add `USER:` comments inline; I'll reply as `CLAUDE:`.
>
> Baseline: ~40.6k lines of Rust, 0 clippy warnings, 0 build warnings,
> 13 `#[allow]`s (all justified inline), effectively zero TODO/FIXME
> markers. This is a clean codebase; the items below are about
> duplication and structure, not hygiene.

Legend: **Size** = how much code moves. **Risk** = chance of behaviour
change / save-format impact. Items marked **⚠ data-structure** change a
fundamental struct or the save format and, per CLAUDE.md, want a
discussion before implementation.

---

## A. Single-source-of-truth violations

These are the items closest to being bugs. CLAUDE.md calls out time and
money specifically, and each of these has two definitions of one thing.

### A1. The monthly expense ledger only sees salaries
- **Where:** `src/game_state/mod.rs:557-598` (`record_expense`,
  `record_income`, `pub(super)`); the only callers are
  `advance.rs:257` (salary) and `flight_ops.rs:953` (contract payment).
  Meanwhile `company.rs:315, 354, 1095, 1152` do `self.money -= ...`
  directly for hiring and build orders.
- **Effect:** the Finance tab's Expenses column excludes hiring and
  manufacturing, while Balance includes them. Two ledgers for one cash
  flow.
- **Refactor:** move the ledger onto `Company` as `debit(date, amount)`
  / `credit(date, amount)` which adjust `money` *and* the current
  month's `MonthlyFinancials` row. Replace every raw `money +=`/`-=`.
  Delete the three `GameState` helpers.
- **Size/risk:** small / low.

### ✅ A2. "One day of salary" is defined three ways
- **Where:** `company.rs:1862` and `manufacturing.rs:291` both compute
  `monthly_salary / 30.0` from the *balance config*; the actual monthly
  charge (`company.rs:346-350`) sums each team's stored
  `monthly_salary`. They agree only because salaries never change.
- **Refactor:** `CostsConfig::daily_engineering_salary()` /
  `daily_manufacturing_salary()` (or a `DAYS_PER_MONTH_APPROX` const in
  `calendar.rs` used by both). Longer term, accrue from the teams.
- **Size/risk:** small / none.

### ✅ A3. UI gauges hardcode `30` where BalanceConfig has the knob
- **Where:** `src/ui/draw.rs:356-367` (engines) and `:825-836`
  (rockets) divide by `30.0` and print `"/30"`. The reactors pane at
  `:626-643` correctly reads `balance.work.testing_cycle_work` /
  `flaw_revision_work`.
- **Refactor:** read the config at all eight sites; extract one
  `testing_gauge(...)` helper since the two blocks are identical.
- **Size/risk:** small / none. Direct CLAUDE.md compliance fix.

### A4. Three delta-v models feed the same decisions
- **Where:**
  (a) `RocketDesign::total_delta_v` (`rocket.rs:302-320`), pure vacuum;
  (b) planner `full_group_dv` (`path_planning.rs:67-77`), vacuum minus
  gravity loss, drag on edges;
  (c) `compute_stage_stats().delta_v_effective` (`rocket.rs:1131`),
  vacuum minus gravity, aero and overexpansion.
  The designer prints "have X" from (a) and "need Y" from (b);
  `simulate_launch` (`launch.rs:284-295`) compares (a) against a route
  priced by (b), and re-implements the overexpansion Isp penalty at
  `launch.rs:269-282` that (c) already computes.
- **Refactor:** one `DesignPerformance` (per-group `{vacuum, gravity,
  aero, overexpansion}` plus totals) computed by a single function of
  `(design, payload_kg, launch_from)`; planner, designer, launch sim
  and stats all read it.
- **Size/risk:** medium / medium. Needs careful test updates; could
  change which contracts are judged feasible.

### ✅ A5. Time constants scattered outside `calendar.rs`
- **Where:** `/ 30.0` (above), `contract.rs:359` (`/ 365.25`),
  `flaw.rs:25` (`365.0`), `advance.rs:481,487` (`>= 365`),
  `economy.rs:201` (private `add_months`), `simulate.rs:185` (its own
  month-index arithmetic). Six hand-rolled ISO date formats
  (`draw.rs:2872, 2915`, `report.rs:260`, `sim.rs:55,120`,
  `advance.rs:342,363`).
- **Refactor:** `DAYS_PER_MONTH_APPROX`, `DAYS_PER_YEAR`,
  `GameDate::add_months`, `GameDate::month_index`, `GameDate::iso()`.
- **Size/risk:** small / none.

### A6. `GameDate::days_until` / `add_days` are O(n) loops on the draw path
- **Where:** `calendar.rs:57-75`. Called per frame by `elapsed_days()`
  (`draw.rs:250`, walks from 2001 to today) and per contract row
  (`draw.rs:1314`), and per generated contract (`contract.rs:359`).
- **Refactor:** `GameDate::ordinal()` / `from_ordinal()`; express the
  four date functions in O(1). Existing calendar tests cover it.
- **Size/risk:** small / low. Not a refactor for its own sake; it
  matters once games run 8+ years.

### ✅ A7. Two "charge" fields for one battery quantity
- **Where:** `PowerSource::stored_kwd` (`power.rs:94`) is set in
  constructors and read only by one test; live charge is
  `StageState::battery_kwd_remaining`.
- **Refactor:** delete `stored_kwd` (serde default keeps saves loading).
- **Size/risk:** small / none.

---

## B. Triplicated R&D pipeline

> Detailed plan for this whole section: `17_1_RD_PIPELINE.md`.

The three project domains were built one after another and the third
copy is very close to the first two. This is the largest structural
opportunity in the codebase.

### B1. One state machine written three times  ⚠ data-structure
- **Where:** `engine_project.rs:303-375, 528-732`,
  `reactor_project.rs:89-414`, `rocket_project.rs:16-186`.
- **How close:** engine and reactor status enums have identical
  variants and field names. `promote_to_in_design`, `new_proposed`,
  `start_revision`, `discovered_flaw_count`, `testing_level` and the
  status-clamp in `apply_edit` are verbatim copies. `apply_daily_work`
  (~105 lines each) differs in exactly three spots: which flaw
  generator, which improvement generator, and how an improvement is
  applied to the design. Rocket is a strict subset (no Proposed, no
  improvements, no tech deficiencies) and names one field differently
  (`remaining_indices` vs `remaining_flaw_indices`). Roughly 320
  duplicated lines.
- **Refactor:** `DesignProject<D: Designable>` holding the shared
  fields and phase machine; a small trait for the three divergent
  hooks. `RocketDesign` implements with no-op improvements. Type
  aliases keep call sites compiling; `#[serde(alias)]` keeps saves
  loading.
- **Size/risk:** large / medium.

### B2. Company-level dispatch: three parallel Vecs, ~15 triplicated methods
- **Where:** `company.rs` — `start_*_revision` (`365-410`, three
  *different* return shapes), `add_team_to_*` / `remove_team_from_*`,
  `visible_*`, `find_*`, `promote_proposed_*` / `delete_proposed_*`,
  `steal_engineering_team_to_*` (`1697-1731`), NRE accrual ×3
  (`1863-1877`). `ProjectKind { Engine, Rocket, Reactor }` already
  exists at `company.rs:40-44` but is private.
  `busiest_engineering_donor` returns a tuple whose middle element is
  always literally `0` (`:1676`).
- **UI side:** `ui/mod.rs:1323-1411, 1423-1547, 1578-1680` — the
  `a`/`+`/`-`/`r`/`x` key arms are the same ~60 lines three times.
  `draw_engines_tab` / `draw_reactors_tab` / `draw_rockets_tab` share
  status strings, gauges, and flaw / improvement / tech-deficiency list
  blocks (~70 lines each; four byte-identical flaw-list copies).
- **Refactor:** make `ProjectKind` public and the single entry point;
  one `add_team(kind)`, `remove_team(kind)`, `start_revision(kind) ->
  RevisionPlan`, etc.; one `handle_project_pane_key(kind, key)` with a
  per-kind match only for the genuinely different keys; shared
  `push_flaws` / `push_improvements` / `push_tech_deficiencies` draw
  helpers.
- **Size/risk:** large / medium. Removes ~250 lines. Can be done
  incrementally before or after B1.

### B3. Three `WorkEvent` enums, 17 `GameEvent` variants that are 6 shapes × 3  ⚠ data-structure
- **Where:** `engine_project.rs:871`, `rocket_project.rs:57`,
  `reactor_project.rs:416`; `company.rs:1787-1859` (three ~25-line
  translation matches); `event.rs:16-46, 93-97` with three display
  arms and three category arms each. `ResearchTick` carries paired
  per-kind fields.
- **Refactor:** one `WorkEvent`; either a single `GameEvent::Project {
  kind, name, event }` (touches save format; `event.rs:594` has a
  compat test) or, less invasively, one
  `work_event_to_game_event(kind, name, we)` free function.
- **Size/risk:** medium / medium. `tick_daily_research` drops from
  116 lines to ~40.

### ✅ B4. Tech-deficiency apply/revert is four copies of one ~50-line block
- **Where:** `advance.rs:45-98, 100-144, 146-199, 201-245` — engine
  revert, engine apply, reactor revert, reactor apply. Apply and revert
  are exact inverses; engine vs reactor differ only in which design
  stat a kind touches.
- **Refactor:** `trait TechTarget { fn apply_deficiency(&mut self,
  kind, Direction) }` on `EngineDesign` and `ReactorDesign`; one
  generic `resolve_tech_attempts` in a new `game_state/tech_ops.rs`.
- **Size/risk:** medium / low. Removes ~150 lines from `advance_day`.
  Independent of B1; a good first step toward it.

### ✅ B5. Index-based references across mutation boundaries  ⚠ data-structure
- **Where:** `ResearchTick.newly_designed_engines: Vec<usize>` and
  `tech_def_attempts: Vec<(usize, _)>` are raw indices consumed after
  the tick. `Revising { remaining_flaw_indices, ... }` indexes into
  `flaws`, requiring a shift-fixup loop in three places
  (`engine_project.rs:598`, `rocket_project.rs:133`,
  `reactor_project.rs:312`) and the guard at `game_state/mod.rs:338`.
  `RetireTarget` (`company.rs:90-100`) already shows the id-based
  pattern and explains why.
- **Refactor:** `remaining_flaw_ids: Vec<FlawId>`; `ResearchTick`
  returns project ids. `#[serde(alias)]` or a one-time migration for
  mid-revision saves.
- **Size/risk:** medium / medium. Simplifies B1 if done first.

### B6. Flaw description pools: nine ~35-line functions with one shape
- **Where:** `flaw.rs:247-583` (~280 lines of `match consequence {
  three arrays of six }; pick`), plus three identical count loops at
  `flaw.rs:104-144, 205-225`, and an inline re-implementation of the
  rocket trigger roll at `game_state/mod.rs:359-370`.
- **Refactor:** `const FlawPool { degradation, part_loss, stage_loss }`
  tables, a `FlawDomain` enum resolving `(pool, endurance_fraction)`,
  one `generate_flaws(domain, ...)`. Existing pool tests pin behaviour.
- **Size/risk:** small-medium / low. ~200 lines removed.

### B7. `EngineProject::new` / `apply_edit` / `design_variant` derive the design three times
- **Where:** `engine_project.rs:389-418, 478-503, 685-702`. Also a
  dead `let _ = work_required;` and an unreachable `< 0.0` clamp at
  `:518-519`.
- **Refactor:** `EngineSpec { name, cycle, preset, scale }` and
  `EngineBaseline::design(spec, vacuum)`. Removes four
  `too_many_arguments` allows (`engine_project.rs:379, 446`,
  `company.rs:448, 476`).
- **Size/risk:** small / low.

---

## C. The daily tick and flight simulation

### C1. `advance_day` is a 686-line script
- **Where:** `advance.rs:16-700`. Month-start work is one `if` block
  (`:246-449`) covering salaries, economy, geopolitics, market
  modifiers, events, tech unlocks, contract generation, campaigns,
  financials. January is checked three different ways (`:295`, `:327`,
  `:478` — the last redundantly). Only `auto_revise_projects` has been
  extracted.
- **Refactor:** one `tick_*` method per section in the order they run
  (`tick_research`, `tick_tech_deficiencies`, `tick_month_start`,
  `tick_year_start`, `tick_market_pipeline`, `tick_manufacturing`,
  `tick_parked_fleet`, `tick_idle_pause`); `advance_day` becomes a
  ~40-line ordered list plus the date roll. A per-subsystem trait is
  *not* recommended: the systems borrow overlapping parts of
  `GameState` and the order is load-bearing for determinism.
- **Size/risk:** medium / low (pure extraction).

### ✅ C2. Event emission: 42 copies of two lines, and three conventions
- **Done:** `GameState::log` / `emit` / `emit_all`; all 42 pairs, the
  three Vec-returning helpers, and the 15 UI/policy sites use them.
  **Left as is:** the ~10 hand-placed pause sites. Deriving pause from
  `importance()` would change behaviour (ContractAwarded pauses but is
  Notable), so that is a design call, not a refactor.
- **Where:** `self.event_log.push(self.date, evt.clone());
  events.push(evt);` appears 22× in `advance.rs`, 16× in
  `market_ops.rs`, 4× in `flight_ops.rs`. But `advance_flights` and
  `resolve_arrived_flight` push to `events` *without* logging and the
  caller logs afterwards (`advance.rs:560-563`), while UI (`ui/mod.rs`,
  11 sites) and `policy.rs` (5 sites) get `Option<GameEvent>` back from
  `Company` and must remember to log it. Pause-on-event is decided by
  hand at ~10 sites rather than from `GameEvent::importance()`.
- **Refactor:** `GameState::log(evt)` and `GameState::emit(events,
  evt)`; or, simplest, log the whole `events` vec once just before the
  date rollover and delete every inline push in the tick. Pause policy
  derives from importance in one place.
- **Size/risk:** small / low. ~90 lines removed.
- **Also:** `GameEvent::MoneyChanged` and `DayAdvanced` are never
  constructed; `EconomicShift` is overloaded for tech unlocks,
  geopolitics and market emergence (`market_ops.rs:853, 875, 897, 990`)
  with a stringly-typed `condition`.

### C3. Parked spacecraft duplicate the in-flight per-day tick
- **Where:** `advance.rs:573-663` (parked power tick + PerDay flaw roll
  with a local `ScFlawRef`) vs `flight_ops.rs:388-403, 452-500,
  756-770` (same on `Flight` with `RocketFlawRef`). They have already
  drifted: the in-flight version breaks after a `StageLoss`, the parked
  one doesn't.
- **Refactor:** `roll_perday_rocket_flaws(design, rocket, table, rng)`
  shared by both; move the parked-fleet section into `flight_ops.rs`
  as `tick_parked_spacecraft`.
- **Size/risk:** medium / low.

### C4. `advance_flights` is 565 lines; launch and mid-flight duplicate flaw rolls
- **Where:** `flight_ops.rs:334-899`. Overexpansion roll:
  `launch.rs:217-267` vs `flight_ops.rs:589-636` (the flight copy
  records no `FlawActivation` and never applies the Isp penalty).
  Engine flaw roll → activation → consequence → StageLoss check:
  `launch.rs:101-140`, `launch.rs:141-176` (contracted engines, a copy
  with `ce` for `ep`), `flight_ops.rs:636-702`. "Disable this stage"
  (zero count, thrust, isp, propellant) written four times. Sea-level
  ambient `101_325.0` hardcoded at `launch.rs:219`, `draw.rs:2387`.
- **Refactor:** `roll_overexpansion(stage, ambient, rng)` and
  `roll_engine_flaws(stage, flaws, rng) -> FlawRoll`, called from both;
  `Stage::disable()`; extract `tick_flight(flight, tables, rng)` as a
  free function (the three local snapshot structs exist only to dodge
  the `&mut self` borrow). Consequence appliers
  (`launch.rs:352-442`) belong in `flaw.rs`.
- **Size/risk:** medium / medium.

### C5. Sealed-bid resolution duplicated between single contracts and campaign blocks
- **Where:** `market_ops.rs:228-380` (`resolve_campaign_bids`) and
  `:496-627` (`resolve_bids`): same score closure, same "player first,
  strict `>`" tie rule, same ceiling gate, same three-way winner match,
  and the competitor `ScheduledLaunch` date computation appears a
  third time at `:71-74`. A comment says "exactly like resolve_bids",
  which is a comment doing a function's job.
- **Refactor:** `run_sealed_auction(market, ceiling, player_bid,
  competitor_bid_fn) -> AuctionResult`; `schedule_competitor_launch`.
- **Size/risk:** medium / low.

### C6. `BasicPolicy` re-implements the bid engine's stock gate and rounding
- **Where:** `policy.rs:156-186` vs `market_ops.rs:462-487` and
  `game_state/mod.rs:529-536`. The bot's gate ignores outstanding
  sealed bids (the engine's doesn't), so the bot can over-commit where
  the player's rules cannot. `policy.rs:373-386` calls `max_payload_to`
  directly, bypassing the capability cache, `retired`, and
  `survives_trip`. Rounding to $10k appears in five places.
- **Refactor:** `GameState::price_bid(cost, margin)`; have the bot call
  `free_capable_stock` and `project_can_serve`.
- **Size/risk:** small / low. The tuning harness should exercise the
  same code path players do.

### C7. Fingerprint hand-enumerates physics fields and misses `cycle`
- **Where:** `RocketDesign::fingerprint` (`rocket.rs:93-141`) hashes
  engine thrust/mass/isp/etc. but not `cycle`, while `is_low_thrust()`
  depends on it. Safe today via engine id; fragile for any new physics
  field. The two memo bodies at `game_state/mod.rs:396-432` are
  copy-paste.
- **Refactor:** hash `cycle`; consider a `#[derive(Hash)]` physics
  subset struct; one `memo(cache, key, compute)` helper.
- **Size/risk:** small / low.

---

## D. Physics and hardware model

### D1. Missing `Rocket::attached_stages()` iterator — ~10 hand-rolled copies
- **Where:** the `stage_states.get(gi).and_then(|g| g.get(si))
  .is_some_and(|ss| ss.attached)` walk plus mass sum recurs in six
  power functions (`rocket.rs:741-939`), three identical "payload above
  group" closures (`:494, 613, 645`), `attached_mass_except` (`:720`),
  `flight.rs:57-66, 364-372`, and the random-attached-stage pick in
  `flight_ops.rs:462` and `advance.rs:611`. `remaining_delta_v`
  (`:490-533`) is `group_remaining_delta_v` (`:603-640`) inside a loop.
- **Refactor:** `attached_stages(design) -> impl Iterator<(gi, si,
  &Stage, &StageState)>`, `attached_mass_kg`, `mass_above_group`.
- **Size/risk:** medium / low. Makes the CLAUDE.md "new field must
  reach every copy site" pitfall a one-site problem.

### D2. Design/instance power duplication
- **Where:** `RocketDesign::total_power_supply_w` etc.
  (`rocket.rs:162-199`) and `Rocket::total_power_supply_w` etc.
  (`:741-790`) are the same bodies with an `attached` filter. Battery
  capacity summed four times. Fuel-cell supply rule split between
  `power.rs:104-116` and `rocket.rs:56-69`. `power.rs:1-4` module doc
  still says fuel cells and reactors are stubbed.
- **Refactor:** `Stage::battery_capacity_kwd()`,
  `Stage::source_supply_w()`; design totals as the instance totals
  with every stage attached.
- **Size/risk:** small / low.

### D3. `location.rs`: string keys, linear scans, id parsing
- **Where:** `DeltaVMap::earth_moon()` is 195 lines of data via
  builder helpers (fine as code, wrong file). `location()`,
  `transfer()`, `transfers_from()` are linear scans over `&'static str`;
  `compute_heuristic` (`path_planning.rs:186-222`) calls `transfer()`
  in an n² loop per A* call, and `max_payload_to` runs ~40 A* calls per
  (design, destination). `sun_distance_au` derives distance by
  stripping `_transfer`/`_escape` suffixes from ids. Two Dijkstra
  implementations (`:539-597`, `:599-645`) and two identical heap
  wrappers (`location.rs:158`, `path_planning.rs:165`). Location is a
  `String` in four structs kept in lockstep.
- **Refactor:** move data to `location/graph_data.rs`; index map +
  adjacency vec built in the constructor; cache the heuristic per goal;
  one Dijkstra with a cost closure; a `sun_distance_au` field set by
  the builders. Later: `LocationId(usize)` newtype, or at least drop
  `Rocket.location` in favour of the owner's.
- **Size/risk:** medium / low-medium.

### D4. Per-group parameter tuples and `mass_flow × count` recomputed everywhere
- **Where:** `engine.mass_flow_rate() * engine_count as f64` at eleven
  sites across `rocket.rs`, `ui/mod.rs`, `ui/draw.rs`;
  `simulate_gravity_losses` takes a positional 4-tuple.
- **Refactor:** `Stage::mass_flow_kg_s()`, `GroupParams` struct,
  `RocketDesign::group_params(gi)`.
- **Size/risk:** small / none.

### D5. Rocket-designer sizing physics lives in `ui/mod.rs`
- **Where:** `ui/mod.rs:313-644` (`autosize_propellant`,
  `recompute_structural_masses`, TWR targets, ~300 lines) plus a
  200-line test module. TWR-target bisection over `Stage` and
  `structure::compute_structural_mass`; no UI dependency. A bot or
  headless sim would want it.
- **Refactor:** move to `src/rocket_sizing.rs` (or into `stage.rs`)
  with tests. Zero behaviour change.
- **Size/risk:** medium / none.

### D6. Physics and balance constants outside `BalanceConfig`
- **Where:** overexpansion slope `engine.rs:124`, destruction curve
  `engine.rs:151`, housekeeping W/kg `stage.rs:76`, drag model
  `location.rs:115-118`, spiral penalty `location.rs:298`, stranding
  and partial-failure cuts `launch.rs:295`, `flight_ops.rs:719, 727,
  947`, all panel/fuel-cell/RTG/battery mass and cost curves in
  `power.rs:163-268` (while `new_reactor` already takes
  `&CostsConfig`), `structure.rs:6-12`, `geopolitics.rs:30-54` (eight
  `pub const`s the module doc reasons about tuning), `economy.rs:19-73`
  transition table, tech unlock chance `market_ops.rs:846`, testing
  level thresholds (×3), improvement magnitudes
  (`engine_project.rs:767-869`, `reactor_project.rs:48-70`),
  `technology.rs` difficulty tables, `BID_PAYLOAD_MARGIN`
  (`game_state/mod.rs:96`). `main.rs` never loads a balance file; only
  `simulate` does.
- **Refactor:** new sections `physics`, `flight`, `geopolitics`,
  `economy`, `technology`; `costs.power_*`; give `main.rs` the same
  `--balance <file>` handling as `simulate.rs`. Per CLAUDE.md, update
  the test assertions in the same edit.
- **Size/risk:** small each / low. Prioritise geopolitics and flight
  cuts (things the sweep harness would actually vary) over physics
  constants that are design decisions.

---

## E. Module splits (mechanical)

All pure moves; low risk; each makes the corresponding thematic
refactor above easier to see. Proposed layouts are in the appendix.

| # | File | Lines | Split into | Notes |
|---|------|-------|-----------|-------|
| E1 | `ui/mod.rs` | 5780 | `ui/{tabs/*, designer/*, editors, modals, planner, format, widgets}` + `ui/tests/*` | 31% of the file is tests. `handle_input_mode_key` is 874 lines. |
| E2 | `ui/draw.rs` | 4324 | same per-feature files as E1 | `draw_modal` is 885 lines, one match with 30 arms. Every feature is split across two files 1500 lines apart. |
| E3 | `rocket.rs` | 2140 | `rocket/{mod, staging, power, stats, fingerprint}` | 47% tests. Falls along existing `// ───` banners. |
| E4 | `company.rs` | 1889 | `company/{mod, projects, retire, production, floor, staffing, research}` | `retire.rs` and `floor.rs` are already self-contained with doc essays. |
| E5 | `contract.rs` | 1714 | `contract/{mod, market, campaign, archetype, templates}` | ~600 lines are pure data literals. |
| E6 | `game_state/tests.rs` | 3381 | `game_state/tests/{clock, rockets, manufacturing, flights, reactors, launch, geopolitics, retire}` | 94 tests, no inner modules. Two UI-table tests belong in `ui/draw.rs`. |
| E7 | `rocket_project.rs` | 796 | move `max_payload_to`, `trip_power_along`, `survives_trip`, `payload_table*` (`:210-399`) to `rocket_perf.rs` | Vehicle performance analysis, not workflow. |
| E8 | `location.rs` | 1380 | `location/{mod, graph_data}` | See D3. |

---

## F. UI patterns

### F1. ~20 hand-rolled Up/Down cursor pairs
- **Where:** `ui/mod.rs` at 2141, 2195, 2233, 2249, 2379, 2415, 2461,
  2496, 2523, 2547, 2573, 2612, 2673, 3188, 3366, 3508, 3614, 3742,
  3865-3940 (six copies of the bound lookup in `handle_up`/
  `handle_down`), and `main.rs:91-97`. The `FlySelect*`/`Dock*`/
  `Undock*` arms re-match `&mut self.input_mode` inside the arm just
  to bump the index.
- **Refactor:** `Cursor { index }` with `up()`, `down(len)`,
  `clamp(len)`; one `list_len_for(tab)` shared with the draw side.
- **Size/risk:** medium / low.

### F2. 13 copies of the same list-modal rendering in `draw_modal`
- **Where:** marker + Yellow style + lines + DarkGray hint + bordered
  block + `Paragraph`: `draw.rs:2721, 2792, 2896, 3195, 3247, 3290,
  3323, 3358, 3403, 3435, 3465, 3497, 3532, 3866, 4200`. 38
  `Paragraph::new(lines).block(Block::default()...)` sequences. Modals
  disagree on border colour (Cyan vs Yellow) and marker width for no
  stated reason.
- **Refactor:** `selectable_line(text, selected)`,
  `selected_style(bool)` (reusing `SELECTION_BG`, which the contracts
  table already adopted), `modal_block(title, accent)`,
  `render_list_modal(frame, area, title, lines)`.
- **Size/risk:** medium / low.

### F3. Editor sub-modal state machines duplicated
- **Where:** `ReactorEditorNameInput` / `ReactorEditorScaleInput` /
  `EngineEditorNameInput` / `EngineEditorScaleInput`
  (`ui/mod.rs:1976-2138`): four ~35-line arms with identical
  Esc/Enter/Backspace/Char structure. The `×√2` scale step with clamp
  is written four times. `BidEntry`, `CampaignBidEntry`, `RocketName`,
  `RocketPayloadInput`, and `main.rs:122-133` are the same text-field
  pattern.
- **Refactor:** `enum TextField { Name(String), Scale(String) }` and
  one `edit_text_field(key, &mut String, numeric) -> Commit | Cancel |
  Continue`; collapse six editor variants to two.
- **Size/risk:** medium / low.

### F4. Business logic in `draw.rs`
- **Where:** current mass and acceleration of an in-flight rocket
  computed in full twice (`draw.rs:1628-1657` for flights,
  `:1727-1756` for spacecraft); active-group scan at four sites;
  initial-accel at `:903` and `:2439`; per-stage power summary at
  `:2338, 2456, 4149`; engine-usage roll-up at `:863-885`.
  `ready_rockets_in_display_order` (`ui/mod.rs:33`) is the sibling of
  `Company::manufacturing_display_order` and should live next to it.
- **Refactor:** `Rocket::active_group()`, `current_mass_kg()`,
  `current_accel_m_s2()`; `Stage::power_summary(sun_au)`;
  `RocketDesign::engine_usage()`. `check_contract_readiness` at
  `draw.rs:1380` is the model: thin over `game_state`.
- **Size/risk:** small / low.

### ✅ F5. Status labels and display names mapped in three places
- **Where:** `EngineDesignStatus` → string at `draw.rs:327`,
  `draw.rs:3600`, `report.rs:224`; same for rocket and reactor.
  `EngineCycle` has no display name (`draw.rs:380-389` hand-matches
  eight variants; the editor prints `{:?}`). `FlawConsequence`
  implements `Display` yet the UI re-matches it four times with
  different words. Three string-truncation helpers (`draw::fit`,
  `report::truncate`, and an inline byte-slice in the finance tab at
  `draw.rs:1939-2000` that will panic on a multibyte char).
- **Refactor:** `label()` on each status enum,
  `EngineCycle::display_name()`, one `fit` helper.
- **Size/risk:** small / low. The byte-slice is a latent panic.

### F6. Designer state threaded through six `InputMode` variants
- **Where:** `Box<RocketDesignerState>` is carried by `RocketPickEngine`,
  `RocketPayloadInput`, `RocketDesignerLocationPicker`, `PowerEditor`,
  `Help`, `EngineEditor`, so every key `mem::replace`s it out and
  reconstructs the variant (six reconstructions in `:3183-3320`; the
  `too_many_arguments` allow at `:3164` exists only for this).
- **Refactor:** `InputMode::RocketDesigner { state, sub:
  DesignerSubMode }` so `state` stays put.
- **Size/risk:** small / medium.

### ✅ F7. `main.rs` duplicates terminal setup without the panic guard
- **Where:** `main.rs:45-58` vs `App::run` (`ui/mod.rs:1165-1195`). A
  panic in the startup menu leaves the terminal in raw mode.
- **Refactor:** `with_terminal(|term| ...)` in `ui/`, used by both.
- **Size/risk:** small / low.

---

## G. Infrastructure and tests

### G1. ID counters: seven on `Company`, four on `GameState`, three idioms
- **Where:** `company.rs:154-166`; `game_state/mod.rs:147-191`;
  `manufacturing.rs:435-467` (already wrapped in methods). External
  code bumps raw fields (`competitor.rs:209, 266-281`,
  `contract.rs:529, 686, 720` via `&mut u64`). `next_flaw_id` is
  threaded as `&mut u64` through `apply_daily_work` in three project
  types and `third_party.rs`.
- **Refactor:** `IdAllocator<T>(u64)` newtype with `next() -> T`. Keep
  field names for serde compatibility.
- **Size/risk:** medium / low.

### G2. Duplicated test fixtures
- **Where:** `temp_save_path` ×3 in `tests/`, `advance_through` ×2
  (identical including a 6-line comment), `fresh_game` ×2,
  `kerolox_engine` ×3 in `src/`, `fn bal()` ×4, `test_rng()` ×4, 26
  occurrences of the literal `200_000_000.0` instead of
  `starting_money`. `GameState::new`'s `starting_money` parameter
  exists only for tests.
- **Refactor:** `tests/common/mod.rs` and a `#[cfg(test)] pub mod
  test_util` in `lib.rs`; `GameState::test_default(seed)`.
- **Size/risk:** medium / none. Fixture drift (`advance_through`'s
  subtle date logic in two places) is the real risk.

### G3. Probe tests named after tasks
- **Where:** `tests/task5_probe.rs`, `tests/dino_probe.rs` (both
  entirely `#[ignore]`), `measure_*` in `campaigns.rs:788` and
  `seed_fairness.rs:258`; `retire_round_trip.rs` is one 30-line test.
- **Refactor:** one `tests/probes.rs` (or `simulate` subcommands);
  fold `retire_round_trip` into `save_compat.rs`.
- **Size/risk:** small / none.

### G4. `world_query` keys are ad-hoc strings with no registry
- **Where:** `seed.rs:67` and 18 call sites building keys with
  `format!`. The world/contingent RNG split itself is clean and
  centralised. `docs/seed_effects.md` documents the keys by hand.
- **Refactor:** `WorldQuery` enum with `Display`; a test that
  cross-checks the doc table.
- **Size/risk:** small / low.

### G5. Serde-default fallbacks doing work `sanitize()` could do with a seed
- **Where:** `default_markets()` (`game_state/mod.rs:284`) produces
  unperturbed templates because serde has no seed, but `sanitize()`
  runs five lines later *with* the seed. `GameSeed` needs
  `fix_after_load` because `default_contingent_rng` is a placeholder.
- **Refactor:** repair empty markets in `sanitize()`; `#[serde(from =
  "u64")]` on `GameSeed`.
- **Size/risk:** small / low. The migration story is otherwise
  healthy: versioned chain, real-save corpus, `#[serde(default)]` used
  correctly.

### ✅ G6. Dead code (partial)
- **Removed:** `rocket_designs`, the three unused `ContractStatus`
  variants, `MoneyChanged`/`DayAdvanced`, `pending_*_orders`,
  `nro_surge`/`debris_active`, `Rocket::burn` (+ its private helper),
  `effective_isp_at`/`effective_exhaust_velocity_at`, the free
  `payload_table*` fns, `surface_location_ids`, `TransferAnimation`,
  the `_low_thrust` param, the `draw::format_money` wrapper, and the
  stale Phase comments and orphaned docs.
- **Kept deliberately:** `company_mut` (the `CompanyRef` seam is half
  built: `Flight.company` is populated), `total_battery_capacity_kwd`
  (a test uses it for a real invariant; D2 will reshape it),
  `flight::build_route` (test-only; C4 restructures that area),
  `can_aerobrake` and `SurfaceProperties` physics fields (design data
  with stated intent).
Zero-caller items found: `Company::rocket_designs` (only ever
`Vec::new()`), `ContractStatus::{Completed, Failed, Expired}`,
`GameEvent::{MoneyChanged, DayAdvanced}`, `GameState::company_mut`,
`Manufacturing::pending_engine_orders` / `pending_stage_orders`,
`Geopolitics::nro_surge` / `debris_active`, `Rocket::burn`,
`Rocket::total_battery_capacity_kwd`, `PowerSource::stored_kwd`,
`EngineDesign::effective_isp_at` / `effective_exhaust_velocity_at`,
`Stage::burn_time_s` (test-only while two sites recompute it inline),
`flight::build_route`, `rocket_project::payload_table*`,
`surface_location_ids`, `Transfer.animation`, `Transfer.can_aerobrake`,
`SurfaceProperties.atmosphere_density` / `orbital_velocity`,
`_low_thrust` param on `reachable_destinations_multistage`. Stale
"Phase 1 / Phase 3" comments at `reactor_project.rs:6, 98, 130`,
`company.rs:625, 1831`. Several orphaned doc comments attached to the
wrong item (`draw.rs:1259, 4025`, `flaw.rs:50`, `ui/mod.rs:319-344`).

---

## Suggested order

1. **Warm-ups, no discussion needed:** A2, A3, A5, A7, C2, G6, F5, F7.
   Each is under an hour and fixes a real single-source-of-truth gap.
2. **Self-contained wins:** A1, B4, B6, C1, C3, C5, C6, D1, D2, D4, D5,
   F1, F2, F3, F4, G2.
3. **Mechanical splits (E1-E8):** do these *after* the thematic
   dedup in the same area, so the moved code is the smaller version.
   E6 (tests) and E3 (rocket.rs) can go any time.
4. **Needs a plan document first (⚠):** B1, B3, B5 as one project
   (order: B5 → B1 → B2/B3), then A4 (delta-v unification), then D3
   (`LocationId`) and G1.
5. **Ongoing:** D6 constant lifting, one section at a time as the
   sweep harness needs them.

## Questions for you

1. Is unifying the three project types (B1) worth a generic
   `DesignProject<D>`, or would you rather keep three concrete types
   and only dedupe at the `Company` dispatch layer (B2) and the
   tech-deficiency layer (B4)? The generic is the bigger win but is
   the one change here that reshapes how the code reads.
   USER:  Conceptually I think that would be best.  There's a certain amount of
   specialization I want in the context of prevelance and nature of flaws but
   mostly the behavior should be symmetrical.
   CLAUDE: Agreed. The generic core will hold the phase machine and the
   shared fields; flaw prevalence and nature stay per-domain through the
   `Designable` hooks (which flaw pool, endurance fraction, complexity
   source), which is where B6's `FlawDomain` slots in. I'll write the B
   plan as its own document (B5 → B1 → B2/B3) before touching code.
2. For B3, do you want `GameEvent` collapsed to a `Project { kind,
   event }` shape (save-format change, cleaner) or the free-function
   translation (no format change)?
   USER:  We're just talking about the GameEvents that start with
   Engine/Rocket/Reactor?  Yeah, probably worth doing now even with a save
   format change to match (1).
   CLAUDE: Yes, exactly those. 17 variants today: DesignStarted /
   DesignComplete / FlawDiscovered / RevisionComplete for all three
   kinds, ImprovementDiscovered / ImprovementActualized /
   TechDeficienciesFound for engine and reactor, plus the rocket-only
   RocketDesignModified. They become one `Project { kind, name, event }`
   variant. AutoRevisionStarted is already kind-agnostic. The compat
   corpus in tests/saves will need a migration so old event logs still
   load; that goes in the B plan.
3. A4 could change which contracts the game judges feasible, since
   the launch sim currently compares vacuum delta-v against a
   gravity-loss-priced route. Do you want that treated as a balance
   change with sim-band checks, or is aligning them clearly correct?
   USER:  We certainly *want* to do it but we should double check the
   impact on teh sims and also the performance anyways.
   CLAUDE: Will do. Plan: build the unified DesignPerformance, run the
   sim bands before and after, and profile the capability cache path.
   If it slows the draw path, we can show reachability in fewer places
   rather than compute it everywhere per frame (your note). The
   per-frame payload_table call in the designer is the obvious first
   candidate to trim.
4. Any of the E-splits you'd rather *not* do? Some people prefer big
   files with good banners.
   USER:  Go ahead and split.
   CLAUDE: Will do, after the dedup in each area so the moved code is
   the smaller version. E6 (tests) and E3 (rocket.rs) can go earlier.
