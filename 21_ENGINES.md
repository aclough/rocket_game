# 21_ENGINES — What makes one engine different from another

Status: **design settled 2026‑09‑28**; steps in §4. Nothing implemented. Started from
`engine-variation-thoughts.txt` and the TODO audit items (complexity,
scale, RL‑10 cost, Merlin‑class TWR). Answer inline as `USER:`; I reply
as `CLAUDE:`. Steps get firmed up once §2 settles.

## 0. What the game does today

**Engine size.** `EngineSpec { preset, scale }`, scale 0.25–4.0 in
0.25 steps. "Scale 1.0" is a different engine per propellant: 900 kN
kerolox, 700 kN methalox, 110 kN hydrolox, 45 kN hypergolic, 500 kN
solid. Thrust and mass are both linear in scale, so TWR never moves
with size. Hypergolics top out at 180 kN (RD‑253 is 1.6 MN); kerolox
bottoms out at 225 kN (Rutherford 25 kN, Kestrel 31 kN).

**Mass.** A fixed TWR per cycle: pressure‑fed 40, gas generator 80,
expander 50, staged 70, full‑flow 60.

**Complexity** (`balance.rs`): cycle 5–10 and propellant 3–5, combined
as the max (+1 when equal), plus 1 for hydrogen in *effective*
complexity. It drives design work (`(cx/5)^2.5`), build work
(`(cx/5)^1.5`) and flaw generation. A `ComplexityPenalty` deficiency
adds to the project's complexity, so it already reaches build work.

**Unit cost** = materials + labour, times the learning curve
(`n^-0.15`):
- materials = mass × bill of materials × a hand‑tuned propellant premium
  (hydrolox 3.0) × a hand‑tuned cycle premium (full‑flow 2.0);
- labour = build work (line‑days) × manufacturing salary ($10k/day).

**Improvements**: 8 % chance per testing cycle; Isp +1–4 %, mass
−2–6 %, thrust +1–4 %. **Deficiencies** (per technology, seeded): Isp,
mass, thrust −5–20 %, or +1–4 complexity.

### Measured

Bot, 12 seeds × 6 years: each engine actualizes **0–3 improvements**,
ending with TWR **×1.00–1.12**. Merlin went from 1A to 1D (about ×2 TWR)
in about seven years.

Game engines at scale 1 against real ones (real figures are public
estimates, rounded):

| engine | real TWR | real unit cost | game family | game TWR | game cost 1st / 20th |
|---|---|---|---|---|---|
| Merlin 1D | ~185 | ~$1M | kerolox GG | 80 | $2.5M / $1.6M |
| Merlin 1C | ~96 | — | kerolox GG | 80 | |
| RL10 | 40–60 | ~$10–20M | hydrolox expander | 42 | $1.8M / $1.2M |
| RS‑68 | ~51 | ~$20M | hydrolox GG (scale ~28) | 80 | out of range |
| RD‑180 | ~78 | ~$10–25M | kerolox staged (scale 4) | 70 | |
| Raptor 2 | ~140 | ~$1M | methalox full‑flow | 60 | $6.3M / $4.0M |
| RD‑253 | ~150 | — | hypergolic staged | 70 | can't reach 1.6 MN |
| AJ10‑190 | ~23 | — | hypergolic pressure‑fed | 40 | $0.23M |
| NERVA‑class | — | — | nuclear thermal | 4 | **$633M** / $404M |

Reading it: the kerolox gas generator is in the right range; the RL10
is ~10× too cheap; Raptor is off both ways (full‑flow premium and a
fixed TWR of 60, with nothing that makes a complex engine cheap); the
nuclear thermal price is an outlier (§2.6).

## 1. Directions agreed in discussion (2026‑09‑23 → 26)

