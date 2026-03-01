# Plan: Remove `Complete` State from Engine and Rocket Design Projects

## Motivation

Currently, engine and rocket design projects have a `Complete` state that gates manufacturing and rocket creation. The user should be able to build engines/rockets at any revision — a design that's been through one round of testing and revision should be buildable, even if more flaws remain undiscovered. This mirrors real aerospace: you can fly a design with known risk.

Removing `Complete` means the player decides when to build, accepting the current flaw profile. The design stays in the Testing ↔ Revising loop indefinitely, and the player can order builds at any time while in Testing status.

---

## Current Workflow

```
InDesign → Testing ↔ Revising → Complete
                                    ↓
                              Manufacturing
```

## Proposed Workflow

```
InDesign → Testing ↔ Revising
              ↓ (player orders build at any time while Testing)
         Manufacturing
```

- `Complete` is removed from both `EngineDesignStatus` and `RocketDesignStatus`
- Builds are allowed whenever status is `Testing` (not during `InDesign` or `Revising`)
- The `mark_complete()` method is removed from both project types
- `start_testing()` (which allowed `Complete → Testing`) is removed since you never leave Testing

---

## Changes by File

### 1. `engine_project.rs`

- **Remove `Complete` variant** from `EngineDesignStatus` enum (line 184)
- **Remove `mark_complete()` method** (lines 350-355)
- **Remove `start_testing()` method** (lines 319-328) — no longer needed since we never leave Testing
- **`apply_daily_work()`**: Remove the `EngineDesignStatus::Complete => {}` arm (line 313)
- **`testing_level()`**: Remove the `Complete => f64::INFINITY` arm (line 373). When in Testing, the accumulated `work_completed` already captures testing progress. Consider adding a "Thoroughly Tested" upper tier instead of the infinite sentinel.
- **Update tests**: Remove `test_mark_complete` test (line 596-603). Update `test_testing_level` (line 607-622) — currently it tests `Complete` level; replace with testing via accumulated work.

### 2. `rocket_project.rs`

- **Remove `Complete` variant** from `RocketDesignStatus` enum (line 22)
- **Remove `mark_complete()` method** (lines 156-163)
- **Remove `start_testing()` method** (lines 126-133)
- **`apply_daily_work()`**: Remove the `RocketDesignStatus::Complete => {}` arm (line 119)
- **`testing_level()`**: Remove the `Complete => f64::INFINITY` arm (line 174). Same fix as engine.

### 3. `third_party.rs`

- Third-party engines are currently initialized with `EngineDesignStatus::Complete` (lines 57, 99, 141)
- **Change to `EngineDesignStatus::Testing { work_completed: f64::MAX }`** — this makes them show as "Thoroughly Tested" and keeps them permanently in a buildable state
- Since third-party engines have `is_third_party: true` and can't have teams assigned, they'll just sit in Testing forever, which is correct

### 4. `game_state.rs`

- **`order_rocket_build()`** (line 240): Change guard from `RocketDesignStatus::Complete` to `RocketDesignStatus::Testing { .. }`
- The same change should apply for engine builds — verify that engine ordering also checks status and update similarly

USER: We also need to make sure engine builds check status. Currently `order_rocket_build` creates engine orders referencing engine project IDs — does it check that the engine project is in a buildable state?

CLAUDE: Looking at the code, `order_rocket_build()` finds the engine project by matching `ep.design.id == stage.engine.id` but does NOT check the engine project's status. This is actually fine since the rocket designer UI only shows engines from projects that are in Testing (formerly Complete). But we should add a status check here for safety:

```rust
.find(|ep| ep.design.id == stage.engine.id
    && matches!(ep.status, EngineDesignStatus::Testing { .. }))
```

### 5. `ui/mod.rs`

- **Remove 'C' key handler** for engines tab (around line 295-300) — the "mark complete" action goes away
- **Remove 'C' key handler** for rockets tab (around line 345-350) — same
- **Engine selection filter** (line 537): Change from `EngineDesignStatus::Complete` to `EngineDesignStatus::Testing { .. }` — engines in Testing can now be used in rocket designs
- **Build order guard** (around line 359): Update status message from "Must be Complete" to "Must be in Testing to order build"

### 6. `ui/draw.rs`

- **Engine status display** (lines 226-227): Remove `Complete` rendering. Testing will show its own status string.
- **Rocket status display** (lines 351-352): Same removal.
- **"Unknown flaws" warning** (line 300, 422): Currently shown for `Testing | Complete`. Just show for `Testing` (which is now the only state where this matters).
- **Controls help text** (line 312, 443): Remove `[C] Complete` from both engines and rockets tabs.
- **Rocket designer engine list** (line 785): Change filter from `Complete` to `Testing { .. }`.
- **"No completed engines" message** (line 815): Change to "No engines ready! Design and test an engine first."

---

## Summary of Behavioral Changes

| Before | After |
|--------|-------|
| Must mark Complete before building | Can build any time while in Testing |
| Complete is a terminal state | Testing is the long-term steady state |
| Complete shows "Complete" in UI | Testing shows testing level (Untested → Thoroughly Tested) |
| Third-party engines are Complete | Third-party engines are Testing with max work |
| `start_testing()` transitions Complete → Testing | Removed — never leave Testing |
| `mark_complete()` transitions Testing → Complete | Removed entirely |

## Test Updates

- Remove `test_mark_complete` tests from both engine_project and rocket_project
- Update any test that constructs projects with `Complete` status
- Update `test_testing_level` to not use Complete
- Manufacturing tests that check for Complete guard should check for Testing instead
- `order_rocket_build` tests should verify builds work from Testing status

## Risk

Low — this simplifies the state machine. The only subtle point is ensuring third-party engines (initialized as Testing with high work_completed) behave correctly with the testing cycle logic. Since `work_completed` for Testing just accumulates and testing cycles fire when `work_completed >= TESTING_CYCLE_WORK`, using `f64::MAX` could cause overflow in subtraction.

**Safer approach for third-party engines**: Use a large but finite value like `1_000_000.0` for `work_completed`, which represents many testing cycles without overflow risk.
