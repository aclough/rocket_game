use crate::engine::costs;
use crate::engine_design::EngineDesignSnapshot;
use crate::rocket_design::calculate_gravity_loss;

/// A stage in a rocket design
#[derive(Debug, Clone)]
pub struct RocketStage {
    /// ID of the engine design used in this stage
    pub engine_design_id: usize,
    /// Cached snapshot of engine stats
    engine_snapshot: EngineDesignSnapshot,
    /// Number of engines in this stage
    pub engine_count: u32,
    /// Mass of propellant in kilograms
    pub propellant_mass_kg: f64,
    /// Whether this stage is a booster that fires in parallel with the stage above it
    /// When true, this stage fires simultaneously with the stage at index-1
    pub is_booster: bool,
}

impl RocketStage {
    /// Create a new rocket stage with default values
    pub fn new(snapshot: EngineDesignSnapshot) -> Self {
        let engine_design_id = snapshot.engine_design_id;
        let mut stage = Self {
            engine_design_id,
            engine_snapshot: snapshot,
            engine_count: 1,
            propellant_mass_kg: 1000.0, // Default starting propellant (will be overridden for solids)
            is_booster: false,
        };
        // For solid motors, set the propellant mass based on fixed mass ratio
        stage.update_solid_propellant();
        stage
    }

    /// Get a reference to the engine snapshot
    pub fn engine_snapshot(&self) -> &EngineDesignSnapshot {
        &self.engine_snapshot
    }

    /// Update the engine snapshot (e.g. after a design revision)
    pub fn update_snapshot(&mut self, snapshot: EngineDesignSnapshot) {
        self.engine_design_id = snapshot.engine_design_id;
        self.engine_snapshot = snapshot;
        self.update_solid_propellant();
    }

    /// Check if this stage uses solid rocket motors
    pub fn is_solid(&self) -> bool {
        self.engine_snapshot.is_solid
    }

    /// Update propellant mass for solid motors based on engine count and fixed mass ratio
    /// Should be called whenever engine_count changes for solid stages
    fn update_solid_propellant(&mut self) {
        if let Some(mass_ratio) = self.engine_snapshot.fixed_mass_ratio {
            // For solid motors: propellant = dry_mass * mass_ratio / (1 - mass_ratio)
            // where dry_mass is the casing mass (engine_mass_kg)
            let dry_mass = self.engine_mass_kg();
            self.propellant_mass_kg = dry_mass * mass_ratio / (1.0 - mass_ratio);
        }
    }

    /// Calculate the mass of engines in this stage
    pub fn engine_mass_kg(&self) -> f64 {
        self.engine_snapshot.mass_kg * self.engine_count as f64
    }

    /// Calculate the structural mass of tanks (walls, insulation, plumbing)
    /// Uses engine-type-specific ratio (Hydrolox needs bigger tanks for low-density LH2)
    /// For solid motors, returns 0 (casing is already in engine_mass_kg)
    pub fn tank_mass_kg(&self) -> f64 {
        if self.is_solid() {
            0.0 // Solid motor casing is already included in engine_mass_kg
        } else {
            self.propellant_mass_kg * self.engine_snapshot.tank_mass_ratio
        }
    }

    /// Calculate the dry mass of this stage (engines + tank structure, no propellant)
    pub fn dry_mass_kg(&self) -> f64 {
        self.engine_mass_kg() + self.tank_mass_kg()
    }

    /// Calculate the wet mass of this stage (engines + tanks + propellant)
    pub fn wet_mass_kg(&self) -> f64 {
        self.dry_mass_kg() + self.propellant_mass_kg
    }

    /// Get the propellant mass, recalculating for solid motors
    pub fn get_propellant_mass(&self) -> f64 {
        if let Some(mass_ratio) = self.engine_snapshot.fixed_mass_ratio {
            // For solid motors: propellant = dry_mass * mass_ratio / (1 - mass_ratio)
            let dry_mass = self.engine_mass_kg();
            dry_mass * mass_ratio / (1.0 - mass_ratio)
        } else {
            self.propellant_mass_kg
        }
    }

