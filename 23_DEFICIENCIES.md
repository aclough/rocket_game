# 23 — Deficiencies: bite harder, more kinds, more technologies

Roadmap item: *"More significant deficiencies."* The user's ask
(2026-10-10): make the deficiencies bit a little harder, and give them
some more diversity. First-round answers (2026-10-11), kept verbatim
in §6: the solving mechanics are fine as they are; what is wanting is
the size of the maluses before they are solved. New kinds wanted:
material cost, harder testing, and more electrical power for electric
drives. No chains, no partial solves, hints are fine. Hydrolox stays
mature; staged combustion becomes a technology, and a tap-off cycle
could too if one existed.

---

## 1. Where deficiencies are now

Three technologies, each with a seeded list a new design inherits
(less what the company has solved, since 22_FLAWS step 10):

| Technology | Unlocked | Difficulty | Count | Solvability per attempt | Magnitude draw |
|---|---|---|---|---|---|
| Methalox | at start | 0 | 0–2 | 0.0–1.0 | 5–20 % |
| Nuclear thermal | yearly roll, 8 % | 2 | 2–4 | 0.0–0.8 | 15–30 % |
| Fission reactor | yearly roll, 8 % | 2 | 2–4 | 0.0–0.8 | 15–30 % |

**Kinds** today, all taking the same magnitude draw `m`: Isp −m,
mass +m, thrust −m, build time ×(1 + 4m), complexity +1..(2+d);
reactors power −m, mass +m, complexity. Three stock descriptions each.

