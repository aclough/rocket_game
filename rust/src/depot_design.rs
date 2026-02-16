/// Fuel depot design system.
/// Depots store propellant at orbital locations for refueling missions.

use crate::resources::depot_resource_cost;

/// A fuel depot design that can be manufactured and deployed
#[derive(Debug, Clone)]
pub struct DepotDesign {
    pub name: String,
    /// Maximum propellant capacity in kg
    pub capacity_kg: f64,
    /// Whether the depot has cryogenic insulation (for hydrolox storage)
    pub insulated: bool,
}

impl DepotDesign {
    pub fn new(name: String, capacity_kg: f64, insulated: bool) -> Self {
        Self {
            name,
            capacity_kg,
            insulated,
        }
    }

    /// Dry mass of the depot structure in kg
    /// Base: 5% of capacity, +20% if insulated
    pub fn dry_mass_kg(&self) -> f64 {
        let base = self.capacity_kg * 0.05;
        if self.insulated {
            base * 1.20
        } else {
            base
        }
    }

    /// Material cost to build this depot
    /// +30% if insulated (specialized materials)
    pub fn material_cost(&self) -> f64 {
        let base_cost = depot_resource_cost(self.dry_mass_kg());
        if self.insulated {
            base_cost * 1.30
        } else {
            base_cost
        }
    }

    /// Work units required to build this depot
    /// Base: 30 + (capacity/1000) * 2, +20% if insulated
    pub fn build_work(&self) -> f64 {
        let base = 30.0 + (self.capacity_kg / 1000.0) * 2.0;
        if self.insulated {
            base * 1.2
        } else {
            base
        }
    }

    /// Manufacturing floor space required
    /// 1 unit, or 2 if capacity > 50,000 kg
    pub fn floor_space(&self) -> usize {
        if self.capacity_kg > 50_000.0 {
            2
        } else {
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_depot() {
        let depot = DepotDesign::new("Small Depot".to_string(), 10_000.0, false);
        assert_eq!(depot.dry_mass_kg(), 500.0); // 10000 * 0.05
        assert!(depot.material_cost() > 0.0);
        assert_eq!(depot.floor_space(), 1);
        // build_work = 30 + (10000/1000)*2 = 50
        assert!((depot.build_work() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_insulated_depot() {
        let depot = DepotDesign::new("Insulated Depot".to_string(), 10_000.0, true);
        let base_depot = DepotDesign::new("Base".to_string(), 10_000.0, false);

        // Mass 20% higher
        assert!((depot.dry_mass_kg() - base_depot.dry_mass_kg() * 1.20).abs() < 0.01);
        // Cost 30% higher (after mass increase)
        let expected_cost = depot_resource_cost(depot.dry_mass_kg()) * 1.30;
        assert!((depot.material_cost() - expected_cost).abs() < 1.0);
        // Build time 20% higher
        assert!((depot.build_work() - base_depot.build_work() * 1.2).abs() < 0.01);
    }

    #[test]
    fn test_large_depot_floor_space() {
        let small = DepotDesign::new("Small".to_string(), 50_000.0, false);
        assert_eq!(small.floor_space(), 1);

        let large = DepotDesign::new("Large".to_string(), 50_001.0, false);
        assert_eq!(large.floor_space(), 2);
    }

    #[test]
    fn test_depot_mass_scales_with_capacity() {
        let small = DepotDesign::new("S".to_string(), 10_000.0, false);
        let large = DepotDesign::new("L".to_string(), 100_000.0, false);
        assert!(large.dry_mass_kg() > small.dry_mass_kg());
        assert!((large.dry_mass_kg() / small.dry_mass_kg() - 10.0).abs() < 0.01);
    }
}
