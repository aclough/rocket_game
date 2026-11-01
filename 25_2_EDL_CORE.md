# 25_2 — EDL core

Sub-plan of `25_EDL.md`: the entry, descent and landing timeline, the
hardware that survives it, the physics that says whether a design can,
the campaigns that pay for it, and the bots that prove it stays
possible. Reuse is `25_3`. Decisions already taken in the top-level
plan: landers and capsules before reuse; heat shield, chutes and legs
are stage attributes (chutes bought like panels); same abstraction as
launch, no new test actions; no restarts, throttle floor per cycle; the
campaigns are Orbital Foundry, Prospector Sample Return, Lunar Surface
Program and COTS downmass, with long lead times; DinoSoar stays
expendable; first splashdown ~60 %, first body landing ~35 %.

---

## 1. What the code gives us

- **A flight is legs.** `Flight.route: Vec<FlightLeg {from, to,
  delta_v_cost, burn_days, coast_days}>`; `tick_flight` counts a leg
  down, burns it (`burn_sequential`), and on the last leg returns
  `FlightEnd::Arrived`; `advance_flights` then rolls `PayloadDeploy`,
  opens the flight review on `events_reached`, and
  `resolve_arrived_flight` pays contracts whose destination this is,
  detaches spacecraft marked for it, and parks the vehicle if
  `persist`. A surface is just another node: today a flight to
  `lunar_surface` burns the edge's delta-v and arrives.
- **Flaws come from pools by words.** `generate_single_flaw` draws a
  consequence, an exposure and then a pool entry whose `requires`
  features the design has; the entry's words carry its phase. So EDL
  flaws exist the moment there are entries requiring EDL features and
  designs that have them, with no change to generation; a design with
  no EDL features draws none. The count is the design's effective
  complexity, so EDL hardware must *add* complexity or it would dilute
  ascent flaws instead of adding its own.
- **Events roll flaws.** `roll_event_flaws(rng, design, event, engines,
  rockets, on_pad)` rolls every flaw in the event's phase on the
  event's group; `FlightEvent::group(design)` says which. New events
  with new phases plug in the same way; the flight review covers
  `(phase, group)` pairs reached, so a launch never covers EDL.
- **Surfaces know themselves.** `SurfaceProperties {gravity, radius,
  has_atmosphere, atmosphere_density, ambient_pressure, scale_height}`
  per surface node — enough for entry speed, terminal velocity and a
  landing burn's gravity loss.
- **Campaigns** (`CampaignSpec`, `spawn_campaign`) announce a block, take
  one bid, then issue missions on an interval; each mission is a
  contract with one destination.
- **Stages carry items** (`power_sources`) edited in a sub-mode of the
  designer; `Stage` just grew `material` and a derived diameter.

---

## 2. Design

### 2.1 The EDL timeline

When a flight's last leg ends on a surface node, before `PayloadDeploy`:

```
Entry    — only if the body has an atmosphere
Descent  — chutes (atmosphere) or the braking burn (airless)
Landing  — touchdown: legs / splashdown / the final burn
```

Three `FlightEvent`s and three `FlawPhase`s, rolled on the group that
lands (the top group, as `PayloadDeploy` is) with the same
`roll_event_flaws`. Consequences keep their meaning: `StageLoss` or a
last-engine `EngineLoss` during the landing burn is a crash (vehicle
and payload lost, `FlightEnd::Lost`), `PerformanceDegradation` is a
hard or off-target landing (the contract pays the partial fraction,
the vehicle is damaged — not reusable, 25_3), `PayloadLoss` is the
cargo wrecked on touchdown. A `LandingRecord` beside the launch record,
the popup and the history list, and the flight review covers the
three phases.

Physics failures are rolled the same way as overexpansion is today:
a deterministic *risk* from the design's shortfall (shield under
rating, touchdown over the legs' limit, no hover authority), applied
at the event as a destruction probability, zero when the design is
right. So a correct design dies only to flaws, and a wrong one dies
to physics whether or not it has flaws — the designer shows the
physics verdict up front, as it shows "cannot reach" today.

### 2.2 Hardware on the stage

All three are stage attributes with mass and cost; none is a project.

