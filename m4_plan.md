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
