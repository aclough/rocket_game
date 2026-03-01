use crate::engine::{EngineDesign, G0};
use crate::propellant::Propellant;

/// G-load limit for thrust structure sizing.
/// Higher values = lighter structure but less safety margin.
const STRUCTURAL_G_LIMIT: f64 = 40.0;

/// Aerodynamic shell mass fraction for stages exposed to airflow.
const AERO_SHELL_FRACTION: f64 = 0.05;

/// Interstage adapter mass in kg per stage boundary.
const INTERSTAGE_MASS_KG: f64 = 200.0;

/// Tank mass fraction: ratio of tank structural mass to propellant mass.
/// Driven primarily by propellant volume (low-density propellants need bigger tanks).
///
/// Returns a fraction such that tank_mass = propellant_mass * fraction.
pub fn tank_mass_fraction(mix: &[(Propellant, f64)]) -> f64 {
    // Compute effective density of the propellant mix
    // volume per kg = sum(fraction_i / density_i)
    let volume_per_kg: f64 = mix.iter()
        .map(|(prop, fraction)| fraction / prop.density_kg_per_l())
        .sum();

    // Reference: kerolox is about 0.04 tank fraction at ~0.9 L/kg effective
    // Hydrogen is about 0.10 at ~2.5 L/kg effective
    // Linear interpolation between these reference points
    let kerolox_vol_per_kg = 0.725 / 1.141 + 0.275 / 0.82; // ~0.97 L/kg
    let kerolox_fraction = 0.04;
    let hydrolox_vol_per_kg = 0.83 / 1.141 + 0.17 / 0.071; // ~3.12 L/kg
    let hydrolox_fraction = 0.10;

    // Linear interpolation/extrapolation
    let slope = (hydrolox_fraction - kerolox_fraction) / (hydrolox_vol_per_kg - kerolox_vol_per_kg);
    let fraction = kerolox_fraction + slope * (volume_per_kg - kerolox_vol_per_kg);

    fraction.max(0.02).min(0.15) // clamp to reasonable range
}

/// Compute thrust structure mass: the structure holding engines to the stage.
/// Sized by maximum thrust load.
pub fn thrust_structure_mass_kg(engine: &EngineDesign, engine_count: u32) -> f64 {
    let total_thrust = engine.thrust_n * engine_count as f64;
    total_thrust / (G0 * STRUCTURAL_G_LIMIT)
}

/// Compute aerodynamic shell mass for stages exposed to airflow.
/// Returns 0 for upper stages not exposed to atmosphere.
pub fn aero_shell_mass_kg(base_structure_mass: f64, exposed_to_airflow: bool) -> f64 {
    if exposed_to_airflow {
        base_structure_mass * AERO_SHELL_FRACTION
    } else {
        0.0
    }
}

/// Whether a stage is the first stage group or a booster (exposed to airflow).
/// `group_index` is 0 for the first (bottom) stage group.
pub fn is_exposed_to_airflow(group_index: usize) -> bool {
    group_index == 0
}

/// Compute total structural mass for a stage.
///
/// Components:
/// - Tank mass (from propellant volume/density)
/// - Thrust structure (from engine thrust)
/// - Aerodynamic shell (for exposed stages)
/// - Interstage adapter
pub fn compute_structural_mass(
    propellant_mass_kg: f64,
    propellant_mix: &[(Propellant, f64)],
    engine: &EngineDesign,
    engine_count: u32,
    exposed_to_airflow: bool,
    has_interstage: bool,
) -> StructuralMassBreakdown {
    let tank_fraction = tank_mass_fraction(propellant_mix);
    let tank_mass = propellant_mass_kg * tank_fraction;
    let thrust_struct = thrust_structure_mass_kg(engine, engine_count);
    let base_structure = tank_mass + thrust_struct;
    let aero_shell = aero_shell_mass_kg(base_structure, exposed_to_airflow);
    let interstage = if has_interstage { INTERSTAGE_MASS_KG } else { 0.0 };

    StructuralMassBreakdown {
        tank_mass,
        thrust_structure: thrust_struct,
        aero_shell,
        interstage,
        total: tank_mass + thrust_struct + aero_shell + interstage,
    }
}

/// Breakdown of structural mass components (for UI display).
#[derive(Debug, Clone, Copy)]
pub struct StructuralMassBreakdown {
    pub tank_mass: f64,
    pub thrust_structure: f64,
    pub aero_shell: f64,
    pub interstage: f64,
    pub total: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::*;
    use crate::propellant::Propellant;

