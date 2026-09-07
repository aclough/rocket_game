//! Great-power conflict: a rare, seeded arc that inverts the launch
//! market for a few years.
//!
//! Deliberately *not* an `EconomicCondition`. The economy is a Markov
//! chain that transitions when a state expires, with durations drawn in
//! months at entry; this is an annual hazard process (2%/yr to start,
//! 50%/yr to end, 40%/yr to escalate). Folding one into the other would
//! make the effective annual war probability depend on how long the
//! economy happened to sit in its current state — a factor-of-ten swing
//! driven by an unrelated system.
//!
//! It is also not expressible as an economic *effect*. `EconomySensitivity`
//! maps the single global modifier monotonically, so there is no way to
//! say "everything down, this one market up". War effects are
//! `MarketModifier`s instead, which already carry per-market volume and
//! rate multipliers — plus, added for this, per-destination multipliers
//! and a reputation-bar delta.
//!
//! Rolled from `world_query("geopolitics_<year>")`, so the whole arc is
//! fixed when the world is generated and survives save/load unchanged.

use rand::Rng;
use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::seed::GameSeed;

/// Annual chance a war breaks out. Over fifteen years that is a 26%
/// chance of seeing one at all — deliberately the exception.
pub const WAR_START_CHANCE: f64 = 0.02;
/// Annual chance an ongoing war ends. Geometric, so the mean war runs
/// two years.
pub const WAR_END_CHANCE: f64 = 0.50;
/// Annual chance a war escalates to anti-satellite weapons. Given the
/// end chance above, 57% of wars get there.
pub const ASAT_CHANCE: f64 = 0.40;

/// Orbits a debris cascade makes unusable. GEO and GTO are far enough
/// out to be left alone, which is the whole reason this is expressed
/// per-destination rather than per-market.
pub const DEBRIS_ORBITS: [&str; 2] = ["leo", "sso"];
/// What survives in those orbits while the debris is being thrown.
pub const DEBRIS_VOLUME_MULT: f64 = 0.2;
/// And what the replacement wave looks like afterwards.
pub const RECONSTITUTION_VOLUME_MULT: f64 = 2.0;

/// Wartime demand for reconnaissance launch.
pub const WAR_NRO_VOLUME_MULT: f64 = 5.0;
/// A desperate customer pays more, which is what makes the surge worth
/// having rather than merely busy.
pub const WAR_NRO_RATE_MULT: f64 = 1.5;
/// Wartime urgency lowers the bar: they need lift more than they need a
/// spotless record. Takes the reputation target from 80 to 60.
pub const WAR_NRO_REP_DELTA: f64 = -20.0;

/// Modifier ids, so entering and leaving a state can add and remove the
/// same thing by name.
pub const MOD_WAR_NRO: &str = "war_nro_surge";
pub const MOD_ASAT_DEBRIS: &str = "asat_debris";
pub const MOD_RECONSTITUTION: &str = "asat_reconstitution";

/// Where the world stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Geopolitics {
    #[default]
    Peace,
    /// Shooting war. Reconnaissance launch demand surges; everything
    /// else carries on.
    War { since_year: u32 },
    /// The war has gone after satellites. LEO and SSO fill with debris.
    WarWithAsat { since_year: u32, asat_since_year: u32 },
    /// The war is over and the dead constellations need replacing.
    Reconstitution { until_year: u32 },
}

impl Geopolitics {
    /// One line for the economy pane, or None in peacetime.
    pub fn headline(&self) -> Option<&'static str> {
        match self {
            Geopolitics::Peace => None,
            Geopolitics::War { .. } =>
                Some("GREAT POWER WAR — reconnaissance launch demand surging"),
            Geopolitics::WarWithAsat { .. } =>
                Some("ASAT EXCHANGE — LEO and SSO choked with debris"),
            Geopolitics::Reconstitution { .. } =>
                Some("RECONSTITUTION — replacing the constellations lost to debris"),
        }
    }
}

/// What changed this year, for the event log and for applying effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeopoliticalShift {
    WarBegins,
    WarEscalates,
    WarEndsWithoutDebris,
    WarEndsIntoReconstitution,
    ReconstitutionEnds,
}

