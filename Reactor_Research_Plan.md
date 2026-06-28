# Reactor Research Plan

> TODO #1 — **Make reactors researchable.** The current code (`power.rs`)
> exposes three fixed reactor presets (Small/Medium/Large) gated behind a
> single `TECH_FISSION_REACTOR` tech unlock. That gate makes reactors
> *available*, but they are not *designs* — the player can't choose
> power, temperature, fuel load, etc. and there's no project workflow
> (InDesign → Testing → flaws → Revising) the way engines have.
>
> User raised an open question: do we want **reactors only**, or also
> **fuel cells / generators** as a sibling research path with different
> efficiency / mass tradeoffs?

---

## What we already have

- `power.rs` — `PowerSource` enum (Battery / SolarPanel / Rtg / FuelCell /
  Reactor), all currently picked from `power_presets()`. Reactors are the
  only kind tech-gated.
- `technology.rs` — yearly random unlock rolls. `TECH_FISSION_REACTOR`
  has difficulty 2 (≈ 8 %/yr). Tech deficiencies are surfaced on
  *engines*, not on power sources, today.
- `engine_project.rs` — full design-project pipeline: `Proposed` →
  `InDesign` (work_completed/work_required) → `Testing` (cumulative work,
  flaw discovery, improvements) → `Revising` (apply flaws/improvements,
  attempt tech-deficiency fixes). NRE cost accrues from teams assigned.
- `flaw.rs` — generates undiscovered flaws and exposes hooks the engine
  project consumes. Largely engine-shaped today (Isp/thrust/mass
  penalties).
- `stage.rs` — `Stage::power_sources: Vec<PowerSource>`. Sources live
  directly on the stage; no project / id indirection (unlike `engine`
  which carries an `EngineSource`).
- `ui/mod.rs` Engines pane — design new engines, run revisions, modify
  scale, etc. Rocket designer has a "power editor" sub-modal that picks
  from `power_presets()`.

---

## Scope question (the user's open musing)

**Option A — Reactors only.**
Reactors get a full project pipeline; fuel cells stay as fixed presets
(they already work and have a clear physical model). Smallest surface
change. RTGs, solar panels, batteries also stay as presets.

**Option B — All "designed" power sources (reactors + fuel cells).**
Both get the project pipeline. Fuel cells trade efficiency vs. mass via
two scalars (`peak_w`, `kg_per_kwd`); player picks both. RTGs (real
fixed sizes) and solar panels (auto-sized to stage demand) stay as
presets.

**Option C — Reactors + RTGs.**
Less obvious; RTGs in real life are very discrete units (Pu-238 is
expensive and scarce). Skip.

**CLAUDE recommendation:** Option A for v1. Fuel cells already have a
sensible efficiency/mass dial in code (`kg_per_kwd`) but no flaw / tech
story is interesting there yet — they burn the same propellant the
engine does. Reactor research is the meaty one because it interacts
with the existing `TECH_FISSION_REACTOR` tech and the deficiency
machinery.

**USER:**  Fuel cells are less exciting for the player, so I think I agree we should go with option A.

---

## Proposed data model

### 1. New `reactor_project.rs`

Mirrors `engine_project.rs` but for reactors. Key types:

```rust
pub struct ReactorProjectId(pub u64);

pub enum ReactorDesignStatus {
    Proposed { work_required: f64 },
    InDesign { work_completed: f64, work_required: f64 },
    Testing { work_completed: f64 },
    Revising { remaining_flaw_indices: Vec<usize>,
               remaining_improvement_indices: Vec<usize>,
               remaining_tech_deficiency_ids: Vec<TechDeficiencyId>,
               work_completed: f64 },
}

pub struct ReactorProject {
    pub project_id: ReactorProjectId,
    pub design: ReactorDesign,        // see below
    pub status: ReactorDesignStatus,
    pub flaws: Vec<Flaw>,
    pub revision: u32,
    pub teams_assigned: u32,
    pub complexity: u32,
    pub nre_cost: f64,
    pub improvements: Vec<Improvement>,
    pub cumulative_testing_work: f64,
    pub tech_deficiency_ids: Vec<TechDeficiencyId>,
    pub technology_id: Option<TechnologyId>,  // TECH_FISSION_REACTOR
}
```

### 2. `ReactorDesign` (the actual artefact)

