# R&D Pipeline Unification Plan

> Covers items **B5 → B1 → B4 → B3 → B2** from `17_REFACTOR.md`, with the
> minimal slice of **B6** the generic core needs. Written against commit
> `e79811a`. Nothing here is implemented. Add `USER:` comments inline;
> I'll reply as `CLAUDE:`.
>
> Your steer from the review: unify conceptually, keep per-domain
> specialisation in the *prevalence and nature of flaws*, and accept a
> save-format change for the event collapse.

---

## 0. Goal and non-goals

**Goal.** One project state machine, one work-event type, one project
`GameEvent`, one company-level dispatch layer, one UI pane handler, with
the three domains (engine, rocket, reactor) differing only through a
small trait. Adding a fourth domain should mean implementing the trait
and adding one pane.

**Behaviour must not change.** Every step is a pure refactor at the
simulation level: identical RNG draw order, identical numbers. The
`simulate` CSV over a fixed seed range is the oracle (§7).

**Out of scope here** (tracked in `17_REFACTOR.md`): the flaw
*description pool* tables (rest of B6), `advance_day` splitting (C1),
the `company.rs` / `ui` file splits (E1, E2, E4), rocket improvements as
a gameplay feature.

---

## 1. What exists today (facts the design rests on)

| | Engine | Rocket | Reactor |
|---|---|---|---|
| Status enum | `EngineDesignStatus` 4 variants | `RocketDesignStatus` 3 (no `Proposed`) | `ReactorDesignStatus` 4 |
| `Revising` fields | `remaining_flaw_indices`, `remaining_improvement_indices`, `remaining_tech_deficiency_ids`, `work_completed` | `remaining_indices`, `work_completed` | same as engine |
| Extra project fields | `preset`, `scale` | none | none |
| Improvements | `EngineImprovement { description, kind: Isp/Mass/Thrust, actualized }` | none | `ReactorImprovement { description, kind: Power/Mass, actualized }` |
| Tech deficiencies | yes (`technology_id`, `tech_deficiency_ids`) | no | yes |
| Flaw generator | `generate_flaws_for_cycle(eff, …, Some(cycle))`, all `PerFlight` | `generate_rocket_flaws`, 30% `PerDay` | `generate_reactor_flaws`, 30% `PerDay` |
| Complexity for flaw count | `effective_complexity(cycle, propellants)` (recomputed at design-complete) | `self.complexity` | `self.complexity` |
| Work-event enum | `WorkEvent` (7 variants) | `RocketWorkEvent` (4) | `ReactorWorkEvent` (7) |
| `GameEvent` variants | 7 | 5 (incl. `RocketDesignModified`) | 7 |

Shared verbatim across all three: `promote_to_in_design`,
`start_revision`, `discovered_flaw_count`, `testing_level`, the
`apply_edit` status clamp, and the whole `apply_daily_work` skeleton
(~105 lines each). The three `apply_daily_work` bodies differ in exactly
three places: which flaw generator runs at design-complete, which
improvement generator (and which `FlawsConfig` chance field) runs per
testing cycle, and how an improvement kind mutates the design.

Company-side: `ProjectKind { Engine(usize), Rocket(usize), Reactor(usize) }`
is private (`company.rs:40`) and index-based; `RetireTarget` is public
and id-based (`company.rs:97`). Fifteen methods exist in triplicate.
`ResearchTick` carries raw indices consumed after the tick.

Save corpus (`tests/saves/m1..m4.json`): **no** project is mid-`Revising`
in any era, but every era's event log contains old project events
(35 `FlawDiscovered`, 34 `RevisionComplete`, 13 `RocketFlawDiscovered`,
14 `ImprovementDiscovered`, …). The save loader has a `migrate()` hook
that is empty today (`SAVE_VERSION = 1`).

---

## 2. Target design

### 2.1 Shared status (B5 folded in)

