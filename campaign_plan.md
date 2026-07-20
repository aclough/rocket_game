# Campaign Block-Bid Redesign Plan

Turns the dormant M2 anchor-customer campaign machinery into competed multi-mission
programs: a campaign announcement becomes a big, pause-worthy decision, companies
submit a sealed **per-mission block price**, one winner takes the whole program, and
the missions then issue on schedule as pre-priced contracts at the won price.

This is the "redesign" following the M3 hygiene pass. Open design questions are
marked **Q:** — please answer inline as USER:.

---

## 1. What exists today (dormant)

- `CampaignSpec` per market (spawn chance/month, mission count range, interval days,
  discount range, program name pool) — every market currently has `campaign: None`,
  so nothing spawns (contract.rs:478, disabled at contract.rs:1056).
- `Campaign` (id, name, market, destination, payload_kg, `payment_per_mission` locked
  at announcement with a block-buy discount, missions_total/issued, next_issue_date,
  interval_days) (contract.rs:498).
- `spawn_campaign` rolls monthly per market from the `campaigns_{y}_{m}` world stream;
  announcement posts a MarketEvent-style log entry; `issue_campaign_contracts` emits
  each mission as an ordinary **pre-priced** contract (`bid_deadline: None`) named
  "{Program} Flight {n}" (market_ops.rs:17).
- From M3: sealed-bid solicitations, logistic scoring (`w_cost`, `w_rep`, `rep_target`,
  `budget_tolerance` per market), DinoSoar with a scripted margin bid, standing bid
  rules, award history.

The redesign keeps all of the spawn/issue plumbing and changes **who gets the
missions and at what price**.

## 2. Campaign lifecycle (proposed)

```
Announce ──(bid window)──> Resolve ──> missions issue on cadence at won price
   │                          │
   pause + modal          single winner (or nobody), award history entry
```

### 2.1 Announcement

- Rolled exactly as today (per-market `CampaignSpec`, monthly seeded stream).
- New `CampaignStatus` on `Campaign`:
  `Soliciting { bid_deadline: GameDate, budget_ceiling_per_mission: f64, player_bid: Option<f64> }`
  → `Won { by_player: bool, company: String, price_per_mission: f64 }`
  → (issuing proceeds as today) → retired when all missions issued.
- The announcement is a **decision event**: game pauses (same mechanism as contract
  award / EconomicShift), Critical log entry, and a campaign announcement modal opens
  showing program name, market, destination, payload class, mission count, cadence,
  and bid deadline. Discovery rule as in M3: **no reference price, no ceiling shown**.
- **Q1:** Pause-on-announce for every campaign, or only when the player's fleet could
  plausibly lift the payload (like the solicitation visibility rule)? My lean: pause
  only if some Testing design can lift it — otherwise just a Notable log line, since
  an un-liftable program is noise early game.
USER:  That makes sense.  In theory we the user could quickly pump a rocket out
before bidding ends but in practice the annoyance/utility ratio is too
unfavorable.

### 2.2 Block pricing and the ceiling

- The hidden reference stays what `spawn_campaign` computes today:
  `payload × rate_per_kg × rate_mult × (1 − discount)` — i.e. the block-buy discount
  **is** the economics of the ceiling. Per-mission ceiling =
  `reference × market.budget_tolerance` (same rule as single solicitations).
