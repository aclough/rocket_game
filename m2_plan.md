# M2 Plan — Seeded Markets & Contract Character

> Working plan for ROADMAP.md milestone M2. Same review process as M1:
> USER: comments inline, CLAUDE: replies, then task-by-task execution
> with approval before each commit.

## Goal

Make contract streams differ from each other and from run to run,
while two mainstay markets stay dependable across all seeds:

- **Rideshare / Smallsat (LEO)** — the initial target. Visible at
  reputation 0, guaranteed floor, never thinned by seed variance.
- **GEO Communications (GTO/GEO)** — the expensive, well-established
  (as of 2000) comsat business. Stable across seeds; still gated
  behind reputation 50, so it's the tier-2 goal a young company
  climbs toward rather than a day-1 option.

Everything else — the other six markets, plus anything new — becomes
seed-varied: whether it appears, when, how big, how it grows, and
what its contracts feel like.

**DECIDED:** GEO keeps its `min_reputation: 50` gate — nobody trusts
a $100M comsat to a company that's never delivered to orbit — but the
market itself is reliably present across playthroughs as the tier-2
revenue goal.

## What exists today (orientation)

- `Market` structs live in `BalanceConfig.markets` (`initial_markets`
  + `event_market_templates`), cloned into `GameState.markets` at
  start. Volume, rates, destinations, `min_reputation`,
  `economy_sensitivity` are all data already.
- Market **emergence** (COTS, constellations, NSSL, Earth obs) is a
  hardcoded table inside `GameState::check_market_events()` — per-key
  `world_query` rolls for trigger + year, plus cross-effects
  (constellations suppress GEO). LEO/MEO constellations are mutually
  exclusive.
- Deadlines (60–180 days) and payment variance are **global** in
  `MarketsConfig`, not per-market.
- Contract failure/expiry penalties are **reputation-only** (global
  values in `ReputationConfig`); no monetary penalty.
- Monthly generation: `count = (effective_volume + rng.gen()) as u32`
  — a uniform fractional draw, same shape for every market.
- Economy: seeded Markov chain (Boom→…→Recession), markets scale by
  `EconomySensitivity`. This is already per-seed variance; M2 layers
  market-level variance on top.
- M1 guardrails: seed-fairness floor (≥2 achievable year-1 contracts,
  ≥$5M value, all 200 seeds), metric bands, byte-determinism test.

## Design overview

One new concept: a **`MarketArchetype`** — a market template plus a
*perturbation spec*. At game start, each archetype is **realized**
into a concrete `Market` via `world_query("market_<key>")` draws:

```
MarketArchetype {
    template: Market,               // exactly today's struct
    presence_probability: f64,      // 1.0 = every seed
    emergence: Option<EmergenceSpec>, // window + flavor + cross-effects
    volume_mult_range: (f64, f64),  // drawn once per seed
    rate_mult_range: (f64, f64),
    annual_growth_range: (f64, f64),  // Task 2
    weight_tilt_strength: f64,      // destination-weight jitter
    cadence: CadenceSpec,           // Task 4
    character: ContractCharacter,   // Task 3: deadlines, penalties
}
```

Consequences:

- The hardcoded `check_market_events()` table dissolves into
  archetype data (`EmergenceSpec { probability, year_range, flavor,
  cross_effects, excludes }`). One source of truth, TOML-sweepable,
  per CLAUDE.md's no-hardcoded-configurables rule.
- **Mainstays are just archetypes with pinned draws**: presence 1.0,
  volume/rate ranges of exactly (1.0, 1.0). The "stable" guarantee is
  data, and a test asserts it. Their `annual_growth_range` (Task 2)
  is still allowed to vary per seed — mainstays start identical
  everywhere but their long-run trajectories diverge.
