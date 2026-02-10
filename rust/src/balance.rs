/// Tuning parameters for game balance.
/// Centralizes constants that affect cost, build time, performance, and risk tradeoffs.

use crate::engine_design::FuelType;

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

/// Per-unit mass reduction from complexity above center (2% per unit).
pub const COMPLEXITY_MASS_BONUS_PER_UNIT: f64 = 0.02;

/// Per-unit exhaust velocity bonus from complexity above center (1% per unit).
pub const COMPLEXITY_VE_BONUS_PER_UNIT: f64 = 0.01;

/// Exponent for complexity cost scaling: (complexity / center)^EXPONENT.
pub const COMPLEXITY_COST_EXPONENT: f64 = 2.0;

/// Exponent for complexity build-time scaling: (complexity / center)^EXPONENT.
pub const COMPLEXITY_BUILD_EXPONENT: f64 = 2.0;

/// Get the complexity range for a given fuel type.
pub fn complexity_range(fuel_type: FuelType) -> ComplexityRange {
    match fuel_type {
        FuelType::Solid => ComplexityRange { min: 2, max: 4, center: 3 },
        FuelType::Kerolox => ComplexityRange { min: 4, max: 8, center: 6 },
        FuelType::Hydrolox => ComplexityRange { min: 5, max: 9, center: 7 },
    }
}

/// Mass multiplier from complexity: 1.0 - BONUS_PER_UNIT * (complexity - center).
/// Higher complexity = lighter engine.
pub fn complexity_mass_multiplier(complexity: i32, center: i32) -> f64 {
    1.0 - COMPLEXITY_MASS_BONUS_PER_UNIT * (complexity - center) as f64
}

/// Exhaust velocity multiplier from complexity: 1.0 + BONUS_PER_UNIT * (complexity - center).
/// Higher complexity = better exhaust velocity.
pub fn complexity_ve_multiplier(complexity: i32, center: i32) -> f64 {
    1.0 + COMPLEXITY_VE_BONUS_PER_UNIT * (complexity - center) as f64
}

/// Cost/build-time multiplier from complexity: (complexity / center)^EXPONENT.
pub fn complexity_cost_multiplier(complexity: i32, center: i32) -> f64 {
    (complexity as f64 / center as f64).powf(COMPLEXITY_COST_EXPONENT)
}

/// Build-time multiplier from complexity: (complexity / center)^EXPONENT.
pub fn complexity_build_multiplier(complexity: i32, center: i32) -> f64 {
    (complexity as f64 / center as f64).powf(COMPLEXITY_BUILD_EXPONENT)
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
    }

    #[test]
    fn test_center_complexity_is_neutral() {
        // At center, all multipliers should be 1.0
        for ft in [FuelType::Solid, FuelType::Kerolox, FuelType::Hydrolox] {
            let range = complexity_range(ft);
            let c = range.center;
            assert!((complexity_mass_multiplier(c, c) - 1.0).abs() < 1e-10);
            assert!((complexity_ve_multiplier(c, c) - 1.0).abs() < 1e-10);
            assert!((complexity_cost_multiplier(c, c) - 1.0).abs() < 1e-10);
            assert!((complexity_build_multiplier(c, c) - 1.0).abs() < 1e-10);
            assert_eq!(complexity_flaw_modifier(c, c), 0);
        }
    }

    #[test]
    fn test_high_complexity_effects() {
        // Kerolox at max complexity (8, center 6)
        let c = 6;
        let high = 8;

        // Mass: 1.0 - 0.02 * 2 = 0.96 (lighter)
        assert!((complexity_mass_multiplier(high, c) - 0.96).abs() < 1e-10);

        // VE: 1.0 + 0.01 * 2 = 1.02 (better)
        assert!((complexity_ve_multiplier(high, c) - 1.02).abs() < 1e-10);

        // Cost: (8/6)^2 ≈ 1.778 (more expensive)
        let cost_mult = complexity_cost_multiplier(high, c);
        assert!((cost_mult - (8.0_f64 / 6.0).powi(2)).abs() < 1e-6);

        // Flaw modifier: +2
        assert_eq!(complexity_flaw_modifier(high, c), 2);
    }

    #[test]
    fn test_low_complexity_effects() {
        // Kerolox at min complexity (4, center 6)
        let c = 6;
        let low = 4;

        // Mass: 1.0 - 0.02 * (-2) = 1.04 (heavier)
        assert!((complexity_mass_multiplier(low, c) - 1.04).abs() < 1e-10);

        // VE: 1.0 + 0.01 * (-2) = 0.98 (worse)
        assert!((complexity_ve_multiplier(low, c) - 0.98).abs() < 1e-10);

        // Cost: (4/6)^2 ≈ 0.444 (cheaper)
        let cost_mult = complexity_cost_multiplier(low, c);
        assert!((cost_mult - (4.0_f64 / 6.0).powi(2)).abs() < 1e-6);

        // Flaw modifier: -2
        assert_eq!(complexity_flaw_modifier(low, c), -2);
    }
}
