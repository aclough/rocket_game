//! The Overview tab's "Next steps" panel.
//!
//! A data-driven rule list evaluated against `GameState`. The first few
//! unsatisfied rules render, each naming the tab and key to act on.
//!
//! The rules mirror the sequence `BasicPolicy` follows, because that
//! sequence is *tested* to reach orbit (`policy.rs` asserts the bot
//! launches within four years). The advice is therefore known-good
//! rather than aspirational.
//!
//! It is advisory only: nothing here gates progress or grabs focus, and
//! the panel goes quiet once the company is running. A player who wants
//! to ignore it and do something else entirely is not obstructed —
//! which is deliberate for an audience that will resent being walked.

use crate::engine_project::EngineDesignStatus;
use crate::game_state::GameState;
use crate::rocket_project::RocketDesignStatus;

/// One suggestion. `tab` and `key` tell the player where to go; both
/// are shown verbatim, so they must match the real keybindings in
/// `keys.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextStep {
    pub text: String,
    pub tab: &'static str,
    pub key: &'static str,
}

/// How many suggestions to show at once. More than this reads as a
/// checklist to grind rather than a nudge.
pub const MAX_SHOWN: usize = 3;

/// Evaluate the rules against the current game, most important first.
/// An empty result means the company is past the opening and the panel
/// should disappear.
pub fn next_steps(game: &GameState) -> Vec<NextStep> {
    let c = &game.player_company;
    let mut steps = Vec::new();

    let step = |text: String, tab: &'static str, key: &'static str| NextStep {
        text, tab, key,
    };

    // --- The opening sequence, in dependency order. ---

    // A rocket needs two engines, and nothing in the UI says so. This
    // is the single least guessable step in the game.
    let committed_engines = c.engine_projects.iter()
        .filter(|p| !matches!(p.status, EngineDesignStatus::Proposed { .. }))
        .count();
    if committed_engines == 0 {
        steps.push(step(
            "Design your first engine — a sea-level booster".into(),
            "Engines", "N",
        ));
    } else if committed_engines == 1 && c.rocket_projects.is_empty() {
        steps.push(step(
            "Design a second engine for the upper stage \
             (a different propellant flies higher)".into(),
            "Engines", "N",
        ));
    }

    // Engines exist but no vehicle to put them in.
    if committed_engines >= 1 && c.rocket_projects.is_empty() {
        steps.push(step(
            "Design a rocket around your engines".into(),
            "Rockets", "N",
        ));
    }

    // Idle engineering capacity with nothing to absorb it. Teams are
    // auto-assigned now, so this only fires when there is genuinely no
    // project to work on.
    if c.unassigned_team_count() > 0 && committed_engines > 0 {
        steps.push(step(
            format!("{} engineering team(s) idle — start another project \
                     or they're drawing salary for nothing",
                c.unassigned_team_count()),
            "Engines", "N",
        ));
    }

    // Manufacturing is a hidden prerequisite: a rocket in Testing can't
    // be built without a team, and nothing says so until you try. Only
    // raised once a design is close, so the player isn't paying idle
    // manufacturing salaries through the whole design phase.
    let design_ready = c.rocket_projects.iter().any(|p| matches!(
        p.status, RocketDesignStatus::Testing { .. } | RocketDesignStatus::Revising { .. },
    ));
    if design_ready && c.manufacturing_teams.is_empty() {
        steps.push(step(
            "Hire a manufacturing team — your design is ready to build".into(),
            "Mfg", "M",
        ));
    }

    // A buildable design and nothing on the shelf.
    let has_stock = !c.manufacturing.inventory.rockets.is_empty();
    let building = !c.manufacturing.orders.is_empty();
    if design_ready && !c.manufacturing_teams.is_empty() && !has_stock && !building {
        steps.push(step(
            "Order a rocket build, or set an auto-build target".into(),
            "Rockets", "O",
        ));
    }

    // Flying an unrevised design nearly always ends in a fireball, and
    // the game never says so. Only worth raising while flaws are known
    // and unfixed — auto-revise handles this for most players, so this
    // fires mainly for someone who turned it off.
    let unrevised = c.rocket_projects.iter().any(|p|
        matches!(p.status, RocketDesignStatus::Testing { .. })
        && p.discovered_flaw_count() > 0
        && !p.auto_revise
    );
    if unrevised {
        steps.push(step(
            "Revise the flaws found in testing before you fly".into(),
            "Rockets", "R",
        ));
    }

    // Vehicle on the shelf, no work booked.
    if has_stock && c.active_contracts.is_empty() && c.launch_history.is_empty() {
        steps.push(step(
            "Bid on a contract — or fly a test mass to prove the vehicle".into(),
            "Contracts", "B",
        ));
    }

    // Everything is in place: fly.
    if has_stock && !c.active_contracts.is_empty() {
        steps.push(step(
            "Launch — you have a vehicle and a customer waiting".into(),
            "Launches", "L",
        ));
    }

    steps.truncate(MAX_SHOWN);
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{BasicPolicy, CompanyPolicy};

    fn new_game() -> GameState {
        GameState::new("Steps Test".into(), 200_000_000.0, 7)
    }

    #[test]
    fn a_brand_new_company_is_told_to_design_an_engine() {
        let steps = next_steps(&new_game());
        assert!(!steps.is_empty(), "a new company needs guidance");
        assert!(steps[0].text.contains("first engine"),
            "the first suggestion should be the first real action, got {:?}", steps[0]);
        assert_eq!(steps[0].tab, "Engines");
    }

    #[test]
    fn never_shows_more_than_the_cap() {
        let mut game = new_game();
        let mut policy = BasicPolicy::new();
        for _ in 0..400 {
            policy.act(&mut game);
            game.advance_day();
            assert!(next_steps(&game).len() <= MAX_SHOWN);
        }
    }

    /// Following the advice must actually retire it — a panel that
    /// keeps asking for something you've done is worse than none.
    #[test]
    fn the_engine_suggestion_clears_once_an_engine_exists() {
        let mut game = new_game();
        let before = next_steps(&game);
        assert!(before[0].text.contains("first engine"));

        game.player_company.start_engine_project(
            "Booster".into(),
            crate::engine::EngineCycle::GasGenerator,
            crate::engine_project::PropellantPreset::Kerolox,
            1.0, None, &game.balance,
        );

        let after = next_steps(&game);
        assert!(
            !after.iter().any(|s| s.text.contains("first engine")),
            "the first-engine step should retire, got {after:?}",
        );
    }

    /// The panel is for the opening. A company that is flying should
    /// not still be lectured.
    #[test]
    fn goes_quiet_once_the_company_is_running() {
        let mut game = new_game();
        let mut policy = BasicPolicy::new();
        // Long enough for BasicPolicy to be launching regularly.
        for _ in 0..(365 * 4) {
            policy.act(&mut game);
            game.advance_day();
        }
        assert!(!game.player_company.launch_history.is_empty(),
            "precondition: the bot should be flying by year 4");

        let steps = next_steps(&game);
        assert!(
            steps.iter().all(|s| !s.text.contains("first engine")
                && !s.text.contains("Design a rocket")),
            "opening advice should be long gone, got {steps:?}",
        );
    }

    /// Every suggestion points at a real tab.
    #[test]
    fn suggestions_name_real_tabs() {
        let mut game = new_game();
        let mut policy = BasicPolicy::new();
        let valid: Vec<&str> = crate::ui::Tab::ALL.iter().map(|t| t.name()).collect();
        for _ in 0..600 {
            policy.act(&mut game);
            game.advance_day();
            for s in next_steps(&game) {
                assert!(valid.contains(&s.tab),
                    "{:?} names a tab that doesn't exist", s);
                assert!(!s.key.is_empty(), "{s:?} has no key");
            }
        }
    }
}