- **Additive-only year-1 rule** is enforced structurally: any
  archetype visible at reputation 0 must have `volume_mult_range.0
  >= 1.0` and `rate_mult_range.0 >= 1.0` (validated at config load,
  so a TOML sweep can't accidentally break it), plus the existing
  fairness-floor test as the behavioral backstop.
- Realized `Market`s are what get saved (same as today), so save
  compatibility is only additive serde fields.

**DECIDED:** Mainstays are literally fixed at (1.0, 1.0) for volume
and rate — if we pin them, pin them. Seed-to-seed differences in the
mainstays come only through `annual_growth_range`, giving different
long-range outcomes from identical starting points.

## Tasks

### Task 1 — `MarketArchetype` + seeded realization

The structural core; everything else hangs off it.

1. Add `MarketArchetype`/`EmergenceSpec` to `balance_config.rs`;
   replace `MarketsConfig.initial_markets`/`event_market_templates`
   with `archetypes: Vec<MarketArchetype>` carrying today's eight
   markets and the emergence table currently in
   `check_market_events()`. Defaults reproduce current values.
2. `realize_markets(seed, &archetypes) -> Vec<Market>` in
   `contract.rs`: presence roll, volume/rate multiplier draws,
   destination-weight tilt, per-archetype `world_query` keys.
3. Rewrite `check_market_events()` to iterate archetype emergence
   data instead of its local table (behavior-preserving for default
   config; verified by determinism + band tests).
4. Config-load validation: rep-0-visible archetypes must have
   multiplier floors ≥ 1.0 (additive-only rule).
5. Tests: mainstay markets byte-identical at game start across 50
   seeds;
   varied markets actually differ across seeds; presence probability
   roughly honored over 200 seeds; fairness floor still green.

This is the fundamental-data-structure change of the milestone —
flagging it per CLAUDE.md. The `Market` struct itself doesn't change
in Task 1; only where markets come from.

### Task 2 — Volume growth over time

Markets currently have static `base_volume` forever. Add seeded
`annual_growth` (drawn from `annual_growth_range`), applied in
`effective_volume()` from the market's activation date. Rideshare
grows (the smallsat wave), GEO mature/flat-to-slightly-declining,
constellations grow fast. Growth compounds the additive-only rule
safely as long as mainstay growth floors are ≥ 0 for rep-0 markets.

Harness step: 20-year runs to eyeball late-game volume sanity
(contract counts shouldn't explode past what the UI/list handles).

### Task 3 — Contract character axes

Per-market deadline tightness and penalty severity:

1. Move `deadline_min_days`/`deadline_max_days` into per-market
   fields (global values remain as defaults). Gov't science: long
   deadlines. Commercial GEO: moderate. Rideshare: tight-ish.
2. Per-market penalty severity multiplier applied to the existing
   reputation penalties (gov't lenient on expiry but gated;
   commercial quick to punish).

**DECIDED:** Reputation-only in M2 — forfeit complexity isn't worth
the UI squeeze. *Carried to M3:* once bidding exists, forfeits should
impact the player's future cashflow through the bid/award layer
rather than as a separate fee system.

**DECIDED:** Per-market `failure_severity` lands here, with the
COTS/crew market notably less forgiving (the cheap version of
market_ideas.txt's "missions involving people"); the full
reliability/excitement reputation split stays deferred.

### Task 4 — Cadence personality

Replace the one-size monthly draw with per-market cadence:

- **Steady** — current behavior (uniform fractional draw).
- **Lumpy** — same long-run volume, higher variance (some months 0,
  some 2–3). Rideshare, Earth obs.
- **Burst** — quiet stretches, then a seeded batch window (several
  contracts in 1–2 months). Constellations especially.

Implementation: `CadenceSpec` enum on the market, consumed inside
`generate_market_contracts`. Long-run volume must be conserved —
test: 10-year contract count per market within a few percent of
`base_volume × months` (growth-adjusted), for each cadence type.

### Task 5 — Anchor customers / campaigns

Seeded named programs emitting correlated contract series: a
`Campaign` realized at game start (or on market emergence) with a
customer name, fixed payload class + destination, cadence (e.g.
every 2–4 months), total mission count, and a block-buy discount on
rate. Contracts carry the campaign name ("Meridian Constellation
Flight 3"). Pairs with manufacturing learning curves: winning a
campaign rewards building the same rocket repeatedly.

Scope guard: campaigns are *offered contracts with shared parameters*
— no new award mechanics (that's M3), no exclusivity. If the player
skips Flight 3, Flight 4 still shows up.

**DECIDED:** No guaranteed starter campaign — campaigns are fully
seed-varied in which ones show up and what they look like, keeping
each runthrough unique. (Rideshare campaigns may still *happen* to
appear in year 1 on some seeds; additive-only means that can only
raise the floor.)

### Task 6 — Discovery rule, UI, and harness re-baseline

1. Audit the contracts/market UI: only realized contracts and
   observed history — no seeded parameters (growth rates, presence
   rolls, multiplier draws, future emergence) may leak. Today's UI
   shows market names + modifier descriptions, which is fine; keep it
   that way and add a headless render test.
2. Re-measure and update M1 guardrails **with variance live**:
   re-run `measure_year1_distribution` (floors should hold or rise —
   additive-only), re-run the 200-seed bands, update
   `sim_bands.rs`/`seed_fairness.rs` baselines in the same change.
3. New permanent test: 200-seed sweep asserting the additive-only
   property directly — for every seed, achievable year-1 value ≥ the
   pinned-mainstay-only baseline.

## What's deliberately NOT in M2 (mapping market_ideas.txt)

| Idea | Where it lands |
|---|---|
| Reputation split (reliability vs excitement) | Later/M4 — fundamental reputation change; M2 gets per-market severity only |
| Crew-mission unforgivingness | Cheap version in Task 3 (per-market failure severity) |
| Tourism (suborbital→lunar) | Later (already a Later loop) |
| Military markets + geopolitical tension metric | Later — tension is a new world-state system; NSSL already covers the MVP military slice |
| Space pharma, solar power, compute | Later — candidates for post-MVP archetypes (cheap to add once archetypes are data) |
| Telecoms LEO/MEO | Already exist (constellation markets); Task 1 makes them archetype-driven, Task 4 makes them bursty |
| Solar flares / war / demand events | Later — event-driven modifiers already have the `MarketModifier` hook when we want them |
| Interest rates + debt | Later (loans/financing) — economy stays a single health metric for MVP |
| Demand curves responding to price | Post-M3, as noted in the file itself |

One thing M2 *does* take from the noodlings: the archetype table
makes adding any of these later a data entry plus flavor, not a code
change. That's most of the point.

## Sequencing & estimates

Task 1 is the prerequisite for everything; 2–5 are independent of
each other after it; 6 closes. Rough effort: Task 1 is the big one
(~like M1's Task 3), Tasks 2–4 are small, Task 5 medium, Task 6
small. Fits the roadmap's 1–2 weeks.

Per-task: tests green (394 + new), fairness floor + determinism +
bands checked before each commit, commit only on your approval.

## Decisions (all questions resolved 2026-07-13)

1. GEO mainstay keeps its rep-50 gate; market reliably present across
   playthroughs as the tier-2 revenue goal.
2. Mainstays literally fixed at (1.0, 1.0) volume/rate; only
   `annual_growth_range` varies for them (long-run divergence).
3. Penalties reputation-only in M2; forfeits enter through future
   cashflow once M3 bidding exists.
4. Per-market `failure_severity` yes; crew/COTS notably harsher.
5. No guaranteed starter campaign — campaigns fully seed-varied.
