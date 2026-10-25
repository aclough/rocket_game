# 24 — Viable strategies: small lift and medium alternatives

ROADMAP "Add different strategies to the automatic balance testing";
TODO "review different strategies and whether they're viable". The
brief: an Electron / Firefly Alpha–class small launcher should be a
viable way to play, and medium lift should have sensible designs
beyond the kerolox booster + hydrolox upper the bot flies. The user's
guess was that either smallsat revenue or small-rocket cost needs to
move. This plan measures both before touching either.

All figures below come from two new probes in `tests/probes.rs`
(`small_launcher_economics`, `small_contract_supply`), run against
HEAD with the game's own sizing (`autosize_propellant`), cost
(`estimated_build_cost`) and capability (`max_payload_to`) code, so
they are what a player would see in the designer.

---

## 1. What the numbers say

### 1.1 Vehicles, seed 1, sized by the designer's rules

| Vehicle | Liftoff | LEO | SSO | Unit cost (materials) | of which fixed | Build | Design work |
|---|---|---|---|---|---|---|---|
| BLV-class, 2× 750 kN kerolox GG + 100 kN hydrolox (the bot) | 127.5 t | 1 849 kg | 1 321 kg | $19.9M | $12.4M | 257 line-days | 435 + 60 team-days |
| **Electron-class**, 9× 25 kN kerolox GG + 1 vac | 19.2 t | 140 kg | 62 kg | $14.0M | $12.4M | 118 | 89 + 60 |
| **Alpha-class**, 4× 190 kN kerolox GG + 70 kN vac | 64.7 t | 703 kg | 440 kg | $16.4M | $12.4M | 165 | 167 + 60 |
| Mono small, 1× 225 kN kerolox GG | 19.2 t | 166 kg | 83 kg | $13.7M | $12.4M | 151 | 152 + 60 |
| Pressure-fed small, 1× 225 kN | 19.2 t | 89 kg | 22 kg | $13.7M | $12.4M | 135 | 117 + 60 |

Real: Electron 12.5 t → 300 kg SSO, ~$7.5M a flight; Alpha 54 t →
1 030 kg LEO, ~$15M; Pegasus XL 23 t → 443 kg LEO.

Two things are wrong at once:

- **Cost does not scale down.** A 19 t rocket costs 70 % of a 127 t
  one. $12.4M of every two-stage vehicle is fixed material:
  `STAGE_ASSEMBLY_MASS_KG` 500 kg per stage and
  `ROCKET_INTEGRATION_MASS_KG` 800 kg, priced through BOMs that are
  10–30 % wiring and electronics at $600 and $80 000 /kg
  (`resources.rs`). That is $4.4M per stage and $3.6M per rocket
  before a tank or an engine is bought — 88 % of the Electron-class
  unit cost. Revenue for its 62 kg to SSO would be ~$1.9M.
- **Small vehicles lift a fifth of what they should.** Electron-class
  payload fraction 0.7 % to LEO (real 2.5 %); Alpha-class 1.1 % (real
  1.9 %); even the BLV is 1.45 % (Falcon 9 ~4 %, Vega 1.1 %). The
  structure breakdown shows where: for the Electron-class first stage
  the tank is 575 kg, the **thrust structure 574 kg**, interstage
  200 kg, engines 733 kg — dry 2.1 t on 14.4 t of propellant (15 %;
  real Electron ~10 %).
  - `thrust_structure_mass_kg` = thrust / (g · 40): 2.55 kg per kN
    for every stage. Saturn V's S-IC thrust structure was ~0.6 kg/kN;
    modern octawebs are lighter. On the 1.5 MN solid first stage below
    it weighs as much as the motor case.
  - `INTERSTAGE_MASS_KG` is a flat 200 kg: 1 % of the Electron-class
    liftoff mass, 0.16 % of the BLV's.
  - A 25 kN gas-generator engine weighs 81 kg (TWR 31): the fixed
    40 kg turbomachinery floor (`turbomachinery_mass_kg`) is half of
    it. Rutherford is 35 kg with electric pumps; a conventional
    small GG is 40–60 kg.
  - Drag: `aero_drag_loss` = 300 m/s × (500 t / m)^0.2, so 576 m/s
    at 19 t against 394 m/s for the BLV. Plausibly high everywhere but
    not the main term.

