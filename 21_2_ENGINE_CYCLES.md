# 21_2 — Engine cycles by size

Status: draft for review (2026‑09‑30). Subsidiary to 21_ENGINES.md.

## 1. Why

The reference-bell fix (commit 6882036) made the hydrolox expander out-Isp
the gas generator (468 vs 459 s). Sweeping vacuum engines from 15 to 250 kN
in every cycle then showed that the cycles still don't find their real
niches:

- At every size the gas generator is lighter, cheaper and simpler than the
  expander. The expander's only edge is 2 % Isp. Real engines chose it for
  simplicity and restarts, not Isp (RL10, Vinci, YF‑75D).
- Nothing stops an expander from growing to booster size. In reality heat
  pickup through the chamber wall limits closed expanders to ~250–300 kN.
- Gas generators stay light when small (15 kN kerolox: TWR 53), so small
  pressure-fed engines and expanders have no niche.
- Kerosene and hypergolic expanders exist and beat gas generators on Isp.
  Neither propellant cools well enough for one, and none have flown.

Hydrolox vacuum engines today (unit cost at the 20th unit):

| Thrust | Cycle | Isp | Mass | TWR | Complexity | $M@20 |
|---|---|---|---|---|---|---|
| 30 kN | Pressure-fed | 459 | 130 kg | 23 | 7 | 0.84 |
| 30 kN | Gas generator | 459 | 70 kg | 43 | 7 | 0.56 |
| 30 kN | Expander | 468 | 76 kg | 40 | 8 | 0.82 |
| 110 kN | Gas generator | 459 | 230 kg | 49 | 7 | 1.22 |
| 110 kN | Expander | 468 | 250 kg | 45 | 8 | 1.76 |
| 250 kN | Gas generator | 459 | 486 kg | 52 | 7 | 2.02 |
| 250 kN | Expander | 458 | 470 kg | 54 | 8 | 2.65 |

## 2. Changes

### A. Expander complexity equals the gas generator's

`balance::cycle_complexity(Expander)` 7 → 6. With the fuel rule
(max of cycle and fuel, +1 when equal, +1 for hydrogen), the combined
complexity becomes:

| | Gas generator | Expander now | Expander after |
|---|---|---|---|
| Hydrolox | 7 | 8 | 7 |
| Methalox | 6 | 7 | 6 |

The expander then costs no more to design or build than a gas generator of
the same mass, and draws no more flaws. Complexity values are integers, so
"one below" would mean 5, the same as pressure-fed; equal seems right.

### B. Expander heat-transfer ceiling

A closed expander drives its turbine with heat picked up in the cooling
jacket. That heat scales with wall area (~F^⅔) while the turbine power it
must supply scales with F, so above a ceiling thrust `F_c` the cycle can't
hold its chamber pressure, and a bigger jacket and longer chamber add mass.

    s  = max(1, F / F_c)        F = the family's vacuum thrust at this size
    pc = pc₀ · s^(−1/3)
    mass × s^0.3

with `F_c` = 250 kN (new `balance_config` value, expressed internally as
the throat area that gives 250 kN at `pc₀`, since size is throat area).
Lower chamber pressure means less thrust per unit of throat, so the same
thrust needs a larger throat; under the 2 m exit bound that leaves a lower
ε, and Isp falls too.

Hydrolox expander (estimates, core TWR without the vacuum bell):

| Thrust | 110 kN | 250 kN | 1 MN | 2 MN |
|---|---|---|---|---|
| pc (bar) | 45 | 45 | 28 | 23 |
| TWR now | 52 | 57 | 65 | 70 |
| TWR after B+C | 48 | 55 | 43 | 38 |

LE‑9 (1.47 MN, 100 bar) is an expander *bleed* cycle, an open cycle this
model doesn't have; it stays an outlier in the yardstick.

### C. Fixed turbomachinery mass

Every turbopump engine carries turbine, bearings, seals and (for gas
generators and preburner cycles) a combustor that don't shrink with the
engine. Replace the pure power law with a fixed part, re-fitted so the
1 MN anchor in `first_production_twr_1mn` is unchanged:

    mass(F) = (m_1MN − m₀) · (F / 1 MN)^0.9 + m₀

where `m_1MN` is today's mass at 1 MN. Per-cycle `m₀` in `balance_config`:

| Cycle | m₀ |
|---|---|
| Pressure-fed | 0 |
| Expander | 20 kg (turbine, no gas generator) |
| Gas generator | 40 kg |
| Staged combustion | 60 kg (preburner) |
| Full-flow | 60 kg |

Core TWR, before → after (sea-level bell, no vacuum bell):

