# 19_NOZZLES — Nozzle variants from chamber pressure

Status: **done 2026‑09‑20** — all four steps committed (records in §6).
Option B, per cycle × propellant chamber pressures, fixed vacuum bell
from an exit‑diameter limit, "pad / vac" presentation. Follow‑ups named
in §6: the structural model and Earth‑rotation credit (own plan), and
Option C (`Propulsion` enum) when the first air‑breathing or pulse
engine is scheduled.
Questions are numbered at the end; answers inline as `USER:` /
`CLAUDE:`.

## 0. Why

The Falcon 9 probe (2026‑09‑19) put the game's kerolox rocket at
7.3 t to LEO and nothing to GTO on Falcon 9's tanks. Engines were the
largest factor, and part of the engine gap is not "the starting engine
is worse than Merlin" but the way the game models nozzles:

- **One bell at two altitudes is being used as two bells.** The
  baseline's `isp_sl_s` / `isp_vac_s` pair (270 / 310 for kerolox) is
  the same short bell at the pad and in vacuum, Merlin‑style 282 / 311.
  But `design_variant` hands the *sea‑level variant* 270 s for its whole
  burn and the *vacuum variant* 310 s. So the vacuum bell gets no real
  gain, and the sea‑level bell is charged the pad figure at 70 km.
- **Then the ascent charges the pad again.** The integrator multiplies
  thrust by `isp_fraction(exit_pressure, ambient)` on top of the stored
  sea‑level Isp. Giving the stage its vacuum figure and letting the
  ascent do the pad work alone moved the probe rocket 7.3 → 9.4 t LEO.
- **The two variants share thrust and mass.** A real vacuum bell is
  heavier and, at the same mass flow, produces proportionally more
  thrust along with more Isp.
- **Isp improvements never fly.** `EngineImprovementKind::Isp` and
  `TechDeficiencyKind::IspPenalty` edit `project.design.isp_s`, but
  `design_variant` overwrites `isp_s` from the baseline. Thrust and mass
  improvements survive because those fields are copied through.
- **The pad penalty is ad hoc and too gentle.** `isp_fraction` uses a
  slope constant K = 0.2 that gives a sea‑level bell a 4 % pad loss;
  Merlin loses 9 %, RS‑25 19 %.

## 1. The physics, and why chamber pressure is the right knob

For anything with a De Laval nozzle the ideal, frozen‑flow relations
are the same regardless of cycle or propellant. Everything below uses
the thrust coefficient `C_F`; Isp = `C_F · c* / g0`, where `c*`
(characteristic velocity) depends only on propellant and chamber
conditions, not on the nozzle.

1. **Vacuum Isp rises with expansion ratio ε = A_exit / A_throat**, with
   diminishing returns. Relative to ε 16 (Merlin 1D): ε 40 → ×1.05,
   ε 100 → ×1.09, ε 165 → ×1.10, ε 280 → ×1.12.
2. **A sea‑level bell's ε is capped by chamber pressure.** The flow
   separates from the wall if exit pressure drops below roughly
   0.35–0.4 × ambient (Summerfield). With p_exit fixed near 40 kPa,
   ε_SL is a function of p_chamber alone: 15 bar → ε 5.6, 90 bar →
   ε 22, 300 bar → ε 56. High‑pressure engines already run a big bell
   at sea level, so they have **less to gain** from a vacuum variant.
   This is your observation, and the model reproduces it: Merlin's
   MVac gains ×1.10 in Isp; Raptor's RVac only ×1.03–1.09.
3. **The pad penalty falls out of the same formula.**
   `C_F(p_a) = C_F,vac − ε · p_a / p_c`. This is linear in ambient
   pressure exactly like today's K model, but with a slope the engine
   dictates. Checked against real engines (γ = 1.2):

   | engine | p_c bar | ε | model SL Isp | real SL Isp |
   |---|---|---|---|---|
   | Merlin 1D | 97 | 16 | 282 | 282 |
   | F‑1 | 70 | 16 | 265 | 263 |
   | RD‑180 | 257 | 37 | 312 | 311 |
   | RS‑25 | 206 | 69 | 372 | 366 |
   | Vulcain 2 | 117 | 58 | 318 | 318 |
   | Raptor 2 | 300 | 40 | 325 | 327 |

4. **Thrust gain equals Isp gain.** Throat area and mass flow are set by
   the pumps and chamber, which both variants share. `F = C_F · p_c ·
   A_t`, so the vacuum bell's thrust rises by the same ratio as its
   Isp. That is one number for the variant, not two. (The game already
   enforces this: `mass_flow_rate = thrust / v_e`.)
