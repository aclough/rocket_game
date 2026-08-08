# Direct SSO / GTO launches, and a real plane change to SSO

## The problem

Right now every Earth-orbit mission funnels through LEO:

```
earth_surface --7800+drag--> leo --500--> sso
earth_surface --7800+drag--> leo --2440--> gto
```

Two things are wrong with that.

**SSO is not 500 m/s from LEO.** SSO is a retrograde ~98° orbit; LEO in this
game is the usual ~28.5° eastward orbit. That is a ~70° plane change, which is
one of the most expensive things you can do in spaceflight — far more expensive
than launching a second rocket. Pricing it at 500 m/s makes "launch to LEO,
then nudge over to SSO" look like the obvious play, when in reality nobody has
ever done it.

**Nothing launches to LEO first and then goes to GTO.** A commercial GTO
mission injects on the ascent, from a short parking coast; the satellite
separates in GTO the same morning it left the pad. Modelling it as "arrive in
LEO, then a separate 1-day transfer" adds a stop that does not exist, and (since
the fencepost work) costs the mission a day it should not cost.

## The change

Add direct ground pairs from Earth's surface to SSO and GTO, and reprice the
LEO↔SSO edge to what a plane change actually costs.

```
earth_surface <--8500+drag--> sso        (new, 0 days)
earth_surface <--10100+drag--> gto       (new, 0 days)
leo           <--8700--------> sso        (was 500)
                low-thrust: 12400        (was 500)
```

Everything else stays as-is. `leo → gto` at 2440 / 1 day stays, so a tug that
is already in LEO can still climb to GTO; it is just no longer how a launch
gets there.

### Where the numbers come from

**earth_surface → sso = 8500.** The LEO edge is 7800. SSO costs more for two
independent reasons: the orbit is higher (~700 km vs ~200 km, worth ~150 m/s of
extra orbital-energy), and a polar/retrograde launch throws away the ~400 m/s
of Earth-rotation assist that an eastward LEO launch gets for free. Call it
+700. Gravity and drag losses are charged separately by the launch-site model,
so this is the ideal-velocity budget only, consistent with the 7800 figure.

**earth_surface → gto = 10100.** The sum via LEO is 7800 + 2440 = 10240. Direct
ascent saves a little by not circularising at perigee altitude and then
re-accelerating out of it — call it 140 m/s. The small discount also breaks the
tie cleanly so the pathfinder has an unambiguous preference and the tests are
not balanced on a coin flip.

**leo ↔ sso = 8700 impulsive.** Δv = 2·v·sin(Δi/2) with v ≈ 7600 m/s (mean of
LEO and SSO circular velocity) and Δi ≈ 69.5° gives 8665.

**leo ↔ sso = 12400 for low thrust.** Edelbaum's combined circular-transfer
result, Δv = 2·v·sin(π/4·Δi), gives 12390. Electric propulsion is *worse* than
impulsive at large plane changes, which is worth having in the map — right now
the edge is spiral-friendly at 500 m/s, so an ion tug can currently hop to SSO
for pocket change.

USER:  Would it make sense to not have a direct LEO <-> SSO edge in the graph so
that pathing is forced to go through higher orbits which are what any sensible
low thrust drive would do?

CLAUDE: I tried it, and no — deleting the edge makes the map say something
sillier than the number it replaces. See the resolution under Q1.

### Consequences

- **SSO gets ~200 m/s more expensive** (8300 via LEO today → 8500 direct). Small
  nerf. SSO payload caps in the competitor config are static numbers and are
  unaffected, so DinoSoar gets slightly better at SSO relative to the player.
- **GTO and GEO get ~140 m/s cheaper**, and GTO missions land a day sooner.
  `earth_surface → geo` becomes `earth → gto → geo` = 11600, replacing
  `earth → leo → gto → geo` = 11740.
- **Nobody routes through LEO to reach SSO ever again** — via-LEO is now 16500
  against 8500 direct.
- Return legs come along symmetrically (`gto → earth_surface` at 10100 with the
  aerobrake flag set), matching how every other ground pair in the map is built.

### Test fallout I already know about

- `test_transfers_from_leo` counts 7 edges — unchanged, LEO keeps its edges.
- `test_shortest_path_multi_hop` asserts `earth → leo → gto → geo`; becomes
  `earth → gto → geo`.
- `test_reverse_transfer_sso_to_leo` asserts the direct `sso → leo` hop; still
  the cheapest (8700 against 16300 via the ground), so it survives, but the dv
  assertion changes.
- `test_transfer_transit_days` gains the two new edges.
- The 20-seed balance bands in `tests/sim_bands.rs` will move. I will run the
  six-seed balance sim before and after and report the delta rather than
  quietly re-baselining. Baseline taken before any of this
  (`simulate --seeds 1..6 --policy basic`):

  ```
  6 seeds: avg final $193,959,105, bankrupt 0/6, ever-profitable 6/6
  avg first-launch month 16.8, dev spend 69.3M, hidden flaws 7.5,
  unit cost 16.9M, payment 35.5M
  ```