1. **The RL10 is a 1960 engine.** Hydrogen needs no unlock in 2000,
   but it did when the RL10 was designed. Deficiencies attach to a
   design and stay until it is revised, so an engine designed when a
   technology was immature keeps the cost. Legacy engines in the
   third‑party catalogue (and DinoSoar's RL10‑like Dactyl) carry their
   era's deficiencies; a player's hydrogen engine designed in 2000
   does not.
2. **Cost and build time come from the complexity abstraction.** No
   split into design and manufacturing complexity; instead a
   deficiency (or improvement) can carry a **multiplier on
   construction time**.
3. **Two‑tier improvements**, where the second tier introduces, or
   risks introducing, new flaws.
4. **Store size, show thrust** for the ladder (reasoning in §2.1).

## 2. Design

### 2.1 Size ladder, with chamber pressure as the lever

Replace `scale` with a **size class**: a doubling ladder of throat
area (so thrust roughly doubles per class at a given chamber pressure).
The editor labels each class with the thrust it gives *this* family —
`Size 5 · ~900 kN` — so the player reads thrust but picks a size.

Why size rather than thrust or mass:
- Mass is an output; nobody designs around engine mass.
- Thrust changes over an engine's life once chamber pressure is a lever
  (Merlin 1C → 1D: same size, +60 % thrust); a thrust ladder would stop
  matching the engine after its first uprate.
- Size is what's fixed at design time (chamber, tooling, test stand,
  what fits under a stage). It feeds the vacuum‑bell limit now and the
  stage‑diameter work in EDL later.

One ladder for every propellant; each family's default class is the
one nearest today's scale‑1 thrust. Sketch: 12 classes from ~25 kN to
~6 MN at gas‑generator pressure, each double the last.

Consequences to decide:
- **Thrust at a size** = `C_F × p_c × A_t`, so a staged or full‑flow
  engine gives more thrust than a gas generator of the same size
  (Raptor ≈ 2.5× Merlin at a similar size). That is real, but the
  label then differs by cycle at the same class number.
- **Mass stops being linear in thrust.** Something like mass ∝
  size^0.9 so small engines have worse TWR (AJ10, RL10) and big ones
  better, with a per‑cycle constant as today. Chamber pressure raises
  thrust at close to the same mass, which is how uprates raise TWR.
- **Cost stops being linear in mass** (see 2.2).
- **Bell exit limit.** Short term: a multiple of the engine's own
  sea‑level exit diameter (MVac ≈ 3.6× Merlin, RVac ≈ 1.8× Raptor),
  say 3×, with today's ε cap. Long term: the stage diameter from the
  EDL/structure work, shared by a cluster.
- **Saves**: existing projects map `scale` onto the nearest class, or
  size is stored as a continuous value with the ladder only in the
  editor. The latter keeps old designs exact.

### 2.2 Cost and build time from complexity

Today materials carry two hand‑tuned premiums that only loosely track
complexity, and labour is already complexity‑driven. Proposal:

- **Materials become a floor**: mass × bill of materials, no
  propellant or cycle premium. What a complex engine costs extra is
  labour, which matches real engine costs.
- **Labour is the complexity term**: build work = base × f(complexity)
  × size term. `f` needs to be steeper than today's `(cx/5)^1.5` —
  roughly ×1.3–1.5 per complexity point — to span Merlin to RS‑68.
  The size term goes sublinear (~mass^0.6, as the existing
  `engine_build_mass_exponent` already is).
- **New deficiency kind `BuildTimePenalty(×)`**: multiplies an
  engine's build work. Because labour is line‑days × salary, this is
  also a cost multiplier. It also ties up manufacturing lines for
  longer, so fewer units get built, the learning curve moves slowly,
  and the engine stays expensive — the RL10 loop, emerging on its own.
- **The matching improvement** (a production process change: fewer
  welds, printed parts) divides build work. That is the route to a
  cheap Raptor.

Calibration target (§3): with the RL10 analogue carrying a ×3–5 build
time deficiency and a low build rate, the hydrolox expander lands at
~$10–20M; the kerolox gas generator stays ~$1–2.5M; RS‑68‑class
hydrolox ~5–6× kerolox per kN.

### 2.3 Two‑tier improvements

