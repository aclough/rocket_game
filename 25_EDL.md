# 25 — Entry, descent and landing

ROADMAP: "Entry, Descent, and Landing features with as much detail and
risk as launch has right now. Heat shields, parachutes, and
retropropulsion. Reusable rocket stages. Different rocket structure
materials maybe." The brief (2026-10-25): landing a rocket should be
as fraught as getting it off the pad; heat shields and parachutes are
components with their own flaws; rockets and engines carry EDL-only
flaws; a successful launch teaches nothing about them.

This is the thinking-stage document: what exists, what EDL could be,
and the questions whose answers shape the steps. No steps yet.

---

## 1. What exists

- **The graph has surfaces.** Earth, the Moon, Mercury, Venus (1 bar
  balloons), Mars, Phobos, Deimos, Vesta, Ceres, Hygiea, Eros, Bennu
  each have a `surface` node joined to their orbit by a ground pair:
  the same nominal delta-v both ways, drag charged on ascent only.
  Descent edges carry `can_aerobrake`, documented as "a future saving,
  not a cost, until EDL is modelled" (`location/mod.rs`). A flight to a
  surface today pays the full propulsive delta-v and then simply
  arrives; nothing is rolled, nothing can go wrong, no spacecraft has
  ever been destroyed on landing.
- **Nobody is asked to land.** No market has a surface destination.
  Contracts end at an orbit or a Lagrange point.
- **Flights persist and tick.** `Flight` moves through the graph over
  days, `PerDay` endurance flaws roll in transit, power is budgeted,
  arrival resolves contracts. Spacecraft persist at a location with
  their design, flaws and payloads. That is the skeleton EDL hangs on.
- **The flaw model is phase-shaped.** `FlawPhase {Ignition, MaxQ,
  Burn, Separation, PayloadDeploy, Endurance}`, rolled at
  `FlightEvent`s during the ascent; three exposure classes with ground
  discovery exponents (bench 2, integrated 5, analysis 9); a flight
  review covers exactly the (phase, group) pairs the flight reached.
  Adding `Entry`, `Descent`, `Landing` phases and the events that roll
  them gives EDL flaws for free *and* gives the brief's "launches
  teach nothing": a launch never reaches those phases, so its review
  never covers them.
- **Components are a pattern already.** A stage carries
  `power_sources: Vec<PowerSource>` with mass, cost and kind-specific
  physics, edited in the power editor, auto-sized to demand. Heat
  shields, parachutes and legs fit the same slot.
- **Engines have everything but a restart.** Cycles, bells, flaws,
  improvements, deficiencies, flight heritage. There is no notion of
  relight count, deep throttling or a landing burn.
- **Deferred into this plan from TODO:** dry masses and rocket bodies
  (structural materials, stage diameter), pressure-fed tank mass.

---

## 2. The shape of EDL in the game

### 2.1 EDL as a timeline, like ascent

An arrival at a surface runs an EDL timeline the way a launch runs an
ascent: `Entry` (atmospheric bodies), `Descent` (parachutes, or the
braking burn on an airless body), `Landing` (touchdown: legs, final
burn, or splashdown), each a `FlightEvent` that rolls the flaws living
in that phase on the hardware involved. Outcomes mirror the ascent's:
lost (burn-through, chute failure at speed, hard landing), degraded
(off-target, rough landing that damages the payload or the stage),
clean. A `LandingRecord` beside `LaunchRecord`, a popup, history.

### 2.2 The physics gates

Enough physics that a design is right or wrong, not a dice roll with
flavour text:

- **Entry heating** from entry speed and ballistic coefficient. LEO
  return is 7.8 km/s, lunar return 11, Mars entry 5–7 against a thin
  atmosphere, Venus 10+ against a thick one. A heat shield is rated
  for a peak heating / total heat load; under-rated shields fail with
  a probability that climbs with the overshoot (the overexpansion
  rule's shape). Mass: ~10–15 % of entry mass for an Earth capsule,
  more for lunar return.
- **Descent**: parachutes bring a given ballistic coefficient to a
  terminal velocity; on Mars a chute alone is not enough below a few
  tonnes and never for a heavy lander (retropropulsion required). On
  an airless body the braking burn is the whole descent: the lunar
  surface edge becomes a real landing burn with gravity losses and a
  hover margin, flown by engines that must relight and throttle.
- **Landing**: touchdown speed against what legs / airbags / the
  structure take; a splashdown needs a sea state and a recovery ship;
  a propulsive landing needs a deep throttle (an engine attribute) and
  a last-second relight.
- **Mass budget**: shield + chutes + legs + landing propellant come out
  of payload. A reusable booster gives up payload for boost-back and
  landing propellant; that number should fall out of the planner, not
  be a hand-set percentage.

### 2.3 Components (settled: stage attributes, not projects)

Heat shields, parachutes and landing legs are things a stage *has*,
chosen in the designer the way a panel or a battery is, each with
mass and cost that the structural model, the bill of materials and
the cost estimate see:

- **Heat shield**: a material and a rating (peak heating / heat load)
  for the entry the stage is designed for; area from the stage's
  derived diameter.
- **Parachutes**: from a short catalogue (drogue, main, cluster),
  sized by the mass they must slow to a touchdown speed on a given
  body.
- **Legs**: a mass fraction and a touchdown-speed limit.

Their flaws are rocket flaws in the `Entry` / `Descent` / `Landing`
phases, drawn from pool entries gated on `Feature::HeatShield`,
`Feature::Parachute`, `Feature::Legs`, with the usual exposure
classes; built units carry them in the rocket snapshot and retrofit
covers them. No new project kind, no new vendor role for now.

### 2.4 EDL-only flaws on rockets and engines

- **Rockets**: guidance and cross-range, centre-of-mass under entry
  loads, leg deployment, slosh on the flip, RCS authority — rocket
  flaws in the new phases, drawn from new pool entries that require
  EDL features (`Feature::HeatShield`, `Parachutes`, `Legs`,
  `LandingBurn`).
- **Engines**: relight after a long coast, deep throttle instability,
  TEA-TEB count, igniter after soak-back, landing-burn timing. These
  only exist on engines that are *asked* to relight (an engine spec
  gains restartability and throttle range; a pressure-fed hypergolic
  restarts for free, a gas generator needs an igniter cartridge per
  start and a design choice to carry them).
- **Discovery**: nothing on the ground finds an integrated EDL flaw
  the way a static fire finds an ignition one. The honest ground
  analogue is the *EDL test flight*: a suborbital hop
  (`earth_surface → suborbital → earth_surface`) or a boost-back of a
  booster on an otherwise ordinary launch, which reaches the EDL
  phases and opens an EDL review. A launch's flight review already
  cannot cover phases it never flew; the plan keeps that and makes
  sure the EDL phases are not reachable by accident.

### 2.5 Who lands, and why it pays

Three customers, which can be three steps:

1. **Reusable stages.** A booster with legs, relight and reserve
   propellant comes back to inventory as a *flown* unit: its flaws
   have been exercised, its wear counts (a per-unit flight count that
   raises activation on wear-type flaws until a refurbishment order),
   and its next flight costs refurbishment instead of a build. Payload
   falls by the reserve; unit cost falls by most of the vehicle. This
   is the one that changes the economy, and the one where "landing is
   as fraught as launch" is literal: a landing loss is a lost booster
   and a lost cadence, not a lost payload.
2. **Returned payloads.** Capsules: ISS downmass as part of COTS,
   sample return (asteroid, lunar, Mars later), film/biological
   return; a `Payload` that must come *back* to `earth_surface`, so the
   contract completes on landing, and a landing failure is a failed
   contract with the customer's irreplaceable sample in it.
3. **Planetary landers.** New markets: lunar landers (government
   science from the start, a commercial wave ~2008, the X-Prize era),
   Mars landers (government, rare, large, failure draws hearings), an
   asteroid touch-and-go. The contract names a surface; the planner
   routes to it; EDL runs at the end of a months-long flight that has
   already survived endurance flaws and the power budget.

### 2.6 Bodies and materials (the deferred TODO)

EDL is where the dry-mass work was deferred to, and it is needed
here: a stage gets a **diameter** (packaging the vacuum bell, the
fairing, the heat shield's area), a **structural material** (aluminium
/ steel / composite with density, cost, temperature limit — steel
takes entry heating a composite tank will not), and pressure-fed
stages get the thick tanks that keep them small. The structural model
then has the inputs an entry-loads check needs.

### 2.7 Testing, bots, bands

- Drop tests and arc-jets as testing cycles on component projects;
  suborbital EDL test flights as the integrated test; an EDL review.
- Bots: the medium bot grows a reusable variant (`reuse`) that lands
  its booster, and a `lander` bot that bids lunar-lander contracts.
  Their first-landing curve becomes a band like the launch curve
  (Falcon 9's first landing attempts failed; a new entrant's first
  landing should succeed about as often as its first launch).
- The flaws retune's slack (the 0.5 % bankruptcy floor) is a natural
  place to spend some of EDL's new risk.

---

## 3. More ideas to weigh

- **Wear and refurbishment.** Reused hardware carries a flight count;
  wear flaws (seal erosion, chute fabric, shield ablation margin) have
  an activation that grows per flight until a refurbishment order
  resets it. The player chooses how far to fly a unit between
  refurbs; the bot has a rule.
- **Turnaround.** Refurbishment is manufacturing work (lines, days),
  so reuse buys cadence only with a line to spare; a recovered booster
  is not a rocket until integrated with a new upper stage.
- **Recovery logistics.** A downrange landing needs a ship or a pad
  (an asset to buy); a return-to-launch-site landing costs more
  boost-back propellant. Sea state scrubs landings the way pad aborts
  scrub launches (a `LandingScrubbed` that expends the booster).
- **Landing accuracy** as a degraded outcome: off-target by km costs
  a recovery delay or, on a planetary lander, the science.
- **Engine restarts as a first-class attribute**: number of relights
  and throttle floor on the engine spec, chosen in the editor, with
  mass and complexity; a `Restart` improvement vein; deficiencies that
  bite restart (a technology's engine that will not relight cold).
- **Hot staging and flip manoeuvres** as rocket-level EDL-adjacent
  events with their own flaws, once a booster returns.
- **Fairing recovery** as the cheap first taste of reuse: chutes on a
  fairing, a ship, a modest saving, flaws of its own.
- **Lunar night and Mars dust**: a landed spacecraft's power budget
  continues on the surface (the power system already models sun
  distance; add night fraction and dust); a lander that dies at
  sunset is a contract that pays less.
- **Planetary protection and crew** are later; EDL for crew raises
  failure severity the way COTS already does.
- **Animations for launch and landing** (next roadmap line) have an
  obvious hook in the EDL timeline events.

---

## 4. Questions

1. **First customer.** Reusable boosters, returned payloads, or
   planetary landers first? Each is a vertical slice; reuse changes
   the economy most, landers add markets, capsules are the smallest.
   USER:  Since doesn't have a seperation event in the middle of a boost maybe
   we should do a more classic landing on a planet or other body first then
   reuse.
   CLAUDE: Agreed. Order: capsules and landers first (the EDL timeline,
   shields, chutes, landing burns, the markets), reuse last as its own
   sub-plan once a stage can come down at all. Reuse also needs the
   separation-time branch (booster peels off mid-ascent and flies its
   own EDL while the rest goes on), which is new flight plumbing.

2. **Components as projects.** Heat shields, parachutes and legs as
   design projects with their own R&D loop (like engines, buyable
   third-party), or as parameters on a stage with flaws attributed to
   the stage? The first is more work and more game.
   USER:  Part of me wants them to be their own components but I'm worried that
   heat shields, in general, are strongly tied to particular vehicles and can't
   be reused for new rockets the way that engines can.  Could parachutes?
   CLAUDE: Split them by that reasoning. A **heat shield is part of the
   stage**: a material and a rating chosen in the designer (ablative /
   reusable tile; sized to the stage's derived diameter and the entry
   it is designed for), its mass in the structural model, its flaws
   *rocket* flaws in the `Entry` phase on that stage. **Parachutes are
   component projects**: designed and tested (drop rig) like engines,
   with their own flaws, fitted to any stage, bought third-party —
   a chute is a chute. **Legs** go with the stage like the shield
   (mass, a touchdown-speed limit, `Landing`-phase rocket flaws).

3. **Ground discovery of EDL flaws.** Arc-jet / drop tests find
   component flaws; integrated EDL flaws only from EDL flights
   (suborbital hops, boost-backs). Right line? Should a suborbital
   test flight be a cheap, explicit action?
   USER:  I think I want to keep essentially the same level of abstration we
   currently have with launches.  We want regular EDL to be about as hard as
   launch and reuse to be much harder.
   CLAUDE: Then no new test actions. EDL flaws are ordinary flaws in
   three new phases with the usual exposure classes: a rocket project's
   testing cycles roll its `Entry`/`Descent`/`Landing` flaws like any
   other (the stand and the analysis find what they can), a parachute
   project's cycles are its drop tests, and a flight review covers only
   the phases the flight reached — so a launch never covers EDL, as
   you asked. Difficulty is then set the way launch difficulty was:
   per-phase flaw counts and the exposure mix (EDL leaning
   integrated/analysis, which the ground finds slowly), tuned to a
   measured curve. Reuse gets its own, larger pool
   (`Feature::BoostBack`) on top of the landing pool.

4. **Engine restarts.** Make relight count and throttle floor engine
   attributes with mass, complexity and their own flaw vein, or treat
   "restartable" as a per-cycle given (hypergolic / pressure-fed yes,
   others with an igniter option)?
   USER:  Just ignore restarts as too much complexity to manage.  Throttle floor
   isn't something the player has to keep track of as it changes so we can go
   with that, and it helps differentiate engine types.
   CLAUDE: Restarts dropped. Throttle floor per cycle, shown on the
   engine, no knob: pressure-fed 10 % (hypergolic landers), expander
   20 %, staged combustion 40 %, gas generator 60 %, solid none. The
   landing gate is physics: at touchdown the stage must be able to
   throttle below its own weight with margin (min thrust ≤ weight ×
   `landing_hover_margin`), so a gas-generator stage lands only with
   enough mass under it, a pressure-fed hypergolic lands anything, and
   a solid never lands. A landing-burn engine gets `Landing`-phase
   engine flaws (deep-throttle instability, burn timing) from an
   EDL engine pool.

5. **Bodies and materials in scope?** Stage diameter and structural
   material are needed for shield area, bell packaging and entry
   loads; they also touch every existing design. Fold them in here
   (as the first steps) or run them as their own plan before this one?
   USER:  We definetly want material here.  It'd sort of like to avoid stage
   diameter as an independant variable, could we abstract it behind mass?
   CLAUDE: Yes. A stage's diameter is derived, never chosen: from its
   propellant volume at a fixed fineness (length ≈ `stage_fineness` ×
   diameter, ~8), so d ∝ volume^⅓; a side booster or a 2 t upper stage
   comes out slender on its own. That gives the shield its area, the
   vacuum bell its packaging cap (replacing the 2 m constant), and the
   fairing a base. **Material** is the chosen variable: aluminium /
   steel / composite per stage, with density, cost per kg, and a
   temperature limit that matters at entry (steel takes it, composite
   needs a shield over everything). This folds the dry-mass and
   pressure-fed-tank TODOs in as the first steps.


6. **Reuse economics.** How much should a landed booster save — most
   of the vehicle's build cost less refurbishment, with wear raising
   risk per flight? And should DinoSoar ever reuse?
   USER:  We want some refurbishment cost/time but I'm inclined not to add wear
   as its own thing.  Refurbishment time should certainly be something that can be
   improved with research and which can be affected by complexity.
   CLAUDE: No wear. A recovered stage goes back as a refurbishment
   order: work = `refurbish_fraction` × its build work × complexity
   factor, materials = a fraction of its build materials; the rocket
   project gets a `Turnaround` improvement vein (found in testing like
   the others) that cuts the fraction. The flown unit keeps its flaw
   snapshot, so what it had, it still has — reuse is harder because
   the booster's EDL pool is larger and nothing on the ground finds
   those flaws fast, not because the hardware ages.

7. **Markets.** Lunar landers from the start (government) with a
   commercial wave later, Mars landers rare and government-only, ISS
   downmass inside COTS, sample return as government science rows —
   anything else, and how big should the pot be against launch?
   USER:  For now I'd like a few big campaigns with lots of lead time for the
   user to prepare.  Some Varda style LEO experiments, some asteroid sample
   returns from a company interested in mining.  NASA landings on the Moon.  And
   as part of COTS which happens pretty reliably.
   CLAUDE: Campaigns with lead time, then: announced one to three
   years before the first mission, a long bid window, and a
   destination that is a surface (or a return to `earth_surface`).
   Four archetypes: **Orbital Foundry** (Varda-style: a capsule to LEO
   that returns in N days, steady from ~2010), **Prospector Sample
   Return** (a mining company's asteroid touch-and-go and return, rare,
   large, from ~2012), **Lunar Surface Program** (government landers,
   a block of three to five, from the start with a long lead), and
   **COTS downmass** (a return leg on a share of resupply missions once
   COTS is present, reliable). Each pays on landing, and a landing
   failure fails the contract with the customer's cargo in it.

8. **Risk level.** "As fraught as launch": a new entrant's first
   landing succeeds about as often as its first launch (~35 %), with
   the same climb? Or harder, since nobody can static-fire an entry?
   USER:  For a parachute splashdown probably notably easier.  For rockets
   landing on another body about as hard.  For reuse notably harder with lots of
   launches to get it to work the first time.
   CLAUDE: Three curves to tune, each with its own bot and band:
   capsule splashdown ~60 % first time (chutes are drop-testable, so
   the ground finds most of it), a propulsive landing on a body ~35 %
   (the launch curve), booster recovery ~15 % first time with the same
   climb — SpaceX needed a dozen tries. Set by the per-phase pools and
   exposure mixes, measured the way the launch curve was.

---

## 5. Second-round questions

1. **Shield on the stage, chutes as projects, legs on the stage** (Q2).
   Agree with the split?
   USER:  Actually, I'm chaning my mind.  Chutes should probably just be things
   you buy like a battery or solar panels.  Maybe we make panels and chutes
   seperate things later but keep them simple additions now.
   CLAUDE: Done that way. A parachute is a stage item like a panel or a
   battery: picked from a short catalogue (drogue, main, cluster —
   sized by the mass it must slow, with mass and cost), carried in a
   `recovery` list beside `power_sources`, no project, no R&D. Its
   flaws are *rocket* flaws in the `Descent` phase gated on
   `Feature::Parachute`, so the stage's testing finds them the way it
   finds slosh baffles. Third-party chutes and chute R&D can come later
   if the market wants them. Same for legs and the heat shield: all
   three are stage attributes, none is a project.

2. **Throttle floors** per cycle as proposed (PF 10 %, expander 20 %,
   SC 40 %, GG 60 %, solid none) and the hover-margin landing gate?
   USER:  Sounds good.

3. **Materials.** Aluminium (today's numbers), steel (heavier, cheap,
   takes heat), composite (lighter, dear, needs shielding at entry).
   Enough for a first pass, or add titanium / aluminium-lithium?
   USER:  Enough for a first pass.  In the future non-aerodynaic, aerodynaic,
 flying might be a thing but that's for alter.

4. **Campaign lead time.** One to three years' notice, bid window of
   months, and the campaign names its vehicle requirements (payload
   mass, destination, return) so the player can design to it?
   USER:  Yup

5. **DinoSoar** never lands or reuses — it stays the expendable
   incumbent, and landing is the player's edge?
   USER:  Yes.  Later we should live player competitors.
   CLAUDE: Noted for the roadmap: live (policy-driven) competitors
   after EDL.

6. **Sub-plans.** Split as 25_1 bodies and materials, 25_2 EDL core
   (phases, shield, chutes, landing burn, markets, bots), 25_3 reuse?
   USER: Sounds good

---

## 6. Steps (draft, one commit each; sub-plans will refine)

Each sub-plan is its own numbered document, written when its turn
comes; this top-level list is the contract between them.

**25_1 — Bodies and materials** (`25_1_BODIES.md`)
1. Derived stage diameter from propellant volume and fineness; the
   vacuum bell's packaging cap and the fairing base read it. Oracle
   byte-identical where the 2 m cap used to bind no one; the engine
   yardstick records the few bells that move.
2. Structural material per stage (aluminium / steel / composite):
   density, cost per kg, temperature limit; the structural model and
   the bill of materials use it; the designer chooses it; saves
   default to aluminium. Bands green (aluminium is today).
3. Pressure-fed tank mass: wall thickness from tank pressure and
   material, pressurant mass; the TODO's "what keeps pressure-fed
   small". Probe and yardstick recorded; bands green (the bots fly no
   pressure-fed stage).

**25_2 — EDL** (`25_2_EDL_CORE.md`)
4. The EDL timeline: `FlawPhase::{Entry, Descent, Landing}`,
   `FlightEvent`s rolled on arrival at a surface, `LandingOutcome`,
   `LandingRecord`, popup and history; the flight review covers what
   was reached. No design has EDL features yet, so nothing changes
   (oracle identical).
5. Heat shields on stages: material and rating, mass from the
   derived diameter, entry heating from the arrival speed per body
   (LEO, lunar return, Mars, Venus), burn-through risk by overshoot;
   `Entry`-phase rocket pool gated on `Feature::HeatShield`.
6. Parachutes and legs on stages: a chute catalogue (drogue, main,
   cluster) picked like a panel, sized by the mass it slows on a given
   body; descent physics (ballistic coefficient → terminal velocity,
   per body); `Descent`-phase rocket pool gated on `Feature::Parachute`;
   splashdown and touchdown limits; legs with a touchdown-speed limit
   and the `Landing`-phase rocket pool.
7. Propulsive landing: the landing burn in the planner (gravity
   losses and hover margin on the surface edge), throttle floor per
   cycle and the hover gate, `Landing`-phase engine pool for engines
   that land.
8. Markets: the four campaign archetypes with lead time, return
   payloads that complete on landing, reputation on landing failures.
9. Bots and bands: a `capsule` bot (Orbital Foundry) and a `lander`
   bot (Lunar Surface Program), their landing curves measured and
   banded (~60 % / ~35 % first landings), the launch bands unchanged.

**25_3 — Reuse** (`25_3_REUSE.md`)
10. Booster recovery: boost-back and landing reserve from the planner
    (payload falls out), legs and landing burn on a first stage, the
    separation-time branch that flies the booster's own EDL during an
    ascent, `Feature::BoostBack` pools (large, slow to find on the
    ground).
11. Refurbishment: a recovered stage as a manufacturing order with
    time and materials, the `Turnaround` improvement vein, complexity
    in the time; a flown unit keeps its flaws.
12. A `reuse` bot and its bands (~15 % first recovery, the climb
    measured), DinoSoar expendable; record and file.