---

## Questions

**CLAUDE Q1 — how expensive should the LEO↔SSO plane change be?**
8700 is the straight impulsive cost, and it is what every other Earth-orbit edge
in this map is priced as (single impulse). A bi-elliptic dodge — raise apogee to
near-escape, change plane where you are moving slowly, come back down — does it
for about 6500. Both are "never do this" numbers in practice, so this is really
a question about which story the map tells.

(a) 8700 impulsive, consistent with the neighbouring edges — my preference.
(b) 6500 bi-elliptic, i.e. the map prices the cleverest known way.

USER:  I don't expect that anyone will really want to do this in practice so
I don't think it's a priority.  But is our current pathfinding able to discover
a LEO -> MEO -> SSO path if that's the lowest delta-v one?  If not potentially
we should just eliminate the LEO <-> SSO edge to force the use of the more
efficient path.

**CLAUDE — resolution: keep the edge, at 8700.** Three findings, in order of
how much they matter.

*The pathfinder can't find LEO → MEO → SSO, because there is no MEO ↔ SSO
edge.* The planner is a general A* and would happily use one; the graph simply
does not have it. This map has no inclination axis — each node bakes its own
inclination in, so "MEO" means the equatorial MEO and there is no arc from it
to a polar orbit. Adding one is possible but it is graph surgery beyond this
change.

*It would not help anyway.* A plane change at MEO altitude is cheaper than at
LEO, but not nearly enough: MEO circular velocity is 3.87 km/s, so the 69.5°
change costs 2·3870·sin(34.75°) ≈ 4400, and the two 2100 m/s rungs to get up
and back down bring LEO → MEO → SSO to about **8600** — against 8700 direct.
Eighty metres per second. The bi-elliptic saving is real but it only appears
much higher up: to get the plane change down to ~12 m/s you need apogee around
200,000 km, past GEO and half way to the Moon, and the round trip out and back
is what costs you ~6000. MEO is nowhere near high enough to matter.

*Deleting the edge produces a worse route, not no route.* I built the map with
the new direct launch edge, deleted `leo ↔ sso`, and asked the planner:

```
leo -> sso : ["leo", "earth_surface", "sso"]        16714 m/s
sso -> geo : ["sso", "earth_surface", "leo", "gto", "geo"]  20654 m/s
```

Surface nodes are ordinary nodes to A*, and the ground pairs are symmetric, so
removing the plane change does not make SSO unreachable from orbit — it makes
the planner land the satellite and launch it again. That is a stranger claim
than "you did an expensive plane change", and it costs twice as much, so
nothing is gained on the "nobody will fly this" front either.

So the edge stays at 8700 / 12400. Nobody will use it, which was always the
point; it is just there so the graph has a sane answer instead of a silly one.

USER:  Thats fine, I don't think it will tend to come up much.

**CLAUDE Q2 — is the 200 m/s SSO nerf acceptable, or should direct-to-SSO come
in at 8300 so today's payload numbers are preserved?**
8500 is the honest figure. 8300 would keep existing SSO contracts feeling
exactly as they do now and make this a pure routing change with no balance
component.

USER:  Lets err on the side of accuracy here.

**CLAUDE Q3 — 0 transit days for a direct GTO launch?**
That means launch on the 1st, satellite separates in GTO on the 1st, mission
closes the 1st — the same shape as a LEO launch, and what really happens. The
alternative is 1 day, keeping parity with the existing `leo → gto` edge.

USER:  That was a lot of the point of this change, following on the battery
changed.  Real launches to GTO, SSE, and LEO can get away with not worrying
about endurance concerns.  But with GEO circularization concerns suddenly its
a big issue.

**CLAUDE — 0 days for the GTO launch, yes. But as specced this change would
*remove* the GEO endurance problem rather than create it.** See Q6.

**CLAUDE Q4 — should MEO get a direct launch edge too, in the same pass?**
GPS-class launches do go more or less directly to MEO, so `earth_surface → meo`
at ~9900 would be the same kind of fix. GEO direct-insertion is genuinely rare
(that is what GTO is *for*), so I would not add one for GEO. You only asked for
SSO and GTO, so my default is to leave MEO alone and do it later if you want it.

USER:  Apparently most launches to MEO wait around at LEO first and its rarer to
have a satellite do its own circularization.  So I'm inclined to let things
stand for simlicity.

**CLAUDE Q5 — the payload table's fallback destination list**
(`relevant_destinations` in `src/ui/draw.rs`) is `["leo", "meo", "gto", "geo"]`
when no market is active yet. SSO is missing from it. Add it while I am here?

