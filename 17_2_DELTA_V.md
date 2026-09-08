# Delta-v Unification Plan (A4)

> Subsidiary to `17_REFACTOR.md` item A4. Written against the tree after
> A6 (`calendar.rs` O(1) dates). Nothing here is implemented. Add
> `USER:` comments inline; I'll reply as `CLAUDE:`.
>
> Your steer: we want this, but check the sim impact and performance,
> and remember reachability can be shown in fewer places if the draw
> path gets slow.

---

## 0. The problem in one paragraph

Four places answer "how much delta-v does this vehicle have, and how
much does the trip cost", and they disagree about which losses count.
The planner decides feasibility one way, the designer displays a margin
a different way, the launch simulation passes or fails a flight a third
way, and the flight itself burns propellant a fourth way. None of them
is wrong on its own; together they let the designer show a healthy
margin on a vehicle the planner is judging as barely reachable, and let
a flaw-degraded vehicle pass a launch check the planner would have
failed. One `DesignPerformance` computed once per (design, payload,
origin) and read by all four is the fix.

---

## 1. The four models today

| | Where | Available Δv | Route cost | Gravity | Drag | Overexpansion |
|---|---|---|---|---|---|---|
| **(a) Vacuum total** | `RocketDesign::total_delta_v` (`rocket.rs:306`) | Σ group Tsiolkovsky | — | no | no | no |
| **(b) Planner** | `path_planning.rs` `full_group_dv` (67) + `edge_cost_for_class` (79) | per group: vacuum − gravity loss | edge Δv | yes, off the group | yes, on atmospheric edges | **no** |
| **(c) Designer stats** | `compute_stage_stats` (`rocket.rs:996`) `delta_v_effective` | vacuum − gravity − aero − overexpansion, per group | — | yes | yes, on group 0 | yes, group 0, flow-weighted Isp fraction |
| **(d) Flight** | `Rocket::burn_sequential` / `burn_group` (`rocket.rs:~540, 617`) | Tsiolkovsky on remaining propellant | `leg.delta_v_cost` = edge Δv + drag (`flight.rs` `build_route_for_rocket`) | **no** | yes, in the leg cost | yes, as `isp_fraction_at(ambient)` on the first burned group's exhaust velocity |

Gravity loss itself has one source, `group_gravity_losses`
(`rocket.rs:954`), which the doc comment rightly calls "the *only* place
the ascent profile is integrated". Both (b) and (c) call it. The
disagreement is not about the numbers; it is about which of them each
consumer adds up.

### Who reads which

| Consumer | Uses | Reads |
|---|---|---|
| Contract colours, bid engine, payload table, campaign liftability | `payload_capability` → `max_payload_to` (`rocket_project.rs`) | (b) via `shortest_path_for_rocket`, bisected over payload |
| Designer mission line "Req Δv / Avail / Margin" (`draw.rs:1893-1944`) | `plan_mission` for Req, `total_delta_v` for Avail | Req = (b), **Avail = (a)** |
| Designer "UNREACHABLE — Δv shortfall: need ≥ X, have Y" | `plan_mission` fallback | need = mass-only Dijkstra (edge Δv + drag), **have = (a)** |
| Designer stage table "Eff dV" / "Vac dV" and "Total dV" (`draw.rs:1963-2175`) | `compute_stage_stats` | (c) |
| Rockets tab "dV: N m/s (0 payload)" (`draw.rs:694`) | `total_delta_v(0.0)` | (a) |
| Launch pass/fail (`launch.rs:300-420`) | `required_dv` from `shortest_path_for_rocket`; `degraded_dv` = `degraded.total_delta_v` after scaling group 0's Isp *and thrust* by `isp_fraction_at(101325)` | required = (b), **available = (a) + a hand-rolled overexpansion** |
| Route and per-leg costs (`flight_ops.rs` `launch_rocket`, `fly_spacecraft`) | `shortest_path_for_rocket` + `build_route_for_rocket` | (b) route, (d) costs |
| Stranding checks (`flight_ops.rs` `tick_flight`) | `remaining_delta_v` (vacuum) vs 0.5 × remaining route Δv; `dv_achieved` vs 0.95 × leg | (a) on the instance vs (d) |
| Fly-spacecraft planner (`shortest_path_for_rocket_state`) | remaining group Δv − gravity from the *current* surface | (b) |

