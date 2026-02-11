/// Tuning parameters for game balance.
/// Centralizes constants that affect cost, build time, performance, and risk tradeoffs.

use crate::engine_design::{FuelType, EngineCycle};

// ==========================================
// Engine Complexity
// ==========================================

/// Complexity range for an engine fuel type.
#[derive(Debug, Clone, Copy)]
pub struct ComplexityRange {
    pub min: i32,
    pub max: i32,
    pub center: i32,
}

/// Exponent for complexity cost scaling: (complexity / center)^EXPONENT.
pub const COMPLEXITY_COST_EXPONENT: f64 = 2.0;

/// Get the complexity range for a given fuel type.
pub fn complexity_range(fuel_type: FuelType) -> ComplexityRange {
    match fuel_type {
        FuelType::Solid => ComplexityRange { min: 2, max: 4, center: 3 },
        FuelType::Kerolox => ComplexityRange { min: 4, max: 8, center: 6 },
        FuelType::Hydrolox => ComplexityRange { min: 5, max: 9, center: 7 },
        FuelType::Methalox => ComplexityRange { min: 5, max: 9, center: 7 },
        FuelType::Hypergolic => ComplexityRange { min: 1, max: 4, center: 2 },
    }
}

/// Cost multiplier from complexity: (complexity / center)^EXPONENT.
pub fn complexity_cost_multiplier(complexity: i32, center: i32) -> f64 {
    (complexity as f64 / center as f64).powf(COMPLEXITY_COST_EXPONENT)
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

/// Cost multiplier by engine cycle (on top of material cost and complexity).
pub fn cycle_cost_multiplier(cycle: EngineCycle) -> f64 {
    match cycle {
        EngineCycle::PressureFed => 0.4,
        EngineCycle::GasGenerator => 1.0,
        EngineCycle::Expander => 1.3,
        EngineCycle::StagedCombustion => 2.0,
        EngineCycle::FullFlowStagedCombustion => 3.0,
    }
}

/// Build time multiplier by engine cycle.
pub fn cycle_build_multiplier(cycle: EngineCycle) -> f64 {
    match cycle {
        EngineCycle::PressureFed => 0.5,
        EngineCycle::GasGenerator => 1.0,
        EngineCycle::Expander => 1.2,
        EngineCycle::StagedCombustion => 1.6,
        EngineCycle::FullFlowStagedCombustion => 2.2,
    }
}

/// Flaw count modifier from complexity.
/// Returns the offset to add to base flaw count (3).
/// At center: 0, above center: positive, below center: negative.
pub fn complexity_flaw_modifier(complexity: i32, center: i32) -> i32 {
    complexity - center
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complexity_ranges() {
        let solid = complexity_range(FuelType::Solid);
        assert_eq!((solid.min, solid.max, solid.center), (2, 4, 3));

        let kerolox = complexity_range(FuelType::Kerolox);
        assert_eq!((kerolox.min, kerolox.max, kerolox.center), (4, 8, 6));

        let hydrolox = complexity_range(FuelType::Hydrolox);
        assert_eq!((hydrolox.min, hydrolox.max, hydrolox.center), (5, 9, 7));

        let methalox = complexity_range(FuelType::Methalox);
        assert_eq!((methalox.min, methalox.max, methalox.center), (5, 9, 7));

        let hypergolic = complexity_range(FuelType::Hypergolic);
        assert_eq!((hypergolic.min, hypergolic.max, hypergolic.center), (1, 4, 2));
    }

    #[test]
    fn test_center_complexity_cost_is_neutral() {
        // At center, cost multiplier should be 1.0
        for ft in [FuelType::Solid, FuelType::Kerolox, FuelType::Hydrolox, FuelType::Methalox, FuelType::Hypergolic] {
            let range = complexity_range(ft);
            let c = range.center;
            assert!((complexity_cost_multiplier(c, c) - 1.0).abs() < 1e-10);
            assert_eq!(complexity_flaw_modifier(c, c), 0);
        }
    }

    #[test]
    fn test_high_complexity_cost_and_flaws() {
        // Kerolox at max complexity (8, center 6)
        let c = 6;
        let high = 8;

        // Cost: (8/6)^2 ≈ 1.778 (more expensive)
        let cost_mult = complexity_cost_multiplier(high, c);
        assert!((cost_mult - (8.0_f64 / 6.0).powi(2)).abs() < 1e-6);

        // Flaw modifier: +2
        assert_eq!(complexity_flaw_modifier(high, c), 2);
    }

    #[test]
    fn test_low_complexity_cost_and_flaws() {
        // Kerolox at min complexity (4, center 6)
        let c = 6;
        let low = 4;

        // Cost: (4/6)^2 ≈ 0.444 (cheaper)
        let cost_mult = complexity_cost_multiplier(low, c);
        assert!((cost_mult - (4.0_f64 / 6.0).powi(2)).abs() < 1e-6);

        // Flaw modifier: -2
        assert_eq!(complexity_flaw_modifier(low, c), -2);
    }

    // ==========================================
    // Cycle Multiplier Tests
    // ==========================================

    #[test]
    fn test_gas_generator_is_baseline() {
        assert_eq!(cycle_thrust_multiplier(EngineCycle::GasGenerator), 1.0);
        assert_eq!(cycle_ve_multiplier(EngineCycle::GasGenerator), 1.0);
        assert_eq!(cycle_mass_multiplier(EngineCycle::GasGenerator), 1.0);
        assert_eq!(cycle_cost_multiplier(EngineCycle::GasGenerator), 1.0);
        assert_eq!(cycle_build_multiplier(EngineCycle::GasGenerator), 1.0);
    }

    #[test]
    fn test_pressure_fed_multipliers() {
        assert_eq!(cycle_thrust_multiplier(EngineCycle::PressureFed), 0.6);
        assert_eq!(cycle_ve_multiplier(EngineCycle::PressureFed), 0.92);
        assert_eq!(cycle_mass_multiplier(EngineCycle::PressureFed), 0.7);
        assert_eq!(cycle_cost_multiplier(EngineCycle::PressureFed), 0.4);
        assert_eq!(cycle_build_multiplier(EngineCycle::PressureFed), 0.5);
    }

    #[test]
    fn test_full_flow_multipliers() {
        assert_eq!(cycle_thrust_multiplier(EngineCycle::FullFlowStagedCombustion), 1.3);
        assert_eq!(cycle_ve_multiplier(EngineCycle::FullFlowStagedCombustion), 1.08);
        assert_eq!(cycle_mass_multiplier(EngineCycle::FullFlowStagedCombustion), 1.3);
        assert_eq!(cycle_cost_multiplier(EngineCycle::FullFlowStagedCombustion), 3.0);
        assert_eq!(cycle_build_multiplier(EngineCycle::FullFlowStagedCombustion), 2.2);
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

    #[test]
    fn test_higher_cycles_cost_more() {
        let pf = cycle_cost_multiplier(EngineCycle::PressureFed);
        let gg = cycle_cost_multiplier(EngineCycle::GasGenerator);
        let ex = cycle_cost_multiplier(EngineCycle::Expander);
        let sc = cycle_cost_multiplier(EngineCycle::StagedCombustion);
        let ff = cycle_cost_multiplier(EngineCycle::FullFlowStagedCombustion);
        assert!(pf < gg);
        assert!(gg < ex);
        assert!(ex < sc);
        assert!(sc < ff);
    }
}
