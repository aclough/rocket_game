//! Complexity tables for engines and rockets. Deliberately NOT part of
//! `BalanceConfig`: complexity already drives cost/flaw/work scaling,
//! and these tables are design decisions rather than tuning knobs.
//! Tunable work/cost formulas live in `balance_config`.

use crate::engine::EngineCycle;
use crate::propellant::Propellant;

/// Complexity of an engine cycle type (5-9 range).
pub fn cycle_complexity(cycle: EngineCycle) -> u32 {
    match cycle {
        EngineCycle::PressureFed => 5,
        EngineCycle::GasGenerator => 6,
        EngineCycle::Expander => 7,
        EngineCycle::StagedCombustion => 8,
        EngineCycle::FullFlow => 9,
        EngineCycle::NuclearThermal => 10,
        EngineCycle::ElectricPropulsion => 8,
        EngineCycle::SolarSail => 6,
    }
}

/// Complexity contribution from the fuel/propellant type.
/// Room temperature = 3, cryogenic = 4, hydrogen = 5.
pub fn fuel_complexity(propellants: &[Propellant]) -> u32 {
    propellants.iter().map(|p| single_fuel_complexity(*p)).max().unwrap_or(3)
}

fn single_fuel_complexity(p: Propellant) -> u32 {
    match p {
        Propellant::LH2 => 5,
        Propellant::LOX | Propellant::Methane => 4,
        Propellant::RP1 | Propellant::UDMH | Propellant::NTO | Propellant::SolidMix | Propellant::Xenon => 3,
    }
}

/// "Unexpected problems" factor for a propellant mix.
/// Currently only hydrogen has this (metal embrittlement).
/// Future: world seed lookup for exotic propellants.
pub fn problems_factor(propellants: &[Propellant]) -> u32 {
    if propellants.iter().any(|p| matches!(p, Propellant::LH2)) {
        1
    } else {
        0
    }
}

/// Combined complexity from cycle and fuel.
/// Takes the max of cycle and fuel complexity, +1 if they are equal.
pub fn combined_complexity(cycle: EngineCycle, propellants: &[Propellant]) -> u32 {
    let cc = cycle_complexity(cycle);
    let fc = fuel_complexity(propellants);
    if cc == fc {
        cc + 1
    } else {
        cc.max(fc)
    }
}

/// Effective complexity for flaw generation (includes problems factor).
pub fn effective_complexity(cycle: EngineCycle, propellants: &[Propellant]) -> u32 {
    combined_complexity(cycle, propellants) + problems_factor(propellants)
}

