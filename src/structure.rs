use serde::{Deserialize, Serialize};

use crate::engine::{EngineDesign, G0};
use crate::propellant::Propellant;

/// What a stage's tanks and skin are made of (25_1_BODIES.md step 2).
/// The material does not change the structural model, only its
/// coefficients and its bill of materials: steel is heavier, cheap and
/// tough; composite lighter, dear and slow to build; aluminium is what
/// every design was before the choice existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum StructuralMaterial {
    #[default]
    Aluminium,
    Steel,
    Composite,
}

impl StructuralMaterial {
    pub const ALL: [StructuralMaterial; 3] = [Self::Aluminium, Self::Steel, Self::Composite];

    pub fn name(self) -> &'static str {
        match self {
            Self::Aluminium => "aluminium",
            Self::Steel => "steel",
            Self::Composite => "composite",
        }
    }

    /// Two letters for a stage line.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Aluminium => "Al",
            Self::Steel => "St",
            Self::Composite => "CF",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Aluminium => Self::Steel,
            Self::Steel => Self::Composite,
            Self::Composite => Self::Aluminium,
        }
    }

    /// Multiplier on the tank mass fraction. Specific strength puts a
    /// cold-worked stainless tank within ~15 % of 2219 aluminium and a
    /// flight composite tank ~30 % under it after knockdowns.
    pub fn tank_mass_factor(self) -> f64 {
        match self {
            Self::Aluminium => 1.0,
            Self::Steel => 1.15,
            Self::Composite => 0.70,
        }
    }

    /// Multiplier on the aero shell and interstage (skin); the thrust
    /// structure is steel in every case.
    pub fn skin_factor(self) -> f64 {
        self.tank_mass_factor()
    }

    /// The structural mass this stage would have in aluminium: its
    /// geometry. The same distance is plumbed and wired whatever the
    /// skin weighs, so fittings and assembly hardware are priced on
    /// this, and only the skin on the mass the material actually has.
    pub fn geometry_mass_kg(self, structural_mass_kg: f64) -> f64 {
        structural_mass_kg / self.tank_mass_factor()
    }

    /// Multiplier on the tank's material cost per kilogram of structure:
    /// a finished composite skin costs several times an aluminium one
    /// per kilogram (tooling, autoclave, scrap), stainless less. Applies
    /// to the skin's share of the bill only; fittings are priced on the
    /// geometry (`geometry_mass_kg`).
    pub fn material_cost_factor(self) -> f64 {
        match self {
            Self::Aluminium => 1.0,
            Self::Steel => 0.6,
            Self::Composite => 2.5,
        }
    }

    /// Multiplier on stage build work: composite is laid up slowly,
    /// steel welds fast.
    pub fn build_work_factor(self) -> f64 {
        match self {
            Self::Aluminium => 1.0,
            Self::Steel => 0.9,
            Self::Composite => 1.4,
        }
    }

    /// Tank wall mass per cubic metre of tank per bar of tank pressure
    /// (thin-wall vessel: `k_shape · p · ρ / σ_allow`, cylinder plus
    /// domes with the safety factor in σ). From ρ/σ: aluminium 2.8,
    /// stainless 3.0, composite 1.2.
    pub fn pressure_kg_per_m3_per_bar(self) -> f64 {
        match self {
            Self::Aluminium => 2.8,
            Self::Steel => 3.0,
            Self::Composite => 1.2,
        }
    }

    /// The skin temperature a stage may reach, during entry, without a
    /// shield and still hold itself up: 2219 aluminium has lost a third
    /// of its strength at 150 °C, austenitic stainless keeps ~60 % at
    /// 500 and creeps past 550, epoxy composite softens at its glass
    /// transition. Read by the entry check (25_2).
    pub fn temperature_limit_c(self) -> f64 {
        match self {
            Self::Aluminium => 150.0,
            Self::Steel => 550.0,
            Self::Composite => 120.0,
        }
    }
}

/// G-load the thrust structure is sized to carry: thrust / (g × this)
/// is its mass. 150 is 0.68 kg per kN — the Saturn V S-IC's thrust
/// structure (~21 t for 33 MN) — and still heavy against a modern
/// octaweb. Was 40 (2.55 kg/kN), which put a 3.8 t structure under the
/// bot's 1.5 MN and 574 kg under an Electron-class first stage, as
/// much as its tanks (24_STRATEGIES.md §2.2).
const STRUCTURAL_G_LIMIT: f64 = 150.0;

/// Aerodynamic shell mass fraction for stages exposed to airflow.
const AERO_SHELL_FRACTION: f64 = 0.05;

