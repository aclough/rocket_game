# NRO launches, Great Power War, and ASAT

## First: the market already exists

`market_nssl` ("National Security") is already the highest-reputation,
least-price-sensitive market in the game — rep target 80, `w_cost` 0.35,
budget tolerance 1.4, `EconomySensitivity::None`, failure severity 1.5. Its
description is literally "Defense and intelligence satellite launches.
Irreplaceable payloads; failures draw hearings."

Present in 44% of worlds, opens 2010–2018, base volume 0.3/month, `Lumpy`
cadence, $60–150k/kg across LEO / GTO / GEO / SSO.

So this is a retune-and-rename rather than a new market, unless you want NRO
*separate* from the broader defence launch business. See Q1.

## What the existing machinery can and can't do

**Good news: per-market effects already exist.** `MarketModifier` carries a
`volume_mult` and `rate_mult` with an optional `end_date`, deduplicated by id,
with `add_modifier` / `expire_modifiers` already wired. The constellation
markets already use it to permanently suppress GEO when they emerge. So the
war's *effects* need no new mechanism — they are modifiers applied to markets.

**Bad news: the economy chain can't express this.** `EconomicCondition` carries
a single global `modifier` scalar, and each market maps it through
`EconomySensitivity` (None / Low / Moderate / High). Every variant is monotonic
in the modifier — there is no inverted one — so "everything down, NRO up" is not
expressible as an economic condition. `None` makes NRO *immune*, not *boosted*.

**Worse: the timing models don't match.** The economy transitions when a state
expires, with a duration in months drawn at entry (Recession 3–8, Normal 12–36).
Your war is an annual hazard process: 2%/yr to start, 50%/yr to end, 40%/yr to
escalate. Folding that into the chain means:

- every existing condition needs a war transition probability, and `transitions`
  must sum to 1.0 (there is a test);
- "2% per year" becomes "x% per transition", and transitions land every 3–36
  months depending on what the economy is doing — so the effective annual war
  probability would swing by a factor of ten based on an unrelated system.

That last one is the killer. I do not think war belongs in `EconomicCondition`.

## Proposed shape

A **separate geopolitical state machine**, rolled annually, running alongside
the economy rather than inside it.

```rust
enum Geopolitics {
    Peace,
    War { since_year: u32 },
    WarWithAsat { since_year: u32, asat_since_year: u32 },
    Reconstitution { until_year: u32 },   // the post-ASAT replacement boom
}
```

Rolled once per game-year from `world_query(&format!("geopolitics_{year}"))`, so
the whole timeline is fixed at game start and save-scum-proof, exactly like the
economy. Transitions:

| from | roll | to |
|---|---|---|
| Peace | 2% | War |
| War | 40% | WarWithAsat |
| War | 50% | Peace |
| WarWithAsat | 50% | Reconstitution |

Effects are `MarketModifier`s applied on entry and removed on exit — no new
mechanism, and the UI already renders modifiers.

| state | National Security | everything else |
|---|---|---|
| War | volume ×5 | — |
| WarWithAsat | volume ×5 | volume ×0.2 |
| Reconstitution | — | volume ×2 for N years |

## How often you would actually see this

| | over 5 years | over 15 years |
|---|---|---|
| any war | 9.6% | 26% |
| escalates to ASAT | 5.5% | 15% |

A war lasts a geometric 50%/yr, so the mean is 2 years. Given a war, the chance
it escalates is `0.4 / (1 − 0.6 × 0.5)` = **57%**.

**So ~85% of games never see ASAT, and ~74% never see a war at all.** That is a
lot of machinery for content most players never meet. Worth deciding
deliberately rather than by default — see Q3.

---

## Feedback / concerns

**The war is all downside for most players, and that may be the point.** During
`WarWithAsat` every market except National Security is at 20% volume. National
Security needs reputation 80 — the highest bar in the game. A player who has not
got there watches their entire business evaporate with no compensating work.
That is a *strong* strategic argument for building reputation early, which is
good design, but it is also a potential run-ender arriving from a 2% roll the
player could not have seen coming. Two softeners worth considering:

- Wartime urgency *lowers* the National Security rep bar (they need lift more
  than they need a spotless record). Realistic, and it opens the market to the
  player who most needs it.
- ×0.2 on everything is very deep. ×0.4 still reshapes the game without
  guaranteeing a bankruptcy.

**ASAT debris is an orbit problem, not a market problem.** In reality a debris
cascade wrecks LEO and SSO and barely touches GEO. `MarketModifier` is
market-wide, so the honest version needs per-destination modifiers — more
machinery. The cheap approximation is to hit the LEO-heavy markets (both
constellations, Earth Observation, Rideshare) hard and GEO Communications
lightly, which lands in roughly the right place without new types.

Note the irony worth keeping: NRO's own reconnaissance satellites are mostly
LEO/SSO, so an ASAT exchange destroys exactly what they most need replaced —
which is *why* their launch demand goes up.

**Reconstitution should probably scale with the damage.** A three-month ASAT
exchange and a four-year one shouldn't produce the same replacement boom.
Simplest version that isn't silly: boom duration = ASAT duration, magnitude
fixed.

**Mutual exclusion.** Your instinct is right that War + Boom reads oddly, though
"defence spending booms during a war" is not actually absurd. The cheapest fix
is to suppress Boom while at war rather than to merge the two state machines.

---

## Questions

**CLAUDE Q1 — is NRO a rename of the existing National Security market, or a
second one alongside it?**
Real-world NRO payloads fly *on* NSSL contracts, so they are the same
procurement pipeline rather than two customers. My preference is to retune and
rename the existing market — it is already tuned exactly as you described, and a
second near-identical market would split the volume without adding a decision.
If you want them separate, the distinction worth drawing is that NRO is
recon (LEO/SSO, heavy, frequent) while NSSL proper is comms and early warning
(GTO/GEO).

