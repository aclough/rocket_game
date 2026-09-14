//! The guided start (18_ONBOARDING.md step 4): walks the guided steps
//! of `next_steps::STEPS`, popping each one up as the previous one is
//! achieved. The popup pauses the clock and leaves it paused when it
//! closes: it is asking the player to go and do something.
//!
//! Achievement is observed two ways, matching `Done`: the game state
//! (checked after every key and every tick) or the day's events (fed
//! in after a tick, or after a key handler that produced one). The
//! popup itself waits until the step is actionable and no other modal
//! is open; the last popup, graduation, ends the guide when closed.

use crate::event::GameEvent;
use crate::guide::{GuideState, StepId};
use crate::ui::next_steps::{step, Done};
use super::*;

/// The step after `id` on the guided path. `AwaitAward` and `LostBid`
/// branch on whether a win arrived; a loss loops back to waiting for
/// the next award; `Graduate` ends the guide.
pub fn next_after(id: StepId, won: bool) -> Option<StepId> {
    use StepId::*;
    Some(match id {
        FirstEngine => SecondEngine,
        SecondEngine => DesignRocket,
        DesignRocket => HireManufacturing,
        HireManufacturing => OrderBuild,
        OrderBuild => PlaceBid,
        PlaceBid => AwaitAward,
        AwaitAward => if won { Launch } else { LostBid },
        // Several bids can be open at once: a win arriving while the
        // player is being told about a loss goes straight to the launch.
        LostBid => if won { Launch } else { AwaitAward },
        Launch => Graduate,
        Graduate | IdleTeams | ReviseFlaws => return None,
    })
}

impl App {
    /// Whether this game is guided and where it is.
    fn guide(&self) -> Option<GuideState> {
        self.game.guide
    }

    /// Look at the game (and `events`, if any) and note whether the
    /// current step is achieved.
    pub fn guide_observe(&mut self, events: &[GameEvent]) {
        let Some(mut g) = self.guide() else { return };
        if g.ready.is_some() {
            return;
        }
        let achieved = match step(g.current).done {
            Done::State(f) => f(&self.game),
            Done::Event(f) => events.iter().any(f),
            Done::Ui => false,
        };
        if !achieved {
            return;
        }
        let won = events.iter().any(|e| matches!(e, GameEvent::ContractAwarded { .. }));
        g.ready = self.first_unmet(next_after(g.current, won));
        self.game.guide = Some(g);
        if g.ready.is_none() {
            // Nothing left to introduce: the path ended.
            self.game.guide = None;
        }
    }

    /// Skip past steps the player has already done out of order: a
    /// state-triggered step that already holds is not worth a popup.
    fn first_unmet(&self, mut next: Option<StepId>) -> Option<StepId> {
        while let Some(id) = next {
            match step(id).done {
                Done::State(f) if f(&self.game) => next = next_after(id, false),
                _ => break,
            }
        }
        next
    }

    /// Show the owed popup if nothing else is on screen. Pauses the
    /// clock; closing the popup leaves it paused.
    pub fn guide_maybe_popup(&mut self) {
        let Some(mut g) = self.guide() else { return };
        let Some(owed) = g.ready else { return };
        if !matches!(self.input_mode, InputMode::Normal) {
            return;
        }
        // The player may have done the owed step while the popup was
        // waiting its turn; skip what is already done.
        let Some(next) = self.first_unmet(Some(owed)) else {
            self.game.guide = None;
            return;
        };
        if !(step(next).ready)(&self.game) {
            g.ready = Some(next);
            self.game.guide = Some(g);
            return;
        }
        let achieved = g.current;
        g.current = next;
        g.ready = None;
        self.game.guide = Some(g);
        self.game.speed = GameSpeed::Paused;
        self.input_mode = InputMode::Guide { achieved: Some(achieved), next };
    }

    /// Run after every key: state-triggered steps may have just been
    /// done, and a popup may now have room to appear.
    pub(super) fn guide_after_input(&mut self) {
        self.guide_observe(&[]);
        self.guide_maybe_popup();
    }

