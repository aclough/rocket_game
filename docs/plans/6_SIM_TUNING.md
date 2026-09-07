# M1 Plan — Simulation & Tuning Infrastructure

Implements roadmap M1: BalanceConfig, headless sim binary, CompanyPolicy,
metric-band tests, seed-fairness floor. Tasks in roadmap order.

## Decisions already made (from discussion)

- **BalanceConfig lives as a field on `GameState`** (`#[serde(default)]`,
  so old saves load with defaults and saves remember their balance).
  Threaded as an argument where needed; no global statics (parallel
  tests would race on a global).
- **Scope of the config** (from the constant inventory): costs &
  salaries, work/time formulas, market table + deadline range, flaw
  distribution parameters, reputation deltas. **Excluded:** complexity
  tables and tech/deficiency generation (already complexity-driven and
  seed-entangled), physics constants (G0, solar flux, structural
  limits — revisit aero shell fraction later for realism/balance),
  economy Markov table (miserable sweep dimension; revisit if wanted),
  UI/mechanical constants (scale steps, log sizes, IDs).
- **Work order kept as the roadmap lists it** — BalanceConfig first,
  because its existence shapes how the sweep harness is constructed.
- Hand-rolled arg parsing for the sim binary (no clap dependency yet).
- Output: CSV of monthly metrics + end-of-run summary line.
- ratatui/crossterm stay as unconditional deps (no feature-gating).

---

## Task 1 — BalanceConfig  (~2-4 days)

### 1a. The struct

New module `src/balance_config.rs`. Nested sub-structs so the TOML
reads in sections:

```toml
[costs]        # starting money, salaries, hiring, floor space, resource $/kg, reactor ref cost
[work]         # design/build base days, exponents, learning curve, revision/testing work
[markets]      # base market table (volumes, rate_per_kg, payload ranges, weights), deadline range
[flaws]        # count stddev, consequence split, degradation range, per-day fractions, improvement chances
[reputation]   # gain/loss deltas, decay factors, MEU/HEU gates
```

`impl Default` = exactly today's values, so behavior is unchanged and
existing test assertions stay valid.

**Layered loading** (add `toml` crate only): defaults serialize to a
`toml::Value` tree; each config file parses to a `Value` and is
deep-merged on top (a ~12-line recursive merge: recurse where both
sides are tables, otherwise replace); deserialize once at the end.
So every file is a **partial override** and files stack:

