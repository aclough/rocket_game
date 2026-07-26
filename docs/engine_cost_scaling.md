# Engine cost/time scaling across size, cycle, and propellant

Post-M4-Task-3 analysis (2026-07): how much do the game's engines vary in
unit cost and dev time across the design space, how much do real engines
vary, and where are the gaps. Measurement + proposal only — no code has
changed. Comment inline as USER:.

## 1. What the game computes today

Unit engine cost = **materials + build labor**, both × learning curve:

- **Materials** = engine mass × per-kg BOM price. Mass = thrust / (TWR × g).
  The chemical BOMs are nearly identical: kerolox/methalox $1,182/kg,
  hydrolox $1,317/kg (+11%), hypergolic $1,152/kg, solid $481/kg. So
  materials are effectively *one price per kg* regardless of cycle or
  (liquid) propellant.
- **Build labor** = `90 × (complexity/5)^1.5` team-days × $10k/day.
  Depends only on complexity (5–9) — **no size term**, so it's flat
  across scale 0.25–4.0.
- **Design work** = `120 × (effective/5)^2.5` team-days (Task 3). Also
  **no size term**: a 0.25-scale and a 4.0-scale engine take identical
  dev time.

## 2. The game's surface (current defaults, scale 1.0, no learning)

| Engine | cx/eff | Thrust | Mass | Unit cost | $/kN | Design days |
|---|---|---|---|---|---|---|
| Kerolox GasGen (Merlin-ish) | 6/6 | 900 kN | 1,147 kg | $2.54M | 2,820 | 189 |
| Kerolox StagedComb (RD-180-ish) | 8/8 | 1,035 kN | 1,507 kg | $3.60M | 3,480 | 389 |
| Kerolox FullFlow | 9/9 | 1,170 kN | 1,988 kg | $4.52M | 3,870 | 522 |
| Hydrolox Expander (RL10-ish) | 7/8 | 88 kN | 179 kg | $1.73M | 19,600 | 389 |
| Hydrolox StagedComb (RS-25-ish) | 8/9 | 126 kN | 184 kg | $2.06M | 16,300 | 522 |
| Methalox FullFlow (Raptor-ish) | 9/9 | 910 kN | 1,546 kg | $4.00M | 4,400 | 522 |
| Hypergolic PressureFed (AJ10-ish) | 5/5 | 27 kN | 69 kg | $0.98M | 36,300 | 120 |
| Solid PressureFed | 5/5 | 300 kN | 765 kg | $1.27M | 4,230 | 120 |

Scale sweep (kerolox GG): $1.52M at 0.25× → $6.61M at 4×. Per-kN falls
from $6.8k to $1.8k purely because flat labor amortizes over more thrust.

**Total spread of the whole liquid-engine space: ~$1M–$4.5M (4.5×).**
Design-day spread after Task 3: 120→679 days (5.7×), and complexity also
drives flaw count, so calendar spread including test/revise cycles is
larger than that.

## 3. Real engines (web-verified 2026-07)

| Engine | Cycle/prop | Thrust | Unit cost | $/kN |
|---|---|---|---|---|
| Raptor 2 | FFSC methalox | 2,256 kN | ~$1M (target below) | ~400 |
| Merlin 1D | GG kerolox | 845 kN | ~$1M (est. up to $3.5M) | ~1,200 |
| BE-4 | ORSC methalox | 2,400 kN | ~$8M | ~3,300 |
| RD-180 | SC kerolox | 3,830 kN | ~$12–25M | 3,000–6,500 |
| RL10 | Expander hydrolox | 110 kN | $17–20M (refs to $38M) | ~160,000 |
| RS-25 | SC hydrolox | 1,860 kN | $100–146M | 54,000–78,000 |

Real spread: **~150× in unit cost, ~400× in $/kN** — and it's *not*
mostly size. The two big real drivers are:

1. **Hydrogen.** RL10 costs ~20× a Merlin at 1/8 the thrust. Hydrolox
   hardware (deep-cryo everything, brazed stainless nozzles, hydrogen
   embrittlement) is the single biggest real cost divider.
2. **Manufacturing philosophy / production rate.** Merlin and Raptor are
   cheap because they're mass-produced by the hundreds; RS-25 is
   hand-built in single digits. The game already models this direction
   correctly via the learning curve — it's the one axis where we're
   structurally right.

Dev-time spread (from the verified §2 tables in m4_plan.md): ~4 years
(Merlin, RL10) to 8–9 years (F-1, RS-25, Raptor-class). Task 3's design
exponent now gives ~1.6× → ~5.7× work spread — directionally right.

## 4. Gap analysis

| Axis | Game today | Reality | Verdict |
|---|---|---|---|
| Cycle premium (GG → SC/FFSC, same prop) | 1.4–1.8× | ~3–10× (Merlin→RD-180 per kN ~3–5×) | Too flat, but not absurd |
| Hydrolox premium | *Cheaper* in absolute $; ~5× per kN | ~20× absolute, ~100× per kN | **Biggest gap.** BOMs differ by 11%; reality differs by an order of magnitude |
| Size scaling of unit cost | Linear materials + flat labor | Roughly linear-ish in thrust within a family | Acceptable |
| Size scaling of dev time | **None** | F-1's size *was* its dev problem (combustion instability); big engines take longer | Missing term |
| Complexity → dev time | 5.7× spread (Task 3) | ~2–3× calendar (4yr → 9yr) | Good now |
| Production learning | builds^-0.15 | Merlin/Raptor vs RS-25 story | Structurally right |
| Absolute anchors | $1–4.5M | $0.3M–$146M | Compressed by design — game vehicles must cost 40–60% of $15–400M bids, so we can't host a $100M engine; the *relative* spread is what's fixable |