/// Rocket integration complexity based on design characteristics.
/// Factors: number of stages, unique engine types, parallel stages.
/// Range: ~3-8.
pub fn rocket_complexity(
    total_stages: u32,
    unique_engine_types: u32,
    max_parallel_stages: u32,
) -> u32 {
    let base = 3u32;
    let stage_factor = total_stages.saturating_sub(1); // each extra stage adds 1
    let engine_variety = unique_engine_types.saturating_sub(1); // each extra type adds 1
    let parallel_factor = if max_parallel_stages > 1 { 1 } else { 0 }; // boosters add 1

    (base + stage_factor + engine_variety + parallel_factor).min(8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cycle_complexity_range() {
        assert_eq!(cycle_complexity(EngineCycle::PressureFed), 5);
        assert_eq!(cycle_complexity(EngineCycle::GasGenerator), 6);
        assert_eq!(cycle_complexity(EngineCycle::Expander), 7);
        assert_eq!(cycle_complexity(EngineCycle::StagedCombustion), 8);
        assert_eq!(cycle_complexity(EngineCycle::FullFlow), 9);
    }

    #[test]
    fn test_fuel_complexity_room_temp() {
        assert_eq!(single_fuel_complexity(Propellant::RP1), 3);
        assert_eq!(single_fuel_complexity(Propellant::UDMH), 3);
        assert_eq!(single_fuel_complexity(Propellant::NTO), 3);
        assert_eq!(single_fuel_complexity(Propellant::SolidMix), 3);
    }

    #[test]
    fn test_fuel_complexity_cryogenic() {
        assert_eq!(single_fuel_complexity(Propellant::LOX), 4);
        assert_eq!(single_fuel_complexity(Propellant::Methane), 4);
    }

    #[test]
    fn test_fuel_complexity_hydrogen() {
        assert_eq!(single_fuel_complexity(Propellant::LH2), 5);
    }

    #[test]
    fn test_fuel_complexity_takes_max() {
        // Kerolox: LOX=4, RP1=3 → max=4
        assert_eq!(fuel_complexity(&[Propellant::LOX, Propellant::RP1]), 4);
        // Hydrolox: LOX=4, LH2=5 → max=5
        assert_eq!(fuel_complexity(&[Propellant::LOX, Propellant::LH2]), 5);
        // Hypergolic: UDMH=3, NTO=3 → max=3
        assert_eq!(fuel_complexity(&[Propellant::UDMH, Propellant::NTO]), 3);
    }

    #[test]
    fn test_problems_factor() {
        assert_eq!(problems_factor(&[Propellant::LOX, Propellant::RP1]), 0);
        assert_eq!(problems_factor(&[Propellant::LOX, Propellant::LH2]), 1);
        assert_eq!(problems_factor(&[Propellant::SolidMix]), 0);
    }

    #[test]
    fn test_combined_complexity_kerolox_gg() {
        // GG=6, Kerolox fuel=4 → max(6,4)=6
        let c = combined_complexity(EngineCycle::GasGenerator, &[Propellant::LOX, Propellant::RP1]);
        assert_eq!(c, 6);
    }

    #[test]
    fn test_combined_complexity_hydrolox_pressure_fed() {
        // PressureFed=5, Hydrolox fuel=5 → equal, so 5+1=6
        let c = combined_complexity(EngineCycle::PressureFed, &[Propellant::LOX, Propellant::LH2]);
        assert_eq!(c, 6);
    }

    #[test]
    fn test_combined_complexity_hydrolox_expander() {
        // Expander=7, Hydrolox fuel=5 → max(7,5)=7
        let c = combined_complexity(EngineCycle::Expander, &[Propellant::LOX, Propellant::LH2]);
        assert_eq!(c, 7);
    }

    #[test]
    fn test_combined_complexity_hypergolic_pressure_fed() {
        // PressureFed=5, Hypergolic fuel=3 → max(5,3)=5
        let c = combined_complexity(EngineCycle::PressureFed, &[Propellant::UDMH, Propellant::NTO]);
        assert_eq!(c, 5);
    }

    #[test]
    fn test_effective_complexity_includes_problems() {
        // Hydrolox Expander: combined=7, problems=1 → 8
        let e = effective_complexity(EngineCycle::Expander, &[Propellant::LOX, Propellant::LH2]);
        assert_eq!(e, 8);
        // Kerolox GG: combined=6, problems=0 → 6
        let e = effective_complexity(EngineCycle::GasGenerator, &[Propellant::LOX, Propellant::RP1]);
        assert_eq!(e, 6);
    }

    #[test]
    fn test_rocket_complexity_simple() {
        // 2 stages, 1 engine type, no parallel = 3 + 1 + 0 + 0 = 4
        assert_eq!(rocket_complexity(2, 1, 1), 4);
    }

    #[test]
    fn test_rocket_complexity_with_boosters() {
        // 3 stages, 2 engine types, parallel boosters = 3 + 2 + 1 + 1 = 7
        assert_eq!(rocket_complexity(3, 2, 2), 7);
    }

    #[test]
    fn test_rocket_complexity_capped() {
        // Even extreme rockets cap at 8
        assert_eq!(rocket_complexity(6, 4, 3), 8);
    }

    #[test]
    fn test_rocket_complexity_minimum() {
        // Single stage, 1 engine type, no parallel = 3
        assert_eq!(rocket_complexity(1, 1, 1), 3);
    }

}
