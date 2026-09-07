# Retiring old designs

Add a key that gets an old design out of the player's way, behind a
confirmation prompt.

## The problem with deleting

A design is not a leaf. By the time a player wants an early Mk1 out of
their list, its id is referenced by:

- outstanding manufacturing orders — `company.rs:55` looks a rocket
  project up by id to work out what a queued stage still needs, and
  returns `None` if it can't find it
- built hardware in inventory (`rocket_project_id`, `EngineSource`)
- spacecraft in flight and every `LaunchRecord` in `launch_history`
- committed rocket designs, whose stages hold
  `EngineSource::PlayerDesign(id)` — deleting an engine design breaks
  every rocket that ever used it
- `auto_build_targets`, `rush_projects`, `engine_build_counts`,
  `engine_cost_history`, `rocket_cost_history`

So a true `Vec::remove` is only safe on a design that has never been
used, which is exactly the case the player cares least about. The one
they actually want gone is the one with the most history behind it.

**Proposal: retire, don't delete.** A `retired: bool` on each project,
`#[serde(default)]` so old saves load. Retiring hides the design from
its pane and from every place it could still be *chosen*, and touches
nothing that was already built. Every id stays resolvable, so no
existing reference can dangle.

This is the same move the codebase already makes for `Proposed`
engines and reactors — hidden from the pane by a `visible_*_projects()`
filter, still present in the `Vec`.

## Q1 — Retire or delete?

**CLAUDE:** Recommend retire-only, one code path for all three kinds.
It gets the same result on screen for a never-built design and is the
only thing that works for a built one.

The alternative — delete when unreferenced, refuse otherwise — means
the key works on the designs you don't mind and refuses on the ones you
do. I don't think that's worth the second code path, but say if you'd
rather have the save actually shrink.

**USER:**  That makes sense and provides everything that's actualy important.

## Q2 — Which panes?

**CLAUDE:** All three: Engines, Reactors, Rockets. Same key, same
prompt.

Note this is not symmetric work. Engines and Reactors already have
`visible_engine_projects()` / `visible_reactor_projects()` and the
matching `engine_pane_real_index()` / `reactor_pane_real_index()` in
`ui/mod.rs`, so retiring is one more clause in an existing filter. The
Rockets pane has neither — `draw.rs:774` and all of
`handle_rockets_key` index `rocket_projects` directly by
`selected_item`. It needs a `visible_rocket_projects()` and a
`rocket_pane_real_index()` built to match the other two, and every key
in that handler rerouted through it.

That refactor is the bulk of the change and it is worth doing anyway —
the Rockets pane being the odd one out is a standing trap for the next
feature that hides a row.

**USER:**  All three.

## Q3 — What else does retiring do?

Hiding alone leaves live machinery running on an invisible project.
Proposed side effects:

| | |
|---|---|
| assigned engineering teams | released — otherwise they keep drawing salary on a project you can't see |
| auto-build target (rockets) | cleared — otherwise it silently keeps ordering builds |
| rush status (rockets) | cleared |
| auto-revise | left as-is; moot once teams are released |
| outstanding manufacturing orders | **left alone** — they finish, and the hardware stays launchable |

**CLAUDE:** On the last row I can see it either way. Letting the orders
finish means builds keep landing for a design that's gone from the
list, which reads as a bug even though it isn't. Refusing to retire
while orders are outstanding is more honest but blocks the
"I've changed my mind about this one" case.

I lean toward allowing it and naming it in the prompt — "3 builds in
progress will still complete" — so it isn't a surprise.

**USER:**  I'm going to go the other way here.  If someone has left auto-build
on naturally there'll be outstanding rockets but in most cases I don't think
people will want more work invested in building them.

## Q4 — What does a retired design stop being eligible for?

Design-time choices only:

- gone from the rocket designer's engine picker
  (`available_engines()`, `ui/mod.rs:3723`)
- gone from the delta-v planner's design list
- not auto-built

Built hardware is unaffected: existing inventory stays launchable,
spacecraft in flight are untouched, launch history still names it.

