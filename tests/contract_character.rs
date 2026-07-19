//! M2 Task 3: contract character axes guards.
//!
//! Locks two per-market "character" knobs added alongside contract
//! generation: `Market::deadline_days` (per-market deadline window,
//! falling back to the global `MarketsConfig` window when unset) and
//! `Market::failure_severity` (multiplier on reputation penalties for
//! failures/expiries involving that market's contracts).

use rand::SeedableRng;
use rand::rngs::StdRng;

use rocket_tycoon::balance_config::BalanceConfig;
use rocket_tycoon::calendar::GameDate;
use rocket_tycoon::contract::{
    default_archetypes, generate_market_contracts, Contract, ContractStatus,
    MARKET_COTS, MARKET_GOV_SCIENCE,
};
use rocket_tycoon::game_state::GameState;

fn mcfg() -> rocket_tycoon::balance_config::MarketsConfig {
    BalanceConfig::default().markets
}

#[test]
fn per_market_deadline_windows_honored() {
    let archetypes = default_archetypes();
    let markets_cfg = mcfg();
    let current_date = GameDate::new(2001, 1, 1);

    for arch in &archetypes {
        let Some((lo, hi)) = arch.template.deadline_days else {
            continue;
        };

        // Bump a clone's base_volume so a handful of months yields
        // plenty of contracts, force it active (event-market templates
        // start inactive until their emergence fires), and set
        // reputation comfortably above the market's floor so
        // generation never gets skipped.
        let mut market = arch.template.clone();
        market.base_volume = 5.0;
        market.active = true;
        // Pin cadence: this test is about deadline windows, and lumpy/
        // burst markets can legitimately go months without contracts.
        market.cadence = rocket_tycoon::contract::Cadence::Steady;

        let mut rng = StdRng::seed_from_u64(7);
        let mut next_id = 1u64;
        let mut generated = 0usize;
        for month in 0..6u32 {
            let date = current_date.add_days(month * 30);
            // Check each batch against its own issue date, so a
            // deadline can't hide behind a neighboring month's window.
            for c in generate_market_contracts(
                &mut market, &mut rng, &mut next_id, date, 1.0, &markets_cfg,
            ) {
                let span = date.days_until(&c.deadline);
                assert!(
                    span >= lo && span <= hi,
                    "archetype `{}`: contract `{}` deadline {} days from issue, \
                     outside [{lo}, {hi}]",
                    arch.key, c.name, span,
                );
                generated += 1;
            }
        }

        assert!(
            generated >= 10,
            "archetype `{}`: only generated {generated} contracts, need >= 10 for a meaningful check",
            arch.key,
        );
    }
}

#[test]
fn global_deadline_fallback_used_when_unset() {
    let archetypes = default_archetypes();
    let markets_cfg = mcfg();
    let current_date = GameDate::new(2001, 1, 1);

    let arch = archetypes
        .iter()
        .find(|a| a.key == "market_rideshare")
        .expect("market_rideshare archetype exists");

    let mut market = arch.template.clone();
    market.deadline_days = None;
    market.base_volume = 5.0;
    market.cadence = rocket_tycoon::contract::Cadence::Steady;

    let mut rng = StdRng::seed_from_u64(11);
    let mut next_id = 1u64;
    let contracts = generate_market_contracts(
        &mut market, &mut rng, &mut next_id, current_date, 1.0, &markets_cfg,
    );

    assert!(
        !contracts.is_empty(),
        "expected at least one contract to check the fallback window",
    );

    for c in &contracts {
        let span = current_date.days_until(&c.deadline);
        assert!(
            span >= markets_cfg.deadline_min_days && span <= markets_cfg.deadline_max_days,
            "contract `{}` deadline span {span} not within global window [{}, {}]",
            c.name, markets_cfg.deadline_min_days, markets_cfg.deadline_max_days,
        );
    }
}

