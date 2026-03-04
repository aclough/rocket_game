use serde::{Serialize, Deserialize};

/// Factor-based reputation tracking.
///
/// Total reputation is the sum of four independent factors, each with
/// its own accumulation and decay rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reputation {
    /// +20 per successful launch, -20 per failure. Decays by 20% each launch.
    pub success_factor: f64,
    /// -50 on mission failure (payload lost). Decays by 15% each launch.
    pub lost_payload_factor: f64,
    /// -10 per year without a launch. Resets to 0 on any launch.
    pub drought_factor: f64,
    /// -10 per expired accepted contract. Decays by 20% each contract launch.
    pub expiry_factor: f64,
}

impl Default for Reputation {
    fn default() -> Self {
        Self::new()
    }
}

impl Reputation {
    pub fn new() -> Self {
        Reputation {
            success_factor: 0.0,
            lost_payload_factor: 0.0,
            drought_factor: 0.0,
            expiry_factor: 0.0,
        }
    }

    /// Current total reputation score.
    pub fn total(&self) -> f64 {
        self.success_factor + self.lost_payload_factor + self.drought_factor + self.expiry_factor
    }

    /// Called on a successful launch.
    pub fn on_launch_success(&mut self) {
        // Decay existing factors
        self.success_factor *= 0.8;
        self.lost_payload_factor *= 0.85;
        // Add success bonus
        self.success_factor += 20.0;
        // Reset drought
        self.drought_factor = 0.0;
    }

    /// Called on a failed launch (payload lost).
    pub fn on_launch_failure(&mut self) {
        // Decay existing factors
        self.success_factor *= 0.8;
        self.lost_payload_factor *= 0.85;
        // Add failure penalties
        self.success_factor -= 20.0;
        self.lost_payload_factor -= 50.0;
        // Reset drought (still launched, even if it failed)
        self.drought_factor = 0.0;
    }

    /// Called on a partially failed launch (reached near destination).
    pub fn on_launch_partial_failure(&mut self) {
        // Decay existing factors
        self.success_factor *= 0.8;
        self.lost_payload_factor *= 0.85;
        // Smaller penalty than full failure
        self.success_factor -= 10.0;
        // Reset drought
        self.drought_factor = 0.0;
    }

    /// Called when a contract launch succeeds (decays expiry factor too).
    pub fn on_contract_launch(&mut self) {
        self.expiry_factor *= 0.8;
    }

    /// Called when an accepted contract expires without successful launch.
    pub fn on_contract_expired(&mut self) {
        self.expiry_factor -= 10.0;
    }

    /// Called on each year anniversary without a launch.
    pub fn on_year_without_launch(&mut self) {
        self.drought_factor -= 10.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_reputation() {
        let rep = Reputation::new();
        assert_eq!(rep.total(), 0.0);
    }

    #[test]
    fn test_success_increases_reputation() {
        let mut rep = Reputation::new();
        rep.on_launch_success();
        assert!(rep.total() > 0.0);
        assert_eq!(rep.success_factor, 20.0);
    }

    #[test]
    fn test_failure_decreases_reputation() {
        let mut rep = Reputation::new();
        rep.on_launch_failure();
        assert!(rep.total() < 0.0);
        assert_eq!(rep.success_factor, -20.0);
        assert_eq!(rep.lost_payload_factor, -50.0);
    }

    #[test]
    fn test_success_decay() {
        let mut rep = Reputation::new();
        // Two successes: first decays by 20%, then +20
        rep.on_launch_success();
        assert_eq!(rep.success_factor, 20.0);
        rep.on_launch_success();
        // 20 * 0.8 + 20 = 36
        assert!((rep.success_factor - 36.0).abs() < 0.01);
    }

    #[test]
    fn test_drought_resets_on_launch() {
        let mut rep = Reputation::new();
        rep.on_year_without_launch();
        rep.on_year_without_launch();
        assert_eq!(rep.drought_factor, -20.0);
        rep.on_launch_success();
        assert_eq!(rep.drought_factor, 0.0);
    }

    #[test]
    fn test_contract_expiry() {
        let mut rep = Reputation::new();
        rep.on_contract_expired();
        assert_eq!(rep.expiry_factor, -10.0);
        rep.on_contract_expired();
        assert_eq!(rep.expiry_factor, -20.0);
        // Contract launch decays it
        rep.on_contract_launch();
        assert!((rep.expiry_factor - (-16.0)).abs() < 0.01);
    }

    #[test]
    fn test_recovery_from_failure() {
        let mut rep = Reputation::new();
        rep.on_launch_failure();
        let after_failure = rep.total();
        // Several successes should recover
        for _ in 0..5 {
            rep.on_launch_success();
        }
        assert!(rep.total() > after_failure);
    }
}
