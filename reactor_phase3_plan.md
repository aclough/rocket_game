# Reactor Research — Phase 3 Plan

> **Status: implemented (pending commit).** All four slices landed with
> A/A/A decisions + D4 in-scope + D5 reactor-specific event. `cargo test`
> green (373 tests, verified stable across 16 full-suite runs). Deferred
> flight wiring captured in `reactor_phase3b_plan.md`.

**Flaws + tech deficiencies on reactors.**

Phase 3 finishes the reactor research pipeline so a reactor project
behaves like an engine project end-to-end: it discovers flaws in
testing, can be revised to remove them, rolls `TECH_FISSION_REACTOR`
tech deficiencies on design completion, and can attempt to fix those
deficiencies during revision.

This is the last coding phase of the reactor plan (the deferred items —
reactor-specific flaw *kinds* and fissile-fuel depletion — stay
deferred).

---

## Current state (what Phases 1–2b left stubbed)

- `reactor_project.rs`
  - `apply_daily_work` — the `Testing` branch accrues work but **never
    discovers flaws**; the `Revising` branch is an empty stub; the
    `next_flaw_id` argument is unused.
  - No `start_revision()` method exists.
  - `flaws`, `tech_deficiency_ids`, `technology_id` fields exist but are
    always empty / never consumed.
- `game_state.rs`
  - Reactor daily-work loop (~L1215) maps events but
    `ReactorWorkEvent::TechDeficiencyAttempted { .. } => continue` — the
    attempt is dropped.
  - No reactor equivalent of the engine "process tech-deficiency
    attempts" pass (L1253) or "apply tech deficiencies to newly
    completed designs" pass (L1305).
- `technology.rs`
  - `TechDeficiencyKind` has only `IspPenalty / MassPenalty /
    ThrustPenalty / ComplexityPenalty`.
  - `TECH_FISSION_REACTOR` (difficulty 2) already seed-generates
    deficiencies, but they're engine-shaped and applied to nothing.
- `ui/mod.rs`
  - Reactors pane (`handle_reactors_key`) has `+ / - / e`, but **no `r`
    (revise)** and no `o` (order build) — parity gap with the engine
    pane.
- `flight.rs` / `launch.rs`
  - Only engine, contracted-engine, and rocket flaws roll during flight.
    **Power sources have no flight-failure path at all.**
- `event.rs`
  - Reactor events already exist: `ReactorDesignStarted`,
    `ReactorDesignComplete`, `ReactorFlawDiscovered`,
    `ReactorRevisionComplete`. No tech-deficiency-specific reactor event
    yet (engines reuse `TechDeficienciesFound` / `FlawDiscovered`).

---

## Open decisions (need USER sign-off before I code)

### D1 — Do reactor flaws activate during flight in Phase 3?

Reactors sit in `Stage::power_sources`. Nothing in `launch.rs` /
`flight.rs` rolls power-source flaws today, so wiring reactor flaws into
flight means building the *first* power-source flight-failure path:
roll installed-reactor flaws at ignition, apply the consequence to the
stage (power loss / reactor loss / stage loss), and write discoveries
back to the owning reactor project (parallel to
`engine_flaw_discoveries`).

- **Option A (recommended): defer flight wiring.** Phase 3 = the
  research machinery only. Reactor flaws are generated, discovered in
  testing, and revised away; they carry a consequence for display but
  don't yet activate in flight. This mirrors how engines were built
  incrementally, keeps Phase 3 self-contained, and avoids threading a
  brand-new failure path through the flight sim in the same change.
- **Option B: wire flight now.** Bigger surface — new
  `power_source_flaw_discoveries` writeback, a `apply_consequence`
  branch for reactors, and balance work on how a mid-flight reactor
  loss cascades (does losing the reactor kill an ion drive that depends
  on it?).

**CLAUDE recommendation: A.** Ship the pipeline; make power-source
flight failures a clean follow-up TODO.

**USER:** A, but make implementing the wiring a phase 3b instead of adding it to the TODO.

---

### D2 — What penalty kinds does `TECH_FISSION_REACTOR` generate?

`TechDeficiencyKind` needs a power-shaped variant. Add:

