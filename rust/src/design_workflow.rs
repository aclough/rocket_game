/// Unified design workflow shared by engine designs and rocket designs.
/// Manages the status progression, flaw tracking, and work advancement.

use crate::engineering_team::{DETAILED_ENGINEERING_WORK, TESTING_WORK};
use crate::flaw::Flaw;

/// Work required to fix a discovered flaw (14 days with 1 team)
pub const FLAW_FIX_WORK: f64 = 14.0;

/// Daily decay rate for hardware boost (applied each day in Testing/Fixing)
/// Pure exponential: multiplier = (1 - DECAY_RATE)^days, no floor.
pub const HARDWARE_DECAY_RATE: f64 = 0.004;

/// Unified status for both engine and rocket designs
#[derive(Debug, Clone, PartialEq)]
pub enum DesignStatus {
    /// Player is editing the specification
    Specification,
    /// Teams are doing detailed engineering work
    Engineering {
        /// Work progress (0.0 to total)
        progress: f64,
        /// Total work required
        total: f64,
    },
    /// Teams are testing and looking for flaws
    Testing {
        /// Work progress (0.0 to total)
        progress: f64,
        /// Total work required per testing cycle
        total: f64,
    },
    /// Teams are fixing a discovered flaw
    Fixing {
        /// Name of the flaw being fixed
        flaw_name: String,
        /// Index of the flaw in active_flaws
        flaw_index: usize,
        /// Work progress (0.0 to total)
        progress: f64,
        /// Total work required
        total: f64,
    },
    /// Design is complete and ready for launch
    Complete,
}

impl Default for DesignStatus {
    fn default() -> Self {
        DesignStatus::Specification
    }
}

impl DesignStatus {
    /// Get the base status name for display
    pub fn name(&self) -> &'static str {
        match self {
            DesignStatus::Specification => "Specification",
            DesignStatus::Engineering { .. } => "Engineering",
            DesignStatus::Testing { .. } => "Testing",
            DesignStatus::Fixing { .. } => "Fixing",
            DesignStatus::Complete => "Complete",
        }
    }

    /// Get the full status string for display (includes flaw name if Fixing)
    pub fn display_name(&self) -> String {
        match self {
            DesignStatus::Fixing { flaw_name, .. } => format!("Fixing: {}", flaw_name),
            other => other.name().to_string(),
        }
    }

    /// Get progress as a fraction (0.0 to 1.0)
    pub fn progress_fraction(&self) -> f64 {
        match self {
            DesignStatus::Specification => 0.0,
            DesignStatus::Engineering { progress, total } => {
                if *total > 0.0 { progress / total } else { 0.0 }
            }
            DesignStatus::Testing { progress, total } => {
                if *total > 0.0 { progress / total } else { 0.0 }
            }
            DesignStatus::Fixing { progress, total, .. } => {
                if *total > 0.0 { progress / total } else { 0.0 }
            }
            DesignStatus::Complete => 1.0,
        }
    }

    /// Check if design is in a work phase (Engineering, Testing, or Fixing)
    pub fn is_working(&self) -> bool {
        matches!(self, DesignStatus::Engineering { .. } | DesignStatus::Testing { .. } | DesignStatus::Fixing { .. })
    }

    /// Check if design can be edited (only in Specification)
    pub fn can_edit(&self) -> bool {
        matches!(self, DesignStatus::Specification)
    }

    /// Check if design is ready for launch
    /// Designs in Testing or Fixing can still be launched (with known risks)
    pub fn can_launch(&self) -> bool {
        matches!(self, DesignStatus::Complete | DesignStatus::Testing { .. } | DesignStatus::Fixing { .. })
    }
}

/// Shared workflow state for engine and rocket designs
#[derive(Debug, Clone)]
pub struct DesignWorkflow {
    /// Current status in the engineering workflow
    pub status: DesignStatus,
    /// Active (unfixed) flaws
    pub active_flaws: Vec<Flaw>,
    /// Fixed flaws (kept for history/UI display)
    pub fixed_flaws: Vec<Flaw>,
    /// Whether flaws have been generated for this design
    pub flaws_generated: bool,
    /// Cumulative work completed during Testing phase (for testing level estimation)
    pub testing_work_completed: f64,
    /// Hardware boost factor (1.0 = fresh hardware test, decays daily in Testing/Fixing)
    pub hardware_boost: f64,
}

impl DesignWorkflow {
    /// Create a new workflow in Specification state
    pub fn new() -> Self {
        Self {
            status: DesignStatus::Specification,
            active_flaws: Vec::new(),
            fixed_flaws: Vec::new(),
            flaws_generated: false,
            testing_work_completed: 0.0,
            hardware_boost: 1.0,
        }
    }

    /// Get the current hardware multiplier (pure exponential, no floor)
    pub fn hardware_multiplier(&self) -> f64 {
        self.hardware_boost
    }

    /// Apply daily hardware boost decay
    pub fn decay_hardware_boost(&mut self) {
        self.hardware_boost *= 1.0 - HARDWARE_DECAY_RATE;
    }