    /// Calculate the exhaust velocity (same for all engines of this type)
    pub fn exhaust_velocity_ms(&self) -> f64 {
        self.engine_snapshot.exhaust_velocity_ms
    }

    /// Calculate total thrust in kN
    pub fn total_thrust_kn(&self) -> f64 {
        self.engine_snapshot.thrust_kn * self.engine_count as f64
    }

    /// Calculate delta-v this stage provides given the mass it's pushing
    ///
    /// # Arguments
    /// * `payload_mass_kg` - Mass above this stage (payload + upper stages)
    ///
    /// # Returns
    /// Delta-v in m/s using Tsiolkovsky rocket equation
    pub fn delta_v(&self, payload_mass_kg: f64) -> f64 {
        let m0 = self.wet_mass_kg() + payload_mass_kg; // Initial mass
        let mf = self.dry_mass_kg() + payload_mass_kg; // Final mass (propellant burned)
        let ve = self.exhaust_velocity_ms();

        // Tsiolkovsky: Δv = Ve × ln(m0/mf)
        ve * (m0 / mf).ln()
    }

    /// Calculate mass fraction (propellant / (propellant + dry mass + payload above))
    /// This is what the slider controls (for liquid engines)
    /// For solid motors, returns the fixed mass ratio
    ///
    /// # Arguments
    /// * `payload_mass_kg` - Mass above this stage
    pub fn mass_fraction(&self, payload_mass_kg: f64) -> f64 {
        let total = self.wet_mass_kg() + payload_mass_kg;
        self.get_propellant_mass() / total
    }

    /// Set propellant mass from a desired mass fraction
    /// For solid motors, this is a no-op (mass ratio is fixed)
    ///
    /// # Arguments
    /// * `fraction` - Desired mass fraction (0.0 to 1.0, typically 0.5 to 0.95)
    /// * `payload_mass_kg` - Mass above this stage
    pub fn set_mass_fraction(&mut self, fraction: f64, payload_mass_kg: f64) {
        // Solid motors have fixed mass ratio - ignore attempts to change it
        if self.is_solid() {
            return;
        }

        // mass_fraction = propellant / (engine_mass + tank_mass + propellant + payload)
        // where tank_mass = propellant * tank_ratio (engine-type specific)
        //
        // Let e = engine_mass, p = propellant, L = payload, t = tank ratio
        // fraction = p / (e + t*p + p + L) = p / (e + p*(1+t) + L)
        //
        // Solving for p:
        // f * (e + p*(1+t) + L) = p
        // f*e + f*L = p - f*p*(1+t) = p * (1 - f*(1+t))
        // p = f*(e + L) / (1 - f*(1+t))

        let fraction = fraction.clamp(0.01, 0.99); // Prevent division by zero
        let engine_mass = self.engine_mass_kg();
        let t = self.engine_snapshot.tank_mass_ratio;
        let denominator = 1.0 - fraction * (1.0 + t);

        if denominator > 0.01 {
            self.propellant_mass_kg =
                fraction * (engine_mass + payload_mass_kg) / denominator;
        }
    }

    /// Set the engine count, updating propellant for solid motors
    pub fn set_engine_count(&mut self, count: u32) {
        self.engine_count = count.max(1);
        self.update_solid_propellant();
    }

    // ==========================================
    // Cost Calculations
    // ==========================================

    /// Get the propellant density for this stage's engine type in kg/m³
    pub fn propellant_density(&self) -> f64 {
        self.engine_snapshot.propellant_density
    }

    /// Calculate the tank volume required for the propellant in m³
    pub fn tank_volume_m3(&self) -> f64 {
        self.propellant_mass_kg / self.propellant_density()
    }

    /// Calculate the cost of engines for this stage in dollars
    pub fn engine_cost(&self) -> f64 {
        self.engine_snapshot.base_cost * self.engine_count as f64
    }

    /// Calculate the cost of tanks for this stage in dollars
    /// Based on tank volume required for the propellant
    /// For solid motors, returns 0 (no separate tanks)
    pub fn tank_cost(&self) -> f64 {
        if self.is_solid() {
            0.0 // Solid motors have no separate tanks
        } else {
            self.tank_volume_m3() * costs::TANK_COST_PER_M3
        }
    }