5. **Separation risk is the same criterion.** Today
   `overexpansion_destruction_risk` ramps from p_a / p_exit = 4. The
   Summerfield limit says separation begins near 2.5–3; the ramp can
   keep its shape with the start point as a balance constant.
6. **Bell mass scales with bell area.** ΔA_exit = A_t · (ε_vac − ε_SL).
   Calibration: MVac is ~130 kg over Merlin 1D for ~8 m² of extra bell
   (~16 kg/m²); RL10B‑2's extendible nozzle is ~110 kg for ~2.7 m²
   (~40 kg/m², carbon plus a deployment mechanism). One balance
   constant `bell_areal_density_kg_m2` ≈ 20, per cycle if wanted.

Vacuum‑bell ε is a design choice bounded by mass and by the stage's
length, not by physics. Real upper‑stage bells run 80–280.

## 2. What the baseline should say

Today `EngineBaseline` carries `isp_vac_s`, `isp_sl_s`,
`exit_pressure_vac_pa`, `exit_pressure_sl_pa`. Under §1 those four are
outputs, not inputs. The inputs are:

| field | meaning | source of the value |
|---|---|---|
| `isp_ref_s` — vacuum Isp of the *sea‑level* bell (ε_SL) | propellant + chamber quality; the recognisable number (kerolox 311, hydrolox 440, …) | propellant preset × cycle Isp multiplier, as today |
| `chamber_pressure_pa` | sets ε_SL and the pad slope | cycle × propellant (table in §5) |
| `gamma` | exhaust heat‑capacity ratio | propellant preset (1.18–1.23; matters little) |
| `vacuum_only` | as today (Expander, NTR, electric, sail) | as today |

Not on the baseline: the vacuum bell's ε. It is derived per engine
from two balance constants, `max_vacuum_bell_exit_m` (3.0 m) and
`max_expansion_ratio` (300), as `ε_vac = min(A_exit,max / A_t, cap)`
with `A_t = F / (C_F,vac(ε_SL) · p_c)` — see the reply under §5. For a
vacuum‑only engine there is no sea‑level bell; its single variant is
the vacuum bell and `isp_ref_s` is still the Isp it would have at
ε_SL, so the tables stay comparable across cycles.

`c*` is never stored: `isp_ref_s · g0 / C_F,vac(ε_SL)` when the maths
needs it.

Keeping the *stored* `EngineDesign` compatible: `isp_s` stays the
variant's **vacuum** Isp, `thrust_n` its vacuum thrust, `mass_kg` its
mass with bell. Two new fields with `#[serde(default)]`:
`chamber_pressure_pa` and `expansion_ratio`. `exit_pressure_pa` becomes
derived (kept in the struct for the UI and for legacy saves).

For the ascent's hot loop the per‑nozzle datum collapses to one
constant: `zero_thrust_pressure_pa = p_c · C_F,vac / ε`, the ambient
pressure at which thrust would reach zero. Then
`fraction = 1 − p_a / zero_thrust_pressure_pa`, the same cost per step
as today.

## 3. Data‑structure options, smallest to largest

### Option A — multipliers on the baseline (½ day)

Add `vac_isp_gain` and `vac_mass_gain` to `EngineBaseline`;
`design_variant(true)` multiplies Isp and thrust by the first, mass by
the second; `design_variant(false)` uses the vacuum Isp of the short
bell and lets the ascent do the pad. Retune K in `isp_fraction`.

