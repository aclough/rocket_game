# 25_1 — Bodies and materials

Sub-plan of `25_EDL.md`: the dry-mass work deferred from 21_2 and the
TODO, done first because entry heating, shield area and bell packaging
need a stage with a size and a skin. Three steps, each its own commit.

---

## 1. Derived diameter

A stage's diameter is never chosen (25_EDL Q5): it follows from what
its tanks hold.

```
V_tank  = Σ_i  propellant_kg × fraction_i / density_i      (m³)
d       = (4 V_tank / (π · tank_fineness))^(1/3)           tank_fineness ≈ 10
```

Checks against real stages (fineness 10):

| stage | propellant | V | d derived | d real |
|---|---|---|---|---|
| Falcon 9 S1 (kerolox 411 t) | 399 m³ | | 3.7 m | 3.66 m |
| Electron S1 (kerolox 9.25 t) | 9.0 m³ | | 1.05 m | 1.2 m |
| Centaur (hydrolox 20.8 t) | 65 m³ | | 2.0 m | 3.05 m (short, fat) |
| bot's BLV S2 (hydrolox 15 t) | 47 m³ | | 1.8 m | — |
| bot's BLV S1 (kerolox 90 t) | 87 m³ | | 2.2 m | — |

Centaur is the shape the formula will not produce (a 10:1 stage is the
norm, a squat one a choice we do not offer yet); fine for now.

**What reads it.**

- **Bell packaging.** The exit budget of each engine on a stage is the
  stage's diameter shared among its engines, `d / √N`, and a long bell
  placed on a stage is the smallest of the engine's own rules (the 3×
  ratio, the 2 m floor, the delta-v optimum, `max_expansion_ratio`)
  and what fits. The engine-level rules stay as the default an engine
  project shows (editor, yardstick) — without a stage there is nothing
  to fit — and the budget only ever shrinks a bell. The variant is
  chosen *per stage*: `design_variant_for_stage(vacuum, cfg, budget)`
  when the designer places or re-sizes a stage (`refresh_derived`), in
  `refresh_rocket_engines`, and in the bot template. Sea-level bells
  are not packaged (Q2).