---

## 2. What goes wrong

1. **Designer margin is inflated by the gravity loss.** Req is the
   route the planner found; the planner's own feasibility test was
   (vacuum − gravity) ≥ route, but Avail shows vacuum. A vehicle the
   planner judged reachable by 50 m/s shows "Margin: +850" if its
   ascent costs 800 m/s of gravity loss. The colour thresholds
   (`< 0` red, `< 500` yellow) are applied to the wrong number.

2. **The shortfall message can report a negative shortfall.**
   `plan_mission`'s fallback compares vacuum total (a) against the
   mass-only route. When the stage-aware search fails on gravity, the
   line reads "need ≥ 9,300, have 9,400 (short -100 m/s)".

3. **Launch is more lenient than the planner.** With no flaws, a
   planner-reachable vehicle passes (vacuum ≥ vacuum − gravity ≥
   route). With flaws, `degraded_dv` (a) is checked against the route
   with gravity ignored, so a flaw that erodes less than the gravity
   loss never fails a launch. Since flaws are the game's central
   mechanic, this is the consequential one for balance: the bands in
   `tests/sim_bands.rs` (aggregate launch success 92.9%) were measured
   with this leniency in place.

4. **Overexpansion is probably charged twice on the first leg.**
   `simulate_launch` scales group 0's `isp_s` and `thrust_n` by
   `isp_fraction_at(ambient)` and hands that `degraded_design` to the
   flight. The first leg then calls `burn_sequential(…, ambient)`,
   whose `burn_group` multiplies each stage's thrust by
   `isp_fraction_at(ambient)` again. `mass_flow_rate()` is thrust over
   Isp·g₀, so scaling both leaves flow unchanged and the exhaust
   velocity comes out as frac² × nominal. **To be confirmed by a test
   in step 1 before anything is changed** — if real, it is a
   longstanding bias against sea-level nozzles that the unification
   will remove, and that alone will move the sim bands.

5. **Flight never pays gravity.** The route cost the flight burns is
   edge Δv plus drag; the ascent's gravity loss, which the planner
   charged when deciding the trip was possible, is never consumed in
   flight. So a vehicle arrives with more propellant than the planner
   expected, and the stranding thresholds (0.5 of remaining route,
   0.95 of a leg) are looser than they look. Whether to fix this is a
   design question (Q3): it changes where and when vehicles strand.

6. **The planner does not charge overexpansion.** The designer's "Eff
   dV" subtracts it; the planner that colours contracts white does not.
   A sea-level-nozzle first stage gets credit in the contract list for
   Δv the stats table says it doesn't have.

---

## 3. Target design

```rust
// src/rocket.rs (or a new src/performance.rs)

/// One stage group's delta-v budget for a specific mission start.
pub struct GroupPerformance {
    pub delta_v_vacuum: f64,
    pub gravity_loss: f64,        // from group_gravity_losses
    pub aero_drag_loss: f64,      // group 0 from a surface with atmosphere, else 0
    pub overexpansion_loss: f64,  // group 0 at surface ambient, flow-weighted
    pub twr: f64,
    pub burn_time_s: f64,
    pub mass_ratio: f64,
}

impl GroupPerformance {
    /// What the planner may spend from this group: vacuum less the
    /// losses it pays on the way up, but *not* drag — the graph charges
    /// drag per atmospheric edge, against the mass actually flown.
    pub fn planner_dv(&self) -> f64 {
        (self.delta_v_vacuum - self.gravity_loss - self.overexpansion_loss).max(0.0)
    }
    /// What the stats table shows: everything charged to the group.
    pub fn effective_dv(&self) -> f64 {
        (self.planner_dv() - self.aero_drag_loss).max(0.0)
    }
}

/// A design's performance for a mission from `launch_from` with
/// `payload_kg` aboard. Computed once, read by the planner, the
/// designer, the launch simulation and the payload search.
pub struct DesignPerformance {
    pub groups: Vec<GroupPerformance>,
}

impl DesignPerformance {
    pub fn compute(design: &RocketDesign, payload_kg: f64, launch_from: &str) -> Self;
    pub fn total_vacuum_dv(&self) -> f64;
    pub fn total_planner_dv(&self) -> f64;   // Σ planner_dv — the "Avail" the planner means
    pub fn total_effective_dv(&self) -> f64; // Σ effective_dv — the stats table's total
}
```

