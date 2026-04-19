use serde::{Serialize, Deserialize};

use crate::engine_project::PropellantPreset;

/// Resource types used in manufacturing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Resource {
    Aluminium,
    Steel,
    Superalloys,
    Composites,
    Wiring,
    Electronics,
    Plumbing,
    SolidPropellant,
    /// Highly Enriched Uranium for nuclear thermal engines.
    HEU,
}

impl Resource {
    pub const ALL: &[Resource] = &[
        Resource::Aluminium,
        Resource::Steel,
        Resource::Superalloys,
        Resource::Composites,
        Resource::Wiring,
        Resource::Electronics,
        Resource::Plumbing,
        Resource::SolidPropellant,
        Resource::HEU,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Resource::Aluminium => "Aluminium",
            Resource::Steel => "Steel",
            Resource::Superalloys => "Superalloys",
            Resource::Composites => "Composites",
            Resource::Wiring => "Wiring",
            Resource::Electronics => "Electronics",
            Resource::Plumbing => "Plumbing",
            Resource::SolidPropellant => "Solid Propellant",
            Resource::HEU => "HEU",
        }
    }

    /// Price per kilogram in dollars.
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
            Resource::HEU => 100_000.0, // very expensive, regulated material
        }
    }
}

/// A bill of materials: fraction of total mass for each resource.
#[derive(Debug, Clone)]
pub struct BillOfMaterials {
    /// (resource, mass_fraction) pairs — fractions should sum to ~1.0.
    pub fractions: Vec<(Resource, f64)>,
}

impl BillOfMaterials {
    /// Compute the material cost for a given total mass.
    pub fn material_cost(&self, mass_kg: f64) -> f64 {
        self.fractions.iter()
            .map(|(r, frac)| mass_kg * frac * r.price_per_kg())
            .sum()
    }

    /// Get the mass of each resource for a given total mass.
    pub fn resource_masses(&self, mass_kg: f64) -> Vec<(Resource, f64)> {
        self.fractions.iter()
            .map(|(r, frac)| (*r, mass_kg * frac))
            .collect()
    }
}

/// BOM for engine manufacturing, varies by propellant type.
///
/// Engines are mostly superalloys/steel for the combustion chamber and nozzle,
/// with plumbing for turbopump/injector and electronics for control.
pub fn engine_bom(preset: PropellantPreset) -> BillOfMaterials {
    match preset {
        PropellantPreset::Kerolox | PropellantPreset::Methalox => BillOfMaterials {
            fractions: vec![
                (Resource::Superalloys, 0.40),
                (Resource::Steel, 0.25),
                (Resource::Aluminium, 0.10),
                (Resource::Plumbing, 0.10),
                (Resource::Wiring, 0.05),
                (Resource::Electronics, 0.005),
                (Resource::Composites, 0.095),
            ],
        },
        PropellantPreset::Hydrolox => BillOfMaterials {
            fractions: vec![
                (Resource::Superalloys, 0.45),
                (Resource::Steel, 0.20),
                (Resource::Aluminium, 0.08),
                (Resource::Plumbing, 0.12),
                (Resource::Wiring, 0.05),
                (Resource::Electronics, 0.005),
                (Resource::Composites, 0.095),
            ],
        },
        PropellantPreset::Hypergolic => BillOfMaterials {
            fractions: vec![
                (Resource::Superalloys, 0.30),
                (Resource::Steel, 0.30),
                (Resource::Aluminium, 0.15),
                (Resource::Plumbing, 0.10),
                (Resource::Wiring, 0.05),
                (Resource::Electronics, 0.005),
                (Resource::Composites, 0.095),
            ],
        },
        PropellantPreset::Solid => BillOfMaterials {
            fractions: vec![
                (Resource::Steel, 0.30),
                (Resource::Composites, 0.20),
                (Resource::Aluminium, 0.10),
                (Resource::SolidPropellant, 0.35),
                (Resource::Wiring, 0.02),
                (Resource::Electronics, 0.003),
                (Resource::Plumbing, 0.027),
            ],
        },
        // Electric propulsion: electronics-heavy, lightweight
        PropellantPreset::Xenon => BillOfMaterials {
            fractions: vec![
                (Resource::Superalloys, 0.20),
                (Resource::Steel, 0.15),
                (Resource::Aluminium, 0.15),
                (Resource::Wiring, 0.15),
                (Resource::Electronics, 0.20),
                (Resource::Plumbing, 0.10),
                (Resource::Composites, 0.05),
            ],
        },
        // Solar sail: mostly lightweight sail material (composites) with structure
        PropellantPreset::Photon => BillOfMaterials {
            fractions: vec![
                (Resource::Composites, 0.50),
                (Resource::Aluminium, 0.20),
                (Resource::Wiring, 0.10),
                (Resource::Electronics, 0.10),
                (Resource::Steel, 0.10),
            ],
        },
        // Nuclear thermal: reactor core (HEU), shielding, hydrogen plumbing
        PropellantPreset::Hydrogen => BillOfMaterials {
            fractions: vec![
                (Resource::HEU, 0.15),
                (Resource::Superalloys, 0.35),
                (Resource::Steel, 0.20),
                (Resource::Aluminium, 0.05),
                (Resource::Plumbing, 0.10),
                (Resource::Wiring, 0.05),
                (Resource::Electronics, 0.01),
                (Resource::Composites, 0.04),
            ],
        },
    }
}