- Fixes: double penalty, no‑gain vacuum bell, Isp improvements lost
  (variants become multipliers on the project's current figures).
- Does not: connect anything to chamber pressure; two ad hoc numbers
  per baseline; pad penalty still a global K.
- Save impact: none to the struct.

### Option B — chamber pressure and expansion ratio, derive the rest (2–3 days) — **recommended**

The §2 baseline. One module `src/nozzle.rs` with the ideal relations
(`cf_vac(ε, γ)`, `eps_for_exit_pressure`, `zero_thrust_pressure`,
`bell_mass`), unit‑tested against the §1 table. `design_variant`
derives ε_SL from p_c, ε_vac from the baseline, and produces Isp,
thrust, mass and exit pressure for each. `isp_fraction` and
`overexpansion_destruction_risk` read the derived constants instead of
K. `EngineDesign` gains `chamber_pressure_pa` and `expansion_ratio`
(defaults 0 → `save::sanitize` back‑fills from the cycle baseline and
the stored exit pressure, so there is one formula, not a legacy path).

- Fixes everything in Option A, and makes the vacuum gain shrink with
  chamber pressure as you described.
- Gives the technology system real levers: a deficiency that lowers
  chamber pressure makes an engine heavier *and* worse at sea level
  *and* gain more from a vacuum bell, which is how an RL‑10‑class
  problem actually behaves. An improvement can raise p_c or ε_vac.
- Engines list and editor show both bells: `SL 282 s / 845 kN · Vac
  348 s / 981 kN (+130 kg)`, which also answers the thrust‑column
  question from the 19th.
- Save impact: two defaulted fields plus a sanitize back‑fill; the
  compat corpus gates it.
- Balance impact: the bot flies vacuum variants on upper stages, so
  the oracle changes. Gate is the 200‑seed bands, and a re‑baseline of
  `oracle_g_base` afterwards.

### Option C — a propulsion‑kind abstraction (1–2 weeks, not now)

Replace the scattered predicates (`is_low_thrust`, `is_solar_sail`,
`is_solid`, `needs_atmosphere`, the `power_draw_w` special case) with
a data enum on `EngineDesign`:

```rust
pub enum Propulsion {
    Nozzle { chamber_pressure_pa, expansion_ratio, gamma },   // chemical, NTR
    Electric { power_draw_w },
    Sail,
    // later:
    AirBreathing { ceiling_pa, max_mach, ram_recovery },      // jets, ATR
    PulseUnit { yield_j, plate_efficiency },                  // Orion
}
```

and one method `performance_at(&Environment) -> (thrust_n, isp_s)`
with `Environment { ambient_pressure_pa, air_density, airspeed_m_s,
sun_distance_au, available_power_w }`. The ascent integrator, the
flight burn and the designer stats all call it. This is the shape that
takes jets and Orion (§4), but it touches every consumer of
`EngineDesign` and is not needed for the nozzle fix.

**Recommendation:** B, with one deliberate step toward C: generalise
`AscentNozzle { exit_pressure_pa, thrust_n }` to
`AscentThruster { thrust_n, response: AtmosphereResponse }` where
`AtmosphereResponse` is `Nozzle { zero_thrust_pressure_pa }` or
`None`. That is a small enum today and the seam where `AirBreathing`
and `PulseUnit` variants land later.

USER:  I'm atually leaning towards C right now but I'm wondering about the
performance in acent where we recompute things many times over the course of
getting to orbit.  Could we pre-compute the thrust/ISP in a table at N mBar
incriments to make that go faster?

CLAUDE: We would not need a table, and C does not make the loop more
expensive if we shape `performance_at` the right way. Measured today
(release, `probes capability::compute_timing`):

| call | cost |
|---|---|
| `DesignPerformance::compute` (one full ascent integration) | 52 µs |
| `max_payload_to` (bracket + bisection, ~18 integrations + planner) | 924 µs |

The integrator steps at 0.25 s, so a Falcon‑9‑class ascent is ~2 200
steps. The only transcendental per step is the `exp` inside
`pressure_at(altitude)`, and the loop already skips it above the
altitude where every nozzle is under‑expanded. The nozzle formula
itself is `1 − p_a / p_zero`: one divide. A table lookup with
interpolation would cost about the same as that divide, and it would
be a table per engine where today there is one constant per engine.

What makes C cheap is splitting it into two calls, which is what the
integrator already does implicitly with `penalty_ceiling_m`:

- `Propulsion::prepare(&Surface) -> AscentResponse` — once per
  design, when the ascent phases are built. For a nozzle this
  computes `C_F,vac`, ε and `p_zero`. For an air‑breather it would
  precompute its density/Mach curve coefficients. This is where the
  nozzle maths lives; it never runs inside the step loop.
- `AscentResponse::thrust_fraction(pressure_pa, airspeed_m_s) -> f64`
  — per step, arithmetic only. For `Nozzle` one divide; for `None`
  the constant 1.0; for `AirBreathing` a couple of multiplies.

That keeps the step cost where it is. If an ascent ever does become
slow, the thing to tabulate is pressure against altitude, one table
per body, because that is where the `exp` is; a thrust table would
not help an air‑breather anyway, whose thrust depends on density and
airspeed together, so a 1‑D table in millibars cannot represent it.

The cost that actually matters is how many integrations run, not
what each step does: the payload bisection, the designer recomputing
on every keystroke, and the capability caches over every design ×
destination. Those are unchanged by A, B or C.

So C is fine on performance. Its cost is breadth: the enum replaces
the `is_low_thrust` / `is_solar_sail` / `is_solid` /
`needs_atmosphere` / `power_draw_w` predicates and every match on
`EngineCycle` that stands in for "what kind of thing is this", which
is a wide, mechanical refactor with a save migration. If you want C,
I would still land it as B first (physics and derived variants, with
the `AscentResponse` seam above) and then a second plan that lifts
the predicates into the enum with the oracle byte‑identical, since
that step changes no numbers. Same destination, two gates instead of
one.

## 4. Future propulsion, and what it needs from the abstraction

**Jet boosters / air‑turborockets** (The Rocket Company's pop‑up
first stage, or an ATR). Thrust depends on air density and Mach, drops
to zero above a ceiling (~20 km, Mach 3–4 for turbojets; Mach 5–6 for
ATRs); no oxidiser carried (the `propellant_mix` is fuel only, so the
cost model is fine); effective Isp 3 000–4 000 s for turbojets, 600–
1 000 s for ATRs. Needs from the ascent: velocity‑ and density‑
dependent thrust and a cutoff, both of which the integrator already
tracks per step. Needs from the graph: a "reaches altitude X and Mach
Y, then hands over" edge, or simply group 0 producing its Δv within
the ascent and the planner charging the rest to the rocket groups,
which is what happens today for any group that runs dry before orbit.
Fits Option C's `AirBreathing` variant. Under Option B it could be
faked as a `Nozzle` with a huge Isp and an `AtmosphereResponse` that
*rises* with pressure, which is why the response enum is worth adding
now.

**Project Orion** (nuclear pulse). Pusher plate, no nozzle: thrust
essentially independent of ambient pressure (`AtmosphereResponse::
None`), Isp 3 000–6 000 s, MN‑class thrust, so it launches from a
surface. Propellant is discrete pulse units; today's continuous
`propellant_mass_kg` is a fine approximation (a burn is thousands of
pulses). The interesting constraints are not physics: launch from
Earth's surface forbidden or catastrophic for reputation, launch from
Mars or Titan allowed. That is a per‑launch‑site rule keyed on the
engine kind, which wants the `Propulsion` enum to exist so the rule has
something to match on. A `PressureFed`‑style pseudo‑cycle would work
under Option B but would be the wrong shape.

**Turborockets / air‑augmented rockets** sit between the two: a
nozzle whose thrust is boosted by ram air below a ceiling. Same
`AirBreathing` machinery plus a `Nozzle` fallback above it.

So: Option B now, with the `AtmosphereResponse` seam; Option C when
the first air‑breathing or pulse engine is actually scheduled, at
which point the enum replaces the predicates in one plan.

## 5. Balance values to start from

Chamber pressure per cycle × propellant (revised per Q2), with what
the model derives from it at scale 1.0 and a 3 m exit limit. Real
anchors: Kestrel/AJ10 (pressure‑fed 9–10 bar), Merlin/F‑1 (gas
generator 70–97), RL10/Vinci (expander 44–60), RD‑180/RS‑25/BE‑4
(staged 134–257), Raptor (full‑flow 300). Kerosene is a poor expander
coolant and hypergolics a poor one too; those rows exist so the
combination stays legal, at low pressure.

| cycle | propellant | p_c bar | ε_SL | pad Isp fraction | ε_vac (3 m exit, scale 1) | vac gain |
|---|---|---|---|---|---|---|
| PressureFed | Kerolox | 15 | 5 | 0.78 | 32 | ×1.12 |
| PressureFed | Hydrolox | 15 | 6 | 0.77 | 267 | ×1.21 |
| PressureFed | Methalox | 15 | 6 | 0.77 | 42 | ×1.13 |
| PressureFed | Hypergolic | 10 | 4 | 0.74 | 300 | ×1.23 |
| PressureFed | Solid | 60 | 17 | 0.84 | 257 | ×1.12 |
| GasGenerator | Kerolox | 90 | 21 | 0.87 | 128 | ×1.07 |
| GasGenerator | Hydrolox | 100 | 24 | 0.87 | 300 | ×1.10 |
| GasGenerator | Methalox | 90 | 21 | 0.87 | 165 | ×1.08 |
| GasGenerator | Hypergolic | 60 | 15 | 0.86 | 300 | ×1.11 |
| Expander (vac only) | Kerolox | 40 | — | — | 69 | ×1.09 |
| Expander (vac only) | Hydrolox | 45 | — | — | 300 | ×1.14 |
| Expander (vac only) | Methalox | 45 | — | — | 100 | ×1.10 |
| Expander (vac only) | Hypergolic | 40 | — | — | 300 | ×1.13 |
| StagedCombustion | Kerolox | 250 | 46 | 0.90 | 300 | ×1.06 |
| StagedCombustion | Hydrolox | 200 | 41 | 0.89 | 300 | ×1.07 |
| StagedCombustion | Methalox | 135 | 29 | 0.88 | 219 | ×1.07 |
| StagedCombustion | Hypergolic | 150 | 30 | 0.89 | 300 | ×1.07 |
| FullFlow | Kerolox | 250 | 46 | 0.90 | 283 | ×1.06 |
| FullFlow | Hydrolox | 250 | 49 | 0.90 | 300 | ×1.06 |
| FullFlow | Methalox | 300 | 55 | 0.90 | 300 | ×1.05 |
| FullFlow | Hypergolic | 200 | 37 | 0.90 | 300 | ×1.06 |

γ per propellant: kerolox 1.22, hydrolox 1.20, methalox 1.21,
hypergolic 1.23, solid 1.18. Other constants: sea‑level bell exit
pressure 0.4 × 101 325 Pa (Summerfield), `max_vacuum_bell_exit_m`
3.0, `max_expansion_ratio` 300, `bell_areal_density_kg_m2` 20,
separation‑risk ramp from p_a / p_exit = 2.5. All in `BalanceConfig`
so the TOML layering can move them.

For the starting kerolox gas generator this gives 271 / 311 s for the
sea‑level bell at the pad and in vacuum, and ~333 s for the vacuum
bell at +7 % thrust and about +20 % mass. The "vac gain" column is the
ratio the player sees as the difference between the two bells; for
vacuum‑only cycles it is the gain over the hypothetical short bell
and only the vacuum figure is shown. Existing tests that pin 270 / 310
or `vac.thrust_n == sl.thrust_n` (`test_vacuum_vs_sea_level_isp`)
change in the same commit.

**Presentation (per Q5):** everywhere an engine's Isp is shown, the
format is `A / B s`, A the sea‑level bell's Isp *at the pad*, B the
vacuum bell's Isp *in vacuum*; a vacuum‑only engine shows just `B s`.
Thrust and mass get the same pair where there is room (engine editor,
expanded engine row); the compact engine list keeps one column each
and labels them. Note this is a different pairing from today's
`SL / vac` column, which shows one bell at two altitudes.

`isp_ref_s` on the baseline stays "vacuum Isp of the sea‑level bell"
(§2), so `engine_baseline` numbers remain recognisable.

USER: Why do different engine classes have different ε_vac values?  Convention?

CLAUDE: Convention plus a shortcut on my part, and there is a better
rule. The per‑cycle numbers in §5 are just what the flagship engine
of each cycle happens to run. The physical limit on a vacuum bell is
not ε but **exit diameter**: the bell has to fit inside the stage and
the interstage, and its mass grows with exit area. ε is exit area over
throat area, and throat area is set by thrust and chamber pressure,
`A_t = F / (C_F · p_c)`. So for the same exit diameter:

- a **high‑pressure** engine has a small throat and reaches a large ε
  cheaply,
- a **high‑thrust** engine has a big throat and runs out of diameter at
  a modest ε,
- a **small** engine gets an enormous ε.

That one constraint explains the three reference engines with a
single number. Exit diameter from thrust, p_c and ε:

| engine | thrust | p_c | ε | exit diameter (derived) | actual bell |
|---|---|---|---|---|---|
| MVac | 934 kN | 97 bar | 165 | 3.4 m | ~3.3 m |
| RVac | 2.5 MN | 300 bar | 90 | 2.3 m | ~2.4 m |
| RL10B‑2 | 110 kN | 44 bar | 280 | 2.2 m | 2.15 m |

The staged‑combustion and full‑flow engines in §5 got low ε defaults
because they are both high‑pressure *and* high‑thrust, and the second
effect wins for the exit size a stage allows; nothing about the cycle
itself limits the bell.

So I would replace the per‑cycle `vacuum_expansion_ratio` column in
§2 and §5 with:

- one balance constant `max_vacuum_bell_exit_m` (start at 3.0 m, the
  upper end of what a single‑engine upper stage carries), and a cap
  `max_expansion_ratio` ≈ 300 where the gain has flattened to under
  1 % per 50;
- `ε_vac = min(A_exit,max / A_t, cap)`, derived per engine from its
  own thrust and chamber pressure, so the scale slider also moves it:
  scaling an engine up shrinks its achievable vacuum ε, which is real
  and a nice trade‑off for the player to run into.

Later, if stages get a diameter (the fairing already has one), the
exit limit becomes a stage property and a clustered upper stage has to
share the diameter among its engines, `A_exit,max / engine_count`.
That would fit naturally and needs nothing now beyond keeping the
limit a parameter rather than a constant baked into `design_variant`.

## 6. Steps (one commit each, ask before each)

1. `src/nozzle.rs`: ideal relations, unit tests pinned to the §1 table
   within 2 %. No callers yet. (Pure addition; oracle unchanged.)
2. `EngineBaseline` → §2 fields; `EngineDesign` gains
   `chamber_pressure_pa`, `expansion_ratio` with defaults; sanitize
   back‑fill; `design_variant` derives both bells as multipliers on the
   project's current figures (fixing the lost Isp improvements).
   `isp_fraction` / risk read derived constants; `AscentNozzle` →
   `AscentThruster` with `AtmosphereResponse`. Save corpus, 200‑seed
   bands, re‑baseline oracle. Balance values from §5.
3. UI: the `A / B s` presentation from §5 in the engine list, engine
   editor and designer stage rows (thrust and mass pairs where there
   is room); help text. Render smoke test.
4. Falcon 9 probe as an ignored test in `tests/probes.rs` (the scratch
   version from the 19th, kept this time) so the next balance question
   has a yardstick.

Follow‑up, separate plan: the structural model (thrust structure at
2.5 % of thrust, upper‑stage tank fraction) and the missing Earth‑
rotation credit, which together cost the real Falcon 9 about 20 % in
the same probe.

### Step 1 record (2026‑09‑20, uncommitted)

`src/nozzle.rs`: `vandenkerckhove`, `expansion_ratio_for_pressure_ratio`
and its bisection inverse `exit_pressure_ratio`, `thrust_coefficient_vacuum`,
`thrust_coefficient`, `zero_thrust_pressure_pa` + `thrust_fraction` (the
one‑constant form for the ascent loop), `throat_area_m2`, `exit_area_m2`,
`exit_diameter_m`, `expansion_ratio_for_exit_diameter`,
`bell_extension_mass_kg`. Ten tests: the area‑ratio relation inverts;
the §1 gain table within 0.5 %; the six real engines' sea‑level Isp
within 2 % and the zero‑thrust form equal to the full formula; Merlin →
MVac gain in 1.09–1.13; Summerfield ε_SL by chamber pressure within
1 %; MVac / RVac / RL10B‑2 exit diameters within 8 %; the exit‑diameter
limit and cap; MVac's bell mass at 16 kg/m²; `thrust_fraction` edges.
No callers, so the oracle is untouched by construction. 666 tests
pass, clippy clean.

### Step 2 record (2026‑09‑20)

**Model.** `EngineBaseline` now carries `isp_ref_s` (vacuum Isp of the
sea‑level bell), `chamber_pressure_pa` (per cycle × propellant,
`engine_project::chamber_pressure_pa`), `gamma` (per propellant,
`exhaust_gamma`), `vacuum_only`, thrust, mass, power. `EngineDesign`
gained `chamber_pressure_pa`, `expansion_ratio`, `gamma` (all
`#[serde(default)]`); `exit_pressure_pa` is now a derived cache kept for
the UI and the separation roll. Methods: `set_nozzle`,
`set_nozzle_for_exit_pressure`, `throat_area_m2`, `with_vacuum_bell(cfg)`
(ε from the 3 m exit limit and the 300 cap; Isp and thrust × the
thrust‑coefficient ratio; mass + bell area × 20 kg/m²),
`atmosphere_response()`, `isp_fraction_at` (via the response),
`overexpansion_destruction_risk(ambient, cfg)` (ramp from ratio 3.0,
slope 0.2). The K = 0.2 free function `engine::isp_fraction` is gone.

**Variants.** `EngineBaseline::design(.., cfg)` builds the sea‑level bell
at `cfg.sea_level_exit_pressure_pa` (0.4 atm) and, for `vacuum` or a
vacuum‑only family, applies `with_vacuum_bell`. `design_variant(vacuum,
cfg)` is now a transform of the project's *current* design, so Isp
improvements and Isp tech penalties reach the flown engine (the §0 bug).
Fifteen callers take `&balance.nozzle`.

**Ascent.** `AscentNozzle { exit_pressure_pa, thrust_n }` →
`AscentThruster { response: AtmosphereResponse, thrust_n }`
(`nozzle::AtmosphereResponse::{None, Nozzle { zero_thrust_pressure_pa }}`,
the seam toward Option C). Per step: `response.thrust_fraction(p)`, one
divide; the lookup ceiling is where the loss falls under 0.1 %.

**Balance.** New `[nozzle]` section (`NozzleConfig`): sea‑level exit
pressure, max bell exit 3.0 m, ε cap 300, bell 20 kg/m², separation
ramp start 3.0 / slope 0.2. Reference Isp per propellant: kerolox 311,
hydrolox 420, methalox 350, hypergolic 285, solid 285; NTR 785. Cycle
Isp multipliers unchanged except Expander 1.04 → 0.97: its low chamber
pressure makes a poor short bell and its long bell gains the most, so
the hydrolox expander lands at 463 s (RL10). Resulting figures at
scale 1 (pad / SL‑bell vacuum / long bell):

| family | pad | SL bell vac | long bell | thrust gain | mass |
|---|---|---|---|---|---|
| Kerolox GG (starting engine) | 271 | 311 | 334 | +7 % | 1147 → 1266 kg |
| Kerolox staged | 297 | 330 | 349 | +6 % | |
| Hydrolox GG | 365 | 420 | 461 | +10 % | |
| Hydrolox expander (vac only) | — | — | 463 | | |
| Methalox GG | 304 | 350 | 379 | +8 % | |
| Hypergolic pressure‑fed | 195 | 262 | 322 (+100 kg, ε 300) | +23 % | AJ10‑190: 316 s, 118 kg |
| Solid | 221 | 262 | 294 | | Castor 30XL 294 |
| NTR (vac only) | — | — | 852 | | NERVA ~850 |

**Saves.** `save::sanitize` → `backfill_nozzles`: every stored
`EngineDesign` without a nozzle gets one fitted to its stored exit
pressure at its family's chamber pressure (`chamber_pressure_for_legacy`;
kerolox figure of the cycle when the mix names no preset). Legacy
designs keep their stored Isp, so old engines are no better or worse
than they were. Found and fixed on the way: `serde_json`'s default float
parser is not exactly round‑trip, so a freshly computed ε changed by one
ulp on reload and the corpus idempotence test caught it —
`float_roundtrip` is now enabled in `Cargo.toml`.