**Tier 1** as today: small, common, no risk.

**Tier 2 — uprates**: rarer, larger, and each one risks a flaw.

| uprate | effect | example |
|---|---|---|
| Chamber pressure | +15–30 % p_c → thrust up at ~same mass; pad Isp up via the nozzle model | Merlin 1C → 1D, RS‑68A, F‑1A |
| Turbomachinery | mass −10–20 % | Merlin 1D, Raptor 2 → 3 |
| Injector / combustion | Isp +3–6 % | |
| Production process | build time ÷1.3–2 (cost) | Raptor 3, RS‑25E |

The risk is shown before the player accepts: "Chamber pressure uprate
+22 % · 35 % chance of a new flaw". Accepting rolls it; a new flaw is
hidden and found by testing, so an uprate implies requalification. An
uprate can also nudge existing flaws' activation chance up (less
margin).

Where tier 2 comes from, options: testing (as tier 1, lower chance);
**flight heritage** (a chance per flight flown, rewarding flying); or
both. Magnitude check: two chamber‑pressure uprates and one
turbomachinery uprate take the kerolox gas generator from TWR 80 to
~160–190 — Merlin 1D.

### 2.4 Era deficiencies on legacy engines

- Hydrogen stays unlocked in 2000.
- Legacy catalogue engines carry deficiencies from their design era,
  fixed as third‑party flaws already are: an RL10 analogue added to
  the catalogue with "hand‑brazed tube‑wall chamber" as a
  `BuildTimePenalty`; the NK‑33 analogue with whatever its era implies.
- Their purchase price follows from their complexity and deficiencies
  through the same cost model, instead of a hand‑set number, so the
  catalogue stays consistent when the cost model is recalibrated.
- DinoSoar's Dactyl upper stage engine gets the same legacy cost.

### 2.5 More flaw variety (candidate follow‑up plan)

Not in this plan unless you want it: conditional flaws (restart on
multi‑burn routes, sea‑level hard start, cold soak on long coasts,
deep throttle for landing), factory‑floor flaws (scrap rate found in
production), fatigue flaws that grow with flights per unit (for reuse).

### 2.6 Outlier to fix regardless

Nuclear thermal costs $633M per unit: 10 t engine × 15 % HEU × $400k/kg.
Both the HEU fraction (real reactor fuel is a small part of engine
mass) and the price look high. A separate small fix, or folded into the
cost recalibration.

USER:  Yeah, 15% is way too high but the price might be right.
CLAUDE: Agreed; the price stays at $400k/kg and the fraction comes down
to what the reactor actually holds. The US and Soviet surveys were
asked for NERVA and RD‑0410 uranium loadings so the fraction can be set
from data rather than guessed.

## 3. Real‑engine calibration set

The full survey — about 120 engines across the US, Soviet Union and
Russia, Europe, Japan, China and India, with sources — is in
`21_1_ENGINE_SURVEY.md`. This section keeps the engines the model
should be fitted to, tagged by what each one is for, and what the data
says about the design in §2.

Corrections to the §0 table: Merlin 1C is TWR ~68, not 96 (Wikipedia's
figure contradicts its own mass); F‑1's 94 is vacuum, 82 at sea level;
RS‑25 is 54 at sea level.

### 3.1 Core points (what a well‑run programme of each family reaches)

| engine | family | TWR | Pc bar | unit cost (per kN vac) | role |
|---|---|---|---|---|---|
| Merlin 1C | kerolox GG | 68 SL | 68 | | a first production engine: the game's starting point |
| RD‑107A | kerolox GG | 79 SL | 60 | | mature, conservative GG |
| RS‑27A, H‑1 | kerolox GG | 79–93 SL | 44–49 | | mature US GG |
| F‑1 | kerolox GG | 82 SL | 70 | ≤ ~$40M today | big GG |
| Viking 5C, YF‑20B | storable GG | 84–105 SL | 59–71 | | storable GG |
| RD‑191, RD‑180 | kerolox ox‑rich SC | 71–89 SL | 258–267 | $2.2–5.6k | high‑pressure SC: *not* lighter |
| Vulcain 2, LE‑7A | hydrolox GG / SC boosters | 49–53 SL | 117–123 | ~$8k (Vulcain) | hydrolox boosters |
| RS‑68 | hydrolox GG | 46 SL | 103 | ~$6k (weak) | designed to cost |
| RL10A‑4, LE‑5B, HM7B, Vinci | hydrolox upper | 33–59 vac | 35–60 | | small hydrolox is heavy |
| AJ10‑190, Aestus | storable pressure‑fed | 23–27 vac | 9–11 | | long‑life upper engines |

