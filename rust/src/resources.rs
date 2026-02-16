/// Resource system: 8 resources with per-kg prices and bills of materials.
/// All quantities are by weight (kg) for future orbit-shipping mechanics.
/// See RESOURCES.md for full design rationale and real-world source data.

use crate::engine::costs;
use crate::engine_design::FuelType;

// ==========================================
// Tank Material
// ==========================================

/// Tank construction material choice — per-rocket, affects tank mass and cost
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TankMaterial {
    Aluminium,
    CarbonComposite,
}

impl TankMaterial {
    pub fn display_name(&self) -> &'static str {
        match self {
            TankMaterial::Aluminium => "Aluminium",
            TankMaterial::CarbonComposite => "Carbon Composite",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            TankMaterial::Aluminium => 0,
            TankMaterial::CarbonComposite => 1,
        }
    }

    pub fn from_index(i: usize) -> Option<TankMaterial> {
        match i {
            0 => Some(TankMaterial::Aluminium),
            1 => Some(TankMaterial::CarbonComposite),
            _ => None,
        }
    }

    pub fn count() -> usize {
        2
    }

    /// Tank mass multiplier: composite tanks are 30% lighter
    pub fn mass_ratio_multiplier(&self) -> f64 {
        match self {
            TankMaterial::Aluminium => 1.0,
            TankMaterial::CarbonComposite => 0.70,
        }
    }

    /// Stage assembly build time multiplier: composite takes 40% longer
    pub fn build_time_multiplier(&self) -> f64 {
        match self {
            TankMaterial::Aluminium => 1.0,
            TankMaterial::CarbonComposite => 1.4,
        }
    }
}

impl Default for TankMaterial {
    fn default() -> Self {
        TankMaterial::Aluminium
    }
}

// ==========================================
// Resource Types and Prices
// ==========================================

/// The 8 resource types in the game
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resource {
    Aluminium,
    Steel,
    Superalloys,
    Composites,
    Wiring,
    Electronics,
    Plumbing,
    SolidPropellant,
}

pub const RESOURCE_COUNT: usize = 8;

/// All resource variants in display order
pub const ALL_RESOURCES: [Resource; RESOURCE_COUNT] = [
    Resource::Aluminium,
    Resource::Steel,
    Resource::Superalloys,
    Resource::Composites,
    Resource::Wiring,
    Resource::Electronics,
    Resource::Plumbing,
    Resource::SolidPropellant,
];

impl Resource {
    pub fn display_name(&self) -> &'static str {
        match self {
            Resource::Aluminium => "Aluminium",
            Resource::Steel => "Steel",
            Resource::Superalloys => "Superalloys",
            Resource::Composites => "Composites",
            Resource::Wiring => "Wiring",
            Resource::Electronics => "Electronics",
            Resource::Plumbing => "Plumbing",
            Resource::SolidPropellant => "Solid Propellant",
        }
    }

    /// Price per kg in dollars
    pub fn price_per_kg(&self) -> f64 {
        match self {
            Resource::Aluminium => 5.0,
            Resource::Steel => 3.0,
            Resource::Superalloys => 80.0,
            Resource::Composites => 50.0,
            Resource::Wiring => 150.0,
            Resource::Electronics => 20_000.0,
            Resource::Plumbing => 1_500.0,
            Resource::SolidPropellant => 15.0,
        }
    }

    /// Get resource by index (matching ALL_RESOURCES order)
    pub fn from_index(i: usize) -> Option<Resource> {
        ALL_RESOURCES.get(i).copied()
    }
}

// ==========================================
// Bill of Materials
// ==========================================

/// A bill of materials: mass fractions of each resource (fractions sum to 1.0)
#[derive(Debug, Clone)]
pub struct BillOfMaterials {
    pub entries: Vec<(Resource, f64)>,
}

impl BillOfMaterials {
    /// Calculate total material cost for a given item mass in kg
    pub fn material_cost(&self, item_mass_kg: f64) -> f64 {
        self.entries
            .iter()
            .map(|(resource, fraction)| fraction * item_mass_kg * resource.price_per_kg())
            .sum()
    }

    /// Get the mass of each resource for a given item mass
    pub fn resource_masses(&self, item_mass_kg: f64) -> Vec<(Resource, f64)> {
        self.entries
            .iter()
            .map(|(resource, fraction)| (*resource, fraction * item_mass_kg))
            .collect()
    }
}

