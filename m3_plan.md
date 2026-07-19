# M3 Plan — Competitive Bidding + DinoSoar

> Working plan for ROADMAP.md milestone M3. Same review process:
> USER: comments inline, CLAUDE: replies, then task-by-task execution
> with approval before each commit.

## Goal

Contracts stop being take-it-or-leave-it payments and become sealed
bids the player prices. One competitor — **DinoSoar**, the incumbent
heavy-lift dinosaur — bids against you, wins things, launches, and
has a reputation that moves. Pricing becomes a real decision:
underbid to build reputation, price high where you're unopposed, and
discover demand by probing (elasticity-as-discovery comes free).

Absorbs two standing TODOs (player-chosen pricing, auto-bid at
margin) and the M2 Task 3 decision that forfeits hit the player
through *future cashflow* — with reputation in the award scoring,
an expiry today literally costs you awards tomorrow, no separate
fee system needed.

## What exists today (orientation)

- Contracts are generated with a fixed `payment` (rate × payload ×
  variance) and sit in `available_contracts`; the player accepts by
  index; payment lands on delivery. No pricing decision anywhere.
- Campaigns issue missions at a block-buy price locked at
  announcement.
- `CompanyPolicy` trait drives the harness bot — built in M1
  explicitly so it could later become DinoSoar's brain.
- Reputation is a single player-company score; markets gate
  visibility on it. Failure severity (M2.3) scales reputation hits
  per market.
- The M1/M2 guardrails: 200-seed bands, year-1 fairness floor,
  additive-only property test, byte-determinism.

## Design overview

### The award mechanic

A generated contract is now a **solicitation**: it carries a
`bid_deadline` (drawn from a per-market window — a new character
axis, like M2.3's delivery deadlines: gov't long, commercial short)
and a hidden **budget ceiling** (the old fixed payment × a per-market
tolerance). The player may submit one sealed bid before the deadline.
At the deadline, all bids are scored and the best wins:

```
score = w_cost × (budget / bid) + w_rep × (reputation / 100)
```

USER:  This formula has a number of good points but I'd like the outcome to look
more like there's some target reputation and it matters a lot if you're under
it, it matters a moderate amount if you're near it, and it doesn't matter a lot
if you're well over it.  Something in the vauge direction of rep / (target
+ rep) but normalized appropriately.  For cubesats it can even be ok if the
  target is negative.  Though that reminds me we should also fix this to handle
  negative numbers as well as we can.  A different equation that does a similar
  thing may work better.

**CLAUDE:** Proposal — a logistic curve around a per-market target:

```
rep_factor = 1 / (1 + exp((rep_target − rep) / rep_scale))
score      = w_cost × (budget / bid) + w_rep × rep_factor
```

