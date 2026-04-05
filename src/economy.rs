use rand::Rng;
use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;
use crate::seed::GameSeed;

/// Economic conditions affecting the space launch market.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EconomicCondition {
    Boom,
    Normal,
    Slowdown,
    Recession,
    Recovery,
}

impl EconomicCondition {
    /// Contract quantity and payment multiplier range for this condition.
    fn modifier_range(&self) -> (f64, f64) {
        match self {
            EconomicCondition::Boom => (1.3, 1.5),
            EconomicCondition::Normal => (1.0, 1.0),
            EconomicCondition::Slowdown => (0.8, 0.9),
            EconomicCondition::Recession => (0.5, 0.7),
            EconomicCondition::Recovery => (0.85, 0.95),
        }
    }

    /// Duration range in months for this condition.
    fn duration_range(&self) -> (u32, u32) {
        match self {
            EconomicCondition::Boom => (6, 18),
            EconomicCondition::Normal => (12, 36),
            EconomicCondition::Slowdown => (4, 10),
            EconomicCondition::Recession => (3, 8),
            EconomicCondition::Recovery => (8, 24),
        }
    }

    /// Transition probabilities to next state. Must sum to 1.0.
    fn transitions(&self) -> &[(EconomicCondition, f64)] {
        match self {
            EconomicCondition::Boom => &[
                (EconomicCondition::Normal, 0.45),
                (EconomicCondition::Slowdown, 0.35),
                (EconomicCondition::Recession, 0.15),
                (EconomicCondition::Boom, 0.05),
            ],
            EconomicCondition::Normal => &[
                (EconomicCondition::Slowdown, 0.40),
                (EconomicCondition::Normal, 0.25),
                (EconomicCondition::Boom, 0.20),
                (EconomicCondition::Recession, 0.15),
            ],
            EconomicCondition::Slowdown => &[
                (EconomicCondition::Recession, 0.45),
                (EconomicCondition::Normal, 0.30),
                (EconomicCondition::Recovery, 0.15),
                (EconomicCondition::Slowdown, 0.10),
            ],
            EconomicCondition::Recession => &[
                (EconomicCondition::Recovery, 0.70),
                (EconomicCondition::Slowdown, 0.20),
                (EconomicCondition::Recession, 0.10),
            ],
            EconomicCondition::Recovery => &[
                (EconomicCondition::Normal, 0.50),
                (EconomicCondition::Boom, 0.25),
                (EconomicCondition::Slowdown, 0.15),
                (EconomicCondition::Recovery, 0.10),
            ],
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            EconomicCondition::Boom => "Boom",
            EconomicCondition::Normal => "Normal",
            EconomicCondition::Slowdown => "Slowdown",
            EconomicCondition::Recession => "Recession",
            EconomicCondition::Recovery => "Recovery",
        }
    }

    /// Flavor text for when this condition begins.
    pub fn flavor_text(&self) -> &'static str {
        match self {
            EconomicCondition::Boom =>
                "Investment capital flooding into space sector — launch demand surging",
            EconomicCondition::Normal =>
                "Space launch market operating at normal levels",
            EconomicCondition::Slowdown =>
                "Government budget cuts reducing satellite procurement",
            EconomicCondition::Recession =>
                "Global recession — launch contracts drying up",
            EconomicCondition::Recovery =>
                "Economy stabilizing — launch demand slowly recovering",
        }
    }
}

/// Persistent economic state, driven by seed-deterministic event chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicState {
    pub condition: EconomicCondition,
    pub modifier: f64,
    pub event_index: u32,
    pub end_date: GameDate,
}

impl Default for EconomicState {
    fn default() -> Self {
        EconomicState {
            condition: EconomicCondition::Normal,
            modifier: 1.0,
            event_index: 0,
            end_date: GameDate::default_start(),
        }
    }
}

/// Generate the initial economic state for a new game.
pub fn initial_state(seed: &GameSeed, start_date: GameDate) -> EconomicState {
    let mut rng = seed.world_query("economy_event_0");
    let (dur_lo, dur_hi) = EconomicCondition::Normal.duration_range();
    let duration_months = rng.gen_range(dur_lo..=dur_hi);
    let end_date = add_months(start_date, duration_months);

    EconomicState {
        condition: EconomicCondition::Normal,
        modifier: 1.0,
        event_index: 0,
        end_date,
    }
}

/// Check if the current economic state has expired and advance to the next.
/// Returns Some(new_condition) if a transition occurred.
pub fn advance_economy(
    state: &mut EconomicState,
    seed: &GameSeed,
    current_date: GameDate,
) -> Option<EconomicCondition> {
    if current_date < state.end_date {
        return None;
    }

    let next_index = state.event_index + 1;
    let query = format!("economy_event_{}", next_index);
    let mut rng = seed.world_query(&query);

    // Special case: event 1 is a dot-com crash ~50% of the time
    let next_condition = if next_index == 1 {
        let mut dot_com_rng = seed.world_query("economy_dot_com");
        if dot_com_rng.gen::<f64>() < 0.5 {
            EconomicCondition::Recession
        } else {
            roll_next_condition(state.condition, &mut rng)
        }
    } else {
        roll_next_condition(state.condition, &mut rng)
    };

    let (dur_lo, dur_hi) = next_condition.duration_range();
    let duration_months = rng.gen_range(dur_lo..=dur_hi);
    let end_date = add_months(current_date, duration_months);

    let (mod_lo, mod_hi) = next_condition.modifier_range();
    let modifier = if mod_lo < mod_hi {
        rng.gen_range(mod_lo..=mod_hi)
    } else {
        mod_lo
    };

    state.condition = next_condition;
    state.modifier = modifier;
    state.event_index = next_index;
    state.end_date = end_date;

    Some(next_condition)
}