### 3.2 Outliers, and what in the game should produce each

| engine | what's unusual | game mechanism |
|---|---|---|
| Merlin 1D Block 5 | TWR 68 → 183 in ten years, same family | tier‑2 uprates (chamber pressure, turbomachinery) plus production rate |
| Raptor 2 → 3 | TWR 141 → 184; cost halved per generation | uprates and production improvements |
| RD‑253 → RD‑275M | TWR ~140 → ~155 at 147 → 165 bar on the same hardware | chamber‑pressure uprates |
| NK‑33 | TWR 124 from a single‑shaft pump at moderate Pc | a mass (turbomachinery) uprate |
| RD‑0110 → RD‑0124 | same thrust; GG → SC: +33 s Isp, +40 % mass | a cycle change is a new engine, not an uprate |
| LE‑7A → LE‑9 | SC → expander bleed: −15 s Isp, ~+35 % mass, much cheaper | same: cycle choice trades Isp for cost |
| RL10 | ~$134k/kN, ~100× Merlin per kN | legacy era deficiency (`BuildTimePenalty`) plus a low build rate |
| RS‑25 | ~$46k/kN (2015 $) | reusable heritage built expendably at low rate; the RS‑25E's −30 % is a production improvement |
| Vulcain 2 → 2.1 | nozzle −40 % cost, −30 % production time | production improvement |
| Prometheus | 10× cheaper than Vulcain 2 (target) | production improvements *and* a 50/year rate together |
| NK‑33 $1.1M | stockpile sale | not a production cost; ignore for fitting |

### 3.3 What the data says about §2

1. **High chamber pressure does not make a cycle light.** RD‑180 at
   ~265 bar is TWR 71; RD‑275M at 165 bar and NK‑33 at 146 bar are
   124–158. Pumps and structure grow with pressure. So:
   - A chamber‑pressure *uprate* on existing hardware raises thrust at
     nearly the same mass (RD‑253 → 275 → 276: +12 % Pc, +14 % thrust;
     RD‑120: +9 % Pc, +9 % thrust; F‑1 → F‑1A +20 %; Merlin FT).
   - A *design* at higher pressure is heavier per unit throat. §2.1's
     "thrust at a size = C_F × p_c × A_t" needs a mass term that grows
     with p_c, or staged‑combustion families will come out lighter
     than real ones.
2. **Within a family TWR spans about 2.5×** (kerolox GG 68 → 183;
   hypergolic SC 105 → 158), and maturity explains most of it. That
   is the room tier‑2 uprates need: a first design near the bottom,
   a refined one near the top. The game's fixed per‑cycle TWR (GG 80,
   SC 70, FF 60) is roughly "first production engine" for GG and SC
   and too low for full‑flow.
3. **Size: small engines are heavier per kN, but not always.** Small
   pump‑fed upper engines sit at 33–60 (RL10, Vinci, HM7B, LE‑5,
   YF‑75D) and long‑life storables at 11–27, while Rutherford (73) and
   Kestrel (~60) are fine. Part of the upper‑stage penalty is the big
   nozzle rather than size (RL10B‑2 37 vs RL10A‑4 59, same engine) —
   which the nozzle model from 19_NOZZLES.md already charges.
