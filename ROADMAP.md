# Rocket Tycoon Roadmap

> Living document: **Shipped → MVP milestones → Later**, with vertical,
> individually-playable milestones instead of horizontal system phases.
> Replaced the original phase-based roadmap in July 2026 after the
> Minimum Playable Loop (old Phases 1–4), flight ops (Phase 5), and
> research (Phase 6, in a different shape than written) shipped.

---

## Shipped (as built)

A snapshot of what exists, for orientation — not a task list.

- **Core loop:** day tick, salaries, event log with importance tiers +
  auto-pause, save/load with world-seed / contingent-RNG split.
- **R&D pipeline** (the signature mechanic, three domains): engines,
  rockets, and reactors all run Proposed → InDesign → Testing →
  Revising with flaws, improvements, seed-driven tech deficiencies,
  NRE tracking, and team assignment (incl. cross-pool steal).
  Third-party engines with unfixable flaws.
- **Manufacturing:** teams, floor space, build orders, learning +
  forgetting curves, inventory.
- **Contracts & launches:** market-driven contract generation (data-
  driven `Market` tables, incl. event-activated markets), launch sim
  with flaw activation, delta-v feasibility, partial failures,
  reputation, financial tracking.
- **Flight ops:** multi-leg routes, per-stage propellant, power model
  (solar/RTG/battery/fuel-cell/reactor with brownout stranding and
  electric-thrust derating), spacecraft persistence, payload
  deploy/dock/undock, mid-flight flaw activation, destroyed-vs-stranded
  outcomes.
- **Technology:** seed-generated tech deficiencies (Methalox, NTR,
  fission reactor), yearly unlock rolls, deficiency-fix flow through
  revisions. (The old Phase 6 "tech tree + research teams" design is
  superseded by this — research happens *through* projects.)
- **Test suite:** ~380 tests including headless render tests.

---

## The MVP thesis

Get the game into strangers' hands as soon as it can answer the
question "is the core loop fun?" That requires: a living market
(seeded variety + price competition + one competitor), numbers that
aren't obviously broken (harness-driven tuning), and a survivable
first 15 minutes. It explicitly does **not** require stations, mining,
crew, tourism, or deep competitor simulation.

**1.0/MVP cut: current loop + M1–M5 below.** One competitor (DinoSoar)
is necessary and sufficient for 1.0; more rivals are post-MVP texture.

Rollout plan: friends first, then a few medium-sized Discords (e.g.
the Hard SF server) or subreddits, then something like itch.io.

---

## MVP milestones

Ordered, but M1 and M2 are independent of each other and can be
swapped or interleaved. M3 depends on M2 (both touch contract
generation/award) and benefits from M1. M4 depends on M1+M3.

### M1 — Simulation & tuning infrastructure  (~1–2 weeks)

The force multiplier for everything after it; also the first half of
the competitor work (bot policy = competitor brain).

1. **`BalanceConfig` refactor** — convert `balance.rs` constants into a
   struct with defaults = current values, loadable from TOML. No
   behavior change; enables sweeps without recompiling.
2. **Headless sim binary** — `cargo run --bin simulate -- --seed N
   --years Y --policy basic` → CSV/summary (money, launches,
   reputation, failures per month). Pure `advance_day()` loop, no UI.
3. **`CompanyPolicy` abstraction** — a scripted "plays the game"
   policy: pick contracts, design/build a rocket, assign teams, launch.
   Start with one honest-but-naive policy; add archetypes (conservative
   / aggressive-R&D) later. *This same trait later drives DinoSoar.*
4. **Metric bands as tests** — e.g. "basic policy profitable by year 3
   in >70% of seeds", "bankruptcy rate < X", "no panics across 200
   seeds". Balance gets regression protection like correctness has.
5. **Seed-fairness floor** — across N seeds, assert viable early-game
   contract volume. (Becomes critical once M2 adds market variance.)

### M2 — Seeded markets & contract character  (~1–2 weeks)

Makes contract streams differ from each other and run to run. The
`Market` architecture already exists; this adds the seed layer and
character axes.

1. **Seed-perturbed market table** — at game start, `world_query`
   per market archetype draws: exists?, emergence date/trigger, volume
   multiplier, growth rate, rate multiplier, destination-weight tilt.
   Keep one bread-and-butter LEO market stable across all seeds.
   **Year-1 variance is strictly additive:** every seed guarantees the
   baseline opening contract stream; seeds may add extra early options
   (bonus markets, opportunities) but never thin the floor.
2. **Contract character axes** — reputation gates, deadline tightness +
   penalty structure per market (gov't lenient/gated, commercial
   tight/price-sensitive).
3. **Anchor customers / campaigns** — seeded named programs emitting
   correlated contract series (same payload class + destination,
   recurring cadence, block-buy discount). Pairs with the existing
   production learning curves.
