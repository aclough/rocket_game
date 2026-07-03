# Reactor Research — Phase 3b Plan (deferred flight wiring)

**Make installed-reactor flaws activate during flight.**

Split out of Phase 3 (decision D1). Phase 3 gives reactors the full
research pipeline — flaws generated, discovered in testing, revised
away, tech deficiencies rolled and fixed — but those flaws carry a
consequence for *display only*. This phase makes them actually bite in
the flight sim, building the first power-source flight-failure path.

> Status: **stub** — flesh out when Phase 3 lands. Captured here (per
> USER) so it's a real planned phase, not a loose TODO.

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