4. **Cost per kN spans 100×**: ~$1–2k (Merlin, RD‑191, NK‑33) to
   ~$5–8k (RD‑180/181, Vulcain 2, RS‑68) to ~$46k (RS‑25) and ~$134k
   (RL10). The middle band is what complexity plus size should
   produce; the two ends are production rate (Merlin, Raptor) and the
   RL10/RS‑25 process‑and‑rate story, which is `BuildTimePenalty` plus
   the learning curve.
5. **Production improvements are 20–50 % per step** in every case with
   numbers (Vulcain 2.1 −40 %, RS‑25E −30 %, Raptor 2 ~−50 %, Skyrora
   printing −20 %). §2.3's ÷1.3–2 is the right size.
6. **Nuclear thermal**: the SNRE design carries ~60 kg of uranium in a
   1,160 kg core, reflector and drums (secondary source). For the
   game's 10 t engine that suggests well under 1 % of engine mass as
   HEU — ~60 kg, ~$24M at $400k/kg — rather than 15 % ($608M).

USER:  Lets try to get a lot more engines here since some of these are going to
be outliers.  Lets look at Russian/Soviet, European, and Japanese engines for
comparison as well.
CLAUDE: Four surveys are running (US; Soviet and Russian; European;
Japanese with Chinese and Indian as a secondary set), each asked for
cycle, propellants, thrust, Isp, dry mass, TWR, chamber pressure,
expansion ratio, unit cost with its year and how solid it is, and
version lineages (Merlin 1A→1D, RD‑170→180→191, Vulcain 1→2, LE‑7→7A,
RS‑68→68A…). Their tables replace this one when they land, with the
engines tagged as core calibration points or as outliers to be
explained by a deficiency or improvement.
CLAUDE (later): Done. About 120 engines are in `21_1_ENGINE_SURVEY.md`
with sources and graded cost evidence; §3.1–3.3 above are the curated
set and what it implies. Two new questions (8, 9) at the end.

## 4. Steps

One commit per step; ask before each unless you give blanket approval.
Balance values below are proposals to review, not decisions.

1. **Engine yardstick probe.** An ignored test in `tests/probes.rs`
   that builds the game analogue of each §3.1 and §3.2 engine and
   prints TWR, Isp, unit cost at units 1 and 20, cost per kN, and
   build days, beside the real figures. It goes first so every later
   step's movement is visible. No game change; oracle byte‑identical.
2. **Nuclear thermal HEU fraction** 15 % → ~0.6 % of engine mass
   (~60 kg on the 10 t baseline, per the SNRE figure), price unchanged.
   Unit cost falls from ~$633M to ~$30M. The bot flies no NTR, so the
   oracle should not move; bands as a check.
3. **Size replaces scale, numbers unchanged.** `EngineSpec` stores a
   continuous size in the kind's natural unit (throat area for a bell,
   input power for electric, area for a sail). A save fold converts
   `scale` exactly, so every existing design keeps its figures. The
   editor's Scale row becomes a Size row: ladder steps labelled with
   the headline figure (thrust, kW, m²), and a wider range than
   0.25–4× — roughly 25 kN to 8 MN for bells. Mass stays linear in
   this step, so the oracle stays byte‑identical.
4. **Mass law and first‑production baselines.** Mass stops being
   linear in size (small engines heavier per kN) and rises with the
   family's chamber pressure (§3.3.1). Baseline TWRs are rebased to
   first production engines (§3.3.2, question 9): kerolox GG ~70,
   with the other families fitted to §3.1. The bell limit becomes a
   multiple of the sea‑level exit diameter instead of a fixed 3 m.
   Balance change: bands, oracle re‑baselined.
5. **Cost from complexity.** Materials become a floor (the propellant
   and cycle premiums go); build work carries the rest, on a steeper
   complexity curve (~×1.4 per point) and a sublinear size term,
   fitted so the yardstick's middle band (§3.3.4) lands on RD‑191,
   RD‑180, Vulcain 2 and RS‑68. Third‑party purchase prices are
   derived from the same model. Balance change: bands.
