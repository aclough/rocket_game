# Resource System Design

## Resources (8 types)

All quantities are by weight (kg). Prices represent what a rocket company purchases — raw stock for metals, finished components for electronics/plumbing.

| Resource | $/kg | Category | Space Availability |
|----------|------|----------|-------------------|
| Aluminium | $5 | Raw metal (7075-T6 plate/bar) | Lunar regolith (early) |
| Steel | $3 | Raw metal (stainless, D6AC) | Asteroid iron (mid) |
| Superalloys | $80 | Raw metal (Inconel 718 forgings) | Asteroid Ni/Co (late) |
| Composites | $50 | Semifinished (carbon fiber prepreg, ablatives) | Difficult off-Earth |
| Wiring | $150 | Simple manufactured (wire, connectors, harnesses, simple sensors) | Local metalwork (mid) |
| Electronics | $20,000 | Complex manufactured (flight computers, IMUs, guidance, microchips) | Earth only (longest) |
| Plumbing | $1,500 | Manufactured (propellant valves, regulators, seals, bearings) | Advanced facilities (late) |
| Solid Propellant | $15 | Raw material (HTPB/AP composite) | Earth only (perchlorate) |

### Price Sources
- Inconel 718: $40-120/kg market (used $80 mid-range)
- Aerospace aluminium 7075-T6: $3-8/kg
- Stainless steel: $2-4/kg
- Carbon fiber prepreg: $30-60/kg
- Solid propellant (APCP): $5-35/kg (used $15 mid-range)
- Electronics/turbopumps: estimated from NASA cost studies showing turbopumps are ~55% of engine cost
- Raw materials are only 5-10% of liquid engine cost (NASA/industry studies)

### Note on Turbomachinery
Turbopumps are NOT a resource. SpaceX, Rocket Lab, Blue Origin, and all engine manufacturers build turbopumps in-house from raw superalloy forgings and steel billets. The turbopump is core engine IP. The high cost of turbopumps is labor (precision machining, balancing, testing), not materials. An 80 kg turbopump contains ~$5,000 of raw metal but costs $500K+ in labor.

---

## Engine Bills of Materials

All quantities as fraction of engine dry mass. Total resource kg = fraction × base_mass × scale.

### Kerolox Engine (base mass 450 kg)
Based on Merlin 1D architecture.

| Resource | Fraction | kg @s1.0 | Cost @s1.0 | What it is |
|----------|----------|----------|------------|------------|
| Steel | 0.31 | 140 | $420 | Manifolds, flanges, gimbal mount, turbopump housing, fasteners |
| Superalloys | 0.27 | 121 | $9,720 | Combustion chamber, nozzle throat, injector, turbine blades |
| Aluminium | 0.18 | 81 | $405 | Nozzle extension, pump housings, structural brackets |
| Plumbing | 0.08 | 36 | $54,000 | Propellant valves, regulators, seals, bearings |
| Wiring | 0.04 | 18 | $2,700 | Harnesses, connectors, sensor leads |
| Composites | 0.02 | 9 | $450 | Thermal blankets, insulation wraps |
| Electronics | 0.01 | 5 | $90,000 | Engine controller, valve drivers |
| **Total** | **1.00** | **450** | **~$158,000** | |

Note: the old "Turbomachinery" fraction (0.18) is now Aluminium — the turbopump housing, impeller casings, and structural elements that teams machine from aluminium stock. The precision machining labor is in build time.

### Hydrolox Engine (base mass 300 kg)
Based on RL-10 architecture. Higher superalloy fraction (H2 embrittlement), more plumbing (cryogenic).

| Resource | Fraction | kg @s1.0 | Cost @s1.0 | What it is |
|----------|----------|----------|------------|------------|
| Superalloys | 0.30 | 90 | $7,200 | H2-resistant chamber, nozzle, cryo-rated alloys |
| Aluminium | 0.25 | 75 | $375 | Pump housings, structural, impeller casings |
| Steel | 0.18 | 54 | $162 | Structure, nozzle body, manifolds |
| Plumbing | 0.12 | 36 | $54,000 | Cryogenic valves, boil-off management, insulated lines |
| Composites | 0.06 | 18 | $900 | Carbon-carbon nozzle extension, cryo insulation |
| Wiring | 0.04 | 12 | $1,800 | Cryo sensor leads, harnesses |
| Electronics | 0.01 | 3 | $60,000 | Engine controller, cryo management |
| **Total** | **1.00** | **300** | **~$124,000** | |

### Solid Motor (base mass 40,000 kg casing)
Based on Shuttle SRB architecture. D6AC steel casing, carbon-cloth ablative nozzle.
Propellant grain listed separately (cast into casing during manufacturing).

