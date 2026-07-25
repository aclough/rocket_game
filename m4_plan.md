# M4 Balance Pass Plan — difficulty, cost realism, dev-time realism

Goals from TODO (Balance section) and your framing: make the game harder, make
rocket costs more realistic, and sanity-check development times against iconic
real programs — with realism as a *reference point*, not a ruler. Open questions
marked **Q:** — please answer inline as USER:.

---

## 1. Where the game sits today (measured)

Start: 2001-01-01, $200M, 1 engineering team. Sim probe (seeds 1–5, BasicPolicy):

| Metric | Current value |
|---|---|
| First orbital launch | month ~14 (Feb 2002, all 5 seeds) |
| Monthly burn, pre-revenue | ~$1.35M (salaries ~0.7% of capital/mo) |
| Min money, 200-seed band | $71.9M floor — nobody is ever close to death |
| Bankruptcies | 0/200; 195/200 profitable |
| Heavy-lift marginal cost | ~$8.6M (DinoSoar catalog $9M) |
| GEO payments | $80–400M |

Work formulas: engine design 120 team-days @ complexity 5, rocket design 60,
engine build 90, flaw fix 30, flaws ≈ complexity. Materials: aluminium $5/kg,
electronics $20k/kg, etc.

### Task 1 instrumentation baseline (200 seeds, pre-retune)

| Metric | Baseline |
|---|---|
| First launch | 2002-02-14, month 13.4 — **all 200 seeds identical** (BasicPolicy timing is deterministic) |
| Dev spend at first launch | $28.5–32.1M (avg $32.0M) — vs real $90–150M programs |
| Hidden flaws at first launch | 1–12 (avg 6.7) — the rush-risk raw material already exists |
| Mean rocket unit cost | $5.2–6.7M (avg $6.0M) |
| Mean contract payment | $26.0–35.1M (avg $30.2M) — payment ≈ 5× unit cost |

### Task 2 cost retune — landed values and 200-seed result

