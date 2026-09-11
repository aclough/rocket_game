# 17.3 — Physics and hardware model (D1–D7)

Sub-plan of `17_REFACTOR.md` section D. Same cadence as the rest of the
pass: one commit per step, approval before each, `cargo test`, clippy and
the 40-seed simulate oracle diffed byte-for-byte against a build of the
previous commit. Steps 1–5 are refactors and must leave the oracle
identical; step 6 (the trajectory model) is a physics change and is
gated on the 200-seed band run like the delta-v plan's steps 3–6.

## 0. Where D stands after E

The E splits moved things the D list pointed at, so the map first:

| Item | State today |
|---|---|
| D1 attached-stage walks | 40 `.attached` sites in 7 files (`rocket/power.rs`, `rocket/staging.rs`, `flight.rs`, `path_planning.rs`, `launch.rs`, `ui/draw/tabs.rs`, `game_state/flight_ops.rs`) |
| D2 power duplication | `rocket/power.rs`: `RocketDesign::{total_power_supply_w, total_housekeeping_w, total_battery_kwd}` and `Rocket::{total_power_supply_w, total_housekeeping_w, total_battery_capacity_kwd}` are the same bodies with an `attached` filter; `stage_source_supply_w` is the one shared piece |
| D3 location | data already in `location/graph_data.rs` (E8). Still: `location()`, `transfer()`, `transfers_from()` are linear scans over `&'static str`; `sun_distance_au` parses `_transfer`/`_escape` suffixes; two Dijkstras in `location/mod.rs` (`shortest_path`, `shortest_path_constrained`) plus the A* in `path_planning.rs`, each with its own heap-state struct |
| D4 mass flow | `Stage::mass_flow_kg_s()` exists; 4 `mass_flow_rate() * count` sites remain; `simulate_gravity_losses` still takes the positional 4-tuple (tests only) |
| D5 sizing physics | `ui/designer.rs` (E1): `autosize_propellant`, `dry_mass_for`, `recompute_structural_masses`, `propellant_step`, `SizingTarget`, the TWR targets, ~200 lines of tests. Pure functions over `Stage` |
| D6 constants | as listed in `17_REFACTOR.md`; `BalanceConfig` has seven sections (costs, work, markets, flaws, reputation, competitor, engine_materials); saves carry the balance (`GameState::balance`, old saves load defaults); `simulate` takes `--balance`, `main` does not |
| D7 trajectory model | new (found in delta-v step 4b): pure gravity turn from a 1° kick at 45 m/s; TWR-1.22 vehicle level at 11 km at staging, TWR-1.64 at 91 km |

## 1. Proposed steps

### Step 1 — D4 + D1: one way to walk the vehicle

- Finish D4: the four remaining `mass_flow_rate() * engine_count` sites
  call `Stage::mass_flow_kg_s()`.
- `Rocket::attached_stages(&self, design) -> impl Iterator<Item = (gi, si, &Stage, &StageState)>`
  and the two sums every caller wants: `attached_mass_kg(design)` and
  `mass_above_group(design, gi)` (dry + remaining propellant of the
  attached stages above `gi`, plus payload).
- Replace the hand-rolled walks: the three "payload above" closures in
  `rocket/staging.rs`, the six in `rocket/power.rs`, `flight.rs`'s two,
  and the random-attached-stage picks in `flight_ops.rs` and
  `advance.rs`. `remaining_delta_v` becomes a fold over
  `group_remaining_delta_v`.
- Gate: oracle identical. Summation order is preserved by iterating
  groups then stages in index order, which is what every site does now.

### Step 2 — D2: design totals are instance totals with everything attached

- `Stage::battery_capacity_kwd()` and `Stage::source_supply_w(src, sun_au)`
  (the latter is today's free `stage_source_supply_w`, moved onto the
  stage).
- One private `power_totals(stages: impl Iterator<Item = &Stage>, sun_au)`
  that both `RocketDesign` and `Rocket` call — the design passes every
  stage, the rocket passes `attached_stages`. `total_battery_kwd` /
  `total_battery_capacity_kwd` become one name.