### 1.2 Medium alternatives, same sizing, 3 t design payload

| First stage / upper | LEO | SSO | GTO | Unit cost | Build |
|---|---|---|---|---|---|
| kerolox GG / hydrolox GG (bot) | 1 849 | 1 321 | 478 | $19.9M | 257 |
| methalox GG / methalox GG | 1 427 | 977 | 285 | $20.0M | 241 |
| kerolox SC / kerolox GG | 1 509 | 1 034 | 315 | $20.0M | 366 |
| kerolox GG / hypergolic PF | 775 | 410 | 0 | $19.8M | 238 |
| solid 1.5 MN / hydrolox GG | 920 | 564 | 0 | $18.1M | 291 |
| hydrolox GG / hydrolox GG | 2 559 | 1 874 | 805 | $24.2M | 606 |

Medium lift is less broken than small: methalox and staged
combustion land within 20 % of the bot's pair, hydrolox-everywhere
buys the most payload for the most money and build time, which is the
right shape. Two notes: the solid first stage is carrying a 3.8 t
thrust structure for its 1.5 MN (the same constant as above), and
the hypergolic upper (Isp 303) is the one genuinely weak option. All
rows share the 6 t upper stage the TWR-1.0 sizing rule gives a 100 kN
engine, which is why GTO is poor across the board; that is a probe
choice, not a game problem.

### 1.3 The smallsat market

Contracts offered per year, no player activity, seeds 1–4:

| Bucket | per year | mean pay | $/kg |
|---|---|---|---|
| ≤ 300 kg | 3–8 | $2.4–6.4M | 12–29k |
| 300 kg – 1 t | 0–4 | $5–82M | 12–137k |
| 1–3 t | 3–11 | $115–226M | 46–90k |
| > 3 t | 8–18 | $200–325M | 44–67k |

Over four years a world offers 24–31 contracts of ≤ 500 kg worth
$124–277M in total — $30–70M a year, against $3B a year above 3 t.
Rideshare is a pinned market (0.5 contracts/month, LEO 50–500 kg at
$15k/kg, SSO 50–300 kg at $30k/kg, growth 2–8 %/yr, rep target −10 so
it is open on day one); Earth observation (70 % of worlds, from
2005–2012) and government science add the rest of the small rows.

The incumbent never bids on them: DinoSoar's `bid_floor` is $60M and
a $5M contract's ceiling is $6.5M, so the small market is uncontested.
A small launcher's problem is not competition, it is that each flight
earns ~$4.5M and costs $14M to build.

### 1.4 What viability looks like

Revenue ceiling for a company that wins every ≤ 500 kg contract: six
to eight flights a year at ~$4.5M, $30–35M. Overheads: two engineering
teams $3.6M, one manufacturing team $3.6M. That leaves ~$3M per flight
for the vehicle — Electron's reality (Rocket Lab's launch segment only
reached positive gross margin in 2023), and a fine thing for the game
to say: small launch is a thin business that works if you are cheap
and fly often, and a ramp to something bigger. It cannot work at all
while a 19 t rocket costs $14M.

---

## 2. Levers

### 2.1 Fixed material cost scales with the vehicle (the big one)

Replace the two flat masses with a floor plus a share of what they are
attached to:

- stage assembly: `60 kg + 10 % of the stage's structural mass`
  (harness, avionics, umbilicals: a small avionics core is the same
  on any stage, the harness and plumbing grow with it);
- rocket integration: `100 kg + 5 % of the vehicle's dry mass`
  (adapters, fairing hardware, final wiring).