**Fixtures.** Every `EngineDesign` literal (≈40 across src) gained the
three fields, fitted to its exit pressure at 90 bar (third‑party and
DinoSoar engines at their realistic pressures). `test_util::kerolox_engine`
and friends likewise. Engine tests re‑pinned: Merlin‑like 97 bar ε 16
keeps 0.907 at the pad and exhausts at 66 kPa; the long bell at the pad
is under 0.5 and rolls 100 % separation.

**Gates.** 669 tests, clippy clean, save corpus round‑trips, 200‑seed
bands pass. The bot's upper stage flies a vacuum bell, so the 40‑seed
oracle changed as intended; new baseline `oracle_19_base.{txt,csv}` in
the scratchpad (avg final $196.5M, 2/40 bankrupt, 37/40 ever‑profitable
— the previous baseline file was no longer on disk to diff against).

### Step 3 record (2026‑09‑20)

`ui::draw::format::BellFigures { sea_level: Option<EngineDesign>, vacuum }`
is the one place the pair is built: `of_project(ep, cfg)` (both bells
of a family, one for a vacuum‑only cycle) and `of_engine(e)` (a built
engine: a sea‑level bell reads pad / vacuum of itself, a vacuum bell its
vacuum figure). `isp()` → `271 / 334 s`, `thrust()` → `900 kN / 966 kN`,
`mass()` → `1,147 kg / 1,266 kg`, `geometry()` → `chamber 90 bar · bell
ε 21 / 128`. Used by: the Engines tab's expanded rows (Isp line with a
`(pad / vac)` legend, then a Thrust / Mass line, then Scale /
Auto‑revise), the contracted‑engines rows, the engine editor (baseline
and scaled lines as pairs, plus a hint line with chamber pressure, both
ε and the legend), the rocket designer's Pick Engine list (family pairs
for player designs, single bell for contracted; legend line under the
list) and the third‑party catalogue modal. The stage rows keep their
` SL` / ` vac` / ` vac‑only` bell tag. Three tests pin the exact strings
for kerolox gas generator, hydrolox expander (`463 s`), ion (`3000 s`)
and the two built bells (`271 / 311 s`, `334 s`). Render smoke test
passes at both widths; 672 tests; clippy clean; oracle byte‑identical to
the step‑2 baseline (UI only).