```rust
pub struct ReactorDesign {
    pub id: ReactorId,            // distinct from EngineId
    pub name: String,
    pub class: ReactorClass,      // Small / Medium / Large baseline
    pub scale: f64,               // multiplier on baseline
    // Derived fields (re-derived from class + scale + flaws/tech):
    pub steady_w: f64,
    pub temperature_k: f64,
    pub mass_kg: f64,             // reactor + bundled radiator
    pub material_cost: f64,
    // Future: pub initial_fuel_load_kg + decay model
}
```

Keep `ReactorClass` as the existing baseline knob; add `scale` so a
single class covers a continuous range like engines do.

USER:  It probably doesn't make sense to both have a ReactorClass and a scale.  Just have a scale that isn't limited to
.25x to 4x so it can go up and down to cover our range.  On a physical basis, having a minimum size probably does make
sense though.


### 3. New `EngineSource`-style indirection on `PowerSource`

Today `PowerSource` is owned outright by a stage. To research a reactor,
we need the stage to point at a *project*, the way `Stage::engine`
points at an `EngineSource`. Two options:

**A.** Add a `ReactorSource` field on `PowerSourceKind::Reactor`:

```rust
PowerSourceKind::Reactor {
    source: ReactorSource,         // PlayerDesign(ReactorProjectId) | Contracted(...)
    steady_w: f64,
    temperature_k: f64,
    radiator: Radiator,
}
```

The stage still owns the `PowerSource` but the reactor variant can be
traced back to a project. Save/load needs to resolve the project on
load to refresh derived fields if the design changed.

**B.** Reactors live entirely as `ReactorProjectId` references — the
stage stores only the id and the rendering code looks up the project
on demand.

CLAUDE recommendation: **A**, matching how stages own a snapshot of
`EngineDesign` today. (Note: today's engine slot is technically a
clone of `EngineDesign`, not a live reference — same effect; the
lockstep mutators preserve invariance.)

**USER:**  Yes, certainly A.

### 4. Fuel cells ride on the existing engine pipeline (only if Option B)

User constraint: minimize divergence from engines. The only differences
between a fuel cell and a chemical engine are *what* the propellant
produces (electricity vs. thrust) and *where it sits* on the stage
(power_sources vs. engine). Everything else — flaws, revisions, NRE,
complexity, propellant mixes, tech deficiencies — is identical.

Concrete approach: extend `EngineCycle` with a `FuelCell` variant and
let the existing engine project pipeline handle it.

```rust
pub enum EngineCycle {
    PressureFed, GasGenerator, Expander, StagedCombustion, FullFlow,
    NuclearThermal, ElectricPropulsion, SolarSail,
    FuelCell,    // new
}
```

`engine_baseline(FuelCell, preset)` returns a baseline where
`thrust_n = 0` and `power_draw_w` is *negative* (output, not draw) — or
we add a new field `power_output_w` to `EngineBaseline` and the engine
design. The latter is cleaner; thrust-vs-output is a real physical
distinction.

Where a fuel-cell engine lives:
- It's still an `EngineProject` — same UI screen, same revision flow.
- But on a stage it's installed via the power editor (not the engine
  slot). The stage's `PowerSource` gets a new variant
  `PowerSourceKind::FuelCellEngine { engine_id: EngineId, kg_per_kwd: f64 }`
  that points at an engine design id, or — to avoid the indirection —
  the existing `FuelCell` variant carries the `EngineDesign` snapshot.

**CLAUDE recommendation:** add a third `PowerSourceKind::FuelCell`
variant that *owns* an `EngineDesign` snapshot (the design carries
all the relevant tunables — propellant mix, mass, efficiency rolled
into `kg_per_kwd`). Player picks "Add Fuel Cell" in the power editor,
which opens the same engine editor used elsewhere with `FuelCell`
cycle pre-selected.

This means: existing `PowerSourceKind::FuelCell { peak_w, kg_per_kwd }`
gets *replaced* by `PowerSourceKind::FuelCell { engine: EngineDesign,
kg_per_kwd: f64 }`. Fuel-cell presets in `power_presets()` go away —
the player designs them like engines.

**USER:**  We're not going down this route.

---

## Reactor design knobs (what the player chooses)

For v1 I'd keep it simple and re-use the existing `ReactorClass` enum as
the *baseline* (preset), with a continuous `scale` multiplier on top.
That parallels engines exactly: pick a class, pick a scale, get a
specific reactor.

Open question — what does *scale* mean physically for a reactor?
- Linear in `steady_w` (more fissile fuel → more power)?
- Sub-linear in mass (bigger reactors are more efficient, like the
  current radiator-mass curve)?