Calibrated so the BLV's fixed share is unchanged at ~$12.5M (so the
bot economy, DinoSoar's `catalog_cost` and the bands all stay where
they are) while the Electron-class falls to ~$3.4M fixed, ~$5M a unit
— which, at the game's $30k/kg SSO rate for a 300 kg payload, is
Electron's margin. Alpha-class lands near $8M. Same formula in
`estimated_build_cost` and the manufacturing pipeline (they share
`resources::stage_assembly_cost` / `rocket_integration_cost`, which
grow a mass argument). Config: `assembly_floor_kg`,
`assembly_fraction`, `integration_floor_kg`, `integration_fraction`
in `CostsConfig`.

Alternative (not recommended): keep the masses flat but cut them to
small-vehicle size, making every vehicle $9M cheaper and re-tuning
the whole economy. The scaled form is the one that is true.

### 2.2 Structure that scales (payload fraction)

- `STRUCTURAL_G_LIMIT` 40 → **150** (0.68 kg/kN, Saturn-class; still
  conservative against modern hardware). BLV S1 thrust structure
  3 824 → 1 020 kg; Electron-class 574 → 153 kg; the 1.5 MN solid
  3 824 → 1 020 kg.
- Interstage: `2 % of the mass it carries (upper stack + payload),
  floor 50 kg` instead of a flat 200 kg.

Both move every vehicle's payload up, the BLV's by a few hundred kg
to LEO, which the planner-dv yardstick (`falcon9` probe) and the
bands will record. Pressure-fed stages still carry no tank penalty
(TODO) — the pressure-fed small row above is already the worst
performer on Isp alone, so this does not open a free lunch, but it is
the next structural item after this plan.

### 2.3 Small engines (maybe)

The 40 kg turbomachinery floor makes a 25 kN GG engine TWR 31. Options:
scale the floor as `40 kg × (thrust / 1 MN)^0.3` (→ 13 kg at 25 kN,
TWR ~45), or leave it and let small launchers pay for small engines as
they do in life (Rutherford's answer, electric pumps, is a cycle the
game does not have). Default: leave it; revisit with the small bot's
numbers in hand.

### 2.4 The market (only if 2.1–2.2 are not enough)

Candidates, in order of how little they distort: rideshare LEO rate
$15k → $20k/kg (Electron sells at ~$25k/kg); rideshare growth
(0.02, 0.08) → (0.05, 0.15) so the 2010s smallsat wave happens; Earth
observation presence 0.7 → 0.85. Decide after measuring the small bot
on 2.1–2.2 alone.

### 2.5 Medium diversity

No cost or physics change needed beyond 2.2 (which rescues solids).
What is missing is pressure: a bot that *flies* an alternative, so
regressions show. The methalox pair is the natural second medium bot
(it also exercises a technology with deficiencies, which no bot does
today); a solid-boosted hydrolox vehicle is the third if wanted.

---

## 3. Strategy bots and bands

`--policy smallsat`: Electron-class template (9 + 1 kerolox GG engines
of one family, design payload 300 kg), 2 engineering teams,
1 manufacturing team, auto-build target 2, standing bids on
rideshare / Earth observation / government science up to its
capability, launching from the start (no quiet-cycle gate). Its
reliability curve will be lower than the medium bot's — more engines,
the same flaw rules — which is also true to life.

`--policy methalox`: the basic bot on a methalox booster and upper,
so technology deficiencies and revisions are on the critical path of a
band run for the first time.

Bands: the existing 200-seed set stays on `basic`; each new policy
gets its own `[fleet]` print and its own thresholds, set from the
first measurement rather than guessed (the 22 pattern: measure,
propose, the user picks). The smallsat bot's success criterion is
"survives and is profitable in most worlds", not the medium bot's
$25M-kept band.

---

## 4. Steps (one commit each)

1. **Probes.** `small_launcher_economics` and `small_contract_supply`
   as they are (already written, uncommitted), plus this plan.
2. **Fixed material cost scales with the vehicle** (2.1): the four
   config fields, shared cost functions taking masses, the
   manufacturing test that pins estimate = pipeline charge, the probe
   re-run recorded; BLV unit cost within ±2 % of today, bands green.
