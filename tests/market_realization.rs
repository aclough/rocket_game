//! M2 Task: market archetype realization guards.
//!
//! Locks two guarantees of the seed-perturbed realization layer: the
//! mainstay markets (Rideshare, GEO Comsats) are byte-for-byte
//! identical in every world, and the additive-only / structural rules
//! enforced by `MarketsConfig::validate` reject configs that would
//! thin the reputation-0 opening floor.

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::contract::{default_archetypes, realize_markets, MarketArchetype};
use rocket_tycoon::game_state::GameState;
use rocket_tycoon::seed::GameSeed;

fn archetype_by_key<'a>(archetypes: &'a [MarketArchetype], key: &str) -> &'a MarketArchetype {
    archetypes
        .iter()
        .find(|a| a.key == key)
        .unwrap_or_else(|| panic!("no archetype with key `{key}`"))
}

#[test]
fn mainstays_are_identical_in_every_world() {
    let archetypes = default_archetypes();
    let rideshare = archetype_by_key(&archetypes, "market_rideshare");
    let geo = archetype_by_key(&archetypes, "market_geo_comsats");

    for seed_value in 1..=50 {
        let seed = GameSeed::new(seed_value);
        let realized = realize_markets(&seed, &archetypes);

        for (name, arch) in [("rideshare", rideshare), ("geo comsats", geo)] {
            let r = realized
                .iter()
                .find(|r| r.market.id == arch.template.id)
                .unwrap_or_else(|| panic!("{name} realized"));
            assert!(r.present, "seed {seed_value}: {name} should always be present");
            // Identical starting market in every world; only the
            // growth trajectory is seeded (within the archetype range).
            let (lo, hi) = arch.annual_growth_range;
            assert!(
                (lo..=hi).contains(&r.market.annual_growth),
                "seed {seed_value}: {name} annual_growth {} outside ({lo}, {hi})",
                r.market.annual_growth,
            );
            let mut normalized = r.market.clone();
            normalized.annual_growth = arch.template.annual_growth;
            assert_eq!(
                normalized, arch.template,
                "seed {seed_value}: {name} market drifted from its template \
                 beyond the seeded growth rate",
            );
        }
    }
}

#[test]
fn varied_markets_actually_vary() {
    let archetypes = default_archetypes();
    let gov_science = archetype_by_key(&archetypes, "market_gov_science");

    let mut volumes: Vec<u64> = Vec::new();
    for seed_value in 1..=50 {
        let seed = GameSeed::new(seed_value);
        let realized = realize_markets(&seed, &archetypes);
        let r = realized
            .iter()
            .find(|r| r.market.id == gov_science.template.id)
            .expect("gov science realized");
        volumes.push(r.market.base_volume.to_bits());
    }
    volumes.sort_unstable();
    volumes.dedup();
    assert!(
        volumes.len() > 1,
        "expected market_gov_science base_volume to vary across seeds, got {} distinct value(s)",
        volumes.len(),
    );
}

#[test]
fn presence_probability_roughly_honored() {
    let archetypes = default_archetypes();
    let cots = archetype_by_key(&archetypes, "market_cots");
    let n = 200;

    let mut present_count = 0;
    for seed_value in 1..=n {
        let seed = GameSeed::new(seed_value);
        let realized = realize_markets(&seed, &archetypes);
        let r = realized
            .iter()
            .find(|r| r.market.id == cots.template.id)
            .expect("cots realized");
        if r.present {
            present_count += 1;
        }
    }

    let expected_mean = n as f64 * cots.presence_probability;
    let lower = expected_mean - 26.0;
    let upper = expected_mean + 26.0;
    assert!(
        (present_count as f64) >= lower && (present_count as f64) <= upper,
        "market_cots present in {present_count}/{n} seeds, expected within [{lower}, {upper}] \
         of mean {expected_mean}",
    );
}