- `power.rs` module doc stops saying fuel cells and reactors are stubs.
- Gate: oracle identical. Risk: the two copies sum in the same order
  today; if the shared fold changes a float's last bit the oracle will
  say so and the fix is to keep the order, not to accept the drift.

### Step 3 — D5: sizing physics leaves the UI

- New `src/rocket_sizing.rs`: `SizingTarget`, `TARGET_LIFTOFF_TWR`,
  `TARGET_STAGE_TWR`, `LOW_THRUST_DV_TARGET`, `MIN_/MAX_AUTOSIZED_PROPELLANT`,
  `PROPELLANT_STEP_BURN_SECONDS`, `dry_mass_for`, `autosize_propellant`,
  `propellant_step`, `recompute_structural_masses`, `is_solid_engine`,
  and the `autosize_tests`. Pure functions over `Stage` / `Vec<Vec<Stage>>`.
- `resize_all_tanks`, `resize_tanks_through`, `rename_all_stages`,
  `apply_picked_engine_to_designer` stay in `ui/designer.rs` — they
  mutate `RocketDesignerState`.
- Gate: oracle identical (the bot doesn't use the sizer today; a later
  policy could, which is the point).

### Step 4 — D3: the graph gets an index

- `DeltaVMap` builds, in the constructor, `index: HashMap<&'static str, usize>`
  and `adjacency: Vec<Vec<usize>>` (transfer indices per origin, in
  insertion order). `location()`, `transfer()`, `transfers_from()`
  become O(1) / O(degree) and keep their signatures.
