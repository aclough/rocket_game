//! The step table: what a new company should do next, in order, with
//! a paragraph of explanation for each step (18_ONBOARDING.md).
//!
//! Two readers share it. The Overview tab's "Next steps" panel shows
//! the first few steps that `applies` to the current game — advisory
//! only, nothing gates on it, and it goes quiet once the company is
//! running. The guided start walks the `guided` steps in order and
//! pops each one's `explain` up when the previous one is `done`.
//! One table means the two cannot drift.
//!
//! The order mirrors the sequence `BasicPolicy` follows, because that
//! sequence is *tested* to reach orbit (`policy.rs` asserts the bot
//! launches within four years). The advice is therefore known-good
//! rather than aspirational. Every `tab` and `key` is shown verbatim,
//! so they must match `keys.rs`.

use crate::contract::AwardOutcome;
use crate::event::GameEvent;
use crate::game_state::GameState;
use crate::rocket_project::RocketDesignStatus;

/// One row of the table; the guide's cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum StepId {
    FirstEngine,
    SecondEngine,
    DesignRocket,
    IdleTeams,
    HireManufacturing,
    OrderBuild,
    ReviseFlaws,
    PickSolicitation,
    PlaceBid,
    AwaitAward,
    LostBid,
    Launch,
    ReadOutcome,
    Graduate,
}

/// How the guide knows a step is achieved.
#[derive(Clone, Copy)]
pub enum Done {
    /// The game state says so.
    State(fn(&GameState) -> bool),
    /// One of the day's events says so.
    Event(fn(&GameEvent) -> bool),
    /// The UI reports it (a modal opened, a key pressed on a result).
    Ui,
}

pub struct Step {
    pub id: StepId,
    /// The Overview one-liner.
    pub text: fn(&GameState) -> String,
    pub tab: &'static str,
    pub key: &'static str,
    /// The guide's paragraph; every line fits a 68-column modal.
    pub explain: &'static [&'static str],
    /// Worth showing on the Overview now.
    pub applies: fn(&GameState) -> bool,
    /// Part of the guided path (the rest are nags the panel raises
    /// when they come up).
    pub guided: bool,
    pub done: Done,
}

/// How many suggestions the Overview shows at once. More than this
/// reads as a checklist to grind rather than a nudge.
pub const MAX_SHOWN: usize = 3;

/// The facts every rule reads.
struct Facts {
    committed_engines: usize,
    live_rockets: bool,
    design_ready: bool,
    has_stock: bool,
    building: bool,
    active_contracts: bool,
    flown: usize,
    pending_bid: Option<(String, crate::calendar::GameDate)>,
    lost_last: bool,
}

fn facts(game: &GameState) -> Facts {
    let c = &game.player_company;
    let live: Vec<_> = c.visible_rocket_projects().map(|(_, rp)| rp).collect();
    let design_ready = live.iter().any(|p| matches!(
        p.status, RocketDesignStatus::Testing { .. } | RocketDesignStatus::Revising { .. },
    ));
    let pending_bid = game.available_contracts.iter()
        .find(|ct| ct.player_bid.is_some())
        .and_then(|ct| ct.bid_deadline.map(|d| (ct.name.clone(), d)));
    // The most recent award the player took part in was a loss.
    let lost_last = game.award_history.iter().rev()
        .find(|r| matches!(
            r.outcome,
            AwardOutcome::PlayerWon { .. }
            | AwardOutcome::PlayerRejected { .. }
            | AwardOutcome::CompetitorWon { player_bid: Some(_), .. }
        ))
        .is_some_and(|r| !matches!(r.outcome, AwardOutcome::PlayerWon { .. }));
    Facts {
        committed_engines: c.visible_engine_projects().count(),
        live_rockets: !live.is_empty(),
        design_ready,
        has_stock: !c.manufacturing.inventory.rockets.is_empty(),
        building: !c.manufacturing.orders.is_empty(),
        active_contracts: !c.active_contracts.is_empty(),
        flown: c.launch_history.len(),
        pending_bid,
        lost_last,
    }
}

