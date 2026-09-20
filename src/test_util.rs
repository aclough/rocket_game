//! Fixtures shared by the crate's unit tests (17_5_G.md step 4 / G2):
//! the default balance, a fixed-seed RNG, and kerolox and solid engines
//! with chosen thrust, mass and Isp. Integration tests under `tests/` have
//! their own `common` module; `GameState::new` / `with_money` are the
//! game-level fixtures and live on `GameState`.

use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::balance_config::BalanceConfig;
use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
use crate::propellant::Propellant;

/// The default balance.
pub fn bal() -> BalanceConfig {
    BalanceConfig::default()
}

/// A fixed-seed RNG for tests that want a particular draw sequence.
pub fn test_rng() -> StdRng {
    StdRng::seed_from_u64(42)
}

/// A gas-generator kerolox engine with the given figures, sea-level
/// bell, no power draw.
pub fn kerolox_engine(id: u64, thrust: f64, mass: f64, isp: f64) -> EngineDesign {
    EngineDesign {
        id: EngineId(id),
        name: format!("Engine-{}", id),
        cycle: EngineCycle::GasGenerator,
        thrust_n: thrust,
        mass_kg: mass,
        isp_s: isp,
        exit_pressure_pa: 70_000.0,
        needs_atmosphere: false,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
            PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
        ],
        power_draw_w: 0.0,
        chamber_pressure_pa: 9_000_000.0,
        expansion_ratio: 14.38,
        gamma: 1.2,
    }
}

/// A solid rocket booster with the given figures: pressure-fed, one
/// propellant, atmospheric exit pressure.
pub fn solid_engine(id: u64, thrust: f64, mass: f64, isp: f64) -> EngineDesign {
    EngineDesign {
        id: EngineId(id),
        name: format!("SRB-{}", id),
        cycle: EngineCycle::PressureFed,
        thrust_n: thrust,
        mass_kg: mass,
        isp_s: isp,
        exit_pressure_pa: 100_000.0,
        needs_atmosphere: false,
        propellant_mix: vec![
            PropellantFraction { propellant: Propellant::SolidMix, mass_fraction: 1.0 },
        ],
        power_draw_w: 0.0,
        chamber_pressure_pa: 9_000_000.0,
        expansion_ratio: 10.96,
        gamma: 1.2,
    }
}
