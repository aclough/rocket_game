//! The Great Power War arc as the markets see it.

use super::*;

/// Drive a game to a chosen geopolitical state by applying the shifts
/// directly — the arc itself is tested in `geopolitics`, and finding a
/// seed that wars in year N would make these tests hostage to the rates.
fn force_shift(gs: &mut GameState, shift: crate::geopolitics::GeopoliticalShift) {
    use crate::geopolitics::{Geopolitics, GeopoliticalShift as S};
    gs.geopolitics = match shift {
        S::WarBegins => Geopolitics::War { since_year: gs.date.year },
        S::WarEscalates =>
            Geopolitics::WarWithAsat { since_year: gs.date.year, asat_since_year: gs.date.year },
        S::WarEndsWithoutDebris | S::ReconstitutionEnds => Geopolitics::Peace,
        S::WarEndsIntoReconstitution =>
            Geopolitics::Reconstitution { until_year: gs.date.year + 2 },
    };
    gs.apply_geopolitical_shift(shift);
}

fn market(gs: &GameState, id: crate::contract::MarketId) -> &crate::contract::Market {
    gs.markets.iter().find(|m| m.id == id).expect("market exists")
}

/// A war surges reconnaissance launch, pays better for it, and lowers the
/// bar so a player who isn't already trusted can actually bid.
#[test]
fn war_surges_the_reconnaissance_market() {
    use crate::contract::MARKET_NSSL;
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    let before = market(&gs, MARKET_NSSL);
    let base_rep = before.rep_target;
    let base_vol = before.effective_volume(1.0, gs.date);
    assert_eq!(base_rep, 80.0, "premise: the highest bar in the game");

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarBegins);
    let at_war = market(&gs, MARKET_NSSL);

    assert!(at_war.active, "the war opens the market even if the seed never did");
    assert!((at_war.effective_volume(1.0, gs.date) / base_vol - 5.0).abs() < 1e-6,
        "five times the launches");
    assert!(at_war.rate_multiplier(1.0) > 1.0, "and they pay more for them");
    assert_eq!(at_war.effective_rep_target(), 60.0,
        "urgency lowers the bar from 80 to 60");
}

/// The debris cascade is an orbit problem: LEO and SSO collapse, GEO and
/// GTO carry on. A market's loss is the share of its weight that sat in
/// the ruined orbits.
#[test]
fn asat_debris_hits_low_orbits_and_spares_high_ones() {
    use crate::contract::{MARKET_GEO_COMSATS, MARKET_LEO_CONSTELLATION};
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);

    let leo_before = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);
    let geo_before = market(&gs, MARKET_GEO_COMSATS).effective_volume(1.0, gs.date);

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarBegins);
    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarEscalates);

    let leo_after = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);
    let geo_after = market(&gs, MARKET_GEO_COMSATS).effective_volume(1.0, gs.date);

    // LEO Constellation is entirely LEO+SSO, so it takes the full 0.2.
    assert!((leo_after / leo_before - 0.2).abs() < 1e-6,
        "an all-LEO market loses everything: {}", leo_after / leo_before);
    // GEO Comsats fly GTO and GEO only — untouched.
    assert!((geo_after / geo_before - 1.0).abs() < 1e-6,
        "a market with no low-orbit business is unaffected: {}", geo_after / geo_before);
}

/// Reconnaissance is exempt from the debris: its own satellites are what
/// was shot down, which is why it needs *more* launches, not fewer.
#[test]
fn the_reconnaissance_market_is_exempt_from_its_own_debris() {
    use crate::contract::MARKET_NSSL;
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarBegins);
    let at_war = market(&gs, MARKET_NSSL).effective_volume(1.0, gs.date);

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarEscalates);
    let after_asat = market(&gs, MARKET_NSSL).effective_volume(1.0, gs.date);

    assert!((after_asat - at_war).abs() < 1e-6,
        "the surge survives the debris: {at_war} then {after_asat}");
}

/// Peace restores what the war changed, and the replacement wave only
/// follows an exchange that actually happened.
#[test]
fn peace_lifts_the_war_modifiers() {
    use crate::contract::{MARKET_LEO_CONSTELLATION, MARKET_NSSL};
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    let leo_base = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);
    let nro_base = market(&gs, MARKET_NSSL).effective_volume(1.0, gs.date);

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarBegins);
    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarEscalates);
    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarEndsIntoReconstitution);

    let nro_after = market(&gs, MARKET_NSSL).effective_volume(1.0, gs.date);
    assert!((nro_after - nro_base).abs() < 1e-6, "the surge is over");
    assert_eq!(market(&gs, MARKET_NSSL).effective_rep_target(), 80.0,
        "and the bar goes back up");

    let leo_boom = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);
    assert!((leo_boom / leo_base - 2.0).abs() < 1e-6,
        "the low orbits need repopulating: {}", leo_boom / leo_base);

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::ReconstitutionEnds);
    let leo_settled = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);
    assert!((leo_settled - leo_base).abs() < 1e-6, "and then it is over");
}

/// A war that ends without going after satellites leaves nothing behind.
#[test]
fn a_war_without_asat_leaves_no_debris_and_no_boom() {
    use crate::contract::MARKET_LEO_CONSTELLATION;
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    let base = market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date);

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarBegins);
    assert!((market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date) - base).abs()
        < 1e-6, "a shooting war alone doesn't touch commercial LEO");

    force_shift(&mut gs, crate::geopolitics::GeopoliticalShift::WarEndsWithoutDebris);
    assert!((market(&gs, MARKET_LEO_CONSTELLATION).effective_volume(1.0, gs.date) - base).abs()
        < 1e-6, "and leaves nothing to replace");
}

/// Suppressing an orbit removes those launches rather than pushing them
/// somewhere else — the count falls *and* the mix shifts.
#[test]
fn suppressing_an_orbit_removes_launches_rather_than_moving_them() {
    use crate::contract::{MarketModifier, MARKET_NSSL};
    let mut gs = GameState::new("Test".into(), 1_000_000_000.0, 42);
    let m = gs.markets.iter_mut().find(|m| m.id == MARKET_NSSL).unwrap();
    let before = m.effective_volume(1.0, gs.date);

    m.add_modifier(MarketModifier {
        id: "probe".into(),
        destination_volume_mult: vec![("leo".into(), 0.0)],
        ..Default::default()
    });
    let after = m.effective_volume(1.0, gs.date);

    // Losing LEO entirely costs exactly LEO's share of the market's
    // weight. Computed rather than hardcoded because the archetype
    // applies a per-seed tilt of up to 10% to each weight.
    let total: f64 = m.destinations.iter().map(|d| d.weight).sum();
    let leo: f64 = m.destinations.iter()
        .filter(|d| d.location_id == "leo").map(|d| d.weight).sum();
    let expected = 1.0 - leo / total;
    assert!((after / before - expected).abs() < 1e-6,
        "volume falls by the lost orbit's share of the weight: {} vs {expected}",
        after / before);
    assert!((0.55..0.65).contains(&expected), "LEO is ~40% of this market: {expected}");
    assert_eq!(m.destination_mult("leo"), 0.0);
    assert_eq!(m.destination_mult("sso"), 1.0, "untouched orbits stay at 1.0");
}