// ==========================================
// Engine BOMs
// ==========================================

/// Bill of materials for an engine casing by fuel type.
/// Fractions adjusted to sum to exactly 1.0 (see RESOURCES.md for source data).
pub fn engine_bom(fuel_type: FuelType) -> BillOfMaterials {
    match fuel_type {
        FuelType::Kerolox => BillOfMaterials {
            entries: vec![
                (Resource::Steel, 0.31),
                (Resource::Superalloys, 0.27),
                (Resource::Aluminium, 0.27), // Includes turbomachinery housings
                (Resource::Plumbing, 0.08),
                (Resource::Wiring, 0.04),
                (Resource::Composites, 0.02),
                (Resource::Electronics, 0.01),
            ],
        },
        FuelType::Hydrolox => BillOfMaterials {
            entries: vec![
                (Resource::Superalloys, 0.30),
                (Resource::Aluminium, 0.29), // Pump housings, structural
                (Resource::Steel, 0.18),
                (Resource::Plumbing, 0.12),
                (Resource::Composites, 0.06),
                (Resource::Wiring, 0.04),
                (Resource::Electronics, 0.01),
            ],
        },
        FuelType::Solid => BillOfMaterials {
            entries: vec![
                (Resource::Steel, 0.76),
                (Resource::Composites, 0.15),
                (Resource::Aluminium, 0.04),
                (Resource::Superalloys, 0.025),
                (Resource::Wiring, 0.0125),
                (Resource::Plumbing, 0.0075),
                (Resource::Electronics, 0.005),
            ],
        },
        FuelType::Methalox => BillOfMaterials {
            entries: vec![
                (Resource::Steel, 0.28),
                (Resource::Superalloys, 0.29),
                (Resource::Aluminium, 0.26),
                (Resource::Plumbing, 0.09),
                (Resource::Wiring, 0.04),
                (Resource::Composites, 0.03),
                (Resource::Electronics, 0.01),
            ],
        },
        FuelType::Hypergolic => BillOfMaterials {
            entries: vec![
                (Resource::Steel, 0.40),
                (Resource::Aluminium, 0.30),
                (Resource::Superalloys, 0.10),
                (Resource::Plumbing, 0.10),
                (Resource::Wiring, 0.05),
                (Resource::Composites, 0.03),
                (Resource::Electronics, 0.02),
            ],
        },
    }
}

// ==========================================
// Tank BOMs
// ==========================================

/// Bill of materials for propellant tanks by fuel type and material.
/// Solid motors have no separate tanks (casing is the engine).
pub fn tank_bom(fuel_type: FuelType, material: TankMaterial) -> BillOfMaterials {
    match (fuel_type, material) {
        (FuelType::Kerolox, TankMaterial::Aluminium) => BillOfMaterials {
            entries: vec![
                (Resource::Aluminium, 0.88),
                (Resource::Steel, 0.06),
                (Resource::Plumbing, 0.03),
                (Resource::Wiring, 0.02),
                (Resource::Composites, 0.01),
            ],
        },
        (FuelType::Kerolox, TankMaterial::CarbonComposite) => BillOfMaterials {
            entries: vec![
                (Resource::Composites, 0.72),
                (Resource::Aluminium, 0.10),
                (Resource::Steel, 0.06),
                (Resource::Superalloys, 0.05),
                (Resource::Wiring, 0.04),
                (Resource::Plumbing, 0.03),
            ],
        },
        (FuelType::Hydrolox, TankMaterial::Aluminium) => BillOfMaterials {
            entries: vec![
                (Resource::Aluminium, 0.74),
                (Resource::Composites, 0.14),
                (Resource::Steel, 0.06),
                (Resource::Plumbing, 0.035),
                (Resource::Wiring, 0.02),
                (Resource::Superalloys, 0.005),
            ],
        },
        (FuelType::Hydrolox, TankMaterial::CarbonComposite) => BillOfMaterials {
            entries: vec![
                (Resource::Composites, 0.72),
                (Resource::Aluminium, 0.08),
                (Resource::Superalloys, 0.075),
                (Resource::Steel, 0.05),
                (Resource::Wiring, 0.04),
                (Resource::Plumbing, 0.035),
            ],
        },
        (FuelType::Methalox, TankMaterial::Aluminium) => BillOfMaterials {
            entries: vec![
                (Resource::Aluminium, 0.85),
                (Resource::Steel, 0.07),
                (Resource::Plumbing, 0.04),
                (Resource::Wiring, 0.02),
                (Resource::Composites, 0.02),
            ],
        },
        (FuelType::Methalox, TankMaterial::CarbonComposite) => BillOfMaterials {
            entries: vec![
                (Resource::Composites, 0.72),
                (Resource::Aluminium, 0.10),
                (Resource::Steel, 0.06),
                (Resource::Superalloys, 0.05),
                (Resource::Wiring, 0.04),
                (Resource::Plumbing, 0.03),
            ],
        },
        (FuelType::Hypergolic, TankMaterial::Aluminium) => BillOfMaterials {
            entries: vec![
                (Resource::Aluminium, 0.82),
                (Resource::Steel, 0.08),
                (Resource::Plumbing, 0.05),
                (Resource::Wiring, 0.03),
                (Resource::Composites, 0.02),
            ],
        },
        (FuelType::Hypergolic, TankMaterial::CarbonComposite) => BillOfMaterials {
            entries: vec![
                (Resource::Composites, 0.70),
                (Resource::Aluminium, 0.10),
                (Resource::Steel, 0.08),
                (Resource::Superalloys, 0.05),
                (Resource::Wiring, 0.04),
                (Resource::Plumbing, 0.03),
            ],
        },
        (FuelType::Solid, _) => BillOfMaterials {
            entries: vec![],
        },
    }
}

