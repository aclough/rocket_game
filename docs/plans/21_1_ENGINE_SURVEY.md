# 21_1_ENGINE_SURVEY — Real engines for calibration

Reference data for `21_ENGINES.md`, gathered 2026‑09‑26 by four web
surveys (US; Soviet and Russian; European; Japan with China and India).
Figures come from manufacturer pages, NASA/NTRS, NASA OIG and GAO
reports, ESA/CNES/JAXA/ISRO pages and Wikipedia with its citations;
the source column says which. Astronautix was unreachable for most of
the work, and Energomash's site refused connections, so some Soviet
specs come from archived pages and Wikipedia.

Conventions: "~" approximate or derived; "?" uncertain or sources
disagree; blank = not found. TWR = thrust ÷ (dry mass × g0), SL where
the engine has a sea‑level rating, else vac. Pc in bar. Costs carry
their currency year and how solid they are.

## United States

### Kerolox

| engine | maker | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | TWR | Pc | ε | unit cost | notes · source |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Merlin 1A | SpaceX | 2006 | GG | 340 / – | | | | | | | ablative chamber; 2 flown · WP Merlin |
| Merlin 1C | SpaceX | 2008 | GG | 420 / 480 | 275 / 305 | 630 | 68 SL | 68 | | | regen chamber; WP's stated TWR 96 contradicts its own mass · WP |
| Merlin 1C Vac | SpaceX | 2010 | GG | – / 411 | – / 342 | | | | | | niobium extension · WP |
| Merlin 1D v1.1 | SpaceX | 2013 | GG | ~653 / ~741 | 282 / 311 | 470? | ~142 SL | 97 | 16 | | built for production; 4 per week (2014) · WP |
| Merlin 1D FT | SpaceX | 2015 | GG | ~730 / 914 | 282 / 311 | 470 | ~158 SL | ~108? | 16 | | uprate: colder denser propellant, higher power · WP |
| Merlin 1D Block 5 | SpaceX | 2018 | GG | 845 / 914 | 282 / 311 | 470 | 183 SL | ~108? | 16 | $0.4–2M (unofficial) | reusable; 1,000+ built · WP |
| MVac‑D | SpaceX | 2013–18 | GG | – / 716 → 981 | – / 340 → 348 | | | | 165 | | · WP |
| Kestrel | SpaceX | 2006 | pressure‑fed | – / ~31 | – / 317 | 52 | ~60 vac | 9.3 | | | · WP |
| Rutherford | Rocket Lab | 2017 | electric pump | 24.9 / – | 311 / – | 35 (no battery) | 73 SL | ~55? | | | 3D‑printed chamber; ~1 MW battery on stage · WP |
| H‑1 200K/205K | Rocketdyne | 1961→ | GG | 890 / 912 | – / 289 | ~1,000 | ~93 SL | 44 | 8 | | 165→188→200→205 klbf lineage · WP |
| F‑1 | Rocketdyne | 1967 | GG | 6,770 / 7,770 | 263 / 304 | 8,400 | 82 SL | 70 | 16 | ≤ $4.4M (1966 $; contract target $133.4M for 30 + support) | · WP, GAO B‑167838 |
| F‑1A | Rocketdyne | tested | GG | 8,007 / 8,989 | 270 / 303 | 9,015 | 91 SL | ~80 | | | ~20 % uprate · WP |
| RS‑27 | Rocketdyne | 1974 | GG | 971 / 1,023 | 264 / 295 | 1,027 | 96 SL | 49 | 8 | | H‑1 derivative · WP |
| RS‑27A | Rocketdyne | ~1990 | GG | 890 / 1,054 | 255 / 302 | 1,147 | 79 SL | 48 | 12 | | longer nozzle · WP |
| LR87‑3 (per chamber) | Aerojet | 1959 | GG | 648 / 734 | 256 / 290 | 839 | 79 SL | 40 | 8 | | Titan I · WP |
| AR1 | Aerojet Rocketdyne | never flew | ox‑rich SC | 2,224 / 2,500 | | | | | | target < $25M per pair (claim) | lost to BE‑4 · WP |

### Hydrolox