### Step 4 record (2026‑09‑20)

`tests/probes.rs::falcon9::falcon9_yardstick` (ignored; `cargo test
--release --test probes falcon9 -- --ignored --nocapture`): Falcon 9's
tanks, expendable, flown five ways. Merlin 1D / MVac are built through
`set_nozzle` (97 bar; ε 16 / 165), the game families through
`EngineProject::new` + `design_variant`, so the probe exercises the
real code path. Output today (LEO / GTO, real Falcon 9: 22.8 / 8.3 t):

| rocket | LEO | GTO | 19th (K model, old bells) |
|---|---|---|---|
| A: Merlin, real dry masses | 17.7 t | 5.6 t | 18.8 / 6.1 |
| B: Merlin, game structure | 14.1 t | 2.9 t | 15.2 / 3.5 |
| C: game kerolox GG family, game structure | 10.7 t | 0.8 t | 7.3 / 0 |
| C: game kerolox staged, game structure | 13.7 t | 1.9 t | — |
| C: game kerolox full‑flow, game structure | 13.8 t | 1.6 t | — |
| D: game kerolox GG family, real dry masses | 15.3 t | 4.2 t | 11.1 / 1.8 |

Reading it: the starting kerolox rocket went from 7.3 t and no GTO to
10.7 t and a GTO capability, all from the nozzle model (right Isp for
the sea‑level bell in vacuum, a real vacuum bell). Row A lost a tonne
because Merlin's pad penalty is now the real 9 %, not 4 %. The rest of
the gap to Falcon 9 is the structural model (A → B: 3.6 t LEO, 2.7 t
GTO) and the ascent's missing Earth‑rotation credit (A vs real), both
for the follow‑up plan named in §6.

