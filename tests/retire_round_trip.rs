//! Retirement has to survive a reload — the flag is the whole feature,
//! so losing it on load would quietly un-retire everything.
//!
//! The *other* half of save safety — that a save written before the
//! field existed still loads — is covered better by `save_compat.rs`,
//! whose corpus of real pre-feature saves fails outright if the
//! `#[serde(default)]` is ever dropped. A hand-stripped save here would
//! only prove serde ignores unknown fields.

#[test]
fn retired_survives_a_save_round_trip() {
    use rocket_tycoon::company::RetireTarget;
    use rocket_tycoon::game_state::GameState;

    let mut gs = GameState::new("RoundTrip".into(), 200_000_000.0, 3);
    gs.player_company.start_engine_project(
        "Old Faithful".into(),
        rocket_tycoon::engine::EngineCycle::GasGenerator,
        rocket_tycoon::engine_project::PropellantPreset::Kerolox,
        1.0, None, &gs.balance,
    );
    let id = gs.player_company.engine_projects[0].project_id;
    gs.player_company.retire(RetireTarget::Engine(id)).expect("retires");

    let json = serde_json::to_string(&gs).expect("serializes");
    let back: GameState = serde_json::from_str(&json).expect("deserializes");

    assert!(back.player_company.engine_projects[0].retired,
        "a retired design must still be retired after a reload");
    assert_eq!(back.player_company.visible_engine_projects().count(), 0);
}
