//! Scripted company policies — bots that play the game headlessly.
//!
//! Used by the `simulate` binary for tuning runs, and later as the
//! brains of AI competitors (DinoSoar). A policy is called once per
//! day, before `GameState::advance_day`, and acts through the same
//! public `GameState`/`Company` methods the UI uses.
//!
//! Policies must be deterministic: index-ordered choices only, no
//! wall-clock, no HashMap-iteration-order dependence. A fixed seed +
//! a fixed policy must always produce an identical run.

use crate::game_state::GameState;

pub trait CompanyPolicy {
    /// Take today's actions. Called once per day before `advance_day`.
    fn act(&mut self, game: &mut GameState);

    /// Name for CLI selection and reporting.
    fn name(&self) -> &'static str;
}

/// Takes no actions at all — the "do nothing" baseline. Useful for
/// measuring pure salary burn and for exercising the sim harness
/// before real policies exist.
pub struct NullPolicy;

impl CompanyPolicy for NullPolicy {
    fn act(&mut self, _game: &mut GameState) {}

    fn name(&self) -> &'static str {
        "none"
    }
}

/// Look up a policy by CLI name.
pub fn policy_by_name(name: &str) -> Option<Box<dyn CompanyPolicy>> {
    match name {
        "none" => Some(Box::new(NullPolicy)),
        _ => None,
    }
}

/// Names accepted by `policy_by_name`, for CLI help text.
pub const POLICY_NAMES: &[&str] = &["none"];
