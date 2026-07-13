use serde::{Serialize, Deserialize};

use crate::balance_config::ReputationConfig;

/// Factor-based reputation tracking.
///
/// Total reputation is the sum of four independent factors, each with
/// its own accumulation and decay rules. The deltas and decay factors
/// live in `balance_config::ReputationConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reputation {
    /// Gains per successful launch, loses per failure. Decays each launch.
    pub success_factor: f64,
    /// Penalized when a payload is lost. Decays each launch.
    pub lost_payload_factor: f64,
    /// Penalized per year without a launch. Resets to 0 on any launch.
    pub drought_factor: f64,
    /// Penalized per expired accepted contract. Decays each contract launch.
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
    pub fn on_launch_success(&mut self, cfg: &ReputationConfig) {
        // Decay existing factors
        self.success_factor *= cfg.success_decay;
        self.lost_payload_factor *= cfg.lost_payload_decay;
        // Add success bonus
        self.success_factor += cfg.success_gain;
        // Reset drought
        self.drought_factor = 0.0;
    }

    /// Called on a failed launch (payload lost). `severity` scales the
    /// penalties by the harshest market on the manifest (1.0 for
    /// test-mass flights).
    pub fn on_launch_failure(&mut self, cfg: &ReputationConfig, severity: f64) {
        // Decay existing factors
        self.success_factor *= cfg.success_decay;
        self.lost_payload_factor *= cfg.lost_payload_decay;
        // Add failure penalties
        self.success_factor -= cfg.failure_penalty * severity;
        self.lost_payload_factor -= cfg.lost_payload_penalty * severity;
        // Reset drought (still launched, even if it failed)
        self.drought_factor = 0.0;
    }

    /// Called on a partially failed launch (reached near destination).
    /// `severity` scales the penalty by the involved market.
    pub fn on_launch_partial_failure(&mut self, cfg: &ReputationConfig, severity: f64) {
        // Decay existing factors
        self.success_factor *= cfg.success_decay;
        self.lost_payload_factor *= cfg.lost_payload_decay;
        // Smaller penalty than full failure
        self.success_factor -= cfg.partial_failure_penalty * severity;
        // Reset drought
        self.drought_factor = 0.0;
    }

    /// Called when a contract launch succeeds (decays expiry factor too).
    pub fn on_contract_launch(&mut self, cfg: &ReputationConfig) {
        self.expiry_factor *= cfg.expiry_decay;
    }

    /// Called when an accepted contract expires without successful
    /// launch. `severity` scales the penalty by the contract's market.
    pub fn on_contract_expired(&mut self, cfg: &ReputationConfig, severity: f64) {
        self.expiry_factor -= cfg.expiry_penalty * severity;
    }

    /// Called on each year anniversary without a launch.
    pub fn on_year_without_launch(&mut self, cfg: &ReputationConfig) {
        self.drought_factor -= cfg.drought_penalty;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ReputationConfig {
        ReputationConfig::default()
    }

    #[test]
    fn test_new_reputation() {
        let rep = Reputation::new();
        assert_eq!(rep.total(), 0.0);
    }

    #[test]
    fn test_success_increases_reputation() {
        let mut rep = Reputation::new();
        rep.on_launch_success(&cfg());
        assert!(rep.total() > 0.0);
        assert_eq!(rep.success_factor, cfg().success_gain);
    }

    #[test]
    fn test_failure_decreases_reputation() {
        let mut rep = Reputation::new();
        rep.on_launch_failure(&cfg(), 1.0);
        assert!(rep.total() < 0.0);
        assert_eq!(rep.success_factor, -cfg().failure_penalty);
        assert_eq!(rep.lost_payload_factor, -cfg().lost_payload_penalty);
    }

    #[test]
    fn test_success_decay() {
        let mut rep = Reputation::new();
        // Two successes: first decays, then gains
        rep.on_launch_success(&cfg());
        assert_eq!(rep.success_factor, 20.0);
        rep.on_launch_success(&cfg());
        // 20 * 0.8 + 20 = 36
        assert!((rep.success_factor - 36.0).abs() < 0.01);
    }

    #[test]
    fn test_drought_resets_on_launch() {
        let mut rep = Reputation::new();
        rep.on_year_without_launch(&cfg());
        rep.on_year_without_launch(&cfg());
        assert_eq!(rep.drought_factor, -20.0);
        rep.on_launch_success(&cfg());
        assert_eq!(rep.drought_factor, 0.0);
    }

    #[test]
    fn test_contract_expiry() {
        let mut rep = Reputation::new();
        rep.on_contract_expired(&cfg(), 1.0);
        assert_eq!(rep.expiry_factor, -10.0);
        rep.on_contract_expired(&cfg(), 1.0);
        assert_eq!(rep.expiry_factor, -20.0);
        // Contract launch decays it
        rep.on_contract_launch(&cfg());
        assert!((rep.expiry_factor - (-16.0)).abs() < 0.01);
    }

    #[test]
    fn test_severity_scales_penalties() {
        let mut baseline = Reputation::new();
        baseline.on_launch_failure(&cfg(), 1.0);
        let mut harsh = Reputation::new();
        harsh.on_launch_failure(&cfg(), 2.0);
        assert!((harsh.total() - baseline.total() * 2.0).abs() < 1e-9);

        let mut lenient = Reputation::new();
        lenient.on_contract_expired(&cfg(), 0.7);
        assert!((lenient.expiry_factor - (-cfg().expiry_penalty * 0.7)).abs() < 1e-9);
    }

    #[test]
    fn test_recovery_from_failure() {
        let mut rep = Reputation::new();
        rep.on_launch_failure(&cfg(), 1.0);
        let after_failure = rep.total();
        // Several successes should recover
        for _ in 0..5 {
            rep.on_launch_success(&cfg());
        }
        assert!(rep.total() > after_failure);
    }
}