- Bottom layer stays `impl Default` in code (single source of truth;
  tests need no file I/O; the binary can't lose its config).
- `--balance FILE` on the sim binary is **repeatable** — e.g.
  `--balance base.toml --balance experiment.toml`, merged in order.
- `--dump-balance` writes the full effective config as TOML — the
  generated "default config file" to read/copy/edit, always in sync
  with code. (Considered figment/config-rs, which do this merging as
  libraries; hand-rolled merge fits the lean-deps approach.)

### 1b. Migration mechanics

- `GameState.balance: BalanceConfig` with `#[serde(default)]`.
- `balance.rs` work formulas become methods on the config
  (`cfg.design_work_required(complexity)`); the complexity functions
  stay in `balance.rs` as free functions (out of scope).
- Scattered consts (team.rs salaries, manufacturing.rs floor space,
  resources.rs $/kg table, flaw.rs work consts, reputation.rs deltas)
  move into the config; their consumers take `&BalanceConfig` (or the
  relevant sub-struct) as an argument. Old const names deleted so the
  compiler finds every consumer.
- `initial_markets()` moves into `Default for MarketConfig`; contract
  generation reads the table from the config. This is deliberately the
  substrate M2's seed-perturbation layer will multiply on top of.
- Starting money moves from the `bin/main.rs` literal into the config.
- Constraint: threading the config must not change RNG draw order
  (determinism).

### 1c. Who loads TOML

The loader is a lib function. The sim binary gets a `--balance FILE`
flag. Proposal: the TUI binary does NOT load override files for now —
players get defaults; sweeps are a dev tool. (Trivial to add a
`--balance` flag to the TUI later for playtesting a candidate tuning.)

USER:  I totally agree with that decision.  The TUI gets the default until/unless we decidee to add difficulty levels.

## Task 2 — Headless sim binary  (~1-2 days)

`src/bin/simulate.rs`, second `[[bin]]` entry.

```
cargo run --bin simulate -- --seed 42 --years 5 --policy basic
       [--seeds 1..200] [--balance base.toml --balance sweep.toml]
       [--dump-balance] [--csv out.csv] [--summary-only]
```

- Loop: `GameState::new(...)` → each day, `policy.act(&mut game)` then
  `advance_day()` (no UI, no pauses; auto-pause events just get logged).
- Monthly CSV row: date, money, reputation, contracts held/completed/
  failed/expired, launches attempted/succeeded, active projects, teams,
  rockets in inventory.
- Summary line per seed: final money, peak debt, bankrupt?, total
  launches, success rate, first-profitable-year.
- `--seeds A..B` runs a range and emits one summary row per seed
  (the sweep harness's unit of work).

## Task 3 — CompanyPolicy  (~3-5 days, includes API lift)

### 3a. Lift UI-resident logic into GameState methods (prerequisite)

The four spots where game logic lives in `ui/mod.rs` key handlers:

1. **Launch manifest assembly** (`submit_manifest_launch`) — the logic
   building `Vec<Payload>` from selected contracts/spacecraft moves to
   a `GameState` method (e.g. `build_launch_payloads(...)`); UI and bot
   both call it.
2. **Floor-space purchase** — money debit happens in the key handler;
   becomes `Company::buy_floor_space(units)`.
3. **`start_revision`** called inline on projects — wrapped in a
   Company/GameState method.
4. **Auto-build target toggling** — direct map mutation; becomes a
   method.

This is also the GUI-readiness discipline: after 3a, the UI performs
no game mutations except through methods.

### 3b. The trait and first policy

```rust
pub trait CompanyPolicy {
    fn act(&mut self, game: &mut GameState);
}
```

Called once per day before `advance_day()`. First policy `basic`
(honest-but-naive): hire a couple of engineering + manufacturing teams,
start two preset engine projects (kerolox gas-generator booster +
hydrolox upper-stage engine), design a fixed-template two-stage rocket
(kerolox first stage, hydrolox upper) when the engines complete, order
builds, accept the best-paying contract it can afford and lift, launch
when rocket + contract are ready, repeat. Skips reactors, electric
propulsion, third-party engines, revisions beyond what blocks it.
(Hydrolox upper is deliberate: it exercises the hydrogen problems
factor and higher-complexity design path in every sim run.)

Constraints: decisions must be deterministic (index-ordered choices,
no HashMap-iteration dependence, no wall clock). New module
`src/policy.rs` (sim-side, not UI).

Later archetypes (conservative, aggressive-R&D) are follow-ons, not
part of M1 unless trivial.

## Task 4 — Metric bands + determinism test  (~1-2 days)

New `tests/` integration directory.

- **Determinism smoke test:** same seed + basic policy twice →
  byte-identical monthly metrics. Guards against HashMap-iteration
  and wall-clock leaks. Cheap; runs in normal `cargo test`.
- **Metric bands:** run basic policy across N seeds × Y years and
  assert e.g. "no panics", "profitable by year 3 in >X% of seeds",
  "bankruptcy rate < Y%". Process: first run the harness to *measure*
  the current baseline, then set bands around observed reality —
  we don't know today what the numbers are. Bands are regression
  protection, not aspirations, until M4 tunes them.
- Cost control: a small version (~20 seeds) in normal `cargo test`;
  the full 200-seed version `#[ignore]`d, run explicitly. Adjust split
  once we know actual per-seed runtime.

## Task 5 — Seed-fairness floor  (~half a day)

Across N seeds, assert year-1 viability: enough contracts in the
achievable payload/destination class, total available contract value
above a floor. Trivial while markets are static; the point is having
the test **before** M2 adds market variance, so M2 can't accidentally
starve a seed.

---

## Order & verification

1 → 2 → 3a → 3b → 4 → 5. After each task: full test suite, plus a
manual TUI session after Task 1 (the config threading touches
everything) and after 3a (UI behavior must be unchanged).

## Open questions

1. Task 1c above — OK that the TUI ignores balance TOML for now?
USER: Yes, certainly
2. The `basic` policy's rocket design: simplest is a fixed template
   (two-stage kerolox sized to the contract class) rather than a
   real optimizer. A dumb-but-deterministic designer is fine for
   metric bands; a smarter one improves sweep signal later. Start
   dumb?
   USER:  Dumb is good but lets do a hydrolox upper stage?
3. Anything you want in the monthly CSV beyond the list in Task 2?
   USER:  No, that looks good I think.