```rust
// src/project.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DesignStatus {
    Proposed { work_required: f64 },
    InDesign { work_completed: f64, work_required: f64 },
    Testing  { work_completed: f64 },
    Revising {
        /// Flaws queued for removal, by id — stable across the
        /// `flaws.remove()` that each completed revision performs.
        remaining_flaw_ids: Vec<FlawId>,
        /// Improvements queued for actualisation, by id (Q6).
        #[serde(default)]
        remaining_improvement_ids: Vec<ImprovementId>,
        #[serde(default)]
        remaining_tech_deficiency_ids: Vec<TechDeficiencyId>,
        work_completed: f64,
    },
}
```

- `pub type EngineDesignStatus = DesignStatus;` etc. so the ~200
  existing `EngineDesignStatus::Testing { .. }` pattern sites compile
  unchanged.
- Rocket gains a `Proposed` variant it never constructs today. The pane
  filter already skips `Proposed`; nothing else needs to know. (Q2)
- The three index-shift fixup loops disappear; flaw removal becomes
  `self.flaws.retain(|f| f.id != fid)`.

### 2.2 The generic project

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "D: Designable + Serialize + DeserializeOwned")]
pub struct DesignProject<D: Designable> {
    pub project_id: D::Id,
    pub design: D,
    /// Domain-specific design parameters kept alongside the design
    /// (engine: preset + scale). Flattened so the JSON layout of
    /// existing saves is unchanged.
    #[serde(flatten)]
    pub spec: D::Spec,
    pub status: DesignStatus,
    pub flaws: Vec<Flaw>,
    pub revision: u32,
    pub teams_assigned: u32,
    pub complexity: u32,
    #[serde(default)] pub nre_cost: f64,
    #[serde(default)] pub improvements: Vec<Improvement<D::ImprovementKind>>,
    #[serde(default)] pub cumulative_testing_work: f64,
    #[serde(default)] pub tech_deficiency_ids: Vec<TechDeficiencyId>,
    #[serde(default)] pub technology_id: Option<TechnologyId>,
    #[serde(default = "crate::flaw::auto_revise_default")] pub auto_revise: bool,
    #[serde(default)] pub retired: bool,
}

pub type EngineProject  = DesignProject<EngineDesign>;
pub type RocketProject  = DesignProject<RocketDesign>;
pub type ReactorProject = DesignProject<ReactorDesign>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImprovementId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Improvement<K> { pub id: ImprovementId, pub description: String, pub kind: K, pub actualized: bool }
```

Each project carries its own `next_improvement_id: u64` (serde default
0). An improvement never leaves the project that discovered it, so the
id only has to be unique within the project, and a per-project counter
avoids threading a second `&mut u64` through every `apply_daily_work`
call and its ~17 test callers. (Deviation from the first draft, which
put the counter on `Company`.)

Serde layout check against today's JSON: every field name is the same;
`preset`/`scale` stay top-level via `flatten`; rocket saves have no
`improvements`/`tech_deficiency_ids`/`technology_id` keys and get the
defaults, exactly as they do now. Rocket's `Spec` and `ImprovementKind`
are empty types (`struct NoSpec {}` flattens to nothing; `enum Never {}`
is fine inside an always-empty `Vec`).

### 2.3 The trait

```rust
pub trait Designable: Clone {
    type Id: Copy + Eq + Hash + Serialize + DeserializeOwned;
    type Spec: Clone + Serialize + DeserializeOwned;
    type ImprovementKind: Clone + fmt::Display + Serialize + DeserializeOwned;

    const KIND: ProjectKind;                 // Engine | Rocket | Reactor
    fn name(&self) -> &str;

    // ── flaws: where the per-domain character lives ──────────────────
    /// Which description pools and endurance fraction apply.
    fn flaw_domain(&self) -> FlawDomain;
    /// Complexity the flaw count is drawn from at design-complete.
    /// Engine recomputes effective complexity from cycle + propellants;
    /// rocket and reactor use the project's stored complexity.
    fn flaw_complexity(&self, spec: &Self::Spec, project_complexity: u32) -> u32;

    // ── improvements ─────────────────────────────────────────────────
    /// Per-cycle discovery chance before decay. `None` = this domain
    /// has no improvements (rocket).
    fn improvement_chance(cfg: &FlawsConfig) -> Option<f64>;
    fn roll_improvement(&self, rng: &mut StdRng) -> Improvement<Self::ImprovementKind>;
    fn apply_improvement(&mut self, kind: &Self::ImprovementKind);

