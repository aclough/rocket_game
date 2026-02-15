use std::collections::BTreeMap;
use crate::engine_design::FuelType;

/// A fuel depot at an orbital location
#[derive(Debug, Clone)]
pub struct FuelDepot {
    pub location: String,
    pub fuel_stored: BTreeMap<FuelType, f64>,
    pub capacity_kg: f64,
}

impl FuelDepot {
    pub fn new(location: &str, capacity_kg: f64) -> Self {
        Self {
            location: location.to_string(),
            fuel_stored: BTreeMap::new(),
            capacity_kg,
        }
    }

    /// Total fuel stored across all types
    pub fn total_stored(&self) -> f64 {
        self.fuel_stored.values().sum()
    }

    /// Remaining capacity
    pub fn available_space(&self) -> f64 {
        (self.capacity_kg - self.total_stored()).max(0.0)
    }

    /// Amount of a specific fuel type stored
    pub fn stored(&self, fuel_type: FuelType) -> f64 {
        self.fuel_stored.get(&fuel_type).copied().unwrap_or(0.0)
    }

    /// Deposit fuel, capped by remaining capacity. Returns actual amount deposited.
    pub fn deposit(&mut self, fuel_type: FuelType, kg: f64) -> f64 {
        let actual = kg.min(self.available_space()).max(0.0);
        if actual > 0.0 {
            *self.fuel_stored.entry(fuel_type).or_insert(0.0) += actual;
        }
        actual
    }

    /// Withdraw fuel, capped by amount stored. Returns actual amount withdrawn.
    pub fn withdraw(&mut self, fuel_type: FuelType, kg: f64) -> f64 {
        let stored = self.stored(fuel_type);
        let actual = kg.min(stored).max(0.0);
        if actual > 0.0 {
            if let Some(entry) = self.fuel_stored.get_mut(&fuel_type) {
                *entry -= actual;
                if *entry < 1e-9 {
                    self.fuel_stored.remove(&fuel_type);
                }
            }
        }
        actual
    }
}

/// Infrastructure at an orbital location (extensible for future structures)
#[derive(Debug, Clone)]
pub struct LocationInfrastructure {
    pub depot: Option<FuelDepot>,
}

impl LocationInfrastructure {
    pub fn new() -> Self {
        Self { depot: None }
    }

    pub fn has_depot(&self) -> bool {
        self.depot.is_some()
    }

    /// Get or create a depot at this location. If a depot already exists,
    /// increases its capacity by `capacity_kg`.
    pub fn get_or_create_depot(&mut self, location: &str, capacity_kg: f64) -> &mut FuelDepot {
        if self.depot.is_none() {
            self.depot = Some(FuelDepot::new(location, capacity_kg));
        } else if let Some(depot) = &mut self.depot {
            depot.capacity_kg += capacity_kg;
        }
        self.depot.as_mut().unwrap()
    }
}

impl Default for LocationInfrastructure {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depot_deposit_withdraw() {
        let mut depot = FuelDepot::new("leo", 10000.0);
        assert_eq!(depot.total_stored(), 0.0);

        let deposited = depot.deposit(FuelType::Kerolox, 5000.0);
        assert_eq!(deposited, 5000.0);
        assert_eq!(depot.stored(FuelType::Kerolox), 5000.0);
        assert_eq!(depot.total_stored(), 5000.0);
        assert_eq!(depot.available_space(), 5000.0);

        let withdrawn = depot.withdraw(FuelType::Kerolox, 3000.0);
        assert_eq!(withdrawn, 3000.0);
        assert_eq!(depot.stored(FuelType::Kerolox), 2000.0);
    }

    #[test]
    fn test_depot_capacity_limit() {
        let mut depot = FuelDepot::new("leo", 1000.0);

        let deposited = depot.deposit(FuelType::Kerolox, 800.0);
        assert_eq!(deposited, 800.0);

        // Try to deposit more than remaining capacity
        let deposited = depot.deposit(FuelType::Hydrolox, 500.0);
        assert_eq!(deposited, 200.0);
        assert_eq!(depot.total_stored(), 1000.0);
        assert_eq!(depot.available_space(), 0.0);
    }

    #[test]
    fn test_depot_withdraw_capped() {
        let mut depot = FuelDepot::new("leo", 10000.0);
        depot.deposit(FuelType::Kerolox, 500.0);

        // Try to withdraw more than stored
        let withdrawn = depot.withdraw(FuelType::Kerolox, 1000.0);
        assert_eq!(withdrawn, 500.0);
        assert_eq!(depot.stored(FuelType::Kerolox), 0.0);

        // Withdraw from empty fuel type
        let withdrawn = depot.withdraw(FuelType::Hydrolox, 100.0);
        assert_eq!(withdrawn, 0.0);
    }

    #[test]
    fn test_location_infrastructure() {
        let mut infra = LocationInfrastructure::new();
        assert!(!infra.has_depot());

        let depot = infra.get_or_create_depot("leo", 5000.0);
        assert_eq!(depot.capacity_kg, 5000.0);
        assert!(infra.has_depot());

        // Calling again should increase capacity
        let depot = infra.get_or_create_depot("leo", 3000.0);
        assert_eq!(depot.capacity_kg, 8000.0);
    }
}