/// The Overview one-liner for each step. Kept in one match so a step's
/// text and its rule sit in the same file, with only the two dynamic
/// ones reading the game.
fn text_of(id: StepId, game: &GameState) -> String {
    let f = facts(game);
    match id {
        StepId::FirstEngine => "Design your first engine — a sea-level booster".into(),
        StepId::SecondEngine =>
            "Design a second engine for the upper stage (a different propellant flies higher)".into(),
        StepId::DesignRocket => "Design a rocket around your engines".into(),
        StepId::IdleTeams => format!(
            "{} engineering team(s) idle — start another project or they're drawing salary for nothing",
            game.player_company.unassigned_team_count(),
        ),
        StepId::HireManufacturing => "Hire a manufacturing team — your design is ready to build".into(),
        StepId::OrderBuild => "Order a rocket build, or set an auto-build target".into(),
        StepId::ReviseFlaws => "Revise the flaws found in testing before you fly".into(),
        StepId::PickSolicitation =>
            "Bid on a contract — or fly a test mass to prove the vehicle".into(),
        StepId::PlaceBid => "Bid a little above your marginal cost".into(),
        StepId::AwaitAward => match f.pending_bid {
            Some((name, date)) => format!("Your bid on {name} resolves {date} — let the clock run"),
            None => "Wait for the bid date".into(),
        },
        StepId::LostBid => "You were outbid — read Award History and bid again nearer the price".into(),
        StepId::Launch => "Launch — you have a vehicle and a customer waiting".into(),
        StepId::ReadOutcome => "Read the launch result".into(),
        StepId::Graduate => "Set standing bid rules so bidding runs itself".into(),
    }
}

macro_rules! text {
    ($id:expr) => { |g: &GameState| text_of($id, g) };
}

