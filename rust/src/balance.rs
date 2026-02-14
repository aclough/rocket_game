/// Tuning parameters for game balance.
/// Centralizes constants that affect cost, build time, performance, and risk tradeoffs.

use crate::engine_design::EngineCycle;

// ==========================================
// Engine Complexity
// ==========================================

/// Fixed baseline complexity for cost and flaw scaling.
/// Complexity 6 (e.g. Kerolox GasGenerator) is the neutral reference point.
pub const COMPLEXITY_BASELINE: i32 = 6;

/// Cost multiplier from complexity: (complexity / COMPLEXITY_BASELINE)^2.
pub fn complexity_cost_multiplier(complexity: i32) -> f64 {
    (complexity as f64 / COMPLEXITY_BASELINE as f64).powi(2)
}

/// Flaw count modifier from complexity.
/// Returns the offset to add to base flaw count (3).
/// At baseline: 0, above: positive, below: negative.
pub fn complexity_flaw_modifier(complexity: i32) -> i32 {
    complexity - COMPLEXITY_BASELINE
}

/// Build time multiplier from complexity: complexity / COMPLEXITY_BASELINE (linear).
/// At baseline (6): 1.0. Lower complexity = faster build, higher = slower.
pub fn complexity_build_multiplier(complexity: i32) -> f64 {
    complexity as f64 / COMPLEXITY_BASELINE as f64
}

// ==========================================
// Engine Cycle Performance Multipliers
// ==========================================
// All values are relative to GasGenerator = 1.0.
// PressureFed: no turbopump → lightest/cheapest, low chamber pressure.
// GasGenerator: workhorse baseline.
// Expander: efficient closed cycle, heat-limited thrust (cryogenics only).
// StagedCombustion: high chamber pressure → excellent perf, heavy/expensive.
// FullFlow: most extreme staged combustion variant.

/// Thrust multiplier by engine cycle.
pub fn cycle_thrust_multiplier(cycle: EngineCycle) -> f64 {
    match cycle {
        EngineCycle::PressureFed => 0.6,
        EngineCycle::GasGenerator => 1.0,
        EngineCycle::Expander => 0.8,
        EngineCycle::StagedCombustion => 1.15,
        EngineCycle::FullFlowStagedCombustion => 1.3,
    }
}

/// Exhaust velocity (ISP) multiplier by engine cycle.
pub fn cycle_ve_multiplier(cycle: EngineCycle) -> f64 {
    match cycle {
        EngineCycle::PressureFed => 0.92,
        EngineCycle::GasGenerator => 1.0,
        EngineCycle::Expander => 1.04,
        EngineCycle::StagedCombustion => 1.06,
        EngineCycle::FullFlowStagedCombustion => 1.08,
    }
}

/// Mass multiplier by engine cycle.
pub fn cycle_mass_multiplier(cycle: EngineCycle) -> f64 {
    match cycle {
        EngineCycle::PressureFed => 0.7,
        EngineCycle::GasGenerator => 1.0,
        EngineCycle::Expander => 0.9,
        EngineCycle::StagedCombustion => 1.15,
        EngineCycle::FullFlowStagedCombustion => 1.3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_baseline_complexity_is_neutral() {
        // At baseline (6), all complexity multipliers should be 1.0
        assert!((complexity_cost_multiplier(COMPLEXITY_BASELINE) - 1.0).abs() < 1e-10);
        assert!((complexity_build_multiplier(COMPLEXITY_BASELINE) - 1.0).abs() < 1e-10);
        assert_eq!(complexity_flaw_modifier(COMPLEXITY_BASELINE), 0);
    }

    #[test]
    fn test_high_complexity_cost_and_flaws() {
        // Complexity 8: cost = (8/6)^2 ≈ 1.778
        let cost_mult = complexity_cost_multiplier(8);
        assert!((cost_mult - (8.0_f64 / 6.0).powi(2)).abs() < 1e-6);
        assert_eq!(complexity_flaw_modifier(8), 2);
    }

    #[test]
    fn test_low_complexity_cost_and_flaws() {
        // Complexity 4: cost = (4/6)^2 ≈ 0.444
        let cost_mult = complexity_cost_multiplier(4);
        assert!((cost_mult - (4.0_f64 / 6.0).powi(2)).abs() < 1e-6);
        assert_eq!(complexity_flaw_modifier(4), -2);
    }

    #[test]
    fn test_very_low_complexity() {
        // Complexity 1 (Hypergolic PressureFed): cost = (1/6)^2 ≈ 0.028
        let cost_mult = complexity_cost_multiplier(1);
        assert!((cost_mult - (1.0_f64 / 6.0).powi(2)).abs() < 1e-6);
        assert_eq!(complexity_flaw_modifier(1), -5);
    }

    #[test]
    fn test_high_complexity_hydrolox_fullflow() {
        // Complexity 9 (Hydrolox FullFlow): cost = (9/6)^2 = 2.25
        let cost_mult = complexity_cost_multiplier(9);
        assert!((cost_mult - (9.0_f64 / 6.0).powi(2)).abs() < 1e-6);
        assert_eq!(complexity_flaw_modifier(9), 3);
    }

    // ==========================================
    // Cycle Multiplier Tests
    // ==========================================

    #[test]
    fn test_gas_generator_is_baseline() {
        assert_eq!(cycle_thrust_multiplier(EngineCycle::GasGenerator), 1.0);
        assert_eq!(cycle_ve_multiplier(EngineCycle::GasGenerator), 1.0);
        assert_eq!(cycle_mass_multiplier(EngineCycle::GasGenerator), 1.0);
    }

    #[test]
    fn test_pressure_fed_multipliers() {
        assert_eq!(cycle_thrust_multiplier(EngineCycle::PressureFed), 0.6);
        assert_eq!(cycle_ve_multiplier(EngineCycle::PressureFed), 0.92);
        assert_eq!(cycle_mass_multiplier(EngineCycle::PressureFed), 0.7);
    }

    #[test]
    fn test_full_flow_multipliers() {
        assert_eq!(cycle_thrust_multiplier(EngineCycle::FullFlowStagedCombustion), 1.3);
        assert_eq!(cycle_ve_multiplier(EngineCycle::FullFlowStagedCombustion), 1.08);
        assert_eq!(cycle_mass_multiplier(EngineCycle::FullFlowStagedCombustion), 1.3);
    }

    #[test]
    fn test_complexity_build_multiplier() {
        // Linear: complexity / 6
        assert!((complexity_build_multiplier(1) - 1.0 / 6.0).abs() < 1e-10);
        assert!((complexity_build_multiplier(3) - 0.5).abs() < 1e-10);
        assert!((complexity_build_multiplier(6) - 1.0).abs() < 1e-10);
        assert!((complexity_build_multiplier(9) - 1.5).abs() < 1e-10);
    }

    #[test]
    fn test_higher_cycles_have_more_thrust() {
        let pf = cycle_thrust_multiplier(EngineCycle::PressureFed);
        let gg = cycle_thrust_multiplier(EngineCycle::GasGenerator);
        let sc = cycle_thrust_multiplier(EngineCycle::StagedCombustion);
        let ff = cycle_thrust_multiplier(EngineCycle::FullFlowStagedCombustion);
        assert!(pf < gg);
        assert!(gg < sc);
        assert!(sc < ff);
    }
}