One I'm unsure about: `market_ops.rs:426` builds bid capability by
walking `rocket_projects`. If a retired design is excluded, any
inventory of it becomes unbiddable — you can still fly it on a contract
you already hold, but it stops winning new ones.

**CLAUDE:** I lean toward excluding it. Retiring a design is a
statement that you don't intend to fly it again, and having it keep
winning contracts would defeat the point. But it does mean retiring a
design you still have three of in the warehouse quietly strands them.
Your call.

**USER:** Yes, I agree, lets exclude it.  It can still launch stuff but
shouldn't factor in bidding.

## Q5 — Is retiring reversible?

**CLAUDE:** Recommend no un-retire. The flag is the whole feature, so
un-retiring is cheap to add, but a second key and a "Retired" section
to browse is a lot of UI for a mistake the confirmation exists to
prevent. Easy to add later if it turns out to bite.

**USER:**  Agreed

## Q6 — The key and the prompt

**CLAUDE:** `X`. It's already the codebase's "remove" key — the rocket
designer's "Remove this stage" and the power editor's remove are both
`X`. `Del` as an alias.

There's no confirmation modal anywhere in the UI yet, so this adds the
first one. Shape:

```
        ┌─ Retire design ────────────────────┐
        │                                    │
        │  Retire Smokey Mk1?                │
        │                                    │
        │  2 teams will be released.         │
        │  3 builds in progress will still   │
        │  complete.                         │
        │                                    │
        │  It stays on past launch records   │
        │  and existing rockets keep flying. │
        │                                    │
        │        [Y] Retire   [Esc] Cancel   │
        └────────────────────────────────────┘
```

`Y` confirms, `Esc`/`N`/anything else cancels. Deliberately **no Enter
default** — Enter is the confirm key in half the other modals and
muscle memory shouldn't be able to retire a design.

New `InputMode::ConfirmRetire { target: RetireTarget }`, where
`RetireTarget` is an enum over the three project ids. Carrying the id
rather than a pane index means the confirmation can't act on the wrong
row if anything reorders underneath it.

**USER:**  Looks good

## Q7 — Wording

**CLAUDE:** "Retire" throughout, over "Delete" or "Archive". The
hardware is real and the history stays, so "delete" would be a lie, and
"archive" implies a place to go and read it.

**USER:**  That makes sense

## Q3a — what cancelling actually means (follows from Q3)

Q3's answer — don't invest more work in a retired design — needs three
sub-decisions the original plan didn't reach. Deciding these rather
than asking again, but flag any you'd have gone the other way on.

**Engines are pooled, so "its orders" is ambiguous.** Manufacturing
components are fungible: an `Engine` order's `rocket_project_id` only
drives priority inheritance, and the engine it produces is consumed by
whatever stage order needs that `EngineSource`. So cancelling every
engine order tagged with a retired rocket can destroy work a *live*
rocket was about to eat.

The rule, keyed off one predicate — is this engine source used by any
stage of any non-retired rocket design?

- **Retiring a rocket** cancels its `RocketIntegration` and `Stage`
  orders outright; those are design-specific and worthless without the
  design. Its `Engine` orders are cancelled *only* if nothing live
  still uses that engine. Otherwise the order survives with its
  `rocket_project_id` cleared to `None`, becoming an ordinary pooled
  build.
- **Retiring an engine** is refused outright while a non-retired rocket
  design still uses it, naming the rocket: *"Smokey Mk1 is still used
  by Lofter — retire that first."* Allowing it would strand those stage
  orders on an engine nothing will ever build. Retired rockets don't
  count, so retiring oldest-first works naturally.

**No refund.** Material cost is charged at order time; cancelling
returns nothing. Partial refunds are a balance question I don't want to
open, and the sunk cost is the honest reading of "you changed your
mind". The prompt says so before you commit.

**Completed stages sitting in inventory are left alone.** They're built
hardware, which Q4 says retiring doesn't touch. Orphan stages for a
retired design just sit there. Only *orders* are cancelled — freeing
their floor space, which falls out of removing them from the queue.

Reactors need none of this: `PowerSourceKind::Reactor` carries a
*cloned* `ReactorDesign`, not a project reference, and there's no
reactor order type. Retiring one is purely cosmetic and always allowed.

**USER:**