- `Location::sun_distance_au` becomes a field the `loc_*` builders set
  (heliocentric nodes get their body's distance), and the suffix
  parsing goes.
- One Dijkstra: `shortest_path` and `shortest_path_constrained` share a
  body parameterised by an edge-cost closure; the heap-state struct is
  shared with `path_planning.rs` (`location::SearchState` or similar).
- A* heuristic cached per goal on the map (it depends only on the
  graph and the goal), so `max_payload_to`'s ~40 searches per
  destination stop recomputing it.
- Not in this step: a `LocationId(usize)` newtype or removing
  `Rocket.location`. Both touch save formats and every struct that
  holds a location string; they go on the list for a later pass.
- Gate: oracle identical — ties between equal-cost paths must resolve
  as before, which insertion-order adjacency guarantees. Measure with
  `compute_timing` (today: `compute` 28 µs, `max_payload_to` 2.8 ms).

### Step 5 — D6: constants into `BalanceConfig`

Three commits, most-tunable first, each with the test assertions that
pin the old literals rewritten against the config in the same edit:

1. **`flight` and `geopolitics`.** Stranding cut (remaining < 0.5 ×
   route), short-burn cut (achieved < 0.95 × leg), partial-failure cut
   (0.95 of required), partial payment (0.5) → `FlightConfig`. The
   eight `geopolitics.rs` consts → `GeopoliticsConfig`. These are what
   a sweep would actually vary.
2. **`economy` and `technology`.** The `economy.rs` transition table,
   the tech unlock chance (`market_ops.rs`), the testing-level
   thresholds, the improvement magnitudes in `engine_project.rs` /
   `reactor_project.rs`, the `technology.rs` difficulty tables,
   `BID_PAYLOAD_MARGIN` (→ `markets`).
3. **`physics` and `costs.power`.** Overexpansion slope, destruction
   curve, housekeeping W/kg, drag model, spiral penalty, the
   `structure.rs` consts, and the panel / fuel-cell / RTG / battery
   mass and cost curves in `power.rs` (which `new_reactor` already
   half-does via `CostsConfig`).

Plus `main.rs` gets the same `--balance <file>` (repeatable) handling as
`simulate.rs`. Saves: every new section is `#[serde(default)]`, so the
corpus loads with today's numbers. Gate: oracle identical for all three
(defaults equal the literals).

### Step 6 — D7: an ascent that stages where launchers stage

The one physics change. Its own record and measurement, and probably
its own document once the approach is picked, because the bands and
the bot's template will move. Three candidate models, cheapest first:

- (a) **Kick-over velocity scales with TWR** — keep the pure gravity
  turn but start it later for sluggish vehicles, so a TWR-1.2 stack
  climbs before it bends. One parameter becomes a function of one
  number; the Falcon 9 anchor test (`gravity_loss_matches_a_real_launch_vehicle`,
  1.5–1.7 km/s) stays the pin.
- (b) **A pitch program with an altitude target at staging** (say
  60–70 km for the first group), pitch rate limited so the model can't
  go horizontal below it. More parameters, more realistic upper-stage
  losses, and the overexpansion charge could then trust the altitude
  and go back to every group.
- (c) **Lofted trajectory by TWR class** — a table of pitch profiles.
  Most data, least physics; not recommended.

Gate: 200-seed run against the step 4b/6 record in `17_2_DELTA_V.md`
§4, capability probe before/after, `sim_bands.rs` recalibrated in the
same commit, and the bot's 60 t first stage re-sized if its
near-horizontal handover no longer exists.

### Step 6 record — D7, done first (your call on Q1)

**Models measured.** `AscentProfile` was added to the integrator with
all three candidates selectable, and a probe (`ascent_models` in
`tests/capability_probe.rs`) flew the Falcon 9 anchor vehicle, the bot's
template (liftoff TWR 1.22) and the m1 corpus rocket (TWR 1.64) under
each:

| Profile | Falcon 9: staging / S2 loss / total | Bot template: staging / S1+S2 loss / LEO | m1 rocket: staging / S1+S2 loss / LEO |
|---|---|---|---|
| Pure gravity turn (before) | 82 km / 437 / **1,571** | **11 km** / 702+0 / 2,752 kg | 91 km / 1,125+**1,856** / 740 kg |
| (a) kick scaled by TWR, k=1 | unchanged | 44 km / 1,059+13 / 2,368 | 90 km / 1,108+1,718 / 817 |
| (a) k=2 | 71 km / 238 / 1,286 | 75 km / 1,304+301 / 1,891 | 84 km / 1,058+1,362 / 999 |
| (a) k=4 | 63 km / 134 / 1,114 | 113 km / 1,628+1,822 / 793 | 71 km / 941+726 / 1,468 |
| (b) pitch program 65 km, 25°, p=0.5 | 79 km / 279 / 1,414 | 80 km / 1,356+328 / 1,848 | 69 km / 962+385 / 1,679 |
| (b) 65 km, 28°, p=0.5 | 82 km / 351 / 1,511 | 84 km / 1,382+437 / 1,760 | 71 km / 980+502 / 1,589 |
| **(b) 65 km, 30°, p=0.5 — chosen** | 84 km / 407 / **1,582** | 86 km / 1,400+512 / 1,689 | 72 km / 991+581 / 1,522 |

(a) is out: where a pure gravity turn ends up is a steep function of
the kick, so no exponent makes both a TWR-1.2 and a TWR-1.6 vehicle
sane at once (k=2 fixes the bot and leaves m1's upper stage at 690 km;
k=4 fixes m1 and lofts the bot to 950 km). The Falcon 9 anchor moved
too, because the fixture's TWR is 1.455, not 1.4, and a 3 m/s earlier
kick is worth 300 m/s of gravity loss in that model. (b) stages every
vehicle at 65–95 km whatever its TWR and pays gravity in proportion to
how long the climb takes, which is the behaviour the model was missing.
30° at 65 km puts the anchor mid-band (1,582 of 1,500–1,700); 25° and
28° fall short of it.

**What changed in code.**
- `location/ascent.rs`: `AscentProfile { GravityTurn, PitchProgram }`,
  `DEFAULT_ASCENT_PROFILE` (pitch program, 45 m/s kick, end pitch 30°
  where the air thins to 50 Pa, exponent 0.5), `simulate_ascent_with(profile, …)`; `simulate_ascent`
  flies the default. The gravity turn is kept as the reference the
  tests compare against.
- `rocket/stats.rs`: every group is charged the sea-level Isp penalty
  again (the 4b "first group only" workaround is gone) — the staging
  altitude can now be trusted, and at 65–95 km the charge is zero.
- Fixtures that only reached LEO through the level-at-11-km handover:
  the launch tests' engine goes 1 → 2.5 MN (it had liftoff TWR 0.96);
  the planner's spanning test gives its 600 t upper stage 6 MN instead
  of 1.5 (TWR 0.24 at 30° pitch spends its burn falling).
- `policy.rs`: the bot's template is re-sized — see next.

**The bot's template.** Under the new model the 60 t single-engine
first stage lifts 1,689 kg to LEO instead of 2,752 (the handover it was
sized for was the fiction). Adding propellant to one engine only makes
it slower and worse (105 t: 1,371 kg). Two booster engines fix the TWR:

| S1 engines / S1 prop / S2 prop | LEO | GEO |
|---|---|---|
| 1 / 60 t / 8 t (old) | 1,689 kg | 15 kg |
| 2 / 90 t / 12 t | 2,986 | 185 |
| 2 / 90 t / 15 t — **chosen** | 3,412 | 196 |
| 2 / 110 t / 12 t | 3,152 | 242 |
| 2 / 130 t / 15 t | 3,647 | 279 |

Chosen for the 200-seed result below (closest launch count and success
to the step 4b record). GEO does not come back — the upper stage now
pays 460 m/s of real gravity on the way up — so the bot bids fewer GEO
contracts than it used to. Structural masses scale with the tanks
(6,450 kg and 1,500 kg).

**Sim**, 200 seeds × 8 years, default balance:

| | Step 4b/6 record | D7, old template | D7, chosen template |
|---|---|---|---|
| Launches | 2,708 | 1,491 | 2,657 |
| Aggregate success | 86.7% | 81.4% | 87.1% |
| Bankrupt | 14/200 | 84/200 | **47/200** |
| Dip below $0 | 31 | 100 | 82 |
| Keep min > $25M | 129 | 70 | 89 |
| Ever profitable | 188 | 135 | 155 |
| Avg final money | $123.2M | $27.9M | $132.5M |
| First launch / dev spend / unit cost / payment | m16.8 / $71.1M / $15.8M / $30.6M | m16.8 / $71.1M / $16.9M / $32.4M | m18.7 / $81.9M / $18.8M / $36.3M |

**Why bankruptcies triple with capability restored.** The per-launch
economics are unchanged — unit cost is still ~50% of the payment — but
a vehicle that really lifts 3 t costs $19M instead of $16M, takes two
more months and $11M more to develop, and every early failure in the
flaw-discovery phase burns $19M. The bankrupt seeds all die at 7–9
launches with 56–78% success, right after a first launch in month 19;
the same seeds under the old physics had reached 12–22 launches. The
game's early-game runway was balanced against a rocket that was cheaper
than physics allows.

Starting money is the lever that runway responds to (`[costs]
starting_money` in a `--balance` file, nothing else changed):

| Starting money | D7 bankrupt | Success | Launches | Ever profitable | (old physics, same money) |
|---|---|---|---|---|---|
| $200M | 47/200 | 87.1% | 2,657 | 155 | 14/200 |
| $250M | 20/200 | 88.9% | 2,921 | 183 | 4/200 |
| $300M | 4/200 | 89.2% | 3,031 | 197 | — |

**Q6 — the balance after D7.** The physics is right and the bot's
template is honest; what is left is a gameplay call, so I have stopped
here with the bands failing (20-seed check: bankruptcies) and nothing
committed. Options:

1. **Accept 23% and recalibrate the bands** to the D7 numbers. Cheapest;
   codifies a death rate five times the roguelike target and makes
   every later balance change measure against it.
2. **Raise starting money in the default balance** to $250M (10%
   bankrupt, about the step 3 rate) or $300M (2%, inside the 2–4%
   target), recalibrating bands to whichever. The honest compensation:
   the early game got harder for a human for the same reason — a real
   rocket costs more — and the purse is the knob that scales with it.
   $250M is my recommendation: it lands where the game was before the
   delta-v plan started moving things, and leaves the roguelike edge.
3. **Other data levers**: unit cost (engine materials) or payment rates
   down/up ~15%. Changes the per-launch economics for everyone rather
   than the runway; a bigger sweep, and D6 would want those knobs in the
   config first.

Whichever you pick lands in the D7 commit with the bands; the
`RT_ASCENT` sweep override in `default_ascent_profile` comes out at the
same time.

USER:  For this case I would definetly agree with the $250 million option.
However thinking about it more an altitude target for staging has a lot of
problems.  What if we have a Space shuttle style rocket with a big hyrolox stage
with a couple of boosters that's just 1 group with a fairly short first segment?
Rather than TWR we might try scaling with detla-v?

CLAUDE: $250M it is (default `costs.starting_money`, bands below).

On the altitude: the program does not stage at 65 km — nothing in it
knows about stages. It is a pitch *schedule*: the commanded pitch is a
function of altitude (vertical at the kick, 30° above the horizon by
65 km, a gravity turn after), and a vehicle stages wherever its
propellant runs out along that schedule. I renamed the parameter from
`target_altitude` to `schedule_altitude` because the old name said
otherwise. Measured on the vehicles you mention (`ascent_models` probe,
Shuttle as one group of core + two SRBs with the SRBs dropping 124 s in,
PSLV as four stages):

| Vehicle | Pure gravity turn (pre-D7) | Pitch program, altitude-keyed (chosen) | Velocity-keyed, 2.5 km/s → 25° | Velocity-keyed, 2.5 km/s → 30° |
|---|---|---|---|---|
| Falcon 9: total loss / MECO | 1,571 / 82 km | **1,582** / 84 km | 1,502 / 85 km | 1,711 / 91 km |
| Shuttle: total loss / MECO | 3,178 / **862 km** | 1,388 / 184 km | 1,553 / 252 km | 1,902 / 392 km |
| PSLV: total loss / S1, S2, S4 burnout | 5,607 / 39, 353, **2,848 km** | 1,363 / 34, 162, 322 km | 1,836 / 32, 191, 665 km | 2,375 / 33, 220, 1,022 km |

So staging heights do differ by vehicle under the chosen program —
PSLV's first stage at 34 km, Falcon 9's at 84, the Shuttle's MECO at
184 — because they come out of the vehicle, not the schedule. (The
Shuttle lofts a little: real MECO is ~110 km; its core after SRB
separation has a TWR of 0.87, and the gravity turn from 30° carries it
higher. Real Shuttles flew a flatter program for exactly that reason.)

I also tried the schedule you suggested, keyed to delta-v gained
instead of altitude (two right-hand columns). It is the weaker law: a
slow vehicle stays steep until it is fast, which is the lofting the old
model suffered from — the Shuttle to 250–390 km, PSLV's last stage to
650–1,000 km — and the Falcon 9 anchor only fits at the very edge of
its band. Pitching by altitude is what real guidance approximates in
the first minutes (pitch-over is scheduled by time-since-liftoff and
altitude, and the gravity turn takes over once out of the thick air),
so the altitude-keyed schedule stays. If a design later wants its own
program — a flat one for a Shuttle, a steep one for a Saturn — the
`AscentProfile` is per-ascent already and could become per-design.

**Other bodies (your last question).** The schedule's altitude is not a
constant but the altitude where the body's air thins to
`PITCH_SCHEDULE_PRESSURE_PA` (50 Pa): 65 km on Earth, 28 km on Mars,
57 km at Venus's 1-bar level, and on an airless body the program
completes at the kick — the vehicle goes to 30° a few seconds off the
pad and flies a gravity turn, which is how the LM flew. Measured
(`ascent_models` probe; LM ascent stage off the Moon, a 100 kN Mars
ascent vehicle):

| Vehicle | Pure gravity turn | Program at a fixed 65 km | Program by pressure (landed) |
|---|---|---|---|
| Falcon 9 (Earth): loss / MECO | 1,571 / 82 km | 1,582 / 84 km | 1,578 / 84 km |
| LM ascent (Moon): loss / burnout | 672 / 284 km | 457 / 147 km | **49 / 1 km** |
| MAV (Mars): loss / burnout | 1,249 / 316 km | 975 / 195 km | **646 / 66 km** |

The lunar figure is optimistic (the real LM budgeted ~200 m/s of losses,
much of it the vertical rise to clear terrain, which the model has no
notion of) but is the right order; the fixed-altitude schedule had a
lunar ascent climbing to 147 km first. Earth is unchanged to the
launch: the 200-seed run differs from the fixed-65 km one by one
launch (2,916 vs 2,915), inside the bands.

**Landed** (default balance now `starting_money = $250M`):

| | Step 4b/6 record ($200M) | D7 ($250M) |
|---|---|---|
| Launches | 2,708 | 2,916 |
| Aggregate success | 86.7% | 88.2% |
| Bankrupt | 14/200 | 23/200 |
| Dip below $0 | 31 | 34 |
| Keep min > $25M | 129 | 146 |
| End above starting money | 32 | 65 |
| Ever profitable | 188 | 179 |
| Avg final money | $123.2M | $189.5M |

`sim_bands.rs` recalibrated to these (bankruptcy ceiling 15% at 200
seeds, 25% at 20; per-seed success floor back up to 65%). The 20-seed
smoke run: 1 bankrupt, 89.1% aggregate. `ascent_models` and
`capability_probe` stay as ignored probes. The `AscentProfile` keeps
the pre-D7 gravity turn as `LEGACY_GRAVITY_TURN` for the comparison
probe; the velocity-keyed candidate was measured (table above) and not
kept.

## 2. Order and why

1 → 2 → 3 → 4 → 5 → 6. Steps 1–3 are small and make 4–6 easier to read
(the power fold is what D2 touches, the sizer is what a smarter bot
would call in D7's re-sizing). Step 4 is the only performance-relevant
one and is worth doing before D7 makes the integrator more expensive.
Step 5 before 6 so the trajectory model's new parameters land in a
`physics` section that already exists. Step 6 last because it is the
only one that moves the game.

## 3. Questions for you

1. **Order.** As above, or D7 first because it is the one you'd feel
   as a player? My recommendation is last: the refactors are quick and
   D7 will want the `physics` config section and the sizer.
   USER:  Lets do D7 first.

2. **D6 scope.** The list flags the physics constants (overexpansion
   slope, drag model, power curves) as design decisions rather than
   tuning knobs. Move them into the config anyway for one source of
   truth, or leave them as `const`s and do only commits 1 and 2?
   USER:  Leave them as constants.  We only want thing there we intend to
   adjust for balance reasons.

3. **D7 model.** (a), (b), or measure both and pick? (b) is the one
   that lets the overexpansion charge trust the altitude for every
   group again; (a) is a one-line change to a model that is already
   anchored to a real vehicle.
   USER:  Lets try (a) and (b).  Definetly not (c)

4. **D3 depth.** Index + one Dijkstra + sun-distance field now, the
   `LocationId` newtype later — or is the newtype worth doing while the
   file is open, given it touches saves (`Rocket.location`,
   `Flight.current_location`, `FlightLeg.from/to`, spacecraft)?
   USER:  I'm waffling but I think lets do the LocationId now.

5. **D5 home.** `src/rocket_sizing.rs`, or into `stage.rs` next to the
   mass helpers it uses?
   USER:  stage.rs I think