Properties: at rep = target the factor is 0.5; well below target it
decays toward 0 (you must compete purely on cost); well above it
saturates toward 1 (surplus reputation stops mattering). It needs no
special-casing anywhere on the number line — negative rep and
negative targets just work (a cubesat market can set
`rep_target = −10`, so a rep-0 startup is already comfortably "above
target"). `rep_scale` controls how wide the "near target" band is
(~10 rep points feels right); I'd make `rep_target` per-market data
and keep `rep_scale` global to hold the data surface down — going
per-market later is a one-line change if some market needs a sharper
cliff.

One interaction to be explicit about: `rep_target` is a *scoring*
input only. The existing `min_reputation` *visibility* gate stays
untouched (whether you see GEO solicitations at all is still the
M2 discovery rule; how you score once bidding is this curve).
Sound right?

USER:  We might actually want to get rid of that entirely.  It would be good to
have some other UI clue that the player's reputation isn't up to what is
required though which drives off the rep_target.  Like maybe if they're below
80% of rep_target there's a TUI indication.

**CLAUDE:** Can do — `rep_target` fully replaces `min_reputation`
(the field is only used in two gates: contract generation and
campaign spawns, the latter dormant once campaigns are off; the
reactor MEU/HEU enrichment gates are a separate config and stay).
Plan:

- Every *active* market generates visible solicitations regardless
  of reputation. Old `min_reputation` values become the first-guess
  `rep_target`s (GEO 50, NSSL 80, etc.); rideshare's goes slightly
  negative so brand-new companies sit comfortably above it.
- TUI indicator per your rule: rep below 80% of `rep_target` marks
  the solicitation (e.g. a colored "rep" tag) meaning "you can bid,
  but you're not who they're looking for." The 80% rule behaves
  sensibly for negative targets too (it only flags deeply negative
  companies). It reveals only static market character, not seeded
  internals, so the discovery rule survives.
- The opening-floor validation rule re-anchors: opening-floor market
  = start-active && `rep_target <= 0` (was `min_reputation <= 0`).
- Two knock-on effects, both absorbed by Task 1's re-baseline:
  year-1 offering grows a lot (GEO mainstay solicitations are now
  *visible* from day 1 — you'll see $100M comsat work you can't
  win yet, which is good aspirational texture but changes every
  measured number), and the additive-only test's premise ("at rep 0
  only rideshare draws from the stream") is gone — that test gets
  rewritten around the new offering shape or folded into the
  winnable-floor test.

One check: you're OK with day-1 visibility of gov/GEO solicitations
(with the rep tag) as the intended experience? It's a real feel
change from M2's "markets unlock as you prove yourself."

`w_cost`/`w_rep` are per-market data (new archetype fields):
gov science and NSSL reputation-heavy, rideshare and constellations
cost-heavy, GEO in between. Bids above the budget ceiling are
rejected outright — even unopposed. That ceiling is the seed of
demand elasticity (full price-responsive demand curves stay post-M3,
per market_ideas.txt).

Outcomes the player observes (this is the discovery loop):
- **You win**: contract moves to `active_contracts` with
  `payment = your bid`.
- **You lose**: event log reports "Awarded to DinoSoar at $42M" —
  losing teaches you the going rate.
- **Nobody wins** (all bids over budget / no bids): solicitation
  lapses quietly.

Two deliberate consequences:
- **A negative-reputation company can still win cost-heavy work by
  lowballing.** This softens the M1 reputation-death-spiral finding:
  bidding is the recovery path the old accept-model lacked.
- **Campaign missions do NOT go through bidding.** The block price
  was locked at announcement; missions stay take-it-or-leave-it
  guaranteed-price work. This keeps campaigns as the "steady
  baseline income" counterweight to bid-market volatility, and it
  keeps M3's scope sane. (Bidding *for* campaign blocks at
  announcement is a natural follow-on — see Not-in-M3.)

**CLAUDE:** Confirm the campaign call above? The alternative —
sealed-bid the whole block when a program is announced — is more
coherent long-term but roughly doubles the milestone's UI and award
surface. I recommend pre-priced missions for M3, block-bidding as a
follow-on.
USER:  I think we don't want campaigns to work like that.  In the playtest of M2
the campaigns didn't pop enough as interesting things coming up.  Maybe we turn
campaigns off for now but then do te block bid thing later?

**CLAUDE:** Agreed — playtest verdict noted. New call: **M3 ships
with campaigns off** — every default archetype gets
`campaign = None`. I'd keep the machinery (spawn/issue code,
validation, tests) in the tree rather than ripping it out: the tests
already inject their own rigged specs so they stay green, the
additive-only property test gets trivially stronger, and the code is
the starting point for the block-bid redesign. Bands re-baseline
without campaign income as part of Task 1's re-measure.

When campaigns return as the follow-on, the fix for "didn't pop" is
the block-bid shape itself: an announcement becomes a
high-importance event with a real decision attached (bid the whole
block, win or lose it visibly) instead of quietly adding rows to the
contract list. OK to keep the dormant code rather than deleting?

### DinoSoar

A `Competitor` on GameState (Vec, but exactly one for MVP):

- **Identity**: name, its own reputation (starts high — it's the
  established provider circa 2000), a canned launch success rate.
  Its reputation moves: slow decay toward a baseline, a hit when one
  of its (occasional, visible) launches fails.
- **Cost model, not a simulation**: per-destination $/kg + fixed
  overhead + a margin policy with noise. Its bid on a solicitation is
  `cost × margin ± noise`, sometimes declining to bid at all
  (capacity ceiling per year).
