# 20_PROPULSION — One enum for what an engine is

Status: **agreed 2026‑09‑20** (fold via `EngineDesignRepr`; cycle stays; contract only for future kinds; sails read the Sun in step 5; blanket approval for steps 1–4). Records appended to §3 as commits land. Option C of `19_NOZZLES.md`
§3: lift "what kind of thing is this engine" out of scattered fields and
cycle matches into one `Propulsion` value on `EngineDesign`, with the
environment‑dependent questions answered in one place. A pure refactor
(oracle byte‑identical) except one optional, separately gated step.
Questions at the end; answer inline as `USER:` and I will reply as
`CLAUDE:`.

## 0. What the code asks today, and where

`EngineDesign` answers "what kind of engine is this" five different
ways, and callers pick whichever is handy:

| question | how it is answered today | asked from |
|---|---|---|
| Does the air act on it? | `has_nozzle()` = `chamber_pressure_pa > 0 && expansion_ratio > 0`; before step 2 it was `exit_pressure_pa` | ascent (via `atmosphere_response`), save back‑fill, editor, `BellFigures` |
| Which bell is this? | `needs_atmosphere` / `is_vacuum_variant()` | designer ×3, draw ×2, `design_variant`, fingerprint |
| Is it low‑thrust (routes from orbit, spiral edges)? | `is_low_thrust()` = `matches!(cycle, ElectricPropulsion \| SolarSail)` | path planner ×2, `RocketDesign::is_low_thrust`, stats, staging, stage sizing, designer ×3 |
| Does it burn propellant at all? | `is_solar_sail()` = `matches!(cycle, SolarSail)` (and `mass_flow_rate` returns 0 when `v_e == 0`) | stats, staging, draw, `Rocket::group_has_attached_sail` |
| Does it need electrical power? | `power_draw_w > 0` | `rocket/power.rs`, `power.rs`, editors, tabs, fingerprint |
| Is its tank a casing? | `is_solid()` = one propellant, `SolidMix` | designer ±propellant ×2 |