/// BOM for tank/stage structure manufacturing.
/// Tanks are mostly aluminium with some composites and plumbing.
pub fn tank_bom() -> BillOfMaterials {
    BillOfMaterials {
        fractions: vec![
            (Resource::Aluminium, 0.65),
            (Resource::Composites, 0.15),
            (Resource::Steel, 0.10),
            (Resource::Plumbing, 0.05),
            (Resource::Wiring, 0.03),
            (Resource::Electronics, 0.002),
            (Resource::Superalloys, 0.018),
        ],
    }
}

/// BOM for stage assembly (fixed cost, not mass-dependent).
/// Covers integration hardware, wiring harnesses, avionics.
pub fn stage_assembly_bom() -> BillOfMaterials {
    BillOfMaterials {
        fractions: vec![
            (Resource::Wiring, 0.30),
            (Resource::Electronics, 0.10),
            (Resource::Aluminium, 0.25),
            (Resource::Steel, 0.15),
            (Resource::Composites, 0.10),
            (Resource::Plumbing, 0.10),
        ],
    }
}

/// Fixed mass for stage assembly hardware (kg).
pub const STAGE_ASSEMBLY_MASS_KG: f64 = 500.0;

/// BOM for final rocket integration.
/// Covers interstage adapters, payload fairings, final wiring.
pub fn rocket_integration_bom() -> BillOfMaterials {
    BillOfMaterials {
        fractions: vec![
            (Resource::Aluminium, 0.35),
            (Resource::Composites, 0.20),
            (Resource::Wiring, 0.15),
            (Resource::Electronics, 0.05),
            (Resource::Steel, 0.15),
            (Resource::Plumbing, 0.05),
            (Resource::Superalloys, 0.05),
        ],
    }
}

/// Fixed mass for rocket integration hardware (kg).
pub const ROCKET_INTEGRATION_MASS_KG: f64 = 800.0;

/// Cost to manufacture an engine of given mass and propellant type.
pub fn engine_material_cost(preset: PropellantPreset, engine_mass_kg: f64) -> f64 {
    engine_bom(preset).material_cost(engine_mass_kg)
}

/// Cost for tank/structure manufacturing of a stage.
pub fn tank_material_cost(structural_mass_kg: f64) -> f64 {
    tank_bom().material_cost(structural_mass_kg)
}

/// Fixed cost for stage assembly (wiring, avionics, etc.).
pub fn stage_assembly_cost() -> f64 {
    stage_assembly_bom().material_cost(STAGE_ASSEMBLY_MASS_KG)
}

/// Fixed cost for final rocket integration.
pub fn rocket_integration_cost() -> f64 {
    rocket_integration_bom().material_cost(ROCKET_INTEGRATION_MASS_KG)
}

