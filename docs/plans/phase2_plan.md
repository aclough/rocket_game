# Phase 2: Engineering Teams & Engine Design (Revised)

## Context

Phase 1 gave us a running game loop with time, events, and a TUI. Phase 2 makes money matter and gives the player something to do: hire engineering teams, design engines with fixed performance parameters per cycle type, discover and fix flaws through testing. By the end, the player can spend money hiring people, direct them to design engines, and iterate on those designs — the first real gameplay loop.

---

## 1. Data Structures

### `rust/src/team.rs` — Engineering Teams

```rust
struct TeamId(pub u64);

struct EngineeringTeam {
    id: TeamId,
    name: String,
    monthly_salary: f64,        // $150K/month (~8-10 engineers)
}
```

- Teams are interchangeable and assigned per-project (not per-team)
- Each project tracks how many teams are assigned to it
- Multiple teams on same project: `sqrt(num_teams)` total work per day
- `monthly_salary`: $150K
- Hiring cost: 1x monthly salary ($150K one-time)
- No firing for now (morale loss — deferred to long-run TODO)

### `rust/src/engine_design.rs` — Engine Design Workflow

Extend existing `EngineDesign` with workflow state, or create a wrapper:

```rust
enum EngineDesignStatus {
    InDesign { work_completed: f64, work_required: f64 },
    Testing { work_completed: f64 },
    Complete,
}

struct EngineProject {
    design: EngineDesign,        // the physical engine parameters
    status: EngineDesignStatus,
    flaws: Vec<Flaw>,            // generated on design completion
    discovered_flaws: Vec<usize>, // indices into flaws that have been found
    revision: u32,               // how many times revised
    teams_assigned: u32,         // number of teams working on this
}
```

**Design workflow:**
1. Player specifies: name, cycle type, propellant preset, scale (within a range)
2. Thrust, mass, and Isp are fixed per cycle+propellant combination (see Engine Parameters below)
3. Scale slider adjusts engine size within a range (e.g. 0.5x - 2.0x) around a baseline
4. Work required scales with complexity: `base_days * (complexity / 5)` where base_days ~ 120
5. Assign teams via project → work accumulates daily at `sqrt(teams_assigned)` rate
6. On completion: flaws are generated (count from gaussian around complexity + problems factor), emit event
7. Engine enters Testing status

**Engine Parameters (fixed per cycle+propellant):**

Each (cycle, propellant_preset) combination has a fixed thrust-to-weight ratio and a fixed Isp (at sea level or vacuum, selectable in UI). The player picks:
- Cycle type
- Propellant preset (Kerolox, Hydrolox, Methalox, Hypergolic, Solid)
- Scale (slider from range, starting at middle) — scales thrust and mass proportionally

This means the player can't accidentally create a physically impossible engine. Exact baseline values TBD during implementation (will use real-engine-inspired numbers).

### Complexity System

**Cycle complexity:**
- PressureFed: 5
- GasGenerator: 6
- Expander: 7
- StagedCombustion: 8
- FullFlow: 9

**Fuel complexity:**
- Room temperature propellants (RP-1, UDMH, NTO, Solid): 3
- Cryogenic propellants (LOX, Methane): 4
- Hydrogen (LH2): 5 (cryogenic handling + embrittlement)

**Combined complexity:** `max(fuel_complexity, cycle_complexity)`, then +1 if they are equal.

**Unexpected problems factor:** For hydrogen, add a problems factor (currently 1, representing metal embrittlement). In the future, this will be looked up from the world seed for new/exotic propellants.

The final effective complexity drives:
- Work required: `base_days * (complexity / 5)`
- Flaw count distribution (see Flaws below)

### `rust/src/flaw.rs` — Flaw System

```rust
struct Flaw {
    id: FlawId,
    description: String,
    consequence: FlawConsequence,
    activation_chance: f64,       // chance per flight this flaw triggers
    discovery_probability: f64,   // chance per testing cycle to discover
    discovered: bool,
}

enum FlawConsequence {
    PerformanceDegradation(f64),  // fraction of thrust/isp lost (e.g. 0.05 = 5%)
    EngineLoss,                    // this engine fails
    StageLoss,                     // entire stage fails
}
```

**Flaw count:** Gaussian distribution centered on `complexity + problems_factor`, standard deviation ~1.5, converted to integer (round). It's OK if sometimes there are zero flaws.

**Activation chance:** Each flaw gets a random activation chance per flight (drawn from appropriate range per consequence type — StageLoss should be rarer than PerformanceDegradation).

