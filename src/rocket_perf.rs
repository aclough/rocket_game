//! Vehicle performance analysis: what a design can carry where, and
//! whether it stays powered for the trip. Pure functions over a
//! `RocketDesign`, read by the capability caches (`GameState`), the
//! designer's payload table and the sim's probes. Split out of
//! `rocket_project.rs` (17_REFACTOR.md E7), which keeps the R&D
//! workflow.

use crate::location::DELTA_V_MAP;
use crate::rocket::RocketDesign;

/// Payloads above this are not a rocket, they are a bug. Bounds the
/// bracket search in `max_payload_to` so a design the planner will
/// fly with anything aboard can't spin it forever.
const PAYLOAD_SEARCH_CEILING_KG: f64 = 1_000_000.0;

/// Compute the maximum payload mass (in kg) that a rocket design can deliver
/// to a given destination. Returns 0.0 if the destination is unreachable.
///
/// Uses binary search over payload mass.
pub fn max_payload_to(design: &RocketDesign, from: &str, to: &str) -> f64 {
    // Ask the planner about every candidate payload rather than pricing the
    // route once at zero payload and then comparing vacuum delta-v against
    // that frozen number. The route's cost and the vehicle's usable delta-v
    // both move with payload — drag scales with mass, and gravity losses
    // scale with how sluggishly the stack leaves the pad — so a fixed
    // threshold credited heavy rockets with delta-v they never had.
    let reachable = |payload: f64| {
        DELTA_V_MAP.shortest_path_for_rocket(from, to, design, payload).is_some()
    };
    if !reachable(0.0) {
        return 0.0;
    }

    // Bracket by growing from a low guess rather than halving down from a
    // high one. Payload is a small fraction of liftoff mass, so a bracket
    // opened at twice total mass spends its first several searches on
    // payloads nothing could lift — and each search is the expensive part.
    // A percent of total mass lands within a few doublings of the answer
    // from either side.
    let mut lo = 0.0_f64;
    let mut hi = (design.total_mass_kg() * 0.01).max(1.0);
    while reachable(hi) && hi < PAYLOAD_SEARCH_CEILING_KG {
        lo = hi;
        hi *= 2.0;
    }

    // Feasibility is monotonic in payload, so bisect. Stop once the
    // bracket is finer than the kilogram the answer is rounded to; the
    // iteration cap is only a backstop against a pathological bracket.
    for _ in 0..40 {
        if hi - lo <= 0.05 {
            break;
        }
        let mid = (lo + hi) / 2.0;
        if reachable(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    // Round down to nearest kg
    lo.floor().max(0.0)
}

/// How a design's power holds up over a specific trip.
///
/// Days are counted the way the flight loop ticks them: `flight_days` is
/// how many days the craft has to keep its own lights on, and it is 1 for
/// a same-day run to LEO — the launch day itself. Calendar time from
/// launch to arrival is therefore `flight_days - 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TripPower {
    /// Days the craft must stay powered, launch day included. Never 0.
    pub flight_days: u32,
    /// The day its batteries run out, 1-indexed to match `flight_days`,
    /// or `None` if it arrives with the lights still on.
    pub dark_on_day: Option<u32>,
}

impl TripPower {
    pub fn survives(&self) -> bool {
        self.dark_on_day.is_none()
    }
}

/// Fly `path` and report how the design's power holds up.
///
/// Lifting the mass is only half the question. A craft running on nothing
/// but its default battery reaches LEO and hands off a payload the same
/// day, but goes dark long before it reaches Mars — and until this check
/// existed, a contract it could never survive still showed as flyable.
///
/// Flies the real route with the real daily power tick rather than
/// comparing a route length against an endurance number. Burns are flown
/// too: staging drops panels and batteries along with the tanks, so a
/// design that looks comfortable on the pad can be dark once its booster
/// is gone. Same model as the flight loop, so a craft that survives here
/// survives in flight for the same reasons.
///
/// Takes the path rather than looking it up, so a caller that already
/// planned the mission doesn't pay for a second search.
pub fn trip_power_along(
    design: &RocketDesign,
    path: &[&'static str],
    payload_kg: f64,
) -> TripPower {
    let sun_au_at = |loc: &str| {
        DELTA_V_MAP.location(loc).map_or(1.0, |l| l.sun_distance_au())
    };

    let mut rocket = design.instantiate(crate::rocket::RocketId(0), payload_kg);
    let route = crate::flight::build_route_for_rocket(path, design, &rocket, payload_kg);

    // Every leg costs at least the day it closes on, so a route of N legs
    // is never shorter than N days however fast the transfers are.
    let flight_days: u32 = route.iter().map(|l| l.total_days().max(1)).sum::<u32>().max(1);

    let mut day = 0u32;
    for leg in &route {
        // Cruise days are billed at the departure end and the closing day
        // at the far end, matching where the flight loop thinks the craft
        // is when it charges each one. A zero-day leg is just the close.
        let cruise_days = leg.total_days().saturating_sub(1);
        let cruise_au = sun_au_at(leg.from.name());
        for _ in 0..cruise_days {
            day += 1;
            if rocket.run_daily_power_tick(design, cruise_au) {
                return TripPower { flight_days, dark_on_day: Some(day) };
            }
        }
        rocket.burn_sequential(design, leg.delta_v_cost, leg.from.name());
        day += 1;
        if rocket.run_daily_power_tick(design, sun_au_at(leg.to.name())) {
            return TripPower { flight_days, dark_on_day: Some(day) };
        }
    }
    TripPower { flight_days, dark_on_day: None }
}

/// `trip_power_along` for callers that have endpoints rather than a path.
/// `None` when the design cannot fly the route at all.
pub fn trip_power(
    design: &RocketDesign,
    from: &str,
    to: &str,
    payload_kg: f64,
) -> Option<TripPower> {
    let (path, _) = DELTA_V_MAP.shortest_path_for_rocket(from, to, design, payload_kg)?;
    Some(trip_power_along(design, &path, payload_kg))
}

/// Whether `design` can keep its own housekeeping powered for the whole
/// trip from `from` to `to` carrying `payload_kg`. False when it cannot
/// fly the route at all.
pub fn survives_trip(design: &RocketDesign, from: &str, to: &str, payload_kg: f64) -> bool {
    trip_power(design, from, to, payload_kg).is_some_and(|p| p.survives())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::*;
    use crate::propellant::Propellant;
    use crate::stage::*;

    fn kerolox_engine(id: u64, thrust: f64, mass: f64, isp: f64) -> EngineDesign {
        EngineDesign {
            id: EngineId(id),
            name: format!("Engine-{}", id),
            cycle: EngineCycle::GasGenerator,
            thrust_n: thrust,
            mass_kg: mass,
            isp_s: isp,
            exit_pressure_pa: 70_000.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
            power_draw_w: 0.0,
        }
    }

    fn simple_two_stage_design() -> RocketDesign {
        let e1 = kerolox_engine(1, 1_000_000.0, 500.0, 280.0);
        let e2 = kerolox_engine(2, 200_000.0, 100.0, 340.0);
        let s1 = Stage {
            id: StageId(1), name: "S1".into(),
            engine: e1, engine_count: 1,
            propellant_mass_kg: 50_000.0, structural_mass_kg: 3_000.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        let s2 = Stage {
            id: StageId(2), name: "S2".into(),
            engine: e2, engine_count: 1,
            propellant_mass_kg: 10_000.0, structural_mass_kg: 500.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        RocketDesign {
            id: crate::rocket::RocketDesignId(1),
            name: "TestRocket".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        }
    }

    /// Piling propellant onto a fixed engine used to *raise* reported
    /// payload without limit, because the payload search compared vacuum
    /// delta-v against a route cost that charged drag but never gravity.
    /// A stack too heavy to leave the pad was reported lifting 5.5 t to
    /// LEO — more than the same stack at four times the thrust-to-weight.
    ///
    /// Gravity is now charged against the stages that fly the ascent, so
    /// extra propellant on an engine that can't lift it is what it should
    /// be: dead weight.
    #[test]
    fn extra_propellant_cannot_buy_payload_a_weak_engine_cannot_lift() {
        let sized = |propellant: f64| {
            let engine = kerolox_engine(1, 4_000_000.0, 1_500.0, 300.0);
            RocketDesign {
                id: crate::rocket::RocketDesignId(1),
                name: "Sweep".into(),
                stage_groups: vec![vec![Stage {
                    id: StageId(1), name: "S1".into(),
                    engine, engine_count: 1,
                    propellant_mass_kg: propellant,
                    structural_mass_kg: propellant * 0.06,
                    fairing: None,
                    power_sources: Vec::new(),
                }]],
            }
        };

        // Thrust is fixed, so TWR at liftoff falls as propellant rises and
        // crosses 1.0 partway along this sweep.
        let mut best_flyable = 0.0_f64;
        for propellant in [100_000.0, 200_000.0, 300_000.0, 400_000.0,
                           500_000.0, 600_000.0, 800_000.0] {
            let design = sized(propellant);
            let twr = 4_000_000.0 / (design.total_mass_kg() * 9.81);
            let payload = max_payload_to(&design, "earth_surface", "leo");
            if twr >= 1.0 {
                best_flyable = best_flyable.max(payload);
            } else {
                assert!(payload <= best_flyable,
                    "a TWR {twr:.2} stack ({propellant:.0} kg of propellant) reported \
                     {payload:.0} kg to LEO, beating the best any flyable version of \
                     the same rocket managed ({best_flyable:.0} kg)");
            }
        }
    }

    #[test]
    fn test_max_payload_to_leo() {
        let design = simple_two_stage_design();
        let payload = max_payload_to(&design, "earth_surface", "leo");
        // Should be able to carry some payload to LEO
        assert!(payload > 0.0, "Should reach LEO with some payload, got {}", payload);
    }

    /// Reaching a place and surviving the trip there are different
    /// questions, and the readiness colours depend on both.
    #[test]
    fn a_design_can_reach_a_destination_it_cannot_stay_alive_to_reach() {
        let bare = simple_two_stage_design();
        assert!(
            bare.stage_groups.iter().flatten().all(|s| s.power_sources.is_empty()),
            "fixture premise: nothing fitted, so every stage flies the default battery",
        );

        // LEO, SSO and GTO are all same-day trips: the launch injects
        // directly and the payload separates that morning. One day of
        // housekeeping is exactly what the default battery is for.
        for direct in ["leo", "sso", "gto"] {
            assert!(
                max_payload_to(&bare, "earth_surface", direct) > 0.0,
                "fixture premise: this design reaches {direct}",
            );
            assert!(
                survives_trip(&bare, "earth_surface", direct, 0.0),
                "a same-day {direct} delivery is what the default battery covers",
            );
        }

        // MEO is not: it climbs through LEO, so the craft is still flying on
        // the second day and there is nothing aboard generating power.
        assert!(
            max_payload_to(&bare, "earth_surface", "meo") > 0.0,
            "fixture premise: this design can lift mass to MEO",
        );
        assert!(
            !survives_trip(&bare, "earth_surface", "meo", 0.0),
            "no generation means the craft goes dark before it arrives",
        );

        // Give the upper stage a panel that covers its own housekeeping and
        // the same trip becomes survivable — nothing about the delta-v
        // changed, only whether the lights stay on.
        let mut solar = simple_two_stage_design();
        {
            let upper = &mut solar.stage_groups[1][0];
            let demand = upper.housekeeping_w();
            upper.power_sources.push(crate::power::PowerSource::new_solar_panel(demand * 4.0));
        }
        assert!(
            survives_trip(&solar, "earth_surface", "meo", 0.0),
            "a panel that outpaces housekeeping keeps it alive the whole way",
        );
    }

    /// The headline consequence of launching straight to GTO: getting *to*
    /// GTO stops costing endurance, and circularising at GEO starts costing
    /// it. A GEO bird spends three days raising apogee under its own power,
    /// which no default battery covers.
    #[test]
    fn circularising_at_geo_is_what_forces_a_real_power_system() {
        let bare = simple_two_stage_design();
        let path = ["earth_surface", "gto", "geo"];

        let unpowered = trip_power_along(&bare, &path, 0.0);
        assert_eq!(
            unpowered.flight_days, 4,
            "launch day plus three days of apogee raising",
        );
        assert_eq!(
            unpowered.dark_on_day, Some(2),
            "one day of default battery carries it through day 1 and no further",
        );
        assert!(!unpowered.survives());

        let mut solar = simple_two_stage_design();
        {
            let upper = &mut solar.stage_groups[1][0];
            let demand = upper.housekeeping_w();
            upper.power_sources.push(crate::power::PowerSource::new_solar_panel(demand * 4.0));
        }
        let powered = trip_power_along(&solar, &path, 0.0);
        assert_eq!(powered.flight_days, 4, "same route, same duration");
        assert!(powered.survives(), "a panel carries it through the campaign");
    }

    /// The designer quotes both of these numbers, so pin how days are
    /// counted: `flight_days` includes the launch day, which makes ETA
    /// `flight_days - 1` and a same-day LEO run come out as zero.
    #[test]
    fn trip_power_counts_the_launch_day_and_names_the_day_it_goes_dark() {
        let bare = simple_two_stage_design();

        let leo = trip_power(&bare, "earth_surface", "leo", 0.0)
            .expect("fixture premise: this design reaches LEO");
        assert_eq!(leo.flight_days, 1, "a single-leg ascent is one day of lights-on");
        assert_eq!(leo.dark_on_day, None, "the default battery covers exactly that");
        assert!(leo.survives());

        let meo = trip_power(&bare, "earth_surface", "meo", 0.0)
            .expect("fixture premise: this design can lift mass to MEO");
        assert!(meo.flight_days > 1, "MEO climbs through LEO, so more than one day");
        assert_eq!(
            meo.dark_on_day, Some(2),
            "one day of reserve carries it through day 1 and no further",
        );
        assert!(!meo.survives());

        // A route the design cannot fly at all is not a power verdict.
        assert_eq!(trip_power(&bare, "earth_surface", "mars_surface", 0.0), None);
    }

    #[test]
    fn test_payload_decreases_with_distance() {
        let design = simple_two_stage_design();
        let leo_payload = max_payload_to(&design, "earth_surface", "leo");
        let gto_payload = max_payload_to(&design, "earth_surface", "gto");
        if gto_payload > 0.0 {
            assert!(leo_payload > gto_payload, "LEO payload {} should exceed GTO payload {}",
                leo_payload, gto_payload);
        }
    }
}
