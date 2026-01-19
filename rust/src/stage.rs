use crate::engine::{EngineSpec, EngineType};

/// A stage in a rocket design
#[derive(Debug, Clone)]
pub struct RocketStage {
    /// Type of engine used in this stage
    pub engine_type: EngineType,
    /// Number of engines in this stage
    pub engine_count: u32,
    /// Mass of propellant in kilograms
    pub propellant_mass_kg: f64,
}

impl RocketStage {
    /// Create a new rocket stage with default values
    pub fn new(engine_type: EngineType) -> Self {
        Self {
            engine_type,
            engine_count: 1,
            propellant_mass_kg: 1000.0, // Default starting propellant
        }
    }

    /// Get the engine specification for this stage
    pub fn engine_spec(&self) -> EngineSpec {
        self.engine_type.spec()
    }

    /// Calculate the dry mass of this stage (engines only, no propellant)
    pub fn dry_mass_kg(&self) -> f64 {
        self.engine_spec().total_mass_kg(self.engine_count)
    }

    /// Calculate the wet mass of this stage (engines + propellant)
    pub fn wet_mass_kg(&self) -> f64 {
        self.dry_mass_kg() + self.propellant_mass_kg
    }

    /// Calculate the exhaust velocity (same for all engines of this type)
    pub fn exhaust_velocity_ms(&self) -> f64 {
        self.engine_spec().exhaust_velocity_ms
    }

    /// Calculate total thrust in kN
    pub fn total_thrust_kn(&self) -> f64 {
        self.engine_spec().total_thrust_kn(self.engine_count)
    }

    /// Calculate failure rate for stage ignition
    /// All engines must ignite successfully
    pub fn ignition_failure_rate(&self) -> f64 {
        self.engine_spec().stage_failure_rate(self.engine_count)
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
    /// This is what the slider controls
    ///
    /// # Arguments
    /// * `payload_mass_kg` - Mass above this stage
    pub fn mass_fraction(&self, payload_mass_kg: f64) -> f64 {
        let total = self.wet_mass_kg() + payload_mass_kg;
        self.propellant_mass_kg / total
    }

    /// Set propellant mass from a desired mass fraction
    ///
    /// # Arguments
    /// * `fraction` - Desired mass fraction (0.0 to 1.0, typically 0.5 to 0.95)
    /// * `payload_mass_kg` - Mass above this stage
    pub fn set_mass_fraction(&mut self, fraction: f64, payload_mass_kg: f64) {
        // mass_fraction = propellant / (propellant + dry_mass + payload)
        // fraction * (propellant + dry_mass + payload) = propellant
        // fraction * dry_mass + fraction * payload = propellant - fraction * propellant
        // fraction * dry_mass + fraction * payload = propellant * (1 - fraction)
        // propellant = fraction * (dry_mass + payload) / (1 - fraction)

        let fraction = fraction.clamp(0.01, 0.99); // Prevent division by zero
        let dry_mass = self.dry_mass_kg();
        self.propellant_mass_kg = fraction * (dry_mass + payload_mass_kg) / (1.0 - fraction);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_stage() {
        let stage = RocketStage::new(EngineType::Kerolox);
        assert_eq!(stage.engine_type, EngineType::Kerolox);
        assert_eq!(stage.engine_count, 1);
        assert_eq!(stage.propellant_mass_kg, 1000.0);
    }

    #[test]
    fn test_dry_mass() {
        let mut stage = RocketStage::new(EngineType::Kerolox);
        stage.engine_count = 3;
        // 3 Kerolox engines at 450 kg each
        assert_eq!(stage.dry_mass_kg(), 1350.0);
    }

    #[test]
    fn test_wet_mass() {
        let mut stage = RocketStage::new(EngineType::Hydrolox);
        stage.engine_count = 2;
        stage.propellant_mass_kg = 5000.0;
        // 2 Hydrolox at 300 kg each = 600 kg dry + 5000 kg propellant
        assert_eq!(stage.wet_mass_kg(), 5600.0);
    }

    #[test]
    fn test_delta_v_calculation() {
        let mut stage = RocketStage::new(EngineType::Hydrolox);
        stage.engine_count = 1;
        stage.propellant_mass_kg = 2700.0; // Will give nice numbers

        // Hydrolox: Ve = 4500 m/s
        // Engine mass: 300 kg
        // Wet mass: 3000 kg
        // With 1000 kg payload:
        // m0 = 4000, mf = 1300
        // Δv = 4500 * ln(4000/1300) = 4500 * ln(3.077) = 4500 * 1.124 = ~5058 m/s

        let delta_v = stage.delta_v(1000.0);
        assert!(delta_v > 5000.0 && delta_v < 5200.0);
    }

    #[test]
    fn test_mass_fraction() {
        let mut stage = RocketStage::new(EngineType::Kerolox);
        stage.engine_count = 1;
        stage.propellant_mass_kg = 4050.0;

        // Dry mass: 450 kg
        // Wet mass: 4500 kg
        // With 500 kg payload: total = 5000 kg
        // Fraction = 4050 / 5000 = 0.81
        let fraction = stage.mass_fraction(500.0);
        assert!((fraction - 0.81).abs() < 0.001);
    }

    #[test]
    fn test_set_mass_fraction() {
        let mut stage = RocketStage::new(EngineType::Kerolox);
        stage.engine_count = 1;
        let payload = 500.0;

        // Set to 80% mass fraction
        stage.set_mass_fraction(0.80, payload);

        // Verify it's close
        let actual_fraction = stage.mass_fraction(payload);
        assert!((actual_fraction - 0.80).abs() < 0.001);
    }

    #[test]
    fn test_ignition_failure_rate() {
        let mut stage = RocketStage::new(EngineType::Kerolox);

        // Single engine: 0.7%
        stage.engine_count = 1;
        assert!((stage.ignition_failure_rate() - 0.007).abs() < 0.0001);

        // Three engines: 1 - 0.993^3 ≈ 2.08%
        stage.engine_count = 3;
        let expected = 1.0 - 0.993_f64.powi(3);
        assert!((stage.ignition_failure_rate() - expected).abs() < 0.0001);
    }

    #[test]
    fn test_total_thrust() {
        let mut stage = RocketStage::new(EngineType::Kerolox);
        stage.engine_count = 5;
        // 5 × 1000 kN = 5000 kN
        assert_eq!(stage.total_thrust_kn(), 5000.0);
    }
}
