# Reactor Research — Phase 3b Plan (deferred flight wiring)

**Make installed-reactor flaws activate during flight.**

Split out of Phase 3 (decision D1). Phase 3 gives reactors the full
research pipeline — flaws generated, discovered in testing, revised
away, tech deficiencies rolled and fixed — but those flaws carry a
consequence for *display only*. This phase makes them actually bite in
the flight sim, building the first power-source flight-failure path.

> Status: **implemented (pending commit).** 377 tests green, stable
> across 12 full-suite runs. Reactor flaws now roll at ignition
> (PerFlight) and daily in transit (PerDay), degrade/shut down the
> flying reactor, cascade through the existing power model (brownout
> stranding + electric-thrust derating), and are discovered on the
> owning reactor project.

## Resolved decisions

- **When flaws roll (USER):** *burst + endurance mix.* Reactor flaws are
  generated as a mix — ~30% `PerDay` (endurance, roll daily in transit),
  the rest `PerFlight` (one-shot). **A reactor runs from flight start**,
  so its `PerFlight` flaws roll **once, on the flight's first in-transit
  tick** — not when a stage's engine fires (that was the initial
  implementation and produced a confusing delay between launch and
  activation). All reactor rolling lives in the flight loop; the launch
  sim doesn't roll reactors. Tracked per-flight by `reactor_flaws_rolled`.
- **Shutdown mapping (USER):** *zero power output.* `EngineLoss` sets the
  reactor's `steady_w` to 0. `PerformanceDegradation(f)` scales
  `steady_w` by `1-f`. `StageLoss` zeroes the whole stage.
- **The cascade is already wired.** Dropping a reactor's `steady_w`
  flows through the existing power model automatically:
  `run_daily_power_tick` strands the flight on brownout (housekeeping
  unmet), and `group_effective_thrust_n` derates any electric engine
  drawing that power. No new cascade code needed.
- **Coverage.** Deployed spacecraft become `Flight`s in the same
  `active_flights` loop, so the daily-tick + burned-group roll sites
  cover both rocket launches and ion cruisers.
- **Discovery writeback** keyed by `ReactorId` (installed reactor
  snapshot's `design.id` → owning `ReactorProject`).

---

## What exists to model against

- `launch.rs::simulate_launch` (approx L60+) already rolls flaws for
  engine projects, contracted engines, and rocket projects, calling
  `apply_consequence_to_stage` and collecting `FlawActivation`s +
  discovery-writeback vectors:
  - `engine_flaw_discoveries: Vec<(EngineId, Vec<usize>)>`
  - `rocket_flaw_discoveries: Vec<usize>`
  - `contracted_flaw_discoveries: Vec<(EngineSource, Vec<usize>)>`
- Reactors live in `Stage::power_sources` as
  `PowerSourceKind::Reactor` carrying a cloned `ReactorDesign`
  (Phase 1–2 indirection: the snapshot can be traced back to its
  `ReactorProject`).
- `FlawConsequence { PerformanceDegradation(frac), EngineLoss,
  StageLoss }` is reused as-is for reactors (D3). Reactor-flavored
  descriptions already come from `flaw::generate_reactor_flaw_*`.

## Work items

1. **Roll reactor flaws at ignition.** For each firing stage's
   installed reactor(s), roll each flaw's `activation_chance` (mirror
   the engine loop; consider scaling by reactor count if a stage can
   hold several).
2. **Apply the consequence to the stage.** Decide the reactor mapping:
   - `PerformanceDegradation(f)` → reduce the reactor's `steady_w` (and
     therefore available power) by `f`.
   - `EngineLoss` → "reactor shutdown": the reactor produces 0 W.
   - `StageLoss` → stage loss (same as today).
3. **Power-dependency cascade (the real design question).** A reactor
   shutdown must propagate to whatever draws its power — most notably an
   electric-propulsion engine on the same stage (`power_draw_w`). Need a
   principled "is stage power demand still met?" check: if a firing
   engine's draw exceeds remaining power, does it lose thrust
   proportionally, or fail outright? This is the crux and needs its own
   USER discussion.
4. **Discovery writeback.** Add a
   `power_source_flaw_discoveries: Vec<(ReactorId, Vec<usize>)>` (or a
   `PowerSourceRef`) to the launch result and have `game_state` mark
   those flaws `discovered` on the owning `ReactorProject`, parallel to
   the engine path.
5. **Flaw activation records + UI.** Surface reactor flaw activations in
   the launch report alongside engine ones.

## Open questions for when we start
- Can a stage carry more than one reactor? If so, how does activation
  scale (per-unit like engines, or per-design)?
- Cascade semantics (item 3) — proportional thrust loss vs hard cutoff
  when power is short.
- Do reactor `PerDay` (endurance) flaws make sense here even though
  Phase 3 generates only `PerFlight`? (Reactors run continuously in
  flight — endurance failures are arguably *more* natural for reactors
  than engines.)