3. **Structure** (2.2): g-limit and proportional interstage, structure
   tests re-pinned, `falcon9` yardstick and the bands/curve recorded.
   **3b. Small engines** (2.3): the turbomachinery floor scales with
   thrust; engine tests re-pinned, `engines` yardstick recorded.
4. **Smallsat bot** (3): template, overheads, bid rules; measure 200
   seeds and report; thresholds chosen by the user; `[fleet]` print
   and `sim_bands` test.
5. **Market tune** (2.4) only if step 4's numbers call for it.
6. **Methalox and solid bots** and their bands.
7. Record, file, roadmap.

---

## 5. Questions

1. **The cost formula (2.1).** Calibrating so the BLV is unchanged
   keeps every gate still and makes the small vehicle ~$5M. Agree, or
   would you rather medium lift also got cheaper (with DinoSoar and
   the bands re-tuned to match)?
   USER:  Lets keep the currrent test vehicle the same.
   CLAUDE: Calibrated form it is: the BLV's fixed share stays ~$12.5M.

2. **Structure (2.2).** g-limit 150 and a proportional interstage move
   every vehicle's payload up by a few percent of liftoff mass; the
   bot's curve should not move (flaws are per design, not per kg) but
   the fleet bands will drift profitable. Fine to take as a physics
   correction gated on the yardstick and bands?
   USER:  Yes, we can use the slack to make flaws worse later maybe.
   CLAUDE: Taken as a physics correction; the fleet drift gets recorded
   in step 3 so the slack is a known number when flaws come up again.

3. **Small engines (2.3).** Leave the 40 kg floor for now?
   USER:  For this juncture I'm going to default to what works gameplay-wise so
   go for it.
   CLAUDE: Scaling the floor, then: `turbomachinery_mass_kg(cycle) ×
   (thrust / 1 MN)^0.3`, so a 25 kN gas generator carries ~13 kg of pumps
   instead of 40 and lands near TWR 45; the 1 MN anchor is unchanged.
   Its own commit (step 3b) so the yardstick shows it apart from the
   structure change.

4. **Which bots (3).** `smallsat` and `methalox` as the two new
   policies? Anything else you want exercised (solid-boosted,
   GEO-direct with a bigger upper stage)?
   USER:  I'd like one design using a solid rocket motor a la the PSLV or the
   new crop of chinese startups, also because we need to get some testing on
   solids.
   CLAUDE: Three new policies then: `smallsat`, `methalox`, and `solid` —
   a solid first stage under a liquid upper (PSLV / Ceres-1 shape), the
   smallest of the three medium vehicles so it also tests solids at
   a size players will build. Step 6 becomes "methalox and solid bots".

5. **What "viable" means for the small bot.** I propose: bankrupt
   ≤ 25 %, ever-profitable ≥ 50 %, ≥ 10 launches in six years median —
   a thin business that works, not a money machine. Pick after the
   first measurement?
   USER:  That sounds reasonable.  Even a bit worse would be fine but it should
   be possible at all.
   CLAUDE: Thresholds proposed from the first 200-seed measurement in
   step 4, erring loose.

---

## 6. Record

Approval: steps 1–4 may be committed without asking (2026-10-25,
"do your own commits for the first stages"); the market tune (step 5)
is to be talked over first.