Two of these key off `EngineCycle`, which is really the R&D and flaw‑pool
key (`balance.rs` complexity, `flaw.rs` pools, the editor's Cycle row);
`ElectricPropulsion` and `SolarSail` are not cycles at all, they are
kinds wearing the cycle enum. Three key off flat `f64` fields whose
zero means "not applicable". Nothing checks that the fields agree with
the cycle. A future jet engine would need a sixth answer and a seventh
flat field.

Also notable: **solar‑sail thrust is constant.** `thrust_n` is "10 mN at
1 AU" per the baseline comment, but nothing scales it by distance; a
sail at Jupiter pushes as hard as one at Earth. That is exactly the kind
of question the enum is for, and it is the one non‑neutral step below.

## 1. The enum

```rust
/// How an engine makes thrust, and what its surroundings do to it.
/// One value per design; the cycle stays the R&D key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Propulsion {
    /// Exhaust through a De Laval bell: chemical and nuclear thermal.
    Nozzle {
        chamber_pressure_pa: f64,
        expansion_ratio: f64,
        gamma: f64,
        /// The short bell, built to fire at sea level. The long bell
        /// is `false`. (Today's `needs_atmosphere`.)
        sea_level: bool,
    },
    /// Electric thruster: thrust is capped by the electrical power the
    /// stage can spare (`power_draw_w` at full thrust).
    Electric { power_draw_w: f64 },
    /// Solar sail: no propellant; thrust from sunlight.
    Sail,
}
```

Gone from `EngineDesign`: `exit_pressure_pa`, `needs_atmosphere`,
`power_draw_w`, `chamber_pressure_pa`, `expansion_ratio`, `gamma`.
Added: `pub propulsion: Propulsion`. Everything else (`thrust_n`,
`mass_kg`, `isp_s`, `propellant_mix`, `cycle`, `name`, `id`) stays:
those describe every kind.

`is_solid` stays a propellant question (`propellant_mix` is one
`SolidMix`), because it is one: a casing is a tank property, and the EDL
/ structure plan is the place to give tanks their own type.

### What the enum answers

```rust
impl Propulsion {
    /// What the air does to thrust, once per design (19_NOZZLES.md §3).
    pub fn atmosphere_response(&self) -> AtmosphereResponse;
    /// Fraction of rated thrust at `env`. Today: the atmosphere
    /// response for a nozzle; 1.0 for the rest (a sail ignores
    /// `sun_distance_au` until step 5 makes it read it).
    pub fn thrust_fraction(&self, env: &ThrustEnvironment) -> f64;
    /// Electrical power at full thrust; 0 unless `Electric`.
    pub fn power_draw_w(&self) -> f64;
    /// Routes from orbit on spiral edges; never lifts off.
    pub fn is_low_thrust(&self) -> bool;     // Electric | Sail
    /// Consumes propellant (a sail does not; its burn is ∞).
    pub fn consumes_propellant(&self) -> bool; // !Sail
    /// Carries a bell the air acts on.
    pub fn has_nozzle(&self) -> bool;
    /// The short bell (None: no bell, or the long one).
    pub fn is_sea_level_bell(&self) -> bool;
    /// Exit pressure of a bell; 0 without one. Derived, never stored.
    pub fn exit_pressure_pa(&self) -> f64;
}

/// Where an engine is firing, for `thrust_fraction`.
pub struct ThrustEnvironment {
    pub ambient_pressure_pa: f64,
    pub sun_distance_au: f64,
}
```

`EngineDesign` keeps thin delegates with today's names
(`is_low_thrust`, `is_solar_sail`, `is_vacuum_variant`, `has_nozzle`,
`isp_fraction_at`, `atmosphere_response`, `with_vacuum_bell`,
`set_nozzle`…) so the fourteen call sites outside `engine.rs` do not
change in step 1; step 2 then moves the ones that read better as a
match onto the enum. `EngineDesign::validate` gains one rule: the
propulsion kind matches the cycle (`Electric` ↔ `ElectricPropulsion`,
`Sail` ↔ `SolarSail`, `Nozzle` ↔ the rest).

### Why the cycle stays

`EngineCycle` is what the player picks in the editor's Cycle row, what
`balance.rs` prices complexity by, what `flaw.rs` picks a flaw pool by,
and what the technology deficiencies target. Those are R&D facts, and
"electric propulsion" is a reasonable R&D bucket even if it is not a
thermodynamic cycle. Splitting the editor into a Kind row and a Cycle
row would be a UI change with no player benefit today. So: the cycle
stays the R&D key, `engine_baseline(cycle, preset)` decides the
`Propulsion` for a family, and `validate` keeps them consistent. When
an air‑breathing family arrives it gets a cycle (`Turbojet`,
`AirTurborocket`) *and* a `Propulsion::AirBreathing` — the two enums
answer different questions and both grow.

## 2. Saves

`EngineDesign` is stored flat in every save, in six places (engine
projects, third‑party catalogue, contracted engines, rocket projects'
stages, flights' stages, spacecraft's stages) and in every stage of
every rocket design in the corpus. A version‑gated JSON migration would
have to walk all of them; a serde‑level fold does not.

`#[serde(from = "EngineDesignRepr")]` on `EngineDesign`, the pattern
`GameSeed` uses (`SeedRepr`). `EngineDesignRepr` derives `Deserialize`
with every current field plus the six legacy flat fields as
`Option<f64>` / `Option<bool>`, and `propulsion: Option<Propulsion>`.
`From<EngineDesignRepr> for EngineDesign`:

- `propulsion` present → use it.
- else fold the legacy fields: `cycle` `ElectricPropulsion` →
  `Electric { power_draw_w }`; `SolarSail` → `Sail`; otherwise
  `Nozzle { chamber_pressure_pa, expansion_ratio, gamma, sea_level:
  needs_atmosphere }`, and when `chamber_pressure_pa` is 0 with an
  `exit_pressure_pa` on record, fit the bell the way
  `save::backfill_nozzles` does today (`chamber_pressure_for_legacy`,
  `set_nozzle_for_exit_pressure`).

`Serialize` is the plain derive on the new shape. `save::backfill_nozzles`
is deleted: the fold is the one place a legacy design is repaired, and
it runs for every holder automatically. `SAVE_VERSION` does not bump:
this is shape‑tolerant reading, not a versioned migration (same
reasoning as v1 in `save.rs`). The compat corpus and the round‑trip
idempotence test gate it.

## 3. Steps (one commit each; blanket approval requested in Q5)

1. **The enum and the fold.** `Propulsion`, `ThrustEnvironment`,
   `EngineDesignRepr`; flat fields removed; delegates on `EngineDesign`
   keep every caller compiling; `engine_baseline` / `design` /
   `with_vacuum_bell` / `set_nozzle` write the enum; `fingerprint`
   hashes it; `backfill_nozzles` deleted. Every `EngineDesign` literal
   (≈40, all tests and the three third‑party / two DinoSoar engines)
   gets `propulsion:` in place of the six fields — by script, as in
   19 step 2, with `test_util` gaining `electric_engine` and
   `sail_engine` constructors so tests stop spelling the enum out.
   Gates: oracle byte‑identical, save corpus + round‑trip, 672 tests,
   clippy.
2. **Kind questions read the enum.** `is_low_thrust` / `is_solar_sail`
   stop matching on `EngineCycle`; the path planner, staging, stats,
   stage sizing, `rocket/power.rs` and the designer call the
   `Propulsion` methods (or match where a match reads better);
   `validate` checks cycle ↔ kind; `compatible_cycles` unchanged.
   `EngineDesign::is_solar_sail` and friends become one‑line delegates
   or go, whichever leaves fewer names. Gate: oracle byte‑identical.
3. **Flight asks the environment.** `Rocket::current_accel_m_s2`,
   `RocketDesign::initial_accel_m_s2`, `group_effective_thrust_n` and
   the ascent build a `ThrustEnvironment` and call `thrust_fraction`;
   `AscentThruster` carries the response as now. Sail returns 1.0 so
   nothing moves. Gate: oracle byte‑identical. (Small; may fold into 2.)
4. **The contract for the next kinds**, as documentation and a test,
   not as variants: the enum's doc lists what a new kind must answer
   (`atmosphere_response`, `thrust_fraction`, `power_draw_w`,
   `is_low_thrust`, `consumes_propellant`, a flaw pool, a cycle, a
   baseline, an editor entry) and where each lands; an exhaustive
   `match` in a test makes adding a variant a compile error until each
   question has an answer. `AirBreathing` and `PulseUnit` are *not*
   added now: a variant nothing constructs is dead code, and each needs
   its own plan (baselines, balance, editor, flaws, launch‑site rules
   for Orion). What 19 §4 said about them still holds; this step makes
   sure the seams they need exist.
5. **Optional, behaviour change: sails read the Sun.** `Sail`'s
   `thrust_fraction` returns `1 / au²` (the baseline's 10 mN is at
   1 AU). Gate: 200‑seed bands and a look at any sail rocket in the
   corpus. Only if you want it now — it is the first thing the enum
   makes easy, and a nice check that the seam works.

Roughly a day for 1–4.

### Step 1 record (2026‑09‑20)

`engine::Propulsion { Nozzle { chamber_pressure_pa, expansion_ratio,
gamma, sea_level }, Electric { power_draw_w }, Sail }` with
`nozzle(..)`, `nozzle_for_exit_pressure(..)`, `has_nozzle`, `bell()`
(the usable `(p_c, ε, γ)` or None), `is_sea_level_bell`,
`exit_pressure_pa` (derived), `atmosphere_response`, `thrust_fraction(&
ThrustEnvironment)`, `power_draw_w`, `is_low_thrust`,
`consumes_propellant`. `ThrustEnvironment { ambient_pressure_pa,
sun_distance_au }` with `VACUUM_1AU` and `at_pressure`. `EngineDesign`
lost the six flat fields and gained `propulsion`; it reads through
`EngineDesignRepr` (`#[serde(from)]`), which folds the flat fields and
fits a pre‑nozzle design's bell from its exit pressure
(`chamber_pressure_for_legacy(cycle, mix)`), so `save::backfill_nozzles`
is gone. Delegates keep every caller's name (`has_nozzle`,
`exit_pressure_pa()`, `power_draw_w()`, `is_vacuum_variant`,
`set_nozzle`, `set_nozzle_for_exit_pressure`, `with_vacuum_bell`,
`isp_fraction_at`, …); `is_low_thrust` / `is_solar_sail` still read the
cycle until step 2. `EngineBaseline::design` builds the enum directly.
The fingerprint hashes the variant and its data. 36 literals converted
by script (`propulsion: Propulsion::nozzle(..)` / `Electric {..}` /
`Sail`); the `test_util` constructors promised in §3 were not needed —
no test spelled the enum out more than once — so none were added. A
new test loads a pre‑nozzle design, a flat‑field design and an old ion
thruster from JSON and checks the fold and the round trip.