USER:  One thing about GroupPerformance, we need to handle cases where there are
two stages in the group with different burn times.  Think Soyuz or Space Shuttle
boosters.  They might be very different.
CLAUDE: Agreed, and today three of the four models get this wrong while
the fourth gets it right. `phased_parallel_delta_v` (`rocket.rs:365`)
already walks the burn in phases — all stages fire, the shortest tank
empties, that stage drops off, repeat — so the *vacuum* Δv of a
Shuttle-like group is correct. But:

- `group_gravity_losses` hands the integrator one lumped
  `(thrust, flow, propellant, dry)` per group, so the ascent is
  integrated as if the boosters burned as long as the core at the core's
  total thrust. Gravity loss is understated for exactly the designs you
  name (high early thrust, long low-thrust tail).
- `compute_stage_stats` reports one TWR and one burn time per group from
  the same lump (burn time = total propellant / total flow, which is
  neither the booster's nor the core's).
- In flight, `burn_group` (`rocket.rs:617`) spends propellant across the
  group in proportion to mass flow and clamps a tank at zero when its
  share exceeds what it holds, so the deficit quietly disappears; and
  `burn_sequential` only jettisons a *whole group* once the group is
  dry, so empty boosters ride along as dead mass until the core is
  spent. Their dry mass is only fictitious for power and housekeeping
  (later groups' Δv is unaffected since the whole group is gone by
  then), but the propellant accounting is wrong for asymmetric groups.

So the primitive underneath `GroupPerformance` has to be the phase, not
the group — see §3.1, added below. The bot's template is two sequential
single-stage groups, so the oracle will not see this; the
`test_core_plus_srbs_phased_burnout` test in `rocket.rs` is the anchor,
and step 1b adds a gravity-loss counterpart.

### 3.1 The phase is the primitive

```rust
/// One interval of a group's burn during which the same stages fire.
/// A Shuttle-style group has two: core + boosters, then core alone.
pub struct BurnPhase {
    pub active: Vec<usize>,      // stage indices firing
    pub thrust_n: f64,
    pub mass_flow_kg_s: f64,
    pub duration_s: f64,
    pub mass_start_kg: f64,      // everything above + group wet at phase start
    pub mass_end_kg: f64,
    /// Stages that empty at the end of this phase and drop off.
    pub jettisoned: Vec<usize>,
}

impl RocketDesign {
    /// The group's burn as phases; `phased_parallel_delta_v` becomes
    /// `burn_phases(..).iter().map(|p| p.delta_v()).sum()`.
    pub fn burn_phases(&self, gi: usize, payload_above_kg: f64) -> Vec<BurnPhase>;
}
```

Then:
- `GroupPerformance.delta_v_vacuum` = Σ phase Δv (unchanged numbers;
  this is what the phased function already computes).
- `simulate_gravity_losses` takes the phases of every group, flattened,
  instead of one lumped tuple per group: thrust and flow step at each
  booster burnout and the dropped dry mass leaves the integrated mass.
  Gravity loss for asymmetric groups changes (goes up); symmetric and
  single-stage groups are bit-identical, which the oracle confirms.
- `GroupPerformance.twr` is at ignition (phase 0); `burn_time_s` is the
  sum of phase durations (the longest tank). The designer's per-stage
  "Burn" column already uses `Stage::burn_time_s` and stays.
- `Rocket::burn_group` walks the same phases on the instance's
  remaining propellant: spend phase by phase, `jettison_stage` each
  stage as its tank empties, stop when the target Δv is met. This
  replaces the proportional split and its clamp, and empty boosters
  leave the vehicle when they are empty. `BurnResult` gains
  `stages_jettisoned: Vec<(usize, usize)>`; `groups_jettisoned` stays
  for the callers that only care about groups.

- `compute_stage_stats` becomes `DesignPerformance::compute(..).groups`
  (`StageGroupStats` = `GroupPerformance`; keep the alias).