pub const STEPS: &[Step] = &[
    // --- The opening sequence, in dependency order. ---
    Step {
        id: StepId::FirstEngine,
        text: text!(StepId::FirstEngine),
        tab: "Engines", key: "N",
        // A rocket needs two engines, and nothing in the UI says so.
        // This is the single least guessable step in the game.
        explain: &[
            "Every rocket starts with an engine you design. Open the",
            "Engines tab and press N. The default is a kerolox gas",
            "generator, a good sea-level booster. Name it; the scale",
            "sets its thrust. Design takes months, and the engine",
            "arrives with hidden flaws. That is normal.",
        ],
        applies: |g| facts(g).committed_engines == 0,
        guided: true,
        done: Done::State(|g| facts(g).committed_engines >= 1),
    },
    Step {
        id: StepId::SecondEngine,
        text: text!(StepId::SecondEngine),
        tab: "Engines", key: "N",
        explain: &[
            "A rocket needs two stages, and the upper stage wants an",
            "engine of its own: a different propellant (hydrolox)",
            "flies higher for the same mass. Press N again on the",
            "Engines tab. Your team splits between the two projects,",
            "so hire a second one (E) if you can afford it.",
        ],
        applies: |g| { let f = facts(g); f.committed_engines == 1 && !f.live_rockets },
        guided: true,
        done: Done::State(|g| { let f = facts(g); f.committed_engines >= 2 || f.live_rockets }),
    },
    Step {
        id: StepId::DesignRocket,
        text: text!(StepId::DesignRocket),
        tab: "Rockets", key: "N",
        explain: &[
            "On the Rockets tab press N to open the designer. Add a",
            "stage with A and pick your booster; add another for the",
            "upper stage. The right pane shows what the stack lifts",
            "and to where. Set a payload (P) and destination (M) to",
            "see whether it reaches orbit, then press D when it does.",
        ],
        applies: |g| { let f = facts(g); f.committed_engines >= 1 && !f.live_rockets },
        guided: true,
        done: Done::State(|g| facts(g).live_rockets),
    },
    // Idle engineering capacity with nothing to absorb it. Teams are
    // auto-assigned, so this only fires when there is genuinely no
    // project to work on.
    Step {
        id: StepId::IdleTeams,
        text: text!(StepId::IdleTeams),
        tab: "Engines", key: "N",
        explain: &[],
        applies: |g| g.player_company.unassigned_team_count() > 0 && facts(g).committed_engines > 0,
        guided: false,
        done: Done::State(|g| g.player_company.unassigned_team_count() == 0),
    },
    // Manufacturing is a hidden prerequisite: a rocket in Testing can't
    // be built without a team, and nothing says so until you try. Only
    // raised once a design is close, so the player isn't paying idle
    // manufacturing salaries through the whole design phase.
    Step {
        id: StepId::HireManufacturing,
        text: text!(StepId::HireManufacturing),
        tab: "Mfg", key: "M",
        explain: &[
            "A design in Testing can be built, but only by a",
            "manufacturing team; engineers do not build rockets.",
            "On the Mfg tab press M to hire one. Teams draw a salary",
            "whether or not they are working, which is why this waits",
            "until you have something to build.",
        ],
        applies: |g| facts(g).design_ready && g.player_company.manufacturing_teams.is_empty(),
        guided: true,
        done: Done::State(|g| !g.player_company.manufacturing_teams.is_empty()),
    },
    Step {
        id: StepId::OrderBuild,
        text: text!(StepId::OrderBuild),
        tab: "Rockets", key: "O",
        explain: &[
            "On the Rockets tab press O to order one rocket. The floor",
            "builds engines and stages in parallel, then integrates",
            "them; the Mfg tab shows progress. A rocket in inventory",
            "is what you bid with, and what it cost to build is your",
            "marginal cost: the floor under every bid you make.",
        ],
        applies: |g| {
            let f = facts(g);
            f.design_ready && !g.player_company.manufacturing_teams.is_empty()
                && !f.has_stock && !f.building
        },
        guided: true,
        done: Done::State(|g| { let f = facts(g); f.has_stock || f.building }),
    },
    // Flying an unrevised design nearly always ends in a fireball, and
    // the game never says so. Only worth raising while flaws are known
    // and unfixed; auto-revise handles this for most players, so this
    // fires mainly for someone who turned it off.
    Step {
        id: StepId::ReviseFlaws,
        text: text!(StepId::ReviseFlaws),
        tab: "Rockets", key: "R",
        explain: &[],
        applies: |g| g.player_company.visible_rocket_projects().any(|(_, p)|
            matches!(p.status, RocketDesignStatus::Testing { .. })
            && p.discovered_flaw_count() > 0
            && !p.auto_revise
        ),
        guided: false,
        done: Done::State(|g| !g.player_company.visible_rocket_projects().any(|(_, p)|
            matches!(p.status, RocketDesignStatus::Testing { .. })
            && p.discovered_flaw_count() > 0
            && !p.auto_revise
        )),
    },
    // --- The bidding arc. ---
    Step {
        id: StepId::PickSolicitation,
        text: text!(StepId::PickSolicitation),
        tab: "Contracts", key: "B",
        explain: &[
            "Customers post solicitations on the Contracts tab: a",
            "payload, a destination, and the date bids close. Rows",
            "drawn white are ones a rocket of yours can lift. Pick",
            "one and press B. Nothing is committed until you submit.",
        ],
        applies: |g| {
            let f = facts(g);
            f.has_stock && !f.active_contracts && f.flown == 0
                && f.pending_bid.is_none() && !f.lost_last
        },
        guided: true,
        done: Done::Ui,
    },
    Step {
        id: StepId::PlaceBid,
        text: text!(StepId::PlaceBid),
        tab: "Contracts", key: "Enter",
        explain: &[
            "The bid is sealed: the customer's budget is hidden and a",
            "competitor is bidding too. The modal shows your marginal",
            "cost and a suggested price above it. Bid too high and",
            "you lose the work; too low and you fly at a loss. Press",
            "Enter to submit.",
        ],
        applies: |_| false,
        guided: true,
        done: Done::Event(|e| matches!(e, GameEvent::BidPlaced { .. })),
    },
    Step {
        id: StepId::AwaitAward,
        text: text!(StepId::AwaitAward),
        tab: "Overview", key: "Space",
        explain: &[
            "Bids resolve on the closing date. Let the clock run",
            "(Space) and watch the event feed. You win, or you are",
            "outbid at a price you get to see, or you were over the",
            "customer's budget, which is then disclosed.",
        ],
        applies: |g| { let f = facts(g); f.pending_bid.is_some() && !f.active_contracts },
        guided: true,
        done: Done::Event(|e| matches!(
            e,
            GameEvent::ContractAwarded { .. }
            | GameEvent::ContractAwardedToCompetitor { player_bid: Some(_), .. }
            | GameEvent::BidRejected { .. }
        )),
    },
    // The branch: taken only when AwaitAward resolved as a loss.
    Step {
        id: StepId::LostBid,
        text: text!(StepId::LostBid),
        tab: "Contracts", key: "H",
        explain: &[
            "You lost. On the Contracts tab press H for Award History:",
            "every award, the winning price, and your bid beside it.",
            "That price is the market talking. Find the next",
            "solicitation you can lift and bid nearer it.",
        ],
        applies: |g| {
            let f = facts(g);
            f.has_stock && !f.active_contracts && f.flown == 0
                && f.pending_bid.is_none() && f.lost_last
        },
        guided: true,
        done: Done::Event(|e| matches!(e, GameEvent::BidPlaced { .. })),
    },
    Step {
        id: StepId::Launch,
        text: text!(StepId::Launch),
        tab: "Launches", key: "L",
        explain: &[
            "You have a contract and a rocket. On the Launches tab",
            "select the rocket and press L; the manifest lets you",
            "pick the contract it flies. The flight resolves at once,",
            "and every hidden flaw gets its chance to fire.",
        ],
        applies: |g| { let f = facts(g); f.has_stock && f.active_contracts },
        guided: true,
        done: Done::Event(|e| matches!(
            e,
            GameEvent::LaunchSuccess { .. }
            | GameEvent::LaunchPartialFailure { .. }
            | GameEvent::LaunchFailure { .. }
        )),
    },
    Step {
        id: StepId::ReadOutcome,
        text: text!(StepId::ReadOutcome),
        tab: "Launches", key: "Enter",
        explain: &[
            "Read the result. A success pays on arrival; a failure",
            "names the flaw that caused it, which testing and a",
            "revision (R on the Rockets tab) fix before the next",
            "flight. Either way you have flown a customer's payload,",
            "and reputation follows.",
        ],
        applies: |_| false,
        guided: true,
        done: Done::Ui,
    },
    Step {
        id: StepId::Graduate,
        text: text!(StepId::Graduate),
        tab: "Contracts", key: "R",
        explain: &[
            "That is the loop. Two things do it for you from now on:",
            "standing bid rules (R on Contracts) bid marginal cost",
            "plus a margin on every solicitation you can fly, and an",
            "auto-build target (m on Rockets) keeps that many rockets",
            "on the shelf. Programs will announce themselves.",
            "Good luck.",
        ],
        // A nudge after the first flights, not a standing nag: a
        // player who bids by hand on purpose stops seeing it.
        applies: |g| {
            let f = facts(g);
            (1..=2).contains(&f.flown)
                && g.player_company.bid_rules.is_empty()
                && g.player_company.auto_build_targets.is_empty()
        },
        guided: true,
        done: Done::Ui,
    },
];