CLAUDE recommendation:
- `steady_w` scales linearly.
- `mass_kg` scales as `scale^0.9` (mild economies of scale).
- `material_cost` scales as `scale^0.85`.
- `temperature_k` stays fixed by class (it's a physics tier, not a
  knob).

**USER:**  That all makes sense to me.

---

## Tech deficiencies, flaws, NRE

Reactors plug into the existing deficiency machinery cleanly:
- `TECH_FISSION_REACTOR` already has seed-generated deficiencies.
  Currently they're never *applied* to anything because no engine can
  be tagged with that tech. We'd tag reactor projects with
  `technology_id = Some(TECH_FISSION_REACTOR)` and apply the
  deficiencies to the reactor's `steady_w` / `mass_kg` instead of
  `isp_s` / `thrust_n`.
- `TechDeficiencyKind::IspPenalty` doesn't apply; we'd need to extend
  the kind enum or reinterpret penalties (Isp → output power,
  Thrust → output power, Mass → mass, Complexity → complexity). Cleaner:
  extend with `PowerPenalty(f64)` and only generate that kind for the
  reactor tech.
- Flaws: reuse `Flaw` (it carries arbitrary penalty fields already);
  just generate ones with `power_penalty` instead of `isp_penalty`.
  Will need a small `flaw.rs` extension to allow that variant.

**USER:**  That makes sense but in the longer run we'll potentially want different sorts of flaws that just affect reactors.

---

## Tech gating

Today `TECH_FISSION_REACTOR` blocks adding reactor presets to a stage.
After this change, what does it gate?

**Option 1.** Blocks *starting* a reactor project (the "New Reactor"
button is hidden until unlocked). Once unlocked, the player can design
freely.

**Option 2.** Reactor projects can be *started* always, but they roll
the tech's deficiencies (like NTR/Methalox engines do today). Tech
unlock just removes the deficiency rolls.

CLAUDE recommendation: **Option 1.** Today the player has no way to
"see" tech deficiencies before committing, and starting a multi-year
research project on a tech you haven't unlocked feels wrong. The
existing engine flow already pre-gates Hydrogen/Methalox tech selection
in the wizard the same way.

**USER:**  I think Option 2 is the better one here.  The principles behind fission reactors are already well known.  How
much work would it be to diverge into LEU, MEU, and HEU reactors with different default power to weight ratios and have
MEU and HEU unlock behind high levels of prestige?

---

## Fissile fuel depletion (the user's musing)

User asked about reactors that "slowly run down their fissile fuel".
Two scopes:

**Defer to a later TODO.** v1 reactors produce constant `steady_w`
forever. Matches today.

**Include in v1.** Each `Reactor` carries `fuel_remaining_kg` /
`fuel_lifetime_days`. Flight ticks drain it; once depleted, the reactor
output drops to zero. Probably want refuel-on-ground as a basic
mechanic. Adds save fields, balance numbers, and a new flight-loop
hook.

CLAUDE recommendation: **defer**. Researchability + project pipeline +
deficiencies is already a fairly large change; fuel depletion is a
self-contained future enhancement that doesn't require any of the
research scaffolding to be reworked.

**USER:**  I'm not even sure this is a feature that wouldn't just be annoying to the user out of proportional to any
realism or interesting decisions.  Certainly defer.

---

## UI surface

- **New "Reactors" pane**, parallel to Engines. Same key-binding
  shape (`R` revise, `T` start, `N` new design, etc.).
- **Engines pane** unchanged (still owns engine projects).
- **Rocket designer power editor**: replace fuel-cell + reactor
  presets with "Design Fuel Cell" / "Design Reactor" entries that open
  the relevant editor. Existing player reactor / fuel-cell projects
  appear as installable items, like engine projects do for the engine
  slot today.
- **Cleanup**: on load, sweep orphan `Proposed` reactor projects, the
  same way `Proposed` engine projects are swept.

**USER:**  That sounds like the right way to do it.

---

## What this changes outside the new file

- `power.rs` — `PowerSourceKind::Reactor` variant updated (drop fixed
  fields, add reactor design source / snapshot). Fuel-cell variant
  same. Presets list shrinks.
- `power_presets()` — drop the three reactor presets and three
  fuel-cell presets; both now come from designed projects.
- `stage.rs` — no struct change, but the rocket designer's lockstep
  mutators need a sibling for `reactor_sources` (parallel to
  `engine_sources`) so stage clones don't lose project provenance.
  *Or* — per the user's clone-on-creation convention — drop project
  provenance from the stage entirely and stash it in `RocketDesign`
  alongside `engine_sources`.