    // ── tech deficiencies (B4) ───────────────────────────────────────
    /// Apply or revert one deficiency's stat effect. Complexity
    /// penalties are handled by the generic code (they hit the
    /// project, not the design).
    fn apply_deficiency(&mut self, kind: &TechDeficiencyKind, dir: Direction);
}

pub enum Direction { Apply, Revert }

pub enum FlawDomain { Engine(EngineCycle), Rocket, Reactor }
```

What each impl looks like:

- **`EngineDesign`**: `Spec = EngineSpec { preset, scale }`,
  `ImprovementKind = EngineImprovementKind`. `flaw_domain` →
  `Engine(self.cycle)`; `flaw_complexity` →
  `balance::effective_complexity(self.cycle, &spec.preset.propellants())`;
  `improvement_chance` → `Some(cfg.improvement_discovery_chance)`;
  `roll_improvement` → today's `generate_improvement(rng, self.cycle)`;
  `apply_improvement` → the Isp/Mass/Thrust match; `apply_deficiency` →
  Isp/Mass/Thrust `*=`/`/=` (PowerPenalty ignored).
- **`ReactorDesign`**: `Spec = NoSpec`, `ImprovementKind =
  ReactorImprovementKind`. `Reactor`; `project_complexity`;
  `Some(cfg.reactor_improvement_discovery_chance)`;
  `generate_reactor_improvement`; Power/Mass with the
  `mass_kg = reactor_mass_kg + radiator.mass_kg` invariant kept inside
  the impl; `apply_deficiency` → Power/Mass (Isp/Thrust ignored).
- **`RocketDesign`**: `Spec = NoSpec`, `ImprovementKind = Never`.
  `Rocket`; `project_complexity`; `None`; `roll_improvement` is
  `unreachable!()` (never called when chance is `None`); the two apply
  fns are no-ops.

`DesignProject<D>` then has **one** implementation each of
`promote_to_in_design`, `apply_daily_work`, `start_revision`,
`discovered_flaw_count`, `testing_level`, `clamp_work_after_edit`, and
`accrue_nre`. `apply_daily_work` is today's engine body with the three
hook calls substituted; RNG draw order is unchanged (flaw gen at
design-complete; per cycle: discoveries, then the improvement roll if
`improvement_chance` is `Some`).

Kind-specific inherent impls stay where they are, on the alias:
`impl DesignProject<EngineDesign> { fn new, new_proposed, apply_edit,
design_variant, has_nozzle_choice }`, and the reactor and rocket
equivalents. B7's `EngineSpec` falls out of this for free: `new` /
`apply_edit` / `design_variant` all derive the design from `(spec,
baseline, vacuum)` in one helper.

### 2.4 Flaw generation entry point (B6, minimal slice)

```rust
// src/flaw.rs
pub fn generate_flaws(domain: FlawDomain, complexity: u32, rng, next_id: &mut u64, cfg) -> Vec<Flaw>
```

replaces `generate_flaws_for_cycle`, `generate_rocket_flaws`,
`generate_reactor_flaws`, and the wrapper `generate_flaws`. Body: one
count loop; per flaw, the endurance roll happens **only** for `Rocket`
and `Reactor` (engine draws no trigger roll today, so adding one would
change RNG order); then `generate_single_flaw(domain, id, trigger, rng,
cfg)` dispatches to the existing description-pool functions. The pools
themselves stay as they are; turning them into tables is the rest of B6
and can follow independently. `third_party.rs:123` switches to
`generate_flaws(FlawDomain::Engine(cycle), …)`; the rocket-modification
roll at `game_state/mod.rs:359` uses `generate_single_flaw(Rocket, …)`.

### 2.5 Tech deficiencies (B4)

New `src/game_state/tech_ops.rs`:

```rust
impl GameState {
    fn resolve_tech_attempts<D: Designable>(&mut self, projects: fn(&mut Company) -> &mut Vec<DesignProject<D>>,
                                            attempts: Vec<(D::Id, TechDeficiencyId)>, events: &mut Vec<GameEvent>)
    fn apply_new_design_deficiencies<D: Designable>(…, newly_designed: Vec<D::Id>, …)
}
```

Each is today's engine block with `project.design.apply_deficiency(kind,
Revert|Apply)` in place of the inline match, and
`project.complexity += / -= n` for `ComplexityPenalty` handled
generically. Called twice each from `advance_day` (engine, reactor) — or
once via a small `for_each_tech_domain` if that reads better. Removes
~200 lines from `advance.rs`. `ResearchTick` becomes:

```rust
pub struct ResearchTick {
    pub events: Vec<GameEvent>,
    pub newly_designed: Vec<ProjectRef>,
    pub tech_def_attempts: Vec<(ProjectRef, TechDeficiencyId)>,
}
```

### 2.6 Events (B3)

```rust
// src/project.rs
pub enum WorkEvent {                       // one enum, replaces three
    DesignComplete, TestingCycleComplete,
    FlawDiscovered { description: String },
    ImprovementDiscovered { description: String },
    ImprovementActualized { description: String },
    RevisionComplete,
    TechDeficiencyAttempted { deficiency_id: TechDeficiencyId },
}