| engine | maker | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | TWR | Pc | ε | unit cost | notes · source |
|---|---|---|---|---|---|---|---|---|---|---|---|
| RL10A‑1 | P&W | 1962 | closed expander | – / 67 | – / 425 | 131 | 52 vac | | 40 | | · WP |
| RL10A‑3‑3A | P&W | ~1981 | closed expander | – / 73.4 | – / ~445 | 141 | 53 vac | 33 | | | Centaur workhorse · NASA CR‑190786 |
| RL10A‑4‑2 | P&W | 2002 | closed expander | – / 99 | – / 451 | 170 | 59 vac | 44 | 84 | | · WP |
| RL10B‑2 | P&W/AR | 1998 | closed expander | – / 110.1 | – / 465.5 | 301 | 37 vac | 44 | 280 | | deployable C‑C extension · WP, NASA |
| RL10C‑1‑1 | AR | 2021 | closed expander | – / 106 | – / 453.8 | 188 | 57 vac | | 155 | $17–20M (weak) | 500+ RL10s flown · WP |
| RL10C‑3 | AR/L3Harris | – | closed expander | – / ~108 | | | | | | ≤ $14.5M (2016 contract $174M: dev + 10 + 2 spares) | · ExecutiveBiz, NASA |
| RL10C‑X | L3Harris | 2025–26? | closed expander | – / 107.3 | – / 460.9 | 231 | 47 vac | | | undisclosed (116‑engine ULA order) | additive parts to cut cost · WP |
| J‑2 | Rocketdyne | 1966 | GG | 486 / 1,033 | 200 / 421 | 1,788 | 59 vac | 53 | 27.5 | | · WP |
| J‑2S | Rocketdyne | tested | tap‑off | – / 1,139 | – / 436 | 1,400 | 83 vac | | | | simplified J‑2 · WP |
| J‑2X | PWR/AR | 2011 | GG | – / 1,310 | – / 448 | 2,472 | 54 vac | ~95? | | ($1.4B development) | · WP, OIG IG‑23‑015 |
| RS‑25D (Block II) | Rocketdyne | 2001 | fuel‑rich SC | 1,860 / 2,279 | 366 / 452 | 3,514 | 54 SL | 206 | 69 | **$104.5M** manufacturing (2015 $, OIG) | reusable, 55 flights · NTRS, OIG |
| RS‑25E | AR/L3Harris | – | fuel‑rich SC | – / 2,320 | – / ~452 | | | | | target **$70.5M**; all‑in ~$145M; 2020 order ~$99M each | expendable; printed parts, fewer parts · OIG |
| RS‑68 | Rocketdyne | 2002 | GG | 2,950 / 3,370 | 365 / 410 | 6,600 | 46 SL | 103 | 21.5 | ~$20M (weak) | ablative, ~80 % fewer parts than SSME · WP |
| RS‑68A | PWR/AR | 2012 | GG | 3,137 / 3,560 | – / 412 | 6,740 | 47 SL | | | | · WP |
| BE‑3PM | Blue Origin | 2015 | tap‑off | 490 / – | | | | | | | throttles 18–100 % · WP |
| BE‑3U | Blue Origin | 2025 | open expander | – / 711 → 942 | – / 445 | | ~90? | | | | · WP |

### Methalox

| engine | maker | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | TWR | Pc | ε | unit cost | notes · source |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Raptor 1 | SpaceX | 2019 | full‑flow SC | 1,814 / ~1,960 | 327 / 350 | 2,080 | 89 SL | 250 | 34 | "well under $1M" (Musk 2019) | · WP |
| Raptor 2 | SpaceX | 2023 | full‑flow SC | 2,256 / – | ~347? | 1,630 | 141 SL | 300 | | ~half of R1 (SpaceX 2022); goal < $250k | > 1 per day (2022) · WP |
| Raptor 3 | SpaceX | 2026 | full‑flow SC | 2,746 / – | 350 / – | 1,525 | 184 SL | ~330 | | goal < $1,000/tf | plumbing in castings · SpaceX, WP |
| BE‑4 | Blue Origin | 2024 | ox‑rich SC | 2,460 → 2,847 / – | – / ~340? | ? | | ~134–140 | | ~$8M (rumour) | · WP |
| Archimedes | Rocket Lab | 2025 | ox‑rich SC | 730 / 890 | 329 / 365 | | | 100 | | | switched from GG · WP |
| Aeon R | Relativity | testing | GG | ~1,197 / – | | | | | | | heavily printed · NSF |