/// The step with this id.
pub fn step(id: StepId) -> &'static Step {
    STEPS.iter().find(|s| s.id == id).expect("every StepId has a row")
}

/// The steps the Overview panel shows now, most important first; empty
/// once the company is past the opening.
pub fn next_steps(game: &GameState) -> Vec<&'static Step> {
    STEPS.iter()
        .filter(|s| (s.applies)(game))
        .take(MAX_SHOWN)
        .collect()
}

/// The guided path, in order.
pub fn guided_steps() -> impl Iterator<Item = &'static Step> {
    STEPS.iter().filter(|s| s.guided)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{BasicPolicy, CompanyPolicy};

    fn new_game() -> GameState {
        GameState::new("Steps Test".into(), 7)
    }

    fn texts(game: &GameState) -> Vec<String> {
        next_steps(game).iter().map(|s| (s.text)(game)).collect()
    }

    #[test]
    fn a_brand_new_company_is_told_to_design_an_engine() {
        let game = new_game();
        let steps = next_steps(&game);
        assert!(!steps.is_empty(), "a new company needs guidance");
        assert!((steps[0].text)(&game).contains("first engine"),
            "the first suggestion should be the first real action, got {:?}", texts(&game));
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
        assert!(texts(&game)[0].contains("first engine"));

        game.player_company.start_engine_project(
            "Booster".into(),
            crate::engine::EngineCycle::GasGenerator,
            crate::engine_project::PropellantPreset::Kerolox,
            1.0, None, &game.balance,
        );

        let after = texts(&game);
        assert!(
            !after.iter().any(|s| s.contains("first engine")),
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

        let steps = texts(&game);
        assert!(
            steps.iter().all(|s| !s.contains("first engine") && !s.contains("Design a rocket")),
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
                assert!(valid.contains(&s.tab), "{:?} names a tab that doesn't exist", s.id);
                assert!(!s.key.is_empty(), "{:?} has no key", s.id);
            }
        }
    }

    /// Every guided step explains itself, and every line of every
    /// explanation fits the guide's modal.
    #[test]
    fn every_guided_step_has_an_explanation_that_fits() {
        for s in guided_steps() {
            assert!(!s.explain.is_empty(), "{:?} has no explanation", s.id);
            for line in s.explain {
                assert!(line.chars().count() <= 60, "{:?}: {line:?} is too long", s.id);
            }
        }
    }

    /// The guided path runs from the first engine to graduation, with
    /// the bidding arc in the middle and the nags left out.
    #[test]
    fn the_guided_path_is_the_opening_in_order() {
        let ids: Vec<StepId> = guided_steps().map(|s| s.id).collect();
        assert_eq!(ids, vec![
            StepId::FirstEngine, StepId::SecondEngine, StepId::DesignRocket,
            StepId::HireManufacturing, StepId::OrderBuild, StepId::PickSolicitation,
            StepId::PlaceBid, StepId::AwaitAward, StepId::LostBid, StepId::Launch,
            StepId::ReadOutcome, StepId::Graduate,
        ]);
        for s in STEPS {
            assert_eq!(step(s.id).id, s.id);
        }
    }

    /// A pending bid shows up on the Overview with its closing date,
    /// and a lost one points at the award history.
    #[test]
    fn the_bidding_arc_shows_on_the_overview() {
        let mut game = new_game();
        // A rocket on the shelf and nothing else going on.
        game.player_company.manufacturing.inventory.rockets.push(
            crate::manufacturing::InventoryRocket {
                item_id: crate::manufacturing::InventoryItemId(1),
                rocket_project_id: crate::rocket_project::RocketProjectId(1),
                design_id: crate::rocket::RocketDesignId(1),
                rocket_name: "Shelf".into(),
                build_cost: 1.0e7, revision: 0, rocket_flaws: Vec::new(),
            },
        );
        assert!(texts(&game).iter().any(|t| t.contains("Bid on a contract")));

        let idx = game.available_contracts.len();
        game.available_contracts.push(crate::contract::Contract {
            id: crate::contract::ContractId(9),
            name: "Pending-1".into(),
            destination: "leo".into(),
            payload_kg: 500.0, payment: 0.0,
            deadline: game.date.add_days(300),
            status: crate::contract::ContractStatus::Available,
            market_id: crate::contract::MARKET_RIDESHARE,
            campaign_id: None,
            bid_deadline: Some(game.date.add_days(5)),
            budget_ceiling: 5.0e7,
            player_bid: None,
        });
        game.place_bid(idx, 2.0e7);
        let t = texts(&game);
        assert!(t.iter().any(|t| t.contains("Your bid on Pending-1 resolves")), "{t:?}");
        assert!(!t.iter().any(|t| t.contains("Bid on a contract")), "{t:?}");

        game.available_contracts.clear();
        game.award_history.push(crate::contract::AwardRecord {
            date: game.date, market_id: crate::contract::MARKET_RIDESHARE,
            contract_name: "Pending-1".into(), destination: "leo".into(),
            payload_kg: 500.0, missions: None,
            outcome: AwardOutcome::CompetitorWon {
                company: "DinoSoar".into(), amount: 1.5e7, player_bid: Some(2.0e7),
            },
        });
        let t = texts(&game);
        assert!(t.iter().any(|t| t.contains("outbid")), "{t:?}");
    }
}