fn roll_next_condition(
    current: EconomicCondition,
    rng: &mut rand::rngs::StdRng,
) -> EconomicCondition {
    let transitions = current.transitions();
    let roll: f64 = rng.gen();
    let mut cumulative = 0.0;
    for &(condition, prob) in transitions {
        cumulative += prob;
        if roll < cumulative {
            return condition;
        }
    }
    // Fallback (shouldn't happen if probabilities sum to 1.0)
    transitions.last().unwrap().0
}

/// Add N months to a date (lands on the 1st of the target month).
fn add_months(date: GameDate, months: u32) -> GameDate {
    let total_months = (date.year * 12 + date.month - 1) + months;
    let year = total_months / 12;
    let month = total_months % 12 + 1;
    GameDate::new(year, month, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::GameSeed;

    #[test]
    fn test_initial_state_is_normal() {
        let seed = GameSeed::new(42);
        let state = initial_state(&seed, GameDate::default_start());
        assert_eq!(state.condition, EconomicCondition::Normal);
        assert_eq!(state.modifier, 1.0);
        assert_eq!(state.event_index, 0);
        assert!(state.end_date > GameDate::default_start());
    }

    #[test]
    fn test_advance_before_expiry_returns_none() {
        let seed = GameSeed::new(42);
        let mut state = initial_state(&seed, GameDate::default_start());
        // Day 2 should be before the end date
        let result = advance_economy(&mut state, &seed, GameDate::new(2001, 1, 2));
        assert!(result.is_none());
    }

    #[test]
    fn test_advance_at_expiry_transitions() {
        let seed = GameSeed::new(42);
        let mut state = initial_state(&seed, GameDate::default_start());
        let end = state.end_date;
        let result = advance_economy(&mut state, &seed, end);
        assert!(result.is_some());
        assert_eq!(state.event_index, 1);
        assert!(state.end_date > end);
    }

    #[test]
    fn test_deterministic_across_calls() {
        let seed = GameSeed::new(123);
        let mut state1 = initial_state(&seed, GameDate::default_start());
        let mut state2 = initial_state(&seed, GameDate::default_start());

        // Advance both to the same point
        let end = state1.end_date;
        advance_economy(&mut state1, &seed, end);
        advance_economy(&mut state2, &seed, end);

        assert_eq!(state1.condition, state2.condition);
        assert_eq!(state1.modifier, state2.modifier);
        assert_eq!(state1.end_date, state2.end_date);
    }

    #[test]
    fn test_dot_com_crash_occurs_in_some_seeds() {
        let mut crash_count = 0;
        for s in 0..100 {
            let seed = GameSeed::new(s);
            let mut state = initial_state(&seed, GameDate::default_start());
            let end = state.end_date;
            advance_economy(&mut state, &seed, end);
            if state.condition == EconomicCondition::Recession {
                crash_count += 1;
            }
        }
        // Should be roughly 50% ± 15%
        assert!(crash_count > 30 && crash_count < 70,
            "Expected ~50 dot-com crashes in 100 seeds, got {}", crash_count);
    }

    #[test]
    fn test_long_chain_stays_valid() {
        let seed = GameSeed::new(99);
        let mut state = initial_state(&seed, GameDate::default_start());
        for _ in 0..50 {
            let end = state.end_date;
            let result = advance_economy(&mut state, &seed, end);
            assert!(result.is_some());
            assert!(state.modifier >= 0.4 && state.modifier <= 2.0,
                "Modifier {} out of range at event {}", state.modifier, state.event_index);
            assert!(state.end_date > end);
        }
    }

    #[test]
    fn test_recession_can_only_lead_to_valid_states() {
        // Run many transitions from Recession, verify all are valid successors
        for s in 0..200 {
            let seed = GameSeed::new(s);
            let mut rng = seed.world_query(&format!("test_recession_{}", s));
            let next = roll_next_condition(EconomicCondition::Recession, &mut rng);
            assert!(
                matches!(next, EconomicCondition::Recovery | EconomicCondition::Slowdown | EconomicCondition::Recession),
                "Recession led to {:?} which is not in its transition table", next
            );
        }
    }

    #[test]
    fn test_add_months() {
        assert_eq!(add_months(GameDate::new(2001, 1, 15), 3), GameDate::new(2001, 4, 1));
        assert_eq!(add_months(GameDate::new(2001, 11, 1), 3), GameDate::new(2002, 2, 1));
        assert_eq!(add_months(GameDate::new(2001, 1, 1), 12), GameDate::new(2002, 1, 1));
        assert_eq!(add_months(GameDate::new(2001, 1, 1), 24), GameDate::new(2003, 1, 1));
    }

    #[test]
    fn test_transition_probabilities_sum() {
        for condition in [
            EconomicCondition::Boom,
            EconomicCondition::Normal,
            EconomicCondition::Slowdown,
            EconomicCondition::Recession,
            EconomicCondition::Recovery,
        ] {
            let sum: f64 = condition.transitions().iter().map(|(_, p)| p).sum();
            assert!((sum - 1.0).abs() < 0.001,
                "{:?} transition probabilities sum to {}, expected 1.0", condition, sum);
        }
    }
}