    /// Calculate the total cost of this stage in dollars
    /// Includes engines, tanks, and stage overhead
    pub fn total_cost(&self) -> f64 {
        self.engine_cost() + self.tank_cost() + costs::STAGE_OVERHEAD_COST
    }

    // ==========================================
    // Booster Attachment Calculations
    // ==========================================

    /// Get the additional structural mass for booster attachment in kg
    /// Returns 0 if this stage is not a booster
    pub fn booster_attachment_mass_kg(&self) -> f64 {
        if self.is_booster {
            costs::BOOSTER_ATTACHMENT_MASS_KG
        } else {
            0.0
        }
    }

    /// Get the additional cost for booster attachment hardware in dollars
    /// Returns 0 if this stage is not a booster
    pub fn booster_attachment_cost(&self) -> f64 {
        if self.is_booster {
            costs::BOOSTER_ATTACHMENT_COST
        } else {
            0.0
        }
    }

    /// Calculate the total cost including booster attachment if applicable
    pub fn total_cost_with_attachment(&self) -> f64 {
        self.total_cost() + self.booster_attachment_cost()
    }

    /// Calculate the dry mass including booster attachment hardware if applicable
    pub fn dry_mass_with_attachment_kg(&self) -> f64 {
        self.dry_mass_kg() + self.booster_attachment_mass_kg()
    }

    /// Calculate the wet mass including booster attachment hardware if applicable
    pub fn wet_mass_with_attachment_kg(&self) -> f64 {
        self.wet_mass_kg() + self.booster_attachment_mass_kg()
    }

    // ==========================================
    // TWR and Gravity Loss Calculations
    // ==========================================

    /// Calculate the initial thrust-to-weight ratio for this stage
    ///
    /// # Arguments
    /// * `payload_mass_kg` - Mass above this stage (payload + upper stages)
    ///
    /// # Returns
    /// TWR as a dimensionless ratio (must be > 1.0 to lift off)
    pub fn initial_twr(&self, payload_mass_kg: f64) -> f64 {
        let thrust_n = self.total_thrust_kn() * 1000.0; // kN to N
        let total_mass_kg = self.wet_mass_kg() + payload_mass_kg;
        let weight_n = total_mass_kg * costs::G0;

        if weight_n > 0.0 {
            thrust_n / weight_n
        } else {
            0.0
        }
    }

    /// Calculate the mass ratio for this stage (initial mass / final mass)
    ///
    /// # Arguments
    /// * `payload_mass_kg` - Mass above this stage
    ///
    /// # Returns
    /// Mass ratio R = m0/mf (always >= 1.0)
    pub fn mass_ratio(&self, payload_mass_kg: f64) -> f64 {
        let m0 = self.wet_mass_kg() + payload_mass_kg;
        let mf = self.dry_mass_kg() + payload_mass_kg;
        m0 / mf
    }

    /// Calculate the burn time for this stage in seconds
    ///
    /// # Returns
    /// Burn time = propellant_mass × exhaust_velocity / thrust
    pub fn burn_time_seconds(&self) -> f64 {
        let thrust_n = self.total_thrust_kn() * 1000.0;
        if thrust_n > 0.0 {
            self.propellant_mass_kg * self.exhaust_velocity_ms() / thrust_n
        } else {
            0.0
        }
    }

    /// Calculate gravity losses for this stage
    ///
    /// Uses the central `calculate_gravity_loss` function which implements:
    /// - At TWR <= 1.0: rocket can't lift off, ALL delta-v is lost
    /// - At TWR > 1.0: gravity loss scales with coefficient, exhaust velocity, and TWR
    ///
    /// # Arguments
    /// * `payload_mass_kg` - Mass above this stage
    /// * `gravity_loss_coefficient` - How much of burn is vertical (higher = more vertical)
    ///
    /// # Returns
    /// Gravity loss in m/s
    pub fn gravity_loss(&self, payload_mass_kg: f64, gravity_loss_coefficient: f64) -> f64 {
        calculate_gravity_loss(
            gravity_loss_coefficient,
            self.exhaust_velocity_ms(),
            self.mass_ratio(payload_mass_kg),
            self.initial_twr(payload_mass_kg),
            self.delta_v(payload_mass_kg),
        )
    }