### Editor revision (2026‑09‑20, after playtest)

USER: the engine editor has room for separate Sea Level and Vacuum
lines where "Scaled" was; no baseline line, no "scaled" label, no note
that the nozzle is chosen per stage.

CLAUDE: Done. The editor's stats block is now one line per bell,
aligned in columns — `Sea level:  276 s pad · 317 s vac    900 kN
1,147 kg   ε 21` over `Vacuum:     340 s vac    966 kN    1,266 kg
ε 128` — then a hint with the chamber pressure (and power draw when
there is one). A vacuum‑only family has just the Vacuum line. The
baseline line and both Nozzle notes are gone. The editor also gets its
own 72‑column box (`engine_editor_area`): the default modal is 60 % of
the screen, 48 columns at 80, which was clipping these lines and had
been clipping the old nozzle note too. `BellFigures::geometry` went
unused and was removed. 672 tests, clippy clean, smoke test at both
widths.

## 7. Questions

1. Option B with the `AtmosphereResponse` seam, or straight to A for
   speed, or C now?
   USER:  Lets do B now with the intention of going to C soon.
   CLAUDE: Agreed. B lands with the `AscentResponse` prepare/step split
   from §3 so the enum has its seam ready; the C plan then lifts the
   predicates into `Propulsion` with the oracle byte‑identical.