## Work

1. `retired: bool` (`#[serde(default)]`) on `EngineProject`,
   `RocketProject`, `ReactorProject`.
2. `Company::retire_engine_project` / `_rocket_project` /
   `_reactor_project`, each taking an id, returning what it did (teams
   released, orders cancelled) for the status line and the prompt.
   `engine_source_in_live_use()` as the shared predicate behind Q3a.
3. `visible_rocket_projects()` + `rocket_pane_real_index()`; add the
   retired clause to the two existing visible filters; reroute
   `handle_rockets_key`.
4. Filter `available_engines()`, the planner's design list, and
   auto-build. Bid capability per Q4.
5. `InputMode::ConfirmRetire` + its draw arm + `X` in all three
   handlers.
6. `keys.rs`: `on_item("X", Some("[X] Retire"), ...)` in `ENGINES`,
   `REACTORS`, `ROCKETS`. Watch the 62-char action limit and
   `MAX_PANE_HINT` — Rockets is already the busiest hint line and may
   need an existing key demoted to modal-only.

## Tests

- retiring hides it from the pane's visible list but leaves the `Vec`
  entry, so the id still resolves
- retiring a rocket cancels its integration and stage orders and frees
  their floor space
- an engine order shared with a live rocket survives its retired
  rocket, with `rocket_project_id` cleared
- an engine order needed by nothing live is cancelled with its rocket
- retiring an engine a live rocket still uses is refused, and the
  message names the rocket
- the same engine retires cleanly once that rocket is retired
- retiring releases teams and clears the auto-build target
- a retired engine is gone from `available_engines()` but the rocket
  design that already uses it still resolves its stages
- `X` opens the confirmation and changes nothing; `Esc` closes it and
  changes nothing; `Y` retires
- the confirmation acts on the id it captured even if the pane
  reorders underneath it
- `documented_keys_still_do_something` and the hint-width guards in
  `keys.rs` already cover the keybinding side once the tables are
  updated

## Outcome

Implemented as planned, with Q3a's sub-decisions as recorded above.

**Shape.** `retired: bool` on all three project types, hidden behind
`visible_engine_projects` / `visible_reactor_projects` / the new
`visible_rocket_projects`. `Company::retirement_plan` works out the
consequences without mutating, the prompt renders that plan, and
`Company::retire` applies it — so what the player is told is what
happens, guarded by `the_plan_shown_matches_what_retiring_does`.

**Eligibility gates went in at the chokepoints**, not at the call sites:
`project_can_serve` (which the contract colours and the bid rules
already shared) for bidding, and `order_rocket_build` /
`order_engine_build` for building — so the `[O]` key, the auto-build
sweep and the sim policy are all covered by one clause each.

**The Rockets pane refactor was the bulk of it**, as expected. It now
has `visible_rocket_projects()` and `rocket_pane_real_index()` matching
the other two panes, and every key in `handle_rockets_key` resolves
through it. `draw_rockets_tab`, the `↑/↓` bound, and the pane hint's
`has_selection` all work off the visible list. Overview's project
counts and `next_steps.rs` moved to the visible lists too — with every
design retired, the game should tell you to design one.

**Both refusal paths are exercised**, and each fix was verified by
temporarily reverting it and watching the matching test go red:
the pooled-engine rule, the engine-in-use refusal, the prompt acting on
its captured id rather than the current row, and the bid-capability
gate.

`X` fits every hint line without demoting an existing key, so the
concern in step 6 didn't materialise.

**Save safety**: `save_compat.rs`'s corpus of real pre-feature saves
fails outright if the `#[serde(default)]` is dropped — confirmed by
removing it and watching all three era tests fail on
`missing field 'retired'`. `tests/retire_round_trip.rs` covers the
other direction, that a retired design is still retired after a reload.

544 lib tests plus 17 green test binaries, clippy clean under
`-D warnings`, and the 200-seed balance guard unchanged — nothing
retires without a keypress, so the sim is untouched.

**Not done, deliberately**: no un-retire (Q5). The Finance tab still
lists retired designs under Rocket Costs and Engine Costs, because the
money was really spent and hiding it would make the column stop adding
up.