/// A pressure-fed engine's tanks run at this multiple of its chamber
/// pressure (the feed drop); a pump-fed stage's tanks run at a few bar
/// for ullage, which the tank fraction already describes
/// (25_1_BODIES.md step 3).
const PRESSURE_FED_TANK_RATIO: f64 = 1.3;
/// Pressurant and its bottles, as a share of propellant mass per bar
/// of tank pressure: helium at high pressure plus the vessels that
/// hold it, ~1.2 % of propellant at 20 bar.
const PRESSURANT_FRACTION_PER_BAR: f64 = 0.0006;

/// Tank pressure of a stage fed by `engine`, in bar: zero unless the
/// engine is pressure-fed and liquid (a solid's case is the motor and
/// already in its mass).
pub fn tank_pressure_bar(engine: &EngineDesign) -> f64 {
    if engine.cycle != crate::engine::EngineCycle::PressureFed || engine.is_solid() {
        return 0.0;
    }
    engine.propulsion.bell().map_or(0.0, |(pc, _, _)| pc / 100_000.0 * PRESSURE_FED_TANK_RATIO)
}

/// Volume of `propellant_mass_kg` of the mix, in cubic metres.
pub fn propellant_volume_m3(propellant_mass_kg: f64, mix: &[(Propellant, f64)]) -> f64 {
    mix.iter().map(|(p, f)| propellant_mass_kg * f / p.density_kg_per_l()).sum::<f64>() / 1000.0
}

/// Interstage adapter: this share of the wet mass it carries (the
/// stages above it), never less than the floor. A flat 200 kg was 1 %
/// of an Electron-class vehicle and 0.16 % of the bot's.
const INTERSTAGE_FRACTION: f64 = 0.02;
const INTERSTAGE_MIN_KG: f64 = 50.0;

/// Mass of the adapter carrying `carried_kg` of upper stack.
pub fn interstage_mass_kg(carried_kg: f64) -> f64 {
    (carried_kg * INTERSTAGE_FRACTION).max(INTERSTAGE_MIN_KG)
}

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

    fraction.clamp(0.02, 0.15) // clamp to reasonable range
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
/// - Interstage adapter, when the stage carries an upper stack of
///   `interstage_carried_kg` (wet, payload excluded)
pub fn compute_structural_mass(
    propellant_mass_kg: f64,
    propellant_mix: &[(Propellant, f64)],
    engine: &EngineDesign,
    engine_count: u32,
    exposed_to_airflow: bool,
    interstage_carried_kg: Option<f64>,
    material: StructuralMaterial,
) -> StructuralMassBreakdown {
    let tank_fraction = tank_mass_fraction(propellant_mix) * material.tank_mass_factor();
    let tank_mass = propellant_mass_kg * tank_fraction;
    // A pressure-fed stage's tanks are pressure vessels: thick walls
    // by the pressure and the material, and the pressurant that fills
    // them (25_1_BODIES.md step 3). Zero for pump-fed and solid stages.
    let pressure_bar = tank_pressure_bar(engine);
    let pressure_wall = propellant_volume_m3(propellant_mass_kg, propellant_mix)
        * pressure_bar * material.pressure_kg_per_m3_per_bar();
    let pressurant = propellant_mass_kg * pressure_bar * PRESSURANT_FRACTION_PER_BAR;
    let thrust_struct = thrust_structure_mass_kg(engine, engine_count);
    let base_structure = tank_mass + pressure_wall + thrust_struct;
    let aero_shell = aero_shell_mass_kg(base_structure, exposed_to_airflow) * material.skin_factor();
    let interstage = interstage_carried_kg.map_or(0.0, interstage_mass_kg) * material.skin_factor();

    StructuralMassBreakdown {
        tank_mass,
        pressure_wall,
        pressurant,
        thrust_structure: thrust_struct,
        aero_shell,
        interstage,
        total: tank_mass + pressure_wall + pressurant + thrust_struct + aero_shell + interstage,
    }
}