#[test]
fn exclusive_group_never_yields_both() {
    let archetypes = default_archetypes();
    let leo = archetype_by_key(&archetypes, "market_leo_constellation");
    let meo = archetype_by_key(&archetypes, "market_meo_constellation");
    let n = 200;

    let mut leo_present = 0;
    let mut meo_present = 0;
    for seed_value in 1..=n {
        let seed = GameSeed::new(seed_value);
        let realized = realize_markets(&seed, &archetypes);
        let leo_r = realized
            .iter()
            .find(|r| r.market.id == leo.template.id)
            .expect("leo realized");
        let meo_r = realized
            .iter()
            .find(|r| r.market.id == meo.template.id)
            .expect("meo realized");

        assert!(
            !(leo_r.present && meo_r.present),
            "seed {seed_value}: both constellation markets present, expected exclusivity",
        );

        if leo_r.present {
            leo_present += 1;
        }
        if meo_r.present {
            meo_present += 1;
        }
    }

    assert!(
        leo_present >= 5,
        "leo constellation present in only {leo_present}/{n} seeds; test would pass vacuously",
    );
    assert!(
        meo_present >= 5,
        "meo constellation present in only {meo_present}/{n} seeds; test would pass vacuously",
    );
}

#[test]
fn realization_is_deterministic() {
    let archetypes = default_archetypes();
    for seed_value in [1u64, 7, 42, 123] {
        let seed_a = GameSeed::new(seed_value);
        let seed_b = GameSeed::new(seed_value);
        let realized_a = realize_markets(&seed_a, &archetypes);
        let realized_b = realize_markets(&seed_b, &archetypes);
        assert_eq!(
            realized_a, realized_b,
            "seed {seed_value}: two realizations of the same seed diverged",
        );
    }
}

#[test]
fn game_state_markets_match_realization() {
    let archetypes = default_archetypes();
    for seed_value in [1u64, 7, 42] {
        let gs = GameState::with_balance("Test".into(), seed_value, BalanceConfig::default());
        let seed = GameSeed::new(seed_value);
        let expected: Vec<_> = realize_markets(&seed, &archetypes)
            .into_iter()
            .map(|r| {
                let mut m = r.market;
                // GameState starts the growth clock on active markets.
                if m.active {
                    m.activation_date = Some(gs.start_date);
                }
                m
            })
            .collect();
        assert_eq!(
            gs.markets, expected,
            "seed {seed_value}: GameState markets diverged from direct realization",
        );
    }
}

#[test]
fn default_config_validates() {
    assert_eq!(BalanceConfig::default().markets.validate(), Ok(()));
}

#[test]
fn additive_only_rule_rejects_thinned_floor() {
    let mut config = BalanceConfig::default().markets;
    let idx = config
        .archetypes
        .iter()
        .position(|a| a.key == "market_rideshare")
        .expect("market_rideshare archetype exists");

    // Lowering the volume floor below 1.0 breaks additive-only variance.
    config.archetypes[idx].volume_mult_range = (0.8, 1.0);
    assert!(
        config.validate().is_err(),
        "expected validate() to reject a rideshare volume_mult_range floor below 1.0",
    );

    // Restore, then break the presence-probability floor instead.
    config.archetypes[idx].volume_mult_range = (1.0, 1.0);
    config.archetypes[idx].presence_probability = 0.9;
    assert!(
        config.validate().is_err(),
        "expected validate() to reject a rideshare presence_probability below 1.0",
    );
}

#[test]
fn cadence_validation_rejects_bad_params() {
    use rocket_tycoon::contract::Cadence;

    let base = BalanceConfig::default().markets;

    let mut config = base.clone();
    config.archetypes[0].template.cadence = Cadence::Lumpy { quiet_chance: 1.0 };
    assert!(
        config.validate().is_err(),
        "expected validate() to reject Lumpy quiet_chance of 1.0 (never active)",
    );

    let mut config = base.clone();
    config.archetypes[0].template.cadence = Cadence::Burst { burst_chance: 0.0 };
    assert!(
        config.validate().is_err(),
        "expected validate() to reject Burst burst_chance of 0.0 (never fires)",
    );

    // Opening-floor markets must stay Steady: conserved mean is not a
    // conserved worst case, and the year-1 floor is a worst-case rule.
    let mut config = base.clone();
    let idx = config.archetypes.iter()
        .position(|a| a.key == "market_rideshare")
        .expect("market_rideshare archetype exists");
    config.archetypes[idx].template.cadence = Cadence::Lumpy { quiet_chance: 0.2 };
    assert!(
        config.validate().is_err(),
        "expected validate() to reject non-Steady cadence on an opening-floor market",
    );
}

#[test]
fn duplicate_archetype_key_rejected() {
    let mut config = BalanceConfig::default().markets;
    let dup = config.archetypes[0].clone();
    config.archetypes.push(dup);
    assert!(
        config.validate().is_err(),
        "expected validate() to reject a duplicate archetype key",
    );
}