- `save.rs` — new `Vec<ReactorProject>` field on `PlayerCompany`;
  load-time sweep of orphan `Proposed` reactors.
- `game_state.rs` — daily tick for `ReactorProject` (same shape as
  engine project daily work); flight loop optionally drains fuel
  (skipped per recommendation above).
- `flaw.rs` — `PowerPenalty(f64)` variant on `TechDeficiencyKind` (or
  reinterpret existing); flaw generation extended to roll power
  penalties.
- `event.rs` — new `GameEvent` variants for reactor design milestones
  (mirroring the engine ones).
- `ui/mod.rs` — new InputMode variants for the reactor editor and
  pane; new top-level pane added to the pane cycle.
- `ui/draw.rs` — draw the reactors pane + reactor editor modal.

---

## Phased implementation

1. **Phase 1 — Reactor projects scaffolding.** Add
   `ReactorProject` + status enum + storage on `PlayerCompany`.
   Reactor pane that lists projects (read-only). Save/load. Daily tick
   that accrues work. No designer UI yet — projects created
   programmatically for testing.
2. **Phase 2a — Reactor editor (LEU only).** Modal for creating /
   editing a reactor project (`scale` + name), with `Proposed` →
   `InDesign` promotion when used in a rocket design. Power editor
   lists completed reactor projects as installable items in place of
   presets. All reactors are LEU at this phase.
3. **Phase 2b — Enrichment + reputation gate.** Add
   `EnrichmentLevel { Leu, Meu, Heu }` to `ReactorDesign`. Enrichment
   picker in the editor, filtered by current reputation
   (`REACTOR_MEU_MIN_REPUTATION = 60.0`,
   `REACTOR_HEU_MIN_REPUTATION = 150.0`). Apply per-enrichment
   mass-per-kW multipliers: LEU 1.0×, MEU 0.55×, HEU 0.30×.
4. **Phase 3 — Flaws + tech deficiencies on reactors.** Extend
   `Flaw` / `TechDeficiencyKind` with a power penalty. Reactor projects
   discover flaws in testing, can be revised. Tech-deficiency fix flow
   identical to engines. `TECH_FISSION_REACTOR` deficiencies roll on
   all reactor projects from project creation (Option 2 gating — no
   create-time tech gate).
5. *(Deferred / later TODO.)* Reactor-specific flaw kinds. Fissile
   fuel depletion model.

CLAUDE recommendation: implement Phase 1, sanity-check, then continue.

**USER:**  That makes sense

---

## Resolved decisions (rolled up from USER answers)

- **Scope:** Option A — reactors only. Fuel cells stay as fixed
  presets (no engine-pipeline merge).
- **Stage indirection:** Option A — `PowerSourceKind::Reactor` owns
  an embedded `Reactor` snapshot (clone, not live reference; matches
  how stages own an `EngineDesign` clone today).
- **Reactor knobs:** Drop `ReactorClass`. One continuous `scale` knob
  that spans the full Small→Large range. Anchor `scale = 1.0` to the
  current Medium baseline (50 kW, 1400 K, ~5000 kg total).
  Minimum `scale` enforced as a balance constant — physically
  reactors don't shrink past a critical-mass floor.
- **Scaling curves:**
  - `steady_w` ∝ `scale` (linear).
  - `mass_kg` ∝ `scale^0.9` (mild economies of scale).
  - `material_cost` ∝ `scale^0.85`.
  - `temperature_k` = `1400 * scale^0.05` — mild temperature rise with
    size; tunable later if it stops feeling right.
- **Enrichment:** LEU / MEU / HEU on the design.
  Mass-per-kW multipliers: LEU 1.0×, MEU 0.55×, HEU 0.30× (big
  differences — this is a meaningful unlock the player drives via
  reputation).
- **Reputation gates:** MEU at 60, HEU at 150
  (`REACTOR_MEU_MIN_REPUTATION` / `REACTOR_HEU_MIN_REPUTATION`).
- **Tech gating:** Option 2 — reactor projects can be started at any
  time; `TECH_FISSION_REACTOR` deficiencies roll on each one. No
  create-time tech gate.
- **Flaws / deficiencies:** Reuse `Flaw` + add a power penalty to
  `TechDeficiencyKind`. Reactor-specific flaw kinds are out of scope
  for v1.
- **Fissile fuel depletion:** Deferred.
- **UI:** New "Reactors" pane parallel to Engines. Power editor in
  the rocket designer lists completed reactor projects as installable
  items in place of the fixed reactor presets.
