# Rocket Tycoon Implementation Roadmap

## Phase 1: Game Loop & Time

**Prereq:** Core data structures (propellant, engine, stage, rocket, location, game state).

1. **Day tick system** — `GameState::advance_day()`, salary tracking, monthly costs
2. **Event queue** — events that pause time (launch results, discoveries, contract offers)
3. **Terminal UI skeleton** (ratatui) — main loop, display day/money, accept "next day"/"next event" commands
4. **Save/load with seed** — game seed stored, deterministic RNG for world generation vs separate RNG for contingent events (explosions, etc.)

## Phase 2: Engineering Teams & Engine Design

1. **Engineering teams** — hire/fire, monthly salary cost, work capacity per day
2. **Engine design workflow** — assign team to design an engine (specify cycle, propellants, thrust/mass/isp targets), work accumulates over days until complete
3. **Flaw system (engines)** — newly designed engines get random flaws based on complexity. Flaws have discovery probability
4. **Engine testing** — assign team to test, work accumulates, flaws discovered over time
5. **Engine revision** — fix discovered flaws, partial reset of production learning
6. **Third-party engines** — some available for purchase (Soviet surplus, etc.), may have unfixable flaws
7. **UI:** Engine design screen, team assignment, testing status

## Phase 3: Rocket Design & Manufacturing

1. **Rocket designer** — compose stages from available engine designs, set propellant/structural mass, arrange into sequential/parallel groups, attach fairings
2. **Flaw system (rockets)** — design-level flaws from integration complexity
3. **Rocket testing** — similar to engine testing
4. **Manufacturing teams & floor space** — hire manufacturing teams, build/expand floor space
5. **Manufacturing orders** — queue engine builds and rocket builds, material costs, build time based on team capacity
6. **Production learning curves** — repeated builds get cheaper/faster; forgetting curve when idle
7. **Resource system** — bill of materials, resource costs, resource inventory
8. **UI:** Rocket designer, manufacturing queue, inventory view

## Phase 4: Contracts & Launching

1. **Contract system** — generated contracts: payload mass, destination orbit, payment, deadline
2. **Contract market** — pool of available contracts, refresh mechanic, varying difficulty/reward
3. **Launch simulation** — select rocket + contract, check delta-v feasibility against path through location graph (including mass-dependent drag), simulate flight
4. **Launch outcomes** — success/failure based on flaw system, partial failures, RNG for unresolved flaws
5. **Fame/reputation** — track record affects contract availability, trust for crewed missions later
6. **Financial summary** — income from contracts, expenses from salaries/materials/facilities
7. **UI:** Contract board, launch screen, results screen, financial overview

## Phase 5: In-Space Operations & Missions

1. **Flight system** — rockets in transit, multi-leg missions through location graph
2. **Spacecraft persistence** — rockets that complete missions can remain in orbit (idle state)
3. **Flight commands** — plan routes, execute burns, jettison stages, deliver payloads
4. **Propellant tracking** — per-stage propellant state during flight, remaining delta-v
5. **Mission types beyond launch** — fuel delivery, crew transport, probe deployment
6. **UI:** Map view with active flights, flight command panel

## Phase 6: Technology Research

1. **Technology tree** — define tech areas (engine cycles, materials, nuclear, etc.)
2. **Research teams** — assign to tech areas, progress over time
3. **Seed-driven tech outcomes** — some techs are dead ends in certain playthroughs, discoverable through investment
4. **Tech unlocks** — new engine cycles, better materials, new part types
5. **Competitor tech visibility** — see what competitors have achieved, option to copy approaches
6. **UI:** Tech tree view, research assignment

## Phase 7: Space Infrastructure

1. **Space stations** — modular construction, hab modules, labs, docking ports
2. **Surface outposts** — lunar/asteroid bases, landing pads
3. **Station/outpost functions:**
   - Research labs (income from third-party research)
   - Tourism (suborbital, orbital, station visits)
   - Manufacturing (in-space production, gated by tech level)
   - Mining (resource extraction, yields determined by seed)
   - Propellant depots (refueling in orbit)
4. **Probes & surveying** — discover resource deposits, map conditions (seed-dependent)
5. **UI:** Station builder, outpost management, resource flows

## Phase 8: Economy & Dynamic World

1. **Demand curves** — discoverable demand for tourism, comms, power; price elasticity
2. **Comms constellations** — recurring revenue from Starlink/Iridium-type services
3. **Space-based solar power** — late-game income stream
4. **Earth simulation** — GDP growth, tech progress, wars/disasters affecting demand
5. **Market dynamics** — cheaper launches increase demand, competitor pricing
6. **Loans & financing** — borrow against reputation, interest rates
7. **Events system** — seed-triggered (COTS-like programs, alien monolith, He3 boom) and random (financial downturns, solar flares)

## Phase 9: Competitors

1. **AI companies** — dinosaur legacy players (minimal tech investment) + new-space rivals
2. **Competitor simulation** — they design rockets, bid on contracts, advance tech
3. **Market competition** — price pressure, reputation races
4. **Espionage/copying** — observe competitor tech choices

## Phase 10: Polish & Extended Content

1. **Routes system** — automate regular missions instead of one-off planning
2. **Crew system** — astronaut hiring, training, morale, risk
3. **Part wear & reusability** — engines with lifetime limits, refurbishment
4. **Expanded solar system** — Mars, asteroids, outer planets
5. **Rare discoveries** — primordial black holes, alien probes (seed-dependent)
6. **Laser energy transfer** — ground-to-orbit or space-to-space power beaming

## Minimum Playable Loop

Phases 1-4 together form the core game loop: earn money → design rockets → launch contracts → earn more money.