- **Heat shield** — `Stage.heat_shield: Option<EntryRating>`: the
  player picks what the stage is rated to enter from (cycled in the
  designer: Mars orbit 4 000 m/s, Mars direct 6 000, LEO 8 000, lunar
  return 11 500, asteroid return 12 500) and the shield's areal density
  is *solved* from the heat, the stage's density and the structure
  behind it, over the base area from the derived diameter:
  - entry heat per m² of face: `Q = f_ref · (v / v_ref) · ½ β v²`, with
    `β = m / (C_d · π d² / 4)` the ballistic coefficient and `f_ref`
    0.01 at `v_ref` 7 800 m/s the fraction of the shed kinetic energy
    that reaches the vehicle (so Q ∝ β v³; LEO return at β 300 is
    ~90 MJ/m², lunar ~260, asteroid ~380, a broad Mars lander ~20);
  - a shield of areal density σ ablates `σ · A_abl` of it (`A_abl`
    2.5 MJ/kg effective) and passes `exp(−σ / σ_ins)` of the rest to
    the wall (`σ_ins` 15 kg/m²: each 15 kg/m² of insulation cuts the
    soak by e);
  - the structure takes `H = skin_kg_per_m² · c_p · (T_limit − 20 °C)`
    before it fails: the material's temperature limit and specific
    heat (Al 900, steel 500, composite 1 000 J/kg·K) over the skin's
    areal mass from the tank geometry, so a steel capsule takes about
    three times the soak an aluminium one does;
  - σ is the smallest with `(Q − σ A_abl)⁺ · exp(−σ / σ_ins) ≤ H`
    (bisection, like the bell fit). At LEO, β 300: steel ~25 kg/m²,
    aluminium ~31; lunar return ~55 / ~61; Mars direct, β 150: ~6 / ~8.
    Lighter shield on a tougher structure, which is the point of steel
    here, and the saving shrinks at lunar speeds where ablation does
    the work (Apollo-like ~70 kg/m² with its structure).
  Bill of materials: composites. An entry hotter than the rating
  (faster, or denser than designed) carries a burn-through risk
  ramping with the overshoot `Q_actual / Q_rated − 1` (the
  overexpansion rule's shape; a design error, not a flaw); a stage with
  *no* shield does not survive any entry through air. β also drives the
  descent: terminal velocity, and on Mars whether a chute can open at
  all (`max_chute_beta` ~200 kg/m² per body).
- **Legs** — `Stage.legs: bool`: a mass fraction of the stage's dry
  mass (`legs_mass_fraction`, ~4 %), a touchdown-speed limit (~3 m/s),
  `Feature::Legs`. A propulsive landing without legs is a crash on
  land and fine on water.
- **Parachutes** — `Stage.recovery: Option<Recovery>` from a
  two-item catalogue, picked in a `[R] Recovery` sub-mode like the
  power editor and sized at fit time to the stage's touchdown mass
  class: `Parachutes` (the drogue + main package, subsonic deploy, so
  thick air only: Earth, Venus, Titan; ~3 % of rated mass) and
  `Supersonic Chute` (one disk-gap-band canopy, opens at Mach 2 in
  thin air; ~2 % of rated mass; on Earth it acts as a drogue alone).
  Each has a drag area and a cost; `Feature::Parachute`. Terminal
  velocity under the fitted chute on the body's air sets the touchdown
  speed: `v = √(2 m g / (ρ C_d A))`.
- **Landing engines** — `Stage.landing_engines: u8`, how many of the
  group's engines light for the landing burn (default 1), set in the
  designer; what the hover and hoverslam gates read.
- **EDL complexity** — a design with any EDL feature has its flaw
  complexity raised (`edl_complexity_bonus`, +2 to start, config), so
  it draws EDL flaws on top of its ascent flaws rather than instead of
  them. This is the knob the curves are tuned with, with the exposure
  mix: EDL entries lean `Integrated` / `Analysis`, which the stand
  finds slowly.

### 2.3 Physics gates

- **Entry speed** per body: from its own orbit (LEO → Earth 7 800 m/s,
  Mars orbit → Mars 3 600, lunar orbit → Moon n/a), or faster when the
  flight arrives from outside (Earth from the Moon or an asteroid
  ~11 200–11 800, Mars from transfer 5 500–7 000). The route knows where the vehicle came
  from; no new edges. A shield lets the stage *aerobrake a chain*: the
  consecutive inbound edges that end at the surface — the heliocentric
  arrival burn into the body's boundary node (`bennu_transfer →
  earth_escape`, `mars_transfer → mars_capture`) and every bound edge
  after it (`earth_escape → lunar_orbit → leo → earth_surface`) — each
  charged `aerobrake_delta_v_m_s` (150, targeting and deorbit) instead
  of its delta-v, flown by the shielded stage alone. The entry speed is
  the energy the chain starts with: `Location.entry_speed_m_s` on the
  bound nodes of bodies with air (LEO/SSO 7 800, GTO 10 300, MEO/GEO,
  lunar orbit, L1/L2, Earth escape 11 200; Mars orbit 3 600, Mars
  capture 4 900), or, when the chain starts with a skipped heliocentric
  arrival burn, the body's escape speed `√(2 g R)` from
  `SurfaceProperties` plus that edge's delta-v — the Oberth identity
  for a periapsis burn (Earth from Mars 11 570, from Bennu 11 580, from
  Eros 11 780; Mars from transfer 5 015 + its capture delta-v). No
  shield: every edge propulsive as today, which is "you cannot land
  this through air". All speeds in m/s, heat in J/m², density kg/m².
  USER:  These are all going to be the existing delta-v edges, right?
  CLAUDE: Mostly, with one correction to how they are charged and a few
  new edges. Today every `orbit → surface` edge carries the *ascent*
  delta-v (LEO → Earth surface is 7 800 m/s, Mars orbit → Mars 4 100)
  with `can_aerobrake` set and ignored; nothing has landed on an
  atmosphere body yet so it never mattered. For EDL the aerobrake
  edges get two fields: `deorbit_delta_v` (what a shielded stage pays
  propulsively: ~150 m/s from LEO, ~100 from Mars orbit, 0 on a direct
  entry) and `entry_speed_km_s`. The planner charges the full
  propulsive value only when the stage has no shield, which is the
  "you cannot land this through air" case anyway. Then the new edges:
  direct entries that skip the parking orbit, which is how a lunar or
  asteroid return actually comes home — `lunar_orbit → earth_surface`
  (TEI ~900 m/s, entry 11.0), `earth_escape → earth_surface` (0 m/s,
  entry 11.2; the asteroid campaigns arrive this way, Bennu/Eros
  12.0–12.5 carried on their own transfer → surface edges), and
  `mars_transfer → mars_surface` (0 m/s, entry 6.0; today's route
  would make you capture first). Airless edges are unchanged apart
  from the landing charge in §2.3. The `only_surface_ascents_are_
  atmospheric` test keeps holding: descents are still never charged
  drag.
  USER:  Rather than adding a large number of edges is there some way we could
  work with our existing abstraction of the solar system to reduce the number of
  edges to somethign close to our current number?  Possibly with some
  re-arranging or addition to our current nodes, and/or breaking delta-v into
  both Holman tranfer burns?  The entry speed of a fast Mars return isn't that
  much more than the return from GTO but since peak heating is something like
  v^3 I guess it matters.
  Also, keep all our units in mks.
  CLAUDE: Done, rewritten above: no new edges, no new edge fields.
  The graph already separates the two Hohmann burns (`earth_escape →
  mars_transfer` is the departure, `mars_transfer → mars_capture` the
  arrival), so aerobraking is "skip the arrival burn and the bound
  edges after it", and the entry speed falls out of what was skipped:
  a node's own entry speed when the chain starts bound, or escape
  speed + the skipped arrival delta-v (periapsis-burn identity) when
  it starts from a transfer. That makes the ladder's small numbers
  honest: Mars return 11 570 m/s against lunar 11 200 is 11 % more heat,
  which is what the v³ model is for. Data added: one optional m/s
  field on ~12 bound nodes and the escape speed derived from the
  gravity and radius the bodies already carry. Units mks throughout;
  the earlier km/s in this file are gone.
- **Descent on an airless body** is the landing burn. The surface edge
  gets a landing charge the way the ascent gets `ascent_gravity_cost`:
  `landing_loss_fraction` (~15 %) of the edge's delta-v for gravity
  loss and hover margin. Throttle: each cycle has a floor
  (`EngineCycle::throttle_floor`: pressure-fed 10 %, expander 20 %,
  staged combustion 40 %, gas generator 60 %, solid 100 %); the
  minimum thrust is `T_min = landing_engines × thrust × floor`, and
  the excess acceleration at touchdown `a_x = T_min / m − g`, in
  m/s² whatever the body. `a_x ≤ 0` hovers: the engines can throttle
  down to the stage's weight. Above that the landing is a hoverslam,
  which cashes out as flaws, not a fixed risk, and gets harder for
  every m/s² of excess whatever the local gravity: the design gets
  `Feature::Hoverslam` and draws extra flaws from the pool entries
  that require it — `hoverslam_complexity_max` (3) × `a_x /
  landing_max_excess_m_s2` of them, all `Analysis` exposure (a stand
  cannot exercise a landing burn; flight review finds them) — and
  every Landing-phase flaw on the stage activates `1 + a_x / a_max`
  times harder. Above `landing_max_excess_m_s2` (20 m/s², absolute,
  config) the burn cannot be timed at all: the designer says "cannot
  land: too much thrust" and a flight that tries is lost at `Landing`.
  The designer verdict reads "hover" or "hoverslam (+2 flaws, ×1.6)".
  USER: To do a good hover the rocket should be able to accelerate
  down to only 100 % of local gravity, but the hoverslam gets more
  difficult for every 1 m/s² above that whatever local gravity is.
  CLAUDE: Fixed above: hover is `a_x ≤ 0`, no ratio margin, and the
  difficulty scale is absolute.
  `Landing`-phase *engine* flaws (deep-throttle instability,
  landing-burn timing) exist only for engines on a stage with
  `Feature::LandingBurn`.
  USER:  The Falcon 9 can do a hoverslam where it isn't actually able to hover.
  We should allow that but make it much more dangerous.
  CLAUDE: Agreed, and that also needs the other half of what Falcon 9
  does: it lands on one engine of nine. So `Stage.landing_engines:
  u8` (how many of the group's engines light for the landing burn,
  default 1, set in the designer) and the gate uses `lit × thrust ×
  floor` against `m g`. Define the touchdown ratio
  `r = T_min / (m g)`: `r ≤ 1.2` is a hover and the Landing event rolls
  only its flaws; above that it is a hoverslam with a physics risk that
  rises with `r` — `hoverslam_risk = max_risk × (r − 1.2) / (r_max −
  1.2)` with `r_max` 3.0 and `max_risk` 0.35 to start (config, tuned
  against the lander bot) — and above `r_max` the burn cannot be timed
  at all, risk 1. The designer verdict reads "hover", "hoverslam
  (risk 18 %)" or "cannot land: too much thrust", and Landing-phase
  engine flaws activate harder under a hoverslam (activation × 2),
  since timing margin is what they eat. A Merlin-class GG engine at
  the 60 % floor on a 25 t stage is r ≈ 2.0, risk ~16 %: dangerous,
  not forbidden, which is where the real one started.
  USER:  We want this to cash out as flaws rather than an irreducable risk.  We
  can have a maximum viable acceleration over local gravity (it should probably
  be in absolute terms) but between that and hoving increase the number and
  difficulty of landing flaws to represent the difficulty.
  CLAUDE: Rewritten above. The ceiling is absolute,
  `landing_max_excess_m_s2` 20 m/s² (one Merlin-class engine at the
  60 % floor on a 25 t stage is ~12 m/s² excess on Earth; the same
  stage on the Moon ~18); between hovering and the ceiling the design
  draws up to three extra Landing flaws, analysis-exposure so only
  flight finds them, and all its Landing flaws activate harder in
  proportion. That is reducible the way launch is: fly it, find them,
  fix them. The only irreducible outcome left is the ceiling itself.
- **Descent with an atmosphere**: chutes bring the stage to its
  terminal velocity; Earth's air brings a sensibly chuted capsule to
  ~7 m/s (splashdown fine), Mars's thin air leaves even a clustered
  stage at 60 m/s above ~1 t, so Mars needs a landing burn after the
  chutes, and a stage denser than the body's `max_chute_beta` gets no
  chute at all and must burn from supersonic — the designer computes
  touchdown speed from the fitted chute and says whether legs, water
  or a burn is needed.
- **Touchdown**: speed against the limit — legs 3 m/s, splashdown
  10 m/s, a bare stage 0 (nothing to land on) — with a risk ramp above
  it, the overexpansion rule's shape.
- **Mass**: shield, legs, chutes and landing propellant come out of
  payload through the planner as today; nothing is hand-set.
- **Designer**: the feasibility table gains an EDL column per surface
  destination — "lands", "splashes down", "burn-through at 11 200 m/s",
  "cannot hover", "no legs" — computed from the design and the body.

### 2.4 Return missions

A mission that comes back: `Contract.itinerary: Option<Itinerary {
via: LocationId, dwell_days: u32 }>` beside `destination` (which is
then `earth_surface`). The route is launch → `via`, a hold leg of
`dwell_days` (zero delta-v, the endurance flaws and the power budget
keep ticking), → `earth_surface`; `launch_rocket` builds it when the
manifest carries such a contract, and the Launches tab shows
"returning". The contract pays on landing; a loss at any point fails
it with the customer's cargo aboard (sample return: irreplaceable,
severity high). `Payload::ContractDelivery` needs nothing new; the
flight does. A landed vehicle on another body persists as a spacecraft
there if `persist`.

### 2.5 Campaigns with lead time

`CampaignSpec` gains `lead_days_range`: the first mission is issued
that long after the announcement, so a program announced in 2004 for
2006 is the player's two years to design, build and test. The bid
window is months. Four archetypes, each a market whose steady volume
is zero (campaign-only), with the destinations and prices below;
every mission pays on landing.

| archetype | from | destination | payload | per mission | missions | lead | notes |
|---|---|---|---|---|---|---|---|
| Orbital Foundry | 2010 | LEO, dwell 30–60 d, return to Earth | 200–600 kg | ~$12–25M | 3–6 every 90–180 d | 1–2 yr | Varda; steady from then on |
| Prospector Sample Return | 2012 | an asteroid surface (Bennu, Eros), dwell, return | 300–800 kg | ~$150–250M | 1–2 | 2–3 yr | a mining company; rare, irreplaceable sample |
| Lunar Surface Program | start | lunar surface | 500–2 000 kg | ~$120–200M | 3–5 every 180–360 d | 2–3 yr | government; failures draw hearings |
| COTS downmass | with COTS | ISS (LEO), dwell, return | 500–1 500 kg | the COTS rate + a return premium | a share of resupply missions | as COTS | reliable once COTS is present |

(Prices are the game's scale, two to three times real, like everything
else; the pot is sized so a lander program is a year's income for the
medium bot, not a side line.) The Programs modal shows what the block
needs: payload, destination, return or not, the entry speed the
vehicle will see.

### 2.6 Bots and bands

- `capsule`: the medium bot's vehicle with a third, capsule stage
  (steel skin, LEO-rated shield, Parachutes, no legs) that bids Orbital
  Foundry and COTS downmass; its first-splashdown rate is the band
  (~60 %).
- `lander`: the medium vehicle under a pressure-fed hypergolic lander
  stage with legs that bids the Lunar Surface Program; first landing
  ~35 %. The bot template takes a third stage (`stages:
  Vec<BotStage>` and an engine per stage); the two-stage templates
  behave as before (oracle).
- Both curves measured at 200 seeds and banded under the measurement;
  the launch bands stay as they are (these bots launch the same
  vehicle); the flaws retune's slack may be spent here.

---

## 3. Steps (one commit each)

1. **Timeline.** `FlawPhase::{Entry, Descent, Landing}`,
   `FlightEvent::{Entry, Descent, Landing}`, `Feature::{HeatShield,
   Parachute, Legs, LandingBurn}`, pool entries requiring them (a
   dozen per phase per domain, exposure leaning integrated/analysis),
   the arrival timeline in `advance_flights` on surface nodes,
   `LandingRecord`, popup, history, review coverage. No design has the
   features and no bot lands: oracle byte-identical, saves unchanged
   (new phases are new enum variants; old flaws keep theirs).
2. **Landing burn and hover.** `EngineCycle::throttle_floor`, the
   surface edge's landing charge in the planner and the route,
   `landing_engines`, the hover test (`a_x ≤ 0`), `Feature::Hoverslam` with its
   targeted extra draws and activation scaling, the absolute ceiling,
   `Feature::LandingBurn` on a design whose top stage is
   asked to land propulsively, `Landing`-phase engine pool; the
   designer's EDL verdict column (airless bodies). Capability probe
   records lunar-surface payloads before and after; bots unaffected.
3. **Heat shield and legs** on stages: `EntryRating`, the heat model
   and the solved areal density over the derived diameter and β,
   legs' mass and limit, `Location.entry_speed_m_s`, the planner's
   aerobraked chain (`aerobrake_delta_v_m_s`, shielded stage alone,
   entry speed from the chain start or the Oberth identity), the entry
   check (rating; no shield never lands through air), the EDL complexity
   bonus, designer rows, costs; the bills of materials; save defaults
   `None` / `false`.
4. **Parachutes**: the two-item catalogue, the `[R] Recovery` editor,
   terminal velocity per body, `max_chute_beta`, touchdown speed
   against legs / water, the
   `Descent` rolls; Mars's "chutes are not enough" shown.
5. **Return itineraries**: `Itinerary` on contracts, the hold leg, the
   route builder, pay-on-landing, the Launches tab's "returning".
6. **Campaigns with lead time**: `lead_days_range`, the four
   archetypes as campaign-only markets, requirements in the Programs
   modal, reputation severities. Supply probe prints a few worlds'
   programs.
7. **Bots and bands**: the three-stage template, `capsule` and `lander`,
   200-seed measurement, thresholds proposed, `[fleet …]` lines.
8. **Record, file**; the dry-mass TODO line goes.

---

## 4. Questions

1. **Shield tiers** (Light / Medium / Heavy by entry speed) rather than
   a continuous rating — agree? And bare steel surviving a Mars entry
   is the one place a shield-less stage lands through air; everything
   else needs a tier.
   USER:  As I understand it only a spacecraft with a very low ballistic
   coefficient can survive reentry without a heat shield on Mars.
   CLAUDE: You are right; the bare-steel exception goes. Every entry
   through air needs a shield, and the tiers stay the heating gate by
   speed. What steel then buys is the ballistic coefficient, which the
   plan uses anyway: `β = m / (C_d · π d² / 4)` from the derived
   diameter. β sets the terminal velocity on the body's air and, on
   Mars, whether the chute can open at all: the atmosphere runs out
   before a dense vehicle slows to chute speed, so Mars has a
   `max_chute_beta` (~200 kg/m², MSL was ~140) above which the
   Descent verdict is "too dense for Mars's air: no chute deploy", and
   the only way down is a landing burn from supersonic, which is
   allowed and charged (the hoverslam rules apply). Shield areal
   densities also scale a little with β (a dense body heats longer),
   `shield_beta_reference` 300 kg/m² with a √ term, so a fat light
   capsule gets a cheaper shield than a slim dense one.
   USER:  We definety want something that results in a *lighter* shield with a
   sturcture behind it that can tolerate more heating.  What can we do to
   simulate this with spherical cow physics?  Maybe calculate the energy being
   shed and then apply a fraction (varying with heat shield) of that to the
   structure.  Minus some value for an ablatative heat shield.
   CLAUDE: That is now the model in §2.2: the shed kinetic energy per
   m² of face, a fraction of it (growing with speed, so Q ∝ β v³)
   reaching the vehicle, the shield ablating a fixed amount per kg and
   insulating the remainder exponentially, and the wall's own heat
   capacity from its skin mass, specific heat and temperature limit
   deciding how much soak it can take. The shield mass is solved from
   those, so the player picks a rating (the entry speed class) and a
   steel capsule gets a ~20 % lighter shield at LEO speeds, less at
   lunar speeds where ablation dominates. Tiers are gone in favour of
   ratings with derived mass; the five rating speeds are the
   discoverable part.

2. **Chute catalogue** of three (drogue, main, cluster) sized by the
   mass class, picked like panels — enough? A capsule would carry
   drogue + main.
   USER:  Would the parachute on a Mars lander be considered a drogue or a main?
   I think for a normal Earth reentry the drogue+main system should be
   considered a single item for simplicity?  But then what to call a Mars
   parachuet?
   CLAUDE: Two items, named by what they do rather than by canopy:
   `Parachutes` — the drogue + main package, one item, rated for a
   touchdown mass class (mass ~3 % of rated, deploys subsonic, so it
   needs thick air: Earth, Venus, Titan) — and `Supersonic Chute` — a
   single disk-gap-band canopy that opens at Mach 2 in thin air
   (Mars), ~2 % of rated mass, which on Mars brings β ≈ 100 to
   ~60 m/s, so a Mars lander is "supersonic chute + landing burn", and
   on Earth it behaves like a drogue alone (~40 m/s, not a landing).
   Each is sized at fit time to the stage's touchdown mass like a
   panel is to its power draw. A capsule carries `Parachutes`; a Mars
   lander carries `Supersonic Chute` and legs and burns. Clusters go
   away: the mass class covers them.
   USER:  Sounds good

3. **EDL complexity bonus** +2 on any design with EDL features as the
   starting knob for "as hard as launch", tuned with the exposure
   mix against the measured curves?
   USER:  Sounds like a good starting point.
   CLAUDE: Noted.

4. **Return itineraries** as a `via` + dwell on the contract, with the
   route built by `launch_rocket` — any other mission shapes you want
   now (two stops, a rendezvous)?
   USER:
   CLAUDE: No answer, so via + dwell only for this pass; a second stop
   or a rendezvous can be a `Vec` of legs later without changing the
   contract's shape (an `Itinerary` with one `via` is the one-element
   case).

5. **Campaign table** (§2.5): destinations, payloads, prices, lead
   times. Anything to move? Should Prospector target a named asteroid
   per campaign (Bennu, Eros, Ryugu-style) so the dwell and the return
   speed differ?
   USER:  Those all look good and the multi-target asteroid campaign would be
   great.
   CLAUDE: Prospector picks a target per campaign from the asteroids
   the graph has a surface for (Bennu, Eros; Vesta and Ceres later if
   we want a bigger payload tier), and the campaign's requirements line
   names it with its return entry speed, since that decides the shield
   rating; both fit under the asteroid-return rating.
   USER:  Lets avoid asteroids with significant gravity wells in the same
   mission, like Ceres.
   CLAUDE: Prospector targets Bennu or Eros only; Vesta and Ceres are
   a different mission class and stay out.

6. **Lander bot** needs a three-stage template; worth doing in this
   sub-plan, or measure the lunar program with the capsule bot's
   machinery and a probe, and build the lander bot with 25_3's work
   on templates?
   USER:  I think we want the lander bot as part of this step to do it right.
   CLAUDE: Agreed; step 7 takes `stages: Vec<BotStage>` with an engine
   per stage (the two-stage templates are unchanged in behaviour, the
   oracle checks that) and builds `lander` on it.

7. **Mars**: physics in (entry speeds, thin-air descent) but no market
   yet — right for this pass?
   USER:  Right.
   CLAUDE: Noted.

---

## 5. Record

**Step 1 — timeline.** `FlawPhase::{Entry, Descent, Landing}` and
`FlightEvent::{Entry, Descent, Landing}` (top group, like payload
deploy; `landing_events(atmosphere)` gives Entry only through air);
`Feature::{HeatShield, Parachute, Legs, LandingBurn}` with 23 rocket
pool entries requiring them, Integrated/Analysis exposure (engine
Landing-phase words wait for step 2, where an engine learns it lands).
`advance_flights` walks the landing events on a surface arrival before
`PayloadDeploy`: a stage loss, or the last engine lost on an airless
body, is a crash (`LandingFailed`, Critical, no launch record, the
contract's cargo lost, reputation as a launch failure); a degradation
is a hard landing that pays the partial fraction; a `PayloadLoss` at
touchdown wrecks the cargo. `LandingRecord`/`LandingOutcome` in
`Company.landing_history`, a Landing Result popup the day it happens
(pauses the clock), a Landing History section on the Launches tab once
there is one, `landings:` in the report, the sim tally counts a crash
as a vehicle lost. The flight review covers the phases reached. No
design has the features and nothing crashes: 100 seeds byte-identical.