**Step 2 — fixed material cost scales with the vehicle.** Stage
assembly hardware is `60 kg + 12 % of structural mass (capped at
7 t)` and rocket integration `100 kg + 6 % of dry mass (capped at
12 t)`, through the same two functions the pipeline and the bid
estimate share (`stage_assembly_cost`, `rocket_integration_cost`,
now taking the masses); `new_integration` takes the vehicle's dry
mass (`RocketDesign::dry_mass_kg`). Config: `assembly_floor_kg`,
`assembly_fraction`, `assembly_cap_kg`, `integration_floor_kg`,
`integration_fraction`, `integration_cap_kg`. Calibrated on the bot's
own template (6 450 + 1 500 kg structure, ~10 t dry): its fixed share
is $12.6M against $12.4M before. The caps exist for DinoSoar's 26 t
core: linear scaling had raised its material cost by half (and broke
`dino_success_pays_and_boosts_rep`'s same-day headroom); capped, the
Brontosaur pays ~$3.4M more per unit before learning, ~4 % of its
cost. Probe: Electron-class $14.0M → $5.4M, Alpha-class $16.4M →
$12.3M, mono small $5.0M; the autosized BLV-class $19.9M → $20.5M
(its 8.7 t structure is heavier than the bot's template). 100 seeds:
bot unit cost 19.8M → 20.1M, curve 35 / 61 / 76 / 98 unchanged.
200 seeds: `[fleet] bankrupt 3/200, kept $25M 178/200, profitable
148/200, profitable year 199/200, aggregate success 90.3%`, bands
green. 732 tests, clippy clean.

**Step 3 — structure that scales.** `STRUCTURAL_G_LIMIT` 40 → 150
(2.55 → 0.68 kg per kN); the interstage is 2 % of the wet stack it
carries with a 50 kg floor instead of a flat 200 kg, carried by the
core of a group only (`compute_structural_mass` takes
`interstage_carried_kg: Option<f64>`, `recompute_structural_masses`
runs top-down so the stack above is settled first, `autosize` passes
the stack it already knows). The bot's template now takes its
structure from the model (`recompute_structural_masses`) instead of
the hardcoded 6 450 / 1 500 kg, with its panels sized afterwards —
sizing them first left the upper stage 68 kg short and
`survives_trip` false, which is what tripped the policy and guide
tests. Falcon 9 yardstick: the game structural model on Merlin now
lifts 17 174 kg to LEO against 17 747 kg with the real dry masses
(was 14 089) — the structure is no longer the gap, the engines are.
Probe: Electron-class 140 → 227 kg LEO, 62 → 142 kg SSO, $4.3M;
Alpha-class 703 → 992 kg LEO (real 1 030), $9.3M; the solid first
stage 920 → 1 320 kg; the bot's autosized class 1 849 → 2 453 kg.
100 seeds: unit cost 20.1M → 18.5M (lighter tanks and harness),
payment 40.5M → 37.4M. 200 × 8: curve 38 / 62 / 76 / 97 (was
36 / 63 / 81 / 97 — the bot flies a different vehicle, so different
draws; 3rd-flight 153/200 vs 162/200 is inside two sigma), `[fleet]
bankrupt 2/200, kept $25M 189/200, profitable 152/200, profitable
year 199/200, aggregate 90.4%`, bands green. 732 tests, clippy clean.

**Step 3b — small engines shed most of the pump floor.** The
turbomachinery part of a pump-fed engine's mass scales as
`size^0.3` (`TURBOMACHINERY_SIZE_EXPONENT`): 40 kg at 1 MN as
before, ~13 kg at 25 kN. A 25 kN kerolox gas generator is 55 kg
(TWR 46) instead of 81 (TWR 31); the 900 kN baseline loses one
kilogram. Engine yardstick otherwise unchanged (Merlin 1C TWR 68 =
real; Kestrel and RL10 are pressure-fed/expander cases the floor
never touched). Probe: Electron-class 227 → 273 kg LEO, 142 → 185 kg
SSO, $4.0M (real: 300 kg SSO, ~$7.5M a flight); Alpha-class 1 021 kg
LEO (real 1 030), $9.1M. 200 × 8: curve 36 / 64 / 79 / 97, `[fleet]
bankrupt 1/200, kept $25M 191/200, profitable 160/200, profitable
year 200/200, aggregate 91.0%`. **The "stay dangerous" floor
(≥ 1 % bankrupt) tripped at 1/200**; lowered to 0.5 % with a note
that steps 3/3b's slack is reserved for the flaws retune (Q2). The
basic bot's economy is now: unit cost 17.3M, payment 34.6M, first
launch month 19.4. 732 tests, clippy clean.

**Step 4 — the smallsat bot.** `BasicPolicy` now runs a `BotTemplate`
(engine families, two-stage stack, payroll): `MEDIUM` is the basic
bot unchanged (100 seeds identical), `SMALLSAT` is nine 25 kN kerolox
gas generators under 15.3 t and the family's vacuum bell under 2.1 t
— 19 t, 288 kg to LEO, 197 kg to SSO, one engine programme — on two
engineering teams and two lines; `ALPHA` (four 190 kN under 53 t,
~1 t to LEO) for comparison. `--policy smallsat`, `alpha`, and
`<template>:<eng>,<mfg>[,<S1 kg>,<S2 kg>]` for sweeps. The band test
gains `smallsat_fleet_200_seeds` (figures only) and
`tests/probes.rs` a `smallsat_bids` probe.

Measured, 200 × 8: `[fleet smallsat] bankrupt 0/200, kept $25M
200/200, profitable 0/200, profitable year 2/200, aggregate success
71.8%, median launches 4`; first launch month 15, dev spend $27M,
unit cost $6.6M (materials ~$4M, the rest labour). Alpha: profitable
year 81/200, unit cost $11.7M, payment $22.7M, median ~2 contract
flights.

**Why it cannot work yet.** After its first success the small bot
never wins a contract: the standing rule bids cost × (1 + margin) =
$14.3M on a 100 kg TechDemo paying $1.4M (ceiling ~$1.8M). Every
contract it can lift (≤ 260 kg LEO, ≤ 180 kg SSO) pays $1–6M; the
vehicle costs $6.6M to build. Payroll alone (two teams, two lines,
$10.8M a year) burns $107M over eight years while two rockets sit
on the shelf. The vehicle is right (real Electron: ~$5–6M to build,
300 kg to SSO); the *prices* are the gap. The game prices every
smallsat contract per kilogram as rideshare ($15k/kg LEO, $30k/kg
SSO), which is what a 100 kg satellite pays to ride along on a big
rocket (Transporter: $5–6k/kg), not what it pays for its own launch
to its own orbit (Electron: ~$7.5M whatever the mass). There is no
customer in the game who pays for the rocket rather than the
kilograms — which is the whole reason small launchers exist. That is
the market question of step 5, to be talked over:

- *A. A per-launch floor.* Each destination carries a minimum payment
  (`min_payment` on `MarketDestination`): rideshare/EO LEO ~$5M, SSO
  ~$6M; a 100 kg contract pays $5M instead of $1.4M, a 400 kg one is
  unchanged. Smallest change; keeps one market.
- *B. A dedicated-smallsat market.* A new archetype, "Dedicated Small
  Launch", 50–300 kg to LEO/SSO at a per-launch price ($6–10M), rep
  target low, emerging ~2006 and growing fast through the 2010s;
  rideshare keeps its cheap $/kg as the big-rocket alternative. Most
  faithful: it also explains why medium lift leaves this money alone.
- *C. Raise rideshare $/kg* (LEO $15k → $30k, SSO $30k → $50k).
  Simplest, but a 500 kg rideshare then pays $15M, which is wrong in
  the other direction.
- *D. Bidding that respects the ceiling* (bid = min(cost × (1 + m),
  ceiling) when cost < ceiling), for the bot and the player's rule
  engine alike. Needed with any of the above; useless alone.

Recommendation: B with D (A with D if the smaller change is
preferred). Either way the small bot's thresholds (bankrupt ≤ 25 %,
profitable year ≥ 50 %, median ≥ 10 launches) get set from the
measurement after it.

**Step 5 — the smallsat market.** Decided with the user (2026-10-25):
rename, a per-launch floor, higher rates; precise-orbit versus
rideshare missions are a later distinction. "Rideshare / Smallsat" is
**"Smallsat Launch"** ("…buying a dedicated ride … to their own
orbit"); `MARKET_RIDESHARE` is `MARKET_SMALLSAT`; the archetype key
stays `market_rideshare` because it seeds the world's growth draw and
is in saves. `MarketDestination.min_payment` (serde default 0) floors
a contract's list price before variance, the economy rate multiplier
and campaign discounts, and is realized with the seed's rate
multiplier like `rate_per_kg`. Smallsat LEO $15k → $25k/kg with a
$6M floor, SSO $30k → $40k/kg with $7.5M; Earth observation keeps its
rates and takes the same floors (a 100 kg imaging satellite buys its
own launch too). Every other destination is unchanged at floor 0.
Bots carry their own markup (`BotTemplate.bid_margin`: medium 1.0 as
before, smallsat 0.25 — Electron sells at about a quarter over cost —
alpha 0.5). On load, a saved world's market names and pricing are
refreshed from the default archetypes realized for its seed
(`refresh_market_pricing`), keeping the seed's multipliers and every
realized volume, growth and modifier; the `m1`–`m5` corpus and a
test with the old name cover it.

Supply: the ≤ 500 kg pot over four years went from $124–277M to
$200–375M per world; a ≤ 300 kg contract now averages $7–9M.

Measured, 200 × 8, `--policy smallsat`: `[fleet smallsat] bankrupt
0/200, kept $25M 200/200, profitable 0/200, profitable year 132/200,
aggregate success 89.5%, median launches 13`; unit cost $5.6M (with
learning), payment $6.7M, first launch month 15, curve 44 / 65 / 84 /
97. Payroll variants: 2 teams + 1 line 78/200 profitable years (first
launch month 34 — engines queue on one line); 2 + 3 124/200; 3 + 2
105/200. Alpha-class 104/200 profitable years, unit cost $11.4M,
payment $16.7M. Nobody ends above the $250M they started with: a thin
business that works, which is the brief ("it should be possible at
all"). Bands set in `smallsat_bands_200_seeds`: bankrupt ≤ 25 %,
profitable year ≥ 50 %, median ≥ 10 launches, aggregate ≥ 80 %.

The basic bot is unaffected (100 seeds identical; 200 × 8 curve
36 / 64 / 79 / 97, `[fleet] bankrupt 1/200, kept $25M 191/200,
profitable 160/200, profitable year 200/200, aggregate 91.0%`). 733
tests, clippy clean.

**Step 6 — methalox and solid bots.** `METHALOX`: two methalox gas
generators (~750 kN) under 90 t and a methalox vacuum bell (~100 kN)
under 12 t on the basic bot's payroll — the one bot whose critical
path runs through technology deficiencies. `SOLID`: one 1.5 MN solid
motor under 58 t of grain and the basic bot's hydrolox expander under
8 t (PSLV / Ceres-1 shape) — the one bot that builds and flies solids.
`--policy methalox|solid`, sweeps as for the others. The band test
shares `assert_strategy_bands` across the three strategy bots.

Measured, 200 × 8:
- `[fleet methalox] bankrupt 10/200, kept $25M 167/200, profitable
  34/200, profitable year 186/200, aggregate success 83.4%, median
  launches 14`; curve 27 / 44 / 77 / 92, unit cost $15.9M, payment
  $30.9M, first launch month 14.5. Viable and visibly harder than
  kerolox: the deficiencies cost it first-flight reliability (27 %
  against 36 %) and a tenth of worlds go under. Bands: bankrupt
  ≤ 15 %, profitable year ≥ 80 %, median ≥ 6, aggregate ≥ 80 %.
- `[fleet solid] bankrupt 2/200, kept $25M 177/200, profitable 1/200,
  profitable year 156/200, aggregate success 76.6%, median launches
  9`; curve 28 / 35 / 76 / 91, unit cost $13.9M, payment $27.4M,
  first launch month 19.6. Viable but thin, and its **second flight
  succeeds 35 % of the time** against the medium bot's 64 %: the
  solid's flaws are not being found between flights the way a liquid
  engine's are — a flaw-model question about solids (bench exposure,
  flight review) worth its own look, recorded here rather than tuned
  away. Bands: bankrupt ≤ 15 %, profitable year ≥ 65 %, median ≥ 4,
  aggregate ≥ 70 %.

The basic bot is untouched. 733 tests, clippy clean.