/// Breakdown of structural mass components (for UI display).
#[derive(Debug, Clone, Copy)]
pub struct StructuralMassBreakdown {
    pub tank_mass: f64,
    /// Thick tank walls on a pressure-fed stage; zero otherwise.
    pub pressure_wall: f64,
    /// Pressurant and its bottles on a pressure-fed stage; zero otherwise.
    pub pressurant: f64,
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
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
            propulsion: Propulsion::nozzle(9_000_000.0, 14.38, 1.2, false),
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
        // 1 MN at 150 g: 680 kg, 0.68 kg per kN.
        assert!((mass - 679.6).abs() < 2.0, "thrust struct: {}", mass);
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
            Some(20_000.0), // carries a 20 t upper stack
            StructuralMaterial::Aluminium,
        );
        assert!(breakdown.total > 0.0);
        assert!(breakdown.tank_mass > 0.0);
        assert!(breakdown.thrust_structure > 0.0);
        assert!(breakdown.aero_shell > 0.0);
        assert!((breakdown.interstage - 400.0).abs() < 1e-9, "2 % of 20 t");
        assert_eq!(interstage_mass_kg(1_000.0), 50.0, "the floor");
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
            None,  // top stage, no interstage above
            StructuralMaterial::Aluminium,
        );
        assert_eq!(breakdown.aero_shell, 0.0);
        assert_eq!(breakdown.interstage, 0.0);
        assert!(breakdown.total > 0.0);
    }

    /// 25_1_BODIES.md step 3: a pressure-fed stage's tanks are pressure
    /// vessels. A 15 bar kerolox pressure-fed engine puts its tanks at
    /// ~20 bar: 2.8 kg/m³/bar on 0.97 L/kg adds ~5.3 % of propellant in
    /// wall and ~1.2 % in pressurant — a 9–10 % tank where a pump-fed
    /// stage has 4 %, which is what keeps pressure-fed stages small.
    #[test]
    fn pressure_fed_tanks_are_pressure_vessels() {
        let mut pf = test_engine(225_000.0, 350.0);
        pf.cycle = EngineCycle::PressureFed;
        pf.propulsion = Propulsion::nozzle(15.0e5, 5.5, 1.22, false);
        let pumped = test_engine(225_000.0, 350.0);
        assert!((tank_pressure_bar(&pf) - 19.5).abs() < 1e-9);
        assert_eq!(tank_pressure_bar(&pumped), 0.0);
        let prop = 14_800.0;
        let b = compute_structural_mass(prop, &kerolox_mix(), &pf, 1, true, None, StructuralMaterial::Aluminium);
        let wall_share = b.pressure_wall / prop;
        let pressurant_share = b.pressurant / prop;
        assert!((wall_share - 0.053).abs() < 0.006, "wall {wall_share}");
        assert!((pressurant_share - 0.0117).abs() < 0.001, "pressurant {pressurant_share}");
        let p = compute_structural_mass(prop, &kerolox_mix(), &pumped, 1, true, None, StructuralMaterial::Aluminium);
        assert!(b.total > p.total * 1.4, "pressure-fed {} vs pump-fed {}", b.total, p.total);
        // Composite walls carry pressure for less; a solid carries none.
        let c = compute_structural_mass(prop, &kerolox_mix(), &pf, 1, true, None, StructuralMaterial::Composite);
        assert!(c.pressure_wall < b.pressure_wall * 0.5);
        let mut solid = pf.clone();
        solid.propellant_mix = vec![PropellantFraction { propellant: Propellant::SolidMix, mass_fraction: 1.0 }];
        assert_eq!(tank_pressure_bar(&solid), 0.0);
    }

    #[test]
    fn test_structural_fraction_reasonable() {
        // For a typical first stage with kerolox, structural mass should be
        // roughly 5-10% of propellant mass
        let engine = test_engine(1_000_000.0, 500.0);
        let breakdown = compute_structural_mass(
            100_000.0, &kerolox_mix(), &engine, 1, true, Some(20_000.0), StructuralMaterial::Aluminium,
        );
        // 25_1_BODIES.md step 2: steel is heavier, composite lighter, the
        // thrust structure the same in every material.
        let steel = compute_structural_mass(
            100_000.0, &kerolox_mix(), &engine, 1, true, Some(20_000.0), StructuralMaterial::Steel,
        );
        let composite = compute_structural_mass(
            100_000.0, &kerolox_mix(), &engine, 1, true, Some(20_000.0), StructuralMaterial::Composite,
        );
        assert!((steel.tank_mass / breakdown.tank_mass - 1.15).abs() < 1e-9);
        assert!((composite.tank_mass / breakdown.tank_mass - 0.70).abs() < 1e-9);
        assert_eq!(steel.thrust_structure, breakdown.thrust_structure);
        assert!(steel.total > breakdown.total && composite.total < breakdown.total);
        // A pump-fed stage carries no pressure vessel.
        assert_eq!(breakdown.pressure_wall, 0.0);
        assert_eq!(breakdown.pressurant, 0.0);
        let fraction = breakdown.total / 100_000.0;
        assert!(fraction > 0.04 && fraction < 0.15,
            "Structural fraction {} should be 4-15% for first stage", fraction);
    }
}