Sweep (50–100 seeds per point): material prices ×2/×3/×4 crossed with bot
margins 1.0/1.25/1.5/4.0. Materials are ~half of unit cost, so ×3 → ~2×
unit cost, ×4 → ~2.6×. At the old margin 4.0 the bot just charged more
(payment $63M — payments must NOT rise, they're already real prices), so
the bot margin comes down with the cost rise. Chosen point:

- **Material prices ×4** (aluminium $20/kg … electronics $80k/kg): unit
  cost $6.0M → **$15.4M**, dev spend to first flight $32M → **$66M**
  (real: $90–150M; Task 3's time stretch adds the rest).
- **BasicPolicy `DEFAULT_BID_MARGIN` 4.0 → 1.0** (bid = cost × 2): avg
  payment **$31.1M** — unchanged from the pre-retune $30.2M, i.e. payments
  stayed at real prices while cost rose to **50% of a winning bid**
  (Q2 target: 40–60%).
- **DinoSoar**: catalog_cost $9M → $36M, margins 8–20× → **3–8×** — its
  GEO bids stay ~$100–170M (real prices) while its implied markup becomes
  merely incumbent-fat instead of absurd. bid_floor $60M unchanged.

200-seed re-baseline (8y): **1/200 bankrupt** (seed 169), 5/200 dip below
$0 mid-run and mostly recover, median final $213M (117/200 end above the
$200M start — was 195/200 pre-retune), median min-money $102M, launches
4–31, aggregate success 95.4%, 200/200 reach a profitable year (latest
start+7). The game is already tighter; Task 3's rush-risk mechanics and
Task 5's calibration take it the rest of the way to 2–4/100.

Flag for Task 5: seed 169 went bankrupt with 31 launches at 97% success —
it won a block program and appears to have lost money on volume. Check
block-bid unit economics when hunting degenerate patterns.

sim_bands.rs re-baselined in the same edit: per-seed min-money floor
replaced by a runaway-debt bound (min > −$60M) plus fleet bands
(bankrupt ≤ 1.5%, ≥85% keep min above $25M, ≥55% end profitable).

### Task 3 dev-time scaling + rush risk — landed values and 200-seed result

Mechanics landed exactly as agreed in Q1:

- **`flaw_discovery_exponent` = 2.0** (FlawsConfig): ground discovery is now
  `uniform^N × sqrt(activation_chance)`. Mean discovery rate drops from 1/2
  to 1/3 of the sqrt cap; the low tail is the de-facto "never saw it on the
  stand" class. Flight activations stay always-discovered.
- **Stop at first StageLoss**: launch and mid-flight flaw rolling (including
  the daily endurance and reactor rolls, same rationale) breaks out at the
  first StageLoss activation — the vehicle is gone, nothing after it can be
  observed, so a launch discovers at most one rocket-destroying flaw. This
  alone moved 200-seed bankruptcies 1 → 3.
- **Work exponents** (WorkConfig, `base × (complexity/5)^p`, anchored so
  complexity 5 is unchanged): **design p = 2.5** (engine, reactor, rocket),
  **engine build p = 1.5**. Sweep grid p∈{1.5,2,2.5} × N∈{2,3}: p2.5/N2 and
  p2/N2 both hit the bankruptcy band; p2.5 chosen for the realism story
  (staged-combustion effective-9 design work ×4.3 → multi-year program;
  the bot's GG kerolox c6 only ×1.6) and lower runaway debt. Build stayed
  gentler after measurement: at p2.5 DinoSoar's complexity-12 booster
  engine builds 3.7× slower, starving its line (campaign cadence test
  caught it, and no free stock = no bids = weaker GEO competition); at 1.5
  it is 1.55×, which its line absorbs.

200-seed re-baseline (8y, final defaults): **4/200 bankrupt** (2.0/100 —
inside the agreed 2–4/100), first launch month **17.5** avg (was 13.4)
carrying **~8 undiscovered flaws** (was ~7), dev spend to first flight
$69M, unit cost $15.6M / payment $31.3M (cost ratio holds at ~50%),
aggregate success 92.6% (was 95.4%), 77/200 end above the $200M start,
161/200 keep min money above $25M, 9 survivors dip below $0 and recover
(worst −$89.7M), 199/200 reach a profitable year (latest start+7).
sim_bands.rs re-baselined: debt bound −$120M, bankrupt ≤ 3.5%, ≥72% keep
min above $25M, ≥30% end profitable, per-seed success ≥ 65%, aggregate
≥ 90%, launches 3..=30.

No BasicPolicy touch was needed — its revise-test-fly rhythm survives the
slower discovery convergence (it just flies with more residue aboard).

### Task 4 plan — engine progression, cost realism, and size scaling

Scope grew per engine_cost_scaling.md (your Q1–Q3 answers there): the
original big-vs-clustered measurement and research escalation, plus the
three engine-cost levers folded in. Sub-parts, in proposed order:

**4a. Hydrolox material premium (your priority).** A data-driven
per-preset `material_multiplier` on the engine BOM — rebalancing BOM
fractions alone can't get there (superalloys are only $320/kg; even a
fraction shift maxes out around 1.7×). Sweep range 2–4×. At 3.5× the
game's RL10-analog lands ~$2.6M and ~10× kerolox per kN (real is ~100×,
but our absolute scale is compressed ~30×, so ~10× relative is the honest
equivalent). **Big known ripple:** DinoSoar's 6.6 t hydrolox booster
engine jumps from ~$8.7M to ~$30M of materials at 3.5×, i.e. its real
unit cost roughly +$20M — its catalog_cost/margins from Task 2 must be
re-swept in the same change, and its GEO bids must stay at real prices
(that's the market anchor). The player's hydrolox upper stage also stops
being free Isp, which is itself part of the Task 4 goal.

**4b. Cycle premium — complexity and/or BOM (your Q3: both).** One
correction from the measurements: Task 3 already made complexity
superlinear, so the *dev-side* cycle premium is now real (FF design work
is 4.3× a starter engine); a further complexity bump would double-tax dev
time and flaw counts. Proposal: the unit-cost side of the premium lives
in a per-cycle material multiplier (e.g. PF 0.8 / GG 1.0 / Expander 1.3 /
SC 1.6 / FF 2.0, data-driven), and we hold the complexity table steady —
*unless* the 4d measurement still shows top cycles dominating, in which
case bumping SC/FF to 9/10 is the fallback sweep point (it feeds design
work, build labor, and flaw counts all at once, which may be exactly the
brake big engines need).

**4c. Size terms (folded in per your Q2).** Engine build work gains a
mass factor `× (engine_mass_kg / 1150)^q`, q ∈ 0.5–0.7, anchored at the
starter kerolox GG's mass so the bot's pace is unchanged; design work
gains `× scale^r`, r ∈ 0.3–0.5 (a 4× engine costs ~1.5–2× the dev, the
F-1 story). Kills the current free amortization where 4×-scale engines
are the cheapest per kN. **Watch:** DinoSoar's 6.6 t booster engine hits
the mass factor at ~(5.7)^q ≈ 2.4–3.4× build labor — the same
line-starvation failure Task 3 caught at build-exponent 2.5. The campaign
cadence test and its bid readiness gate the sweep; if it starves, either
q comes down or DinoSoar gets a production-line/config compensation, to
be decided by measurement, not assumption.

**4d. Measurement: big vs clustered.** Fixed capability target (the
template's LEO smallsat + a GTO comsat case), two builds of the same
rocket: one scale-2 engine vs four scale-0.5 engines at equal total
thrust. Compare unit cost, dev calendar to Testing, launch failure
probability (per-engine flaw activation already scales 1−(1−p)^n, taxing
clusters), and $/kg delivered. Run before 4a–4c land and after, so we
know whether the cost levers alone rebalance the trade or something
engine-specific is still needed. This answers the TODO "fewer, bigger
engines seem too superior?".

**4e. Research-improvement escalation.** The TODO "engine improvements
get harder quickly the more there are": `improvement_discovery_chance`
(flat 8%/cycle today) decays with improvements already found on that
design — `chance × decay^n`, `improvement_decay` data-driven in
FlawsConfig (sweep 0.5–0.8). Same treatment for the reactor chance.

**4f. Re-baseline + bookkeeping.** 200-seed re-baseline (bands moved by
4a–4c), DinoSoar catalog/margin re-sweep in the same edit, both Balance
TODO items removed at commit.

Open questions:

- **Q4a:** Hydrolox premium magnitude — comfortable landing ~10× kerolox
  per kN (multiplier ~3–3.5×), or softer (~2×, keeping DinoSoar's
  re-tune smaller)?
- USER: Lets go all the way here.
- **Q4b:** OK holding the complexity table steady unless 4d's measurement
  demands the SC/FF bump, per the double-tax argument above?
- Yeah, we'll try the measurement first
- **Q4c:** If the size-term sweep starves DinoSoar's line, is giving it
  an extra production line (config) acceptable as the compensation, or
  would you rather cap q?
- USER:  That would be fine.

### Task 4 — landed values and 200-seed result

All five sub-parts landed; one mechanism was added beyond the plan
because measurement demanded it (flagged below).

- **EngineMaterialsConfig** (new balance section): per-preset material
  multipliers (hydrolox **3.0×**, others 1.0) × per-cycle multipliers
  (PF 0.8 / GG 1.0 / Expander 1.3 / SC 1.6 / FF 2.0), applied to the
  engine BOM in `engine_material_cost` — the single cost source both
  the player and DinoSoar consume. Materials-only: no labor premium,
  labor teams stay unspecialized (decision below).
- **Hydrolox calibration story (4a).** First pass targeted ~10×
  kerolox per kN off the RL10-vs-Merlin gap, and materials alone
  couldn't reach it (the 4c mass term removes more flat labor from a
  small engine than the premium adds), so a "specialist-build
  surcharge" — the premium applied to expected build labor as cash at
  order time — was tried and hit 13×/kN. A follow-up research pass
  killed the target itself: **RL10 over-generalizes.** RS-68, the
  hydrolox booster *designed to cost*, runs ~$20M at 3,137 kN ≈
  $6.4k/kN — only **~5× a Merlin** — and RL10's price is mostly a
  hand-brazed 1960s process (~20 months per combustion chamber,
  ~1 engine/month; the 3D-printed RL10C-X is ~1/week), i.e. the
  learning curve's territory, not the BOM's. Decision (yours): drop
  the surcharge, keep the unspecialized-teams abstraction, tune
  hydrolox to 3.0× materials-only. Result: hydrolox GG ~2.9×/kN,
  expander upper ~5.7×/kN — right in the RS-68-grounded band.
- **Size terms (4c)**: engine build work × (mass/1150 kg)^0.6 (anchored
  at the starter kerolox GG), engine design work × scale^0.4. Reactors
  pass scale 1.0.
- **Improvement decay (4e)**: discovery chance × 0.7^(improvements
  already found), engines and reactors.
- **DinoSoar ripple (re-swept in the same change)**: measured marginal
  cost $36M → **$39M** (hydrolox premium on its 6.6 t booster engine);
  catalog_cost 36→39M, margins 3-8× → **2.6-7.2×** so its GEO bid
  range stays ~$103-285M (real prices — the market anchor holds);
  production_lines 8 → **12** (the mass exponent makes its booster
  ~2.9× the line-days; 12 is the minimum that keeps a won campaign's
  cadence, per Q4c). dino_probe: still ~18-21 awards/8y, healthy
  stock, money growing.

**New engine cost surface** (unit cost, per-kN vs kerolox GG):
kerolox GG $2.54M (unchanged, the anchor); kerolox SC $5.0M (1.7×/kN);
kerolox FF $7.7M (2.3×); hydrolox GG upper $0.9M (2.9×); hydrolox
expander $1.4M (5.7×); hydrolox SC $1.8M (5.0×); methalox FF $6.3M
(2.4×); hypergolic PF $0.23M. Liquid-space spread went from 4.5× to
~34×, with the per-kN ordering matching the RS-68/RD-180/Merlin data
rather than the RL10 outlier.

**4d big-vs-clustered, before → after** (same design, 1×scale-2 vs
4×scale-0.5 kerolox GG at equal thrust): before, big dominated
outright — same design time, same launch risk (cluster's ~4× flaw
activation rate almost exactly cancels its engine-out redundancy), and
$3.9M vs $7.4M unit cost (clusters paid 4× flat labor). After the size
terms: big $4.5M vs cluster $5.8M (1.9× → 1.3× gap) and **250 vs 143
design days** — big engines are now a slower program, clusters a
faster, costlier one. A real trade; no engine-specific mechanics
needed, and the SC/FF complexity bump stays on the shelf (Q4b).

**200-seed re-baseline** (8y, final defaults): **1/200 bankrupt**
(seed 172 — the bot's small hydrolox upper engine builds cheaper under
the mass term, easing Task 3's 4/200; Task 5 recalibrates toward
2-4/100), first launch month 16.7 with ~9 hidden flaws, dev spend
$69M, unit cost $15.0M / payment $30.0M (cost ratio ~50% holds),
aggregate success 92.9%, 91/200 end above start, 16 survivors dip
below $0 and recover (worst −$100M), 200/200 reach a profitable year.
sim_bands re-baselined (band values unchanged from Task 3; only
recorded baselines moved). Both Balance TODO items ("fewer, bigger
engines", "improvements get harder") are addressed and removed at
this commit.

## 2. Real-world benchmarks (web-verified per Q5; corrections applied and marked ✱)

### Engines — start of full development → first flight

| Engine | Dev span | Notes |
|---|---|---|
| Merlin 1A | 2003 → 2006 ✱ | ~3 yr (first prototype hot-fire 2003; 2002 was just SpaceX's founding), pintle gas-gen |
| Rutherford | 2013 → 2017 | ~3.5–4 yr (first hot fire Dec 2013), electric pumps, printed chambers |
| RL10 | 1958 → 1963 | ~5 yr, first hydrolox |
| RD-180 | 1996 → 2000 | ~4 yr, but derived from RD-170 |
| F-1 | 1959 → 1967 | ~8.8 yr; years lost to combustion instability |
| RS-25 (SSME) | 1971/72 → 1981 | ~9 yr, staged-combustion reusable |
| Raptor | ~2012 concept → 2019 flight ✱ | 2016 test article was subscale; first full-scale firing Feb 2019, flew on Starhopper mid-2019 |

Pattern: **~3–4 years is the floor** for a clean-sheet engine, even simple
ones; ambitious cycles run 8–9. Derivatives run ~2–4.

### Rockets — program start → first orbital attempt, and program cost

| Rocket | Dev span | Program cost to first flight |
|---|---|---|
| Falcon 1 | 2002 → 2006 (success 2008) | ~$90M incl. 3 failures (NASA-verified) |
| Electron | ~2013 → 2017 | ~$148M raised by first flight ✱ (not all spent) |
| Pegasus | 1987 → 1990 | ~$45–55M (1990 $) ✱ — famously cheap |
| Falcon 9 v1.0 | 2005 → 2010 | ~$300M (NASA-audited ~$390M incl. F1) |
| Terran 1 | 2015 → 2023 ✱ | ~7.5–8 yr (the cautionary tail) |
| Saturn V | 1962 → 1967 | ~$6.4B then-year (~$50B today) |

### Prices / unit costs

| Vehicle | List price | Est. build cost |
|---|---|---|
| Electron | ~$7.5M | few $M |
| Falcon 9 | $62–70M | ~$30–50M expendable-era (~$28M marginal with reuse per Musk) |
| Atlas V | $110–153M | — |
| GEO comsat launch, market | $60–120M per payload ✱ | Ariane 5 was ~$150–180M/launch but dual-manifested |

## 3. What the comparison says

1. **Payments are roughly realistic already.** $80–400M GEO vs real $60–120M —
   high at the top, but the right order of magnitude. Anchor here.
2. **Hardware is ~5–10× too cheap.** An $8.6M heavy lifter selling into $80M+
   contracts is the whole TODO scale mismatch. Closing the gap by *raising
   costs* (not cutting payments) also delivers the difficulty goal for free:
   thin margins, losses that hurt, a min-money band that actually dips.
3. **Dev is ~3× too fast — with one team.** 14 months from nothing to orbit vs
   ~4 years for Electron/Falcon 1 (with 100+ staff). And the game lets teams
   parallelize it further.
4. **NRE is nearly free.** Real programs burn $50–100M+ before first flight; in
   game the pre-revenue burn over 14 months is ~$19M, mostly recoverable. The
   $200M start never functions as a runway. SpaceX and Rocket Lab both reached
   first flight on roughly $90–150M — with realistic burn, $200M is a *tense*
   2–4 year runway. That's a game.

## 4. Proposed direction

**Anchor payments at real launch prices; pull costs and dev times toward real,
stopping where fun demands.** Concretely:

- **Costs (the big lever).** Retune material prices / masses-to-cost so a
  small lifter lands ~$5–10M marginal and a heavy ~$30–60M — i.e. marginal
  cost ≈ 40–60% of a competitive bid instead of ~10%. Raise salaries and
  facilities so pre-revenue burn is several $M/month — dev spend becomes real
  NRE. DinoSoar's `catalog_cost`, bid floor, and margins retune in the same
  sweep (TODO already flags measuring ripple with the harness). Your earlier
  note "raise budgets for contracts when we have competition" slots here as a
  `budget_tolerance` adjustment.
- **Dev time (the medium lever).** Don't chase 4 years — an empty first year
  kills the game. Lean: stretch work numbers ~1.5–2× so first launch at
  starting team levels lands ~2–2.5 game-years, and let *realism live in the
  follow-on generations*: each subsequent engine class/improvement gets
  substantially more expensive in team-days (this is also the TODO item
  "engine improvements get harder quickly the more there are" — reality
  agrees: F-1 and RS-25 were 8–9 yr programs). Early game stays brisk;
  a Raptor-class engine is a near-decade commitment, as it should be.
      USER:  THat specifically was research to add bonuses like +3 thrust or things
      like that.  For time we might want to look at scaling time to complexity or
      size more aggressively.
      CLAUDE: Understood on both counts — I'd conflated the two. Revised:
      (a) **Superlinear complexity/size scaling** replaces the flat stretch.
      Work formulas are currently linear (`base × complexity/5`); change to
      `base × (complexity/5)^p` with a data-driven exponent per formula
      (design, build), plus a mass exponent on stage build. Anchor so a
      complexity-4–5 starter engine stays near today's pace, while a
      complexity-8+ staged-combustion monster becomes a multi-year program —
      which is exactly where the real 8–9-year outliers live. Exponents are
      config knobs, swept in Task 3.
      (b) The **research-bonus escalation** (+3 thrust etc. via
      `improvement_discovery_chance`, flat 8%/cycle today) is its own knob:
      decay the discovery chance as a function of improvements already found
      on that design (data-driven curve). Filed as part of Task 4 with the
      engine-count question, since both shape late-game engine progression.
- **Difficulty as a measurement target, not a vibe.** Define "harder" as
  band targets before touching knobs, then sweep until the harness hits them.

- **Q1:** Pacing target for the first launch: keep ~14 months, or stretch to
  ~2–2.5 years (my lean)? This is the single biggest feel change.
  USER:  I'd like to make it more dangerous in terms of hidden flaws to get
  a rocket up in 12 months.  That might mean making them harder to find just by
  research as opposed to flight.
  CLAUDE: Speed stays, safety becomes the tradeoff — better than a flat
  stretch. Mechanically this lands well on the existing model: each flaw
  today rolls `discovery_probability = uniform × sqrt(activation_chance)`
  per ground-testing cycle, and flights only reveal flaws by *activating*
  them. Proposal for Task 3: (a) a config `ground_discovery_factor` (< 1)
  scaling ground-test discovery rolls, so test campaigns converge slower and
  leave a real residue of undiscovered flaws at the 12-month mark; (b) a
  config fraction of flaws that are **flight-only** (ground discovery ≈ 0 —
  the "we never saw it on the stand" class, think F-1 combustion
  instability); (c) flights that survive a flaw activation still discover
  it, as today, so early flying is genuinely informative — you're paying in
  risk instead of calendar. Rushing in 12 months stays possible and becomes
  a gamble; a patient 2-year test campaign buys reliability. Knobs
  data-driven, swept so the 200-seed failure rates land where Q3 wants them.
  USER:  Could we just switch to a tougher distribution, like uniform^N
  * sqrt(activation_chance) where we figure out an N that gives us the
    difficulty we want?  Also, we should probably add a decent chance of
    discovering a flaw even if it does destroy the rocket if we're going this
    route.
  CLAUDE: Yes — `uniform^N × sqrt(activation_chance)` is strictly better
  than my two knobs: one exponent, and the low tail of uniform^N *is* the
  "never saw it on the stand" class (at N=3, ~46% of flaws roll under 10%
  of their sqrt cap), so no discrete flight-only category needed. Mean
  discovery rate scales as 1/(N+1), so N=1 is today and N≈2–3 is probably
  the sweep range. `flaw_discovery_exponent` in FlawsConfig, swept in
  Task 3 against the month-12 undiscovered-residue metric.
  On the second point, one correction: today a flaw that activates in
  flight is **always** discovered, even when it destroys the rocket. So
  the change is to make destructive activations *probabilistic* instead of
  certain: add `loss_investigation_chance` (default ~0.7?) — an activation
  your vehicle survives is always discovered (you have the vehicle and full
  telemetry), one that destroys it is discovered at the investigation
  chance. That makes a failed rush doubly costly — sometimes the accident
  board comes back inconclusive and you've spent a rocket for nothing —
  which sharpens the gamble without touching the ground path.
  USER (from chat): For simplicity keep always-discovering flaws that
  activate — no investigation roll; if we need more difficulty we can add
  more flaws. And: stop rolling flaws once one destroys the rocket, so a
  launch discovers at most one rocket-destroying flaw.
  CLAUDE: Agreed and confirmed against the code: `simulate_launch` today
  rolls every flaw independently (engine flaws stage-by-stage, then
  rocket-level flaws, then overexpansion risk) and discovers all
  activations — so one bad launch can currently harvest several StageLoss
  discoveries. Change for Task 3: roll in the existing order and **break
  out of all remaining flaw/overexpansion rolls at the first StageLoss
  activation on a firing stage group** (the vehicle is gone; nothing after
  it can be observed). Same rule applies to the mid-flight rolls when upper
  stage groups ignite. Degradation/EngineLoss activations don't stop the
  rolls — the vehicle flies on. `loss_investigation_chance` is dropped.
- **Q2:** Close the cost/payment gap from the cost side as proposed (payments
  stay ≈ real prices, marginal cost rises to ~40–60% of a winning bid)? The
  alternative — cutting payments to match today's costs — makes the money
  numbers on screen unrealistically small forever.
  USER:  That sounds good.
- **Q3:** How hard is "harder"? My lean for the 200-seed BasicPolicy bands:
  min-money floor drops from $71.9M to the $10–30M range, a handful of seeds
  end unprofitable, but still ~0 bankruptcies for the scripted bot (bots don't
  make greedy human mistakes; if the *bot* goes bankrupt, a human's first game
  is unwinnable). Comfortable with a floor that low?
  USER:  For a vaugely rougelike flavor I don't think I'd go quite so low.
  Maybe 2-4 out of 100 seeds.
  CLAUDE: Reading that as: don't grind *every* run down to a $10–30M floor —
  instead let 2–4 out of 100 BasicPolicy seeds actually go bankrupt, with
  surviving seeds keeping a less desperate floor. Roguelike: death is
  possible, not constant near-death. Band targets become: bankruptcies
  2–4/100 (guard test asserts a range, e.g. 1..=6 per 100, instead of
  zero), surviving-seed min-money floor somewhere around $30–50M. The
  bankruptcies will mostly come from the Q1 risk mechanics (a rushed rocket
  eating a flight-only flaw streak) plus thinner Q2 margins — which is the
  right *kind* of death for the flavor.
- **Q4:** Scope: fold the other Balance TODO item "fewer, bigger engines seem
  too superior?" into this pass, or keep M4 to costs + time + difficulty and
  hunt that separately? (The improvements-get-harder item is already in via
  the dev-time lever.) My lean: keep it out — it's a design-hole hunt, not a
  number retune, and this pass is already wide.
  USER:  I think that it's an important part of the overall struggle to get to
  the GTO/GEO market and at least bares a look.
  CLAUDE: In scope then — Task 4. Note the levers interact in our favor:
  superlinear complexity scaling (dev-time answer above) already taxes the
  one-big-engine path with much longer dev, and flaw count ≈ complexity
  means a big engine carries more hidden risk under the Q1 mechanics. Task 4
  measures whether those two shifts alone rebalance big-vs-clustered
  engines before we touch anything engine-specific.
- **Q5:** The benchmark tables above are from memory (dates included so
  they're checkable). Good enough to steer a game-balance pass, or want me to
  run a web-verified research pass first?
  USER:  Yes, do a web search double check but I suspect you're correct.
  CLAUDE: Verification pass running; corrections will be folded into the
  §2 tables and marked.

## 5. Tasks (each with its own commit + your approval)

1. **Harness instrumentation** — add to the sim summary: date of first launch,
   cumulative dev spend at first launch, mean marginal cost per launch, mean
   payment per completed contract, and undiscovered-flaw count at first
   launch (eyes for the Q1 risk knobs). Small; gives the retune eyes.
   Baseline these on 200 seeds before any knob moves.
2. **Cost retune** — materials/salaries/facilities + DinoSoar economics +
   budget_tolerance, swept with the harness toward the Q2/Q3 targets;
   re-baseline `sim_bands.rs` (test assertions updated in the same edit, per
   CLAUDE.md).
3. **Dev-time scaling + rush risk** — superlinear complexity/size exponents
   on the work formulas (anchored so complexity-4–5 stays near today's
   pace), plus the Q1 flaw-discovery shift: `flaw_discovery_exponent`
   (uniform^N in the discovery-probability roll, data-driven) and the
   stop-at-first-StageLoss rule in launch/flight flaw rolling (activated
   flaws stay always-discovered; at most one rocket-destroying discovery
   per launch). May need a BasicPolicy touch if its scripted rhythm
   assumes today's discovery convergence.
4. **Engine progression balance** — measure big-vs-clustered engine
   economics under the new scaling (is "fewer, bigger" still dominant?),
   and add the research-improvement escalation curve (discovery chance
   decays with improvements already found). Mechanics changes only if the
   measurements say the Task 2–3 shifts didn't already fix it.
5. **Difficulty validation** — 200-seed re-baseline against the agreed band
   targets (2–4/100 bankrupt, surviving floor ~$30–50M), then hand playtest
   for feel; record findings and any degenerate strategies spotted for the
   M4 part 2 (design-hole hunting).

Not in scope: second small-lift competitor (separate M4 item, worth doing
*after* costs settle so its economics are born correct), flight-loop
extraction, split awards.
USER:  Sounds good.