### Storable and solid

| engine | maker | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | TWR | Pc | ε | unit cost | notes · source |
|---|---|---|---|---|---|---|---|---|---|---|---|
| AJ10‑137 (Apollo SPS) | Aerojet | 1966 | pressure‑fed | – / 91 | – / 314 | ~295 | ~31 vac | 6.7 | 62.5 | | · WP, DTIC |
| AJ10‑118K | Aerojet | 1982 | pressure‑fed | – / 43.7 | – / ~319 | 90–100 | ~47 vac | 7–9 | | | Delta II · WP |
| AJ10‑190 (OMS) | Aerojet | 1981 | pressure‑fed | – / 26.7 | – / 316 | 118 | 23 vac | 8.6 | 55 | | 100 missions, 1,000 starts · WP |
| LR87‑5 (per chamber) | Aerojet | 1962 | GG | 957 / 1,097 | 259 / 297 | 739? | ~132? | 54 | 8 | | Titan II · WP |
| LR91‑11 | Aerojet | 1960s | GG | – / 467 | – / 316 | 589 | 81 vac | 59 | 49 | | · WP |
| Castor 30XL | NG | 2014 | solid | – / 533 max | – / 294 | 1,392 burnout | | | | | gross 26.4 t · NG catalog |
| Castor 120 | NG | ~1995 | solid | – / 1,957 max | – / 280 | 4,126 burnout | | | | | goal −50 % cost · NG catalog |
| GEM 63XL | NG | 2024 | solid | 2,061 max | | ~5,400 burnout | | | | | · NG datasheet |
| SLS 5‑segment | NG | 2022 | solid | 14,600 max | – / 268 | ~91,000 | | | | ~$640M per pair incl. operations | · OIG |

### Nuclear thermal

| engine | year | thrust vac kN | Isp vac | mass | Pc | notes · source |
|---|---|---|---|---|---|---|
| Kiwi B4E | 1964 | 245 | 834 | | 34.5 | 937 MW · WP Rover |
| NRX‑A6 | 1967 | | | | 41.5 | 1.1 GW, 62 min at power · WP NERVA |
| XE‑Prime | 1969 | 246.7 | 841 | 18,144 (ground‑test engine) | 38.6 | 1.14 GW, 24 starts · WP NERVA |
| SNRE (1972 design) | – | ~73 | 875 | core + reflector + drums 1,160 kg; fuel elements 433 kg | | **~60 kg of 92.5 % U** (secondary, ?) · OSTI 1348063 |
| Rover/NERVA programme | 1955–73 | | | | | ~$1.4B total (then‑year); no unit cost exists |

## Soviet and Russian

### Kerolox