USER: Sounds good.

---

**CLAUDE Q6 (new, from your Q3 answer) — the GEO endurance problem lives on
`gto → geo`, and that edge is currently 0 days.**

You want GEO to be where endurance starts to bite. As the plan stands it would
go the other way. Today:

```
earth_surface --0d--> leo --1d--> gto --0d--> geo     = 1 transit day
```

After the change as specced:

```
earth_surface --0d--> gto --0d--> geo                 = 0 transit days
```

The one day a GEO mission currently pays is on `leo → gto`, and this change
routes around it. A GEO satellite would arrive the same day it launched and
never draw down a battery — the opposite of what you described.

The fix is to move the time to where it physically belongs. A GTO launch really
is same-day: the satellite separates half an hour after liftoff, and that is
the 0 days from Q3. What takes time is the apogee-raising campaign afterwards —
several burns at successive apogees, with the spacecraft coasting through the
Van Allen belts in between, on its own power, exactly the situation you want to
make expensive. Real missions take anywhere from ~2 days (single-burn) to ~2
weeks (multi-burn), before drift to slot.

So: **`gto ↔ geo` transit_days 0 → 3.** Three is inside the honest range, it
sits sensibly against the neighbours (`leo → lunar_orbit` is 4, `leo → l1` is
5), and it actually bites: `DEFAULT_BATTERY_DAYS` is 1.0, so a design with no
deliberate power system dies on the way and the player has to fit solar or an
RTG to sell GEO. A value of 1 would be the conservative choice but it lands
exactly on the default battery's endurance, which makes the whole thing a
coin-flip rather than a decision.

(a) 3 days — my preference, makes GEO the power-systems tutorial.
(b) 1 day — preserves today's GEO mission duration exactly; endurance is
    marginal rather than binding.
(c) Leave it at 0 — GTO and GEO both become same-day and endurance never
    matters inside the Earth system.

USER:  Apparently most GEO circulariztions are done in a few days with some
tracking input from ground so I think the 3 days is good for both realism and
gamism.  We can do that.

**CLAUDE Q7 — related, and I would rather flag it than fix it silently.**
The low-thrust route to GEO (`leo → meo → geo`, spiral-friendly) is 0 transit
days. An electric tug spiralling to GEO really takes three to six *months*.
That is untouched by this change and wrong in the same direction, but fixing it
is a separate balance question about whether electric propulsion should be slow
as well as efficient. Leave for later?

USER:  In this case essentially all the time is going to be the burn duration
rather than coast phases so I think that's actually correct.  Any ion drive
design the player makes is going to take a similar time without adding in
transit days.

CLAUDE: Agreed and withdrawn — `build_route` already integrates burn time into
`burn_days` from thrust and propellant, so charging transit days on a spiral
edge would count the same months twice.

---

## Outcome

Implemented as agreed. `src/location.rs` and one line of `src/ui/draw.rs`.

```
earth_surface <--8500+drag--> sso     0 days   (new)
earth_surface <--10100+drag--> gto    0 days   (new)
leo           <--8700--------> sso    0 days   (was 500)
                low-thrust: 12400              (was 500)
gto           <--1500--------> geo    3 days   (was 0)
```

**The GEO endurance requirement lands.** For the two-stage test fixture flying
`earth_surface → gto → geo`:

```
no power system : flight_days 4, goes dark on day 2
+ a solar panel : flight_days 4, survives
```

Four days is the launch day plus three days of apogee raising. One day of
default battery carries it through day 1 and no further, so GEO now genuinely
requires fitting solar or an RTG — while LEO, SSO and GTO all stay same-day
deliveries that the default battery covers on its own. That is the shape you
described.

**Balance moved slightly and the 20-seed bands still hold.**

```
before : avg final $193,959,105  unit cost 16.9M  payment 35.5M  0/6 bankrupt  6/6 profitable
after  : avg final $198,077,152  unit cost 17.1M  payment 36.1M  0/6 bankrupt  6/6 profitable
```

+2.1% on the average, with individual seeds moving both ways (seed 1 fell from
$163M to $116M on three launches instead of five; seed 4 rose from $111M to
$183M and stopped losing a vehicle). Unit cost and payment both drift up ~1%,
which is SSO costing 200 m/s more and pushing designs slightly larger.
`tests/sim_bands.rs` passes unchanged — I did not touch the bands.

483 lib tests plus all integration suites pass; clippy clean under `-D warnings`.

**One thing I did not change, flagged rather than fixed:** DinoSoar's payload
caps in `balance_config.rs` are static numbers (`sso` 20,000 kg, `gto`
13,500 kg) rather than being derived from the graph, so the competitor did not
pay the SSO delta-v increase that the player just did. That was true of every
previous graph change too; it is a pre-existing seam, not a new one.