**Discovery probability:** `uniform(0, 1) * sqrt(activation_chance)`. This means flaws that trigger more often are also easier to discover through testing (makes intuitive sense — you're more likely to notice something that happens frequently).

**Testing:**
- Testing work cycles: 30 work units each
- At each cycle completion, roll for each undiscovered flaw using its `discovery_probability`
- Player sees: "N flaws discovered, testing level: Lightly/Moderately/Well/Thoroughly Tested"
- Player doesn't know how many flaws remain (uncertainty is the point)

**Revision:**
- Player can revise to fix a discovered flaw
- Costs: 30 work units per flaw (uniform regardless of type)
- After revision: flaw is removed, revision counter increments
- Future: revision partially resets production learning curve (Phase 3)

### `rust/src/third_party.rs` — Third-Party Engines

```rust
struct ThirdPartyEngine {
    engine: EngineProject,
    purchase_cost: f64,
    available_from: GameDate,  // when it appears in the market
}
```

- 2-3 starter engines available from game start:
  - A small solid kick motor
  - A medium kerolox engine (NK-33 analogue)
  - Maybe a small hypergolic thruster
- Generated at game init using world_query seed
- Player can buy and use immediately, but **cannot assign teams** to third-party engines
- Flaws are only discovered through **flight** (not testing)
- Provides a shortcut to launch early while designing custom engines

### Updates to `Company` in `game_state.rs`

```rust
struct Company {
    name: String,
    money: f64,
    next_team_id: u64,
    next_engine_project_id: u64,
    teams: Vec<EngineeringTeam>,
    engine_projects: Vec<EngineProject>,
    // engine_designs removed — replaced by engine_projects
    rocket_designs: Vec<RocketDesign>,
}
```

### Updates to `GameState::advance_day()`

After advancing the date:
1. For each project with teams_assigned > 0, accumulate `sqrt(teams_assigned)` work
2. Check for design completions → generate flaws, emit event
3. Check for testing cycle completions → roll for flaw discovery, emit event
4. Check for revision completions → remove flaw, emit event
5. On first of month: deduct team salaries from company money (allow going negative)

### New GameEvents

```rust
// Add to GameEvent enum:
TeamHired { team_id: TeamId, name: String },
EngineDesignStarted { engine_name: String },
EngineDesignComplete { engine_name: String, flaw_count: u32 },
FlawDiscovered { engine_name: String, flaw_description: String },
RevisionComplete { engine_name: String },
SalariesPaid { amount: f64 },
InsufficientFunds { shortfall: f64 },
```

---

## 2. Terminal UI Additions

### New tabs in sidebar

- **Teams** — team count, hire controls, monthly cost summary
- **Engines** — list of engine projects, status, start new design, assign/remove teams
- (Overview and Events stay)

### Teams tab (content pane)

```
  Engineering Teams (3)          Monthly cost: $450K
  ─────────────────────────────────────────────────
  Total teams: 3
  Unassigned: 1

  [H] Hire team ($150K)
```

Teams are interchangeable — the Teams tab just shows the pool. Assignment happens in the Engines tab.

### Engines tab (content pane)

```
  Engine Projects (1)
  ─────────────────────────────────────────────────
  ▶ Kestrel (Rev 0)          In Design [72/120]
    GasGenerator  Kerolox  200kN  280s
    Teams: 2 (effective rate: 1.41/day)

  [N] New design    [T] Start testing    [R] Revise flaw    [B] Buy 3rd-party
  [+] Add team      [-] Remove team
```

### Engine detail (when selected)

```
  Kestrel (Revision 0)
  ─────────────────────────────────────────────────
  Status: Testing [45/30 work — cycle 2]
  Cycle: Gas Generator
  Propellants: LOX 73% / RP-1 28%
  Thrust: 200 kN    Mass: 150 kg    Isp: 280 s

  Flaws: 1 discovered
    ⚠ Performance degradation: Turbopump seal leak (5% thrust loss, 12%/flight)
    ? Unknown flaws may remain

  Testing level: Moderately Tested
```

(Propellant display: 2 significant figures)

### Input flow for new engine design

Modal or sequential prompts (within the TUI):
1. Enter name
2. Select cycle type (up/down, enter)
3. Select propellant preset (Kerolox, Hydrolox, Methalox, Hypergolic, Solid)
4. Shows fixed Isp and thrust/mass ratio for the selection
5. Set scale within allowed range (slider or +/- keys, starting at middle)
6. Shows computed thrust, mass at selected scale
7. Confirm → creates EngineProject in InDesign status

---

## 3. Implementation Order

1. **Complexity system** (`balance.rs` or similar) — cycle_complexity, fuel_complexity, combined_complexity, problems_factor functions. Tests.
2. **`flaw.rs`** — FlawId, Flaw, FlawConsequence, generation function (gaussian count, random activation/discovery), discovery rolls. Tests.
3. **`team.rs`** — TeamId, EngineeringTeam, salary constants. Tests.
4. **`engine_project.rs`** — EngineProject wrapping EngineDesign + status + flaws + workflow methods + team assignment. Tests.
5. **Engine parameter tables** — fixed thrust/mass ratio and Isp per (cycle, propellant) combo, scale range. Tests.
6. **`third_party.rs`** — ThirdPartyEngine, starter engines generated from seed. Tests.
7. **Update `game_state.rs`** — Company gets teams/projects, advance_day processes work/salaries, new events. Tests.
8. **Update `event.rs`** — new event variants, importance levels, display strings.
9. **UI: Teams tab** — list, hire.
10. **UI: Engines tab** — list, assign/remove teams, start testing, revise flaw controls.
11. **UI: New design flow** — text input and selection prompts for engine design parameters.
12. **UI: Buy third-party flow** — list available, purchase.
13. **Integration testing** — full flow: hire team → design engine → test → discover flaws → revise.

## 4. Verification

- `cargo test` — all existing tests pass + new tests for complexity, flaws, teams, engine workflow, salary deduction, third-party engines
- `cargo run` — hire a team, start an engine design, watch work accumulate, see design complete with flaws, test to discover them, revise to fix
- Verify salary deduction on month boundaries
- Verify third-party engines appear and can be purchased but can't have teams assigned
- Verify negative money is allowed (no crash/block)
- `cargo build` — clean build

## 5. What This Enables

After Phase 2, the player has a reason to unpause: they're burning money on teams and racing to design engines. Phase 3 plugs in rocket design and manufacturing (using these engines), and Phase 4 gives income through contracts — completing the core gameplay loop.

## 6. Long-Run TODO Items

- Team firing with morale loss mechanic
- Loans / game over when money is deeply negative (Phase 8)
- Exotic propellant "unexpected problems" factor from world seed