| engine | bureau | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | TWR | Pc | ε | unit cost | notes · source |
|---|---|---|---|---|---|---|---|---|---|---|---|
| RD‑107 | Glushko | 1957 | GG (H2O2 turbine) | 814 / 1,000 | 256 / 313 | | | 59 | | | 4 chambers + verniers · WP |
| RD‑107A | Energomash/Kuznetsov | ~2001 | GG | 839 / 1,020 | 263 / 320 | 1,090 | 79 SL | 60 | | | new injectors +7 s Isp · UEC |
| RD‑108A | Energomash/Kuznetsov | ~2001 | GG | 792 / 922 | 258 / 321 | 1,075 | 75 SL | 54 | | | 99.88 % reliability (UEC) · UEC |
| RD‑0110 | KBKhA | 1965 | GG | – / 298 | – / 326 | 408.5 | 74 vac | 68 | | | > 1,500 built · WP |
| RD‑0124 | KBKhA | 2006 | ox‑rich SC | – / 294 | – / 359 | 572 | 52 vac | 157 | | | same thrust as RD‑0110: +33 s Isp, +40 % mass · WP |
| NK‑33 | Kuznetsov | ~1972 | ox‑rich SC | 1,510 / 1,680 | 297 / 331 | 1,240 | **124 SL** | 146 | ~27 | **$1.1M** (1990s surplus sale, 36 engines) | single‑shaft pump · WP, UEC, AIAA‑98‑3361 |
| NK‑43 | Kuznetsov | ~1972 | ox‑rich SC | – / 1,755 | – / 346 | 1,396 | 128 vac | ~146 | | | vacuum NK‑33 · AIAA‑98‑3361 |
| AJ26 | Aerojet | 2013 | ox‑rich SC | ~1,510 / ~1,680 | 297 / 331 | | | ~146 | | ~$10M (2018, Russian source) | modified NK‑33 · ru WP |
| RD‑170 | Energomash | 1985 | ox‑rich SC, 4 ch. | 7,257 / 7,906 | 310 / 337 | 9,750 | 76 SL | 245 | 36.9 | | · WP |
| RD‑171M | Energomash | ~2007 | ox‑rich SC | 7,257 / 7,904 | 310 / 337 | 9,300 | 80 SL | 250 | | | lighter than RD‑170 · WP |
| RD‑180 | Energomash | 2000 | ox‑rich SC, 2 ch. | 3,830 / 4,150 | 311 / 338 | 5,480 | 71 SL | 262–267 | 36.9 | **~$10M** (1996 deal, $1B/101) → **$23.4M** (2014) | · WP, RD Amross, Lexington |
| RD‑191 | Energomash | 2014 | ox‑rich SC | 1,922 / 2,090 | 311 / 338 | 2,200–2,290 | 86–89 SL | 258 | 37 | **~$4.7M** domestic (2019), target $3.1M | 40 per year target · Kommersant |
| RD‑191M | Energomash | 2023–24 | ox‑rich SC | 2,132 / 2,295 | 314 / 338 | | | | | | +10–15 % thrust · ru WP |
| RD‑181 | Energomash | 2016 | ox‑rich SC | 1,922 / 2,085 | 311 / 338 | | | ~257 | | **$11.2M** (2014 export, $224.5M/20) | · bmpd, WP |
| RD‑193 | Energomash | 2013 | ox‑rich SC | 1,920 / 2,085 | 311 / 338 | 1,900 | 103 SL | ~257 | | | no gimbal: −300 kg · WP |
| RD‑120 | Energomash | 1985 | ox‑rich SC | – / 834 → 912 | – / 350 | 1,125 | 76 → 83 vac | 163 → 178 | 114.5 | | uprated on same hardware · WP |
| RD‑58M | OKB‑1 | 1974? | ox‑rich SC | – / 83.4 | – / 356 | 310 | 27 vac | 78 | 189 | | Block D/DM · WP |

### Hypergolic (N2O4/UDMH)

| engine | bureau | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | TWR | Pc | ε | notes · source |
|---|---|---|---|---|---|---|---|---|---|---|
| RD‑253 | Energomash | 1965 | ox‑rich SC | 1,470 / 1,630 | 285 / 316 | ~1,080 | ~139 SL | 147 | 26.2 | Proton · WP |
| RD‑275 | Energomash | 1995 | ox‑rich SC | 1,590 / 1,750 | 287 / 316 | ~1,070 | ~151 SL | 157 | 26.2 | +7 % Pc, same hardware · WP |
| RD‑275M (RD‑276) | Energomash | 2007 | ox‑rich SC | 1,671 / 1,832 | 288 / 316 | 1,080–1,120 | **152–158 SL** | 165 | 26.2 | highest TWR surveyed · WP, Kommersant |
| RD‑0210/0211 | KBKhA | 1967 | ox‑rich SC | – / 582 | – / 326.5 | 566 | 105 vac | 147 | | Proton stage 2 · WP |
| RD‑250 | Glushko | ~1966 | GG | 788 / 882 | – / 301 | 788? | ? | 83 | | mass suspect · WP |
| RD‑869 | Yuzhnoye | 1986 | GG | – / 20.5 | – / 313 | 196 | 11 vac | 41 | | 50 restarts · WP |
| S5.92 | KhimMash | 1988 | GG | – / 19.6 | – / 327 | 75 | 27 vac | 96 | | Fregat · WP |
| S5.98M | KhimMash | 1990 | GG | – / 19.6 | – / 328.6 | 95 | 21 vac | 98 | | Briz‑M · WP |

### Hydrolox, methalox, nuclear

