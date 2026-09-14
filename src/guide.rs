//! The guided start's persisted state (18_ONBOARDING.md step 4).
//!
//! A guided game carries which step of `ui::next_steps::STEPS` the
//! player is on and, once that step is achieved, which one is waiting
//! to be introduced. Both survive a save so a reload resumes the guide
//! where it was. `None` on `GameState::guide` means the player declined
//! the guide, stopped it, or graduated.

use serde::{Deserialize, Serialize};

/// One row of the step table; the guide's cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepId {
    FirstEngine,
    SecondEngine,
    DesignRocket,
    IdleTeams,
    HireManufacturing,
    OrderBuild,
    ReviseFlaws,
    PlaceBid,
    AwaitAward,
    LostBid,
    Launch,
    Graduate,
}

/// Where the guide is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideState {
    /// The step the player is working on.
    pub current: StepId,
    /// Set once `current` is achieved: the step whose popup is owed the
    /// moment no other modal is open.
    pub ready: Option<StepId>,
}

impl GuideState {
    /// A guide at its first step.
    pub fn start() -> Self {
        GuideState { current: StepId::FirstEngine, ready: None }
    }
}
