# Power TODO — plan

The three remaining TODO.txt items, all in the power system. Item 1 is the
fencepost work; items 2 and 3 are smaller and independent of it.

---

## Item 1 — Launch/battery fenceposts

> Timing for batteries and launches, related fenceposts. We want a launch to
> LEO on the 1st of the month to arrive in LEO, then deliver any payload,
> then check battery, then for it to progress to the 2nd.

### What happens today

Four separate things conspire to break that ordering:

1. **`advance.rs:19` bumps the date first.** `advance_day` does
   `self.date = self.date.next_day()` before any work, so every tick's work
   is stamped the day *after* the player acted on it. A launch clicked while
   the clock reads the 1st is resolved by a tick that already reads the 2nd.

2. **A 0-day leg still can't finish on its launch day.** `earth_surface→leo`
   has `transit_days: 0`, and `launch_rocket` sets `leg_days_remaining = 0`
   (`flight_ops.rs:289`, `:306`) — but arrival is only ever *checked* inside
   `advance_flights`, so the flight sits on the pad until the next tick.

3. **The power tick runs before arrival.** In `advance_flights`, the battery
   check is at `flight_ops.rs:467`; the leg-completion / arrival /
   payload-delivery block is at `flight_ops.rs:600`. So the battery is
   evaluated at the *departure* location's solar distance, with the payload
   still aboard. This is the inversion the TODO names.

4. **Bug — arrival-day double drain.** `resolve_arrived_flight` pushes the
   craft into `self.spacecraft` (`flight_ops.rs:1007`, `:1030`) during
   `advance_flights`, which `advance.rs:539` calls *before* the parked
   spacecraft power loop at `advance.rs:552`. A persisting craft therefore
   pays two housekeeping days on the day it arrives — once as a `Flight`,
   once as a `Spacecraft`.

### Decisions taken

- **Move the date bump to the end of `advance_day`**, so the tick that
  resolves a 1st-of-month launch is itself stamped the 1st.
- **The arrival day costs one housekeeping day**, not zero. Every calendar
  day the craft exists is a power day, including the day a 0-day leg
  completes. Uniform rule; a marginal battery can brown out on the day it
  reaches orbit.

### Changes

- `advance.rs` — move `self.date = self.date.next_day()` from the top of
  `advance_day` to the bottom.
- `flight_ops.rs` — inside the per-flight loop, move the power tick from
  before the leg-completion block to after it, so the order per flight is:
  decrement leg → endurance/reactor flaw rolls → arrive + deliver → power.
  The tick then uses the *arrival* location's `sun_distance_au`, which is
  what "check battery at LEO" means.
- `flight_ops.rs` / `advance.rs` — mark a craft that became a parked
  `Spacecraft` this tick as already power-ticked, so the `advance.rs:552`
  loop skips it. (Simplest shape: `resolve_arrived_flight` runs the arrival
  power tick itself and the parked loop skips craft whose last tick date ==
  today. Alternative: have `advance_flights` return the set of newly parked
  indices. I lean toward a `last_power_tick: GameDate` field on `Spacecraft`
  — it makes the invariant checkable rather than positional, but it is a
  save-format field, so flagging it rather than assuming.)

CLAUDE: do you want the `last_power_tick` field, or the cheaper
positional fix (skip-list threaded out of `advance_flights`)? The field
survives future reordering; the skip-list doesn't touch the save format.
USER:  Many ships will generate power so I think a  `last_power_tick` isn't
something we want.

### Consequence worth deciding on: January is no longer empty

The start date is `2001-01-01` (`calendar.rs:22`), and contracts are only
ever generated inside the `is_first_of_month()` block (`advance.rs:239` →
`:324`). Today the first tick runs the body on 2001-01-02, so the first
monthly generation lands on **2001-02-01** — a brand-new game has *no
contracts at all for its entire first month*.

With the bump at the end, the first tick runs the body on 2001-01-01 and
January gets its contracts on day one.

I think this is a fix rather than a regression — the empty first month reads
like a bug — but it is a balance change and it will move `sim_bands.rs`.
The yearly launch-drought check already guards with
`self.date != self.start_date` (`advance.rs:470`), so that one is harmless.

CLAUDE: happy to take the January contracts as an improvement and re-baseline
the bands, or should the first tick be suppressed to preserve the cold start?
USER:  Yes, the change makes sense even with baseline consequences.

### Test impact