// src/event.rs
GameEvent::Project { kind: ProjectKind, name: String, event: ProjectEvent }

pub enum ProjectEvent {
    DesignStarted, DesignComplete, RevisionComplete,
    FlawDiscovered { description: String },
    ImprovementDiscovered { description: String },
    ImprovementActualized { description: String },
    TechDeficienciesFound { tech_name: String, deficiencies: String },
    /// A revision attempt on a deficiency failed. Today this is
    /// reported as a `FlawDiscovered` with a "Failed to resolve…"
    /// description, which is a lie the Events tab repeats.  (Q1)
    TechDeficiencyUnresolved { description: String },
    /// Rocket-only today; harmless on the shared enum.
    DesignModified { new_flaw: bool },
}
```

Replaces 17 `GameEvent` variants. `tick_daily_research` maps
`WorkEvent → ProjectEvent` in one 8-arm match instead of three.
`AutoRevisionStarted` already carries `project_name` and stays as is.

**Display.** Today's strings are inconsistent ("Flaw found in X" for
engines, "Rocket flaw in X", "Reactor flaw in X"; "Design complete: X"
vs "Rocket design complete: X"). Proposal: one template per
`ProjectEvent` with `kind.label()` = "Engine" / "Rocket" / "Reactor":

| event | text |
|---|---|
| DesignStarted | `Started {kind} design: {name}` |
| DesignComplete | `{Kind} design complete: {name}` |
| FlawDiscovered | `{Kind} flaw in {name}: {description}` |
| RevisionComplete | `{Kind} revision complete: {name}` |
| ImprovementDiscovered | `Improvement found for {name}: {description}` |
| ImprovementActualized | `Improvement applied to {name}: {description}` |
| TechDeficienciesFound | `{tech_name} deficiencies on {name}: {deficiencies}` |
| TechDeficiencyUnresolved | `{name}: {description}` |
| DesignModified | as today |

Engine strings change slightly ("Design complete: X" → "Engine design
complete: X"). The test `design_complete_event_does_not_leak_flaw_count`
pins the current strings and gets updated in the same edit. (Q3)

**Importance.** All `Notable`, as today.

### 2.7 Save migration

- `SAVE_VERSION` → 2.
- New step in `load_game`: parse to `serde_json::Value`, read
  `save_version` (default 0), run `migrate_json(&mut value, from)`, then
  `from_value::<GameState>`. Shape changes go here; `migrate()` on the
  typed state stays for anything that needs game logic. Cost: one extra
  parse of a ~0.2 MB document, once per load.
- `v<2` JSON migration does two things:
  1. **Event log.** For each `(date, event)` entry whose single key is
     one of the 17 old names, rewrite to
     `{"Project": {"kind": …, "name": <the *_name field>, "event": {<new variant>: {…}}}}`.
     A 17-row table `(old_name, kind, new_variant, name_field)` drives
     it. Unknown keys are left alone.
  2. **Improvement ids.** Walk every project's `improvements` array
     and stamp `id` sequentially from a fresh counter; write the
     counter's final value to `player_company.next_improvement_id`
     (and each competitor's). Every era from m3 on has improvements,
     so the corpus exercises this.
  3. **Mid-revision projects.** For a `status.Revising` object:
     replace `remaining_indices` / `remaining_flaw_indices: [i, …]`
     with `remaining_flaw_ids: [flaws[i].id, …]`, and
     `remaining_improvement_indices: [i, …]` with
     `remaining_improvement_ids: [improvements[i].id, …]` (after step
     2 has stamped them). No corpus save is mid-revision, so this gets
     a unit test with a hand-built v1 project snippet (fine for a unit
     test; the corpus rule is about not hand-editing the era files).
- Compat corpus: `tests/save_compat.rs` already loads m1–m4 and asserts
  the event log is non-empty and displayable, which covers (1) for
  real data. Before starting, generate **`m5.json`** from `e79811a` the
  same way the others were made, so the pre-change shipping state is in
  the corpus too. (Q4)

### 2.8 Company dispatch (B2)

```rust
// src/company.rs (public)
#[derive(Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectKind { Engine, Rocket, Reactor }

