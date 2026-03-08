# Phase 5B: Spacecraft Persistence & Propellant Tracking

## Goal

After a flight arrives at its destination, the rocket persists in orbit/on the surface with its remaining propellant. The player can send it on new missions. A delta-v planner lets the player plan multi-hop trips before committing.

## Current State

- Flights in transit work (Phase 5A). On arrival, the flight is resolved and removed.
- `Flight` stores a `design: RocketDesign` (degraded clone from launch sim). This is the vehicle blueprint.
- `Rocket` struct exists in `rocket.rs` with per-stage propellant tracking (`StageState`), burn/jettison operations, and `remaining_delta_v()`.
- The launch simulation already computes a degraded design. But no `Rocket` instance (with propellant state) is created — the flight just carries the design.
- `Flight.route` has `FlightLeg` entries with `delta_v_cost` per leg, but propellant isn't actually consumed per-leg; it's all-or-nothing at launch.

## Decisions

- **Stranded spacecraft**: Mark as debris for now, defer rescue to a later phase.
- **Staging**: Auto-stage — burn lowest group first, jettison when empty, continue with next. Future: player choice for parallel stages (ion drives, solar sails), but ordered stages always burn in order.
- **Delta-v budget**: Show on flight command modal (remaining vs required).
- **Payload pickup in space**: Not for now — contracts assume Earth origin. Need origin+destination contracts before spacecraft can pick up payloads in orbit.
- **Spacecraft persistence**: Opt-in. Normal launches (`[L]`) don't create spacecraft on arrival (disposable). A separate key (`[S] Sustain`) creates a persistent spacecraft flight.
- **Planner start location**: Earth surface by default, but player can change it. Supports simulating sub-vehicles like a lunar lander starting in low lunar orbit.

## Design

### 5B.1: Instantiate a Rocket at Launch

When a flight is created in `launch_rocket()`:
- Instantiate a `Rocket` from the degraded design with the payload mass
- Store the `Rocket` on the `Flight` alongside the `RocketDesign`
- The `Rocket` tracks per-stage propellant remaining
- New field on `Flight`: `persist: bool` — whether to create a spacecraft on arrival

### 5B.2: Consume Propellant Per Leg

When `advance_flights()` completes a leg:
- Consume propellant from the rocket for that leg's `delta_v_cost`
- Auto-staging: burn lowest attached group first. When a group is exhausted, jettison all stages in it, move to next group.
- New helper `Rocket::burn_sequential(design, target_dv) -> f64` that burns through groups in order, jettisoning as needed, returning actual dv achieved.
- After consuming propellant, check if the rocket has enough remaining delta-v for the rest of the route.
- If insufficient dv remains for the next leg, mark flight as `FlightStatus::Stranded`.
- Stranded flights become debris — persist at their location but are non-functional. Event generated.

### 5B.3: Persist Arrived Rockets

When a flight arrives at its final destination:
- If `flight.persist` is true, add the rocket to `GameState::spacecraft: Vec<Spacecraft>`
- If `flight.persist` is false, the rocket is discarded (current behavior for simple contract launches)
- `Spacecraft` struct:
  ```
  SpacecraftId(u64)
  name: String
  rocket: Rocket        // current propellant state
  design: RocketDesign  // degraded blueprint for dv computation
  location: String      // current orbit/surface
  ```
- Payloads are resolved on arrival (contract delivery/payment), spacecraft persists empty.

### 5B.4: Flight Commands — New Flights from Orbit

Send a persisted spacecraft on a new mission:
- Select spacecraft from Launches tab
- Choose destination (free flight only — no contract pickup in space yet)
- Route computed from `spacecraft.location` to destination
- Propellant continues from current state — no refueling
- Remove from `spacecraft` list, create new `Flight` (always with `persist: true`)
- Flight modal shows delta-v budget: "Remaining: X m/s | Required: Y m/s"

### 5B.5: Delta-V Planner

Interactive planning tool accessible from the Launches tab (`[P] Plan`):