- **It doesn't do smallsats.** DinoSoar only bids where payload ≥ a
  threshold (data-driven, ~1000 kg) and in markets it cares about
  (GEO, gov science, NSSL, COTS — not rideshare, not tiny Earth-obs
  birds). This is both flavor-true (dinosaurs don't chase cubesats)
  and **structurally protects the year-1 floor**: the opening
  rideshare market stays uncontested, so the fairness guarantee
  survives bidding untouched.
- **Visible behavior**: launch events in the news (with occasional
  failures), award announcements, reputation the player can infer
  from its track record. No R&D, no manufacturing, no fleet sim —
  that's all post-MVP per the roadmap.
USER:  I think we want it to be more player like than this.  A fixed set of
engines and rocket with no research that only makes sense for high end missions.
And a manufacturing capacity just like the player has. And then scripting to
only bid high, especially when it doesn't have a full roster of rockets.  That
way the player will essentially always get the smallsats DynoSoar overbids on
and when there's a spurt of demand it can't handle.  How does that sounds.
I want to keep as much symmetry with the player company as is reasonable with
the big divergence in the AI and starting conditions.

**CLAUDE:** That sounds better than what I had — capacity pressure
and smallsat overbidding *emerge* from real constraints instead of
being canned numbers. Revised shape:

- **Fixed catalog, no R&D**: one proven heavy-lift design
  (data-driven: build cost, build days, payload-to-destination
  table). No flaws, no revisions — a canned per-launch success rate
  stands in for the reliability its 40-year heritage implies.
USER:  To avoid adding extra mechanisms, exactly one flaw that destorys the
rocket with what would otherwise be the canned failure rate.

**CLAUDE:** Nice — its fixed design carries exactly one permanent,
loss-of-vehicle flaw whose activation chance is the seeded failure
rate. Since DinoSoar never tests or revises, the flaw is never
found or fixed. Zero new mechanisms, and its launches roll the same
flaw-activation code as the player's.
- **Real manufacturing capacity**: N production lines (N is the
  swept knob per your Q2 answer), each building one catalog rocket
  at a time into a real inventory. Simplified relative to the player
  — no learning curves, no floor space, no team management — but the
  same build-order → inventory shape.
- **Real commitments**: won contracts reserve rockets from that
  inventory; launches consume them on schedule, make the news, and
  (rarely) fail and dent its reputation.
- **Scripted bidding, high by policy**: bid = catalog cost × margin,
  with margin scaling *up* as free stock (inventory − reserved)
  shrinks, and declining to bid at zero free capacity. Consequences
  fall out exactly as you describe: its cost basis on a 200 kg
  cubesat is still a whole heavy-lift rocket, so it structurally
  overbids smallsats, and a demand spurt outruns its line count so
  the player eats the overflow.

The one real architecture decision (flagging per CLAUDE.md): I
recommend **structurally parallel, not shared-pipeline** — a
`Competitor` that owns its own small lines/inventory/backlog state,
ticked in `advance_day`, reusing shared *types* (destinations,
GameDate, contract references) but not threading through the
player's manufacturing teams/learning-curve code. Full pipeline
symmetry (real teams, real learning curves) is the post-MVP
"competitor depth" item. Is that the right symmetry line, or did you
want it literally running the player manufacturing code?
USER:  I literally want it running the same manufacturing code as the player in
the long run.  If too much player specific state isn't stored in the player's
Company right now we'll need to extract it at some point to allow reuse.  How
hard a task is that?

**CLAUDE:** I audited this and the answer is: much easier than I
assumed when I recommended parallel-but-separate. The *state* is
already extracted — `Company` (game_state.rs:109) holds money,
teams, all projects/designs, `Manufacturing` (orders, inventory,
floor space), active contracts, reputation, launch history, cost
history, and auto-build targets. `GameState` holds world state plus
one `player_company: Company`. So DinoSoar can literally be a second
`Company` instance from day one.

What's welded is the *orchestration*: `GameState::advance_day`
hardcodes `self.player_company` (~84 references) while interleaving
the R&D loops, manufacturing tick, and event-log emission. But the
pieces DinoSoar actually needs in M3 are already company-scoped
methods with no GameState dependency:

- `Manufacturing::advance_day(&mut self, costs) -> Vec<ManufacturingEvent>`
  — fully self-contained.
- Auto-build ordering is a `Company` method — DinoSoar's
  "maintain a full roster" behavior is the *existing*
  `auto_build_targets` feature, not new code.
- Reputation lives inside `Company`, so a second moving reputation
  is free.

So the M3-sized job is small: tick the competitor's
`company.manufacturing` and auto-builds each day and route its
events under its name (~a day of refactor, not a rewrite). Its
launches stay abstract in M3 — consume a rocket from its real
inventory, roll the one flaw, emit news — no flight-system
integration. The *full* extraction (moving the R&D project loops
and launch/flight paths off `self.player_company` into Company
methods) is the bigger mechanical job; it pairs naturally with the
standing "split game_state.rs" hygiene item and can happen
incrementally post-M3 without blocking anything. Verdict: same
manufacturing code in M3 is realistic, and Task 2 below is
rewritten to do exactly that.

### Bid automation (standing rules)

The TODO item: "auto-bid on contracts above marginal cost + chosen
margin." One rule engine, two consumers:

1. **Player automation**: per-market standing rule — enabled flag +
   margin %. When a solicitation appears in an enabled market and
   the readiness check passes, the rule computes marginal cost
   (rocket build cost from existing manufacturing cost history +
   launch ops) and files the bid automatically.
2. **The harness bot**: BasicPolicy's bidding brain IS this rule
   engine (single source of truth, per CLAUDE.md). The bot bids
   cost + margin everywhere; DinoSoar prices from its own cost
   model. This keeps the 200-seed harness meaningful under M3.

   USER:  We also need something to prevent auto-bidding with no rockets
   available.  And by available I mean comparing the existing stock of rockets
   minus the rockets required for existing missions to what the new contract
   would require.

**CLAUDE:** Agreed — the rule engine gets a readiness gate:
`free stock = capable rockets in inventory − rockets reserved by
already-awarded, unflown contracts`, and no bid files unless free
stock covers the new contract. I'd start strict (built stock only,
not counting builds that would finish before the deadline) and
loosen later if the harness bot turns out to underbid its own
capacity. Bonus symmetry: this is the same gate DinoSoar uses to
decline at zero free capacity — one mechanism, three consumers
(player rules, bot, competitor).

