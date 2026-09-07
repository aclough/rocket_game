# Phase 3: Rocket Design & Manufacturing (Revised)

## Context

Phase 2 gave us engineering teams and engine design with flaws. Phase 3 lets the player compose engines into rockets, test those rocket designs for integration flaws, then build physical copies through a manufacturing pipeline. By the end, the player can design a rocket, manufacture it, and have it sitting on the pad ready for Phase 4's contracts and launches.

---

## 1. Rocket Designer

### Overview

The player assembles a rocket from completed engine projects. The existing `RocketDesign` structure (Vec<Vec<Stage>> for sequential/parallel staging) is the target — the designer builds toward it.

### Design Flow

1. Player names the rocket design
2. Add stage groups sequentially (bottom to top):
   - For each group, add one or more stages (parallel stages in same group)
   - For each stage: pick a completed engine project, set engine count, set propellant mass
   - Structural mass is computed automatically (see below)
3. Optionally add a fairing to the top stage
4. System shows computed delta-v, total mass, estimated build cost
5. Delta-v feasibility display shows **max payload to each destination** (binary search over payload mass using location graph + atmospheric drag)
6. Confirm → creates a rocket design project with integration flaws

### Structural Mass (Computed, Not Player-Set)

The player sets propellant mass and engine count. Structural mass is derived automatically from:

**Tank mass:** Based on propellant volume (from propellant mass and density). Larger tanks need more material. `tank_mass = propellant_mass * tank_fraction` where `tank_fraction` varies by propellant density (~0.04 for dense propellants like kerolox, ~0.10 for hydrogen due to enormous tank volume).

**Thrust structure:** Sized by the max acceleration the stage produces. `thrust_struct_mass = total_thrust / (g0 * structural_g_limit)` where `structural_g_limit` ~ 40 (similar to engine TWR — the structure holding the engines). This scales with engine count and thrust.

**Aerodynamic shell:** First-stage and booster stages (exposed to airflow during ascent) need a heavier skin. `aero_mass = base_aero_fraction * (tank_mass + thrust_struct_mass)` where `base_aero_fraction` ~ 0.05 for exposed stages, 0 for upper stages.

**Interstage/adapter:** Small fixed mass per stage connection. ~200 kg per stage boundary.

So: `structural_mass = tank_mass + thrust_struct_mass + aero_mass + interstage_mass`

The player sees the computed structural mass and total stage mass. They can tweak propellant mass and engine count and see the structural mass update in real time.

### Rocket Design Project

Similar to EngineProject but for rockets:

```rust
struct RocketProject {
    project_id: RocketProjectId,
    design: RocketDesign,
    status: RocketDesignStatus,    // InDesign, Testing, Revising, Complete
    flaws: Vec<Flaw>,
    revision: u32,
    teams_assigned: u32,
    complexity: u32,               // derived from stage count + engine variety
}

enum RocketDesignStatus {
    InDesign { work_completed: f64, work_required: f64 },
    Testing { work_completed: f64 },
    Revising { remaining_indices: Vec<usize>, work_completed: f64 },
    Complete,
}
```

**Rocket complexity:** Based on integration difficulty, not engine complexity (engines were already designed). Factors:
- Number of stages
- Number of unique engine types used
- Number of parallel stages (booster configurations are harder)
- Baseline ~3-5 for simple rockets, up to ~8 for complex multi-stage/parallel designs

**Flaw generation:** Same gaussian system as engines but using rocket complexity. Rocket flaws represent integration issues: separation failures, wiring harness problems, staging timing bugs, structural resonance, etc.