| engine | bureau | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | TWR | Pc | ε | notes · source |
|---|---|---|---|---|---|---|---|---|---|---|
| RD‑0120 | KBKhA | 1987 | fuel‑rich SC | 1,526 / 1,961 | 354 / 455 | 3,450 | 45 SL | 219 | 85.7 | expendable RS‑25 counterpart · WP |
| KVD‑1 (RD‑56) | KhimMash | 2001 flight | SC | – / 69.6 | – / 462 | 282 | 25 vac | 56 | 200 | sold to India · WP |
| RD‑0146 | KBKhA | tests | closed expander | – / 98 | – / 463 | ~242 | 41 vac | 79 | | never flew · WP |
| RD‑0162 | KBKhA | development | SC (sources disagree) | 2,000 / 2,211 | 301? / 347 | ~2,100 | ~97 SL | 172 | | · ru WP |
| RD‑0169A | KBKhA | development | ox‑rich SC | 1,079 / – | 315 / 346 | ~2,100 | ~52 SL? | ~159 | | reusable Amur · ru WP |
| RD‑0410 | KBKhA | 1978–80s tests | nuclear thermal | – / 35.2 | – / 910 | 2,000 | 1.8 vac | | | 196 MW; uranium mass not found · WP |

## Europe

### Hydrolox

| engine | maker | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | TWR | Pc | ε | unit cost | notes · source |
|---|---|---|---|---|---|---|---|---|---|---|---|
| HM7B | SEP/ArianeGroup | 1979 | GG | – / 62.2 | – / 445 | 155–165 | ~38 vac | 35–37 | 83 | | ~300 built · WP, Braeunig |
| Vulcain 1 | SEP/Snecma | 1996 | GG | ~900 / 1,140 | – / 431 | 1,300–1,475 | ~70 SL | 100–110 | 45 | | · WP, Braeunig |
| Vulcain 2 | Snecma/ArianeGroup | 2005 | GG | ~940 / 1,359 | ~318? / 429 | 1,800 | ~53 SL | 117 | 58 | **~€10M** (ArianeGroup 2017) | 30‑engine contract · WP, ESA, ArianeGroup |
| Vulcain 2.1 | ArianeGroup | 2024 | GG | – / 1,324–1,370 | | 2,000 | ~67 vac | 121 | | | nozzle: 90 % fewer parts, −40 % cost, −30 % production time; printed gas generator · WP, DLR |
| Vinci | ArianeGroup | 2024 | closed expander | – / 180 | – / 457 | ~550 with deployable nozzle | ~33 vac | 60 | 240 | | restartable · WP |

### Storable

| engine | maker | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | TWR | Pc | ε | notes · source |
|---|---|---|---|---|---|---|---|---|---|---|
| Viking 2 | SEP | 1979 | GG | 611 / 690 | – / 281 | 776 | ~80 SL | 55 | | · WP |
| Viking 2B | SEP | 1984 | GG | 643 / – | | 776 | ~84 SL | | | fuel changed to UH25 after an instability · WP |
| Viking 4B | SEP | Ariane 2–4 | GG | – / 805 | – / 296 | 826 | ~99 vac | ~59 | 30.8 | · WP, Braeunig |
| Viking 5C | SEP | 1988 | GG | 678 / 752–758 | – / 278 | 826 | ~84 SL | ~59 | 10.5 | · WP, Braeunig |
| Aestus | DASA/Astrium | 1997 | pressure‑fed | – / 27.5–29.6 | – / 324 | 111 | ~27 vac | 11 | 84 | · WP |
| Aestus II (RS‑72) | Astrium/Rocketdyne | tested | GG | – / 55.4 | – / 340 | 138 | ~41 vac | 60 | 84 | · WP |

### LOX‑hydrocarbon, historical and new

