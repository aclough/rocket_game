# 22 — Flaws rework

Roadmap item: *"Rework and rebalance flaws. Look for new categories of
maluses and make sure that each description makes sense with the
engines it's paired with. Make things a bit harder so that we expect a
failure on most first launches. Look at early launches of Falcon 1,
Electron, Firefly Alpha, Astra Rocket 1."*

Agreed in conversation (2026-10-10):

- **Target curve at bot-level effort:** ~35 % first flight, ~60 %
  second, settling near 90 %. A company that keeps many engineering
  teams on a design for long enough should be able to buy ~98 %, even
  though no scrappy start-up can afford to.
- **Cost stays abstract.** No hot-fire line items; reliability is bought
  with engineering-team months. Raising engineering team salary is a
  lever to explore, not a commitment.
- **Built units track their flaws**, with UI affordances that keep it
  from being tedious. Part of the answer is players learning not to
  stockpile.
- **In scope:** per-company deficiency retirement (TODO), third-party
  parts with unfixable flaws, the flat 10 % modification-flaw chance,
  new malus categories (roadmap), descriptions that fit the engine
  they're on (roadmap), the power-draw engine flaw (roadmap hygiene).
- **The bot may do worse at first.** When the rework is done, a
  bot-prudence sweep decides how the bot copes and the bands are
  re-baselined. Expect the game to get harder here.
- **Difficulty setting: deferred** to the next roadmap item.

---

## 1. What the real record says

Full survey with per-failure causes and sources:
[`docs/launch_survey_2026.md`](docs/launch_survey_2026.md). Guidance,
not a binding target.

**First attempts.** Non-Chinese start-ups reached orbit first time in
3 of 13 vehicles (23 %); counting only each company's *first* vehicle,
2 of 12 (17 %). Chinese start-ups managed 60 %, almost entirely with
small solid vehicles on state solid-motor heritage; their liquid-engine
debuts split 3 for 3. Established players' new vehicles: 56 %.

**The curve**, pooled over 28 start-up vehicles:

| Ordinal | Flights | Full success |
|---|---|---|
| 1st | 28 | 43 % |
| 2nd | 17 | 71 % |
| 3rd | 15 | 67 % |
| 4th–10th | 64 | 83 % |
| 11th+ | 815 | 98.9 % (94 % without Falcon 9) |

**What fails**, start-ups, 45 in-flight failures and partials:

| Category | Flights 1–3 (26) | Flights 4+ (19) |
|---|---|---|
| Feed / plumbing / pressurisation | 9 | 2 |
| Ground / range / FTS | 4 | 0 |
| Engine internal | 3 | 5 |
| Staging / separation | 3 | 2 |
| Upper-stage ignition / relight | 2 | 0 |
| Electrical / harness / avionics | 2 | 4 |
| GNC / software | 1 | 1 |
| Structures / tanks | 1 | 1 |
| Undisclosed | 1 | 4 |

**Third-party parts** caused 2 start-up failures (CRS-7 strut, Electron
F1 ground telemetry) and 2 established ones (Antares NK-33, Angara
Persei). **Modifications** clearly preceded 3–4 of ~50 failures
(Falcon 1 F3, Astra LV0006, Persei, Amos-6); most failures hit
unchanged hardware.

What this says for the model:

- The game's current curve (58 / 58 / 86 / 95) is close to the pooled
  real one. The pooled figure is flattered by Chinese solids. A player
  designing their own liquid engines is in the Western column, where
  first flights succeed one time in four. The 35 % target sits between
  the two, which is right for a game.
- **Plumbing, not engines, kills early flights.** Engine-internal
  failures are 3 of 26 early losses: test stands work. The big early
  class is valves, lines, leaks and pressurisation on the integrated
  vehicle. So the `Integrated` exposure carries the most weight for
  rocket flaws, with staging / ignition / GNC in `Analysis` as the
  second class, not the first.
- **The long tail is electrical and manufacturing.** Late failures are
  harnesses, connectors, arcs, and unit defects (a bad screw hole on
  Ceres-1, a bad strut on CRS-7, a bad SRB insulator on Vulcan). That
  supports keeping low-activation flaws that are hard to find, and
  suggests a per-*unit* defect class on built hardware (§3.11).
- **Ground and range** account for 15 % of early losses and none later.
  A `PadAbort` consequence and a few ground-worded `Analysis` entries
  ("range terminates on corrupted telemetry") cover it.
- **Third-party and modification failures are rare.** Keep third-party
  flaws few and unfixable; a vendor-fix mechanic is not worth a step.
  Modification flaws stay modest.
- **Mature vehicles sit at 94–99 %.** The game's 95 % plateau is right
  where it is; what must change is how many flights it takes to get
  there and how much the player's effort moves it.

---

## 2. Where the game is now

