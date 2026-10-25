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