/// Which project, across the three lists, by id. Replaces both the
/// private index-based `ProjectKind(usize)` and `RetireTarget`.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum ProjectRef { Engine(EngineProjectId), Rocket(RocketProjectId), Reactor(ReactorProjectId) }
```

`RetireTarget` becomes a type alias for `ProjectRef` (or is renamed
outright; the `retire` API is the only user). (Q5)

Object-safe view over the shared fields, implemented once for
`DesignProject<D>`:

```rust
pub trait ProjectCore {
    fn kind(&self) -> ProjectKind;
    fn project_ref(&self) -> ProjectRef;
    fn name(&self) -> &str;
    fn status(&self) -> &DesignStatus;
    fn flaws(&self) -> &[Flaw];
    fn revision(&self) -> u32;
    fn teams_assigned(&self) -> u32;
    fn teams_assigned_mut(&mut self) -> &mut u32;
    fn nre_cost_mut(&mut self) -> &mut f64;
    fn auto_revise_mut(&mut self) -> &mut bool;
    fn retired(&self) -> bool;
    fn discovered_flaw_count(&self) -> usize;
    fn pending_improvement_count(&self) -> usize;
    fn tech_deficiency_count(&self) -> usize;
    fn testing_level(&self, cfg: &BalanceConfig) -> &'static str;
    fn is_proposed(&self) -> bool;
    fn promote_to_in_design(&mut self);
    fn start_revision(&mut self) -> Option<RevisionPlan>;
}
pub struct RevisionPlan { pub flaws: usize, pub improvements: usize, pub deficiencies: usize }
```

`Company` gains:

```rust
fn project(&self, r: ProjectRef) -> Option<&dyn ProjectCore>;
fn project_mut(&mut self, r: ProjectRef) -> Option<&mut dyn ProjectCore>;
fn projects(&self) -> impl Iterator<Item = &dyn ProjectCore>;            // all three lists
fn projects_mut(&mut self) -> impl Iterator<Item = &mut dyn ProjectCore>;
fn visible_projects(&self, kind: ProjectKind) -> impl Iterator<Item = &dyn ProjectCore>;