6. **`BuildTimePenalty` deficiency kind**: multiplies an engine's
   build work only (question 4). Added to the technology deficiency
   roll table and displayed like the other kinds.
7. **Tier‑2 uprates.** Discoverable once an engine has reached a
   testing level *and* flown a minimum number of flights (question 5),
   then rolled per testing cycle. Offered in the revise flow with the
   flaw risk shown up front (question 6); new flaws are hidden until
   testing finds them. Proposed kinds and values:

   | uprate | effect | flaw rolls |
   |---|---|---|
   | Chamber pressure | +15–30 % chamber pressure: thrust up by the same, mass nearly unchanged, pad Isp up via the nozzle model | 3 × 25 % |
   | Turbomachinery | mass −10–20 % | 2 × 25 % |
   | Injector / combustion | Isp +3–6 % | 2 × 20 % |
   | Production process | build work ÷1.3–2 | 1 × 25 % |

   Prerequisites: "Thoroughly Tested" and 5 flights; 3 % per testing
   cycle after that. The bot policy accepts uprates, so the oracle
   moves; bands.
8. **Era deficiencies on legacy engines.** An RL10 analogue joins the
   third‑party catalogue carrying a 1960s‑process `BuildTimePenalty`
   (fixed, as third‑party flaws are); DinoSoar's Dactyl gets the same.
   Their prices come from step 5's model, so the RL10 analogue lands
   near ~$10–20M.

Steps 1–3 change no numbers. Steps 4, 5 and 7 are the balance changes
and each gets the 200‑seed bands and a yardstick comparison in its
record.

### Step 1 record (2026‑09‑28)

`tests/probes.rs::engines::engine_yardstick` (ignored; `cargo test
--release --test probes engines -- --ignored --nocapture`). 29 real
engines — 21 core points, 8 outliers — each beside its game analogue:
same cycle and propellant, sized so its thrust matches on the basis the
real figure is quoted at (sea‑level bell at the pad for boosters,
vacuum bell in vacuum for upper engines; the vacuum sizing iterates
because the bell's gain depends on the throat). Unit cost is materials
plus build work at the line's daily salary, times the learning curve,
through the same functions a build order calls. No game change; oracle
identical (new baseline `oracle_21_base` in the scratchpad: avg final
$196.5M, 2/40 bankrupt — the same as after 19).

**Baseline reading** (the table later steps are measured against):

- **TWR, core points: geometric mean ×0.98 of real.** The per‑cycle
  figures are right on average. The misses are the ones step 4 targets:
  - hydrolox boosters too light (Vulcain 2, RS‑68, LE‑7A: game 62–70
    against real 46–53, ×1.3–1.5);
  - small engines inconsistent: small hydrolox and storable GG engines
    far too light (HM7B ×1.9, S5.98M ×3.0) while small pressure‑fed
    ones are too heavy (Kestrel ×0.41);
  - moderate misses on staged combustion (RD‑191 ×0.73).
- **TWR, outliers: ×0.51** — Merlin 1D, NK‑33, RD‑275M and Raptor at
  ×0.29–0.51. That gap is what tier‑2 uprates (step 7) must be able to
  close from a first production engine.
- **Cost, middle band close already:** RD‑180 game $10.7M at the 20th
  unit against $10–23M; RD‑191 $5.9M against $4.7M; RS‑68 $12.6M against
  ~$20M; Vulcain 2 $4.3M against ~$11M.
- **Cost, the ends:** RL10 $1.2M against ≤ $14.5M (×12 short); RS‑25
  $14.4M against $104.5M (×7 short); Merlin 1D $1.7M and Raptor 2
  $9.4M against ~$1M (too dear — production improvements and rate).
- **Cost per kN spans 1.5–12 $k/kN in the game** against 0.4–146 in
  the survey: the game has the middle and neither end.

### Step 2 record (2026‑09‑28)

`resources::engine_bom(Hydrogen)`: HEU 15 % → 0.6 % of engine mass
(~61 kg on the 10.1 t baseline, per the SNRE figure), superalloys
35 % → 54.4 %. The table summed to 0.95 before — the existing sum test
never covered the hydrogen preset — so the missing 5 % went to
structure as well. First‑unit cost of the baseline NTR: $633M → $50M
($24M uranium at the unchanged $400k/kg, $17M other materials, $9M
labour). New tests: every engine BOM sums to one; a 10 t NTR carries
40–80 kg of HEU and costs $10–60M in materials. 677 tests, clippy
clean, bands pass, oracle identical (the bot builds no NTR).

### Step 3 record (2026‑09‑28)

`EngineSpec { preset, size }`: the engine's size in its kind's natural
unit (`SizeUnit::of_cycle`) — throat area for a bell, input power for
an electric thruster, sail area for a sail (`natural_size`; sails at
`SAIL_THRUST_N_PER_M2_1AU` = 9 µN/m², so the 10 mN baseline is
~1,111 m²). Constructors still take a multiple of the family default
(`DEFAULT_SCALE`; the bot and ~40 tests pass 1.0 or a small multiple),
and store `size = scale × default_size(cycle, preset)`;
`EngineProject::scale(cfg)` gives the multiple back for the formulas
that still take one (design work). Mass and thrust are unchanged —
still linear — so step 4 is where size starts to matter physically.
`MIN_SCALE`/`MAX_SCALE`/`SCALE_STEP` are gone.

**Editor.** The Scale row is a Size row showing the kind's headline
figure (`headline`): `900 kN`; `30 kW (1.0 N)`; `1111 m² (10 mN at
1 AU)`. Enter opens a Size field typed in kN, kW or m²; ←/→ step ×√2;
both clamp to the kind's range (`SizeUnit::headline_range`: 10 kN–
10 MN, 1 kW–1 MW, 10 m²–1 km²) through `scale_for_headline`, which
iterates for vacuum‑only families whose bell gain depends on the
throat. Changing propellant or cycle keeps the headline within a
propulsion kind (a 400 kN kerolox engine becomes a 400 kN methalox one)
and takes the new family's default across kinds. The Engines tab drops
its "Scale: 1.00x". `EditorField::Size` is the engine field; reactors
keep `EditorField::Scale`. `format_thrust_n` shows millinewtons below
0.1 N.