| | 15 kN | 30 kN | 60 kN | 110 kN | 250 kN | 1 MN | 2 MN |
|---|---|---|---|---|---|---|---|
| Kerolox PF | 49 | 53 | 57 | 60 | 65 | 75 | 80 |
| Kerolox GG | 57→23 | 61→34 | 65→47 | 69→57 | 75→69 | 86 | 92→94 |
| Hydrolox GG | 41→20 | 44→29 | 48→37 | 51→44 | 55→52 | 63 | 68 |
| Hydrolox Exp | 43→28 | 46→36 | 49→43 | 52→48 | 57→55 | 65→43 | 70→38 |
| Kerolox SC | 49→17 | 52→26 | 56→37 | 59→47 | 64→58 | 74 | 79→81 |
| Methalox FF | 60→18 | 65→29 | 69→43 | 74→55 | 80→71 | 92 | 99→101 |

Real checks: HM7B (hydrolox GG, 62 kN) game 46 → ~38, real 38. S5.98M
(hypergolic GG, 20 kN) game 60 → ~28, real 21. Kestrel (kerolox PF,
31 kN) unchanged at the core; its vacuum bell is §3's question.

Result: kerolox pressure-fed beats the gas generator on TWR below
~150 kN, and the hydrolox expander beats the gas generator below ~500 kN.

Large engines barely move (≤ 2 % at 2 MN), so boosters and the bot's
first stage should be unchanged; its hydrolox gas-generator upper stage
(~110 kN) gets ~15 % heavier.

### D. Remove kerosene and hypergolic expanders

`chamber_pressure_pa` drops `(Expander, Kerolox | Hypergolic)`, so
`engine_baseline` returns `None` and the editor no longer offers them.
Methalox expanders stay: methane cools adequately and they have been
built and tested.

Saves: an engine project of a removed family migrates on load to the gas
generator of the same propellant at the same size, keeping its flaws,
improvements and history (the save version bumps). Engines already built
into rockets keep their `EngineDesign` and fly as they are.

## 3. Pressure-fed engines (USER's question)

USER: (the small pressure-fed vacuum engine question goes here)

Two things I noticed that may bear on it:

- A pressure-fed vacuum engine now always takes the 2 m bell, because its
  low chamber pressure means a large throat and a small ε at 3× the
  sea-level exit. That trades TWR for Isp, and not by the player's choice:
  AJ10‑class 302 → 315 s but TWR 30 → 20 (real 23); Kestrel‑class 326 →
  340 s but TWR 50 → 31 (real 60, ε 60).
- Pressure-fed stages carry no heavier tanks. Real ones need thick walls
  and pressurant bottles, which is why pressure-fed is for small stages.
  With C, a pressure-fed booster engine would out-TWR a gas generator
  below ~150 kN with no stage-level cost.

## 4. Steps

Each step: `cargo test`, clippy, engine yardstick before/after, 200-seed
bands, oracle rebaselined, recorded here; ask before each commit.

1. A — expander complexity. Moves the DL‑10 Dactyl's price, so check
   DinoSoar's margins and the catalogue probe.
2. C — fixed turbomachinery mass. The bot's upper stage gets heavier;
   bands may need a look.
3. B — expander ceiling.
4. D — remove kerosene/hypergolic expanders, save migration,
   save-compat corpus entry.
5. Whatever §3 decides.

## 5. Questions

1. A: equal to the gas generator (6), or should the expander be simpler
   still in some way that isn't complexity (e.g. fewer flaw rolls, better
   restart reliability once restarts exist)?
   USER:  Lets keep it equal to the generator at 6.
2. B: `F_c` 250 kN and the ⅓ / 0.3 exponents OK? They put a 1 MN
   hydrolox expander at 28 bar and TWR 43, clearly worse than a gas
   generator but still buildable.
   USER:  Yes, as long as there's a clear preference.
