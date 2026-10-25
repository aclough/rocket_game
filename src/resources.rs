use serde::{Serialize, Deserialize};

use crate::balance_config::{CostsConfig, ResourcePrices};
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

}

/// A bill of materials: fraction of total mass for each resource.
#[derive(Debug, Clone)]
pub struct BillOfMaterials {
    /// (resource, mass_fraction) pairs — fractions should sum to ~1.0.
    pub fractions: Vec<(Resource, f64)>,
}

impl BillOfMaterials {
    /// Compute the material cost for a given total mass.
    pub fn material_cost(&self, mass_kg: f64, prices: &ResourcePrices) -> f64 {
        self.fractions.iter()
            .map(|(r, frac)| mass_kg * frac * prices.price_per_kg(*r))
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
        // Nuclear thermal: reactor core (HEU), shielding, hydrogen plumbing.
        // The uranium is a small part of the engine: the 1972 SNRE design
        // carried ~60 kg of U in a 1,160 kg core, reflector and drums
        // (21_1_ENGINE_SURVEY.md), so ~0.6 % of a 10 t engine. The rest of
        // the core — graphite/carbide matrix, reflector, drums — is
        // superalloy-priced structure. (The table summed to 0.95 before;
        // the missing 5 % is structure too.)
        PropellantPreset::Hydrogen => BillOfMaterials {
            fractions: vec![
                (Resource::HEU, 0.006),
                (Resource::Superalloys, 0.544),
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

/// BOM for stage assembly hardware: wiring harnesses, avionics,
/// umbilicals. Its mass is `CostsConfig::assembly_floor_kg` plus a
/// share of the stage's structure (24_STRATEGIES.md §2.1).
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

/// BOM for final rocket integration: interstage adapters, fairing
/// hardware, final wiring. Its mass is `CostsConfig::integration_floor_kg`
/// plus a share of the vehicle's dry mass.
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

/// The materials floor of an engine's cost: its mass priced through its
/// bill of materials. What a complex or hydrogen engine costs beyond
/// this is labour — build work (`WorkConfig::engine_build_work`) at
/// the line's daily salary (21_ENGINES.md step 5).
pub fn engine_material_cost(
    preset: PropellantPreset,
    engine_mass_kg: f64,
    prices: &ResourcePrices,
) -> f64 {
    engine_bom(preset).material_cost(engine_mass_kg, prices)
}

/// Cost for tank/structure manufacturing of a stage.
pub fn tank_material_cost(structural_mass_kg: f64, prices: &ResourcePrices) -> f64 {
    tank_bom().material_cost(structural_mass_kg, prices)
}

/// Mass of a stage's assembly hardware: a floor plus a share of its
/// structure up to a cap, so a 150 kg upper stage is not wired like a
/// 6 t booster and a 26 t core is wired like one.
pub fn stage_assembly_mass_kg(structural_mass_kg: f64, costs: &CostsConfig) -> f64 {
    costs.assembly_floor_kg + costs.assembly_fraction * structural_mass_kg.min(costs.assembly_cap_kg)
}

/// Material cost of a stage's assembly hardware (wiring, avionics, etc.).
pub fn stage_assembly_cost(structural_mass_kg: f64, costs: &CostsConfig) -> f64 {
    stage_assembly_bom().material_cost(stage_assembly_mass_kg(structural_mass_kg, costs), &costs.resource_prices)
}

/// Mass of a vehicle's integration hardware: a floor plus a share of
/// its dry mass up to a cap.
pub fn rocket_integration_mass_kg(dry_mass_kg: f64, costs: &CostsConfig) -> f64 {
    costs.integration_floor_kg + costs.integration_fraction * dry_mass_kg.min(costs.integration_cap_kg)
}

/// Material cost of final rocket integration for a vehicle of
/// `dry_mass_kg` (every stage's dry mass, engines included).
pub fn rocket_integration_cost(dry_mass_kg: f64, costs: &CostsConfig) -> f64 {
    rocket_integration_bom().material_cost(rocket_integration_mass_kg(dry_mass_kg, costs), &costs.resource_prices)
}

/// Format a dollar amount for display (e.g. "$1.5M", "$300K").
/// Format a money amount with comma thousands separators, e.g. $1,234,567.
/// Used where exact amounts matter more than compactness (event log).
pub fn format_money_exact(amount: f64) -> String {
    let n = amount.round() as i64;
    let s = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-${}", out) } else { format!("${}", out) }
}

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
        let prices = ResourcePrices::default();
        assert_eq!(prices.price_per_kg(Resource::Aluminium), 20.0);
        assert_eq!(prices.price_per_kg(Resource::Electronics), 80_000.0);
        assert_eq!(prices.price_per_kg(Resource::SolidPropellant), 60.0);
    }

    #[test]
    fn test_bom_fractions_sum_to_one() {
        let boms = [
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
        let cost = engine_material_cost(PropellantPreset::Kerolox, 500.0, &ResourcePrices::default());
        // 500 kg of superalloy, steel, plumbing, wiring: hundreds of
        // thousands, not millions — labour carries the rest.
        assert!(cost > 200_000.0, "Kerolox engine cost {} too low", cost);
        assert!(cost < 2_000_000.0, "Kerolox engine cost {} too high", cost);
    }

    #[test]
    fn test_engine_material_cost_solid_higher() {
        // Solid engines include solid propellant in the casing mass
        let solid = engine_material_cost(PropellantPreset::Solid, 1000.0, &ResourcePrices::default());
        let kerolox = engine_material_cost(PropellantPreset::Kerolox, 1000.0, &ResourcePrices::default());
        // Solid should be cheaper (less superalloy, more cheap steel/propellant).
        assert!(solid < kerolox, "Solid {} should be cheaper than kerolox {}", solid, kerolox);
    }

    #[test]
    fn test_engine_materials_carry_no_premium() {
        // Hydrolox hardware is a slightly pricier BOM, not a multiple:
        // its premium is build work (PropellantHandlingConfig).
        let prices = ResourcePrices::default();
        let kerolox = engine_material_cost(PropellantPreset::Kerolox, 1000.0, &prices);
        let hydrolox = engine_material_cost(PropellantPreset::Hydrolox, 1000.0, &prices);
        assert!(hydrolox > kerolox && hydrolox < kerolox * 1.3, "{hydrolox} vs {kerolox}");
    }

    #[test]
    fn test_tank_material_cost() {
        let cost = tank_material_cost(2000.0, &ResourcePrices::default());
        // Includes electronics at $80K/kg: 2000 * 0.002 * 80000 = $320K, plus plumbing, etc.
        assert!(cost > 200_000.0 && cost < 2_000_000.0,
            "Tank cost {} out of range for 2000kg structure", cost);
    }

    /// 24_STRATEGIES.md §2.1: assembly and integration hardware scale
    /// with the vehicle. The bot's 6 450 kg booster stage pays about
    /// what the old flat 500 kg did (834 kg of hardware, ~$7.3M); an
    /// Electron-class 150 kg upper stage pays a tenth of that.
    #[test]
    fn test_stage_assembly_cost_scales_with_the_stage() {
        let costs = CostsConfig::default();
        let booster = stage_assembly_cost(6_450.0, &costs);
        let small = stage_assembly_cost(150.0, &costs);
        assert!((stage_assembly_mass_kg(6_450.0, &costs) - 834.0).abs() < 1e-9);
        assert!(booster > 6_000_000.0 && booster < 8_500_000.0, "booster assembly {booster}");
        assert!(small < booster / 9.0, "small stage assembly {small} vs booster {booster}");
        assert!(small > 400_000.0, "the avionics floor is never free: {small}");
        // The incumbent's 26 t core is wired like a 7 t stage.
        let core = stage_assembly_cost(26_000.0, &costs);
        assert!((core - stage_assembly_cost(7_000.0, &costs)).abs() < 1e-6, "core {core}");
        assert!(core < booster * 1.1, "core {core} vs booster {booster}");
    }

    #[test]
    fn test_rocket_integration_cost_scales_with_dry_mass() {
        let costs = CostsConfig::default();
        // The bot's vehicle: ~10 t dry.
        let medium = rocket_integration_cost(10_044.0, &costs);
        let small = rocket_integration_cost(2_372.0, &costs);
        assert!((rocket_integration_mass_kg(10_044.0, &costs) - 702.64).abs() < 1e-6);
        assert!(medium > 2_500_000.0 && medium < 4_000_000.0, "medium integration {medium}");
        assert!(small < medium / 2.5, "small integration {small} vs medium {medium}");
    }

    #[test]
    fn test_resource_masses() {
        let bom = engine_bom(PropellantPreset::Kerolox);
        let masses = bom.resource_masses(1000.0);
        let total: f64 = masses.iter().map(|(_, m)| m).sum();
        assert!((total - 1000.0).abs() < 1.0, "Total mass {} should be ~1000", total);
    }

    /// Every engine bill of materials accounts for the whole engine.
    #[test]
    fn engine_boms_sum_to_one() {
        for preset in PropellantPreset::ALL {
            let sum: f64 = engine_bom(*preset).fractions.iter().map(|(_, f)| f).sum();
            assert!((sum - 1.0).abs() < 1e-9, "{preset:?} BOM sums to {sum}");
        }
    }

    /// A NERVA-class engine carries tens of kilograms of uranium, not
    /// tonnes: its unit cost is tens of millions, not hundreds.
    #[test]
    fn nuclear_thermal_uranium_is_a_small_share() {
        let heu_kg: f64 = engine_bom(PropellantPreset::Hydrogen).resource_masses(10_000.0)
            .iter().filter(|(r, _)| *r == Resource::HEU).map(|(_, m)| m).sum();
        assert!((40.0..=80.0).contains(&heu_kg), "HEU in a 10 t engine: {heu_kg} kg");
        let bal = crate::balance_config::BalanceConfig::default();
        let cost = engine_material_cost(PropellantPreset::Hydrogen, 10_000.0, &bal.costs.resource_prices);
        assert!(cost > 10e6 && cost < 60e6, "NTR material cost ${:.1}M", cost / 1e6);
    }

    #[test]
    fn test_all_resources_have_names() {
        for r in Resource::ALL {
            assert!(!r.name().is_empty());
        }
    }
}
