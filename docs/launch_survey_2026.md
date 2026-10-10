# Survey: orbital launch vehicles from new entrants (data as of Oct 2026)

Compiled 2026-10-10 for `22_FLAWS.md` from Wikipedia vehicle pages and
launch lists, SpaceNews, NASASpaceflight, Seradata, China-in-Space,
Space.com, europeanspaceflight.com and spacepolicyonline. Outcome codes:
**S** success, **P** partial (orbit reached, objective compromised),
**F** failure. "Western" means non-Chinese start-ups (includes Japan's
Space One and India's Skyroot). Antares is listed with start-ups but
counted with established players.

## 1. Vehicles

| Vehicle (company) | 1st orbital attempt | Flights 1–5 | Total flights / failures (partials) |
|---|---|---|---|
| Falcon 1 (SpaceX) | Mar 2006, F | F F F S S | 5 / 3 |
| Falcon 9 (SpaceX) | Jun 2010, S | S S S P S | 706 / 2 (1) + Amos-6 pre-flight loss |
| Electron (Rocket Lab) | May 2017, F | F S S S S | 89 / 4 |
| Rocket 3 (Astra) | Sep 2020, F | F F F S F (then S F) | 7 / 5 + Rocket 3.0 pad loss |
| Alpha (Firefly) | Sep 2021, F | F P S P S (then F S) | 7 / 2 (2) |
| Terran 1 (Relativity) | Mar 2023, F | F | 1 / 1 (retired) |
| RS1 (ABL) | Jan 2023, F | F | 1 / 1 + 2nd vehicle lost in static fire |
| LauncherOne (Virgin Orbit) | May 2020, F | F S S S S (then F) | 6 / 2 |
| Spectrum (Isar) | Mar 2025, F | F S | 2 / 1 |
| RFA One (RFA) | unflown | — | stage lost in Aug 2024 static fire |
| Eris (Gilmour) | Jul 2025, F | F | 1 / 1 |
| Kairos (Space One) | Mar 2024, F | F F F | 3 / 3 |
| New Glenn (Blue Origin) | Jan 2025, S | S S F | 3 / 1 + NG-4 lost in static fire |
| Vikram-1 (Skyroot) | Jul 2026, S | S | 1 / 0 |
| Antares (Orbital) | Apr 2013, S | S S S S F | 18 / 1 |
| Zhuque-1 (LandSpace) | Oct 2018, F | F | 1 / 1 |
| Zhuque-2/2E (LandSpace) | Dec 2022, F | F S S S S (then F S S S) | 9 / 2 |
| Zhuque-3 (LandSpace) | Dec 2025, S | S S | 2 / 0 |
| Hyperbola-1 (iSpace) | Jul 2019, S | S F F F S (then S F S) | 8 / 4 |
| Ceres-1/1S (Galactic Energy) | Nov 2020, S | S S S S S | 23 / 2 (first failure flight 10) |
| Ceres-2 (Galactic Energy) | Jan 2026, F | F | 1 / 1 |
| Pallas-1 (Galactic Energy) | Aug 2026, S | S | 1 / 0 |
| Tianlong-2 (Space Pioneer) | Apr 2023, S | S | 1 / 0 |
| Tianlong-3 (Space Pioneer) | Apr 2026, F | F | 1 / 1 + Jun 2024 static-fire breakaway |
| Kinetica-1 (CAS Space) | Jul 2022, S | S S S S S (F at 6) | 16 / 1 |
| Kinetica-2 (CAS Space) | Mar 2026, S | S | 1 / 0 |
| Gravity-1 (Orienspace) | Jan 2024, S | S S S S | 4 / 0 |
| OS-M1 (OneSpace) | Mar 2019, F | F | 1 / 1 |
| Kuaizhou-1A (ExPace) | Jan 2017, S | S S S S S (first F at 11) | 31 / 3 |
| Kuaizhou-11 (ExPace) | Jul 2020, F | F S S S S | 7 / 1 |
| Vulcan (ULA) | Jan 2024, S | S S S S | 4 / 0 (SRB anomalies on 2 and 4) |
| Ariane 6 | Jul 2024, P | P S S S S | 9 / 0 (1) |
| H3 (JAXA/MHI) | Mar 2023, F | F S S S S | 9 / 2 |
| Nuri (KARI) | Oct 2021, F | F S S S S | 5 / 1 |
| SSLV (ISRO) | Aug 2022, F | F S S | 3 / 1 |
| Angara A5 | Dec 2014, S | S S P S S | 5 / 0 (1) |
| LVM3 (ISRO) | Jun 2017, S | S S S S S | 8 / 0 |

## 2. Failure causes

ENG engine internal · FEED feed/plumbing/pressurisation · STG staging/separation · US-IGN upper-stage ignition/relight · GNC guidance/software · ELEC electrical/harness/avionics · STRUCT structures/tanks · GRND ground/range/FTS. 3P = third-party part failed; MOD = followed a recent change.

| Flight (ordinal) | Cause | Cat. | 3P | MOD |
|---|---|---|---|---|
| Falcon 1 F1 (1) | Corroded fuel-line nut, leak and fire, engine shutdown T+34 s | FEED | | |
| Falcon 1 F2 (2) | Interstage hit stage-2 nozzle; slosh/TVC coupling | STG | | |
| Falcon 1 F3 (3) | Residual thrust of new regen Merlin 1C recontacted stage 2 | STG | | **yes** |
| Falcon 9 CRS-1 (4) P | Merlin chamber breach; secondary payload stranded | ENG | | |
| Falcon 9 CRS-7 (19) | Stage-2 He COPV strut failed far below rating | STRUCT | **yes** | |
| Falcon 9 Amos-6 (pre-flight) | COPV liner buckling, SOX friction ignition during fast subcooled load | FEED | ? | **yes** |
| Falcon 9 Starlink 9-3 (~354) | Cracked LOX sense line, igniter fluid over-chilled, hard relight | FEED | | |
| Electron F1 (1) | Contractor ground telemetry corrupted data; range destroyed healthy rocket | GRND | **yes** | |
| Electron F13 (13) | One bad electrical connection, pump power lost | ELEC | | |
| Electron F20 (20) | Stage-2 igniter fault corrupted controller data | ENG | | |
| Electron F41 (41) | Arc in stage-2 motor-controller power system (Paschen) | ELEC | | |
| Astra 3.0 (pre-flight) | Plastic He valve stuck when chilled, tank overpressure | FEED | ? | |
| Astra 3.1 (1) | Roll oscillation T+15 s | GNC | | |
| Astra 3.2 (2) | Upper-stage mixture-ratio error, 0.5 km/s short | FEED | | |
| Astra LV0006 (3) | Feed-system leak ignited at liftoff, engine lost | FEED | | stretched 3.3 debut |
| Astra LV0008 (5) | Wiring error fired fairing separation out of order | ELEC | | |
| Astra LV0010 (7) | Fuel boiled in regen channels (warm propellant) | ENG | | operational |
| Alpha F1 (1) | Fuel-valve connector sheared, engine 2 out, loss of control | ELEC | ? | |
| Alpha F2 (2) P | Stage 2 under-delivered; cause not published | ? | | |
| Alpha F4 (4) P | Relight missed orbit; GNC software error | GNC | | |
| Alpha F6 (6) | Separation mishap damaged stage-2 nozzle | STG | | |
| Terran 1 (1) | Stage-2 engine failed to ignite: slow valve, vapour in LOX pump | US-IGN | | |
| RS1 (1) | Pad plume recirculation burned harnesses, all engines off T+11 s | GRND→ELEC | | |
| LauncherOne F1 (1) | High-pressure LOX line broke | FEED | ? | |
| LauncherOne F6 (6) | Dislodged fuel filter starved stage-2 pump | FEED | ? | |
| Spectrum F1 (1) | Vent valve opened, attitude lost T+18 s | FEED | ? | |
| Eris (1) | Hybrid motors under-thrust T+14 s | ENG | | |
| Kairos F1 (1) | Autonomous FTS triggered by over-optimistic burn model | GRND | partner motors | |
| Kairos F2 (2) | Attitude lost T+95 s; nozzle/TVC or sensor | ENG/GNC? | | |
| Kairos F3 (3) | FTS engaged T+70 s; no vehicle anomaly found | GRND? | | |
| New Glenn NG-3 (3) | Cryo leak froze hydraulic line, thrust short on 2nd burn | FEED | | |
| Zhuque-1 (1) | Broken pipe in stage-3 ACS thruster | FEED | | |
| Zhuque-2 F1 (1) | Shutdown transient damaged vernier LOX feed | FEED | | |
| Zhuque-2E F6 (6) | Voltage anomaly in stage 2 | ELEC | | variant 3rd flight |
| Hyperbola-1 F2 (2) | Insulation foam jammed grid fin, control over-compensated | STRUCT/GNC | | |
| Hyperbola-1 F3 (3) | Fairing failed to separate | STG | | |
| Hyperbola-1 F4 (4) | Not published | ? | | |
| Hyperbola-1 F7 (7) | Believed abnormal stage 3/4 separation | STG? | | |
| Ceres-1 (10) | Nozzle ablation from carbon-cloth defect at a screw hole | ENG (unit defect) | ? | |
| Ceres-1 (22) | Stage-4 engine shut down early; not published | ENG | | |
| Ceres-2 (1) | Insufficient margin in nozzle throat-liner support | ENG | | |
| Tianlong-3 (1) | Propellant leak; engine over-expanded, nozzle damaged | FEED | | |
| Kinetica-1 (6) | Attitude instability after stage-3 ignition; not published | ? | | |
| OS-M1 (1) | Rate gyro failed after stage-1 separation | ELEC | | |
| Kuaizhou-1A (11, ~15) | Not published | ? | | |
| Kuaizhou-11 (1) | Stage-3 ignition failure (secondary sources) | US-IGN? | | |
| Antares Orb-3 (5) | Refurbished NK-33/AJ26 LOX turbopump failed T+6 s | ENG | **yes** | |
| Ariane 6 (1) P | APU temperature limit, Vinci deorbit relight skipped | GNC | | |
| H3 TF1 (1) | Overcurrent in stage-2 ignition circuit, LE-5B did not ignite | ELEC | | |
| H3 F8 (8) | Payload support structure failed, damaged stage-2 LH2 tank | STRUCT | | |
| Nuri (1) | Stage-3 He tank broke loose (buoyancy), damaged oxidiser tank | STRUCT | | |
| SSLV D1 (1) | Accelerometer anomaly, open-loop logic fired VTM 0.1 s | GNC | | |
| Angara A5 (3) P | New Persei upper stage failed to relight | US-IGN | **yes** | **yes** |
| Vulcan (2, 4) | GEM-63XL SRB nozzle loss (insulator defect); core compensated | ENG | **yes** | |

## 3. Aggregates

First-attempt success:

| Group | Vehicles | First-flight successes | Rate |
|---|---|---|---|
| Non-Chinese start-ups | 13 | 3 (Falcon 9, New Glenn, Vikram-1) | 23 % |
| …each company's first vehicle only | 12 | 2 | 17 % |
| Chinese start-ups | 15 | 9 | 60 % |
| …each company's first vehicle only | 8 | 6 | 75 % |
| Established players, new vehicles | 9 | 5 S + 1 P | 56 % (67 % with partial) |

Chinese first-flight success is dominated by small solid vehicles on
state solid-motor heritage; Chinese liquid debuts split 3 S / 3 F.

Success by flight ordinal, pooled over 28 start-up vehicles:

| Ordinal | Flights | S | P | F | Full-success rate |
|---|---|---|---|---|---|
| 1st | 28 | 12 | 0 | 16 | 43 % |
| 2nd | 17 | 12 | 1 | 4 | 71 % |
| 3rd | 15 | 10 | 0 | 5 | 67 % |
| 4th–10th | 64 | 53 | 2 | 9 | 83 % |
| 11th+ | 815 | 806 | 0 | 9 | 98.9 % (94 % excluding Falcon 9) |

Failure causes, start-ups (45 in-flight failures and partials):

| Category | Flights 1–3 (26) | Flights 4+ (19) |
|---|---|---|
| Engine internal | 3 | 5 |
| Feed / plumbing / pressurisation | 9 | 2 |
| Staging / separation | 3 | 2 |
| Upper-stage ignition / relight | 2 | 0 |
| GNC / software | 1 | 1 |
| Electrical / harness / avionics | 2 | 4 |
| Structures / tanks | 1 | 1 |
| Ground / range / FTS | 4 | 0 |
| Undisclosed | 1 | 4 |

Early flights are dominated by plumbing, valves and leaks (35 %) and
ground/range mistakes (15 %); later flights shift to electrical (21 %)
and engine-internal wear or manufacturing defects (26 %). Established
players' first-flight failures skew to electrical, structures and
software and never to plumbing.

Third-party parts: 2 start-up in-flight failures (CRS-7 strut, Electron
F1 ground gear) and 2 established (Antares NK-33, Angara Persei);
Kairos's partner-built motors uncertain. Failures clearly following a
modification: Falcon 1 F3, Astra LV0006, Angara Persei, Amos-6 — about
3–4 of ~50 (6–8 %). Most failures hit unchanged hardware.

## 4. Uncertainty

Chinese root causes are often unpublished; starred rows rest on
secondary trackers. Kairos F2/F3 and Eris causes are leading theories.
Firefly Alpha F6 mechanism from memory. Falcon 9 (706) and Electron
(89) totals are Wikipedia lead figures as of early Oct 2026.