Gates: 673 tests, clippy clean, save corpus round‑trips, 40‑seed oracle
byte‑identical to `oracle_19_base`.

## 4. Questions

1. Serde fold via `EngineDesignRepr` (§2, recommended) or a versioned
   `migrate_json` arm walking every holder?
   USER:  I'll take your recommendation.
   CLAUDE: `EngineDesignRepr` fold it is.
2. `EngineCycle` stays the editor's Cycle row with `ElectricPropulsion`
   and `SolarSail` in it (recommended), or split into a Kind row and a
   Cycle row now?
   USER:  I think I might want something else besides the Propulsion enum as a
   top level in propulsion selector besides cycle in the future so leave Cycle
   for now.
   CLAUDE: Cycle stays as the editor's row and the R&D key; the enum is
   physics data only, so a future top‑level selector is free to be its
   own thing.
3. Step 4 as contract‑and‑test only (recommended), or add
   `AirBreathing` / `PulseUnit` as placeholder variants now?
   USER:  Yeah, we can add them all at once later.
   CLAUDE: Contract and exhaustive‑match test only; no placeholder variants.
4. Step 5 (sail thrust ∝ 1/au²): now, later, or not at all?
   USER:  That's a smaller change, now sounds good.
   CLAUDE: Step 5 goes in after 4, gated on the 200‑seed bands; I will
   ask before that commit since it is the one that moves numbers.
5. Blanket commit approval for steps 1–4 as with 19?
   USER:  Yes, for these steps that looks reasonable
   CLAUDE: Noted: steps 1–4 commit without asking.