// ==========================================
// Stage and Integration BOMs
// ==========================================

/// Fixed mass for stage assembly hardware (interstage, separation, avionics bay)
pub const STAGE_ASSEMBLY_MASS_KG: f64 = 300.0;

/// Fixed mass for rocket integration (fairing, flight computer, guidance, harness)
pub const ROCKET_INTEGRATION_MASS_KG: f64 = 700.0;

/// Bill of materials for stage assembly hardware (fractions from fixed kg in RESOURCES.md)
pub fn stage_assembly_bom() -> BillOfMaterials {
    BillOfMaterials {
        entries: vec![
            (Resource::Aluminium, 100.0 / 300.0),
            (Resource::Wiring, 70.0 / 300.0),
            (Resource::Steel, 45.0 / 300.0),
            (Resource::Plumbing, 35.0 / 300.0),
            (Resource::Composites, 25.0 / 300.0),
            (Resource::Electronics, 25.0 / 300.0),
        ],
    }
}

/// Bill of materials for rocket integration (fractions from fixed kg in RESOURCES.md)
pub fn rocket_integration_bom() -> BillOfMaterials {
    BillOfMaterials {
        entries: vec![
            (Resource::Aluminium, 300.0 / 700.0),
            (Resource::Composites, 200.0 / 700.0),
            (Resource::Wiring, 80.0 / 700.0),
            (Resource::Electronics, 50.0 / 700.0),
            (Resource::Steel, 40.0 / 700.0),
            (Resource::Plumbing, 30.0 / 700.0),
        ],
    }
}

// ==========================================
// Cost Functions
// ==========================================

/// Calculate solid propellant mass from engine casing mass
pub fn solid_propellant_mass(engine_mass_kg: f64) -> f64 {
    engine_mass_kg * costs::SOLID_MASS_RATIO / (1.0 - costs::SOLID_MASS_RATIO)
}

/// Material cost for an engine (casing BOM + solid propellant if applicable).
/// `engine_mass_kg` is the casing/dry mass (snapshot.mass_kg).
pub fn engine_resource_cost(fuel_type: FuelType, engine_mass_kg: f64) -> f64 {
    let casing_cost = engine_bom(fuel_type).material_cost(engine_mass_kg);

    if fuel_type == FuelType::Solid {
        let propellant_cost =
            solid_propellant_mass(engine_mass_kg) * Resource::SolidPropellant.price_per_kg();
        casing_cost + propellant_cost
    } else {
        casing_cost
    }
}

/// Material cost for propellant tanks
pub fn tank_resource_cost(fuel_type: FuelType, material: TankMaterial, tank_mass_kg: f64) -> f64 {
    tank_bom(fuel_type, material).material_cost(tank_mass_kg)
}

