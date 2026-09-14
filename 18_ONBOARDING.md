# 18 — Guided start and bidding onboarding

First playtest feedback: the bidding process needs onboarding. Three
pieces, each useful on its own and shipped as its own commit:

1. **The Place Bid modal explains itself** — the rocket that would fly
   the mission, its marginal cost, the deadline, a suggested price.
2. **A lapsed or over-budget solicitation reveals its budget** once it
   has resolved with no winner, so a rejected bid teaches something.
3. **A guided start**: chosen with the company name, walks the same
   step list the Overview panel already shows, with a paragraph of
   explanation per step and a pause-and-popup on each transition.

Same cadence as the refactor plans: one commit per step, approval
before each, `cargo test` and clippy; the render smoke test covers the
new modals; the 200-seed bands are not touched (nothing here changes
what the sim bot does — see step 2's note on the award record).

## 0. What exists today

| Piece | State |
|---|---|
| Next steps | `ui/next_steps.rs`: a rule list evaluated against `GameState`, first three unmet rules drawn on the Overview tab. Deliberately advisory; the module doc says a walked player will resent it. Eight rules from "design an engine" to "launch"; nothing between "bid" and "launch", nothing after |
| Intro | `InputMode::Intro`, a one-screen modal on a new game: two paragraphs, points at Next steps, "any key to begin" |
| Place Bid | `InputMode::BidEntry`: the contract name and an empty `$M` field. No payload, destination, deadline, cost or hint |
| Bid resolution | `resolve_bids` in `game_state/market_ops.rs`: player won → `ContractAwarded` + pause; competitor won → `ContractAwardedToCompetitor { amount, player_bid }`; player over ceiling → `AwardOutcome::PlayerRejected { bid }` and `BidRejected`, ceiling withheld ("the customer doesn't say what the budget was"); no valid bids → lapses with no record and no event |
| Cost knowledge | `GameState::player_capable_cost(dest, payload)` → capable Testing designs + cheapest mean-of-last-5 build cost (None until a rocket has been built); the standing bid rules price at that cost × (1 + margin); a shelf rocket carries its own `build_cost` |
| New game | `bin/main.rs` startup loop: menu → company name → `GameState::with_balance`; no other choices |

## 1. Design

### 1.1 The step table (one source of truth)

`next_steps.rs` becomes a table of `Step`s in order:

```rust
pub struct Step {
    pub id: StepId,                 // enum, one per row; the guide's cursor
    pub line: &'static str,         // the Overview one-liner (today's text)
    pub tab: &'static str, pub key: &'static str,
    pub explain: &'static [&'static str],   // the guide's paragraph, 3–6 lines
    pub done: fn(&GameState) -> bool,       // achieved
    pub applies: fn(&GameState) -> bool,    // worth showing at all (today's guards)
}
```

The Overview panel keeps drawing the first `MAX_SHOWN` steps that
apply and are not done — same behaviour as today for everyone. The
guide walks the same table. Steps added for the bidding arc:

| id | line | done when |
|---|---|---|
| PickSolicitation | Find a solicitation your rocket can lift and open it with B | a `BidEntry` was opened (event-free: `player_bid` set on any contract, or the guide sees the modal open) |
| PlaceBid | Bid a little above your marginal cost; the customer's budget is hidden | a `BidPlaced` event |
| AwaitAward | Wait for the bid date — the clock runs, the award is sealed | `ContractAwarded`, `ContractAwardedToCompetitor` (with a player bid) or `BidRejected` fires |
| LostBid (branch) | You lost — Award History (H) shows the winning price; bid again nearer it | next `BidPlaced` |
| Launch | Launch the contract from the Launches tab | a launch record exists |
| ReadOutcome | Read the result: payment, or the flaw that ended the flight | any key on the launch result |
| Graduate | Standing bid rules (R) and auto-build (M) do this for you from now on | the player opens the bid-rules modal, or three days pass |

The explanation text lives in the table next to the line it expands,
so it cannot go stale on its own.

### 1.2 The guide

- `GameState.guide: Option<GuideState { current: StepId }>`,
  `#[serde(default)]` so old saves are unguided. `None` = the player
  declined or graduated.
- `App` checks once per tick, after `advance_day`, whether the
  current step's `done` holds (or its event fired in `day_events`);
  if so it enters `InputMode::Guide { achieved: StepId, next: StepId }`
  through `enter_modal`, which pauses. The modal says what was just
  done in one line, then the next step's explanation, tab and key;
  any key closes it and the game stays paused until the player
  unpauses (Space), the same as a contract award today.
- The branch: if `AwaitAward` resolves as a loss or rejection, the
  next step is `LostBid`; on a win it is `Launch`. That is the one
  place the table is not linear, so `next` is computed by a small
  function rather than by index.
- The intro modal becomes the guide's first popup when guided, and
  stays as it is when not.
- Esc on a guide popup with a confirmation ("Stop the guide? Next
  steps stay on the Overview tab") sets `guide = None`.

### 1.3 The Place Bid modal

Lines, in order: contract name; payload and destination; bid deadline
and today's date; the rocket that would fly it (the cheapest capable
design, or "no design can lift this yet" in red, in which case the
field is still open but the hint says so); its marginal cost — the
mean of its last five builds if it has been built, else the
`build_cost` of a shelf rocket of that design, else "unknown until one
is built"; and the field, prefilled with cost × (1 + the market's
standing margin, default from the rule engine) rounded to $0.1M, so
Enter bids something sane and Backspace edits it. One hint line: "The
customer's budget is hidden. Higher bids win less often; the award is
sealed until the bid date."