    /// Reset hardware boost to 1.0 (after hardware test or launch)
    pub fn reset_hardware_boost(&mut self) {
        self.hardware_boost = 1.0;
    }

    /// Add testing work from a launch and reset hardware boost
    pub fn add_launch_testing_work(&mut self, work: f64) {
        self.testing_work_completed += work;
        self.reset_hardware_boost();
    }

    /// Transition from Specification to Engineering phase
    pub fn submit_to_engineering(&mut self) -> bool {
        if !matches!(self.status, DesignStatus::Specification) {
            return false;
        }
        self.status = DesignStatus::Engineering {
            progress: 0.0,
            total: DETAILED_ENGINEERING_WORK,
        };
        true
    }

    /// Advance work on this design by one day's worth of efficiency.
    /// Returns true if the current work phase completed.
    pub fn advance_work(&mut self, efficiency: f64) -> bool {
        match &mut self.status {
            DesignStatus::Engineering { progress, total } => {
                *progress += efficiency;
                if *progress >= *total {
                    // Move to Testing phase
                    self.status = DesignStatus::Testing {
                        progress: 0.0,
                        total: TESTING_WORK,
                    };
                    return true;
                }
            }
            DesignStatus::Testing { progress, total } => {
                *progress += efficiency;
                if *progress >= *total {
                    // Testing cycle complete - reset for next cycle
                    self.status = DesignStatus::Testing {
                        progress: 0.0,
                        total: TESTING_WORK,
                    };
                    return true;
                }
            }
            DesignStatus::Fixing { progress, total, .. } => {
                *progress += efficiency;
                if *progress >= *total {
                    // Fixing complete - will be handled by complete_flaw_fix()
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    /// Start fixing a discovered flaw.
    /// Transitions from Testing to Fixing state.
    pub fn start_fixing_flaw(&mut self, flaw_index: usize) -> bool {
        if !matches!(self.status, DesignStatus::Testing { .. }) {
            return false;
        }
        if flaw_index >= self.active_flaws.len() {
            return false;
        }
        let flaw = &self.active_flaws[flaw_index];
        if !flaw.discovered || flaw.fixed {
            return false;
        }

        self.status = DesignStatus::Fixing {
            flaw_name: flaw.name.clone(),
            flaw_index,
            progress: 0.0,
            total: FLAW_FIX_WORK,
        };
        true
    }

    /// Complete the current flaw fix and return to Testing.
    /// Returns the name of the fixed flaw, or None if not in Fixing state.
    pub fn complete_flaw_fix(&mut self) -> Option<String> {
        if let DesignStatus::Fixing { flaw_index, flaw_name, .. } = &self.status {
            let flaw_name = flaw_name.clone();
            let flaw_index = *flaw_index;

            // Move flaw from active to fixed (consistent with fix_flaw_by_index)
            if flaw_index < self.active_flaws.len() {
                let mut flaw = self.active_flaws.remove(flaw_index);
                flaw.fixed = true;
                self.fixed_flaws.push(flaw);
            }

            // Return to Testing with progress reset for new cycle
            self.status = DesignStatus::Testing {
                progress: 0.0,
                total: TESTING_WORK,
            };

            Some(flaw_name)
        } else {
            None
        }
    }

    /// Get the index of the first discovered but unfixed flaw
    pub fn get_next_unfixed_flaw(&self) -> Option<usize> {
        self.active_flaws
            .iter()
            .position(|f| f.discovered && !f.fixed)
    }

    /// Roll flaw discovery on testing cycle completion.
    /// Returns names of newly discovered flaws.
    pub fn discover_flaws_on_cycle_complete(&mut self) -> Vec<String> {
        use rand::Rng;
        let mut discovered = Vec::new();
        let mut rng = rand::thread_rng();
        for flaw in self.active_flaws.iter_mut() {
            if !flaw.discovered && !flaw.fixed {
                let roll = rng.gen::<f64>();
                if roll < flaw.discovery_probability() {
                    flaw.discovered = true;
                    discovered.push(flaw.name.clone());
                }
            }
        }
        discovered
    }

    /// Get count of discovered (but not yet fixed) flaws
    pub fn get_discovered_unfixed_count(&self) -> usize {
        self.active_flaws.iter().filter(|f| f.discovered && !f.fixed).count()
    }

    /// Get names of discovered (but not fixed) flaws
    pub fn get_unfixed_flaw_names(&self) -> Vec<String> {
        self.active_flaws
            .iter()
            .filter(|f| f.discovered && !f.fixed)
            .map(|f| f.name.clone())
            .collect()
    }

    /// Get names of fixed flaws
    pub fn get_fixed_flaw_names(&self) -> Vec<String> {
        self.fixed_flaws.iter().map(|f| f.name.clone()).collect()
    }

    /// Fix a flaw by index - moves from active_flaws to fixed_flaws
    pub fn fix_flaw_by_index(&mut self, index: usize) -> Option<String> {
        if index < self.active_flaws.len() && self.active_flaws[index].discovered {
            let mut flaw = self.active_flaws.remove(index);
            let name = flaw.name.clone();
            flaw.fixed = true;
            self.fixed_flaws.push(flaw);
            return Some(name);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_workflow() {
        let wf = DesignWorkflow::new();
        assert!(matches!(wf.status, DesignStatus::Specification));
        assert!(wf.active_flaws.is_empty());
        assert!(wf.fixed_flaws.is_empty());
        assert!(!wf.flaws_generated);
        assert_eq!(wf.testing_work_completed, 0.0);
        assert_eq!(wf.hardware_boost, 1.0);
    }

    #[test]
    fn test_hardware_boost_decays() {
        let mut wf = DesignWorkflow::new();
        for _ in 0..30 {
            wf.decay_hardware_boost();
        }
        let expected = (1.0 - HARDWARE_DECAY_RATE).powi(30);
        assert!((wf.hardware_boost - expected).abs() < 0.001,
            "After 30 days: expected {:.4}, got {:.4}", expected, wf.hardware_boost);
        // 0.996^30 ≈ 0.887
        assert!(wf.hardware_boost > 0.88 && wf.hardware_boost < 0.90);
    }

    #[test]
    fn test_hardware_multiplier_approaches_zero() {
        let mut wf = DesignWorkflow::new();
        // Pure exponential decay approaches zero, no floor
        for _ in 0..2000 {
            wf.decay_hardware_boost();
        }
        let mult = wf.hardware_multiplier();
        assert!(mult < 0.01,
            "Multiplier {:.6} should approach zero after heavy decay", mult);
        assert!(mult > 0.0, "Multiplier should remain positive");
    }

    #[test]
    fn test_hardware_boost_resets() {
        let mut wf = DesignWorkflow::new();
        for _ in 0..100 {
            wf.decay_hardware_boost();
        }
        assert!(wf.hardware_boost < 0.7);
        wf.reset_hardware_boost();
        assert_eq!(wf.hardware_boost, 1.0);
        assert_eq!(wf.hardware_multiplier(), 1.0);
    }

    #[test]
    fn test_add_launch_testing_work() {
        let mut wf = DesignWorkflow::new();
        wf.testing_work_completed = 50.0;
        for _ in 0..100 {
            wf.decay_hardware_boost();
        }
        assert!(wf.hardware_boost < 1.0);
        wf.add_launch_testing_work(30.0);
        assert_eq!(wf.testing_work_completed, 80.0);
        assert_eq!(wf.hardware_boost, 1.0);
    }

    #[test]
    fn test_submit_to_engineering() {
        let mut wf = DesignWorkflow::new();
        assert!(wf.submit_to_engineering());
        assert!(matches!(wf.status, DesignStatus::Engineering { .. }));
        // Can't submit again
        assert!(!wf.submit_to_engineering());
    }

    #[test]
    fn test_advance_engineering_to_testing() {
        let mut wf = DesignWorkflow::new();
        wf.submit_to_engineering();
        // Advance past engineering
        let completed = wf.advance_work(DETAILED_ENGINEERING_WORK + 1.0);
        assert!(completed);
        assert!(matches!(wf.status, DesignStatus::Testing { .. }));
    }

    #[test]
    fn test_testing_cycle_resets() {
        let mut wf = DesignWorkflow::new();
        wf.status = DesignStatus::Testing { progress: 0.0, total: TESTING_WORK };
        let completed = wf.advance_work(TESTING_WORK + 1.0);
        assert!(completed);
        // Should reset to testing
        assert!(matches!(wf.status, DesignStatus::Testing { progress, .. } if progress == 0.0));
    }

    #[test]
    fn test_status_display() {
        assert_eq!(DesignStatus::Specification.name(), "Specification");
        assert_eq!(DesignStatus::Complete.name(), "Complete");

        let fixing = DesignStatus::Fixing {
            flaw_name: "Leak".to_string(),
            flaw_index: 0,
            progress: 0.0,
            total: 14.0,
        };
        assert_eq!(fixing.display_name(), "Fixing: Leak");
    }

    #[test]
    fn test_can_edit_and_launch() {
        assert!(DesignStatus::Specification.can_edit());
        assert!(!DesignStatus::Specification.can_launch());

        let testing = DesignStatus::Testing { progress: 0.0, total: 30.0 };
        assert!(!testing.can_edit());
        assert!(testing.can_launch());

        assert!(DesignStatus::Complete.can_launch());
        assert!(!DesignStatus::Complete.can_edit());
    }

    #[test]
    fn test_progress_fraction() {
        assert_eq!(DesignStatus::Specification.progress_fraction(), 0.0);
        assert_eq!(DesignStatus::Complete.progress_fraction(), 1.0);

        let eng = DesignStatus::Engineering { progress: 15.0, total: 30.0 };
        assert!((eng.progress_fraction() - 0.5).abs() < 0.001);
    }
}