/// Material cost for stage assembly hardware (~$565K)
pub fn stage_assembly_cost() -> f64 {
    stage_assembly_bom().material_cost(STAGE_ASSEMBLY_MASS_KG)
}

/// Material cost for rocket integration (~$1.07M)
pub fn rocket_integration_cost() -> f64 {
    rocket_integration_bom().material_cost(ROCKET_INTEGRATION_MASS_KG)
}

// ==========================================
// Depot Bill of Materials
// ==========================================

/// Bill of materials for a fuel depot: aluminium-heavy with plumbing and wiring
pub fn depot_bom() -> BillOfMaterials {
    BillOfMaterials {
        entries: vec![
            (Resource::Aluminium, 0.60),
            (Resource::Plumbing, 0.20),
            (Resource::Wiring, 0.08),
            (Resource::Electronics, 0.02),
            (Resource::Composites, 0.10),
        ],
    }
}

/// Material cost for a fuel depot given its dry mass
pub fn depot_resource_cost(dry_mass_kg: f64) -> f64 {
    depot_bom().material_cost(dry_mass_kg)
}

// ==========================================
// Build Time Constants
// ==========================================

/// Base build days for an engine at scale 1.0 by fuel type.
/// Actual build work = base_days * scale^0.75 (exponent in manufacturing.rs).
pub fn engine_base_build_days(fuel_type: FuelType) -> f64 {
    match fuel_type {
        FuelType::Kerolox => 120.0,
        FuelType::Hydrolox => 180.0,
        FuelType::Solid => 45.0,
        FuelType::Methalox => 150.0,
        FuelType::Hypergolic => 60.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_bom_sums_to_one(bom: &BillOfMaterials) {
        let sum: f64 = bom.entries.iter().map(|(_, f)| f).sum();
        assert!(
            (sum - 1.0).abs() < 0.001,
            "BOM fractions should sum to 1.0, got {}",
            sum
        );
    }

    // ==========================================
    // BOM fraction tests
    // ==========================================

    #[test]
    fn test_kerolox_engine_bom_sums_to_one() {
        assert_bom_sums_to_one(&engine_bom(FuelType::Kerolox));
    }

    #[test]
    fn test_hydrolox_engine_bom_sums_to_one() {
        assert_bom_sums_to_one(&engine_bom(FuelType::Hydrolox));
    }

    #[test]
    fn test_solid_engine_bom_sums_to_one() {
        assert_bom_sums_to_one(&engine_bom(FuelType::Solid));
    }

    #[test]
    fn test_kerolox_tank_bom_sums_to_one() {
        assert_bom_sums_to_one(&tank_bom(FuelType::Kerolox, TankMaterial::Aluminium));
    }

    #[test]
    fn test_hydrolox_tank_bom_sums_to_one() {
        assert_bom_sums_to_one(&tank_bom(FuelType::Hydrolox, TankMaterial::Aluminium));
    }

    #[test]
    fn test_solid_tank_bom_empty() {
        let bom = tank_bom(FuelType::Solid, TankMaterial::Aluminium);
        assert!(bom.entries.is_empty());
    }

    #[test]
    fn test_kerolox_composite_tank_bom_sums_to_one() {
        assert_bom_sums_to_one(&tank_bom(FuelType::Kerolox, TankMaterial::CarbonComposite));
    }

    #[test]
    fn test_hydrolox_composite_tank_bom_sums_to_one() {
        assert_bom_sums_to_one(&tank_bom(FuelType::Hydrolox, TankMaterial::CarbonComposite));
    }

    #[test]
    fn test_solid_composite_tank_bom_empty() {
        let bom = tank_bom(FuelType::Solid, TankMaterial::CarbonComposite);
        assert!(bom.entries.is_empty());
    }

    #[test]
    fn test_methalox_engine_bom_sums_to_one() {
        assert_bom_sums_to_one(&engine_bom(FuelType::Methalox));
    }

    #[test]
    fn test_hypergolic_engine_bom_sums_to_one() {
        assert_bom_sums_to_one(&engine_bom(FuelType::Hypergolic));
    }

    #[test]
    fn test_methalox_tank_bom_sums_to_one() {
        assert_bom_sums_to_one(&tank_bom(FuelType::Methalox, TankMaterial::Aluminium));
    }

    #[test]
    fn test_methalox_composite_tank_bom_sums_to_one() {
        assert_bom_sums_to_one(&tank_bom(FuelType::Methalox, TankMaterial::CarbonComposite));
    }

    #[test]
    fn test_hypergolic_tank_bom_sums_to_one() {
        assert_bom_sums_to_one(&tank_bom(FuelType::Hypergolic, TankMaterial::Aluminium));
    }

    #[test]
    fn test_hypergolic_composite_tank_bom_sums_to_one() {
        assert_bom_sums_to_one(&tank_bom(FuelType::Hypergolic, TankMaterial::CarbonComposite));
    }

    #[test]
    fn test_stage_assembly_bom_sums_to_one() {
        assert_bom_sums_to_one(&stage_assembly_bom());
    }

    #[test]
    fn test_rocket_integration_bom_sums_to_one() {
        assert_bom_sums_to_one(&rocket_integration_bom());
    }

    // ==========================================
    // Engine cost tests
    // ==========================================

    #[test]
    fn test_kerolox_engine_cost() {
        // 450 kg at scale 1.0 → ~$157,896
        let cost = engine_resource_cost(FuelType::Kerolox, 450.0);
        assert!(
            (cost - 157_896.0).abs() < 100.0,
            "Kerolox engine cost should be ~$158K, got ${:.0}",
            cost
        );
    }

    #[test]
    fn test_hydrolox_engine_cost() {
        // 300 kg at scale 1.0 → ~$124,497
        let cost = engine_resource_cost(FuelType::Hydrolox, 300.0);
        assert!(
            (cost - 124_497.0).abs() < 100.0,
            "Hydrolox engine cost should be ~$124K, got ${:.0}",
            cost
        );
    }

    #[test]
    fn test_solid_motor_total_cost() {
        // 40,000 kg casing → ~$5M casing + ~$4.4M propellant ≈ $9.4M
        let cost = engine_resource_cost(FuelType::Solid, 40_000.0);
        assert!(
            (cost - 9_404_200.0).abs() < 10_000.0,
            "Solid motor total should be ~$9.4M, got ${:.0}",
            cost
        );
    }

    #[test]
    fn test_solid_propellant_mass_calculation() {
        // 40,000 kg casing, mass_ratio 0.88 → 293,333 kg propellant
        let mass = solid_propellant_mass(40_000.0);
        assert!(
            (mass - 293_333.0).abs() < 100.0,
            "Solid propellant should be ~293,333 kg, got {:.0}",
            mass
        );
    }

    // ==========================================
    // Tank cost tests
    // ==========================================

    #[test]
    fn test_kerolox_tank_cost() {
        // 6,000 kg tank → ~$318K (dominated by plumbing)
        let cost = tank_resource_cost(FuelType::Kerolox, TankMaterial::Aluminium, 6_000.0);
        assert!(
            cost > 310_000.0 && cost < 325_000.0,
            "Kerolox 6000 kg tank should be ~$318K, got ${:.0}",
            cost
        );
    }

    #[test]
    fn test_hydrolox_tank_cost() {
        // 2,000 kg tank → higher composite fraction
        let cost = tank_resource_cost(FuelType::Hydrolox, TankMaterial::Aluminium, 2_000.0);
        assert!(
            cost > 120_000.0 && cost < 140_000.0,
            "Hydrolox 2000 kg tank cost: ${:.0}",
            cost
        );
    }

    #[test]
    fn test_solid_tank_cost_zero() {
        assert_eq!(tank_resource_cost(FuelType::Solid, TankMaterial::Aluminium, 1000.0), 0.0);
    }

    // ==========================================
    // Composite tank tests
    // ==========================================

    #[test]
    fn test_composite_tank_cost_higher_per_kg() {
        // Carbon composite should cost more per kg than aluminium
        let alu_cost = tank_resource_cost(FuelType::Kerolox, TankMaterial::Aluminium, 1000.0);
        let comp_cost = tank_resource_cost(FuelType::Kerolox, TankMaterial::CarbonComposite, 1000.0);
        assert!(
            comp_cost > alu_cost * 1.5,
            "Composite cost/kg should be ~1.7x aluminium: alu=${:.0}, comp=${:.0}",
            alu_cost, comp_cost
        );
    }

    #[test]
    fn test_composite_tank_net_cost_higher() {
        // With 30% lighter tanks, net cost should still be ~20% more
        let mass = 1000.0;
        let alu_mass = mass * TankMaterial::Aluminium.mass_ratio_multiplier();
        let comp_mass = mass * TankMaterial::CarbonComposite.mass_ratio_multiplier();
        let alu_cost = tank_resource_cost(FuelType::Kerolox, TankMaterial::Aluminium, alu_mass);
        let comp_cost = tank_resource_cost(FuelType::Kerolox, TankMaterial::CarbonComposite, comp_mass);
        assert!(
            comp_cost > alu_cost,
            "Net composite cost should exceed aluminium: alu=${:.0}, comp=${:.0}",
            alu_cost, comp_cost
        );
    }

    #[test]
    fn test_tank_material_mass_multipliers() {
        assert_eq!(TankMaterial::Aluminium.mass_ratio_multiplier(), 1.0);
        assert!((TankMaterial::CarbonComposite.mass_ratio_multiplier() - 0.70).abs() < 0.001);
    }

    #[test]
    fn test_tank_material_build_multipliers() {
        assert_eq!(TankMaterial::Aluminium.build_time_multiplier(), 1.0);
        assert!((TankMaterial::CarbonComposite.build_time_multiplier() - 1.4).abs() < 0.001);
    }

    #[test]
    fn test_tank_material_from_index() {
        assert_eq!(TankMaterial::from_index(0), Some(TankMaterial::Aluminium));
        assert_eq!(TankMaterial::from_index(1), Some(TankMaterial::CarbonComposite));
        assert_eq!(TankMaterial::from_index(2), None);
    }

    // ==========================================
    // Assembly and integration cost tests
    // ==========================================

    #[test]
    fn test_stage_assembly_cost() {
        // 300 kg → ~$564,885
        let cost = stage_assembly_cost();
        assert!(
            (cost - 564_885.0).abs() < 100.0,
            "Stage assembly should be ~$565K, got ${:.0}",
            cost
        );
    }

    #[test]
    fn test_rocket_integration_cost() {
        // 700 kg → ~$1,068,620
        let cost = rocket_integration_cost();
        assert!(
            (cost - 1_068_620.0).abs() < 100.0,
            "Rocket integration should be ~$1.069M, got ${:.0}",
            cost
        );
    }

    // ==========================================
    // Scaling and misc tests
    // ==========================================

    #[test]
    fn test_engine_cost_scales_linearly() {
        let cost_1x = engine_resource_cost(FuelType::Kerolox, 450.0);
        let cost_2x = engine_resource_cost(FuelType::Kerolox, 900.0);
        assert!(
            (cost_2x / cost_1x - 2.0).abs() < 0.001,
            "Double mass should double cost"
        );
    }

    #[test]
    fn test_build_days_by_type() {
        assert_eq!(engine_base_build_days(FuelType::Kerolox), 120.0);
        assert_eq!(engine_base_build_days(FuelType::Hydrolox), 180.0);
        assert_eq!(engine_base_build_days(FuelType::Solid), 45.0);
        assert_eq!(engine_base_build_days(FuelType::Methalox), 150.0);
        assert_eq!(engine_base_build_days(FuelType::Hypergolic), 60.0);
    }

    #[test]
    fn test_resource_from_index() {
        assert_eq!(Resource::from_index(0), Some(Resource::Aluminium));
        assert_eq!(Resource::from_index(7), Some(Resource::SolidPropellant));
        assert_eq!(Resource::from_index(8), None);
    }

    #[test]
    fn test_resource_prices() {
        assert_eq!(Resource::Aluminium.price_per_kg(), 5.0);
        assert_eq!(Resource::Electronics.price_per_kg(), 20_000.0);
        assert_eq!(Resource::SolidPropellant.price_per_kg(), 15.0);
    }

    #[test]
    fn test_bom_resource_masses() {
        let bom = engine_bom(FuelType::Kerolox);
        let masses = bom.resource_masses(450.0);
        // Steel fraction 0.31 × 450 = 139.5
        let steel_mass = masses
            .iter()
            .find(|(r, _)| *r == Resource::Steel)
            .map(|(_, m)| *m)
            .unwrap();
        assert!((steel_mass - 139.5).abs() < 0.1);
    }
}