The cost lookup is `player_capable_cost` plus the shelf fallback;
that fallback becomes part of the same function so the bid rules and
the modal agree.

### 1.4 Revealing the budget

When a solicitation resolves with no winner the customer's ceiling is
no longer secret:

- `AwardOutcome::PlayerRejected { bid, ceiling }` (serde default 0.0
  for old records, drawn as "budget unknown"); the `BidRejected` event
  carries the ceiling and reads "Bid of $X on Y rejected — the budget
  was $Z"; the Award History row shows both.
- A solicitation nobody bids on gets `AwardOutcome::Lapsed { ceiling }`
  and a low-importance event, for every solicitation (Q2: the history
  then shows what missions the player cannot fly yet would have paid).

The sim bot never reads award history, so the bands are unaffected;
this is a display change on the record.

### 1.5 The new-game screen

After the company name, one more line: `Guided start: [Yes] / No`,
Left/Right toggles, Enter starts. Default Yes when there are no
saved games, No otherwise (Q1). `startup_loop` returns
the choice and `App::new` seeds `guide` accordingly.

## 2. Steps

### Step 1 — Place Bid modal

The modal as in 1.3; `player_capable_cost` grows the shelf fallback.
Tests: a render test with a capable design and no history shows the
shelf cost; with history shows the mean; with nothing shows the
unknown line; the prefilled bid equals the rule engine's price. Gate:
render smoke, full tests, clippy.