impl GeopoliticalShift {
    pub fn flavor(&self) -> &'static str {
        match self {
            GeopoliticalShift::WarBegins =>
                "Great power war breaks out — reconnaissance launch demand surges, \
                 and the DoD opens national security bidding to previously \
                 excluded entrants",
            GeopoliticalShift::WarEscalates =>
                "Anti-satellite weapons used indiscriminately — LEO and SSO are \
                 filling with debris",
            GeopoliticalShift::WarEndsWithoutDebris =>
                "The war ends. Reconnaissance launch demand returns to normal",
            GeopoliticalShift::WarEndsIntoReconstitution =>
                "The war ends. Replacing what the debris destroyed will take years",
            GeopoliticalShift::ReconstitutionEnds =>
                "Reconstitution complete — the orbits are repopulated",
        }
    }
}

/// Roll one year of geopolitics. Call once per game-year.
///
/// The whole arc is a function of the seed and the year, so this is
/// idempotent for a given year and identical across save/load.
pub fn advance_geopolitics(
    state: &mut Geopolitics,
    seed: &GameSeed,
    year: u32,
) -> Option<GeopoliticalShift> {
    let mut rng = seed.world_query(&format!("geopolitics_{year}"));

    match *state {
        Geopolitics::Peace => {
            if rng.gen::<f64>() < WAR_START_CHANCE {
                *state = Geopolitics::War { since_year: year };
                return Some(GeopoliticalShift::WarBegins);
            }
            None
        }
        Geopolitics::War { since_year } => {
            // Escalation is checked first: a war that goes nuclear-adjacent
            // and ends in the same year still leaves the debris behind.
            if rng.gen::<f64>() < ASAT_CHANCE {
                *state = Geopolitics::WarWithAsat { since_year, asat_since_year: year };
                return Some(GeopoliticalShift::WarEscalates);
            }
            if rng.gen::<f64>() < WAR_END_CHANCE {
                *state = Geopolitics::Peace;
                return Some(GeopoliticalShift::WarEndsWithoutDebris);
            }
            None
        }
        Geopolitics::WarWithAsat { asat_since_year, .. } => {
            if rng.gen::<f64>() < WAR_END_CHANCE {
                // The replacement wave runs as long as the shooting did:
                // a three-month exchange and a four-year one shouldn't
                // leave the same hole to fill.
                let asat_years = year.saturating_sub(asat_since_year).max(1);
                *state = Geopolitics::Reconstitution { until_year: year + asat_years };
                return Some(GeopoliticalShift::WarEndsIntoReconstitution);
            }
            None
        }
        Geopolitics::Reconstitution { until_year } => {
            if year >= until_year {
                *state = Geopolitics::Peace;
                return Some(GeopoliticalShift::ReconstitutionEnds);
            }
            None
        }
    }
}

/// The modifier a surging reconnaissance customer carries.
pub fn nro_war_modifier() -> crate::contract::MarketModifier {
    crate::contract::MarketModifier {
        id: MOD_WAR_NRO.into(),
        description: "Wartime reconnaissance surge".into(),
        volume_mult: WAR_NRO_VOLUME_MULT,
        rate_mult: WAR_NRO_RATE_MULT,
        rep_target_delta: WAR_NRO_REP_DELTA,
        ..Default::default()
    }
}

/// Debris suppression for every market that isn't the one buying the
/// replacements.
pub fn debris_modifier() -> crate::contract::MarketModifier {
    crate::contract::MarketModifier {
        id: MOD_ASAT_DEBRIS.into(),
        description: "Orbital debris — LEO and SSO barely usable".into(),
        destination_volume_mult: DEBRIS_ORBITS.iter()
            .map(|o| ((*o).to_string(), DEBRIS_VOLUME_MULT))
            .collect(),
        ..Default::default()
    }
}