| Resource | Fraction | kg @s1.0 | Cost @s1.0 | What it is |
|----------|----------|----------|------------|------------|
| Steel | 0.76 | 30,400 | $91,200 | D6AC casing segments, nozzle structure, TVC actuators |
| Composites | 0.15 | 6,000 | $300,000 | Ablative nozzle liner, internal insulation, covers |
| Aluminium | 0.04 | 1,600 | $8,000 | Adapter rings, mounting hardware, brackets |
| Superalloys | 0.025 | 1,000 | $80,000 | Nozzle throat insert, igniter body |
| Wiring | 0.0125 | 500 | $75,000 | Full-length harnesses, igniter leads, TVC wiring |
| Plumbing | 0.0075 | 300 | $450,000 | TVC hydraulic lines, pressurization fittings |
| Electronics | 0.005 | 200 | $4,000,000 | TVC controller, ignition sequencer, instrumentation |
| **Total casing** | **1.00** | **40,000** | **~$5,004,000** | |

Plus propellant grain (from mass_ratio 0.88):

| Resource | kg @s1.0 | Cost @s1.0 |
|----------|----------|------------|
| Solid Propellant | 293,333 | $4,400,000 |

**Solid motor total: ~$9.4M** (casing materials + propellant grain)

---

## Tank Bills of Materials

Fractions of tank mass. Tank mass = propellant_mass × tank_mass_ratio.
Tanks are structural only — avionics are in Stage Assembly.
Solid motors have no separate tanks.

### Kerolox Tank (tank_mass_ratio = 0.06)

| Resource | Fraction | Notes |
|----------|----------|-------|
| Aluminium | 0.88 | Al-7075 barrel sections, dome caps |
| Steel | 0.06 | Flanges, ring frames, aft skirt fittings |
| Plumbing | 0.03 | Fill/drain valves, pressurization fittings |
| Wiring | 0.02 | Level sensors, pressure sensor cabling |
| Composites | 0.01 | Cork/foam TPS if exposed |

Example: 100,000 kg propellant → 6,000 kg tank → ~$262K materials

### Hydrolox Tank (tank_mass_ratio = 0.10)

| Resource | Fraction | Notes |
|----------|----------|-------|
| Aluminium | 0.74 | Al-Li 2195 barrel, common bulkhead |
| Composites | 0.14 | Spray-on foam insulation, MLI blankets |
| Steel | 0.06 | Ring frames, flanges |
| Plumbing | 0.035 | Cryo fill/drain, boil-off vent, pressurization |
| Wiring | 0.02 | Cryo temp sensors, pressure cabling |
| Superalloys | 0.005 | Cryo-rated joint inserts |

Example: 20,000 kg propellant → 2,000 kg tank → ~$134K materials

---

## Stage Assembly (fixed per stage, ~300 kg)

Interstage adapter, separation system, avionics bay, pressurization, stage harness.

| Resource | kg | Cost |
|----------|-----|------|
| Aluminium | 100 | $500 |
| Wiring | 70 | $10,500 |
| Steel | 45 | $135 |
| Plumbing | 35 | $52,500 |
| Composites | 25 | $1,250 |
| Electronics | 25 | $500,000 |
| **Total** | **300** | **~$565,000** |

## Rocket Integration (fixed per vehicle, ~700 kg)

Fairing, nose cone, flight computer, guidance/navigation, full-vehicle harness.

| Resource | kg | Cost |
|----------|-----|------|
| Aluminium | 300 | $1,500 |
| Composites | 200 | $10,000 |
| Wiring | 80 | $12,000 |
| Steel | 40 | $120 |
| Electronics | 50 | $1,000,000 |
| Plumbing | 30 | $45,000 |
| **Total** | **700** | **~$1,069,000** |

---

## Labor Model

### Team Composition

**Engineering Team** (~8-10 people, $150K/month):
- 6-8 aerospace/propulsion/structural/systems engineers @ ~$200K/yr fully loaded
- 1-2 support/admin staff @ ~$120K/yr fully loaded
- Total: ~$1.8M/year = $150K/month
- Hiring cost: $150K (1× monthly salary)

**Manufacturing Team** (~20-25 people, $300K/month):
- 18-20 manufacturing technicians/machinists/welders @ ~$90K/yr fully loaded
- 2-3 manufacturing engineers @ ~$180K/yr fully loaded
- 1 QA inspector @ ~$100K/yr fully loaded
- Equipment/facilities overhead baked in
- Total: ~$3.6M/year = $300K/month
- Hiring cost: $900K (3× monthly salary)

### Engine Build Times