```rust
/// Steady electrical output reduced by this fraction (e.g. 0.10 = -10%).
PowerPenalty(f64),
```

Then `generate_deficiencies` must branch by tech "domain" so the reactor
tech draws from reactor-relevant kinds instead of Isp/Thrust.

- **Option A (recommended): Power + Mass + Complexity.** Reactor tech
  rolls from `{PowerPenalty, MassPenalty, ComplexityPenalty}`. Gives the
  same variety engines get (a stat hit, a mass hit, a
  more-flaws/harder-testing hit) with Isp/Thrust reinterpreted as power.
- **Option B: Power only.** Simpler, less texture.

Either way this shifts the deterministic seed stream for
`TECH_FISSION_REACTOR` — harmless, since those deficiencies were applied
to nothing before Phase 3.

Implementation note: I'll pass a small `domain: TechDomain { Engine,
Reactor }` (or reuse the tech id) into `generate_deficiencies` so the
kind table is data-driven rather than hardcoded per id.

**CLAUDE recommendation: A.**

**USER:**  Yes, A is much better here.

---

### D3 — Flaw consequence model for reactors

Engines use `FlawConsequence { PerformanceDegradation(frac), EngineLoss,
StageLoss }`. The overall plan said "reuse `Flaw`, add a power penalty."

- **Option A (recommended): reuse `FlawConsequence` unchanged,
  reactor-flavored descriptions.** `PerformanceDegradation` reads as
  "% power loss", `EngineLoss` as "reactor shutdown", `StageLoss` as
  "stage loss". A new `flaw.rs` generator
  (`generate_reactor_flaws` + `generate_reactor_flaw_description`)
  supplies reactor-appropriate text. No enum churn; every existing
  match site (`launch.rs`, `flight.rs`, UI) keeps compiling. Consequence
  *semantics* only matter once D1 wires flight — until then it's display
  text.
- **Option B: add a reactor-specific consequence variant.** Cleaner
  naming but touches every `FlawConsequence` match site for a
  distinction that doesn't pay off until flight wiring exists.

**CLAUDE recommendation: A.** (Reactor-specific flaw *kinds* are already
an explicitly deferred item; this stays consistent with that.)

**USER:**  That sounds good and I'm not actually sure we need reactor specific flaws.

---

## Implementation plan (assuming A/A/A above)

### 1. `technology.rs`
- Add `TechDeficiencyKind::PowerPenalty(f64)` + its `Display` arm
  (`-{:.0}% power`).
- Introduce `TechDomain { Engine, Reactor }` (or a bool) and thread it
  into `generate_deficiencies`; select the kind table by domain. Reactor
  domain → `{PowerPenalty, MassPenalty, ComplexityPenalty}`.
- `generate_technologies`: mark `TECH_FISSION_REACTOR` as reactor-domain.
- Update tests: existing engine-tech tests unchanged; add a test that
  the reactor tech only produces reactor-valid kinds.

### 2. `flaw.rs`
- Add `generate_reactor_flaws(effective_complexity, rng, next_flaw_id)`
  and `generate_reactor_flaw_description(consequence, rng)` with
  reactor-appropriate strings (core temp excursions, coolant loop,
  control-drum, radiator, shielding, etc.). All `PerFlight` trigger
  (reactors, like engines, don't get `PerDay` endurance flaws in v1).
- No struct change to `Flaw` / `FlawConsequence`.

### 3. `reactor_project.rs`
- Derive a reactor `effective_complexity` (base complexity + any
  `ComplexityPenalty` from applied deficiencies), mirroring the engine
  path.
- `apply_daily_work`:
  - `InDesign → Testing`: on completion, generate flaws via
    `generate_reactor_flaws`; report real `flaw_count`.
  - `Testing`: consume `TESTING_CYCLE_WORK` cycles, roll discoveries via
    `flaw::roll_discoveries_with_rng`, emit `FlawDiscovered` +
    `TestingCycleComplete`. (No improvement discovery for reactors in
    v1 — engines have improvements; reactors defer that unless USER
    wants parity — see D4 below.)
  - `Revising`: consume `FLAW_REVISION_WORK` per item — remove
    discovered flaws, then attempt tech deficiencies (emit
    `TechDeficiencyAttempted`), then drop back to `Testing` with
    leftover work. Same index-shuffle logic as engines.
- Add `start_revision()` (Testing-only; builds discovered-flaw indices +
  `tech_deficiency_ids`; bumps `revision`; returns false if nothing to
  do).
- Tests: design-complete generates flaws at high complexity; testing
  discovers a forced high-probability flaw; revision removes discovered
  flaws and returns to Testing.

### 4. `game_state.rs`
- Reactor daily-work loop: collect `(reactor_index, deficiency_id)` for
  `TechDeficiencyAttempted` instead of dropping it.
- Add a reactor "process tech-deficiency attempts" pass mirroring the
  engine one (L1253): `attempt_solve`; on success remove the id and undo
  the `steady_w` / `mass_kg` / `complexity` penalty; on failure emit a
  reactor failure message.
- Add a reactor "apply tech deficiencies to newly completed designs"
  pass mirroring L1305: on `DesignComplete`, look up
  `TECH_FISSION_REACTOR`, apply unsolved deficiency penalties to the
  design, store `tech_deficiency_ids`, emit `TechDeficienciesFound`
  (or a new reactor-specific event — see D5).
- Penalty application helper shared between engine + reactor where the
  kinds overlap (`MassPenalty`, `ComplexityPenalty`); `PowerPenalty`
  handled only on the reactor side; `Isp/ThrustPenalty` only engine.

### 5. `ui/mod.rs` + `ui/draw.rs`
- `handle_reactors_key`: add `r` (start_revision, Testing-only) and
  surface discovered-flaw / deficiency counts, mirroring the engine
  pane. Update the pane's help/legend line.
- `draw.rs`: show reactor flaw counts, testing level, and deficiency
  list in the Reactors pane detail, mirroring the engine pane.

### 6. Save/load
- No new fields (Phase 1 already added `flaws`, `tech_deficiency_ids`,
  `technology_id`, `cumulative_testing_work`). The orphan-`Proposed`
  sweep already exists. Verify a round-trip test still passes.

### 7. TODO.txt
- On approval/commit, remove `[] Make reactors researchable`.

---

## Secondary questions (lower stakes)

### D4 — Reactor improvements during testing?
Engines discover `Improvement`s (Isp/Mass/Thrust bumps) during testing
and actualize them via revision. Reactors could get Power/Mass
improvements the same way.
- **CLAUDE recommendation: defer** — keep Phase 3 focused on
  flaws+deficiencies; add reactor improvements as a small follow-up if
  they feel wanted. (Cheap to add later; the revision loop is already
  shaped for it.)

**USER:**  For symmetry reasons I think it would be best to do this now.

**CLAUDE (resolution):** In scope. Reactors get a reactor-specific
`ReactorImprovementKind { Power(frac), Mass(frac) }` (Isp/Thrust don't
apply), discovered in testing at the same ~8%/cycle rate and actualized
in revision. **Follow-up note:** rename the engine's generic
`ImprovementKind` (in `engine_project.rs`) to an engine-specific name
(e.g. `EngineImprovementKind`) so the two domains read symmetrically.
Tracked as a small cleanup — done alongside or just after Phase 3.

### D5 — Reactor-specific tech-deficiency event, or reuse engine events?
Engines emit `TechDeficienciesFound { engine_name, tech_name,
deficiencies }`. For reactors I can either add
`ReactorTechDeficienciesFound { reactor_name, ... }` or reuse the engine
event with the reactor name in the `engine_name` field.
- **CLAUDE recommendation: add a reactor-specific event** for clean log
  text ("Reactor deficiencies…"), consistent with the existing
  reactor-specific event family in `event.rs`.

**USER:**  Yes, reactor specific would be best.

---

## Recommended commit slicing
1. `technology.rs` + `flaw.rs` primitives (`PowerPenalty`, reactor flaw
   generation) — pure data, no behavior change, fully unit-tested.
2. `reactor_project.rs` pipeline (flaw discovery + revision +
   `start_revision`) — unit-tested in isolation.
3. `game_state.rs` wiring (deficiency roll + attempt passes).
4. UI (`r` key + pane detail).

Each slice compiles + tests green on its own; I'll check with you before
each commit per CLAUDE.md.