/// The replacement wave, expiring on its own.
pub fn reconstitution_modifier(end: GameDate) -> crate::contract::MarketModifier {
    crate::contract::MarketModifier {
        id: MOD_RECONSTITUTION.into(),
        description: "Replacing constellations lost to debris".into(),
        end_date: Some(end),
        destination_volume_mult: DEBRIS_ORBITS.iter()
            .map(|o| ((*o).to_string(), RECONSTITUTION_VOLUME_MULT))
            .collect(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Walk many worlds to check the arc's shape and rarity, rather than
    /// asserting one seed's story.
    fn run_years(seed_value: u64, years: u32) -> Vec<(u32, Geopolitics, Option<GeopoliticalShift>)> {
        let seed = GameSeed::new(seed_value);
        let mut state = Geopolitics::Peace;
        (2001..2001 + years).map(|y| {
            let shift = advance_geopolitics(&mut state, &seed, y);
            (y, state, shift)
        }).collect()
    }

    #[test]
    fn war_is_rare_and_escalation_rarer() {
        let n = 2_000u64;
        let mut wars = 0;
        let mut asats = 0;
        for s in 0..n {
            let hist = run_years(s, 15);
            if hist.iter().any(|(_, _, sh)| *sh == Some(GeopoliticalShift::WarBegins)) {
                wars += 1;
            }
            if hist.iter().any(|(_, _, sh)| *sh == Some(GeopoliticalShift::WarEscalates)) {
                asats += 1;
            }
        }
        let war_pct = 100.0 * wars as f64 / n as f64;
        let asat_pct = 100.0 * asats as f64 / n as f64;
        // 1 - 0.98^15 = 26%; 57% of those escalate.
        assert!((20.0..33.0).contains(&war_pct), "war in {war_pct:.1}% of worlds");
        assert!((9.0..22.0).contains(&asat_pct), "asat in {asat_pct:.1}% of worlds");
        assert!(asats < wars, "escalation is a subset of war");
    }

    #[test]
    fn the_arc_only_moves_through_legal_states() {
        for s in 0..500u64 {
            let mut prev = Geopolitics::Peace;
            for (_, state, _) in run_years(s, 40) {
                let ok = match (prev, state) {
                    (a, b) if a == b => true,
                    (Geopolitics::Peace, Geopolitics::War { .. }) => true,
                    (Geopolitics::War { .. }, Geopolitics::WarWithAsat { .. }) => true,
                    (Geopolitics::War { .. }, Geopolitics::Peace) => true,
                    (Geopolitics::WarWithAsat { .. }, Geopolitics::Reconstitution { .. }) => true,
                    (Geopolitics::Reconstitution { .. }, Geopolitics::Peace) => true,
                    _ => false,
                };
                assert!(ok, "illegal transition {prev:?} -> {state:?} (seed {s})");
                prev = state;
            }
        }
    }

    /// A war can only end into reconstitution if it went through ASAT —
    /// there is nothing to replace otherwise.
    #[test]
    fn reconstitution_only_follows_debris() {
        for s in 0..500u64 {
            let hist = run_years(s, 40);
            for (i, (_, _, shift)) in hist.iter().enumerate() {
                if *shift == Some(GeopoliticalShift::WarEndsIntoReconstitution) {
                    assert!(
                        hist[..i].iter().any(|(_, _, sh)|
                            *sh == Some(GeopoliticalShift::WarEscalates)),
                        "seed {s} reconstituted without an ASAT exchange",
                    );
                }
            }
        }
    }

    /// The whole arc is a function of the seed, so replaying a world
    /// gives the same history — no save-scumming the war away.
    #[test]
    fn the_arc_is_reproducible() {
        for s in 0..50u64 {
            let a: Vec<_> = run_years(s, 30).iter().map(|(y, st, _)| (*y, *st)).collect();
            let b: Vec<_> = run_years(s, 30).iter().map(|(y, st, _)| (*y, *st)).collect();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn reconstitution_lasts_as_long_as_the_shooting() {
        // Find a world that escalates, and check the replacement wave is
        // sized to the exchange rather than fixed.
        for s in 0..2_000u64 {
            let seed = GameSeed::new(s);
            let mut state = Geopolitics::Peace;
            let mut asat_start = None;
            for y in 2001..2041 {
                let before = state;
                advance_geopolitics(&mut state, &seed, y);
                if let Geopolitics::WarWithAsat { asat_since_year, .. } = state {
                    asat_start.get_or_insert(asat_since_year);
                }
                if let (Geopolitics::WarWithAsat { .. }, Geopolitics::Reconstitution { until_year })
                    = (before, state)
                {
                    let shooting = y.saturating_sub(asat_start.unwrap()).max(1);
                    assert_eq!(until_year - y, shooting,
                        "seed {s}: {shooting}y of debris should take {shooting}y to replace");
                    return;
                }
            }
        }
        panic!("no world escalated and recovered in 2000 seeds — check the rates");
    }
}


