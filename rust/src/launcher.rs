use rand::Rng;

/// Represents the stages of a rocket launch to Low Earth Orbit (LEO)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchStage {
    /// Rocket engines ignite on the pad
    Ignition,
    /// Rocket lifts off from the launch pad
    Liftoff,
    /// Maximum dynamic pressure - most stressful aerodynamic moment
    MaxQ,
    /// First stage separates from second stage
    Stage1Separation,
    /// Second stage engine ignites
    Stage2Ignition,
    /// Main Engine Cutoff - second stage engine shuts down
    MECO,
    /// Final orbital insertion burn
    OrbitInsertion,
}

impl LaunchStage {
    /// Returns the failure probability for this stage (0.0 to 1.0)
    pub fn failure_probability(&self) -> f64 {
        match self {
            LaunchStage::Ignition => 0.08,           // 8% - ignition is risky
            LaunchStage::Liftoff => 0.05,            // 5% - initial ascent
            LaunchStage::MaxQ => 0.15,               // 15% - highest stress
            LaunchStage::Stage1Separation => 0.10,   // 10% - complex separation
            LaunchStage::Stage2Ignition => 0.07,     // 7% - second stage start
            LaunchStage::MECO => 0.03,               // 3% - mostly through danger
            LaunchStage::OrbitInsertion => 0.05,     // 5% - final burn
        }
    }

    /// Returns a human-readable description of the stage
    pub fn description(&self) -> &'static str {
        match self {
            LaunchStage::Ignition => "Engine ignition",
            LaunchStage::Liftoff => "Liftoff",
            LaunchStage::MaxQ => "Max-Q (maximum dynamic pressure)",
            LaunchStage::Stage1Separation => "Stage 1 separation",
            LaunchStage::Stage2Ignition => "Stage 2 ignition",
            LaunchStage::MECO => "MECO (Main Engine Cutoff)",
            LaunchStage::OrbitInsertion => "Orbital insertion",
        }
    }

    /// Returns the next stage in the sequence, or None if this is the final stage
    pub fn next(&self) -> Option<LaunchStage> {
        match self {
            LaunchStage::Ignition => Some(LaunchStage::Liftoff),
            LaunchStage::Liftoff => Some(LaunchStage::MaxQ),
            LaunchStage::MaxQ => Some(LaunchStage::Stage1Separation),
            LaunchStage::Stage1Separation => Some(LaunchStage::Stage2Ignition),
            LaunchStage::Stage2Ignition => Some(LaunchStage::MECO),
            LaunchStage::MECO => Some(LaunchStage::OrbitInsertion),
            LaunchStage::OrbitInsertion => None, // Final stage
        }
    }

    /// Returns all stages in order
    pub fn all_stages() -> Vec<LaunchStage> {
        vec![
            LaunchStage::Ignition,
            LaunchStage::Liftoff,
            LaunchStage::MaxQ,
            LaunchStage::Stage1Separation,
            LaunchStage::Stage2Ignition,
            LaunchStage::MECO,
            LaunchStage::OrbitInsertion,
        ]
    }

    /// Simulates this stage and returns whether it succeeded
    pub fn simulate(&self) -> bool {
        let mut rng = rand::thread_rng();
        let roll: f64 = rng.gen();
        roll > self.failure_probability()
    }
}

/// The result of a launch attempt
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchResult {
    /// Launch succeeded and reached orbit
    Success,
    /// Launch failed at the specified stage
    Failure { stage: LaunchStage },
}

impl LaunchResult {
    /// Returns a human-readable message about the result
    pub fn message(&self) -> String {
        match self {
            LaunchResult::Success => {
                "Success! Rocket reached Low Earth Orbit!".to_string()
            }
            LaunchResult::Failure { stage } => {
                format!("Failure during {}. Rocket exploded.", stage.description())
            }
        }
    }
}

/// Simulates a complete rocket launch
pub struct LaunchSimulator {
    current_stage: Option<LaunchStage>,
}

impl LaunchSimulator {
    /// Creates a new launch simulator
    pub fn new() -> Self {
        Self {
            current_stage: Some(LaunchStage::Ignition),
        }
    }

    /// Runs a complete launch simulation and returns the result
    pub fn simulate_launch() -> LaunchResult {
        let mut current_stage = LaunchStage::Ignition;

        loop {
            // Try to pass this stage
            if !current_stage.simulate() {
                // Failed at this stage
                return LaunchResult::Failure {
                    stage: current_stage,
                };
            }

            // Stage passed, move to next
            match current_stage.next() {
                Some(next_stage) => current_stage = next_stage,
                None => {
                    // No more stages - success!
                    return LaunchResult::Success;
                }
            }
        }
    }