| engine | maker | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | Pc | unit cost | notes · source |
|---|---|---|---|---|---|---|---|---|---|
| Gamma 8 | Bristol Siddeley | 1969 | GG (HTP) | 235 / – | 265 | | | | Black Arrow; programme ~£3M/yr · WP |
| RZ.2 | Rolls‑Royce | 1964 | GG | – / 837 per engine | 282 | | | | Blue Streak · WP |
| Prometheus | ArianeGroup | tests 2022–25 | GG | 980 / – | – / 360 | | 100 | **target €1M** vs ~€10M Vulcain 2 | ~50–70 % printed by mass; 50 per year; 5 reuses · ESA, CNES, ArianeGroup |
| M10 | Avio | 2022 | closed expander | – / 98 | – / ≥362 | 250 | | | printed chamber · Avio |
| Helix 1.0 | RFA | 2023 | ox‑rich SC | 100 / – | 325 / 350 | | ~100? | | · RFA |
| Helix 2.0 | RFA | 2026 | ox‑rich SC | 200 / – | | ≈ Helix 1.0 | | ≈ Helix 1.0 | twice the thrust at the same mass and cost · ESA |
| Aquila | Isar | 2025 | fuel‑rich GG | 75 / 94 (vac) | | | | | 124 hot fires in 12 months · Isar |
| TEPREL‑B | PLD | 2023 | pressure‑fed | 30.2 / – | | | 22 | | · WP |
| Skyrora 70 kN | Skyrora | tests | closed SC (HTP/kero) | ~70 / – | | | | printing −20 % cost, −66 % time (claim) | · Skyrora |

### Solids

| motor | year | thrust kN | Isp | propellant t | inert kg | cost | source |
|---|---|---|---|---|---|---|---|
| EAP P241 | 1996 | 7,080 max | 271 vac | ~238 | 33,000 | | ESA, WP |
| P80 | 2012 | 3,015 max | 280 | 88.4 | 7,330 | development €76M | WP |
| P120C | 2022 | ~4,600 max | 279 | 142 | 11,200 | not published | ESA |
| Zefiro 9 / 23 / 40 | 2012–22 | 317 / 1,120 / 1,304 | 296 / 288 / 294 | 10.6 / 23.8 / 36.2 | 1,315 / 2,486 / – | | WP |

## Japan

| engine | maker | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | TWR | Pc | ε | unit cost | notes · source |
|---|---|---|---|---|---|---|---|---|---|---|---|
| LE‑3 | MHI | 1975 | pressure‑fed (N2O4/A‑50) | – / 52.9 | – / 290 | | | | | | · WP |
| LE‑5 | MHI | 1986 | GG | – / 102.9 | – / 450 | 245–255 | ~42 vac | 36.5 | 140 | | first Japanese hydrolox · WP |
| LE‑5A | MHI | 1994 | expander bleed | – / 121.5 | – / 452 | 242–248 | ~50 vac | 40 | 130 | | "simplified, lower cost" · WP |
| LE‑5B(‑2/‑3) | MHI | 2001→ | expander bleed (chamber only) | – / 137 | – / 447 | 285–303 | ~46–49 vac | 36 | 110 | | cheaper, lowest Isp of family · WP |
| LE‑7 | MHI | 1994 | fuel‑rich SC | 844 / 1,078 | 349 / 446 | 1,714 | 50 SL | 127 | 52 | | expensive, troubled · WP |
| LE‑7A | MHI | 2001 | fuel‑rich SC | 870 / 1,098 | 338 / 440 | 1,800 | 49 SL | ~120–123 | 52 | | redesign for cost and reliability · WP, JAXA |
| LE‑9 | JAXA/MHI | 2023 | expander bleed | ? / 1,471 | – / 425 | 2,400–2,600 | ~58–63 vac | 100 | 37 | none published; enables H3‑30 ≤ ¥5bn (half of H‑IIA) | no preburner: single‑stage pump, blisks, cheap alloys, printed parts; fails safe · JAXA, MHI, IHI 2025 |
| SRB‑A / SRB‑3 | IHI Aerospace | 2001 / 2023 | solid | 2,260–2,300 max | 284 | ~9,000 inert | | | | | SRB‑3 dropped thrust vectoring to cut cost · WP |
| M‑14 | ISAS/IHI | 1997 | solid | 3,780 max | | | | | | M‑V ¥7.5bn per launch | · WP |
| COSMOS | Interstellar | tests | GG (methalox) | ~130 target | | | | | | | pintle injector · USU |

## China and India (secondary)