**Step 1 record.** The shelf fallback turned out to be moot — cost
history is written when a rocket finishes integration, so a shelf
rocket always has history — and the honest pre-build figure is a
material estimate instead: `Company::estimated_build_cost(project,
balance)` prices a first build from scratch with the same per-order
formulas `order_rocket_build` charges (engine, tank and assembly,
integration, each under its learning multiplier, the engine curve
advancing per unit as the pipeline does; contracted engines at their
purchase price), and a test holds it equal to a real order's total.
`GameState::bid_basis(index) -> BidBasis { rocket, cost: Option<(f64,
CostBasis)>, margin, suggested }` names the cheapest capable design
(built ones by their last-five mean, `CostBasis::BuildHistory`; unbuilt
by the estimate, `CostBasis::MaterialEstimate`), takes the market's
standing margin (the rule's, enabled or not, else the default), and
suggests `round_price(cost × (1 + margin))` — the rule engine's own
price. The bid rules themselves are unchanged (they still refuse to bid
without history), so the bands are untouched; oracle byte-identical.
`InputMode::BidEntry` carries the basis; B prefills the field with a
pending bid or the suggestion (`ui::millions_text`), and the modal
shows payload and destination, the bid date and today, the rocket, its
cost and basis, the suggested price, the field, and the hidden-budget
hint, in its own 60-column box so nothing clips at 80 columns (a test
renders it at 80 and 120). `game_with_capable_player` and `inject_contract` moved from the
bid-rules suite into `tests/common`; `tests/bid_basis.rs` covers the
three cases; a tabs test presses B and Enter and checks the bid placed
equals the suggestion. 645 tests, clippy clean.

### Step 2 — Budget reveal

`AwardOutcome` and the two events as in 1.4; Award History draws the
ceiling. Test: a rejected bid's record carries the ceiling and the
history line shows it; an unbid solicitation the player could have
flown lapses into the history, one it could not have flown does not.
Save corpus loads (old `PlayerRejected` records default the ceiling).
Gate: save_compat, full tests, clippy; 200-seed bands run once to
confirm no drift (expected identical: the bot ignores the record).

**Step 2 record.** `AwardOutcome::PlayerRejected { bid, ceiling }`
(ceiling `#[serde(default)]`, drawn as "over budget (you $X)" when an
old record has none) and `AwardOutcome::Lapsed { ceiling }`; the
`BidRejected` and `CampaignBidRejected` events carry the ceiling and
say "…the customer's budget of $Z", and new `SolicitationLapsed` /
`CampaignLapsed` events (Routine importance: every market closes unbid
solicitations monthly) announce a lapse with its budget. Both the
solicitation and the program auctions record every lapse (Q2). Award
History shows "over budget $Z (you $X)" and "no bids, budget $Z".
Tests: the over-ceiling bid's event and record carry the contract's
ceiling; an unbid solicitation lapses into the history with its
budget; the history render fixture gained a lapsed row. Save corpus
loads (old `PlayerRejected` records and `BidRejected` log entries
default the ceiling). Oracle byte-identical; 200-seed bands unchanged.
646 tests, clippy clean.

### Step 3 — The step table

`next_steps.rs` restructured as in 1.1 with the new bidding steps and
every step's explanation text; the Overview panel unchanged in
behaviour (its tests pass as they are, plus one that every step's
explanation fits a 68-column modal). No guide yet.

### Step 4 — The guide

`GuideState`, the tick check, `InputMode::Guide`, the branch, the Esc
confirmation, the new-game toggle. Tests: the bot drives a guided game
and every step's popup appears exactly once and in order, with the
loss branch taken when the bot's first bid loses (pick a seed where it
does) and skipped when it wins; an unguided game never enters the
modal; a guided save reloads at the same step. Gate: render smoke
(every guide popup), full tests, clippy.

## 3. Questions for you

1. **Default.** Guided on by default for a new game, or off with the
   option shown? I'd default on: the player this is for is the one
   who won't know to turn it on.
   USER:  I was thinking of it only being on by default if there are no
   pre-existing saves, but maybe just on by default is best.
   CLAUDE: The no-saves rule is cheap (the startup screen already lists
   the saves) and it is the better signal, so: default Yes when there
   are no saves, No otherwise, always shown and toggleable.

2. **Lapsed solicitations.** Record a lapse (with the budget) only
   when the player had a capable rocket, or for every solicitation?
   Every one is more information but buries the player's own bids in
   history; I'd take capable-only.
   USER:  Let's do every one.  They can learn how profitable certain not yet
   available missions are.
   CLAUDE: Every lapse recorded with its budget, then; the history row
   reads "no bids — budget was $Z".

3. **Pause after the popup.** Keep the game paused after the guide
   popup closes (the player presses Space when ready, as after an
   award today), or resume automatically? I'd keep it paused: the
   popup is asking them to go and do something.
   USER:  Definetly pause.
   CLAUDE: Paused it stays.

4. **Graduation.** End the guide after the first flown contract with
   a pointer to bid rules and auto-build, or carry on into programs
   and block bids? I'd stop at the first flown contract; programs
   announce themselves already.
   USER:  Right
   CLAUDE: The guide ends after the first flown contract.
