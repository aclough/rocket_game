//! The gravity-turn ascent integrator: what a climb off a surface
//! costs each stage group in gravity loss and in sea-level Isp, given
//! the phases it burns in. Read through `DesignPerformance`.

use super::SurfaceProperties;

/// Velocity at which the rocket begins pitching from vertical (gravity turn initiation).
pub const KICK_OVER_VELOCITY: f64 = 45.0;

/// Size of the initial pitch-over kick, in radians.
pub const PITCH_KICK_RAD: f64 = 0.02;

/// Integration timestep for the ascent, in seconds.
pub const ASCENT_TIMESTEP_S: f64 = 0.25;

/// One interval of an ascent burn during which thrust and mass flow are
/// constant: the same engines firing. A stage group whose stages have
/// different burn times is several of these — core plus boosters, then
/// core alone — and the boosters' dry mass leaves at the end of theirs.
#[derive(Debug, Clone)]
pub struct AscentPhase {
    /// Vacuum thrust of everything firing.
    pub thrust_n: f64,
    pub mass_flow_kg_s: f64,
    /// Propellant consumed during this phase.
    pub propellant_kg: f64,
    /// Structure that falls away when the phase ends (empty stages).
    pub dry_mass_dropped_kg: f64,
    /// The nozzles firing, for the altitude-by-altitude Isp penalty.
    /// Empty means "charge nothing" (a vacuum-only fixture).
    pub nozzles: Vec<AscentNozzle>,
}

/// One engine cluster's contribution to an [`AscentPhase`]'s thrust,
/// with the exit pressure that decides how much of it sea level eats.
#[derive(Debug, Clone, Copy)]
pub struct AscentNozzle {
    pub exit_pressure_pa: f64,
    /// Vacuum thrust of the cluster.
    pub thrust_n: f64,
}

/// What the ascent integration charges one stage group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AscentGroupResult {
    /// Gravity loss during the group's burn (m/s).
    pub gravity_loss: f64,
    /// Propellant-weighted mean of the group's Isp fraction over its
    /// burn: 1.0 in vacuum, the pad value for a stage that never climbs,
    /// and in between for a booster that spends most of its propellant
    /// low and the rest where the air has thinned.
    pub isp_fraction: f64,
    /// Altitude when the group's burn ended (m): where the next group
    /// lights, and the check that the climb is a climb.
    pub burnout_altitude_m: f64,
}

/// Thrust-weighted Isp fraction of the nozzles firing at `pressure_pa`.
fn nozzle_fraction(nozzles: &[AscentNozzle], pressure_pa: f64) -> f64 {
    if nozzles.is_empty() || pressure_pa <= 0.0 {
        return 1.0;
    }
    let total: f64 = nozzles.iter().map(|n| n.thrust_n).sum();
    if total <= 0.0 {
        return 1.0;
    }
    nozzles.iter()
        .map(|n| n.thrust_n * crate::engine::isp_fraction(n.exit_pressure_pa, pressure_pa))
        .sum::<f64>() / total
}