**Inputs:**
- Select a rocket design (from completed rocket projects) OR an existing spacecraft
- If design: specify payload mass, configurable start location (default: Earth surface)
- If spacecraft: uses current location, remaining propellant, no payload

**Interactive loop:**
1. Show current state: location, remaining delta-v, payload mass, stages remaining
2. List reachable destinations with delta-v cost for each
   - Only show destinations where required dv <= remaining dv
   - Show thrust requirement too (some transfers need minimum thrust)
3. Player selects a destination → planner simulates the burn:
   - Deduct propellant, auto-stage as needed
   - Show new state: arrived at destination, updated remaining dv
4. Player can also `[D] Drop payload` — this is recorded as an action in the plan (not a separate flag), so it interacts properly with undo
5. Player can continue selecting destinations (multi-hop planning)
6. `[U] Undo` reverses the last action (leg or payload drop), restoring the previous rocket state
7. `[Esc]` to cancel, `[Enter]` to confirm and execute (if using a real spacecraft)

**Key detail:** The planner works on a *clone* of the rocket state. Nothing is committed until the player confirms. For design-mode planning (not a real spacecraft), it's purely informational — helps the player evaluate rocket designs.

**Planner state:**
- Source: either `DesignPlan { project_id, payload_kg, start_location }` or `SpacecraftPlan { spacecraft_id }`
- Current `Rocket` and `RocketDesign` (cloned, being simulated)
- `actions: Vec<PlanAction>` — the plan so far, where `PlanAction` is:
  - `Leg { from, to, dv_cost }` — a flight leg
  - `DropPayload { mass_dropped }` — payload release
- `snapshots: Vec<Rocket>` — rocket state before each action (for undo)
- `current_location: String`
- Destination list cursor position

When confirmed for a real spacecraft, we execute the planned route as a new Flight with the full multi-leg route.

### 5B.6: UI Changes

**Launches tab** layout (top to bottom):
1. Ready Rockets (existing)
2. Spacecraft (new) — arrived rockets with name, location, remaining dv
3. In Flight (existing, enhanced with propellant info)
4. Launch History (existing)

**Keys:**
- `[L]` Launch a ready rocket (disposable — no spacecraft on arrival)
- `[S]` Sustain launch — launch with spacecraft persistence (rocket survives as spacecraft on arrival)
- `[F]` Fly a spacecraft to a new destination
- `[P]` Open delta-v planner

**In-Flight section** enhancement:
- Show remaining delta-v and current leg for in-transit flights

**Delta-v planner modal:**
- Top: current state (location, dv remaining, payload, stages)
- Middle: reachable destinations list with costs
- Bottom: planned route so far (list of actions)
- Keys: `[Enter]` select destination, `[D]` drop payload, `[U]` undo, `[Esc]` cancel

## Implementation Order

1. `Rocket::burn_sequential()` helper in `rocket.rs`
2. Add `Rocket` to `Flight`, instantiate at launch, add `persist` flag
3. Consume propellant per-leg in `advance_flights()`, handle stranding
4. `Spacecraft` struct, persist arrived rockets (when `persist: true`)
5. `[S]` sustain launch key
6. Flight-from-orbit command (`[F]` key)
7. Delta-v planner modal (`[P]` key) with undo support
8. UI: spacecraft list, enhanced in-flight display, delta-v budget in modals
9. Tests throughout

## Files Modified

- `src/rocket.rs` — `burn_sequential()` helper
- `src/flight.rs` — add `Rocket` to `Flight`, `persist` flag, propellant consumption per leg
- `src/game_state.rs` — `Spacecraft` struct, `spacecraft` vec, flight-from-orbit, planner execution
- `src/ui/mod.rs` — `[S]` sustain, `[F]` fly, `[P]` planner key, planner state machine
- `src/ui/draw.rs` — spacecraft section, planner modal, enhanced in-flight display, delta-v budget
- `src/event.rs` — `SpacecraftStranded`, `SpacecraftFlight` events