2. Chamber pressure per cycle only (table in §5), or per cycle ×
   propellant? Per‑cycle is enough for the physics; per‑propellant
   would let solids and hypergolics differ from kerolox pressure‑fed.
   USER:  Per cycle and per propellant I think.
   CLAUDE: Done; §5 is now the full 21‑row table with real anchors.
3. Vacuum expansion ratio: a fixed per‑cycle default (§5), or a
   player‑editable field in the engine editor from the start (an
   `EditorField::ExpansionRatio` alongside Scale)? I lean fixed now,
   editable later as an improvement or tech unlock.
   USER:  Definetly fixed.
   CLAUDE: Fixed, and derived rather than tabulated: from the 3 m
   exit‑diameter limit and the ε cap (§2, reply under §5). Scaling an
   engine up lowers its vacuum ε, which is the one way a player moves
   it for now.
4. Should tech deficiencies get a `ChamberPressurePenalty` kind now,
   or keep the existing Isp/mass/thrust kinds and add it when the
   technology audit on your TODO happens?
   USER: This'll go into the technology audit happening soon.
   CLAUDE: Noted; no new deficiency kind in this plan. The chamber
   pressure field will be there for the audit to hang one on.
5. Baseline Isp expressed as "vacuum Isp of the sea‑level bell" (§5
   proposal) or as `c*`?
   USER:  Ideally it would be something like A / B where A is the ground level
   ISP of a sea-level bell and B is the vacuum ISP of a vacuum bell.  At least
   for how its presented in the engine designer.  Just the vac for vac only
   engines like expander cycles.
   CLAUDE: That is the presentation, and §5 now specifies it: `A / B
   s` with A the sea‑level bell at the pad and B the vacuum bell in
   vacuum, `B s` alone for vacuum‑only cycles (Expander is already
   `vacuum_only` in `engine_baseline`, matching your remark). Under
   the hood the baseline keeps "vacuum Isp of the sea‑level bell" as
   its reference so the numbers in `engine_baseline` stay the
   familiar 311 / 440 / 350; A and B are derived from it.
