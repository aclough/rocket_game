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


### Step 3 record (measured after `simulate_launch` judges on `total_planner_dv`)

Same 200 seeds × 8 years. The whole shift is the leniency removal: a
variant that fixed only the double overexpansion charge and kept the
old gravity-blind check reproduced the baseline to the launch
(95.0%, 8 bankrupt).

| Metric | Baseline | Step 3 |
|---|---|---|
| Bankrupt | 8/200 | 13/200 |
| Dip below $0 | 16/200 | 27/200 |
| Ever profitable | 196/200 | 189/200 |
| End above starting money | 91/200 | 44/200 |
| Keep min money above $25M | 163/200 | 131/200 |
| Launches | 2,608 | 2,839 |
| Aggregate launch success | 94.9% | 87.7% |
| Worst surviving seed | 73% | 62% |
| Avg final money | $191.6M | $139.8M |
| First-launch month / dev spend / hidden flaws | 16.8 / $71.1M / 8.3 | unchanged |

**Why it moves this much.** The old check compared the degraded design's
vacuum Δv against the route, so every launch carried a hidden 8–11%
allowance (the ascent's gravity loss). The bot's template bids at zero
margin on every market — LEO capability 2,820 kg against contracts of
500–5,000 kg, GEO 380 kg against 200–3,000 — so a flaw that costs one
stage 5–15% of its Δv now lands as a shortfall. Over 30 seeds × 6 years
the outcome mix is 258 success / 22 partial (<5% short) / 19 failure,
and 16 of the 19 failures are degradation or engine-loss with no stage
loss.

**Your option 2 (a bidding Δv margin) was tried and does not work for
this bot.** Implemented as `Company::bid_dv_margin` (a per-company knob
the rule engine applies in `player_capable_cost`), and swept together
with a template-size factor *k* on both stages' propellant and
structure:

| Template ×k | Bid margin | Launches | Success | Bankrupt | Min > $25M |
|---|---|---|---|---|---|
| 1.00 | 0 (step 3 alone) | 2,839 | 87.7% | 13 | 131 |
| 1.00 | 10% | 1,909 | 84.4% | 29 | 104 |
| 1.00 | 15% | 1,456 | 81.5% | 45 | — |
| 1.00 | 20% | 1,217 | 80.0% | 51 | — |
| 1.15 | 0 | 3,224 | 88.7% | 29 | 105 |
| 1.15 | 10% | 2,338 | 86.6% | 43 | 93 |
| 1.30 | 0 | 3,217 | 88.4% | 33 | 100 |
| 1.30 | 10% | 2,408 | 86.3% | 41 | 89 |

Every cell is worse than step 3 alone, on every metric. Two effects:
the bot's income is the edge-of-capability contracts, so a margin gate
starves it (salaries continue, launches fall by a third); and fewer
launches means flaws stay hidden longer, so the per-launch success
rate *falls* rather than rises. A bigger template just moves the edge
to heavier, dearer payloads and fails there instead. The gate is not a
percentage that "roughly works"; there isn't one for a fixed-template
zero-margin bidder.

**What landed.** Step 3 itself (launch.rs, plus the
`overexpansion_is_charged_once_across_launch_and_first_leg` test); the
bidding knob, its cache and the gate were removed again per Q5.
`sim_bands.rs` is recalibrated to the step 3 numbers, marked
provisional until step 6 re-measures.

**Q5 — what to do about the shift.** Options:

1. **Accept step 3 as measured** and drop the knob (delete
   `bid_dv_margin`, `GameState::dv_margin` and the gate — about forty
   lines). The bot is a novice who flies at zero margin and now pays
   for it; the game is honest about what a flaw costs. Bands stay at
   the provisional numbers. My recommendation.
2. **Keep the knob as a player-facing bidding rule** ("only bid where I
   have ≥ N% spare Δv") with the bot leaving it at 0. It is a data
   structure change I made without discussing it first, which is why
   I am asking rather than keeping it quietly. Saves gain one field
   (`serde(default)`, no migration).
3. **Teach the bot to size its template to the market** — a real
   design loop (pick stage sizes so the biggest contract it wants has
   margin *m*), not a fixed template. Bigger job, belongs in a policy
   plan of its own, not in this one.

USER: The increase in failures is within what can be accepted, lets go
with (1).

CLAUDE: Done — knob, cache and gate removed; step 3 is launch.rs, its
test, the bands and this record.

### Step 4 record (measured after the planner charges overexpansion)

`GroupPerformance::planner_dv` = vacuum − gravity − overexpansion;
`effective_dv` = that − drag. The bot's first-stage nozzle loses 1.6%
of its vacuum Δv at sea level (177 m/s of 11,256 at a 1 t payload), and
the capabilities move with it (`tests/capability_probe.rs`, kept as an
ignored probe for later steps):

| Design | LEO | GTO | GEO | Lunar orbit |
|---|---|---|---|---|
| Bot template, before | 2,820 kg | 1,016 | 380 | 363 |
| Bot template, after | 2,640 kg (−6%) | 931 (−8%) | 318 (−16%) | 302 (−17%) |
| Corpus m1–m4 rocket (early-era booster) | 703 → 645 kg | 0 | 0 | 0 |
| Corpus m5 rocket | same as the bot template | | | |

The smallsat class still lifts to LEO (Q2's line), so the physics is
accepted as recommended. The sim moved more than the capabilities did:

| Metric | Step 3 | Step 4 |
|---|---|---|
| Launches | 2,839 | 2,450 |
| Aggregate launch success | 87.7% | 85.9% |
| Bankrupt | 13/200 | 24/200 |
| Dip below $0 | 27/200 | 37/200 |
| Ever profitable | 189/200 | 184/200 |
| Keep min money above $25M | 131/200 | 122/200 |
| Avg final money | $139.8M | $106.5M |
| Unit cost / payment | $15.7M / $30.5M | $16.0M / $31.1M |

The mechanism is the one the step 3 sweep exposed: the bot's income is
the contracts at the edge of its capability, and a 6–17% smaller edge
takes a seventh of its launches away while salaries continue. Fewer
launches also leave flaws hidden longer, so the success rate drifts
down rather than up. The bankruptcy rate is now 12%, three times the
roguelike target of 2–4% and above the provisional 8% ceiling set at
step 3; the 20-seed smoke check also fails (18/20 seeds ever
profitable against a 92% floor).

**Q6 — the step 4 gate.** The plan named the choices in advance:
retune in balance data, or ship step 3 without 4. A third has appeared.

1. **Accept step 4 as measured and recalibrate the bands again**
   (bankrupt ceiling 14% at 200 seeds, ever-profitable floor 88%,
   aggregate success 82%). Cheapest, and step 6 will move them once
   more anyway. But it codifies a 12% death rate for the bot until
   someone gives it a better template, and every later balance change
   is measured against that.
2. **Retune balance data so the bot's opening capability comes back**:
   the engine presets' exit pressures (the bot's booster nozzle sits
   at ~40 kPa; a preset nearer 60 kPa would halve the penalty) or the
   booster's Isp baseline. This restores roughly the step 3 numbers
   without touching code, at the cost of quietly changing every
   engine in the game to pay for one bot.
3. **Charge overexpansion the way the atmosphere actually thins**:
   both the flight's first burn and the planner charge the *whole*
   first-stage burn at sea-level ambient, which over-penalises a
   90-second booster that spends most of its burn above 10 km. An
   ambient-averaged fraction along the ascent profile is a physics
   change to `burn_group` and `compute` together (a D-section item),
   and would shrink the penalty for every design, not just the bot's.
   Right long-term, but it is a new plan item, not a step of this one.

My recommendation: (1) now so this step lands and step 6 can be
measured on top of it, and open (3) as a D item for the physics pass,
since the double charge this plan removed and the whole-burn ambient
are the same kind of error.

USER: Lets try 3 but check if it slows down the calculations

CLAUDE: Done as step 4b below; the timing is in its record. The bands
did not need to move.

### Step 4b record (overexpansion charged as the air thins)

**Design.** The ascent integrator already tracks altitude, so it now
carries the atmosphere too:

- `SurfaceProperties::scale_height_m` (Earth 8,500 m, Mars 11,100 m,
  Venus at the 1-bar level 7,500 m, 0 for airless bodies) and
  `pressure_at(altitude)` = ambient × e^(−h/H). A new column in the
  static location table, not a save-format change.
- `AscentPhase` carries the nozzles firing (`AscentNozzle { exit_pressure_pa,
  thrust_n }`); at each step the thrust is the vacuum figure times the
  thrust-weighted `engine::isp_fraction` at the local pressure (the one
  formula, extracted from `EngineDesign::isp_fraction_at`), and that
  fraction weighted by the propellant burned at it becomes the group's
  `AscentGroupResult::isp_fraction`. Propellant burned after orbital
  velocity counts as vacuum. `simulate_ascent` takes the
  `SurfaceProperties` and returns `AscentGroupResult { gravity_loss,
  isp_fraction, burnout_altitude_m }` per group; `simulate_gravity_losses`
  stays as the airless wrapper the tests use.
- `DesignPerformance::compute` reads the fraction from the same
  integration it already ran for gravity, so the planner's
  `overexpansion_loss` is `vacuum × (1 − fraction)`;
  `AscentLosses::isp_fraction_by_group` exposes it.
- The flight uses the same figure: `Rocket::burn_sequential(design,
  dv, from)` takes the departure location instead of a pressure, runs
  `compute` for the flying design and payload when `from` has an
  atmosphere, and `burn_group` scales exhaust velocity by the group's
  fraction. `FlightLeg::ambient_pressure_pa` is gone — `from` was
  already there, and the flow-separation roll reads pad pressure from
  the surface table. Old saves load (the field is ignored).
- The launch test now asserts pad fraction < flown fraction < 1 and
  that the first burn's implied exhaust velocity is nominal × the flown
  fraction.

**What the first attempt found.** Charging every group by the
integrator's altitude *raised* the bot's penalty from 177 to 929 m/s:
the pure gravity turn has the TWR-1.22 template flying level at 11.4 km
when the first stage burns out, so the vacuum bell on the upper stage
was billed for 26 kPa of air it will never see (the m1 corpus rocket,
TWR 1.64, stages at 91 km and its upper stage climbs to 926 km). The
integrator's altitude is not to be trusted for staging, so
`design_ascent` leaves upper groups' nozzles out and only the first
group is charged — the assumption the old `burn_sequential` made in
words ("upper stages fire at high altitude"), now in one place with
the reason. The trajectory model itself is filed as 17_REFACTOR.md D7.

**Capabilities** (`tests/capability_probe.rs`):

| Design | LEO | GTO | GEO | Lunar orbit |
|---|---|---|---|---|
| Bot template, step 3 | 2,820 kg | 1,016 | 380 | 363 |
| Bot template, step 4 (whole burn at pad pressure) | 2,640 | 931 | 318 | 302 |
| Bot template, step 4b | 2,752 (−2%) | 992 (−2%) | 362 (−5%) | 345 (−5%) |
| m1–m4 corpus rocket | 703 → 740 kg | 0 | 0 | 0 |

The bot's 80 kPa booster nozzle is under-expanded above 2 km, so its
penalty falls from 177 m/s (4.2% of the stage) to 29 m/s (0.7%); the
first stage's gravity loss rises 669 → 702 m/s because its thrust is
now the sea-level figure while it is low. The early-era corpus rocket
gains capability for the same reason.

**Sim**, 200 seeds × 8 years:

| Metric | Step 3 | Step 4 | Step 4b |
|---|---|---|---|
| Launches | 2,839 | 2,450 | 2,708 |
| Aggregate launch success | 87.7% | 85.9% | 86.7% |
| Bankrupt | 13/200 | 24/200 | 14/200 |
| Dip below $0 | 27/200 | 37/200 | 31/200 |
| Ever profitable | 189/200 | 184/200 | 188/200 |
| End above starting money | 44/200 | — | 32/200 |
| Keep min money above $25M | 131/200 | 122/200 | 129/200 |
| Avg final money | $139.8M | $106.5M | $123.2M |

Within the provisional step 3 bands (20 and 200 seeds), so `sim_bands.rs`
keeps its numbers; its header records the step. The oracle differs on
1,551 of 2,896 rows, as a behaviour change should.

**Timing** (release, bot template; `compute_timing` in the probe file):

| | Before 4b | 4b, every step looks up pressure | 4b as landed |
|---|---|---|---|
| `DesignPerformance::compute` | 26.6 µs | 51.5 µs | 28–33 µs |
| `max_payload_to` | 2.85 ms | 3.15 ms | 2.82–2.85 ms |
| 200-seed × 8-year simulate | 6.1–6.6 s | 6.4 s | 6.6–6.8 s |

The `exp` per integration step doubled `compute`; the ceiling altitude
above which every nozzle firing is matched or under-expanded (2 km for
the bot's booster) skips the lookup for the rest of the climb and
brings it back to within noise of before. The sim wall-clock is
dominated by other work and moved by less than run-to-run scatter.

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
| 3 ✅ (bands provisional until 6, see record) | `simulate_launch` judges on `total_planner_dv()` of the degraded design; drop the Isp/thrust scaling (fixes #4 if step 0 confirmed it). | **launch outcomes for flawed vehicles; first-leg propellant** | **measure**: launch success %, bankruptcies, first-launch month |
| 4 ✅ | Planner charges overexpansion (`planner_dv()` as written above). | **capabilities drop for sea-level-nozzle first stages** | **measure**: plus a capability probe — `max_payload_to` for the bot's template and each corpus save's rocket to LEO/GTO/GEO before and after |
| 4b ✅ | Overexpansion charged as the air thins: `SurfaceProperties::scale_height_m`, nozzles in the ascent integration, first group only; flight and planner read the same `isp_fraction_by_group` (Q6 → option 3). | **capabilities recover most of step 4's drop; first-stage gravity loss rises slightly** | **measure** + timing |
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
   CLAUDE: Doing so.
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