    /// Calculate gravity efficiency (1.0 = no losses, 0.0 = all lost to gravity)
    ///
    /// # Arguments
    /// * `payload_mass_kg` - Mass above this stage
    /// * `gravity_loss_coefficient` - How much of burn is vertical
    ///
    /// # Returns
    /// Efficiency as a ratio (0.0 to 1.0)
    pub fn gravity_efficiency(&self, payload_mass_kg: f64, gravity_loss_coefficient: f64) -> f64 {
        let ideal_dv = self.delta_v(payload_mass_kg);
        if ideal_dv <= 0.0 {
            return 0.0;
        }

        let loss = self.gravity_loss(payload_mass_kg, gravity_loss_coefficient);
        (1.0 - loss / ideal_dv).max(0.0)
    }

    /// Calculate effective delta-v after gravity losses
    ///
    /// # Arguments
    /// * `payload_mass_kg` - Mass above this stage
    /// * `gravity_loss_coefficient` - How much of burn is vertical
    ///
    /// # Returns
    /// Effective delta-v in m/s
    pub fn effective_delta_v(&self, payload_mass_kg: f64, gravity_loss_coefficient: f64) -> f64 {
        let ideal_dv = self.delta_v(payload_mass_kg);
        let loss = self.gravity_loss(payload_mass_kg, gravity_loss_coefficient);
        (ideal_dv - loss).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_design::default_snapshot;

    fn kerolox_snapshot() -> EngineDesignSnapshot {
        default_snapshot(1) // Index 1 = Kerolox
    }

    fn hydrolox_snapshot() -> EngineDesignSnapshot {
        default_snapshot(0) // Index 0 = Hydrolox
    }

    #[test]
    fn test_new_stage() {
        let stage = RocketStage::new(kerolox_snapshot());
        assert_eq!(stage.engine_design_id, 1);
        assert_eq!(stage.engine_count, 1);
        assert_eq!(stage.propellant_mass_kg, 1000.0);
    }

    #[test]
    fn test_dry_mass() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 3;
        // 3 Kerolox engines at 450 kg each = 1350 kg
        // Default propellant 1000 kg, tank mass = 1000 × 0.06 = 60 kg (Kerolox)
        // Total dry = 1350 + 60 = 1410 kg
        assert_eq!(stage.dry_mass_kg(), 1410.0);
        assert_eq!(stage.engine_mass_kg(), 1350.0);
        assert_eq!(stage.tank_mass_kg(), 60.0);
    }

    #[test]
    fn test_wet_mass() {
        let mut stage = RocketStage::new(hydrolox_snapshot());
        stage.engine_count = 2;
        stage.propellant_mass_kg = 5000.0;
        // 2 Hydrolox at 300 kg each = 600 kg engines
        // Tank mass = 5000 × 0.10 = 500 kg (Hydrolox has higher ratio)
        // Dry mass = 600 + 500 = 1100 kg
        // Wet mass = 1100 + 5000 = 6100 kg
        assert_eq!(stage.wet_mass_kg(), 6100.0);
    }

    #[test]
    fn test_delta_v_calculation() {
        let mut stage = RocketStage::new(hydrolox_snapshot());
        stage.engine_count = 1;
        stage.propellant_mass_kg = 2700.0;

        // Hydrolox: Ve = 4500 m/s
        // Engine mass: 300 kg
        // Tank mass: 2700 × 0.10 = 270 kg
        // Dry mass: 300 + 270 = 570 kg
        // Wet mass: 570 + 2700 = 3270 kg
        // With 1000 kg payload:
        // m0 = 4270, mf = 1570
        // Δv = 4500 * ln(4270/1570) = 4500 * ln(2.72) = 4500 * 1.00 = ~4500 m/s

        let delta_v = stage.delta_v(1000.0);
        assert!(
            delta_v > 4400.0 && delta_v < 4600.0,
            "Expected ~4500 m/s, got {}",
            delta_v
        );
    }

    #[test]
    fn test_mass_fraction() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 1;
        stage.propellant_mass_kg = 4050.0;