/// Simulate a gravity-turn ascent to estimate what the climb costs each
/// stage group: gravity loss, and the sea-level Isp penalty averaged
/// over the propellant actually burned at each altitude.
///
/// Numerically integrates the gravity turn equations with a coarse timestep:
///   d(pitch)/dt = -g * cos(pitch) / velocity + velocity * cos(pitch) / R
///   gravity_loss accumulates g * sin(pitch) * dt each step
///
/// The second term is the Earth-curvature correction: at orbital velocity
/// (v = sqrt(g*R)) the two terms cancel and pitch rate goes to zero (orbit
/// achieved). Without this term vehicles pitch horizontal too early and
/// upper stages show unrealistically low gravity losses.
///
/// `groups` gives each stage group's burn as its phases (see
/// [`AscentPhase`]); the loss is charged to the group whose phase it
/// happened in. Groups are simulated in order, each inheriting the
/// velocity and pitch the previous one reached; the first starts at
/// rest, vertical. The only free parameter is KICK_OVER_VELOCITY (45 m/s),
/// the velocity at which the rocket begins pitching from vertical.
///
/// Thrust at each step is the vacuum figure scaled by the nozzles' Isp
/// fraction at the current altitude's pressure (`surface.pressure_at`),
/// and that fraction, weighted by the propellant burned at it, is the
/// group's `isp_fraction`. Propellant burned after orbital velocity, or
/// in a phase that never entered the integration, counts as vacuum.
///
/// Returns one [`AscentGroupResult`] per stage group.
pub fn simulate_ascent(
    surface: &SurfaceProperties,
    groups: &[Vec<AscentPhase>],
    initial_mass_kg: f64,
) -> Vec<AscentGroupResult> {
    let surface_gravity = surface.gravity_m_s2;
    let body_radius = surface.radius_m;
    let mut velocity = 0.0_f64;
    let mut pitch = std::f64::consts::FRAC_PI_2; // 90° = vertical
    let mut mass = initial_mass_kg;
    // Height gained so far. The vehicle does not fly the whole ascent at
    // sea level: both the gravity it fights and the radius its curvature
    // term uses change on the way up, and holding them at their surface
    // values stalls the pitch-over — which is what made upper stages look
    // like they were burning most of their delta-v straight up.
    let mut altitude = 0.0_f64;
    let mut results = Vec::with_capacity(groups.len());

    let mut kicked_over = false;

    // Local conditions decide when the ascent is over: reaching circular
    // velocity for the current radius means the vehicle is in orbit, and
    // every burn after that is a transfer the delta-v graph already
    // prices impulsively. Without the cutoff the integrator keeps
    // charging `g·sin(pitch)·dt` for the whole of every remaining burn,
    // which for a low-thrust upper stage is tens of km/s of fiction
    // across a burn measured in weeks.
    let orbital = |alt: f64| {
        let r = body_radius + alt;
        (surface_gravity * body_radius * body_radius / r).sqrt()
    };

    for phases in groups {
        let mut group_loss = 0.0;
        // Σ (Isp fraction × propellant burned at it), and the propellant
        // the integration burned at all, for the group's mean fraction.
        let mut fraction_weight = 0.0;
        let mut burned = 0.0;
        let group_propellant: f64 = phases.iter().map(|p| p.propellant_kg).sum();
        for AscentPhase { thrust_n, mass_flow_kg_s, propellant_kg, dry_mass_dropped_kg, nozzles } in phases {
            let (thrust, mass_flow, propellant, dry_mass) =
                (*thrust_n, *mass_flow_kg_s, *propellant_kg, *dry_mass_dropped_kg);
            let mut remaining_prop = propellant;
            // Above this altitude every nozzle firing is under-expanded or
            // matched and the fraction is 1: no pressure lookup needed,
            // which spares the integration an `exp` per step for most of
            // the climb.
            let penalty_ceiling_m = nozzles.iter()
                .map(|n| surface.altitude_where_pressure_falls_to(n.exit_pressure_pa))
                .fold(0.0_f64, f64::max);

            // Skip phases with no propellant/mass flow (solar sails), and any
            // phase that only starts once the vehicle is already orbital.
            if mass_flow <= 0.0 || propellant <= 0.0 || velocity >= orbital(altitude) {
                continue;
            }

            while remaining_prop > 1e-6 && velocity < orbital(altitude) {
                let dt = ASCENT_TIMESTEP_S.min(remaining_prop / mass_flow);
                let r = body_radius + altitude;
                let g = surface_gravity * (body_radius / r).powi(2);

                group_loss += g * pitch.sin() * dt;

                // The air here decides how much of the vacuum thrust the
                // nozzles deliver.
                let fraction = if altitude < penalty_ceiling_m {
                    nozzle_fraction(nozzles, surface.pressure_at(altitude))
                } else {
                    1.0
                };
                let net_accel = thrust * fraction / mass - g * pitch.sin();
                velocity += net_accel * dt;
                velocity = velocity.max(0.0); // can't go backwards
                altitude = (altitude + velocity * pitch.sin() * dt).max(0.0);

                if velocity > KICK_OVER_VELOCITY {
                    // Initiate gravity turn with a small kick if we haven't already
                    if !kicked_over {
                        kicked_over = true;
                        // Small initial pitch-over: ~1 degree
                        pitch -= PITCH_KICK_RAD;
                    }
                    let pitch_rate = g * pitch.cos() / velocity
                        - velocity * pitch.cos() / r;
                    pitch -= pitch_rate * dt;
                    pitch = pitch.clamp(0.0, std::f64::consts::FRAC_PI_2);
                }

                let dm = mass_flow * dt;
                mass -= dm;
                remaining_prop -= dm;
                fraction_weight += fraction * dm;
                burned += dm;
            }

            // Separation: the spent structure falls away, so what burns
            // next doesn't haul it. Omitting this was charging upper
            // stages for mass they'd already dropped, which both
            // understated their acceleration and inflated their loss.
            mass = (mass - dry_mass).max(0.0);
        }
        // Whatever the integration didn't burn (after orbital velocity,
        // or in a phase it skipped) burns in vacuum.
        let isp_fraction = if group_propellant > 0.0 {
            ((fraction_weight + (group_propellant - burned).max(0.0)) / group_propellant).min(1.0)
        } else {
            1.0
        };
        results.push(AscentGroupResult { gravity_loss: group_loss, isp_fraction, burnout_altitude_m: altitude });
        // Next group inherits velocity and pitch
    }

    results
}