pub fn add_team(&mut self, r: ProjectRef) -> bool;
pub fn remove_team(&mut self, r: ProjectRef) -> bool;
pub fn steal_team_to(&mut self, r: ProjectRef) -> Option<String>;
pub fn start_revision(&mut self, r: ProjectRef) -> Option<RevisionPlan>;
pub fn promote_proposed(&mut self, r: ProjectRef) -> Option<String>;
pub fn delete_proposed(&mut self, r: ProjectRef);
pub fn toggle_auto_revise(&mut self, r: ProjectRef) -> Option<(String, bool)>;
```

and loses the 15 per-kind versions. `unassigned_team_count`,
`auto_assign_idle_engineering_teams`, `busiest_engineering_donor`,
`move_engineering_team`, NRE accrual and the three `visible_*` fns
collapse onto `projects()` / `projects_mut()`. The `(kind, 0, name)`
tuple with the always-zero middle element goes away.

Kind-specific company methods (`start_engine_project`,
`start_proposed_reactor`, `start_rocket_project`, `order_*_build`,
`installable_reactor_projects`, `set_auto_build_target`, `retire`) stay
as they are: they genuinely differ.

Callers to update: `ui/mod.rs` pane handlers (below), `policy.rs:97-110,
262-279` (uses `start_engine_revision(i)`, `add_team_to_project(i)`,
`steal_engineering_team_to_rocket_project(ri)`), `report.rs:115-140`
(three near-identical loops → one over `projects()`), and
`game_state/tests.rs` (28 `engine_projects[` and 40 `rocket_projects[`
sites, most of which read fields and keep compiling; the ones calling
the index-based mutators switch to `ProjectRef`).

### 2.9 UI (the B2 half that lives in `ui/`)

- `handle_project_pane_key(kind, key)` replaces
  `handle_reactors_key` / `handle_engines_key` / `handle_rockets_key`.
  Shared arms (`a`, `+`, `-`, `r`, `x`, `e`-hire) are written once over
  `ProjectRef`; a trailing `match kind` keeps the genuinely different
  keys (`n` new, `o` order build, `b` buy third-party, `M`/`m` rocket
  modify / auto-build, `e`-editor on reactors). The three
  `*_pane_real_index` fns become one `selected_project(kind) ->
  Option<ProjectRef>` over `visible_projects(kind)`.
- Draw: `push_project_row(lines, gauges, &dyn ProjectCore, selected,
  cfg)`, `push_flaws(lines, &[Flaw], kind)`, `push_improvements`,
  `push_tech_deficiencies(lines, project, &technologies)`, and one
  `testing_gauge(...)`. The three `draw_*_tab` fns keep their
  kind-specific detail blocks (engine cycle/Isp lines, rocket payload
  table, reactor power/enrichment) and call the shared helpers for the
  rest. The four byte-identical flaw-list copies become one.
- `RocketDesignerState` and the editors are untouched: they hold a
  `project_id` and call kind-specific company methods.

---

## 3. Sequence

Each step compiles, passes `cargo test` and clippy, matches the
`simulate` oracle (§7), and is one commit.

| # | Step | Touches | Risk |
|---|---|---|---|
| 0 ✅ | Generate `tests/saves/m5.json` from `e79811a` per the corpus recipe. The recipe is now an `#[ignore]`d test, `generate_corpus_snapshot`. Folded into the step 1 commit: m5 is written at save v1, and the compat test's "corpus predates the current version" check only holds once step 1 bumps to v2. | `tests/saves`, `save_compat.rs` | none |
| 1 ✅ | **B5.** `ImprovementId` + `next_improvement_id`; `remaining_flaw_ids` / `remaining_improvement_ids` on all three `Revising` variants (rocket's field renamed too). `SAVE_VERSION = 2`, `migrate_json` with the improvement-id stamp and the Revising index→id rewrite, plus unit tests. Delete the three shift-fixup loops. | `*_project.rs`, `company.rs`, `save.rs`, `tests/bid_rules.rs:588` | low |
| 2 ✅ | **B4 + FlawDomain.** Add `Direction`, `apply_deficiency` as inherent methods on `EngineDesign` / `ReactorDesign`; `tech_ops.rs` with the two generic fns (generic over a tiny private trait for now, since `Designable` doesn't exist yet); `flaw::generate_flaws(FlawDomain, …)`. `advance_day` shrinks by ~200 lines. | `engine.rs`, `reactor.rs`, `flaw.rs`, `third_party.rs`, `game_state/{advance,tech_ops,mod}.rs` | low |
| 3 | **B1.** `src/project.rs` with `DesignStatus`, `DesignProject<D>`, `Designable`, `Improvement<K>`, `WorkEvent`. Implement `Designable` for the three designs (moving the improvement generators and `apply_deficiency` in). Type aliases. Delete the duplicated methods from the three `*_project.rs`. `EngineSpec` (B7). `tick_daily_research` still maps three ways to `GameEvent` (that's step 4). | `project.rs` (new), `*_project.rs`, `company.rs`, `technology.rs` | **medium** — serde generic bounds, `flatten` |
| 4 | **B3.** `GameEvent::Project { kind, name, event }`; event-log JSON migration + table; `ResearchTick` by `ProjectRef`; Display table in §2.6; update the pinned-string test and the six `matches!` sites in `game_state/tests.rs`; UI's three direct constructions. | `event.rs`, `save.rs`, `company.rs`, `advance.rs`, `flight_ops.rs`, `ui/mod.rs`, tests | medium — save format |
| 5 | **B2.** `ProjectKind` public + `ProjectRef`, `ProjectCore`, company dispatch collapse, `policy.rs` / `report.rs` callers, `handle_project_pane_key`, shared draw helpers. | `company.rs`, `ui/mod.rs`, `ui/draw.rs`, `policy.rs`, `report.rs`, tests | medium — largest diff, but mechanical |

Steps 1 and 2 are independent of each other and could be reordered.
Step 3 depends on both. Steps 4 and 5 depend on 3 and are independent
of each other.

Rough size: steps 1–3 remove ~450 lines net; step 4 removes ~120; step
5 removes ~300 (company) + ~250 (ui). Total around 1,100 lines lighter
with one new ~400-line `project.rs`.

---

## 4. Serde details worth checking early (step 3 spike)

Before committing to §2.2, a 30-minute spike should confirm on a
scratch struct:

1. `#[derive(Deserialize)]` on `DesignProject<D>` with
   `#[serde(bound = …)]` and associated-type fields compiles, and that
   `#[serde(default)]` on individual fields coexists with a
   `#[serde(flatten)]` sibling (it does in serde ≥ 1.0.100, but the
   combination with `deny_unknown_fields` does *not* — we don't use
   that).
2. `#[serde(flatten)] spec: NoSpec` where `struct NoSpec {}` round-trips
   a rocket project with no extra keys emitted.
3. `Vec<Improvement<Never>>` with `enum Never {}` derives cleanly.
4. Round-trip of an m4 engine project JSON object through
   `DesignProject<EngineDesign>` is byte-for-byte identical on
   re-serialise (field order may differ; compare as `Value`).

If (1) or (2) bites, the fallback is to drop `flatten` and put `preset`
/ `scale` inside `EngineDesign` with a one-shot JSON migration moving
the two keys; that is more churn, so the spike decides.

---

## 5. Determinism

RNG consumption per project per day must be identical before and after.
Today's order, which the generic `apply_daily_work` preserves:

1. `InDesign` completing: one flaw-generation burst (gaussian count,
   then per flaw the core roll; rocket/reactor draw one extra trigger
   roll per flaw before the core roll — the `FlawDomain` branch keeps
   that exact placement).
2. Each `Testing` cycle: `roll_discoveries_with_rng` over all flaws,
   then **one** `rng.gen::<f64>()` for the improvement chance *only for
   engine and reactor*, then the improvement generator's own draws if it
   fires.
3. `Revising`: no RNG.

Rocket must not draw the improvement roll (it doesn't today).
`improvement_chance() -> None` short-circuits before the draw.

Tech-deficiency resolution draws from `contingent_rng` in
`advance_day` in list order: all engine attempts, then engine
new-designs, then reactor attempts, then reactor new-designs. The
generic version is called in the same order.

---

## 6. What the trait deliberately leaves per-domain

Per your note that flaw prevalence and nature should stay specialised:

- **Prevalence**: `flaw_complexity()` (engine's effective complexity vs
  stored complexity), the `FlawsConfig` endurance fractions
  (`rocket_endurance_fraction`, `reactor_endurance_fraction`, none for
  engines), and the per-domain improvement chance fields.
- **Nature**: `FlawDomain` selects the description pools and whether
  `PerDay` flaws exist; `FlawConsequence` wording differs per domain in
  the UI (`short_label` vs `reactor_short_label`, done in F5).
- **Tech deficiency kinds**: `TechDomain` already partitions which kinds
  a technology can generate; `apply_deficiency` on each design ignores
  kinds from the other domain, as the four inline matches do today.

Everything else — phases, work rates, testing cycles, revision costs,
NRE accrual, team assignment — is symmetric, which matches the
"behaviour should be symmetrical" half of your answer.

---

## 7. Verification

- `cargo test` and `cargo clippy --all-targets` green at every step.
- **Oracle:** before step 1, run (step 1 result: identical, 2,896 rows;
  the "before" build lives in a git worktree of `e79811a` so later steps
  can re-run it)
  ```
  cargo run --release --bin simulate -- --seeds 1..40 --years 6 --policy basic --summary-only --csv /tmp/oracle_before.csv
  ```
  and keep the CSV. After each step, re-run to a second path and
  `diff` the two. Steps 1–3 and 5
  must be byte-identical. Step 4 changes only event *text*, which the
  CSV doesn't carry, so it should also be identical; if the harness
  ever counts project events, that column would shift and the reason
  would be known.
- `tests/save_compat.rs` (m1–m5) after step 1 proves the improvement-id
  stamp on real data and after step 4 the event-log migration; the
  step 1 unit test proves the Revising migration on a synthetic one.
- `tests/sim_bands.rs` and `tests/seed_fairness.rs` as a second net.

---

## 8. Questions for you

1. **Failed tech-deficiency attempts** are logged today as
   `FlawDiscovered` events with a "Failed to resolve…" description.
   Give them their own `ProjectEvent::TechDeficiencyUnresolved`
   (recommended), or keep reporting them as flaws for continuity?
   USER:  Yeah, a new event would make sense here.
   CLAUDE: Will do — `ProjectEvent::TechDeficiencyUnresolved`.
2. **Rocket `Proposed`.** The shared status gives rockets a `Proposed`
   variant nothing constructs. Fine to leave dormant? It would be the
   hook if you ever want rocket drafts to work like engine drafts.
   USER:  Having rocket drafts makes sense but we'd have to think about the UI.
   It theoretically makes sense so its possibly good that it falls out of this
   but no sense hooking it up now so yeah leave it dormant.
   CLAUDE: Left dormant; noted as a future hook in the status doc comment.
3. **Event wording.** Normalise to the table in §2.6 (engine strings
   gain an "Engine" prefix, one pinned test updated), or keep the exact
   current strings per kind (a per-kind lookup table instead of a
   template; slightly more code, zero visible change)?
   USER:  Go ahead and normalize.
   CLAUDE: Table in §2.6 it is; the pinned-string test gets updated in step 4.
4. **Corpus m5.** Generate `tests/saves/m5.json` from the current
   commit before starting? Recommended: it is the last save shape
   before the format bump, and the corpus recipe says to generate, not
   hand-edit.
   USER:  Yup.
   CLAUDE: Step 0, from `e79811a`, following the recipe in the corpus header.
5. **`RetireTarget` → `ProjectRef`.** Rename outright (touches ~12
   sites in `company.rs` / `ui/mod.rs`) or keep `RetireTarget` as a
   type alias?
   USER:  I guess it didn't make sense to have this duplicated.  Yeah,
   ProjectRef instead.
   CLAUDE: Renamed outright in step 5; `RetireRefusal` / `RetirementEffects` keep their names since they are about retiring.
6. **Improvement addressing.** Keep `remaining_improvement_indices`
   (append-only vec, indices stable, no new id type) as proposed, or
   introduce `ImprovementId` for symmetry with flaws?
   USER:  Update for symmetry.
   CLAUDE: Done in the design below (§2.1, §2.2, §2.7, step 1). Consequences:
   `Improvement<K>` gains `id: ImprovementId`; each project gets a
   `next_improvement_id` counter (per project, not per company — see
   §2.2 for why); the v<2 JSON migration stamps ids on improvements
   already in a save and rewrites any `remaining_improvement_indices`
   to ids. Revision then removes from both queues by id.
   Implementation note: improvements are also snapshotted onto
   manufacturing orders and inventory engines (`manufacturing.rs:35,
   137, 324`), which the first draft missed; the id stamp therefore
   walks the whole document rather than just the project lists.
7. **Step order.** Steps 1 and 2 first as two small commits, then the
   step-3 spike before writing the generic for real — agreed? Or would
   you rather see the spike result before approving step 3 at all?
   USER:  Agreed.
   CLAUDE: Starting with step 0 and step 1.