    fn kerolox_mix() -> Vec<(Propellant, f64)> {
        vec![(Propellant::LOX, 0.725), (Propellant::RP1, 0.275)]
    }

    fn hydrolox_mix() -> Vec<(Propellant, f64)> {
        vec![(Propellant::LOX, 0.83), (Propellant::LH2, 0.17)]
    }

    fn test_engine(thrust: f64, mass: f64) -> EngineDesign {
        EngineDesign {
            id: EngineId(1),
            name: "Test".into(),
            cycle: EngineCycle::GasGenerator,
            thrust_n: thrust,
            mass_kg: mass,
            isp_s: 300.0,
            exit_pressure_pa: 70_000.0,
            needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
        }
    }

    #[test]
    fn test_tank_fraction_kerolox() {
        let frac = tank_mass_fraction(&kerolox_mix());
        // Should be around 0.04 for kerolox
        assert!((frac - 0.04).abs() < 0.01, "kerolox tank fraction: {}", frac);
    }

    #[test]
    fn test_tank_fraction_hydrolox() {
        let frac = tank_mass_fraction(&hydrolox_mix());
        // Should be around 0.10 for hydrolox (larger tanks needed)
        assert!((frac - 0.10).abs() < 0.02, "hydrolox tank fraction: {}", frac);
    }

    #[test]
    fn test_hydrolox_heavier_tanks_than_kerolox() {
        let kero_frac = tank_mass_fraction(&kerolox_mix());
        let hydro_frac = tank_mass_fraction(&hydrolox_mix());
        assert!(hydro_frac > kero_frac, "Hydrolox should need heavier tanks");
    }

    #[test]
    fn test_thrust_structure_scales_with_thrust() {
        let engine = test_engine(1_000_000.0, 500.0);
        let m1 = thrust_structure_mass_kg(&engine, 1);
        let m9 = thrust_structure_mass_kg(&engine, 9);
        assert!((m9 / m1 - 9.0).abs() < 0.01);
    }

    #[test]
    fn test_thrust_structure_reasonable() {
        let engine = test_engine(1_000_000.0, 500.0);
        let mass = thrust_structure_mass_kg(&engine, 1);
        // 1MN / (9.8 * 40) ≈ 2551 kg
        assert!((mass - 2551.0).abs() < 10.0, "thrust struct: {}", mass);
    }

    #[test]
    fn test_aero_shell_only_for_exposed() {
        assert_eq!(aero_shell_mass_kg(1000.0, false), 0.0);
        assert!((aero_shell_mass_kg(1000.0, true) - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_structural_mass_first_stage() {
        let engine = test_engine(1_000_000.0, 500.0);
        let breakdown = compute_structural_mass(
            50_000.0,
            &kerolox_mix(),
            &engine,
            1,
            true,  // first stage
            true,  // has interstage
        );
        assert!(breakdown.total > 0.0);
        assert!(breakdown.tank_mass > 0.0);
        assert!(breakdown.thrust_structure > 0.0);
        assert!(breakdown.aero_shell > 0.0);
        assert_eq!(breakdown.interstage, INTERSTAGE_MASS_KG);
        assert!((breakdown.total - (breakdown.tank_mass + breakdown.thrust_structure + breakdown.aero_shell + breakdown.interstage)).abs() < 0.01);
    }

    #[test]
    fn test_compute_structural_mass_upper_stage() {
        let engine = test_engine(200_000.0, 100.0);
        let breakdown = compute_structural_mass(
            10_000.0,
            &kerolox_mix(),
            &engine,
            1,
            false, // upper stage
            false, // top stage, no interstage above
        );
        assert_eq!(breakdown.aero_shell, 0.0);
        assert_eq!(breakdown.interstage, 0.0);
        assert!(breakdown.total > 0.0);
    }

    #[test]
    fn test_structural_fraction_reasonable() {
        // For a typical first stage with kerolox, structural mass should be
        // roughly 5-10% of propellant mass
        let engine = test_engine(1_000_000.0, 500.0);
        let breakdown = compute_structural_mass(
            100_000.0, &kerolox_mix(), &engine, 1, true, true,
        );
        let fraction = breakdown.total / 100_000.0;
        assert!(fraction > 0.04 && fraction < 0.15,
            "Structural fraction {} should be 4-15% for first stage", fraction);
    }
}