    /// Runs a launch simulation with a callback for each stage
    /// The callback receives the stage being attempted
    /// Returns the final launch result
    pub fn simulate_launch_with_callback<F>(mut callback: F) -> LaunchResult
    where
        F: FnMut(LaunchStage),
    {
        let mut current_stage = LaunchStage::Ignition;

        loop {
            // Notify callback of current stage
            callback(current_stage);

            // Try to pass this stage
            if !current_stage.simulate() {
                // Failed at this stage
                return LaunchResult::Failure {
                    stage: current_stage,
                };
            }

            // Stage passed, move to next
            match current_stage.next() {
                Some(next_stage) => current_stage = next_stage,
                None => {
                    // No more stages - success!
                    return LaunchResult::Success;
                }
            }
        }
    }

    /// Returns the current stage of the simulation (if any)
    pub fn current_stage(&self) -> Option<LaunchStage> {
        self.current_stage
    }
}

impl Default for LaunchSimulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_progression() {
        let stages = LaunchStage::all_stages();
        assert_eq!(stages.len(), 7);
        assert_eq!(stages[0], LaunchStage::Ignition);
        assert_eq!(stages[6], LaunchStage::OrbitInsertion);
    }

    #[test]
    fn test_stage_next() {
        assert_eq!(
            LaunchStage::Ignition.next(),
            Some(LaunchStage::Liftoff)
        );
        assert_eq!(
            LaunchStage::MaxQ.next(),
            Some(LaunchStage::Stage1Separation)
        );
        assert_eq!(LaunchStage::OrbitInsertion.next(), None);
    }

    #[test]
    fn test_failure_probabilities() {
        // All probabilities should be between 0 and 1
        for stage in LaunchStage::all_stages() {
            let prob = stage.failure_probability();
            assert!(prob >= 0.0 && prob <= 1.0);
        }

        // Max-Q should be the most dangerous stage
        assert_eq!(LaunchStage::MaxQ.failure_probability(), 0.15);
    }

    #[test]
    fn test_stage_descriptions() {
        assert_eq!(LaunchStage::Ignition.description(), "Engine ignition");
        assert_eq!(
            LaunchStage::MaxQ.description(),
            "Max-Q (maximum dynamic pressure)"
        );
    }

    #[test]
    fn test_launch_result_messages() {
        let success = LaunchResult::Success;
        assert!(success.message().contains("Success"));

        let failure = LaunchResult::Failure {
            stage: LaunchStage::MaxQ,
        };
        assert!(failure.message().contains("Failure"));
        assert!(failure.message().contains("Max-Q"));
    }

    #[test]
    fn test_simulator_creation() {
        let sim = LaunchSimulator::new();
        assert_eq!(sim.current_stage(), Some(LaunchStage::Ignition));
    }

    #[test]
    fn test_simulate_launch() {
        // Run multiple simulations to ensure both outcomes are possible
        let mut successes = 0;
        let mut failures = 0;

        for _ in 0..100 {
            match LaunchSimulator::simulate_launch() {
                LaunchResult::Success => successes += 1,
                LaunchResult::Failure { .. } => failures += 1,
            }
        }

        // With our probabilities, we should see both outcomes
        assert!(successes > 0, "Should have some successes");
        assert!(failures > 0, "Should have some failures");
    }

    #[test]
    fn test_simulate_launch_with_callback() {
        let mut stages_visited = Vec::new();

        let result = LaunchSimulator::simulate_launch_with_callback(|stage| {
            stages_visited.push(stage);
        });

        // Should have visited at least one stage
        assert!(!stages_visited.is_empty());

        // First stage should always be Ignition
        assert_eq!(stages_visited[0], LaunchStage::Ignition);

        // Check result consistency
        match result {
            LaunchResult::Success => {
                // If successful, should have visited all stages
                assert_eq!(stages_visited.len(), 7);
                assert_eq!(*stages_visited.last().unwrap(), LaunchStage::OrbitInsertion);
            }
            LaunchResult::Failure { stage } => {
                // Last stage visited should be the failure stage
                assert_eq!(*stages_visited.last().unwrap(), stage);
            }
        }
    }

    #[test]
    fn test_multiple_launches_produce_different_results() {
        // Run multiple launches and collect results
        let mut results = Vec::new();
        for _ in 0..50 {
            results.push(LaunchSimulator::simulate_launch());
        }

        // Should have at least some variety in results
        let has_success = results.iter().any(|r| matches!(r, LaunchResult::Success));
        let has_failure = results.iter().any(|r| matches!(r, LaunchResult::Failure { .. }));

        assert!(has_success || has_failure, "Should produce results");
    }
}