**Work required:** `base_days * (complexity / 5)` where base_days ~ 60 (shorter than engines since we're integrating known components).

### Delta-V Feasibility

For each reachable destination in the location graph, binary search for the maximum payload mass the rocket can deliver (where total_delta_v(payload) >= path_delta_v including mass-dependent drag loss for surface launches).

Display as a table:
```
  Destination        Max Payload
  LEO                12,400 kg
  SSO                 8,200 kg
  GTO                 4,100 kg
  Lunar Orbit           800 kg
  Lunar Surface           — kg
```

---

## 2. Manufacturing

### Manufacturing Teams

New team type alongside engineering teams:

- Manufacturing team salary: $300K/month (~20-25 workers)
- Hiring cost: 3x monthly salary ($900K one-time)
- Manufacturing teams build physical copies of completed designs
- Engineering teams continue to handle design, testing, and revision
- Manufacturing team efficiency: `n^0.85` for multiple teams on one order

### Floor Space

Manufacturing requires floor space:

```rust
struct FloorSpace {
    total_units: u32,
    under_construction: Vec<FloorSpaceOrder>,  // units being built
}
```

- Start with 12 units of floor space
- Can purchase more: $5M per unit, ~30 days to build
- Engine manufacturing: 1 floor space unit per engine being built
- Rocket manufacturing: scales with total mass — `ceil(total_mass_kg / 10_000)` units

### Manufacturing Orders

When the player orders a rocket build, the system auto-queues all sub-orders:

```rust
enum ManufacturingOrderType {
    Engine { engine_project_id: EngineProjectId },
    Stage { rocket_project_id: RocketProjectId, stage_index: (usize, usize) },
    RocketIntegration { rocket_project_id: RocketProjectId },
}
```

**Build pipeline for a rocket:**
1. Engine builds (one per engine needed, unless already in inventory)
2. Stage builds (tank fabrication + structural assembly, one per stage)
3. Rocket integration (assembling stages + engines, lighter/faster job)

Each is a separate ManufacturingOrder visible to the player. Stage and integration orders wait until their prerequisites are in inventory.

**Engine build work:** Function of engine complexity: `base_days * (complexity / 5)` where base_days ~ 90. (Same complexity system as design, not scale-dependent.)

**Stage build work:** Based on stage mass: `base_days * (stage_mass_kg / 10_000)^0.75` where base_days ~ 60.

**Integration work:** Light job: ~30 work units per stage, plus 20 base.

**Material costs:** Paid upfront from the resource BOM when each sub-order starts.

### Inventory

Completed items go into inventory:

```rust
struct Inventory {
    engines: Vec<InventoryEngine>,   // built engines, ready to install
    stages: Vec<InventoryStage>,     // built stages (can be reused after flight return)
    rockets: Vec<InventoryRocket>,   // integrated rockets, ready to launch
}
```

- Engine builds → engine inventory
- Stage builds → stage inventory
- Integration consumes engines + stages from inventory → rocket inventory
- Stages that return from flight (future reusability) go back into stage inventory

### Resource System

Bill of materials for manufacturing, tracked by weight:

```rust
enum Resource {
    Aluminium,       // $5/kg
    Steel,           // $3/kg
    Superalloys,     // $80/kg
    Composites,      // $50/kg
    Wiring,          // $150/kg
    Electronics,     // $20K/kg
    Plumbing,        // $1.5K/kg
    SolidPropellant, // $15/kg
}
```

Each engine and stage has a BOM based on its type and mass. Material cost is computed from the BOM and paid when a manufacturing order starts. BOM composition varies by engine type (e.g. superalloys for high-performance engines, more aluminium for tanks).

### Production Learning Curves

- First build of a design costs 100% time and materials
- Each subsequent build gets slightly faster/cheaper: `cost * (total_built)^(-0.15)` (roughly 90% learning curve)
- Forgetting: if no builds for a long time, learning partially resets
- Engine revisions partially reset the learning curve (new rev = new learning)

---

## 3. Updates to Company & GameState

```rust
struct Company {
    // ... existing fields ...
    manufacturing_teams: Vec<ManufacturingTeam>,
    floor_space: FloorSpace,
    manufacturing_orders: Vec<ManufacturingOrder>,
    inventory: Inventory,
    rocket_projects: Vec<RocketProject>,
}
```

### advance_day additions

1. Process manufacturing work (similar to engineering work)
2. Check for completed sub-orders → add to inventory
3. Check if waiting orders can now proceed (prerequisites in inventory)
4. Floor space construction progress
5. Manufacturing team salaries on month start

### New GameEvents

```rust
RocketDesignStarted { rocket_name: String },
RocketDesignComplete { rocket_name: String, flaw_count: u32 },
ManufacturingOrderStarted { item_name: String },
ManufacturingOrderComplete { item_name: String },
FloorSpaceComplete { units: u32 },
ManufacturingTeamHired { name: String },
RocketReadyToLaunch { rocket_name: String },  // integration complete
```

---

## 4. UI Additions

### Updated sidebar tabs

- Overview
- Teams (now shows both engineering and manufacturing teams)
- Engines (existing)
- **Rockets** (new — rocket designer + project list)
- **Manufacturing** (new — orders, inventory, floor space)
- Events

### Rockets tab

```
  Rocket Projects (1)
  ─────────────────────────────────────────────────
  ▶ Falcon (Rev 0)          Testing [45/30] Moderately Tested
    2 stages, 10 engines    ΔV: 9,400 m/s (12,400 kg to LEO)
    Teams: 1

    Flaws: 1 discovered
      ⚠ Stage separation timing fault (stage loss, 8%/flight)

  [N] New design    [+] Add team    [-] Remove team
  [T] Test    [R] Revise    [C] Complete
```

### Rocket designer modal

Multi-step flow:
1. Name the rocket
2. Add stage groups (bottom up):
   - Select engine, set engine count
   - Set propellant mass (structural mass auto-computed, shown in real time)
   - Mark if exposed to airflow (first stage / boosters)
3. Optionally add fairing
4. Preview: max payload to each destination, total mass, estimated build cost
5. Confirm

### Manufacturing tab

```
  Manufacturing
  ─────────────────────────────────────────────────
  Floor space: 8/12 used    [B] Buy more ($5M, 30 days)
  Mfg teams: 2              [H] Hire mfg team ($900K)

  Orders:
  ▶ [1] Merlin engine       [45/90 work]   Teams: 1
    [2] Falcon S1 stage     Waiting for engines
    [3] Falcon S2 stage     [12/30 work]   Teams: 1
    [4] Falcon integration  Waiting for stages

  Inventory:
    Merlin engines: 3
    Falcon S1 stages: 0
    Falcon S2 stages: 1
    Falcon rockets: 0

  [O] Order rocket build    [+] Add team    [-] Remove team
```

---

## 5. Implementation Order

1. **Structural mass computation** — tank_mass + thrust_struct + aero_shell + interstage, derived from propellant/engines/position
2. **Rocket complexity & flaw generation** — rocket-specific complexity formula, reuse flaw system
3. **`rocket_project.rs`** — RocketProject with design/testing/revision workflow (parallel to EngineProject)
4. **Max payload calculation** — binary search for max payload to each destination using location graph
5. **Manufacturing team type** — extend team.rs with TeamType, separate salary/hiring constants
6. **Resource system** — Resource enum, BOM per engine/stage type, material costs
7. **`manufacturing.rs`** — FloorSpace, ManufacturingOrder (Engine/Stage/Integration), Inventory, auto-queue pipeline, work processing, learning curves
8. **Update Company & advance_day** — integrate manufacturing pipeline, new events
9. **UI: Rockets tab** — project list, team assignment, testing/revision
10. **UI: Rocket designer modal** — stage-by-stage builder with structural mass display and max payload preview
11. **UI: Manufacturing tab** — orders, inventory, floor space, team management
12. **Integration testing** — full flow: design rocket → test → order build → engines built → stages built → integration → rocket in inventory

## 6. Verification

- `cargo test` — all existing tests pass + new tests for structural mass, rocket projects, manufacturing pipeline, inventory, learning curves, BOM costs, max payload search
- `cargo run` — design a rocket from completed engines, test it, order a build, watch sub-orders auto-queue and complete, see integrated rocket in inventory
- Verify structural mass responds correctly to propellant mass and engine count changes
- Verify floor space limits block oversized orders
- Verify stage/engine inventory requirements gate later build steps
- Verify learning curve discount on repeated builds
- Verify manufacturing team salary deduction
- `cargo build` — clean build

## 7. What This Enables

After Phase 3, the player has physical rockets ready to launch. Phase 4 adds contracts (income) and the actual launch simulation — completing the core gameplay loop where money in → design → build → launch → money out.