    pub(super) fn handle_guide_key(&mut self, key: KeyCode, achieved: Option<StepId>, next: StepId) {
        match key {
            KeyCode::Esc => {
                self.input_mode = InputMode::GuideStop { achieved, next };
            }
            _ => {
                // The last step's popup is the graduation itself.
                if next == StepId::Graduate {
                    self.game.guide = None;
                }
                self.input_mode = InputMode::Normal;
            }
        }
    }

    pub(super) fn handle_guide_stop_key(&mut self, key: KeyCode, achieved: Option<StepId>, next: StepId) {
        match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.game.guide = None;
                self.input_mode = InputMode::Normal;
                self.status_message = Some("Guide stopped — Next steps stay on the Overview tab".into());
            }
            _ => {
                self.input_mode = InputMode::Guide { achieved, next };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{BasicPolicy, CompanyPolicy};

    fn guided_app(seed: u64) -> App {
        let mut game = GameState::new("Guided".into(), seed);
        game.guide = Some(GuideState::start());
        App::new_game(game)
    }

    /// Drive a guided game with the bot at the controls. Returns the
    /// sequence of popups (achieved → next) in the order they appeared.
    fn walk(seed: u64, max_days: u32) -> Vec<(Option<StepId>, StepId)> {
        let mut app = guided_app(seed);
        let mut policy = BasicPolicy::new();
        let mut popups = Vec::new();
        let mut days = 0;
        while app.game.guide.is_some() && days < max_days {
            if let InputMode::Guide { achieved, next } = app.input_mode {
                popups.push((achieved, next));
                app.handle_key(KeyCode::Enter);
                continue;
            }
            policy.act(&mut app.game);
            app.game.speed = GameSpeed::Normal;
            app.tick();
            days += 1;
        }
        popups
    }

    #[test]
    fn an_unguided_game_never_shows_the_guide() {
        let mut app = App::new_game(GameState::new("Plain".into(), 3));
        assert!(matches!(app.input_mode, InputMode::Intro));
        app.handle_key(KeyCode::Enter);
        let mut policy = BasicPolicy::new();
        for _ in 0..600 {
            policy.act(&mut app.game);
            app.game.speed = GameSpeed::Normal;
            app.tick();
            assert!(!matches!(app.input_mode, InputMode::Guide { .. }));
            if !matches!(app.input_mode, InputMode::Normal) {
                app.handle_key(KeyCode::Esc);
            }
        }
    }

    /// A guided game opens on the guide's first popup instead of the
    /// intro, and the bot then walks every step exactly once, in path
    /// order, with the award branch taken one way or the other.
    #[test]
    fn the_bot_walks_the_whole_guided_path() {
        let app = guided_app(1);
        assert!(matches!(app.input_mode, InputMode::Guide { achieved: None, next: StepId::FirstEngine }));

        let mut saw_loss = false;
        let mut saw_win = false;
        for seed in 1..=12u64 {
            let popups = walk(seed, 365 * 6);
            let nexts: Vec<StepId> = popups.iter().map(|(_, n)| *n).collect();
            assert_eq!(nexts.first(), Some(&StepId::FirstEngine), "seed {seed}: {nexts:?}");
            assert_eq!(nexts.last(), Some(&StepId::Graduate), "seed {seed}: the guide should finish, got {nexts:?}");
            // Path order (steps the bot did in one go are skipped, so a
            // popup may jump ahead), allowing the LostBid ↔ AwaitAward
            // loop; and each popup names the step just done.
            let order: Vec<StepId> = crate::ui::next_steps::guided_steps().map(|s| s.id).collect();
            let pos = |id: StepId| order.iter().position(|s| *s == id).unwrap();
            for w in popups.windows(2) {
                let (prev, this) = (w[0].1, w[1].1);
                assert_eq!(w[1].0, Some(prev), "seed {seed}: each popup names the step just done");
                let forward = pos(this) > pos(prev);
                let rebid = prev == StepId::LostBid && this == StepId::AwaitAward;
                assert!(forward || rebid, "seed {seed}: {prev:?} → {this:?} goes backwards");
            }
            let launches = nexts.iter().filter(|n| **n == StepId::Launch).count();
            assert_eq!(launches, 1, "seed {seed}: launch is introduced once");
            if nexts.contains(&StepId::LostBid) { saw_loss = true } else { saw_win = true }
        }
        assert!(saw_loss, "some seed should lose its first bid");
        assert!(saw_win, "some seed should win its first bid");
    }

    /// Steps done out of order are skipped, and a popup waits until its
    /// step is possible: two engines before the first popup closes means
    /// the next popup is the rocket — once both engines are out of design.
    #[test]
    fn steps_already_done_are_skipped_and_popups_wait_to_be_actionable() {
        let mut app = guided_app(4);
        for name in ["B1", "B2"] {
            app.game.player_company.start_engine_project(
                name.into(), EngineCycle::GasGenerator, PropellantPreset::Kerolox,
                1.0, None, &app.game.balance,
            );
        }
        app.handle_key(KeyCode::Enter); // close the first popup
        assert!(matches!(app.input_mode, InputMode::Normal), "no popup while the engines are in design");
        assert_eq!(app.game.guide.and_then(|g| g.ready), Some(StepId::DesignRocket),
            "the rocket step is owed, not the second engine");

        for ep in app.game.player_company.engine_projects.iter_mut() {
            ep.status = EngineDesignStatus::Testing { work_completed: 0.0 };
        }
        app.handle_key(KeyCode::Char(' ')); // any key: the state check runs
        assert!(matches!(app.input_mode, InputMode::Guide {
            achieved: Some(StepId::FirstEngine), next: StepId::DesignRocket,
        }), "got {:?}", app.input_mode);
        assert_eq!(app.game.speed, GameSpeed::Paused, "the popup leaves the clock paused");
    }

    /// Ordering a rocket is not the same as having one: the bid step
    /// is introduced when the rocket is on the shelf.
    #[test]
    fn the_bid_step_waits_for_a_built_rocket() {
        let mut app = guided_app(8);
        app.input_mode = InputMode::Normal;
        app.game.guide = Some(GuideState { current: StepId::OrderBuild, ready: None });
        app.game.player_company.manufacturing.orders.push(
            crate::manufacturing::ManufacturingOrder::new_integration(
                crate::manufacturing::ManufacturingOrderId(1),
                crate::rocket_project::RocketProjectId(1), crate::rocket::RocketDesignId(1),
                "Ordered".into(), 2, 0, 0, Vec::new(), &app.game.balance,
            ),
        );
        app.handle_key(KeyCode::Char(' '));
        assert!(matches!(app.input_mode, InputMode::Normal), "an order alone introduces nothing");

        app.game.player_company.manufacturing.inventory.rockets.push(
            crate::manufacturing::InventoryRocket {
                item_id: crate::manufacturing::InventoryItemId(1),
                rocket_project_id: crate::rocket_project::RocketProjectId(1),
                design_id: crate::rocket::RocketDesignId(1),
                rocket_name: "Built".into(),
                build_cost: 1.0e7, revision: 0, rocket_flaws: Vec::new(),
            },
        );
        app.handle_key(KeyCode::Char(' '));
        assert!(matches!(app.input_mode, InputMode::Guide {
            achieved: Some(StepId::OrderBuild), next: StepId::PlaceBid,
        }), "got {:?}", app.input_mode);
    }

    /// Esc asks first; Y stops the guide, anything else goes back.
    #[test]
    fn esc_asks_before_stopping() {
        let mut app = guided_app(5);
        app.handle_key(KeyCode::Esc);
        assert!(matches!(app.input_mode, InputMode::GuideStop { .. }));
        app.handle_key(KeyCode::Char('n'));
        assert!(matches!(app.input_mode, InputMode::Guide { .. }));
        app.handle_key(KeyCode::Esc);
        app.handle_key(KeyCode::Char('y'));
        assert!(app.game.guide.is_none());
        assert!(matches!(app.input_mode, InputMode::Normal));
    }

    /// The guide's place survives a save.
    #[test]
    fn the_guide_survives_a_round_trip() {
        let mut game = GameState::new("Saved".into(), 6);
        game.guide = Some(GuideState { current: StepId::PlaceBid, ready: Some(StepId::AwaitAward) });
        let json = serde_json::to_string(&game).unwrap();
        let back: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.guide, game.guide);
    }
}