**Solving** is unchanged by this plan: one attempt per deficiency per
revision at `flaw_revision_work`, succeeding at `solvability` (×3,
capped 0.95, once another of the company's designs has solved it),
retiring for the company on success.

Every cycle, every propellant but methane, and electric propulsion
are mature from day one.

---

## 2. Bite harder: magnitudes per kind

One draw does not suit every kind. Ten per cent off Isp is a different
engine; ten per cent on mass is a Tuesday. So the draw `m` grows, its
top end grows with difficulty, and each kind scales it. The brief
(second round): low ends about here, high ends a lot worse,
difficulty 2 worst of all — frequently a deficiency should make the
technology essentially unusable until it is fixed. Hard-to-test is
about right as it was.

`m` ∈ [0.10 + 0.05d, 0.50 + 0.25d]: d0 0.10–0.50, d1 0.15–0.75, d2
0.20–1.00.

| Kind | Today (d0 / d2) | Scale | d0 | d2 |
|---|---|---|---|---|
| Isp | −5..20 % / −15..30 % | −0.4 m | −4..20 % | −8..40 % |
| Thrust | −5..20 % / −15..30 % | −m, capped at −80 % | −10..50 % | −20..80 % |
| Mass | +5..20 % / +15..30 % | +1.5 m | +15..75 % | +30..150 % |
| Build time | ×1.2..1.8 / ×1.6..2.2 | ×(1 + 4m), as now | ×1.4..3.0 | ×1.8..5.0 |
| Complexity | +1..2 / +1..4 | as now | | |
| Power (reactor) | −5..20 % / −15..30 % | −0.7 m | −7..35 % | −14..70 % |
| **Material cost** (new) | — | ×(1 + 2m) | +20..100 % | +40..200 % |
| **Hard to test** (new) | — | +k on every discovery exponent, k = 1..(1+d) | +1 | +1..3 |
| **Power draw** (new, electric) | — | ×(1 + m) | +10..50 % | +20..100 % |

At the top of difficulty 2 an engine has lost 40 % of its Isp, or
80 % of its thrust, or weighs two and a half times what it should: it
is a programme, not a product, until that is revised out — which is
the brief. Config: `deficiency_magnitude_base` 0.05 → 0.10,
`deficiency_magnitude_per_difficulty` 0.05 (as now),
`deficiency_magnitude_span` 0.15 → 0.40, new
`deficiency_magnitude_span_per_difficulty` 0.25, and a per-kind scale
table (`isp_scale` 0.4, `thrust_cap` 0.8, `mass_scale` 1.5,
`power_scale` 0.7, `material_cost_scale` 2.0) beside the existing
`build_time_penalty_scale` 4.0.

Counts and solvability stay where they are, as asked.

---

## 3. More kinds

### 3.1 `MaterialCost(m)`

The engine's bill of materials costs ×(1 + m): exotic alloys, a
coating only one shop can do. Lives on the project as
`material_cost_multiplier` (beside `build_time_multiplier`), applied
where an engine build order prices its materials
(`engine_material_cost`) and in the build-cost estimate the Engines
pane and the designer show; reverted on solve. Catalogue prices are
not touched — vendors have no deficiencies.

### 3.2 `HardToTest(k)`

The design's flaws hide longer: every discovery exponent the project
rolls at is raised by `k` (`DesignProject::discovery_exponent_for`
adds `self.test_difficulty`), including during a flight review. An
engine with it ships with more hidden flaws for the same testing, and
the player sees why on the project ("instrumentation immature").

### 3.3 `PowerDrawPenalty(m)` — electric engines

An electric engine draws ×(1 + m) the power per newton:
`EngineDesign::apply_deficiency` scales `Propulsion::Electric {
power_draw_w }`. Panels sized to the design's demand size up with it,
thrust under a power shortfall derates through the existing
`group_effective_thrust_n`, so a stage that was fine becomes
power-starved until the deficiency is solved. Only an electric design
can draw this kind (the generator checks the family).

Descriptions: three per new kind per domain, methane-, staged-
combustion- and electric-flavoured where the family is known.

---

## 4. More technologies

### 4.1 Staged combustion

`TECH_STAGED_COMBUSTION`, unlocked at start, difficulty 1, carried by
every `StagedCombustion` design on any propellant. Count 1–3,
solvability 0.0–0.9, and the engine kinds above (no power draw). A
tap-off cycle, if one is added, joins here.

**Full-flow** is its own technology, `TECH_FULL_FLOW`, locked at
start and rolled yearly like nuclear (8 %), difficulty 2: a full-flow
design carries it (and not the staged-combustion one — the harder
technology subsumes the easier). Until it unlocks the cycle is not on
the editor's list, as nuclear thermal is not.

### 4.2 Electric propulsion

`TECH_ELECTRIC`, unlocked at start, difficulty 1, carried by every
`ElectricPropulsion` design: the home of `PowerDrawPenalty`, with
thrust, mass, hard-to-test and material cost besides. Without a
technology there is nowhere for "electric drives need more power" to
live.

### 4.3 A design on two technologies

A methalox staged-combustion engine is on both. Today a project has
one `technology_id`; it becomes `technology_ids: Vec<TechnologyId>`
(serde: the old single field migrates to a one-element list), the
design completing inherits the unsolved deficiencies of every
technology it is on, the ⚗ cue names them all, and the company's
solved set works as it does. `technology_for_preset(preset)` becomes
`technologies_for(cycle, preset) -> Vec<TechnologyId>`. This is the
one data-structure change in the plan.

### 4.4 Who is affected

The basic bot flies a kerolox gas generator and a hydrolox expander:
no technology, so the curve and the fleet bands do not move. DinoSoar
flies a hydrolox gas generator and buys its upper stage: likewise. The
vendor catalogue's staged-combustion booster is mature by definition.
Only a player (or a future strategy bot) reaching for staged
combustion, methane or electric drives pays.

---

## 5. Steps (one commit each)

1. **Magnitudes.** The larger draw and the per-kind scales (§2), with
   the generation tests re-pinned and a probe that prints a seed's
   deficiency lists for all three technologies.
2. **`MaterialCost`** on engines and reactors, through the build order
   and the cost estimate.
3. **`HardToTest`** through the discovery exponent.
4. **`technology_ids`** (§4.3): the Vec, migration, inheritance from
   every technology, the cue.
5. **Staged combustion and full-flow** as technologies (§4.1), the
   full-flow cycle gated on its unlock.
6. **Electric propulsion** as a technology with `PowerDrawPenalty`
   (§4.2, §3.3).
7. **Descriptions** per kind and family (§3.3's last line), and the
   plan's record.

---

## 6. First-round answers

1. **How much harder?** USER: I wasn't actually thinking of any of the
   three dials you outlined. Those seem good right now. Rather I was
   thinking that the magnitude of the maluses they provide before
   they're solved maybe aren't high enough.
   CLAUDE: §2 is now the magnitude table; the dials are dropped.
2. **Which new kinds?** USER: I think MaterialCost and HardToTest are
   the sort of thing I was thinking of. But also needing more
   electrical power for electrical drives is something that should
   certainly be in there.
   CLAUDE: §3, with `PowerDrawPenalty` needing electric propulsion to
   be a technology (§4.2).
3. **Chains?** USER: Nah.
4. **More technologies?** USER: Hydrolox is too well established.
   Staged combustion is a good candidate though. If we were to add a
   tap off cycle we could do it there too.
   CLAUDE: §4.1; a tap-off cycle is noted as joining it when it exists.
5. **Partial solves?** USER: Eh, probably not worth it.
6. **Hints?** USER: I think things are good there.

---

## 7. Questions

1. **The magnitude table** (§2): is the shape right — Isp gentler than
   mass, materials and thrust in the middle — and are the top ends
   (mass +45 % at difficulty 0, +60 % at 2) where you want "bites
   harder" to land?
   USER:  Even harder than this, I think.  We're looking for stuff where
   frequently a deficiency makes a technology essentially unusable until they
   get fixed.
   USER (chat): Generally the proposed low ends are good but make the high
   ends a lot worse, especially for difficulty 2. Except I think hard to test
   is about right.
   CLAUDE: §2 redrawn: the span grows with difficulty, top ends at
   difficulty 2 are −40 % Isp, −80 % thrust, +150 % mass, ×5 build time,
   +200 % materials; hard-to-test unchanged.

2. **Electric propulsion as a technology** (§4.2) so power-draw
   deficiencies have somewhere to live: yes, with difficulty 1 and
   unlocked at start?
   USER:  Sounds good
   CLAUDE: In, as step 6.

3. **Two technologies on one design** (§4.3) via `technology_ids`: the
   data-structure change, or should a design carry only its cycle's
   technology when it has both (a methalox staged-combustion engine
   would then dodge methane's deficiencies)?
   USER:  Definetly both
   CLAUDE: `technology_ids`, step 4.

4. Full-flow under the staged-combustion technology for now (§4.1), or
   its own locked technology at difficulty 2 straight away?
   User:  It's own difficulty 2 I think.
   CLAUDE: §4.1 now has `TECH_FULL_FLOW`, locked, difficulty 2, with the
   cycle gated on its unlock; step 5 covers both.

---

## 8. Record

**Step 1 — magnitudes.** `TechnologyConfig::magnitude_range(d)` =
[base + d·step, base + span + d·span_step] with base 0.10, step 0.05,
span 0.40, `deficiency_magnitude_span_per_difficulty` 0.25 (new): d0
0.10–0.50, d1 0.15–0.75, d2 0.20–1.00. Per-kind scales (new config,
serde defaults): `isp_scale` 0.4, `thrust_cap` 0.8, `mass_scale` 1.5,
`power_scale` 0.7; build time keeps ×(1 + 4m). The generation tests
re-pinned; `magnitudes_land_in_their_kinds_bands` walks 400 worlds and
checks each kind's band and that the worst of them reaches the brutal
end (−35 % Isp, −75 % thrust, +130 % mass, ×4.5 build time); the
`deficiencies` probe prints a few worlds. 723 tests, clippy clean. The
bot is on no technology, so the curve and bands are untouched.