~29 date-bearing assertions across `tests/` and `game_state/tests.rs`.
`test_advance_to_new_month` (`game_state/tests.rs:36`) is the clearest
casualty: 31 ticks currently land on 2001-02-01, and will land on
2001-01-31 (with Feb 1's MonthStart arriving on tick 32). These get updated
in the same edit per CLAUDE.md.

New test: a launch to LEO on the 1st arrives, delivers, and has its battery
checked at LEO, all within the tick stamped the 1st — and pays exactly one
housekeeping day, not two.

---

## Item 2 — Grandfathered power / default battery

> Check on grandfathered power. Maybe add a default battery instead?

`Rocket::has_explicit_power` (`rocket.rs:708`) returns false when no attached
stage carries a `power_source`, and every power path short-circuits on it —
`run_daily_power_tick` returns `false` immediately (`rocket.rs:737`), and
both callers skip such craft entirely. So a design with no power sources is
*immortal*: it never browns out, anywhere, forever.

Meanwhile `PowerSource::default_battery_for` (`power.rs:96`) already exists
and sizes a one-day battery at 1 W per 10 kg of dry mass — **and is never
called from anywhere in the codebase.** It was written for exactly this and
then not wired up.

### Change

Replace the exemption with the default battery: a stage with no explicit
power sources gets `default_battery_for(stage.dry_mass_kg())` at
instantiation, rather than being skipped by the daily tick. `has_explicit_power`
then goes away (or becomes an internal detail), and the "grandfathered"
concept disappears with it.

The behavioural consequence is real: a source-less craft now survives about
one day away from the pad instead of forever. That is the point of the item,
but it means any existing design that relied on the exemption now dies. For
a LEO delivery that arrives and delivers on its launch day (item 1) this is
fine. For anything further out it is not, which is the *intended* pressure.

CLAUDE: should the default battery be applied at design time (visible in the
designer, counted in dry mass, and therefore affecting delta-v and TWR
sizing), or only at instantiation (invisible, mass-free, purely a
survival-clock)? Design-time is more honest and feeds the autosizer
correctly; instantiation-time is a smaller change and won't perturb existing
designs' performance numbers.

USER:  Design time.  This is going to perturp balance a bit too so makes the
previous decision make more sense.

Old saves: `stored_kwd`/`capacity_kwd` already deserialize, so a loaded craft
with no sources just gains one on load under the instantiation-time option.
Under the design-time option, loaded *designs* change mass — needs a
migration note.

---

## Item 3 — Contract colour coding vs. spacecraft duration

> Make color coding of contracts take into account spacecraft duration

`check_contract_readiness` (`draw.rs:1299`) asks exactly one question:
`max_payload_to(design, "earth_surface", destination) >= contract.payload_kg`.
Nothing consults power. So a battery-only craft with a one-day endurance
shows **green/Ready** against a contract whose route takes 200 days — the
player builds it, flies it, and loses it to a brownout in transit.

There is no explicit "must operate for N days" field on `Contract`; the
duration that matters is the route's own transit time, summed over legs.

### Change

- Add `ContractReadiness::PowerShort` (rendered amber-distinct from
  `NeedsBuild`, or red — see question below).
- In `check_contract_readiness`, once a design passes the payload check,
  sum `transit_days` over the planned route and ask whether the craft's power
  balance survives it: free supply vs. housekeeping demand at each leg's
  solar distance, with battery capacity covering any deficit. Reuse the same
  supply/demand functions the daily tick uses (`free_supply_w`,
  `total_housekeeping_w`) rather than writing a second model — per CLAUDE.md,
  one source of truth.
- Prefer a design that is power-adequate over one that is merely
  payload-adequate: the current loop returns on the *first* design that fits,
  so it should keep scanning and only fall back to `PowerShort` if nothing
  clears both bars.

CLAUDE: two readings of "duration" — (a) survive the transit, which is what
I've planned above, or (b) survive until the contract deadline, i.e. the
craft has to still be alive when the delivery is due. (a) is what actually
kills flights today. Confirm (a) is what you meant, or say if you want both.
USER:  (a) is indeed what I meant.

Colour: `PowerShort` as red (it's as fatal as Impossible, just for a
different reason) or as its own amber tier (it's fixable by editing the
design, unlike Impossible)? I lean amber-distinct.
USER:  Everything is fixable by editing the design, essentially.  We want the
red for no current design works, orange for a buildable rocket fixes it.

---

## Ordering

Items 2 and 3 are independent of item 1 and of each other. Item 1's arrival
reorder makes item 2 survivable for LEO deliveries, so **1 → 2 → 3** is the
kindest order for the balance sim: the fencepost fix lands first, then the
default battery, then the UI catches up to both. Each is a separate commit.

Balance check after each: `simulate --seeds 1..6 --years 10 --policy basic
--summary-only`, plus `sim_bands.rs`.