**Saves: v4.** `migrate_json` turns each engine project's flattened
`scale` into `size` = scale × the family default under the save's own
nozzle balance, for the player's and each competitor's projects; a
family without a baseline keeps its design's own size. Designs are not
re‑derived on load, so no loaded engine changes. Test: a v3 project at
scale 2 loads with size = 2 × default and its thrust untouched.

Gates: 679 tests, clippy clean, corpus loads and round‑trips, render
smoke test (the engine field is now `Size`), oracle byte‑identical.

## 5. Questions

1. Size ladder (§2.1): the class label reads each family's thrust, so
   "Size 5" is ~900 kN for a kerolox gas generator and ~2.3 MN for a
   methalox full‑flow. Fine, or should the class name itself be a
   thrust figure at a reference pressure?
   USER:  We might have to think about this a bit more.  What do size classes
   mean when we're talking about electric drives, solar sails, etc?  Maybe it
   should actually be starting mass?  Or do we have equivalent options for
   those?
   CLAUDE: Each propulsion kind has a natural size, and the enum from
   20_PROPULSION.md is the place to hang it:
   - **Nozzle** (chemical, nuclear thermal): throat area, shown as thrust.
   - **Electric**: input power, shown as kW and thrust (NEXT ~7 kW,
     X3 ~100 kW); that is how ion and Hall thrusters are actually sized,
     and it lines up with the power system the stage already has.
   - **Sail**: area, shown as m² and thrust at 1 AU.
   The editor keeps one "Size" row whose label is the kind's headline
   figure, so the player always sees a familiar number.
   I'd avoid starting mass as the size. Mass is what refinement moves:
   Merlin 1C to 1D got lighter at the same size. With mass as the fixed
   input, a mass improvement has nothing to change, and uprates would
   have to show up as thrust at a fixed mass. That can work, but it hides
   the TWR gain that is the point of refinement.