4. **Cadence personality** — steady vs. lumpy vs. burst markets.
5. **Discovery rule:** UI shows only realized contracts / observed
   history, never seeded parameters.

### M3 — Competitive bidding + DinoSoar  (~2 weeks)

Absorbs two TODO items (player-chosen pricing, auto-bid at margin).

1. **Award mechanic** — player names a price per bid; sealed bid with
   per-source weighting of cost vs. reputation (different mission
   sources prioritize them to different extents — gov't reputation-
   heavy, commercial cost-heavy). *Follow-on, possibly post-M3:*
   COTS-style sources that deliberately distribute awards across more
   than one company.
2. **DinoSoar (market-presence competitor)** — a `CompanyPolicy` with a
   cost model that generates competing bids, occasionally launches with
   canned success rates, and has a moving reputation. **No simulated
   R&D or manufacturing** — that's post-MVP.
3. **Bid automation** — standing rule: auto-bid on contracts above
   marginal cost + chosen margin (the existing TODO item).
4. **Elasticity-as-discovery** — with player pricing live, probing
   demand by bidding becomes the discovery mechanic for free.

### M4 — Balance pass  (~1–2 weeks, interleaved)

Using M1's harness against the M2+M3 economy:

1. Parameter sweeps (dumb random/grid search — no LLM in the inner
   loop) to eliminate degenerate/broken regions: build delays, costs,
   NRE, contract rates, learning-curve slopes.
2. **Degenerate-strategy hunting** — bots that discover "never test
   engines" or "spam cheapest rocket" are design holes; fix mechanics,
   not just numbers.
3. Hand-tune pacing from inside the surviving region (bots can't feel
   boredom; the last mile is human).
4. Clears the TODO "Later tuning" items: construction time, design
   time, profitability.

### M5 — First 15 minutes & release readiness  (~1–2 weeks)

1. **Onboarding** — suggested-first-steps panel or lightly guided first
   contract; a help pane with keybindings per tab.
2. **Save robustness** — checked-in save corpus + load-compat test;
   panic handler that preserves the save and dumps a report.
3. **Session/bug report dump** — one keypress writes event log + game
   state summary to a file (players will hit things you can't repro).
4. **Windows support** — must build and run on Windows (crossterm
   should mostly carry this, but verify: paths, terminal behavior,
   save locations); add a Windows build check to the workflow.
5. **Packaging** — GitHub releases initially; README; feedback via
   friends + Hard SF Discord. Long-term intent (beyond this planning
   horizon): open source, with maybe a cheap Steam release — possibly
   with a GUI — if reception warrants it.

**→ MVP release. Collect feedback before committing to Later scope.**

---

## Later (post-MVP, in rough order)

Vertical loops, each individually shippable — replacing old Phases
7–10. Order should be revisited against player feedback.

- **Propellant depot loop** — depot module, fuel-delivery contracts,
  refuel in orbit. Smallest in-space-economy step; reuses flight
  system nearly as-is.
- **Probe & survey loop** — probes + seed-determined resource maps and
  conditions. Extends the discovery pillar beyond markets; feeds
  mining later.
- **Comms constellation loop** — recurring service revenue
  (Starlink/Iridium-style), mostly launch-side.
- **Tourism loop** — demand strongly tied to safety record; first
  crew-adjacent content.
- **Competitor depth** — more rivals; competitors with real
  R&D/manufacturing simulation, tech-copying visibility, reputation
  races. (Also: COTS-style split awards if not done in M3.)
- **Stations & outposts** — modular construction, labs, in-space
  manufacturing, mining (each of these is its own sub-loop; do not
  attempt as one milestone).
- **Crew system** — hiring, training, risk; trust-gated missions.
  (Absorbs TODO: crew support and modules; gov't trust gates;
  prestige/trust reputation split.)
- **Routes automation** — standing missions instead of one-off
  planning.
- **Flight-model depth** — propellant boiloff; mid-route payload
  detach; payload transfer between rockets; principled
  acceleration/gravity landing rules (all currently on TODO).
- **Part wear & reusability**, **expanded solar system**, **rare
  seeded discoveries**, **laser power**, **loans/financing**, **Earth
  macro-simulation** (GDP, wars, disasters).

---

## Ongoing engineering hygiene (no milestone; do opportunistically)

- Split `game_state.rs` (~4,900 lines) into submodules (flights,
  launches, projects, economy) before it gets worse.
- Fix the pre-existing clippy error (`location.rs:55`, literal `3.14`)
  so clippy can become a green gate.
- Archive completed plan .md files (phase1–5b, reactor plans) into
  `docs/plans/`; repo root keeps only living docs.
- Engine flaw that adds power draw (small TODO item — slot into any
  milestone touching flaws).

---

## TODO.txt policy

Every TODO item as of July 2026 lives in a milestone or Later (mapped
above). TODO.txt holds short-lived bugs/notes only; this file is the
planning source of truth.