Measured on `simulate --policy basic`, 100 seeds × 6 years, at
`4b1ec69`:

| Launch ordinal | Success |
|---|---|
| 1st | 58 % |
| 2nd | 58 % |
| 3rd | 86 % |
| 4th and later | 95 % |

Bot launches with ~7.4 undiscovered flaws, 10/100 seeds bankrupt.
(An earlier reading of 52 / 72 / 72 was derived from monthly CSV
totals, which can't order launches within a month; the figures above
come from the per-launch tally added in step 1.)

The second flight being no safer than the first is the built-unit
problem of §3.5 in miniature: the bot keeps one rocket on the shelf,
so the second vehicle was integrated before the first flight's flaw
was fixed and carries the same `rocket_flaws` snapshot. Only the
third flight flies the revised design.

USER:  I'd actually like a better sense of how long the bots keep refining the
engines and rocket before launching, how many 30 day test cycles they do and how
many flaws they find.

CLAUDE: Measured with a new probe, `tests/probes.rs::testing_cadence`
(`cargo test --release --test probes testing_cadence -- --ignored
--nocapture`), 20 seeds, state at the first launch:

| Project | Months since design done | Test cycles | Flaws at design | Fixed | Known, unfixed | Still hidden |
|---|---|---|---|---|---|---|
| Booster engine | 14.6 | 12.0 | 6.2 | 4.3 | 0.6 | 1.4 |
| Upper engine | 9.8 | 6.4 | 7.5 | 3.3 | 0.8 | 3.4 |
| Rocket | 7.8 | 5.9 | 4.9 | 1.9 | 0.7 | 2.3 |

(Engine rows averaged from the probe's per-seed output; the booster is
designed first and tests longest.)

So the bot's "decision" to launch is no decision: it launches the day
the first rocket is built, 7–8 months after the rocket design
completes, and that is simply how long one rocket takes to build. The
booster has had ~11 cycles, the upper engine ~6, the rocket ~6, and
testing keeps going at one team each (`√teams` work rate). About
seven flaws are still hidden and one or two are known but mid-revision
on launch day. Discovery per cycle is near the model's mean of ⅙: six
cycles find roughly half of a design's flaws.

How the model produces that:

- **Count.** `generate_flaws` draws N(complexity, 1.5) per design.
  Engines: effective complexity 6–9. Rockets: `rocket_complexity`
  3–8 (stages, engine variety, boosters). Both use the same
  probability core (`roll_flaw_core`).
- **Activation** is `U²` (mean ⅓, median ¼). **Discovery per testing
  cycle** is `U² · √activation` (mean ≈ ⅙): a flaw with a 25 % chance
  of firing is found in ~6 cycles of 30 team-days; a 1 % flaw is
  nearly unfindable and nearly harmless. Every domain discovers at the
  same rate — a staging flaw is as findable on the test stand as an
  injector flaw.
- **Consequences:** 50 % degradation (3–15 %), 35 % engine/part loss,
  15 % stage loss. Engine flaws scale with engine count
  (`1-(1-p)^n`). A flight that fires a flaw always discovers it, and
  only it.
- **Flights teach nothing else.** A clean flight leaves every hidden
  flaw exactly as hidden as before.
- **Built units are inconsistent.** An integrated rocket snapshots
  `rocket_flaws` at integration (`InventoryRocket.rocket_flaws`) and
  launches with that snapshot, so a fix on the design never reaches
  the shelf. Engine flaws are read live from the engine project
  (`engine_flaw_table`), so an engine fix reaches every built unit for
  free. Inventory engines also carry a `flaws` snapshot that nothing
  reads. No screen shows a built rocket's flaws — which is how the
  "ullage motor" report happened.
- **Deficiencies are world-global.** `TechDeficiency.solved` lives on
  the world's `Technology`; a new design inherits *every* deficiency,
  solved or not, and the ⚗ cue shows for any project on the tech.
- **Third-party engines** get `complexity/8` (≥1) flaws from the same
  core; their flaws cannot be revised.
- **Modification** of a rocket rolls one new flaw at a flat 10 %,
  whatever changed.
- **DinoSoar** flies a single synthetic flaw whose activation is its
  failure rate. Untouched by this plan.

Why the curve is wrong in the way it is: the first flight is a coin
flip because ~7 flaws at mean activation ⅓ nearly always fire
something, and half of those are degradations the margins absorb. The
curve then matures in two flights because every flaw that fires is
found and auto-revised, and what's left is the long tail that neither
fires nor is found. There is no class of flaw that ground testing
*can't* reach, so a company that tests enough is simply safe, and the
difference between a careful and a reckless first launch is only how
many testing cycles the bot happened to run.

---

## 3. Design

### 3.1 Flaws have an exposure

