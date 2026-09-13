//! What the Place Bid modal is told (`GameState::bid_basis`): the
//! cheapest capable design, its marginal cost and where that figure
//! comes from, the standing margin, and the suggested price — the same
//! price the rule engine would bid.

use rocket_tycoon::contract::{round_price, MARKET_RIDESHARE};
use rocket_tycoon::game_state::{BidRule, CostBasis, GameState};

mod common;
use common::{game_with_capable_player, inject_contract};

#[test]
fn nothing_capable_means_no_rocket_no_cost_no_suggestion() {
    let mut gs = GameState::new("Basis".into(), 1);
    let idx = inject_contract(&mut gs, 900, "Basis-0", MARKET_RIDESHARE);
    let basis = gs.bid_basis(idx);
    assert_eq!(basis.rocket, None);
    assert_eq!(basis.cost, None);
    assert_eq!(basis.suggested, None);
    assert_eq!(basis.margin, BidRule::default().margin, "the default margin still shows");
}

#[test]
fn an_unbuilt_design_is_priced_from_its_material_estimate() {
    let mut gs = game_with_capable_player(2);
    let idx = inject_contract(&mut gs, 901, "Basis-1", MARKET_RIDESHARE);
    let rp = gs.player_company.rocket_projects[0].clone();
    let estimate = gs.player_company.estimated_build_cost(&rp, &gs.balance);

    let basis = gs.bid_basis(idx);
    assert_eq!(basis.rocket.as_deref(), Some(rp.design.name.as_str()));
    assert_eq!(basis.cost, Some((estimate, CostBasis::MaterialEstimate)));
    assert_eq!(basis.suggested, Some(round_price(estimate * (1.0 + basis.margin))));
}

#[test]
fn a_built_design_is_priced_from_its_last_five_builds_at_the_rule_margin() {
    let mut gs = game_with_capable_player(3);
    let idx = inject_contract(&mut gs, 902, "Basis-2", MARKET_RIDESHARE);
    let design_id = gs.player_company.rocket_projects[0].design.id;
    gs.player_company.rocket_cost_history.insert(
        design_id, vec![99.0e6, 10.0e6, 11.0e6, 12.0e6, 13.0e6, 14.0e6],
    );
    gs.player_company.bid_rules.insert(MARKET_RIDESHARE, BidRule { enabled: false, margin: 0.4 });

    let basis = gs.bid_basis(idx);
    let mean = 12.0e6;
    assert_eq!(basis.cost, Some((mean, CostBasis::BuildHistory)), "the 99 is outside the last five");
    assert_eq!(basis.margin, 0.4, "a rule's margin applies even when the rule is off");
    assert_eq!(basis.suggested, Some(round_price(mean * 1.4)));
}
