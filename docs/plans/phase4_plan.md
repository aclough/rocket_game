# Phase 4: Contracts & Launching

## Overview

Phase 4 completes the core game loop: earn money -> design rockets -> launch contracts -> earn more money. It adds contract generation, launch simulation with flaw activation, reputation tracking, and the UI to tie it together.

## Sub-phase 4a: Data Structures & Contract Generation

### New file: `rust/src/contract.rs`

```rust
pub struct ContractId(pub u64);

pub struct Contract {
    pub id: ContractId,
    pub name: String,              // e.g. "ComSat Deploy to GEO"
    pub destination: &'static str, // location id (e.g. "geo", "lunar_orbit")
    pub payload_kg: f64,
    pub payment: f64,
    pub deadline: GameDate,        // must launch by this date
    pub status: ContractStatus,
}

pub enum ContractStatus {
    Available,          // on the market
    Accepted,           // player took it
    Completed,          // successfully delivered
    Failed { reason: String },
    Expired,            // deadline passed without launch
}
```

### Contract generation

Use `seed.world_query("contracts_YYYY_MM")` to deterministically generate **1 contract per month**. Together with deadlines (30-180 days), this naturally builds a pool of 2-6 available contracts at any given time. Unaccepted contracts expire when their deadline passes.

Contract parameters:
- **Destination**: weighted random from locations the player's reputation unlocks. Early game = LEO/SSO. Higher reputation unlocks GEO, lunar, etc.
- **Payload mass**: range depends on destination (LEO: 500-20000 kg, GEO: 500-5000 kg, lunar: 200-2000 kg).
- **Payment**: base = `destination_dv_cost * payload_kg * rate_per_kg_per_dv` with variance.
- **Deadline**: 30-180 days from offer date.

No special contract types for now (no suborbital test flights, no crewed missions). Just "deliver X kg to Y orbit."

### State changes

Add to `GameState`:
```rust
pub available_contracts: Vec<Contract>,   // market pool (not player-owned)
pub next_contract_id: u64,
```

Add to `Company`:
```rust
pub active_contracts: Vec<Contract>,      // accepted by player
pub reputation: Reputation,               // factor-based reputation tracker
pub launch_history: Vec<LaunchRecord>,
```

## Sub-phase 4b: Launch Simulation

### New file: `rust/src/launch.rs`

```rust
pub struct LaunchRecord {
    pub launch_date: GameDate,
    pub rocket_name: String,
    pub contract_id: Option<ContractId>,
    pub destination: &'static str,
    pub payload_kg: f64,
    pub outcome: LaunchOutcome,
    pub flaws_activated: Vec<FlawActivation>,
}

pub enum LaunchOutcome {
    Success,
    PartialFailure { reason: String },  // within 5% dV of target, partial payment
    Failure { reason: String },          // total loss
}

pub struct FlawActivation {
    pub flaw_description: String,
    pub consequence: FlawConsequence,
    pub stage_name: String,             // which stage was affected
    pub stage_group_index: usize,       // for future animation
}
```

### Launch flow

1. **Pre-launch check**: Select rocket from inventory + contract (or test launch). Verify `max_payload_to(design, launch_from, destination) >= payload_kg`.
2. **Consume rocket**: Remove `InventoryRocket` from inventory.
3. **Roll flaws**: For each engine project used in the design, roll `contingent_rng < activation_chance` for every flaw (discovered AND undiscovered). Same for rocket project flaws. Rocket-level flaws are assigned a random `stage_group_index` at flaw generation time for determining which stage is visually affected.
4. **Apply consequences**:
   - `PerformanceDegradation(f)`: Reduce affected engine's thrust and Isp by fraction `f`. Recompute delta-v.
   - `EngineLoss`: Remove one engine from the stage's engine_count. If engine_count drops to 0, stage is lost.
   - `StageLoss`: Entire stage group fails. All delta-v from that group is lost.
5. **Determine outcome**:
   - Sufficient dV for destination -> `Success`
   - Within 5% of required dV -> `PartialFailure` (50% payment)
   - Below 95% of required dV, or stage loss -> `Failure` (no payment)
6. **Apply results**:
   - Success: full payment credited, reputation boost
   - Partial failure: 50% payment, small reputation hit
   - Failure: no payment, reputation hit
   - **All activated flaws become discovered** (player learns about the flaw for future flights)

### Launches resolve instantly

Same-day outcome for Phase 4. Phase 5 will add in-flight tracking with transit_days.

### Test launches

Player can launch without a contract (test launch). No payment, but activated flaws are discovered and reputation is affected (success = small boost, failure = hit).

### Fix: InventoryRocket.design_id

Currently `design_id` is always `RocketDesignId(0)`. Fix `game_state.rs` `advance_day()` to populate it from the rocket project when integration completes.

### Fix: Inventory.take_rocket()

Add method to remove and return a rocket from inventory by item_id.

## Sub-phase 4c: Reputation System

Reputation is the sum of discrete factors, each with independent decay:

```rust
pub struct Reputation {
    pub success_factor: f64,       // +20 per success, -20 per failure; decays by 20% each launch
    pub lost_payload_factor: f64,  // -50 on mission failure; decays by 15% each launch
    pub drought_factor: f64,       // -10 per year without launch; resets to 0 on any launch
    pub expiry_factor: f64,        // -10 per expired accepted contract; decays by 20% each contract launch
}

impl Reputation {
    pub fn total(&self) -> f64 {
        self.success_factor + self.lost_payload_factor + self.drought_factor + self.expiry_factor
    }
}
```

**Decay triggers:**
- `success_factor` and `lost_payload_factor` decay on every launch
- `drought_factor` accumulates every year without a launch, resets to 0 on any launch
- `expiry_factor` decays on every contract launch (not test launches)

**Contract availability thresholds** (temporary, will be reworked later around payload value):
```
Suborbital/LEO:    rep >= 0 (always available)
SSO/MEO/GTO:       rep >= 20
GEO:               rep >= 50
L1/L2:             rep >= 80
Lunar orbit:       rep >= 100
Lunar surface:     rep >= 150
```

## Sub-phase 4d: Financial Summary

### New tab: Finance

Shows:
1. **Burn rate**: Current monthly expenses (salaries + avg material costs)
2. **Runway**: Months of cash remaining at current burn rate
3. **12-month rolling history**: Table showing each month's income, expenses, and net

```
  Burn rate: $1.2M/mo    Runway: 14 months

  Month       Income    Expenses      Net
  ─────────────────────────────────────────
  Jan 1958       $0      $450K    -$450K
  Feb 1958       $0      $450K    -$450K
  Mar 1958    $2.0M      $850K    $1.15M
  ...
```

Track monthly income/expenses in a rolling 12-entry buffer on Company:
```rust
pub struct MonthlyFinancials {
    pub month: GameDate,     // first day of month
    pub income: f64,         // contract payments
    pub expenses: f64,       // salaries + materials + floor space
}
pub monthly_financials: VecDeque<MonthlyFinancials>,  // last 12 months
```

## Sub-phase 4e: UI

### Two new tabs:

**Contracts tab** (later "Business Development"):
- Shows available contracts from the market pool
- Shows accepted contracts with days remaining
- `[A]` Accept selected contract
- `[↑↓]` Navigate

**Launches tab**:
- Shows accepted contracts ready to launch
- Shows "Test Launch" option (no contract, just fly a rocket)
- `[L]` Launch — opens rocket selection modal
- Rocket selection modal: list inventory rockets, show payload capacity vs required, `[Enter]` to launch
- Launch result modal: outcome, flaws activated, payment — pauses game

**Tab order update**: Overview, Teams, Engines, Rockets, Mfg, **Contracts**, **Launches**, Finance, Events

**Status bar**: Add reputation display

### Launch history

Shown in Events tab via new event types (LaunchSuccess, LaunchFailure).

## Implementation Order

1. **4a-1**: Contract struct, ContractId, ContractStatus
2. **4a-2**: Contract generation logic (seed-driven, 1/month)
3. **4a-3**: GameState + Company field additions (available_contracts on GameState, active_contracts + reputation + launch_history on Company)
4. **4a-4**: advance_day() — monthly contract generation, deadline expiry
5. **4b-1**: Fix InventoryRocket.design_id, add take_rocket()
6. **4b-2**: LaunchRecord, LaunchOutcome, FlawActivation structs
7. **4b-3**: Launch simulation logic (flaw rolling, consequence application, outcome determination)
8. **4b-4**: Assign rocket-level flaws a stage_group_index at generation time
9. **4c-1**: Reputation struct with factor-based tracking and decay
10. **4d-1**: MonthlyFinancials tracking, burn rate, runway calculation
11. **4e-1**: Contracts tab UI
12. **4e-2**: Launches tab UI with rocket selection modal
13. **4e-3**: Launch result display modal
14. **4e-4**: Finance tab UI (burn rate, runway, 12-month history)
15. **4e-5**: Reputation in status bar, new event types

## New Events

```rust
GameEvent::ContractsRefreshed { count: usize }
GameEvent::ContractAccepted { name: String, payment: f64 }
GameEvent::ContractExpired { name: String }
GameEvent::LaunchSuccess { rocket_name: String, destination: String, payment: f64 }
GameEvent::LaunchPartialFailure { rocket_name: String, destination: String, reason: String }
GameEvent::LaunchFailure { rocket_name: String, reason: String }
GameEvent::PaymentReceived { amount: f64, contract_name: String }
```

## Testing Strategy

- Unit tests for contract generation (deterministic with seed)
- Unit tests for launch simulation (mock flaws, verify outcomes)
- Unit tests for reputation factor decay math
- Unit tests for flaw activation (statistical, ~1000 trials)
- Unit tests for partial failure threshold (5% dV rule)
- Integration test: contract accept -> launch -> payment -> reputation update

## Save/Load

All new GameState and Company fields must be added to serialization. Contract, LaunchRecord, Reputation, MonthlyFinancials must derive `Serialize, Deserialize`. Existing saves will need migration (default empty vecs, zero reputation factors).

## TODO items deferred

- Track average and marginal cost per rocket type (for Finance tab, future enhancement)