2. Store size as a class index (clean ladder, old designs rounded) or
   a continuous value (old designs exact, ladder only in the editor)?
   USER:  Continuous I think.
   CLAUDE: Continuous it is; the ladder is only the editor's steps, and
   old designs map exactly (size = today's scale × the family's base).
3. Cost model (§2.2): OK to drop the propellant and cycle material
   premiums and let labour carry the difference?
   USER:  Yes, I think that's a good chagne.
   CLAUDE: Noted; labour carries the difference, materials become a floor.
4. `BuildTimePenalty`: a plain multiplier on the engine's build work,
   or should it also apply to the engine's design and revision work?
   USER:  Just build time
   CLAUDE: Build time only.
5. Tier‑2 source (§2.3): testing, flight heritage, or both?
   USER:  I think it should require both.
   CLAUDE: Both, then: an uprate becomes discoverable once the engine
   has reached a testing level *and* flown a minimum number of flights,
   and after that it rolls per testing cycle like tier 1. Thresholds
   are balance values.
6. Tier‑2 risk: shown up front and rolled on acceptance (recommended),
   or hidden?
   USER:  Show up front but I think something like introducing an average number
   of flaws, like 3 25% rolls.
   CLAUDE: Good, that reads well: "Chamber pressure uprate +22 % · may
   introduce up to 3 flaws (0.75 expected)". Rolls and chance per uprate
   kind in the balance config; a bigger uprate can carry more rolls.
   Any flaws it introduces are hidden until testing finds them.
7. Conditional/factory/fatigue flaws (§2.5): later plan, or in this one?
   USER:  Lets add more types of flaws later.  Possibly after EDL
   CLAUDE: Deferred; §2.5 stays as a note for after EDL.
8. (From §3.3.1) A chamber‑pressure *design* choice should cost mass,
   while an *uprate* on existing hardware mostly doesn't. OK to model
   mass per unit throat as rising with chamber pressure, so
   staged‑combustion engines are not automatically lighter?
   USER:  Yes, certainly
   CLAUDE: Mass per unit throat will rise with chamber pressure, fitted
   to §3.1 (RD‑180 vs RD‑275M, Merlin vs Raptor).
9. (From §3.3.2) Should the baseline TWR per cycle represent a *first*
   production engine (kerolox GG ~70, like Merlin 1C and RD‑107) with
   uprates carrying a family up to ~180, or a mature one? The first
   reading makes refinement the way to Merlin 1D; the second makes
   the starting engines stronger but leaves uprates less room.
   USER:  Definetly a first production engine.
   CLAUDE: Baselines become first production engines (kerolox GG ~70,
   Merlin 1C / RD‑107A), and tier‑2 uprates carry a family to ~180.
10. (Question 1, still open) Size by propulsion kind: throat area shown
    as thrust for bells, input power for electric, area for sails, as
    in the reply under question 1. Agreed?
    USER:  Right, that ladder does make sense and we should go with it.
    CLAUDE: Size by kind it is; step 3.
11. Is chamber pressure a player choice when designing an engine, or
    fixed by the family (today's per cycle × propellant table from
    19_NOZZLES.md) and moved only by uprates and deficiencies? A choice
    gives the editor a real trade (thrust per size and pad Isp against
    mass and complexity); fixed keeps the editor as simple as it is now.
    USER (2026‑09‑28, in discussion): a knob too far; start with a
    standard chamber pressure per configuration.
    CLAUDE: Chamber pressure stays fixed per cycle × propellant (the
    19_NOZZLES.md table) and moves only through uprates and
    deficiencies. For the record, a doubling at fixed size would give
    ×2.05 thrust, +2.6 % vacuum Isp and +5 % pad Isp, which would make
    high‑pressure boosters and low‑pressure upper stages emerge on their
    own — worth revisiting when stage diameter exists.