- **Fairings** are not player-facing yet (`fairing: None` everywhere
  the designer builds a stage; only DinoSoar's vehicle carries one), so
  nothing to derive there today; the EDL plan's heat shield will take
  its area from `d`.
- **The UI** shows the diameter on the stage line in the designer and
  the rocket pane: "S2 1.8 m".

**Gates.** The bot's hydrolox upper is an expander, a vacuum-only
family with no bell choice, so it is not refitted and the medium bot
is byte-identical; bands green. Engine yardstick unchanged (it reads
engines, not stages); an MVac-class engine on a 3.66 m stage takes
ε ~176 (real 165) against 187 alone.

---

## 2. Structural material

A per-stage choice with three options for the first pass (Q3). Every
factor is config (`MaterialsConfig`, one row per material):

| | aluminium (today) | steel | composite |
|---|---|---|---|
| tank mass factor (× today's tank fraction) | 1.00 | 1.15 | 0.70 |
| aero shell and interstage factor | 1.00 | 1.15 | 0.70 |
| thrust structure | unchanged (steel in every case) | | |
| tank bill of materials | 65 % Al, 15 % composites… (today) | 65 % steel, 10 % Al, rest as today | 65 % composites, 15 % Al, rest as today |
| stage build work factor | 1.0 | 0.9 | 1.4 |
| temperature limit (25_2 entry check) | 150 °C | 550 °C | 120 °C |
| cryogenic tank flaw weight (later) | — | — | a hook, not now |

Why those numbers: specific strength puts a cold-worked stainless tank
within ~15 % of 2219 aluminium (Starship's argument) at a fraction of
the price per kilogram, and a flight composite tank at ~30 % lighter
than aluminium after knockdowns (Electron's), at ten times the price
per kilogram and slower to lay up. The material does not change the
structural *model*, only its coefficients and its bill of materials,
so a steel stage is heavier, cheaper and tougher, a composite one
lighter, dearer and slower to build, and aluminium is exactly what
every design is today.

**Where it lives.** `Stage.material: StructuralMaterial` with
`#[serde(default)]` aluminium, so every save and every existing design
is unchanged. `compute_structural_mass` takes the material;
`tank_material_cost` picks the BOM by material; `stage_build_work`
takes the factor. The designer gets a material row on the stage editor
(cycle with ←/→, like the engine cycle row), with the mass and cost
effect shown; the rocket pane shows it in the stage line. The bots stay
on aluminium.

**Gates.** Aluminium is today's numbers: oracle byte-identical, bands
green. A probe prints the bot's vehicle in each material (dry mass,
payload, unit cost, build days) for the record, and the Electron-class
vehicle in composite against the real thing.

---

## 3. Pressure-fed tanks and pressurant

The TODO: pressure-fed stages carry no heavier tanks or pressurant, so
nothing keeps them small. A pressure-fed engine's tanks run at the
chamber pressure plus the feed drop: `tank_pressure = chamber_pressure
× pressure_fed_tank_ratio` (1.3; kerolox/methalox/hydrolox families
run 15 bar chambers, hypergolic 10, so tanks at ~20 and ~13 bar). A
pump-fed stage's tanks run at a few bar for ullage and are what the
tank fraction already describes.

Thin-wall pressure vessel, per cubic metre of tank:

```
m_wall / V = k_shape · p · ρ / σ_allow        k_shape ≈ 3 (cylinder + domes, safety factor in σ)
```

per material (`pressure_kg_per_m3_per_bar`, from ρ/σ): aluminium 2.8,
steel 3.0, composite 1.2. At 20 bar on a kerolox stage (0.97 L/kg) an
aluminium tank adds 20 × 2.8 × 0.97 / 1000 ≈ **5.4 % of propellant
mass** on top of the 4 % base fraction — the small pressure-fed stage
is now a 9–10 % tank, which is Kestrel's stage and why nobody builds a
pressure-fed first stage of any size. The pressurant system (helium at
high pressure plus its bottles) adds `pressurant_fraction_per_bar`
(0.06 % of propellant per bar of tank pressure: ~1.2 % at 20 bar).

Both terms are zero for pump-fed engines and solids (the case is the
motor and already in the engine mass). The structural breakdown gains
`pressure_wall` and `pressurant` lines for the designer.

**Gates.** No bot flies a pressure-fed stage: oracle byte-identical,
bands green. The small-launcher probe's pressure-fed row is the
record: its payload falls (good — it was a free lunch), and the TODO
line goes.

---

## 4. Questions

1. **Fineness 10** and **`d / √N`** per engine for packaging. Agree, or
   would you rather the packaging rule be softer (a bell may overhang
   the stage by some fraction, as the real MVac's 3.3 m does on a
   3.66 m stage it nearly fills)?
   USER:  Agree

2. **Sea-level bells packaged too**, or only vacuum bells? (Today's
   sea-level ε comes from the Summerfield limit alone; the check would
   bind only on absurd clusters.)
   USER:  Just vacuum bells

3. **Material factors** in the table. Anything you want moved before
   it is pinned in tests?
   USER:  Can steel really take 800 C on re-entry?  That seems high, can you
   double check it?  We shouldn't just worry about melting but also excessive
   softness.
   CLAUDE: You're right, 800 was the oxidation figure, not a structural
   one. Austenitic stainless (301/304) keeps ~60 % of its room-
   temperature yield at 500 °C, ~40 % at 650 and ~20 % at 800, and
   creep sets in past ~550 °C under sustained load; Starship's bare
   leeward skin is designed around a few hundred degrees and the
   windward side is tiled. So the *structural* limit for a tank that
   is still holding the stage up is ~550 °C: table changed to 550.
   Aluminium 2219 is honest at 150 (it has lost a third of its strength
   there and collapses past 200); epoxy composite at 120 (resin glass
   transition). The limits are "the skin may reach this during entry
   without a shield", which is why in 25_2 a steel capsule's backshell
   can go bare and an aluminium one cannot; the windward face needs a
   shield in every material.

4. **Pressure-fed numbers**: tank at 1.3 × chamber pressure, 2.8 kg per
   m³ per bar for aluminium, pressurant 0.06 % per bar. Fine as the
   first cut?
   USER:  Sure

5. **Commit of the top-level plan.** `25_EDL.md` goes in with step 1
   here (the series' convention), and `25_2` / `25_3` get written when
   their turn comes?
   USER:  Yup

---

## 5. Record

**Step 1 — derived diameter.** `BodiesConfig { tank_fineness: 10 }`;
`Stage::tank_volume_m3`, `diameter_m`, `bell_exit_budget_m` (`d / √N`),
`fit_vacuum_bell(base, nozzle, fineness)`;
`EngineDesign::with_vacuum_bell_within(cfg, Option<exit_budget_m>)` and
`EngineProject::design_variant_for_stage`. The designer's
`refresh_derived` recomputes structure and refits bells after every
tank, engine or stage change (threaded through `sync_stages_to_projects`,
`resize_*`, `apply_picked_engine_to_designer`); `refresh_rocket_engines`
and the bot template fit per stage. The designer's stage line shows
"⌀1.8m". Checks: Falcon 9's first stage 3.7 m (real 3.66), Electron's
1.05 (real 1.2); an MVac-class engine on a 3.66 m stage ε 176 against
187 alone (real 165). The medium bot's expander upper is a vacuum-only
family and is not refitted: 100 seeds byte-identical. The strategy
bots' kerolox and methalox vacuum bells do fit their stages now (the
smallsat upper, 2.1 t on a 0.63 m stage, goes from ε 250 to ~183), so
their figures move a little: `[fleet smallsat] profitable year
126/200 (was 132), median 13`, `[fleet methalox] 192/200, median 13
(was 14)`, `[fleet solid] 161/200`, basic `bankrupt 4/200, profitable
year 200/200`; all four band tests green. 739 tests, clippy clean.