| engine | maker | year | cycle | thrust SL / vac kN | Isp SL / vac | dry kg | TWR | Pc | ε | notes · source |
|---|---|---|---|---|---|---|---|---|---|---|
| YF‑100 | AALPT | 2015 | ox‑rich SC | 1,200 / 1,340 | 300 / 335 | ~1,920? | ~64 SL | 180 | 35 | RD‑120 lineage · WP |
| YF‑115 | AALPT | 2015 | ox‑rich SC | 147 / 176.5 | – / 341.5 | | | 120 | | · WP |
| YF‑20B | AALPT | 1988 | GG (N2O4/UDMH) | 731.5 / 816 | 259 / 289 | 712.5 | ~105 SL | 71 | ~12 | 236 flown · WP |
| YF‑75 | CALT/AALPT | 1994 | GG | – / 78.5 | – / 438 | 550? | ? | 37.6 | 80 | mass may be two engines · WP |
| YF‑75D | AALPT | 2016 | closed expander | – / 88.4 | – / 442.6 | 265 | 34 vac | 41 | 80 | · WP |
| YF‑77 | AALPT | 2016 | GG | 518 / 700 | 317 / 428 | 2,700? | ~20 SL? | 102 | 49 | mass may be a pair · WP |
| TQ‑12A | LandSpace | 2022 | GG (methalox) | 658 / 785 (vac version) | 285 / 337 | | | 101 | 45 | +9 % thrust, −100 kg vs TQ‑12 · WP |
| Longyun‑70 | Jiuzhou Yunjian | 2020s | GG (methalox) | 686.5 / 770 | 290 / 350 | 880 | ~80 SL | | | throttles 30–100 % · WP |
| Vikas | ISRO | 1993 | GG (N2O4/UDMH) | ? / 725 → 821 | 262 / 293 | | | 53.5 → 58.5 | | licensed Viking · WP |
| CE‑7.5 | ISRO | 2014 | fuel‑rich SC | – / 73.5 | – / 454 | 435 | 17 vac | 58–75 | | · WP |
| CE‑20 | ISRO | 2017 | GG | – / 186 | – / 442 | 588 | ~33 vac | 60 | 100 | chose GG over SC for simplicity · WP |
| SCE‑200 | ISRO | testing | ox‑rich SC | 1,820 / 2,030 | 299 / 335 | ~2,700 | ~69 SL | 180 | | RD‑810 lineage · WP |

## Cost evidence, graded

**Solid** (contracts, government reports, export deals):

| engine | unit cost | per kN (vac) | basis |
|---|---|---|---|
| RS‑25D | $104.5M (2015 $) | ~$46k | OIG manufacturing cost, Shuttle era |
| RS‑25E | $70.5M target; ~$99M on the 2020 order | ~$30–43k | OIG |
| RL10C‑3 | ≤ $14.5M | ≤ ~$134k | 2016 contract including development |
| F‑1 | ≤ $4.4M (1966 $, ~$40M today) | ~$0.6k (1966 $) | GAO contract target |
| RD‑180 | ~$10M (1996) → $23.4M (2014) | ~$2.4–5.6k | export deal |
| RD‑181 | $11.2M (2014) | ~$5.4k | export deal |
| RD‑191 | ~$4.7M (2019), target $3.1M | ~$2.2k | domestic cost estimate |
| Vulcain 2 | ~€10M (2017) | ~$8k | ArianeGroup statement |
| NK‑33 | $1.1M (1990s) | ~$0.7k | **surplus stockpile price, not a production cost** |

**Weak** (rumour or statements of intent): Merlin 1D $0.4–2M; RS‑68
~$20M; BE‑4 ~$8M; RL10 $17–20M; Raptor "well under $1M", goal
< $250k; Prometheus €1M target.

**What drives the outliers** (as the sources explain them):
- **RL10**: hand‑brazed tube‑wall chamber, "a lot of craftwork", and
  a production rate fallen to three or four a year. The RL10C‑X turns
  to additive manufacturing to reverse it.
- **RS‑25**: a 206‑bar reusable staged‑combustion design built in
  small batches and thrown away; the RS‑25E cut is printed parts,
  fewer parts and RS‑68 manufacturing practice.
- **Merlin, Raptor**: production rate (4 a week; more than 1 a day)
  and parts removed at each redesign; Raptor 2 about half of Raptor 1.
- **RS‑68**: deliberately low‑tech (ablative, GG, ~80 % fewer parts
  than the SSME), so cheap per kN despite a TWR of ~46.
- **Prometheus**: the 10× target rests on architecture, printing *and*
  a rate of 50 a year together.
- **LE‑9**: dropped staged combustion for expander bleed specifically
  to cut production cost, at −15 s Isp and ~+35 % mass versus LE‑7A.
