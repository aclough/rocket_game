/// Launch site infrastructure and upgrades
///
/// The launch site determines what rockets can be launched based on
/// pad capacity and propellant storage. Players can upgrade the site
/// to handle larger rockets.

/// Launch site infrastructure
#[derive(Debug, Clone)]
pub struct LaunchSite {
    /// Launch pad level (1-5), affects maximum rocket mass
    pub pad_level: u32,
    /// Propellant storage capacity in kg
    pub propellant_storage_kg: f64,
}

impl LaunchSite {
    /// Create a new launch site with starter infrastructure
    pub fn new() -> Self {
        Self {
            pad_level: 1,
            propellant_storage_kg: 500_000.0,
        }
    }

    /// Get maximum rocket wet mass that can be launched at current pad level
    pub fn max_launch_mass_kg(&self) -> f64 {
        match self.pad_level {
            1 => 300_000.0,    // Small rockets only
            2 => 750_000.0,    // Medium rockets
            3 => 1_500_000.0,  // Large rockets
            4 => 3_000_000.0,  // Heavy lift
            5 => 7_500_000.0,  // Super heavy
            _ => 7_500_000.0,  // Max level
        }
    }

    /// Get cost to upgrade pad to next level (0 if already at max)
    pub fn pad_upgrade_cost(&self) -> f64 {
        match self.pad_level {
            1 => 50_000_000.0,    // $50M for level 2
            2 => 150_000_000.0,   // $150M for level 3
            3 => 400_000_000.0,   // $400M for level 4
            4 => 1_000_000_000.0, // $1B for level 5
            _ => 0.0,             // Already at max
        }
    }

    /// Check if pad can be upgraded
    pub fn can_upgrade_pad(&self) -> bool {
        self.pad_level < 5
    }

    /// Upgrade pad to next level (returns true if successful)
    pub fn upgrade_pad(&mut self) -> bool {
        if self.can_upgrade_pad() {
            self.pad_level += 1;
            true
        } else {
            false
        }
    }

    /// Get propellant upgrade cost
    pub fn propellant_storage_upgrade_cost(&self) -> f64 {
        // Cost scales with current capacity
        self.propellant_storage_kg * 0.1 // $0.10 per kg of additional capacity
    }

    /// Upgrade propellant storage by a given amount
    pub fn upgrade_propellant_storage(&mut self, additional_kg: f64) {
        self.propellant_storage_kg += additional_kg;
    }

    /// Check if a rocket can be launched given its total wet mass
    pub fn can_launch_rocket(&self, rocket_wet_mass_kg: f64) -> bool {
        rocket_wet_mass_kg <= self.max_launch_mass_kg()
    }

    /// Get a formatted string describing the pad level
    pub fn pad_level_name(&self) -> &'static str {
        match self.pad_level {
            1 => "Small Pad",
            2 => "Medium Pad",
            3 => "Large Pad",
            4 => "Heavy Pad",
            5 => "Super Heavy Pad",
            _ => "Super Heavy Pad",
        }
    }
}

impl Default for LaunchSite {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_launch_site() {
        let site = LaunchSite::new();
        assert_eq!(site.pad_level, 1);
        assert_eq!(site.propellant_storage_kg, 500_000.0);
    }

    #[test]
    fn test_max_launch_mass() {
        let mut site = LaunchSite::new();
        assert_eq!(site.max_launch_mass_kg(), 300_000.0);

        site.pad_level = 3;
        assert_eq!(site.max_launch_mass_kg(), 1_500_000.0);

        site.pad_level = 5;
        assert_eq!(site.max_launch_mass_kg(), 7_500_000.0);
    }

    #[test]
    fn test_pad_upgrade() {
        let mut site = LaunchSite::new();
        assert!(site.can_upgrade_pad());
        assert_eq!(site.pad_upgrade_cost(), 50_000_000.0);

        assert!(site.upgrade_pad());
        assert_eq!(site.pad_level, 2);
        assert_eq!(site.pad_upgrade_cost(), 150_000_000.0);

        // Upgrade to max
        site.pad_level = 5;
        assert!(!site.can_upgrade_pad());
        assert_eq!(site.pad_upgrade_cost(), 0.0);
        assert!(!site.upgrade_pad());
    }

    #[test]
    fn test_can_launch_rocket() {
        let site = LaunchSite::new();
        assert!(site.can_launch_rocket(100_000.0));
        assert!(site.can_launch_rocket(300_000.0));
        assert!(!site.can_launch_rocket(300_001.0));
    }
}
