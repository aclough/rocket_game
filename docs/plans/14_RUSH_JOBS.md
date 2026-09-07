# Rush Jobs (supersedes the project-priority design below)

## Why the first design was wrong

Relative priority on the Rockets tab answers "how should my factory be
balanced?" The actual use case is "a deadline is coming and I need *this*
rocket out". Those want different controls: the first is a standing bias, the
second is a temporary override with a clear beginning and end.

Playtesting also exposed how weak the idle-only rule is for the real case. A
deadline means *now*, and a standing priority that waits for teams to come free
is exactly the wrong shape.

## The new design

A **Rush Job** is declared in the Manufacturing tab against one rocket build.
Rush orders take teams until there are none left; everything else waits.

### The one structural surprise: builds are already fungible

There is no such thing as "a single rocket construction" in the model today,
and it turns out we don't need one.

- Engines pool by `EngineSource` alone (`Inventory::engine_count`).
- Stages pool by `(rocket_project_id, group_index, stage_index)`.
- An integration order takes whichever stages exist; a stage order takes
  whichever engines of the right source exist.

So two concurrent Draco 5 builds are not distinguishable, and "rush this one"
really means "rush the next Draco 5 out of the door". Which is what a player
under deadline pressure wants anyway.

**This gives us the stealing behaviour for free.** There is no ownership to
transfer: the most-advanced engine of the right source is the one that finishes
first, and it flows to whichever stage order is waiting. Marking the right
orders as rush and letting teams pile onto them is the whole mechanism — the
partially-built components get consumed by the rush job because they complete
first, and the fresh ones fall through to the build that was going to get them.

No `BuildId`, no rebinding of in-flight components, no change to `try_unblock`.

### Mechanism

1. **Declare.** `[R] Rush` on a selected order in the Manufacturing tab marks
   that order's *rocket* as the rush target. Realistically the player selects
   the integration order, but any order of that rocket works.
2. **Propagate backwards through the prerequisite chain.** Integration ←
   stages ← engines. A rush integration order lifts the stage orders it is
   waiting on; those lift the engine orders they are waiting on. This is the
   same priority-inheritance machinery already built for the blocked-order bug
   — it just needs the integration→stage level added to the existing
   stage→engine level.
3. **Allocate.** Any unblocked rush order takes teams ahead of everything
   else. Among several rush orders, apportion evenly (they are all on the same
   critical path).
4. **Clear.** The rush ends when that rocket reaches inventory.

### What survives from the first attempt

- `parent_rocket()` and the order→rocket linkage — needed, unchanged.
- `stage_engine_need()` — needed to answer "which engines is this stage
  waiting on", unchanged.
- The backwards inheritance through blocked orders — this was the load-bearing
  insight and it carries over intact. It is *more* central here, since a rush
  job is almost always blocked behind components when you declare it.
- The `text_width` character-count fix — unrelated, keep.

### What goes

- The `Priority` enum, its weights, and the `[P]` key on the Rockets tab.
- D'Hondt apportionment. Rush is precedence, not proportion, and with rush
  gone from the picture everything else is equal-weight — which is the old
  "fewest teams wins" round-robin. Simpler to go back to it.
- `RocketProject::priority` and its save-compat default.

Net: roughly half the code, and the half that stays is the half that was
hard.

---

## Questions

**CLAUDE Q1 — does declaring a rush job pull teams off other work, or only
collect them as they come free?**
The idle-only rule you picked for standing priorities is a poor fit here: a
deadline means now, and waiting for orders to finish elsewhere could take
in-game weeks. My view is that a rush job *should* preempt — it is explicit,
temporary, and player-initiated, which makes overriding manual assignment far
more defensible than a standing priority quietly doing it. The teams go back
where they were... or don't, which is Q2.

(a) Preempt: declaring a rush job pulls every team onto it immediately.
(b) Idle-only, consistent with the old rule.

USER:  Yes, in this model we want preemption so (a).

**CLAUDE Q2 — when the rush ends, do teams return to what they were doing?**
If rush preempts, we either remember the previous assignment and restore it, or
just let the teams fall idle and be picked up by the normal assigner next tick.
Restoring is more code and more state; falling idle is one line and self-heals
within a day. I lean on letting them fall idle.

USER:  No need to restore I think.  Manual assignments are temporary by their
nature and this change will reduce the need for them.

**CLAUDE Q3 — can more than one rocket be a rush job at a time?**
One at a time is easier to reason about and matches "get *this* out". Several
is more flexible but reintroduces the question of how to split between them —
and if everything is rushed, nothing is.

(a) Exactly one, declaring a new one clears the old.
(b) Any number, teams split evenly across all rush orders.

USER: (b).  Rush orders should have teams split between them the same way that
they would as if they were the only normal orders.

**CLAUDE Q4 — should a rush job also jump the engine queue when it is short?**
Suppose the rush build needs 4 engines, 2 are nearly done and 2 have not been
started. Piling teams on the two in flight is automatic. But should the rush
job also *order* the missing two immediately, ahead of the normal shortfall
logic? Today `order_rocket_build` already ordered whatever it was short of at
order time, so this only matters if stock got consumed by someone else in the
meantime. I lean no — it is a rare edge and the existing shortfall logic
handles the normal case.

USER:  I'm actually having trouble seeing the sequence of events that would lead
to this shortfall.

**CLAUDE — withdrawn, it can't happen.** The invariant holds:

- `uncommitted_engines` is a complete ledger at order time: stock, plus
  everything on the line, minus what blocked stage orders will claim.
- Stage orders are created `waiting_for_prerequisites: true`, so demand is
  counted from the instant it exists — no window where a stage needs engines
  but isn't counted as claiming them.
- The only path that removes an order is completion (`manufacturing.rs:588`),
  which produces the inventory item. No cancellation, and rocket projects are
  never removed.

Supply is never destroyed and demand is never added except by a new build,
which does its own accounting. Nothing to handle.

**CLAUDE Q5 — should I strip the priority work first, or layer rush on top and
remove the old control in the same commit?**
Stripping first gives a clean diff for each, at the cost of an intermediate
commit where the feature is gone. Doing it in one keeps master always useful.
I lean one commit, since the priority feature has never been pushed.

USER:  The priority work hasn't been committed so just do one commit.