/// Format a dollar amount for display (e.g. "$1.5M", "$300K").
pub fn format_money(amount: f64) -> String {
    if amount >= 1_000_000_000.0 {
        format!("${:.1}B", amount / 1_000_000_000.0)
    } else if amount >= 1_000_000.0 {
        format!("${:.1}M", amount / 1_000_000.0)
    } else if amount >= 1_000.0 {
        format!("${:.0}K", amount / 1_000.0)
    } else if amount < 0.0 {
        if amount <= -1_000_000_000.0 {
            format!("-${:.1}B", (-amount) / 1_000_000_000.0)
        } else if amount <= -1_000_000.0 {
            format!("-${:.1}M", (-amount) / 1_000_000.0)
        } else if amount <= -1_000.0 {
            format!("-${:.0}K", (-amount) / 1_000.0)
        } else {
            format!("-${:.0}", -amount)
        }
    } else {
        format!("${:.0}", amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_prices() {
        assert_eq!(Resource::Aluminium.price_per_kg(), 5.0);
        assert_eq!(Resource::Electronics.price_per_kg(), 20_000.0);
        assert_eq!(Resource::SolidPropellant.price_per_kg(), 15.0);
    }

    #[test]
    fn test_bom_fractions_sum_to_one() {
        let boms = vec![
            engine_bom(PropellantPreset::Kerolox),
            engine_bom(PropellantPreset::Hydrolox),
            engine_bom(PropellantPreset::Hypergolic),
            engine_bom(PropellantPreset::Solid),
            tank_bom(),
            stage_assembly_bom(),
            rocket_integration_bom(),
        ];
        for (i, bom) in boms.iter().enumerate() {
            let sum: f64 = bom.fractions.iter().map(|(_, f)| f).sum();
            assert!((sum - 1.0).abs() < 0.01,
                "BOM {} fractions sum to {} (expected ~1.0)", i, sum);
        }
    }

    #[test]
    fn test_engine_material_cost_kerolox() {
        let cost = engine_material_cost(PropellantPreset::Kerolox, 500.0);
        // 500kg engine: superalloys(200kg*$80) + steel(125*$3) + alu(50*$5) + plumb(50*$1500) + ...
        // Should be in the tens-to-hundreds of thousands range
        assert!(cost > 50_000.0, "Kerolox engine cost {} too low", cost);
        assert!(cost < 500_000.0, "Kerolox engine cost {} too high", cost);
    }

    #[test]
    fn test_engine_material_cost_solid_higher() {
        // Solid engines include solid propellant in the casing mass
        let solid = engine_material_cost(PropellantPreset::Solid, 1000.0);
        let kerolox = engine_material_cost(PropellantPreset::Kerolox, 1000.0);
        // Solid should be cheaper (less superalloy, more cheap steel/propellant)
        assert!(solid < kerolox, "Solid {} should be cheaper than kerolox {}", solid, kerolox);
    }

    #[test]
    fn test_tank_material_cost() {
        let cost = tank_material_cost(2000.0);
        // Includes electronics at $20K/kg: 2000 * 0.002 * 20000 = $80K, plus plumbing, etc.
        assert!(cost > 50_000.0 && cost < 500_000.0,
            "Tank cost {} out of range for 2000kg structure", cost);
    }

    #[test]
    fn test_stage_assembly_cost() {
        let cost = stage_assembly_cost();
        // 500kg: electronics(50kg*$20K=$1M) + wiring(150kg*$150) + plumbing(50kg*$1.5K) + ...
        assert!(cost > 100_000.0 && cost < 2_000_000.0,
            "Stage assembly cost {} out of range", cost);
    }

    #[test]
    fn test_rocket_integration_cost() {
        let cost = rocket_integration_cost();
        // 800kg: electronics(40kg*$20K=$800K) + wiring(120kg*$150) + ...
        assert!(cost > 100_000.0 && cost < 2_000_000.0,
            "Integration cost {} out of range", cost);
    }

    #[test]
    fn test_resource_masses() {
        let bom = engine_bom(PropellantPreset::Kerolox);
        let masses = bom.resource_masses(1000.0);
        let total: f64 = masses.iter().map(|(_, m)| m).sum();
        assert!((total - 1000.0).abs() < 1.0, "Total mass {} should be ~1000", total);
    }

    #[test]
    fn test_all_resources_have_names() {
        for r in Resource::ALL {
            assert!(!r.name().is_empty());
        }
    }
}