- Companies bid one number: **price per mission**, applied to every mission in the
  block. Total program value = bid × missions_total (shown in the modal as the
  headline number — that's what makes it feel big).
  USER:  Good way of handling that.
- Scoring reuses the M3 logistic formula unchanged (`bid_score` with the per-market
  weights), one evaluation, winner takes the whole program. Ties break to the player,
  bids over ceiling are rejected — identical rules to single solicitations, so the
  player's price-discovery knowledge transfers.
- COTS-style split awards (two winners sharing missions) stay a **follow-on**, per
  ROADMAP — this plan is single-winner only.

### 2.3 Capacity / readiness to bid

A block is a bigger commitment than one flight, but demanding "free stock ≥ missions"
would make blocks unbiddable (missions arrive over months; you build between them).

Proposal: to submit a block bid a company needs a **capable Testing design with cost
history** (same gate as bid rules) and free stock ≥ 1. Missions then count against
readiness one at a time as they issue, exactly like ordinary contracts. The
cancellation clause (2.4) is what makes overcommitting hurt, not an upfront gate.

- **Q2:** OK with free stock ≥ 1 as the only capacity gate, with the penalty clause
  carrying the real risk? Alternative: require production capacity ≥ mission cadence
  (lines × build rate vs interval_days), which is more simulationist but harder to
  read in the UI.
  USER:  That makes sense.  I can see revisiting it later but that should be
  the context of changing regular bidding too, potentially.

### 2.4 Cancellation and reputation clause

Today a skipped campaign mission just expires like any contract. With a won block that
feels wrong — you signed a program.

Proposal: each issued mission is a normal accepted contract (auto-accepted for the
winner at the won price, normal deadline window). If the winner misses a mission
deadline:

- The mission fails with the normal contract-failure reputation hit **plus** a program
  penalty (config: `campaign_miss_rep_penalty`, e.g. 2× the normal hit).
- After `campaign_max_misses` (config, default 2) missed missions, the customer
  **cancels the remainder**: campaign retires early, Critical event, one larger
  reputation hit (`campaign_cancel_rep_penalty`). No money clawback (keeps the
  accounting simple; reputation is the currency of trust here).
- All three knobs data-driven in `BalanceConfig`, per CLAUDE.md.
- **Q3:** Happy with reputation-only penalties (no cash clawback / termination fee)?
USER:  Yes, and missing the rest of the missions.  This sounds good.

### 2.5 DinoSoar

DinoSoar bids blocks with its existing margin script: `marginal_cost × margin(free_stock)`
per mission, same `bid_floor` and jitter (seeded per campaign id). One extra factor —
a small **block discount** on its margin (config `competitor.block_discount`, e.g.
10%) so DinoSoar behaves like a real incumbent pricing volume work slightly keener.
Won missions go into `scheduled_launches` one at a time as they issue, respecting its
existing capacity model — if it overcommits, it eats the same cancellation clause.

### 2.6 Award history & UI

- Campaign resolution writes an `AwardRecord` (new outcome variants or a `missions`
  count on the existing ones) so the `[H]` modal becomes price discovery for blocks too.
- UI surfaces:
  - Announcement modal (on pause): program details + `[B]`-style bid entry.
  - Contracts pane: a campaigns section (or tag on mission contracts) showing
    "{Program} — flight 3/8, next issue {date}" and who holds it.
  - Won-by-player missions appear as accepted contracts automatically (no re-accept
    click per mission — the block bid was the acceptance).
- **Q4:** Auto-accept each issued mission for the player-winner (my lean — you signed
  the program), or issue them as pre-priced offers you still confirm, with expiry
  counting as a miss under 2.4?
  USER:  Definetly auto-accept.

## 3. Turning campaigns on

Populate `CampaignSpec` for a first set of markets — proposal: GEO Comsats, LEO
Constellation, and COTS (the thematic home of block buys), with spawn chances tuned so
a typical year sees ~1–2 announcements across all markets. Small mission counts to
start (3–8), intervals 45–120 days, discounts 10–25%. All numbers in balance config,
measured against the 200-seed bands before committing.

## 4. Flight ownership (deferred data-structure question)

The remaining hygiene item: `flight_ops.rs` hardcodes `player_company` (fly/dock/
undock and the launch/advance loop), which blocks extracting a shared flight loop and
means competitor launches stay abstract (`world_query` rolls, no real `Flight`).

Option A (minimal, my lean): add `company: CompanyRef` to `Flight` where
`CompanyRef = Player | Competitor(usize)`, `#[serde(default = Player)]` so old saves
load unchanged. The flight loop resolves the ref to `&mut Company` at the top of each
tick. DinoSoar's abstract launches stay as-is for now; this just unblocks the
extraction and future real competitor flights.

Option B: keep flights player-only forever and drop the extraction idea; competitor
launches remain permanently abstract.

- **Q5:** A or B? (A costs one field + a serde default; B closes the door on ever
  watching DinoSoar fly.)
  USER:  Definetly A.

## 5. Tasks (each with its own commit + your approval)

1. **Campaign solicitation core** — `CampaignStatus`, announcement→resolve pipeline,
   player block bid via a place-bid path, scoring reuse, missions issue pre-priced at
   won price to the winner. Player as sole bidder first. Tests: lifecycle unit tests +
   seed-fairness style "reference block bid wins" check.
2. **DinoSoar block bidding** — scripted block bid with block discount, scheduled
   launches per issued mission, capacity interplay. Tests: outbid/undercut/decline,
   determinism.
3. **Cancellation clause + award history + UI** — miss/cancel penalties, AwardRecord
   integration, announcement modal, campaigns display, pause behavior. Render tests
   incl. discovery-rule leak list.
4. **Enable + balance** — populate CampaignSpecs, sim-harness sweeps (does BasicPolicy
   need a block-bid arm? probably yes, small addition), re-baseline the 200-seed
   bands, record findings for M4.
5. *(Independent, can slot anywhere)* **Flight ownership field** per Q5-A, if approved.

Not in scope: split/COTS dual awards, cost-realism retune (M4, together with your
budget_tolerance note), second small-lift competitor (M4).
