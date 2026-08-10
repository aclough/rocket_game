# What the seed decides, and by how much

Measured 2026-08-09 against `a09f361`+2. Market/DinoSoar/economy/tech figures
are from 200 seeds; outcome figures from `simulate --seeds 1..40 --policy basic`
(5 years, identical policy, so every difference below is the seed).

## Two channels

`GameSeed` (`src/seed.rs`) hands out randomness two ways, and they have very
different properties.

**`world_query(question)`** hashes the seed together with a question string to
build a per-question RNG. The same question always gives the same answer no
matter what order questions are asked in, or how many times. This is the
save-scum-proof channel and it carries every fact about *what world you are in*.

**`contingent_rng`** is one linear stream seeded from `seed + 1`. It carries
in-the-moment rolls: flaw activation, launch failure, discovery. Order matters,
so it is not reproducible across different play.

## World facts (`world_query`)

### Markets — the biggest single lever

Eight archetypes, realized once at game start (`contract::realize_archetype`).
Presence across 200 seeds:

| archetype | present | opens |
|---|---|---|
| `market_geo_comsats` | 100% | start |
| `market_gov_science` | 100% | start |
| `market_rideshare` | 100% | start |
| `market_cots` | 74% | 2004–2008 |
| `market_earth_obs` | 70% | 2005–2012 |
| `market_leo_constellation` | 50% | 2008–2015 |
| `market_nssl` | 44% | 2010–2018 |
| `market_meo_constellation` | 18% | 2008–2015 |

Between **3 and 7 of the 8** exist in a given world.

Two of those percentages are lower than the configured probability because of
the `constellation` exclusive group: LEO is configured at 60% and MEO at 30%,
but only one can win, so MEO lands at 18% observed. NSSL's 44% against a
configured 50% is just sampling noise at n=200.

Per-market perturbation, on top of presence:

- **volume** ×0.7–1.5 depending on archetype
- **rate per kg** ×0.85–1.2
- **annual growth** −2%/yr (GEO) to +12%/yr (LEO constellations), compounding
  from activation
- **destination weight tilt** ±0–20%, so the mix of where contracts go shifts

**The opening is nearly fixed.** All three start-active markets are always
present, and two of them (`geo_comsats`, `rideshare`) are pinned to ×1.0 volume
and ×1.0 rate — only their growth trajectory varies. So on day one the only
market differences are `gov_science`'s size (±30% volume, ±15% rate) and the
growth rates. Everything that really separates two worlds arrives from 2004
onward, when the event markets do or don't open.

That shows up sharply in the outcome data: **first launch happens on 2002-05-26
in 38 of 40 seeds**, and never more than two days either side.

### DinoSoar's reliability

`competitor.rs::realize_dinosoar`: `failure_base + failure_spread * u^skew`,
with base 0.003, spread 0.047, skew 8.0.

| | per-launch failure rate |
|---|---|
| min | 0.3% |
| median | 0.3% |
| p90 | 2.3% |
| max | 4.9% |

A 16× spread end to end, but the `u^8` skew means most worlds get a rival that
essentially never fails. A visibly unreliable DinoSoar is the rare world.

### The economy

`economy.rs`. A fixed ladder of conditions with seeded durations and modifiers;
condition *n* is asked as `economy_event_n`, so the whole 15-year trajectory is
fixed at game start.

- Recession multiplies contract volume by **0.5–0.7**; Boom by **1.3–1.5**.
- Over 15 years, time spent in recession is **0 to 48 months**, averaging 14.
- The first transition is special-cased: a **50%** chance of a dot-com
  recession, regardless of the normal transition table.

A world that spends four years in recession and one that never sees a downturn
are both reachable from the seed alone.

### Technology

`technology.rs::generate_technology`. Each of the three seeded technologies gets
2–4 deficiencies, each with a `solvability` and a magnitude.

Solvability is rolled as `-(difficulty × 0.1) + u × (1 - difficulty × 0.1)` and
clamped at zero, so a difficulty-2 tech has a **20% chance per deficiency of
being permanently unsolvable** — no amount of engineering will clear it.
Sampled across three seeds: 0, 2 and 5 permanently unsolvable deficiencies.

That is the widest *qualitative* swing in the game: some worlds simply cannot
have a good nuclear engine.

### Smaller world facts

- **Third-party engine flaws** (`3p_flaws_<name>`) — the catalog's specs are
  fixed (`generate_starter_engines` ignores the seed); only the flaws vary.
- **DinoSoar's bid jitter** (`dino_bid_<id>`, `dino_block_bid_<id>`) — ±5% on
  each bid, drawn per contract so it can't be re-rolled.
- **DinoSoar's launch outcomes** (`dino_launch_<contract id>`).
- **Contract generation** (`contracts_<year>_<month>_<market>`) and **campaign
  announcements** (`campaigns_<year>_<month>`) — per month, so the contract
  stream is fixed for the whole game.
- **Campaign missions** (`campaign_issue_<id>_<n>`).
- **Tech unlock rolls** (`tech_unlock_<id>_<year>`) — 10%/yr for difficulty 1,
  8%/yr for difficulty 2.

## In-the-moment rolls (`contingent_rng`)

Flaw activation in flight, launch failure, engine loss, per-day endurance rolls,
and the flaw a design modification introduces.

**Loading a save rewinds this stream to its first draw.** `fix_after_load`
re-seeds from `seed + 1` rather than restoring the position, so a game reloaded
at year 5 resumes drawing from where a brand-new game would start. Verified
directly: burn five draws, serialize, reload, and the next draw equals the very
first draw of a fresh `GameSeed`.

The practical effect: reloading and repeating identical actions replays
identical outcomes, but reloading and doing *anything* that consumes a different
number of draws re-rolls everything downstream. Save-scumming a failed launch
works if you change what you do first. Whether that matters is a design call —
noting it because the doc comment describes the channel as "non-deterministic by
design", which is not quite what it is.

## What it all adds up to

40 seeds, five years, identical `basic` policy:

| metric | min | median | max | spread |
|---|---|---|---|---|
| final money | **−$1.8M** (bankrupt) | $144.7M | **$243.8M** | — |
| launches | 3 | 7 | 10 | 3.3× |
| launch success | 78% | 100% | 100% | — |
| avg payment | $31.4M | $36.2M | $38.6M | 1.2× |
| unit cost | $18.3M | $17.2M | $16.4M | 1.1× |
| hidden flaws at first launch | 4 | 9 | 13 | 3.2× |

1 of 40 went bankrupt (seed 35, nine launches at 78% success). 2 of 40 never
turned a profit. First profitable year ranges 2002–2005, with 22 of 40 in 2003.

**The shape of it:** unit economics barely move (payment 1.2×, unit cost 1.1×),
but *volume* and *luck* move a lot — launches vary 3.3× and success rate is what
separates a bankruptcy from a $244M finish on the same policy. The seed is not
tuning how good your rockets are; it is deciding how much work exists and
whether your vehicles survive it.

**Where the variance comes from, roughly in order:** which event markets open
(and when), the economy trajectory, launch-failure luck via the flaw rolls,
and — for the long game — whether the technologies you want are solvable at all.
