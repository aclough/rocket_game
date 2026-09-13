use rand::Rng;
use serde::{Serialize, Deserialize};

use crate::balance_config::EconomyConfig;
use crate::calendar::GameDate;
use crate::seed::{GameSeed, WorldQuery};

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
    // Modifier ranges, durations and transitions are balance data:
    // `EconomyConfig` (17_3_PHYSICS.md D6).

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
pub fn initial_state(seed: &GameSeed, start_date: GameDate, cfg: &EconomyConfig) -> EconomicState {
    let mut rng = seed.world_query(WorldQuery::EconomyEvent(0));
    let normal = cfg.condition(EconomicCondition::Normal);
    let (dur_lo, dur_hi) = (normal.duration_min_months, normal.duration_max_months);
    let duration_months = rng.gen_range(dur_lo..=dur_hi);
    let end_date = start_date.add_months(duration_months);

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
    cfg: &EconomyConfig,
) -> Option<EconomicCondition> {
    if current_date < state.end_date {
        return None;
    }

    let next_index = state.event_index + 1;
    let mut rng = seed.world_query(WorldQuery::EconomyEvent(next_index));

    // Special case: event 1 is a dot-com crash ~50% of the time
    let next_condition = if next_index == 1 {
        let mut dot_com_rng = seed.world_query(WorldQuery::EconomyDotCom);
        if dot_com_rng.gen::<f64>() < cfg.dot_com_crash_chance {
            EconomicCondition::Recession
        } else {
            roll_next_condition(state.condition, &mut rng, cfg)
        }
    } else {
        roll_next_condition(state.condition, &mut rng, cfg)
    };

    let next = cfg.condition(next_condition);
    let (dur_lo, dur_hi) = (next.duration_min_months, next.duration_max_months);
    let duration_months = rng.gen_range(dur_lo..=dur_hi);
    let end_date = current_date.add_months(duration_months);

    let (mod_lo, mod_hi) = (next.modifier_min, next.modifier_max);
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
    cfg: &EconomyConfig,
) -> EconomicCondition {
    let transitions = &cfg.condition(current).transitions;
    let roll: f64 = rng.gen();
    let mut cumulative = 0.0;
    for t in transitions {
        cumulative += t.chance;
        if roll < cumulative {
            return t.to;
        }
    }
    // Fallback (shouldn't happen if probabilities sum to 1.0)
    transitions.last().unwrap().to
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> EconomyConfig {
        EconomyConfig::default()
    }
    use crate::seed::GameSeed;

    #[test]
    fn test_initial_state_is_normal() {
        let seed = GameSeed::new(42);
        let state = initial_state(&seed, GameDate::default_start(), &cfg());
        assert_eq!(state.condition, EconomicCondition::Normal);
        assert_eq!(state.modifier, 1.0);
        assert_eq!(state.event_index, 0);
        assert!(state.end_date > GameDate::default_start());
    }

    #[test]
    fn test_advance_before_expiry_returns_none() {
        let seed = GameSeed::new(42);
        let mut state = initial_state(&seed, GameDate::default_start(), &cfg());
        // Day 2 should be before the end date
        let result = advance_economy(&mut state, &seed, GameDate::new(2001, 1, 2), &cfg());
        assert!(result.is_none());
    }

    #[test]
    fn test_advance_at_expiry_transitions() {
        let seed = GameSeed::new(42);
        let mut state = initial_state(&seed, GameDate::default_start(), &cfg());
        let end = state.end_date;
        let result = advance_economy(&mut state, &seed, end, &cfg());
        assert!(result.is_some());
        assert_eq!(state.event_index, 1);
        assert!(state.end_date > end);
    }

    #[test]
    fn test_deterministic_across_calls() {
        let seed = GameSeed::new(123);
        let mut state1 = initial_state(&seed, GameDate::default_start(), &cfg());
        let mut state2 = initial_state(&seed, GameDate::default_start(), &cfg());

        // Advance both to the same point
        let end = state1.end_date;
        advance_economy(&mut state1, &seed, end, &cfg());
        advance_economy(&mut state2, &seed, end, &cfg());

        assert_eq!(state1.condition, state2.condition);
        assert_eq!(state1.modifier, state2.modifier);
        assert_eq!(state1.end_date, state2.end_date);
    }

    #[test]
    fn test_dot_com_crash_occurs_in_some_seeds() {
        let mut crash_count = 0;
        for s in 0..100 {
            let seed = GameSeed::new(s);
            let mut state = initial_state(&seed, GameDate::default_start(), &cfg());
            let end = state.end_date;
            advance_economy(&mut state, &seed, end, &cfg());
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
        let mut state = initial_state(&seed, GameDate::default_start(), &cfg());
        for _ in 0..50 {
            let end = state.end_date;
            let result = advance_economy(&mut state, &seed, end, &cfg());
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
            let mut rng = seed.world_query(WorldQuery::Test(&format!("test_recession_{}", s)));
            let next = roll_next_condition(EconomicCondition::Recession, &mut rng, &cfg());
            assert!(
                matches!(next, EconomicCondition::Recovery | EconomicCondition::Slowdown | EconomicCondition::Recession),
                "Recession led to {:?} which is not in its transition table", next
            );
        }
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
            let sum: f64 = cfg().condition(condition).transitions.iter().map(|t| t.chance).sum();
            assert!((sum - 1.0).abs() < 0.001,
                "{:?} transition probabilities sum to {}, expected 1.0", condition, sum);
        }
    }
}