3. C: the m₀ values above, or calibrate them further against the survey
   (few small turbopump engines have public masses)?
   USER:  What are you envisioning for further calibration against a survey?
   The fixed constant seems maybe unphysical but we don't have to be perfect as
   long as things roughly line up.
   CLAUDE: Nothing elaborate: add the few small turbopump engines with
   public masses to the yardstick (RD‑58 kerolox staged combustion,
   83 kN, ~300 kg, TWR ~28; HM7B and S5.98M are already there; RL10,
   LE‑5B and Vinci check the expander) and nudge each cycle's m₀ until
   they land roughly, as step 2 of 21_ENGINES did for TWR. No formal fit;
   there are too few points.
   On the physics: you're right that a constant is a stand-in. What's
   really going on is that turbopump mass grows more slowly than thrust,
   roughly with shaft power^0.6, so the pump is a big share of a small
   engine and a small share of a large one. A more physical form splits
   the engine into a part that scales with thrust and a turbopump part
   that scales with thrust^0.6:
       mass(F) = (m_1MN − m_tp) · (F/1 MN) + m_tp · (F/1 MN)^0.6
   with `m_tp` the turbopump's mass at 1 MN per cycle (none for
   pressure-fed). I checked it against the fixed form: to make a 15 kN
   kerolox gas generator as heavy as m₀ = 40 kg does, `m_tp` has to be
   ~740 kg, 62 % of a 1 MN engine, where real turbopumps are more like
   15–25 %. With a realistic share the power law hardly penalises small
   engines at all. So the fixed constant fits better, and it has a
   physical reading of its own: the parts with a minimum practical size
   (bearings, seals, valves, igniter, gas generator, instrumentation)
   that don't shrink with the engine. Recommend keeping the fixed m₀.
   USER:  Ok, fixed m it is.
4. D: migrate old projects to the gas generator (above), or drop them
   with a message?
   USER:  Just migrate them to the gas generator.

## 6. Records

### Step 1 — expander complexity (2026‑09‑30)

`cycle_complexity(Expander)` 7 → 6: hydrolox expander 8 → 7, methalox
7 → 6, level with the gas generator. Isp, mass and TWR unchanged; build
work and so unit cost fall ~27 % (yardstick, 20th unit: RL10 analogue
$1.65M → $1.21M, Vinci $2.23M → $1.65M, LE‑9 $7.94M → $6.15M).
DinoSoar's Dactyl gets cheaper with it: its marginal launch cost falls
$46.8M → $42.1M; awards over the 8 probe seeds 16–22 (was 17–22).
Oracle identical (the bot flies no expander). Bands pass.

### Step 2 — fixed turbomachinery mass (2026‑10‑04)

`turbomachinery_mass_kg(cycle)` (gas generator 40, expander 20, staged
combustion and full‑flow 60, others 0) as `EngineBaseline::fixed_mass_kg`;
`mass = (mass_kg − fixed) · scale^0.9 + fixed`, the 1 MN anchor unchanged.
Yardstick gains Aestus II (hypergolic GG, 55 kN, TWR 41) and RD‑58M
(kerolox SC, 83 kN, TWR 27). Small engines: HM7B 46 → 36 (real 38),
S5.98M 60 → 28 (21), Aestus II 45 (41), RL10 44 → 41 (59), RD‑58M 42
(27, the one miss: a heavy 1970s engine, left alone). Large engines move
≤ 3 %. Core TWR median ×0.99, 16 of 23 within 0.8–1.25×. Falcon 9 probe
unchanged.

Bands first failed: 36/200 bankrupt. HEAD itself was at 29/200 (14.5 %)
against the 15 % ceiling, and on fresh seeds 201–600 bankruptcies were
level (44 → 45), final cash −3 %: the bot's 110 kN hydrolox gas‑generator
upper stage had got heavier. USER chose to switch the bot's upper engine
to a hydrolox expander, now the better engine at that size, and to raise
the 200‑seed bankruptcy ceiling to 20 %. Result: band seeds 31/200
bankrupt, avg final $154M (HEAD $150M); fresh seeds 42/400, $158M (HEAD
44, $149M). Bands pass; oracle re‑baselined (40 seeds: avg final $186M,
3 bankrupt).

### Step 3 — expander heat-transfer ceiling (2026‑10‑04)

`EXPANDER_CEILING_THRUST_N` 250 kN, `EXPANDER_CEILING_PRESSURE_EXPONENT`
⅓, applied in `EngineBaseline::design`. Deviation from §B: pc falls with
the thrust the engine *delivers* (the factor solves F = nominal ·
(F/F_c)^−⅓), and there is no separate mass factor. Applying both on the
throat size stacked them (1 MN hydrolox expander at TWR 22); the lower
pressure already demands a bigger, heavier chamber for the same thrust,
which is the mass penalty.

Hydrolox, vacuum: 500 kN expander 37 bar, 455 s, TWR 49 (gas generator
457 s, 55); 1 MN 29 bar, 453 s, TWR 44 (GG 457, 60); 2 MN 23 bar, 450 s,
TWR 39 (GG 65). Below 250 kN nothing changes. LE‑9 (an expander bleed,
outside the model) falls to TWR 41 against a real 60. Oracle identical
(the bot's 110 kN expander is under the ceiling); bands pass.