### Forfeits through the award layer

No monetary forfeit (M2 decision stands). An expired awarded
contract hits reputation (M2.3 severity), and reputation is now in
every award score — so the forfeit's cost is real, delayed, and
proportional to how reputation-weighted your target markets are.
Gov't work: forfeit rarely, it follows you. Rideshare: shrug.

## Tasks

*(Rewritten after the second review round to fold in: campaigns
off, min_reputation → rep_target, logistic scoring, single 30-day
bid window, DinoSoar as a second Company on the real manufacturing
code.)*

### Task 1 — Award mechanic, end-to-end (player as sole bidder)

The data-structure change of the milestone (flagging per CLAUDE.md):

1. Contract gains `bid_deadline: GameDate` (issue date + global
   `bid_window_days = 30`), `budget_ceiling: f64` (hidden from UI),
   `player_bid: Option<f64>`; new status flow Available → (bid
   placed) → Awarded/Lapsed at deadline. Serde defaults chosen so
   pre-M3 saves load: contracts with no bid_deadline behave as
   legacy accept-at-payment.
2. `min_reputation` → `rep_target` on Market/archetypes; every
   active market generates visible solicitations regardless of rep;
   TUI rep-tag on solicitations where rep < 80% of `rep_target`.
   Opening-floor validation re-anchors to `rep_target <= 0`.
3. New market character fields: `budget_tolerance`, `w_cost`/`w_rep`
   (+ validation). Scoring uses the logistic rep curve
   (`rep_target` per market, `rep_scale` global).
4. Campaigns off: every default archetype gets `campaign = None`;
   machinery and tests stay (tests inject rigged specs).
5. Award resolution in `advance_day`: score bids at deadline, sole
   bidder wins iff bid ≤ ceiling, payment = bid.
6. Minimal but playable UI: enter a price on a selected solicitation
   (InputMode number entry exists), pending-bid marker, rep-tag,
   award/lapse events.
