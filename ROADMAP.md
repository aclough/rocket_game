# Rocket Tycoon Roadmap

> Living document: **Shipped → Later**. Work is organised as vertical,
> individually-playable milestones rather than horizontal system
> phases. The MVP milestones (M1–M5) are done; their task-level
> breakdowns are archived in
> [`docs/plans/roadmap_mvp.md`](docs/plans/roadmap_mvp.md), and how
> each one was actually built is in [`docs/plans/`](docs/plans/).

---

## Shipped (as built)

A snapshot of what exists, for orientation — not a task list.

- **Core loop:** day tick, salaries, event log with importance tiers +
  auto-pause, save/load with world-seed / contingent-RNG split.
- **R&D pipeline** (the signature mechanic, three domains): engines,
  rockets, and reactors all run Proposed → InDesign → Testing →
  Revising with flaws, improvements, seed-driven tech deficiencies,
  NRE tracking, and team assignment (incl. cross-pool steal).
  Third-party engines with unfixable flaws. Engine projects design a
  *family* — the nozzle variant is chosen per stage. Rush jobs, and
  retirement for designs you're done with.
- **Manufacturing:** teams, build orders, learning + forgetting
  curves, inventory.
- **Contracts & launches:** market-driven contract generation (data-
  driven `Market` tables, incl. event-activated markets), launch sim
  with flaw activation, delta-v feasibility, partial failures,
  reputation, financial tracking.
- **Seeded markets:** archetype realization per world — presence,
  emergence dates, volume and growth, destination tilts, cadence
  personality (steady / lumpy / burst), and anchor-customer campaigns
  emitting correlated contract series. Year-1 variance is strictly
  additive: no seed thins the opening contract floor, enforced at
  config load and by a 200-seed property test.
- **Competitive bidding:** you name the price. Sealed bids scored on
  cost against reputation, weighted per market, against DinoSoar — an
  incumbent with a cost model, a launch record, and a moving
  reputation. Standing bid rules automate the routine cases.
- **Flight ops:** multi-leg routes, per-stage propellant, power model
  (solar/RTG/battery/fuel-cell/reactor with brownout stranding and
  electric-thrust derating), spacecraft persistence, payload
  deploy/dock/undock, mid-flight flaw activation, destroyed-vs-stranded
  outcomes.
- **Technology:** seed-generated tech deficiencies (Methalox, NTR,
  fission reactor), yearly unlock rolls, deficiency-fix flow through
  revisions. (The old Phase 6 "tech tree + research teams" design is
  superseded by this — research happens *through* projects.)
- **A world that moves:** economic conditions, and a geopolitical arc
  rolled each new year — great-power war opens the NRO market and
  closes others.
- **First fifteen minutes:** intro screen, a Next Steps panel driven by
  the same opening the scripted bot is *tested* to survive, `?` help
  for every key on every tab, idle-team auto-assignment, and
  auto-revise on by default.
- **Robustness:** month-start and on-quit autosave across three
  rotating slots, versioned saves with a compatibility corpus of real
  saves from past versions, a panic handler that gives the terminal
  back and rescues the game, and `F12` bug reports.
- **Tuning infrastructure:** balance constants in loadable TOML, a
  headless sim harness that plays the game with scripted policies, and
  metric bands over 200 seeds as tests.
- **Test suite:** ~615 tests including headless render tests. CI builds
  and tests on Linux, Windows, and macOS with clippy as a gate.

---

## The MVP thesis

Get the game into strangers' hands as soon as it can answer the
question "is the core loop fun?" That requires: a living market
(seeded variety + price competition + one competitor), numbers that
aren't obviously broken (harness-driven tuning), and a survivable
first 15 minutes. It explicitly does **not** require stations, mining,
crew, tourism, or deep competitor simulation.

One competitor (DinoSoar) is necessary and sufficient for 1.0; more
rivals are post-MVP texture.

Rollout plan: friends first, then a few medium-sized Discords (e.g.
the Hard SF server) or subreddits, then something like itch.io.

**Where that stands:** M1–M5 are done and the MVP cut is built. Next
is putting it in front of players and fixing what that turns up —
which is what the `F12` key exists for. The list below is deliberately
not committed to until then.

---

## Later (post-MVP, in rough order)

Vertical loops, each individually shippable. Order should be revisited
against player feedback.

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
  races. Also COTS-style sources that deliberately split awards across
  more than one company.
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

- Engine flaw that adds power draw (small TODO item — slot into any
  milestone touching flaws).

---

## TODO.txt policy

Planning lives here and in `docs/plans/`; TODO.txt holds short-lived
bugs and notes only.