        // Engine mass: 450 kg
        // Tank mass: 4050 × 0.06 = 243 kg (Kerolox)
        // Dry mass: 450 + 243 = 693 kg
        // Wet mass: 693 + 4050 = 4743 kg
        // With 500 kg payload: total = 5243 kg
        // Fraction = 4050 / 5243 ≈ 0.772
        let fraction = stage.mass_fraction(500.0);
        assert!(
            (fraction - 0.772).abs() < 0.01,
            "Expected ~0.772, got {}",
            fraction
        );
    }

    #[test]
    fn test_set_mass_fraction() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 1;
        let payload = 500.0;

        // Set to 70% mass fraction (achievable with tank mass overhead)
        stage.set_mass_fraction(0.70, payload);

        // Verify it's close
        let actual_fraction = stage.mass_fraction(payload);
        assert!(
            (actual_fraction - 0.70).abs() < 0.01,
            "Expected ~0.70, got {}",
            actual_fraction
        );
    }

    #[test]
    fn test_total_thrust() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 5;
        // 5 × 500 kN = 2500 kN
        assert_eq!(stage.total_thrust_kn(), 2500.0);
    }

    // ==========================================
    // Cost Tests
    // ==========================================

    #[test]
    fn test_propellant_density() {
        // Kerolox: ~1020 kg/m³
        let kerolox_stage = RocketStage::new(kerolox_snapshot());
        assert_eq!(kerolox_stage.propellant_density(), 1020.0);

        // Hydrolox: ~290 kg/m³
        let hydrolox_stage = RocketStage::new(hydrolox_snapshot());
        assert_eq!(hydrolox_stage.propellant_density(), 290.0);
    }

    #[test]
    fn test_tank_volume() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.propellant_mass_kg = 10200.0; // 10200 kg / 1020 kg/m³ = 10 m³
        assert!((stage.tank_volume_m3() - 10.0).abs() < 0.01);

        let mut hydrolox_stage = RocketStage::new(hydrolox_snapshot());
        hydrolox_stage.propellant_mass_kg = 2900.0; // 2900 kg / 290 kg/m³ = 10 m³
        assert!((hydrolox_stage.tank_volume_m3() - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_engine_cost() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 3;
        // 3 × $10M = $30M
        assert_eq!(stage.engine_cost(), 30_000_000.0);

        let mut hydrolox_stage = RocketStage::new(hydrolox_snapshot());
        hydrolox_stage.engine_count = 2;
        // 2 × $15M = $30M
        assert_eq!(hydrolox_stage.engine_cost(), 30_000_000.0);
    }

    #[test]
    fn test_tank_cost() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.propellant_mass_kg = 10200.0; // 10 m³
        // 10 m³ × $100,000/m³ = $1M
        assert!((stage.tank_cost() - 1_000_000.0).abs() < 100.0);
    }

    #[test]
    fn test_stage_total_cost() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 1;
        stage.propellant_mass_kg = 10200.0; // 10 m³

        // Engine cost: 1 × $10M = $10M
        // Tank cost: 10 m³ × $100K = $1M
        // Stage overhead: $5M
        // Total: $16M
        let expected = 10_000_000.0 + 1_000_000.0 + 5_000_000.0;
        assert!((stage.total_cost() - expected).abs() < 100.0);
    }

    #[test]
    fn test_hydrolox_tanks_more_expensive_per_kg() {
        // Same propellant mass, but hydrolox needs larger tanks
        let mut kerolox = RocketStage::new(kerolox_snapshot());
        kerolox.propellant_mass_kg = 10000.0;

        let mut hydrolox = RocketStage::new(hydrolox_snapshot());
        hydrolox.propellant_mass_kg = 10000.0;

        // Hydrolox tanks should cost more due to lower density
        // Kerolox: 10000/1020 = 9.8 m³
        // Hydrolox: 10000/290 = 34.5 m³
        assert!(hydrolox.tank_cost() > kerolox.tank_cost() * 3.0);
    }

    // ==========================================
    // TWR and Gravity Loss Tests
    // ==========================================

    #[test]
    fn test_initial_twr() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 3;
        stage.propellant_mass_kg = 25000.0;

        // Kerolox: 500 kN per engine, 450 kg per engine
        // 3 engines = 1500 kN thrust, 1350 kg engine mass
        // Tank mass = 25000 × 0.06 = 1500 kg
        // Dry mass = 1350 + 1500 = 2850 kg
        // Wet mass = 2850 + 25000 = 27850 kg
        // With 1000 kg payload: total = 28850 kg
        // Weight = 28850 × 9.81 = 283,019 N
        // Thrust = 1,500,000 N
        // TWR = 1,500,000 / 283,019 ≈ 5.30

        let twr = stage.initial_twr(1000.0);
        assert!(twr > 4.5 && twr < 6.0, "TWR should be ~5.3: {}", twr);
    }

    #[test]
    fn test_twr_decreases_with_payload() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 1;
        stage.propellant_mass_kg = 10000.0;

        let twr_light = stage.initial_twr(1000.0);
        let twr_heavy = stage.initial_twr(50000.0);

        assert!(
            twr_light > twr_heavy,
            "TWR should decrease with more payload: {} vs {}",
            twr_light,
            twr_heavy
        );
    }

    #[test]
    fn test_mass_ratio() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 1;
        stage.propellant_mass_kg = 9000.0;
        // Engine mass: 450 kg
        // Tank mass: 9000 × 0.06 = 540 kg (Kerolox)
        // Dry mass: 990 kg
        // Wet mass: 9990 kg
        // With 550 kg payload:
        // m0 = 10540 kg, mf = 1540 kg
        // R = 10540 / 1540 = 6.84

        let ratio = stage.mass_ratio(550.0);
        assert!(
            (ratio - 6.84).abs() < 0.1,
            "Mass ratio should be ~6.84: {}",
            ratio
        );
    }

    #[test]
    fn test_burn_time() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 1;
        stage.propellant_mass_kg = 10000.0;

        // Kerolox: Ve = 3000 m/s, Thrust = 500 kN = 500,000 N
        // burn_time = propellant_mass * Ve / thrust
        // burn_time = 10000 * 3000 / 500,000 = 60 seconds

        let burn_time = stage.burn_time_seconds();
        assert!((burn_time - 60.0).abs() < 0.1, "Burn time should be 60s: {}", burn_time);
    }

    #[test]
    fn test_gravity_loss_increases_with_lower_twr() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.propellant_mass_kg = 10000.0;
        let payload = 5000.0;
        let coefficient = 0.85;

        // With 3 engines (higher TWR)
        stage.engine_count = 3;
        let loss_high_twr = stage.gravity_loss(payload, coefficient);
        let twr_high = stage.initial_twr(payload);

        // With 1 engine (lower TWR, same mass ratio)
        stage.engine_count = 1;
        let loss_low_twr = stage.gravity_loss(payload, coefficient);
        let twr_low = stage.initial_twr(payload);

        assert!(
            twr_high > twr_low,
            "3 engines should have higher TWR: {} vs {}",
            twr_high,
            twr_low
        );
        assert!(
            loss_low_twr > loss_high_twr,
            "Lower TWR ({:.2}) should have higher gravity losses: {:.2} vs {:.2}",
            twr_low,
            loss_low_twr,
            loss_high_twr
        );
    }

    #[test]
    fn test_effective_delta_v_less_than_ideal() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 3;
        stage.propellant_mass_kg = 25000.0;

        let ideal_dv = stage.delta_v(5000.0);
        let effective_dv = stage.effective_delta_v(5000.0, 0.85);

        assert!(
            effective_dv < ideal_dv,
            "Effective delta-v should be less than ideal: {} vs {}",
            effective_dv,
            ideal_dv
        );
        assert!(
            effective_dv > 0.5 * ideal_dv,
            "Effective delta-v shouldn't be too much less: {} vs {}",
            effective_dv,
            ideal_dv
        );
    }

    #[test]
    fn test_gravity_efficiency() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 3;
        stage.propellant_mass_kg = 25000.0;

        let efficiency = stage.gravity_efficiency(5000.0, 0.85);

        // With good TWR, efficiency should be fairly high (>50%)
        assert!(
            efficiency > 0.5 && efficiency < 1.0,
            "Gravity efficiency should be reasonable: {}",
            efficiency
        );
    }

    #[test]
    fn test_zero_gravity_loss_when_coefficient_zero() {
        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 1;
        stage.propellant_mass_kg = 10000.0;

        // With coefficient = 0 (fully horizontal burn), no gravity loss
        let loss = stage.gravity_loss(5000.0, 0.0);
        assert!(loss.abs() < 0.001, "Zero coefficient should mean zero loss: {}", loss);
    }

    #[test]
    fn test_upper_stage_low_twr_still_provides_delta_v() {
        // Upper stages have low gravity coefficients (0.01-0.03) because they're
        // burning mostly horizontally in orbit. Even with TWR < 1.0, they should
        // still provide most of their delta-v.

        let mut stage = RocketStage::new(hydrolox_snapshot());
        stage.engine_count = 1;
        stage.propellant_mass_kg = 20000.0;

        // Set up a very heavy payload to get TWR well below 1.0
        let heavy_payload = 200000.0; // 200 tons
        let twr = stage.initial_twr(heavy_payload);
        assert!(twr < 0.5, "Should have very low TWR: {}", twr);

        let ideal_dv = stage.delta_v(heavy_payload);

        // Upper stage coefficient (nearly horizontal burn in orbit)
        let upper_stage_coefficient = 0.01;
        let effective_dv = stage.effective_delta_v(heavy_payload, upper_stage_coefficient);

        // Should retain 99% of delta-v even with terrible TWR
        let efficiency = effective_dv / ideal_dv;
        assert!(
            efficiency > 0.98,
            "Upper stage should retain >98% delta-v even with low TWR. Got {}% (TWR={})",
            efficiency * 100.0,
            twr
        );
    }

    #[test]
    fn test_adding_propellant_at_twr_one_doesnt_increase_effective_dv() {
        // At TWR = 1.0, the rocket is at the margin where it can just barely lift off.
        // Adding more propellant lowers TWR (more mass, same thrust) and increases
        // gravity losses proportionally, so effective delta-v should not increase.

        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 1;

        // Kerolox: 500 kN thrust per engine
        // For TWR = 1.0: thrust = total_mass × g
        // 500,000 N = total_mass × 9.81
        // total_mass = 50,968 kg

        // Set up initial propellant
        stage.propellant_mass_kg = 10000.0;
        let wet_mass = stage.wet_mass_kg();

        // Calculate payload that gives TWR = 1.0
        // TWR = thrust / ((wet_mass + payload) × g) = 1.0
        // payload = thrust/g - wet_mass
        let thrust_n = stage.total_thrust_kn() * 1000.0;
        let payload_for_twr_one = thrust_n / costs::G0 - wet_mass;

        // Verify we actually have TWR = 1.0
        let twr = stage.initial_twr(payload_for_twr_one);
        assert!(
            (twr - 1.0).abs() < 0.001,
            "TWR should be 1.0: {}",
            twr
        );

        // Calculate effective delta-v at TWR = 1.0
        let gravity_coefficient = 0.85; // Typical first stage value
        let effective_dv_before = stage.effective_delta_v(payload_for_twr_one, gravity_coefficient);

        // Add more propellant (this will lower TWR below 1.0)
        let additional_propellant = 5000.0;
        stage.propellant_mass_kg += additional_propellant;

        // Recalculate payload to maintain the same total payload
        // (the original payload mass doesn't change, but now we have more propellant)
        // TWR will now be < 1.0

        let new_twr = stage.initial_twr(payload_for_twr_one);
        assert!(
            new_twr < 1.0,
            "Adding propellant should lower TWR below 1.0: {}",
            new_twr
        );

        // Calculate new effective delta-v
        let effective_dv_after = stage.effective_delta_v(payload_for_twr_one, gravity_coefficient);

        // The key assertion: adding propellant at TWR <= 1.0 should not increase effective delta-v
        // In fact, it should decrease or stay roughly the same
        assert!(
            effective_dv_after <= effective_dv_before + 1.0, // Small tolerance for floating point
            "Adding propellant at TWR=1.0 should not increase effective delta-v. Before: {}, After: {}, TWR went from 1.0 to {}",
            effective_dv_before,
            effective_dv_after,
            new_twr
        );
    }
}