/// [`simulate_ascent`] for groups that burn as a single phase each in a
/// vacuum (no Isp penalty): `(thrust_n, mass_flow_kg_s, propellant_kg,
/// dry_mass_kg)` per group. Returns the gravity loss per group.
pub fn simulate_gravity_losses(
    surface_gravity: f64,
    body_radius: f64,
    stage_params: &[(f64, f64, f64, f64)],
    initial_mass_kg: f64,
) -> Vec<f64> {
    let groups: Vec<Vec<AscentPhase>> = stage_params.iter()
        .map(|&(thrust_n, mass_flow_kg_s, propellant_kg, dry_mass_dropped_kg)| vec![AscentPhase {
            thrust_n, mass_flow_kg_s, propellant_kg, dry_mass_dropped_kg, nozzles: Vec::new(),
        }])
        .collect();
    let airless = SurfaceProperties {
        gravity_m_s2: surface_gravity, radius_m: body_radius,
        has_atmosphere: false, atmosphere_density: 0.0, ambient_pressure_pa: 0.0, scale_height_m: 0.0,
    };
    simulate_ascent(&airless, &groups, initial_mass_kg).iter().map(|g| g.gravity_loss).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================
    // Gravity loss simulation tests
    // ==========================================

    const EARTH_RADIUS: f64 = 6_371_000.0;
    const MOON_RADIUS: f64 = 1_737_000.0;

    /// The ascent model's anchor to reality.
    ///
    /// `KICK_OVER_VELOCITY` and `PITCH_KICK_RAD` are the model's only free
    /// parameters — nothing derives them, so without a reference vehicle
    /// they're arbitrary, and the gravity loss they produce feeds straight
    /// into what every rocket in the game can lift. This pins them to a
    /// Falcon 9 v1.2 flying its own published numbers: ~411 t / 22.2 t of
    /// first stage burning ~160 s, ~107.5 t / 4 t of second stage burning
    /// ~400 s, 15.6 t of payload, liftoff TWR ~1.4. Gravity losses for that
    /// class are documented at roughly 1.5-1.7 km/s, most of it on the
    /// first stage.
    ///
    /// Retune the parameters if this drifts — don't widen the band.
    #[test]
    fn gravity_loss_matches_a_real_launch_vehicle() {
        let s1_thrust = 8_000_000.0;
        let s1_flow = s1_thrust / (300.0 * 9.80665);
        let s2_thrust = 934_000.0;
        let s2_flow = s2_thrust / (348.0 * 9.80665);
        let total_mass = 411_000.0 + 22_200.0 + 107_500.0 + 4_000.0 + 15_600.0;

        let losses = simulate_gravity_losses(
            9.81, EARTH_RADIUS,
            &[
                (s1_thrust, s1_flow, 411_000.0, 22_200.0),
                (s2_thrust, s2_flow, 107_500.0, 4_000.0),
            ],
            total_mass,
        );
        let total: f64 = losses.iter().sum();

        assert!((1_500.0..=1_700.0).contains(&total),
            "Falcon 9 class should lose 1.5-1.7 km/s to gravity, got {total:.0} \
             (S1 {:.0}, S2 {:.0})", losses[0], losses[1]);
        assert!(losses[0] > losses[1] * 2.0,
            "the first stage should carry most of the loss, got S1 {:.0} vs S2 {:.0}",
            losses[0], losses[1]);
    }

    #[test]
    fn test_gravity_loss_single_stage_positive() {
        // A single stage launching from Earth: should have significant gravity loss
        let thrust = 2_000_000.0; // 2 MN
        let isp = 300.0;
        let ve = isp * 9.80665;
        let mass_flow = thrust / ve;
        let propellant = 100_000.0;
        let dry_mass = 10_000.0;
        let total_mass = dry_mass + propellant;

        let losses = simulate_gravity_losses(9.81, EARTH_RADIUS, &[(thrust, mass_flow, propellant, dry_mass)], total_mass);
        assert_eq!(losses.len(), 1);
        assert!(losses[0] > 500.0, "Earth launch should have >500 m/s gravity loss, got {}", losses[0]);
        assert!(losses[0] < 3000.0, "Gravity loss should be <3000 m/s, got {}", losses[0]);
    }

    #[test]
    fn test_gravity_loss_higher_twr_means_less_loss() {
        // More engines = higher TWR = rocket gets through vertical phase faster = less gravity loss
        let isp = 300.0;
        let ve = isp * 9.80665;
        let single_thrust = 500_000.0;
        let mass_flow_per_engine = single_thrust / ve;
        let propellant = 50_000.0;
        let dry_mass = 5_000.0;
        let total_mass = dry_mass + propellant;

        // 1 engine
        let loss_1 = simulate_gravity_losses(
            9.81, EARTH_RADIUS,
            &[(single_thrust, mass_flow_per_engine, propellant, dry_mass)],
            total_mass,
        )[0];

        // 3 engines (3x thrust, 3x flow, same propellant = 1/3 burn time)
        let loss_3 = simulate_gravity_losses(
            9.81, EARTH_RADIUS,
            &[(single_thrust * 3.0, mass_flow_per_engine * 3.0, propellant, dry_mass)],
            total_mass,
        )[0];

        assert!(loss_3 < loss_1,
            "3 engines (loss={:.0}) should have less gravity loss than 1 engine (loss={:.0})",
            loss_3, loss_1);
    }

    #[test]
    fn test_gravity_loss_lunar_less_than_earth() {
        let thrust = 1_000_000.0;
        let isp = 300.0;
        let ve = isp * 9.80665;
        let mass_flow = thrust / ve;
        let propellant = 50_000.0;
        let total_mass = 60_000.0;

        let loss_earth = simulate_gravity_losses(
            9.81, EARTH_RADIUS, &[(thrust, mass_flow, propellant, 10_000.0)], total_mass,
        )[0];
        let loss_moon = simulate_gravity_losses(
            1.62, MOON_RADIUS, &[(thrust, mass_flow, propellant, 10_000.0)], total_mass,
        )[0];

        assert!(loss_moon < loss_earth,
            "Moon (loss={:.0}) should have less gravity loss than Earth (loss={:.0})",
            loss_moon, loss_earth);
    }

    #[test]
    fn test_gravity_loss_upper_stages_less_than_first() {
        // Falcon 9-like: first stage has high TWR and long burn.
        // With curvature correction, S1 won't pitch fully horizontal at suborbital
        // speeds, so S2 still shows meaningful gravity loss — but less than S1.
        // S1: 9 engines, ~7MN, burns ~160s
        let thrust_s1 = 7_000_000.0;
        let isp = 300.0;
        let ve = isp * 9.80665;
        let mass_flow_s1 = thrust_s1 / ve;
        let prop_s1 = mass_flow_s1 * 160.0; // 160 second burn

        // S2: 1 engine at higher Isp
        let thrust_s2 = 800_000.0;
        let isp_s2 = 340.0;
        let ve_s2 = isp_s2 * 9.80665;
        let mass_flow_s2 = thrust_s2 / ve_s2;
        let prop_s2 = mass_flow_s2 * 60.0; // 60 second burn

        // Total mass: S1 dry (5000) + S1 prop + S2 dry (1000) + S2 prop + payload (5000)
        let total_mass = 5_000.0 + prop_s1 + 1_000.0 + prop_s2 + 5_000.0;

        let losses = simulate_gravity_losses(
            9.81, EARTH_RADIUS,
            &[
                (thrust_s1, mass_flow_s1, prop_s1, 5_000.0),
                (thrust_s2, mass_flow_s2, prop_s2, 1_000.0),
            ],
            total_mass,
        );

        assert_eq!(losses.len(), 2);
        assert!(losses[0] > losses[1],
            "First stage loss ({:.0}) should exceed upper stage loss ({:.0})",
            losses[0], losses[1]);
    }

    #[test]
    fn test_gravity_loss_ssto_moderate() {
        // SSTO: long burn but most is horizontal after pitch-over
        let thrust = 3_000_000.0;
        let isp = 350.0;
        let ve = isp * 9.80665;
        let mass_flow = thrust / ve;
        let propellant = 200_000.0;
        let total_mass = 220_000.0;

        let losses = simulate_gravity_losses(
            9.81, EARTH_RADIUS, &[(thrust, mass_flow, propellant, 20_000.0)], total_mass,
        );
        assert_eq!(losses.len(), 1);
        // Should be moderate — not as bad as a weak first stage, but still significant
        assert!(losses[0] > 300.0 && losses[0] < 2500.0,
            "SSTO gravity loss should be moderate, got {:.0}", losses[0]);
    }
}
