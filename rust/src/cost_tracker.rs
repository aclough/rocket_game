/// Per-design cost tracking: NRE (engineering salary + hardware test costs),
/// production material costs, and average cost per flight.

#[derive(Debug, Clone, Default)]
pub struct CostTracker {
    /// Cumulative engineering salary attributed to this design
    pub engineering_salary_spent: f64,
    /// Cost of engines consumed in hardware testing (NRE)
    pub hardware_test_cost: f64,
    /// Total material cost of all units produced (engines or rockets)
    pub total_production_material_cost: f64,
    /// Number of units produced (manufactured)
    pub units_produced: u32,
}

impl CostTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Non-Recurring Engineering cost: salary + hardware test costs
    pub fn nre(&self) -> f64 {
        self.engineering_salary_spent + self.hardware_test_cost
    }

    /// Total cost: NRE + all production material costs
    pub fn total_cost(&self) -> f64 {
        self.nre() + self.total_production_material_cost
    }

    /// Average cost per flight: total cost amortized over launches
    pub fn average_cost_per_flight(&self, launches: u32) -> f64 {
        if launches == 0 {
            0.0
        } else {
            self.total_cost() / launches as f64
        }
    }

    /// Average production cost per unit
    pub fn average_production_cost(&self) -> f64 {
        if self.units_produced == 0 {
            0.0
        } else {
            self.total_production_material_cost / self.units_produced as f64
        }
    }

    /// Attribute engineering salary cost
    pub fn add_salary(&mut self, amount: f64) {
        self.engineering_salary_spent += amount;
    }

    /// Attribute hardware test cost (engine consumed during testing)
    pub fn add_hardware_test_cost(&mut self, amount: f64) {
        self.hardware_test_cost += amount;
    }

    /// Record production of units with their material cost
    pub fn add_production_cost(&mut self, cost: f64, units: u32) {
        self.total_production_material_cost += cost;
        self.units_produced += units;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_tracker_new() {
        let ct = CostTracker::new();
        assert_eq!(ct.nre(), 0.0);
        assert_eq!(ct.total_cost(), 0.0);
        assert_eq!(ct.average_cost_per_flight(0), 0.0);
        assert_eq!(ct.units_produced, 0);
    }

    #[test]
    fn test_nre_calculation() {
        let mut ct = CostTracker::new();
        ct.add_salary(500_000.0);
        ct.add_hardware_test_cost(150_000.0);
        assert_eq!(ct.nre(), 650_000.0);
    }

    #[test]
    fn test_total_cost() {
        let mut ct = CostTracker::new();
        ct.add_salary(1_000_000.0);
        ct.add_hardware_test_cost(200_000.0);
        ct.add_production_cost(300_000.0, 3);
        assert_eq!(ct.total_cost(), 1_500_000.0);
        assert_eq!(ct.units_produced, 3);
    }

    #[test]
    fn test_average_cost_per_flight() {
        let mut ct = CostTracker::new();
        ct.add_salary(1_000_000.0);
        ct.add_production_cost(500_000.0, 5);
        // total = 1.5M, 5 launches => 300K each
        assert_eq!(ct.average_cost_per_flight(5), 300_000.0);
        // 0 launches => 0
        assert_eq!(ct.average_cost_per_flight(0), 0.0);
    }

    #[test]
    fn test_average_production_cost() {
        let mut ct = CostTracker::new();
        ct.add_production_cost(600_000.0, 3);
        assert_eq!(ct.average_production_cost(), 200_000.0);
        // No units => 0
        let ct2 = CostTracker::new();
        assert_eq!(ct2.average_production_cost(), 0.0);
    }

    #[test]
    fn test_incremental_additions() {
        let mut ct = CostTracker::new();
        ct.add_salary(100_000.0);
        ct.add_salary(200_000.0);
        assert_eq!(ct.engineering_salary_spent, 300_000.0);

        ct.add_hardware_test_cost(50_000.0);
        ct.add_hardware_test_cost(75_000.0);
        assert_eq!(ct.hardware_test_cost, 125_000.0);

        ct.add_production_cost(100_000.0, 1);
        ct.add_production_cost(200_000.0, 2);
        assert_eq!(ct.total_production_material_cost, 300_000.0);
        assert_eq!(ct.units_produced, 3);
    }
}