USER:  Lets do a return and rename then.

**CLAUDE Q2 — the war multiplies National Security volume ×5 from a base of
0.3/month.** That is 1.5 contracts/month, against a peacetime total across all
markets of roughly 3–4/month. So a war roughly doubles total available work
while ASAT then removes most of the rest. Does ×5 sound right, or should the
base volume rise and the multiplier fall?

USER:  I think that sounds right.  It should be really dramatic.

**CLAUDE Q3 — 2%/yr means 74% of games never see a war at all.** Is that the
intended rarity? Options:

(a) Keep 2%/yr — a rare event that defines the games it appears in.
(b) 5%/yr — about half of 15-year games see one.
(c) Keep 2%/yr but guarantee the *possibility* is visible: rising tension
    reported in the events feed, so players know the mechanic exists.

USER:  Keep it at 2%.

**CLAUDE Q4 — should the market suppression be ×0.2 as specced, or gentler?**
See the concern above: ×0.2 with a rep-80 gate is a plausible run-ender for a
mid-game player who cannot reach the only market still buying.

USER:  Most users will have enough runway to survive, most of the time.  Maybe
we should just hit LEO and SSO launches though.  Also, we should open up the
NSSL market if it hasn't already triggered "War prompts DoD to open NSSL bidding
to previously excluded entrants" or something.  And we can reduce the rep
requirements somewhat while the war is ongoing.  Maybe down to 60?

**CLAUDE Q5 — does war affect *rates* as well as volume?**
A desperate customer pays more. Raising National Security's `rate_mult` during
the war would make the surge lucrative rather than merely busy, and would give
a well-prepared player a genuine windfall to offset the crash elsewhere. Not in
your spec; worth a line either way.

USER:  Good point, lets do that

**CLAUDE Q6 — should the war be visible in the economy UI, or its own thing?**
The economy pane currently shows one condition. A war is not an economic
condition under this design, so it needs somewhere to live — either a second
line in the same pane, or folded into the events feed only.

USER:  Lets add a new line while the war is ongoing

---

## Outcome

Built as agreed. `market_nssl` retuned and renamed to **National
Reconnaissance**; the archetype *key* stays `market_nssl`, since changing it
would reshuffle every existing seed's market presence. The retune is the
destination mix — recon flies low and polar, so LEO 0.3→0.4 and SSO 0.25→0.35
against GTO 0.15 and GEO 0.1. That weighting is load-bearing, not flavour: it
makes this the customer hit hardest by an ASAT exchange, which is why its
demand surges.

`src/geopolitics.rs` holds the state machine, rolled each January from
`world_query("geopolitics_<year>")`. Measured over 2,000 worlds: **25.7% see a
war, 14.8% see ASAT**, against the predicted 26% / 15%.

**The one new mechanism** is `MarketModifier::destination_volume_mult`, which
"just hit LEO and SSO" required — a market-wide multiplier cannot say that. One
per-destination number produces both consequences that ought to follow: a
market's total volume falls by exactly the affected orbits' share of its weight
(`Market::destination_share`), and the contracts that remain skew toward orbits
that still work (`pick_destination`). LEO Constellation is all LEO/SSO and loses
80%; GEO Comsats fly GTO/GEO and lose nothing. No special-casing.

Also added: `rep_target_delta` for the wartime bar, and `Default` for
`MarketModifier` so construction sites name only the fields they mean.

### The balance guard, and why it moved

`sim_bands`' 20-seed smoke check went red at 2/20 bankrupt. Isolated by zeroing
`WAR_START_CHANCE`: the retune alone gives 1/20, the war adds seed 11. But the
**200-seed guard passes** — 8/200 = 4.0% against a 1–6% band and a 3.0%
baseline, with ever-profitable unchanged at 196/200.

The 20-seed ceiling was statistically unsound regardless of this change: 6% on
twenty seeds means "at most one bankruptcy", which at a 4% true rate fails 19%
of the time. Made size-aware, mirroring the floor treatment that was already
there for the same reason.

### Seed 11: the softener is the trap

Worth recording, because it is the opposite of the failure mode this plan
predicted. From the event log:

```
Jan 1, 2002  Great Power War — National Reconnaissance Market Open
Sep 22, 2002 BidPlaced   "KEYHOLE Follow-on to LEO"  $38.59M
Oct  2, 2002 ContractAwarded "KEYHOLE Follow-on to LEO"
Oct  3, 2002 LaunchFailure "Flight computer resets during staging
                            transient (+2 more) — 36% delta-v shortfall"
```

Reputation 20 → −55 in one launch, because `on_launch_failure` scales both
penalty terms by the market's `failure_severity` and National Reconnaissance
carries 1.5, the highest in the game. Negative reputation then gates every
market shut, so the bot spends the rest of the run flying test masses to
rebuild: ten launches, eight successes, **zero contracts completed**, bleeding
salary until the debt limit.

The company had reputation 20 against a normal bar of 80 and could never have
touched this market. Dropping the bar to 60 is what let it in, carrying nine
hidden flaws — three activated at once and ate 36% of the delta-v. So the war
does not hurt the player by removing work, as this plan assumed. It invites an
unready company into the highest-consequence market in the game.

The 36% shortfall came from flaw activation, not from bidding beyond
capability — the payload gating worked. The vehicle simply was not reliable
enough for a customer that punishes failure this hard.

**Decision: left as-is.** "Don't fly reconnaissance work on an unproven
vehicle" is a real lesson, and the 200-seed guard is comfortably in band. The
alternative considered was to soften `failure_severity` during the war instead
of the reputation bar — keeping the access without the amplifier — which is the
first thing to reach for if this turns out to bite in play.