7. BasicPolicy interim behavior: bid exactly the reference payment
   (keeps harness green before Task 3's rule engine).
8. Tests: award/lapse paths, ceiling rejection, legacy-save flow,
   determinism. Full re-baseline — the year-1 offering changes
   shape twice over (campaigns gone, gov/GEO visible from day 1),
   so bands, fairness floor, and the additive-only test all get
   re-derived here, not patched.

### Task 2 — DinoSoar as a second Company

1. `Competitor` struct = a real `Company` + script parameters
   (margin policy, decline rules) + seeded reliability. Realized at
   game start via `world_query`: N manufacturing teams/lines (the
   swept capacity knob) and a failure rate drawn from a skewed
   distribution (norm ~99%+, rare ~95% seeds).
2. Fixed catalog, no R&D: pre-built engine + heavy-lift rocket
   design injected at creation with exactly one permanent
   loss-of-vehicle flaw (activation chance = seeded failure rate).
   Roster maintenance via the existing `auto_build_targets`
   mechanism; builds run through the real
   `Manufacturing::advance_day`.
3. Tick routing refactor: competitor's manufacturing + auto-builds
   tick daily alongside the player's; its events log under its
   name. Player path untouched. (Full extraction of R&D/flight
   loops off `player_company` is post-M3 hygiene, tracked in
   ROADMAP.)
4. Scripted bidding: bid = catalog marginal cost × margin, margin
   rises as free stock (inventory − reserved) shrinks; declines at
   zero free stock (same readiness gate as Task 3); explicit bid
   floor as a safety knob. Enters award resolution as a second
   sealed bidder; lost-award events name its price.
5. Abstract launches: won contracts consume rockets from its real
   inventory on schedule, roll the flaw, make the news; failures
   dent its reputation (real `Reputation` inside its Company).
6. Tests: player loses when badly outbid, wins when DinoSoar
   declines (no free stock / bid floor), DinoSoar rep moves on its
   failures, capacity actually limits it (demand spurt overflows),
   and **DinoSoar never wins year-1 rideshare across 200 seeds**
   (floor protection by economics).

### Task 3 — Bid automation + bot brain

1. Rule engine: marginal-cost estimate + margin → bid, per market,
   behind the readiness gate (free stock = capable rockets in
   inventory − reserved by awarded unflown contracts; no bid unless
   it covers the new contract).
2. Player-facing standing rules (per-market enable + margin).
3. BasicPolicy adopts the rule engine; re-baseline bands with the
   bot actually pricing (expect meaningful shifts — bot now splits
   awards with DinoSoar).
4. Degenerate-strategy check (early M4 tripwire): sweep margin 0–50%
   in the harness; if "always bid cost+ε" strictly dominates,
   scoring weights need work before M4.

### Task 4 — Discovery polish + guardrail re-baseline

1. UI: bid history / observed-award info surfaced the discovery-rule
   way (only what happened, never internals: no ceilings, no
   DinoSoar cost model or reliability). Headless render test like
   M2.6's.
2. Fairness floor test extended: year-1 rideshare solicitations must
   be *winnable* (DinoSoar never wins them + reference bid ≤
   ceiling), not just offered.
3. Full re-measure: bands, floor, determinism; update baselines;
   record new M4 findings (there will be some — award splitting
   changes everything downstream).
4. Balance note (USER, after Task 1 playtest, 2026-07-19): contract
   budgets feel balanced with the player as sole bidder, but should
   be raised once competition exists — losing awards to DinoSoar
   shrinks income, so `budget_tolerance` (and possibly reference
   payments) need a competitive-era re-tune here.

## Not in M3 (mapping)

| Idea | Where it lands |
|---|---|
| COTS-style deliberate split awards | Post-M3 follow-on (roadmap already flags it) |
| Campaigns (off in M3) returning as block-bid solicitations | Follow-on; the announcement becomes a big decision event |
| Demand curves responding to price (with delay) | Post-M3 per market_ideas.txt |
| More competitors / competitor R&D | Post-MVP (Later: competitor depth) — DinoSoar's *manufacturing* is real as of M3 |
| Full Company extraction (R&D + flight loops off `player_company`) | Post-M3 hygiene, incremental, pairs with the game_state.rs split |
| DinoSoar reacting strategically to player pricing | Post-MVP; M3 margin policy is stock-driven + noise |
| Monetary forfeit fees | Never (M2 decision) — forfeits flow through reputation × award scoring |

## Sequencing & estimates

Tasks in order; each is separately committable and playable. Task 1
is the structural core (~M2 Task 1 sized), Task 2 medium, Task 3
small-medium, Task 4 small. Roadmap allots ~2 weeks, which fits.

Per-task: full suite + all four ignored 200-seed checks before each
commit, commit only on your approval (properly, this time).

## Open questions

1. **Campaigns**: pre-priced missions in M3, block-bidding later —
   confirm?
   USER:  See above
   **CLAUDE:** Resolved: campaigns off in M3, block-bid redesign as
   the follow-on (see reply above).
2. **DinoSoar's numbers**: starting reputation ~65, canned success
   ~94%, payload floor ~1,000 kg, capacity ~8–12 awards/year — sane
   first guesses? (All TOML-tunable; M4 sweeps them.)
   USER:  That makes sense, but in general the success rate should only rarely
   be as low as 94%.  This is an affectionate parody of ULA which in our
   timeline has a very high success rate indeed.  Have the number of
   manufactuerers be swept to limit how many launches it can do.  We probably
   don't want a payload floor but more of a bid floor which ought to translate
   in fewer places that will accept its bids.
   **CLAUDE:** Updated: success rate ~99% (data-driven; a DinoSoar
   failure is a rare, genuinely newsworthy event), capacity emerges
   from the swept production-line count rather than an awards/year
   cap, and the payload floor is dropped in favor of a bid floor —
   which mostly falls out of the cost model for free (its only
   rocket is huge, so its minimum sane bid is huge); an explicit
   floor field stays as a safety knob. One consequence: the
   floor-protection test rewords from "never *bids on* rideshare" to
   "never *wins* year-1 rideshare across 200 seeds" — same
   guarantee, enforced by economics instead of a hard filter.
    USER:  It's ok if every once in a while the seed gives us a DynoSoar that's
    only 95% reliable, but that shouldn't be the norm.
    **CLAUDE:** So the failure rate is a seeded draw from a skewed
    distribution: most seeds land ~0.5–1.5% failure, a thin tail
    reaches ~5%. Drawn via `world_query` at realization like the M2
    market draws, bounds in TOML.
3. **Losing-bid visibility**: after an award, the event names the
   winning price ("Awarded to DinoSoar at $42M") — confirm? It's the
   elasticity-discovery mechanic, but it does hand the player exact
   competitor pricing.
   USER:  I think that's best as a matter of making the game fun.
   **CLAUDE:** Locked in.
4. **Bid windows as character**: per-market window ranges (gov long
   ~45–90 days, commercial short ~14–30) — or one global window for
   simplicity?
   USER: Just one window.  Keepting it always 30 days maybe?
   **CLAUDE:** Done: one global `bid_window_days = 30` config value,
   fixed (not a range). Drops the per-market window fields from
   Task 1 entirely — bid windows stop being a character axis.
5. **Unopposed gov work**: when DinoSoar declines and you're alone,
   the budget ceiling is the only price check. Comfortable with
   "name any price up to the hidden ceiling" as the M3 experience,
   with real tension arriving via M4 tuning?
   USER:  As long as we have the "Nobody wins" option for bids that's fine.
   **CLAUDE:** Confirmed — the lapse path stays; overbidding an
   unopposed solicitation loses it.

## Still open after round 2

Round-1 items all resolved (curve ✓, campaigns off ✓, and the
symmetry audit came back clean — see the manufacturing reply: the
`Company` struct already holds everything, so DinoSoar runs the real
manufacturing code in M3; the task list above is rewritten to
match). One new question from this round:

1. Removing the reputation *visibility* gate means gov/GEO
   solicitations are visible (with the rep-tag) from day 1 — OK as
   the intended experience? (Reply under the award mechanic.)
   USER:  Yeah, the player should know that they start out as a mammel scurrying
   between the feet of dinosaurs until the grow large enough to take over.
   **CLAUDE:** Resolved — plan approved 2026-07-14, execution begun.

Resolved history:

1. The logistic `rep_target`/`rep_scale` scoring curve (reply under
   the award mechanic) — right shape?
   USER:  Yes, looks great.
2. Campaigns off via `campaign = None` defaults, machinery kept
   dormant for the block-bid follow-on — OK to keep the code?
   USER:  Yes, lets keep it for now.
3. DinoSoar as structurally-parallel state (own lines/inventory/
   backlog ticked in advance_day, shared types but not the player's
   manufacturing pipeline) — right symmetry line?
   USER:  We really do want it to use the same code.  The question is whether we
   have leaky abstractions that make that difficult to achieve in M3.
   **CLAUDE:** Audited — no leaky abstraction in the way. Same code
   it is; details in the manufacturing reply above and the rewritten
   Task 2.