- `full_group_dv(design, gi, payload, gravity_losses)` becomes
  `perf.groups[gi].planner_dv()`. `shortest_path_for_rocket` computes
  one `DesignPerformance` instead of one gravity-loss vector; the
  per-group figure the search spills into is read from it.
- `simulate_launch` computes `DesignPerformance` on the *degraded*
  design and passes when `total_planner_dv() ≥ required_dv`. The
  hand-rolled Isp/thrust scaling goes; overexpansion is a loss in the
  budget, and the flight charges it once, in `burn_group`, as today.
- Designer: Avail = `total_planner_dv()`; the shortfall fallback's
  "have" likewise; the stats table keeps "Eff dV" = `effective_dv()`
  and "Vac dV" = vacuum. Rockets tab "dV" = `total_planner_dv()` at
  zero payload, labelled "usable".
- `RocketDesign::total_delta_v` survives as the vacuum figure (it is
  what `Rocket::remaining_delta_v` mirrors for an instance), but no
  feasibility decision reads it any more. Consider renaming to
  `vacuum_delta_v` so a future reader can't mistake it for usable.
USER:  A useful invariant about our travel graph is that a journey can begin
with a first leg using gravity and drag losses but every edge after that will be
a vacuum transition without either (until we add EDL).  I'm not so familiar with
all the current code paths as to know if that implies a change in design but do
think about it.
CLAUDE: The graph honours it today: `add_ground_pair` (`location.rs:250`)
is the only builder that sets `through_atmosphere`, and only on the
surface → orbit direction of a body with an atmosphere; the descent
direction gets `can_aerobrake` instead. Gravity loss is likewise only
ever integrated for the ascent (`simulate_gravity_losses` stops at
orbital velocity, and origins that aren't surfaces get zero). So the
three loss terms — gravity, drag, overexpansion — are all *departure*
phenomena of one leg, and the invariant says exactly that.

It does sharpen the design, in two ways (§3.2 below):

1. `DesignPerformance` should be shaped as *pure vacuum groups plus one
   ascent budget*, not as per-group loss columns that happen to be zero
   past group 0. An `AscentLosses` value computed from
   `(design, payload, launch_from)` is what the planner subtracts and
   what the first surface leg consumes; everything after it is
   Tsiolkovsky. Departing from orbit (a parked spacecraft flying on)
   has an empty ascent budget by construction.
2. Drag can stay charged on the one atmospheric edge, as now — that is
   already the flight's mechanism and it fires exactly once per the
   invariant — but the invariant becomes a *tested* property:
   `only_surface_ascents_are_atmospheric` over the whole graph, so
   adding a body later can't silently create a second atmospheric leg.
   When EDL arrives, descent gets its own budget (`DescentLosses`),
   symmetric to this one, rather than a flag on an edge.
- `Transfer::total_delta_v(mass)` (edge Δv + drag) shares a name with
  the design method and means something else; rename to
  `Transfer::cost_for_mass` in passing.

### 3.2 One ascent budget, then vacuum

Per your invariant, the losses are properties of the departure, not of
the groups, so the struct is better read as:

```rust
pub struct AscentLosses {
    /// Gravity loss charged to each group that burns during the
    /// ascent — group 0, and any later group that ignites before
    /// orbital velocity. Zero for a non-surface origin.
    pub gravity_by_group: Vec<f64>,
    /// Flow-weighted sea-level Isp penalty on the first group; zero
    /// without an atmosphere.
    pub overexpansion: f64,
    /// Drag, for display; the planner charges it on the ascent edge.
    pub drag: f64,
}

pub struct DesignPerformance {
    pub groups: Vec<GroupPerformance>,   // vacuum figures from phases
    pub ascent: AscentLosses,            // the one non-vacuum leg
}
```

`GroupPerformance::planner_dv` / `effective_dv` are then thin views
over `groups[gi]` and `ascent`. A tested invariant guards the shape:
`only_surface_ascents_are_atmospheric` walks every `Transfer` and
asserts `through_atmosphere` implies `from` is a surface with an
atmosphere and `to` is its orbit. EDL later adds a `DescentLosses`
mirror rather than reusing the edge flag.

The one gravity integration stays where it is (now fed phases). The
planner's edge-drag charge stays where it is. What changes is that every
"how much have I got" question is answered by one struct.

---

## 4. Steps and gates

### Step 0 record (measured before any change)

200 seeds × 8 years, basic policy, default balance:

| Metric | Baseline |
|---|---|
| Bankrupt | 8/200 |
| Dip below $0 and recover | 16/200 |
| Ever profitable | 196/200 |
| Aggregate launch success | 94.9% (2,476 of 2,608) |
| Avg first-launch month | 16.8 |
| Dev spend to first launch | $71.1M |
| Hidden flaws at first launch | 8.3 |
| Unit cost / payment | $15.9M / $32.4M |
| Avg final money | $191.6M |

Bot template (seed 42, day 730) `max_payload_to` from Earth: LEO
2,820 kg · GTO 1,016 · GEO 380 · lunar orbit 363.

**Double overexpansion charge confirmed.** With a 40 kPa-exit nozzle at
sea level, `isp_fraction_at` = 0.8790; `simulate_launch` hands the
flight a design whose group 0 Isp is already ×0.8790, and the first
leg's burn comes out at ×0.7726 of nominal exhaust velocity = 0.8790².
Step 3 removes the pre-scaling.

**Seventh symptom, found in step 1:** the planner has no solar-sail
handling. `Stage::delta_v` gives a sail 0 m/s (no propellant), so a
sail design plans as unreachable everywhere, while a *flying* sail
(`Rocket::group_remaining_delta_v`) is ∞ and a parked sail craft can
plan anywhere. Not touched by this plan; recorded so it isn't
mistaken for a regression later.


Every step: `cargo test`, clippy, the 40-seed simulate oracle, and for
the steps marked **measure**, the 200-seed band run
(`cargo run --release --bin simulate -- --seeds 1..200 --years 8 --policy basic --summary-only`)
compared against the baseline recorded first.

| # | Step | Behaviour | Gate |
|---|---|---|---|
| 0 ✅ | Record the 200-seed baseline summary to `docs/` (or the plan) and write the **double-overexpansion test**: launch a sea-level-nozzle design with no flaws, fly the first leg, and assert the propellant consumed matches a single `isp_fraction_at` penalty. Expect it to fail today. | none | test documents the bug either way |
| 1 ✅ | `DesignPerformance` + `AscentLosses` + `compute_stage_stats` as a view of it; planner reads `planner_dv()` **with `overexpansion` set to zero** for now, so (b) is unchanged. `only_surface_ascents_are_atmospheric` test. | none | oracle identical |
| 1b ✅ | `BurnPhase` / `burn_phases`; `phased_parallel_delta_v` becomes a fold over it; the gravity integrator takes phases; `burn_group` walks phases and jettisons per stage. Add a gravity-loss test for a core + boosters group (loss must exceed the lumped figure) beside `test_core_plus_srbs_phased_burnout`. | **asymmetric groups only**: gravity loss and in-flight jettison | oracle identical (the bot's groups are symmetric); the new test pins the asymmetric case |
| 2 ✅ | Designer Avail, shortfall "have", Rockets tab dV → `total_planner_dv()`. The negative-shortfall case is its own `MissionPlan::StagingInfeasible`. | UI only | oracle identical |
| 3 | `simulate_launch` judges on `total_planner_dv()` of the degraded design; drop the Isp/thrust scaling (fixes #4 if step 0 confirmed it). | **launch outcomes for flawed vehicles; first-leg propellant** | **measure**: launch success %, bankruptcies, first-launch month |
| 4 | Planner charges overexpansion (`planner_dv()` as written above). | **capabilities drop for sea-level-nozzle first stages** | **measure**: plus a capability probe — `max_payload_to` for the bot's template and each corpus save's rocket to LEO/GTO/GEO before and after |
| 5 | Update `sim_bands.rs` to the re-measured reality in the same commit as whichever of 3/4 moved it, per CLAUDE.md. Update pinned Δv numbers in `rocket.rs`, `path_planning.rs`, `launch.rs` tests. | — | bands green |

Steps 1 and 2 are pure and can land immediately. Steps 3 and 4 each
get their own commit and measurement so the effect of each is known
separately. If step 4 moves capabilities more than we like, the
choices are: retune (balance TOML, not code), or ship step 3 without 4
and record the planner/stats disagreement as deliberate. My
recommendation is to take step 4: "Eff dV" already tells the player
that number, and a contract list that promises what the stats table
denies is the worse experience.

| 6 | Flight pays the ascent: `build_route_for_rocket` adds `ascent.gravity_by_group` to the first leg's `dv_cost` when the origin is a surface (and `fly_spacecraft` from a surface likewise), so what reaches orbit is what the planner budgeted and the stranding thresholds mean what they say. | **gameplay**: less propellant in orbit, strandings become possible for marginal designs | **measure** |

Your call on Q3 puts step 6 in scope. It lands after 3 and 4 so its
effect is measured on its own.

---

## 5. Performance

`DesignPerformance::compute` costs one gravity integration
(0.25 s steps over each group's burn, ~600–2,000 steps) plus a handful
of sums — the same work `shortest_path_for_rocket` already does per
call, so the planner and payload bisection (40 searches per
destination) are unchanged. The designer currently integrates twice per
frame (once inside `plan_mission`, once in `compute_stage_stats`); step
1 makes that once. Net: neutral to slightly faster.

The per-frame cost that matters is elsewhere: `payload_table` in the
designer and Rockets tab runs the bisection per destination, cached by
fingerprint. Step 4 changes no caching. If it ever does get slow, your
fallback stands: draw the payload table only for the selected row, and
only on a fingerprint change.

---

## 6. Risks

- **Bands move.** Certain for step 3 if #4 is real (sea-level stages
  arrive with more propellant), likely for step 4 (capabilities drop).
  The measurement protocol is the mitigation; the bands file's header
  already says to re-measure and update in the same change.
- **Test pins.** `rocket.rs` stats tests (1552–1640),
  `path_planning.rs` route tests (578–721), `launch.rs` outcome tests,
  `ui/mod.rs` autosize tests (4051, 4088) assert specific Δv figures.
  Step 1 must leave them; steps 3/4 update them with the reason in the
  test.
- **Save compatibility.** None of this touches serialized state.
- **Bot behaviour.** `BasicPolicy` sizes and bids through
  `project_can_serve`, so it follows the planner automatically; its
  template may need a scale tweak if step 4 takes its LEO capability
  below the smallsat class (`test_basic_policy_template_lifts_smallsats_to_leo`).

---

## 7. Questions for you

1. **Order of 3 and 4.** Land step 3 (launch judged the planner's way)
   first and measure, then step 4 (planner charges overexpansion) and
   measure — or reverse, so the planner is fully honest before the
   launch check is aligned to it?
   CLAUDE (recommendation, still open): 3 then 4. Step 3 removes a
   leniency and a probable double charge; measuring it alone tells us
   how much of the band shift is the bug fix. Step 4 is then a pure
   "planner now agrees with the stats table" change.
   USER:  Ok, 3 then 4 then.
2. **If step 4 drops capabilities noticeably**, retune in balance data
   (engine baselines / exit pressures) to restore roughly today's
   capabilities, or accept the new numbers as the corrected physics
   and recalibrate the bands only?
   CLAUDE (recommendation, still open): accept the physics and
   recalibrate the bands, retuning only if the bot's template can no
   longer lift the smallsat class to LEO — that one test is the line
   between "numbers moved" and "the opening is broken".
   USER:  Yes, I don't expect it to be that large a change though.
3. **Flight gravity (5b).** Should the flight consume the ascent
   gravity loss on the first surface leg, so orbit arrival matches the
   planner's budget? It makes strandings meaningful but is a gameplay
   change beyond A4's scope.
   USER:  For things like this we shoudl be willing to change gameplay a bit.
   CLAUDE: Then it is in scope — step 6 below. The first leg from a
   surface burns edge Δv + drag + the ascent gravity loss, so what
   reaches orbit is what the planner budgeted, and the stranding
   thresholds mean what they say. Measured with the band run like
   steps 3 and 4.
4. **Naming.** Rename `RocketDesign::total_delta_v` →
   `vacuum_delta_v` and `Transfer::total_delta_v` → `cost_for_mass` as
   part of step 1, or leave names alone?
   CLAUDE (recommendation, still open): rename both in step 1. After
   this plan nothing should reach for a bare total without saying
   which one it means.
   USER:  Lets do the rename then.