Add `FlawExposure` to `Flaw`:

| Exposure | Found by | Discovery exponent N | Mean of U^N | Typical flaws |
|---|---|---|---|---|
| `Bench` | component test stand | 2 (today's) | 0.33 | injector, pump, chamber, igniter, valve timing |
| `Integrated` | full-up stage test, wet dress | 3 (step 3; 5 proposed) | 0.25 | feed lines, pogo, harness, pressurisation, engine bay thermal |
| `Analysis` | simulation, review, or flight | 4 (step 3; 9 proposed) | 0.20 | staging, fairing jettison, upper-stage ignition, slosh/GNC, max-Q structure |

Discovery stays `U^N · √activation`; the exposure picks N
(`FlawsConfig::discovery_exponent_{bench,integrated,analysis}`,
replacing the single `flaw_discovery_exponent`). To let N vary after
generation — the flight review in §3.2 lowers it for a while — the
flaw stores its raw uniform draw (`discovery_roll`) instead of the
finished probability, and the probability is computed when a cycle
rolls. Old saves recover the draw as `(p / √activation)^(1/2)`.

Which exposure a flaw gets is decided by its **description entry**, not
rolled: every pool entry carries its exposure, so the words and the
findability agree ("Separation pyrotechnics fire out of sequence" is
never found on the stand). Pool sampling is weighted by an exposure
mix per domain (config; the picker draws the exposure first, then an
entry of that exposure and consequence). Starting mixes, from the
survey's early-flight causes: engines ~70 % `Bench`, 25 %
`Integrated`, 5 % `Analysis`; rockets ~15 % `Bench`, 55 %
`Integrated` (feed, pressurisation, harness), 30 % `Analysis`
(staging, ignition, GNC, range).

A high `Analysis` N is what makes the "ULA path" exist: several times
the mean testing finds them, with a long tail that takes far longer,
and that costs the team-months. Step 3 landed at 2 / 3 / 4 because the
proposed 2 / 5 / 9 put the bot at 39 % bankrupt before the flight
review (step 5) exists to find analysis flaws; step 7's sweep decides
how far apart the classes end up once it does.

USER:  This is what the `U² · √activation` discovery chance is meant to do.
Having different categories being easier or harder to discover can be good
though.  In general we can just change the exponent rather than stacking
modifiers.

CLAUDE: Agreed; rewritten above as one exponent per class, no
multipliers. The starting N values are placeholders for step 7's
sweep.

### 3.2 Flights are tests too

A flight is the one test that exercises everything that flew. After a
flight, the projects whose hardware flew get a **flight review**: for
their next `flight_review_cycles` (config, e.g. 3) testing cycles, the
flaws the flight exercised roll discovery at
`discovery_exponent_flight_review` (config, e.g. 1 — below `Bench`,
so flight data beats any stand) instead of their class's N. It is a
bonus on the testing the player is already paying for, not an instant
reveal, so a company that stops testing after a flight learns nothing
from it.

"Exercised" is decided by the flight-event timeline of §3.12: an
engine's flaws count if its stage ignited; a rocket flaw counts if the
event it rolls at was reached; a vehicle lost at first-stage burn
reviews nothing on the second stage. Endurance (`PerDay`) flaws count
only when the flight lasted at least `flight_review_min_days`, so a
LEO delivery does not review a year's wear.

This is the second-flight jump. It also means a lost rocket still buys
knowledge, which is what makes early failures tolerable rather than
pure loss.

USER:  I like a flight review cycle, but again we can do a lower exponent.
Maybe lower than the lowest flaw category.

CLAUDE: Done — a bonus on the next cycles at N=1, scoped to what flew.
The scoping needs rocket flaws to know which stage and event they
belong to, which §3.12 adds.

### 3.3 New consequences

`FlawConsequence` gains:

- **`PadAbort { delay_days }`** — the launch doesn't happen. The flaw
  is discovered, the rocket returns to inventory, the contract clock
  keeps running. Rolls at the first stage's `Ignition` event (§3.12)
  and ends the timeline there. This is how real programs catch things
  without losing a vehicle, and it makes the launch-day popup less
  binary.
- **`PayloadLoss`** — the vehicle flies nominally but the payload is
  damaged or fails to separate: contract fails with the lost-payload
  reputation hit, the rocket's record shows a success-with-loss.
  Rocket domain, top stage only: it rolls at the `PayloadDeploy`
  event.
- **`PowerDraw(watts)`** — the engine or reactor draws extra bus power
  (the roadmap hygiene item). Engine and reactor domains; feeds the
  existing power model.

Weights per consequence move into the domain's exposure mix table so
rockets can be heavier on `StageLoss`/`PadAbort` and engines on
`EngineLoss`.

### 3.4 Descriptions fit the hardware

Replace the flat `FlawPool` arrays with entries tagged by what they
need:

```
FlawEntry { text, exposure, consequence_class, requires: &[Feature] }
Feature: Turbopump | Preburner | GasGenerator | Igniter | RegenCooling |
         LiquidFeed | Hypergolic | Cryogenic | Vacuum | MultiEngine | Boosters | UpperStage
```

The picker filters by the design: a pressure-fed engine never gets a
turbopump flaw, a hypergolic engine never an igniter flaw, a solid
never a valve flaw, "preburner" only on staged combustion / full flow,
"gas generator" only on GG. Rocket entries filter on stage count,
boosters and whether there is an upper stage (no staging flaws on a
single-stage vehicle). Electric, nuclear and sail pools stay as they
are but gain exposure tags.

Pools get larger (aim ≥ 8 entries per exposure × consequence cell that
is reachable) so the filter never empties; a unit test walks every
cycle × propellant combination and every rocket shape and asserts each
cell has at least one entry.

### 3.5 Built units carry their flaws

Make engines consistent with rockets, in the stricter direction:

- `InventoryStage` gains `engine_flaws: Vec<Flaw>` snapshotted from
  the engine inventory items consumed at stage build.
- `InventoryRocket` gains `stage_engine_flaws` (per stage, in design
  order) beside `rocket_flaws`.
- `launch_rocket` and the flight loop roll from the rocket's own
  snapshots. `engine_flaw_table` takes snapshots rather than live
  projects. Discoveries still mark the *project's* flaw by id.
- Each inventory item records `revision` already; the UI shows
  **stale** when its revision is behind the project's and lists the
  flaws the design has since fixed (known fixed flaws only — hidden
  ones stay hidden).

**Retrofit.** A manufacturing order on an inventory engine, stage or
rocket that brings its snapshots to the current design: work
`retrofit_work_per_flaw` × flaws changed (config), material cost a
config fraction of the item's build cost. One key on the inventory
list ("Retrofit all stale"), and a per-project `auto_retrofit` toggle
beside `auto_revise`. **Default: on** — the onboarding path should not
need the concept, and a player who wants to fly a stale unit turns it
off. (Open question below.)

Deficiencies are already live (they change the design's stats, which
the launch reads from the project), so they are not snapshotted.

### 3.6 Deficiencies retire per company

Move `solved` from `Technology` to the company:
`Company.solved_deficiencies: HashSet<TechDeficiencyId>`.

- A new design inherits only the deficiencies its company has not
  solved. Existing projects keep what they carry until revised.
- A successful attempt adds the id to the company's set and removes it
  from that project. Other projects still carrying it keep it until
  their own revision, which succeeds at the current
  `solved_elsewhere_multiplier` rate (kept — it now means "solved on
  another of *our* engines").
- The ⚗ Experimental cue shows on a project only while it carries an
  unsolved deficiency; on the designer's tech line only while the
  company has any unsolved deficiency on that tech.
- Rivals don't use technologies yet; when they do, they face the full
  list.

### 3.7 Third-party parts

Already right in kind (few flaws, unfixable). Changes:

- Tag third-party flaws by exposure like any other; they default to
  `Integrated`/`Analysis` (you don't own the drawings).
- The inventory and engine screens mark their flaws **vendor part —
  cannot be revised**.
- A vendor-fix-after-failure mechanic was considered and dropped: the
  survey finds four third-party failures in ~50, which doesn't carry a
  mechanic. If it ever comes back, Antares/AJ26 is the model.
- **Invented catalogue.** The four fixed entries (`KM-15 Kick Motor`,
  `RD-33K`, `HT-40`, the Dactyl) become a seeded set: per world, 3–5
  vendor engines drawn from role templates (solid kick motor,
  hypergolic upper, kerolox booster, hydrolox expander upper) with
  made-up names (a vendor syllable name plus a model number, the way
  DinoSoar gets its name), stats within a band of the game's own
  baselines for that cycle and size, a price, an availability date and
  a legacy note. Names and stats come from the world seed so a
  playthrough's vendors are stable. Anything that refers to the fixed
  names (the Dactyl constant, guide text, tests) moves to roles.

### 3.8 Modification flaws

Replace the flat 10 % with a per-change roll: each stage added,
removed, or given a different engine or engine count rolls
`modification_flaw_prob_per_change`; a resized stage counts as half a
change. Falcon 1 flight 3 (new engine, residual thrust at staging) is
the canonical case: a new engine on an old stage is exactly one change.
The survey puts modification-caused failures at 6–8 % of all failures,
so the per-change chance stays near the current 10 % and a one-stage
tweak gets no harder than today; only a wholesale redesign does.

### 3.11 Unit defects (optional)

The survey's late failures are often not design flaws but bad units: a
mis-drilled screw hole, a strut of bad steel, one SRB insulator. The
game has nowhere for those to live. A per-unit roll at build time
would give each built engine or rocket a chance of one `UnitDefect`
flaw (own small pool, `Integrated` exposure, never on the design) with
chance falling along the existing learning curve, so the twentieth
unit off the line is cleaner than the first. The retrofit path of §3.5
is also how a defect gets found and fixed on the shelf — or, more
often, the flight finds it.

Cheap once §3.5 exists; left as a question rather than a step.

### 3.12 Flight events: where and when a flaw rolls

Today a launch rolls the first group's engine flaws, then every rocket
flaw, in one lump; upper-stage engine flaws roll in the flight loop
when their group first burns; the popup and the event log get a flat
list. A launch animation needs an ordered timeline, and the flight
review needs to know which hardware was exercised. Both want the same
structure:

```
enum FlightEvent {
    Ignition { group },      // engines start; pad abort lives here for group 0
    MaxQ,                    // atmospheric ascent only — skipped from airless bodies
    Burn { group },          // the stage's engines running
    Separation { group },    // this group lets go of what's above it
    PayloadDeploy,           // top stage releases the payload
    Endurance,               // the daily PerDay roll, unchanged
}
```

- **Every flaw knows its event.** Engine pool entries carry
  `Ignition` (igniter, start transient, hard start) or `Burn`
  (everything else) and roll at that event of their stage. Rocket pool
  entries carry their event — feed and pressurisation at `Ignition` or
  `Burn`, fairing and max-Q structure at `MaxQ`, pyros and interstage
  and residual thrust at `Separation`, payload adapter at
  `PayloadDeploy`, wear at `Endurance` — and each rocket flaw is
  assigned a **stage group** at generation (uniform over groups;
  `MaxQ` and `PayloadDeploy` are pinned to the first and last). So a
  rocket flaw reads "Stage 2 · Separation · Separation pyrotechnics
  fire out of sequence".
- **Separation belongs to the lower stage.** The interstage, the pyros
  and the residual thrust that recontacts (Falcon 1 F3) are the lower
  stage's hardware, so `Separation { group: 0 }` is stage 1's last
  event. Upper-stage ignition problems (Terran 1, H3) are the upper
  stage's `Ignition`. The two roll back to back in the timeline.
- **The ascent is one timeline.** `simulate_launch` walks the events
  of every group the first leg burns, in order — `Ignition 0`, `MaxQ`,
  `Burn 0`, `Separation 0`, `Ignition 1`, `Burn 1`, …,
  `PayloadDeploy` when the leg ends at the destination — rolling each
  event's flaws, applying consequences, and stopping at a stage loss
  or pad abort. The result is `timeline: Vec<TimelineEntry { event,
  activations }>`; the delta-v judgement happens at the end as now.
  The popup lists it in order; the event log already names the
  source. The animation later plays the same vector with a drawing.
- **Later legs** (in-space burns, a landing) run the same event
  sequence for the groups they burn inside the flight loop, minus
  `MaxQ`, so a transfer-stage ignition days later rolls at its own
  `Ignition`. `MaxQ` is only emitted when the departure body has an
  atmosphere (`ambient_pressure > 0`, the figure the overexpansion
  roll already uses).
- **Flight review** (§3.2) takes the set of events reached from the
  timeline.

Migration: existing rocket flaws get an event and group drawn from
the world seed keyed by flaw id, so old saves are stable; engine flaws
default to `Burn`.

USER:  My own thing.  I was just doing another test and saw to engine loss flaws
fire for a rocket after a performance loss issue on the 1st stage.  The rocket
still made orbit and all three flaws are there to show up.  In the "Events" log
the two engine flaws for FC 2 also showed up in the text as for the rocket
(Upgoer 1).  I think there are some issues with rolling flaws and displaying
them, still.  When we add launch animations we'll be breaking up flaw rolling
and display into stages with a simplified animation of the rocket going to
space.  Make sure our structures make sense with that.

CLAUDE: The event-log attribution is a real bug and a small one:
`MidFlightFlawActivated` carries only the rocket name, and
`push_activation_events` in `flight_ops.rs` drops the activation's
`source` (which the popup already uses since 3b37f4a). So an upper
stage's engine flaws, which roll in the flight loop rather than at the
pad, log as "In-flight flaw on Upgoer 1: …". Fix: the event carries
`source` and reads "In-flight flaw on Upgoer 1 — Engine FC 2: …". I
would make that step 0, a separate commit before the plan starts, like
the two launch fixes. The rolls themselves look right from the code:
two engine losses on a stage with margin still reaches orbit. The
structural answer to the animation is §3.12 above.

USER:  In preprarion, we probably want to break up stage flaws to various
events.  Stage 1 will have ignition and Max-Q.  Stage 2 seperation and ignition,
etc.  (Should seperation be stage 1)?  And we can skip the Max-Q one on takeoff
from airless bodies.  Let's think about how that gets supported.

CLAUDE: §3.12. Separation on the lower stage, max-Q only with an
atmosphere, and each rocket flaw pinned to a stage and an event at
generation so the timeline, the popup, the review and the animation
all read the same thing.

USER:  Also, we probably don't want flaws that trigger on long duration missions
being exposed on launches to LEO, though they can get a Flight review bonus.

CLAUDE: They already aren't: since 95fa238 `PerDay` flaws never roll
at launch, only daily in flight, and a LEO delivery is a day or two of
exposure. The review bonus for them is gated on
`flight_review_min_days` (§3.2), so a LEO flight reviews nothing on
the endurance side and a months-long transfer does.

### 3.9 Measuring the curve

`RunSummary` gains the outcome of every launch in order; the summary
line shows the first twelve as `S`/`F`/`?` letters and the multi-seed
footer prints the pooled rate per ordinal (1st, 2nd, 3rd, 4th+). The
monthly CSV is left alone — it cannot order launches within a month,
and leaving it untouched keeps the 40-seed oracle byte-identical. This
is step 1 so every later step is measured against the same line.

### 3.10 Bands and the bot

Per the discussion: steps 3–7 are expected to break the 65 % per-seed
and 85 % aggregate success floors and probably the 20 % bankruptcy
ceiling. Each step records the band result; bands are **not** adjusted
until the final step, which decides between:

- a more prudent `BasicPolicy` (keep testing until a run of quiet
  cycles before the first launch; fly a test mass after a redesign),
- re-baselining the bands to the new default,
- or both.

The engineering salary lever is tried in the same sweep.

---

## 4. Steps (one commit each)

0. **In-flight events name the engine.** `MidFlightFlawActivated`
   carries `source`. Separate small commit before the plan.
1. **Curve metrics.** Success by launch ordinal in `RunSummary`, the
   footer and the CSV; the `testing_cadence` probe (already written).
   Pure instrumentation; 40-seed oracle identical.
2. **Flight events.** `FlightEvent`, stage + event on every rocket
   flaw, `Ignition`/`Burn` on engine entries, the ordered launch
   timeline, popup in event order, `MaxQ` only with an atmosphere
   (§3.12). Migration draws old flaws' event and group from the world
   seed. Odds unchanged in intent; the RNG sequence moves, so the gate
   is the 200-seed bands plus the curve within noise of §2.
3. **Exposure.** `FlawExposure`, `discovery_roll` stored on the flaw,
   per-class exponents, exposure mix per domain. Old saves: exposure
   defaults by domain (engine `Bench`, rocket `Integrated`) and the
   draw recovered from the stored probability. First curve reading.
4. **Hardware-aware descriptions.** `FlawEntry.requires` and the
   filtering picker; pools extended; coverage test. The engine-cycle
   wording fix from the roadmap.
5. **Flight review.** The post-flight bonus cycles, scoped by events
   reached (§3.2).
6. **New consequences.** `PadAbort`, `PayloadLoss`, `PowerDraw`, with
   launch flow, flight loop, popup and event wording. Save v6.
7. **Rebalance to the curve.** Sweep activation exponent, counts,
   consequence weights, exposure mix and exponents to the 35/60/90
   target. Record the sweep in §6. Bands recorded, not fixed.
8. **Built-unit snapshots.** Engine flaws through stages into rockets;
   launch and flight read snapshots; migration fills old items from
   their projects. Stale marker in inventory.
9. **Retrofit.** Orders, "retrofit all stale", `auto_retrofit`.
10. **Deficiencies per company** (§3.6) with migration of `solved`
    flags into the player's set; ⚗ cue.
11. **Third-party and modification.** Invented seeded catalogue,
    exposure tags, "cannot be revised" marker, per-change modification
    roll (§3.7, §3.8).
12. **Bot prudence sweep and bands** (§3.10). Guide text and Next
    Steps updated for the new odds ("most first flights fail").

Steps 8–9 or 10 may become `22_1_BUILT_UNITS.md` / `22_2_DEFICIENCIES.md`
if their design grows during review.

---

## 5. Questions

1. **Exposure classes.** Three (`Bench` / `Integrated` / `Analysis`)
   with discovery 1.0 / 0.3 / 0.1 as the starting point — right shape?
   A fourth, truly flight-only class (0.0) would kill the ULA path, so
   I've left it out.
   USER:  As above we should maybe just try changing the exponent on the
   discoverability first before adding this.  Having it change by flaw class
   makes sense.

2. **Flight review.** Flights roll discoveries against all remaining
   flaws, `Analysis` ones at full rate. Starting at 3 cycles per
   flight. OK in principle?
   USER:  I like the outline of the idea.  We shouldn't have e.g. second stage flaws
   be revealed when we have a first stage explosion, though.  Possibly also we
   should have this be a bonus on the next 3 cycles of analysis on the part or
   rocket after the flight rather than something that happens instantly.

3. **Pad aborts.** Keep? Suggested delay 3–14 days, rolled per flaw.
   The contract keeps ticking, which is the real cost.
   USER:  Sounds great.

4. **Payload loss.** A rocket-domain consequence where the vehicle
   succeeds and the payload is lost. Keep?
   USER:  For the top stage?  Sure.

5. **Engines go stricter.** Engine fixes currently reach built units
   for free; after step 8 they don't. That is a difficulty increase on
   its own. Confirm.
   USER:  Yes, let's do this.

6. **Retrofit default.** `auto_retrofit` on by default (players never
   meet the concept unless they switch it off) or off (players learn
   not to stockpile, the stale marker teaches it)?

   CLAUDE: Still open. My recommendation is **on**, matching
   `auto_revise`: the onboarding path then never meets the concept,
   and the cost of stockpiling shows up as retrofit orders in the
   manufacturing queue rather than as a stale rocket that explodes.
   USER:  Yeah, I think we should go ahead on making it a manufacturing task
   that happens automatically unless turned off.

7. **Exposure mix.** The survey says plumbing on the integrated
   vehicle, not staging, is the big early killer, so `Integrated`
   carries most rocket flaws and `Analysis` about 30 %. Agree, or do
   you want the flight-only feel stronger?
   USER:  As a distribution of difficulty lets try it and see what the results
   are.

8. **Modification roll** per change rather than per modification — OK?
    USER:  Yes.

9. **Third-party flaws** from `complexity/8`: keep, or move to a
   catalogue field per engine so an NK-33 can be dicier than an RL10?
   USER:  Keep.  Though we probably shouldn't be using the names of real rocket
   engines here.  This is actually something where we should have some random
   assortment of made up engines.

   CLAUDE: Added to §3.7 as a seeded invented catalogue, and to the
   steps. Should the stats vary per world too, or only the names?

10. **Unit defects** (§3.11): in this plan as a step after retrofit,
    or deferred?
    USER:  Defer.  I'm not sure about the gameplay implications.

11. Anything from the survey (§1) you want treated as binding rather
    than guidance?
    USER:  No, we want to be informed by the real world but we're neccesarily
    simplifying it.  Informed verisimilatide rather than strict realism per se.

    CLAUDE: Your three closing notes (event-log attribution, flight
    events, endurance flaws on LEO flights) are answered under §3.12,
    where the design they asked for now lives.

12. **Step 0.** Fix the in-flight event attribution now as its own
    commit, before the plan proper?
    USER:  Yes.

13. **Timeline scope.** §3.12 has the launch roll every group the
    first leg burns, in order, so the popup and animation get the
    whole ascent at once; today upper-stage flaws roll in the flight
    loop on day one. Same odds, different place. OK?
    USER:  We want every stage involved in the initial burn to roll one by one
    until we reach orbit.  If a stage only fires up for, e.g., circularizing
    from GTO to GEO we can roll its flaws then.

---

## 6. Record

**Step 0 — in-flight events name the engine.** `MidFlightFlawActivated`
gains `source` (serde default, so old event logs load with it empty
and print as before); the three constructors in `flight_ops.rs` pass
the activation's source, and the overexpansion event names the stage.
Text: "In-flight flaw on Upgoer 1 — Engine FC 2: … (engine loss)". The
three-stage mid-flight test now asserts the source names the engine.
No RNG or CSV change.

**Step 1 — curve metrics.** `Tally.launch_outcomes` records every
attempt in departure order and resolves it from the arrival, loss,
stranding or partial-failure event, matching by rocket name oldest
first (events carry no flight id). `RunSummary.launch_outcomes`,
`curve_word()` on the summary line, `curve_by_ordinal` /
`curve_line` for the footer, three unit tests. The
`testing_cadence` probe (§2) is part of this step. 40-seed oracle CSV
byte-identical to HEAD. Baseline at 100 seeds × 6 years:

| Ordinal | Success |
|---|---|
| 1st | 58 % (58/100) |
| 2nd | 58 % (58/100) |
| 3rd | 86 % (86/100) |
| 4th+ | 95 % (820/863) |

**Step 2 — flight events (uncommitted, awaiting a decision).**
`FlawPhase` on every flaw, `stage_group` on rocket flaws, pools as
`FlawEntry { text, phase }`, `FlightEvent` and `ascent_events` /
`ascent_groups` / `roll_event_flaws` in `launch.rs`, the launch walking
every group the first leg burns, the flight loop walking later groups
(separation of the group below, ignition, burn), `Flight.rocket_flaws`
for those later events, rocket discoveries by flaw id, save v6 with
`migrate_flaw_phases_to_v6` (phase from the description's pool entry,
group from the world seed), flaw lists prefixed "S2 separation ·", the
popup prefixed with the event. 702 tests, clippy clean.

The curve did **not** stay within noise, and not because the RNG
sequence moved. Two leniencies in the old flow closed:

1. *Upper-stage flaws rolled after the burn that used the stage.* On a
   single-leg flight (every LEO delivery) the flight loop burned the
   whole route first, then rolled the newly burned groups' flaws, so an
   upper-stage engine loss or degradation never bit; only a `StageLoss`
   did, as "lost in flight". The timeline rolls them on the pad and the
   delta-v judgement sees them.
2. *A stage that lost its only engine kept its delta-v.* The planner's
   figure is mass-ratio Isp and ignores thrust, so the pad judged the
   launch sound and the flight loop then stranded it "at LEO" on a burn
   with no thrust (a thrustless *first* stage got an empty route and
   "arrived"). An engineless stage is now dead weight
   (`apply_consequence_to_stage`), and the launch fails on the pad
   with the flaw named.

First launches by outcome, 100 seeds (`probes::first_launch_outcomes`):

| | pad failure | partial | stranded | lost in flight | success |
|---|---|---|---|---|---|
| step 1 | 37 | 0 | 0 | 11 | 52 |
| timeline only | 39 | 1 | 21 | 0 | 39 |
| + dead-weight stages | 64 | 1 | 0 | 0 | 35 |

Curve, 100 seeds × 6 years: **33 / 54 / 75 / 92** (was 58 / 58 / 86 /
95); bankrupt 22/100 (was 10). 200 seeds × 8 years: 41/200 bankrupt
against the 20 % ceiling; the 20-seed smoke band fails the same way.
The bot's upper stage has one engine, so every upper-engine
`EngineLoss` (35 % of its flaws) is now a lost mission.

Decision (user, 2026-10-10): accept — this is the physics the timeline
exists to make honest, and the first flight already sits at the 35 %
target before any tuning; step 7 tunes from here and step 12
re-baselines the bands. To keep every commit passing `cargo test` the
bankruptcy ceilings are lifted now, 20 → 25 % at 200 seeds and 25 →
40 % on the 20-seed smoke run, and the two floors the same shift
moved come down with them: seeds keeping $25M 62 → 50 % (measured
107/200), seeds with a profitable year 85 → 72 % (156/200; 60 % on the
smoke run). Every other band holds, including the 85 % aggregate
success floor. Each carries a comment pointing here. The "stranded at
LEO" wording for an ascent that fell short is a TODO.

**Step 3 — exposure.** `FlawExposure`
(`Bench` / `Integrated` / `Analysis`) on every flaw, taken from its
pool entry; `discovery_roll` stored instead of the finished
probability, with `Flaw::discovery_probability(exponent)` computed at
roll time; `FlawsConfig::discovery_exponent_{bench,integrated,analysis}`
replacing `flaw_discovery_exponent`; per-domain exposure
mixes (engine 70/25/5, rocket 15/55/30, reactor 70/25/5) drawn before
the entry is picked, falling back to any entry when a pool has no
entry of that class for the consequence; save v7 (`discovery_roll`
recovered at the JSON level as `sqrt(p / sqrt(activation))`, exposure
from the words). 702 tests, clippy clean.

First cut at the proposed 2 / 5 / 9, then the user's choice (option 3
of three: continue with fleet bands suspended, lift the bands again,
or soften the exponents) — soften:

| 100 seeds × 6 years | step 2 | 2 / 5 / 9 | 2 / 4 / 6 | **2 / 3 / 4** |
|---|---|---|---|---|
| 1st / 2nd / 3rd / 4th+ | 33 / 54 / 75 / 92 | 29 / 43 / 61 / 88 | 33 / 42 / 65 / 89 | 33 / 41 / 73 / 90 |
| Bankrupt | 22 | 39 | 34 | 25 |
| Hidden flaws at first launch | 7.5 | 8.7 | 8.3 | — |
| Rocket flaws fixed before first launch (of ~4.8) | 1.9 | 0.9 | — | 1.4 |
| 200 × 8 y bankrupt | 41 | 75 | 68 | 49 |

At 2 / 5 / 9 the plateau fell because `Analysis` flaws are rarely
found on the ground and the flight review meant to find them is step
5. At 2 / 3 / 4 the 25 % bankruptcy ceiling holds (49/200); the $25M
floor moves 50 → 40 % (89/200; smoke 30 %) and the launch cap 30 → 35
(one seed re-flying failures reached 31). The class gap is small for
now; step 7 widens it once flights review.
