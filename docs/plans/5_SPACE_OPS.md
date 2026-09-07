# Phase 5: In-Space Operations & Missions

## Overview

Currently launches are instant — the player picks a rocket and contract, the simulation runs, and the outcome is immediate. Phase 5 makes space real: rockets travel through the location graph over time. This opens the door for future fuel delivery, crew transport, probe deployment, and reusability.

## Scope: Sub-phase 5A Only (Flights in Transit)

5B (spacecraft persistence) and 5C (multi-leg mission planning) are deferred to later phases.

1. `Flight` data structure and `FlightStatus` enum
2. Launch creates a flight instead of instant resolution
3. Daily flight processing (transit countdown, arrival resolution)
4. UI: in-transit flights on Launches tab with ETA
5. New events: `FlightDeparted`, `FlightArrived`
6. Contract payment on arrival (not launch)

---

## 5A.1: Flight Data Structure

**New file:** `src/flight.rs`

```rust
pub struct FlightId(pub u64);

/// What a flight is carrying. Expandable later for fuel, crew, probes, etc.
pub enum Payload {
    ContractDelivery {
        contract_id: ContractId,
        payload_kg: f64,
    },
    TestMass {
        mass_kg: f64,
    },
}

pub struct Flight {
    pub id: FlightId,
    pub rocket_name: String,
    pub rocket_project_id: RocketProjectId,
    pub design: RocketDesign,          // degraded clone — what's actually flying
    pub payloads: Vec<Payload>,        // multiple payloads, expandable
    pub current_location: &'static str,
    pub route: Vec<FlightLeg>,         // planned legs
    pub current_leg: usize,            // index into route
    pub leg_days_remaining: u32,       // burn_days + coast_days combined
    pub status: FlightStatus,
    pub flaws_activated: Vec<FlawActivation>,
    pub launch_date: GameDate,
}

pub enum FlightStatus {
    InTransit,
    Arrived,    // reached final destination, awaiting resolution
    Failed { reason: String },
}

pub struct FlightLeg {
    pub from: &'static str,
    pub to: &'static str,
    pub delta_v_cost: f64,
    pub burn_days: u32,     // time to execute the burn (trivial for chemical, significant for ion)
    pub coast_days: u32,    // ballistic transit time after the burn
}
```

Design decisions:
- Store the degraded design clone (what's actually flying)
- `payloads: Vec<Payload>` instead of single `payload_kg` + `contract_id`
- `FlightLeg` has separate `burn_days` and `coast_days` for future ion drive support
- `leg_days_remaining` = burn_days + coast_days, counted down as a single value

CLAUDE: For burn time calculation, I think your suggestion of adding burn time to transit is cleanest. We compute burn time as `delta_v / (thrust / mass)` in seconds, convert to days. For chemical rockets this is negligible (seconds → 0 days), but it sets the pattern for ion drives later. I'd store `burn_days` and `coast_days` separately on `FlightLeg` for visibility, but combine them into the single `leg_days_remaining` countdown. Does that work?
USER:  That works.

CLAUDE: For staging flaws mid-flight — when starting a new leg, we check if the current stage group is exhausted and needs jettisoning. We could roll separation/staging flaws at that point. This means categorizing some flaws as "triggered at staging" vs "triggered at launch" via a `FlawTrigger` enum. Should we add this now, or keep it simpler for Phase 5 and just roll all rocket flaws at launch as currently done? The TODO item "Add flight endurance related flaws that can trigger as day advances" suggests you want this eventually, but it could be a follow-up.
USER:  Flight endurance is a seperate thing for things like the Apollo 13 disaster.  Make a new TODO for this and put it
at the head of the queue for staging flaws or engine flaws in the new stages to trigger between FlightLegs

---

## 5A.2: Launch Creates a Flight

Modify `GameState::launch_rocket()`:
- Flaw rolling still happens at launch (flaws activate on ignition/ascent)
- If catastrophic failure at launch (stage loss on first stage), resolve immediately as Failure — no flight created
- Otherwise, compute route via `shortest_path()`, compute burn time per leg, create `Flight`, add to `GameState::active_flights`
- Rocket consumed from inventory at launch time (same as now)
- No contract payment at launch — deferred to arrival

Burn time per leg:
- Determine which stage group provides thrust for this leg
- `burn_time_s = delta_v_cost / (total_thrust / current_mass)`
- `burn_days = ceil(burn_time_s / 86400)`
- For chemical rockets this will be 0 days for most burns

---

## 5A.3: Daily Flight Processing

In `GameState::advance_day()`, after manufacturing:

For each active flight with `InTransit` status:
1. Decrement `leg_days_remaining`
2. When a leg completes (`leg_days_remaining == 0`):
   - Update `current_location` to the leg's destination
   - Deduct propellant from the rocket's stages for this leg's delta-v
   - If this exhausts a stage group, jettison it (future: roll staging flaws here)
   - Advance `current_leg`, set up next leg's `leg_days_remaining`
   - Emit `FlightLegComplete` event
3. When all legs complete: set status to `Arrived`

For `Arrived` flights:
- Resolve each payload (contract payment, reputation update)
- Emit `FlightArrived` event
- Create `LaunchRecord` and add to launch history
- Remove from active flights

---

## 5A.4: UI — Flights Display

**Launches tab** changes:
- New section: "In-Flight" between "Ready Rockets" and "Launch History"
- For each flight: rocket name, destination, current location, ETA (days remaining)
- Color: yellow for in-transit, green when arrived (briefly before removal)

**Launch modal** changes:
- After selecting contract, show the planned route with legs and total transit time
- "Launch? ETA: N days" confirmation

**New events:**
- `FlightDeparted { rocket_name, destination }`
- `FlightArrived { rocket_name, destination }`
- `FlightLegComplete { rocket_name, location }` (optional, might be noisy)

---

## 5A.5: Contract Deadlines

- Contract payment moves to arrival (pay on delivery)
- If a flight's contract expires while in transit, the contract is lost but the rocket keeps flying
- The launch modal should warn if estimated arrival is after the contract deadline

---

## Key Files
- `src/flight.rs` — new module: Flight, FlightLeg, FlightStatus, Payload
- `src/game_state.rs` — active_flights on GameState, daily processing, launch modification
- `src/launch.rs` — split: flaw simulation stays at launch, contract resolution moves to arrival
- `src/event.rs` — new event variants
- `src/ui/draw.rs` — flights display on Launches tab, route in launch modal
- `src/ui/mod.rs` — launch flow changes
- `src/lib.rs` — register flight module

## Verification
- `cargo build && cargo test`
- Existing launch tests still pass (flaw simulation unchanged)
- New tests: flight creation, daily transit processing, arrival resolution, burn time calc
- GUI: launch a rocket, see it in transit on Launches tab, watch it arrive, get paid