Kerolox is actually well-anchored: game GG at $2.8k/kN vs Merlin $1.2k
(within the estimate range), game SC at $3.5k/kN vs RD-180 $3–6.5k —
nearly exact. The distortions are hydrogen (way too cheap) and, to a
lesser degree, top-cycle premium (too flat).

## 5. Candidate levers (data-driven, for a future pass)

1. **Per-preset materials that actually differ.** Hydrolox BOM shifts
   hard toward superalloys/plumbing, or a per-preset `material_multiplier`
   in the BOM (e.g. hydrolox ~3–4×). Ripple: DinoSoar's hydrolox heavy
   gets more expensive to build (its bids track cost automatically —
   Task 2 showed this works), player upper stages cost more, and the
   hydrolox Isp advantage finally has a real price tag. Today hydrolox
   upper stages are strictly great; this is also a quiet Task 4 lever.
2. **Per-cycle labor multiplier** on build days (PF 0.8× … FFSC ~2×),
   distinct from complexity: complexity says "hard to design without
   flaws", the cycle multiplier says "precision machining per unit".
   Alternative: steepen `engine_build_complexity_exponent` — but Task 3
   measured that starving DinoSoar (c12 booster) at 2.5, so a per-cycle
   table is safer than a global exponent.
3. **Size term on engine build work**: `× (mass/1000kg)^q`, q ≈ 0.5–0.7.
   Kills the free amortization that currently makes 4×-scale engines the
   cheapest per kN — directly relevant to the "fewer, bigger engines"
   Task 4 question.
4. **Size term on design work**: `× (scale)^r`, r ≈ 0.3–0.5, so a 4×
   engine costs ~1.5–2× the dev of a 1× (F-1 story) instead of nothing.
5. Leave absolute anchors alone — Task 2 just landed the cost/payment
   ratio; all levers above should be swept holding the *fleet average*
   unit cost roughly constant (e.g. hydrolox up, small-engine labor
   down).

## 6. Questions

- **Q1:** Is the hydrolox premium (lever 1) the priority? It's the
  biggest realism gap and doubles as an engine-balance lever for the
  GTO/GEO struggle (hydrolox uppers stop being free performance).
  USER:  Yes, I think that's pretty important.
  CLAUDE: Made it Task 4a in m4_plan.md. Note it needs a per-preset
  material *multiplier*, not just BOM fraction shifts — fractions alone
  cap out around 1.7× because superalloys are cheap per kg. The DinoSoar
  ripple (+~$20M on its unit cost at 3.5×) is called out there; its
  catalog/margins re-sweep in the same change.
  CLAUDE (post-implementation correction): the ~100×-per-kN RL10-vs-
  Merlin figure in §3 over-generalizes. RS-68 — a hydrolox booster
  *designed to cost* — runs ~$20M at 3,137 kN ≈ $6.4k/kN, only ~5× a
  Merlin; RL10's price is mostly its hand-brazed 1960s process at
  ~1 engine/month (20 months per combustion chamber; the 3D-printed
  RL10C-X reaches ~1/week), which is the learning curve's territory.
  Final landed value: hydrolox 3.0× materials-only (no labor premium,
  teams stay unspecialized), giving ~3-6× kerolox per kN — the
  RS-68-grounded band. Full story in m4_plan.md Task 4 results.
- **Q2:** Size terms (levers 3–4): fold into the Task 4 "fewer, bigger
  engines" measurement, or keep as a separate later pass? They change
  the same trade-off Task 4 is about to measure, so doing Task 4's
  measurement first would give a before/after.
  USER:  Yes, lets fold that in.  Task 4 never made it to the m4_plan.md so
  let's put things there to work out.
  CLAUDE: Done — "Task 4 plan" section added to m4_plan.md with
  sub-parts 4a–4f: hydrolox premium, cycle premium, size terms, the
  big-vs-clustered measurement protocol (run before and after the cost
  levers), improvement escalation, and the re-baseline. Three open
  questions (Q4a–Q4c) await your answers there.
- **Q3:** Per-cycle labor multiplier vs per-cycle BOMs — preference for
  where the cycle premium lives (labor days vs material $/kg)? Labor
  shows up in build queue time too (slower to build an SC engine),
  which is realistic but touches production pacing; materials are purely
  monetary.
  USER:  I think both an increase in complexity and an a more expensive BOM?
  CLAUDE: One caution folded into the plan (4b): Task 3 already made
  complexity superlinear, so complexity now carries a real dev-time and
  flaw-count premium per cycle — bumping SC/FF complexity further would
  tax those a second time on top of Task 3's stretch. Proposal in
  m4_plan.md: the unit-cost premium lives in a per-cycle material
  multiplier first, and the SC/FF complexity bump (8→9, 9→10) is the
  fallback sweep point if the big-vs-clustered measurement (4d) shows top
  cycles still dominating. Q4b there asks whether you're OK with that
  ordering.

Sources: RS-25 [SpaceNews](https://spacenews.com/aerojet-rocketdyne-defends-sls-engine-contract-costs/),
[AmericaSpace](https://www.americaspace.com/2020/05/02/nasa-orders-18-more-rs-25-engines-for-sls-moon-rocket-at-1-79-billion/);
RL10/RD-180 [Motley Fool](https://www.fool.com/investing/2018/09/23/are-aerojet-and-blue-origin-rocket-engines-worse-t.aspx),
[SpaceNews](https://spacenews.com/41047questions-about-rd-180-price-provide-new-ammo-for-spacex/);
Merlin/Raptor [Everyday Astronaut](https://everydayastronaut.com/raptor-engine/),
[NextBigFuture](https://www.nextbigfuture.com/2019/05/spacex-raptor-engine-will-be-best-on-cost-and-nearly-best-on-isp.html).