Build work in team-days. Wall-clock = build_days / manufacturing_team_efficiency(n_teams).
Manufacturing team efficiency: n^0.85

Formula: base_build_days × scale^0.75

| Engine Type | Base Build Days | Scale 0.5 | Scale 1.0 | Scale 2.0 | Scale 4.0 |
|-------------|----------------|-----------|-----------|-----------|-----------|
| Kerolox | 120 | 71 days | 120 days | 202 days | 339 days |
| Hydrolox | 180 | 107 days | 180 days | 302 days | 508 days |
| Solid | 45 | 27 days | 45 days | 76 days | 127 days |

### Total Engine Costs (scale 1.0, varying team count)

**Kerolox ($300K/mo mfg salary, 120 team-days, ~$158K materials):**

| Teams | Wall-clock | Labor Cost | Materials | Total |
|-------|-----------|------------|----------|-------|
| 1 | 120 days (5.5 mo) | $1.64M | $158K | $1.79M |
| 2 | 67 days (3.0 mo) | $1.82M | $158K | $1.97M |
| 3 | 47 days (2.1 mo) | $1.93M | $158K | $2.09M |
| 5 | 31 days (1.4 mo) | $2.09M | $158K | $2.25M |

**Hydrolox ($300K/mo, 180 team-days, ~$124K materials):**

| Teams | Wall-clock | Total |
|-------|-----------|-------|
| 1 | 180 days (8.2 mo) | $2.59M |
| 2 | 100 days (4.5 mo) | $2.85M |
| 3 | 70 days (3.2 mo) | $2.98M |

**Solid ($300K/mo, 45 team-days, ~$9.4M materials):**

| Teams | Wall-clock | Labor | Materials | Total |
|-------|-----------|-------|----------|-------|
| 1 | 45 days (2.0 mo) | $614K | $9.4M | $10.0M |
| 2 | 25 days (1.1 mo) | $682K | $9.4M | $10.1M |

Solid motors are materials-dominated (94% materials, 6% labor).

### Reference: Real-World Production Data

**SpaceX Merlin production evolution:**
- 2005-2008: Development, 160→500 employees total
- 2010: 18 weeks (4.5 months) per engine, ~$1-2M each
- 2011: 8 engines/month
- 2014: 4-5 per week
- 2020s: 1 every 18 hours (mature, highly parallel, ~120 technicians on Merlin line)

**Traditional aerospace:**
- RL-10 chamber: 20 months fabrication (pre-additive manufacturing)
- RS-25: 3-4 years per engine, $40-100M each
- RL-10: 16-18 engines/year, ~$30M each

**Labor costs (US aerospace, fully loaded with 1.5-2× overhead):**
- Manufacturing technician: $50K salary → $75-100K/yr fully loaded
- Manufacturing engineer: $110K salary → $165-220K/yr fully loaded
- SpaceX technicians: ~$41K (famously underpaid)
- SpaceX manufacturing engineers (Hawthorne): ~$111K base, ~$138K total comp

---

## Complete Default Rocket Cost Estimate

2-stage kerolox/hydrolox, 5+1 engines, ~100t prop stage 1, ~20t prop stage 2.
All engines at scale 1.0, 1 manufacturing team, sequential production.

| Component | Materials | Labor (1 team) | Total | Wall-clock |
|-----------|----------|----------------|-------|------------|
| 5× Kerolox engine | $790K | $8.18M | $8.97M | 600 days |
| 1× Hydrolox engine | $124K | $2.45M | $2.58M | 180 days |
| Stage 1 tank (6,000 kg) | $262K | (in rocket assembly) | $262K | — |
| Stage 2 tank (2,000 kg) | $134K | (in rocket assembly) | $134K | — |
| 2× Stage assembly | $1.13M | (in rocket assembly) | $1.13M | — |
| Rocket integration | $1.07M | (in rocket assembly) | $1.07M | — |
| Rocket assembly work | — | ~$1.4M | ~$1.4M | ~100 days |
| **Vehicle total** | **$3.5M** | **$12.0M** | **~$15.5M** | |

With 3 manufacturing teams working in parallel on different components:
wall-clock for all 5 kerolox engines drops from 600 to ~200 days.

---

## Future Resource Compatibility

When in-space manufacturing is added:
- Aluminium ($5/kg) worth sourcing locally once launch cost > $5/kg to orbit
- Steel ($3/kg) similarly available from asteroid iron
- Superalloys ($80/kg) justify shipping from Earth until asteroid processing exists
- Electronics ($20,000/kg) and Plumbing ($1,500/kg) justify shipping from Earth longest
- Solid Propellant requires perchlorate chemistry — Earth only
- Weight-based quantities enable cost calculation for shipping resources to orbit