#[test]
fn severity_data_shape() {
    let archetypes = default_archetypes();

    let by_key = |key: &str| {
        archetypes
            .iter()
            .find(|a| a.key == key)
            .unwrap_or_else(|| panic!("no archetype with key `{key}`"))
    };

    let cots = by_key("market_cots");
    let gov_science = by_key("market_gov_science");
    let rideshare = by_key("market_rideshare");
    let geo = by_key("market_geo_comsats");

    // COTS (crew-adjacent) is the strictly harshest market.
    for a in &archetypes {
        if a.key == "market_cots" {
            continue;
        }
        assert!(
            cots.template.failure_severity > a.template.failure_severity,
            "market_cots severity {} should exceed `{}` severity {}",
            cots.template.failure_severity, a.key, a.template.failure_severity,
        );
    }
    assert!(
        cots.template.failure_severity >= 2.0,
        "market_cots severity {} should be >= 2.0",
        cots.template.failure_severity,
    );

    // Government science is the most lenient.
    assert!(
        gov_science.template.failure_severity < 1.0,
        "market_gov_science severity {} should be < 1.0",
        gov_science.template.failure_severity,
    );

    // Every severity is strictly positive.
    for a in &archetypes {
        assert!(
            a.template.failure_severity > 0.0,
            "archetype `{}` has non-positive failure_severity {}",
            a.key, a.template.failure_severity,
        );
    }

    // The two mainstays sit close to baseline.
    for a in [rideshare, geo] {
        assert!(
            a.template.failure_severity > 0.5 && a.template.failure_severity < 1.5,
            "archetype `{}` severity {} should be within (0.5, 1.5)",
            a.key, a.template.failure_severity,
        );
    }
}

#[test]
fn expiry_applies_market_severity_end_to_end() {
    let cfg = BalanceConfig::default();
    let cots_severity = cfg.markets.archetypes.iter()
        .find(|a| a.template.id == MARKET_COTS)
        .expect("cots archetype exists")
        .template.failure_severity;
    let gov_science_severity = cfg.markets.archetypes.iter()
        .find(|a| a.template.id == MARKET_GOV_SCIENCE)
        .expect("gov science archetype exists")
        .template.failure_severity;
    let expiry_penalty = cfg.reputation.expiry_penalty;

    // COTS: harsh multiplier.
    {
        let mut gs = GameState::with_balance("Test".into(), 42, BalanceConfig::default());
        let deadline = gs.date;
        gs.player_company.active_contracts.push(Contract {
            id: rocket_tycoon::contract::ContractId(9001),
            name: "Test COTS Contract".into(),
            destination: "leo".into(),
            payload_kg: 1000.0,
            payment: 1_000_000.0,
            deadline,
            status: ContractStatus::Accepted,
            market_id: MARKET_COTS,
            campaign_id: None,
            bid_deadline: None,
            budget_ceiling: 0.0,
            player_bid: None,
        });
        gs.advance_day();

        let expected = -expiry_penalty * cots_severity;
        assert!(
            (gs.player_company.reputation.expiry_factor - expected).abs() < 1e-9,
            "expected expiry_factor {expected}, got {}",
            gs.player_company.reputation.expiry_factor,
        );
    }

    // Gov science: lenient multiplier.
    {
        let mut gs = GameState::with_balance("Test2".into(), 43, BalanceConfig::default());
        let deadline = gs.date;
        gs.player_company.active_contracts.push(Contract {
            id: rocket_tycoon::contract::ContractId(9002),
            name: "Test Gov Science Contract".into(),
            destination: "leo".into(),
            payload_kg: 1000.0,
            payment: 1_000_000.0,
            deadline,
            status: ContractStatus::Accepted,
            market_id: MARKET_GOV_SCIENCE,
            campaign_id: None,
            bid_deadline: None,
            budget_ceiling: 0.0,
            player_bid: None,
        });
        gs.advance_day();

        let expected = -expiry_penalty * gov_science_severity;
        assert!(
            (gs.player_company.reputation.expiry_factor - expected).abs() < 1e-9,
            "expected expiry_factor {expected}, got {}",
            gs.player_company.reputation.expiry_factor,
        );
    }
}

#[test]
fn validation_rejects_bad_character_values() {
    let base = BalanceConfig::default().markets;
    let idx = base
        .archetypes
        .iter()
        .position(|a| a.key == "market_cots")
        .expect("market_cots archetype exists");

    // deadline_days.0 == 0 is invalid (must be >= 1).
    let mut config = base.clone();
    config.archetypes[idx].template.deadline_days = Some((0, 90));
    assert!(
        config.validate().is_err(),
        "expected validate() to reject deadline_days lo == 0",
    );

    // deadline_days reversed (lo > hi) is invalid.
    let mut config = base.clone();
    config.archetypes[idx].template.deadline_days = Some((200, 100));
    assert!(
        config.validate().is_err(),
        "expected validate() to reject reversed deadline_days",
    );

    // Negative failure_severity is invalid.
    let mut config = base.clone();
    config.archetypes[idx].template.failure_severity = -0.5;
    assert!(
        config.validate().is_err(),
        "expected validate() to reject negative failure_severity",
    );
}
