use rand::Rng;

use crate::location::DELTA_V_MAP;

/// Destinations for rocket missions
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Destination {
    Suborbital,
    LEO,
    SSO,
    MEO,
    GTO,
    GEO,
}

impl Destination {
    /// Map destination to its location graph ID
    pub fn location_id(&self) -> &'static str {
        match self {
            Destination::Suborbital => "suborbital",
            Destination::LEO => "leo",
            Destination::SSO => "sso",
            Destination::MEO => "meo",
            Destination::GTO => "gto",
            Destination::GEO => "geo",
        }
    }

    /// Delta-v required to reach this destination from Earth's surface (m/s)
    /// Uses the location graph shortest path for physically correct values
    pub fn required_delta_v(&self) -> f64 {
        DELTA_V_MAP
            .shortest_path("earth_surface", self.location_id())
            .map(|(_, dv)| dv)
            .expect("All destinations must be reachable from earth_surface")
    }

    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        DELTA_V_MAP
            .location(self.location_id())
            .expect("All destinations must exist in location graph")
            .display_name
    }

    /// Short code for UI
    pub fn short_name(&self) -> &'static str {
        DELTA_V_MAP
            .location(self.location_id())
            .expect("All destinations must exist in location graph")
            .short_name
    }

    /// All destinations in order of difficulty
    pub fn all() -> &'static [Destination] {
        &[
            Destination::Suborbital,
            Destination::LEO,
            Destination::SSO,
            Destination::MEO,
            Destination::GTO,
            Destination::GEO,
        ]
    }
}

/// Payload type definition with mass range and pricing
#[derive(Clone, Debug)]
pub struct PayloadType {
    pub name: &'static str,
    pub destination: Destination,
    pub mass_range: (f64, f64),    // Min/max kg
    pub reward_per_kg: f64,        // $/kg
    pub base_reward: f64,          // Flat fee
}

/// All available payload types
pub const PAYLOAD_TYPES: &[PayloadType] = &[
    // Suborbital
    PayloadType {
        name: "Sounding rocket experiment",
        destination: Destination::Suborbital,
        mass_range: (50.0, 200.0),
        reward_per_kg: 20_000.0,
        base_reward: 1_000_000.0,
    },
    PayloadType {
        name: "Technology demonstrator",
        destination: Destination::Suborbital,
        mass_range: (100.0, 300.0),
        reward_per_kg: 18_000.0,
        base_reward: 1_500_000.0,
    },
    PayloadType {
        name: "Microgravity research",
        destination: Destination::Suborbital,
        mass_range: (200.0, 500.0),
        reward_per_kg: 15_000.0,
        base_reward: 2_000_000.0,
    },

    // LEO
    PayloadType {
        name: "CubeSat rideshare",
        destination: Destination::LEO,
        mass_range: (50.0, 200.0),
        reward_per_kg: 50_000.0,
        base_reward: 3_000_000.0,
    },
    PayloadType {
        name: "Small imaging satellite",
        destination: Destination::LEO,
        mass_range: (200.0, 800.0),
        reward_per_kg: 35_000.0,
        base_reward: 8_000_000.0,
    },
    PayloadType {
        name: "Earth observation satellite",
        destination: Destination::LEO,
        mass_range: (1000.0, 3000.0),
        reward_per_kg: 22_000.0,
        base_reward: 18_000_000.0,
    },
    PayloadType {
        name: "Space station resupply",
        destination: Destination::LEO,
        mass_range: (3000.0, 8000.0),
        reward_per_kg: 18_000.0,
        base_reward: 25_000_000.0,
    },

    // SSO
    PayloadType {
        name: "Small weather satellite",
        destination: Destination::SSO,
        mass_range: (200.0, 600.0),
        reward_per_kg: 50_000.0,
        base_reward: 10_000_000.0,
    },
    PayloadType {
        name: "Reconnaissance satellite",
        destination: Destination::SSO,
        mass_range: (1000.0, 3000.0),
        reward_per_kg: 32_000.0,
        base_reward: 28_000_000.0,
    },
    PayloadType {
        name: "Imaging constellation satellite",
        destination: Destination::SSO,
        mass_range: (500.0, 1500.0),
        reward_per_kg: 40_000.0,
        base_reward: 15_000_000.0,
    },

    // MEO
    PayloadType {
        name: "Navigation satellite",
        destination: Destination::MEO,
        mass_range: (1000.0, 2500.0),
        reward_per_kg: 50_000.0,
        base_reward: 30_000_000.0,
    },
    PayloadType {
        name: "MEO communications satellite",
        destination: Destination::MEO,
        mass_range: (2000.0, 4000.0),
        reward_per_kg: 45_000.0,
        base_reward: 40_000_000.0,
    },

    // GTO
    PayloadType {
        name: "Communications satellite",
        destination: Destination::GTO,
        mass_range: (2000.0, 4500.0),
        reward_per_kg: 38_000.0,
        base_reward: 25_000_000.0,
    },
    PayloadType {
        name: "TV broadcast satellite",
        destination: Destination::GTO,
        mass_range: (3000.0, 6000.0),
        reward_per_kg: 40_000.0,
        base_reward: 20_000_000.0,
    },
    PayloadType {
        name: "Heavy comsat",
        destination: Destination::GTO,
        mass_range: (5000.0, 7000.0),
        reward_per_kg: 42_000.0,
        base_reward: 15_000_000.0,
    },

    // GEO (direct insertion - premium pricing)
    PayloadType {
        name: "Premium comsat",
        destination: Destination::GEO,
        mass_range: (2000.0, 4000.0),
        reward_per_kg: 70_000.0,
        base_reward: 40_000_000.0,
    },
    PayloadType {
        name: "Broadcast satellite",
        destination: Destination::GEO,
        mass_range: (3000.0, 5000.0),
        reward_per_kg: 65_000.0,
        base_reward: 60_000_000.0,
    },
];

/// Company names for contract generation
pub const COMPANY_NAMES: &[&str] = &[
    "SatCom Industries",
    "GlobalLink",
    "TerraView",
    "OrbitNet",
    "StarComm",
    "AstroTech",
    "SpaceData",
    "CelestialMedia",
    "Northstar Aerospace",
    "Pacific Satellite",
    "Atlantic Comm",
    "Meridian Systems",
    "Equatorial Broadcasting",
    "PolarSat",
    "Horizon Networks",
    "Zenith Communications",
];

/// A contract for a launch mission
#[derive(Clone, Debug)]
pub struct Contract {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub destination: Destination,
    pub payload_type: String,
    pub payload_mass_kg: f64,
    pub reward: f64,
}

impl Contract {
    /// Generate a random contract
    pub fn generate(id: u32) -> Self {
        let mut rng = rand::thread_rng();

        // Pick a random payload type
        let payload_type_idx = rng.gen_range(0..PAYLOAD_TYPES.len());
        let payload_type = &PAYLOAD_TYPES[payload_type_idx];

        // Generate random mass within range
        let mass = rng.gen_range(payload_type.mass_range.0..=payload_type.mass_range.1);
        let mass = (mass / 10.0).round() * 10.0; // Round to nearest 10 kg

        // Calculate reward
        let reward = payload_type.base_reward + (mass * payload_type.reward_per_kg);

        // Pick a random company
        let company_idx = rng.gen_range(0..COMPANY_NAMES.len());
        let company = COMPANY_NAMES[company_idx];

        Contract {
            id,
            name: format!("{} - {}", company, payload_type.name),
            description: format!(
                "Launch {:.0} kg {} to {}",
                mass,
                payload_type.name.to_lowercase(),
                payload_type.destination.display_name()
            ),
            destination: payload_type.destination.clone(),
            payload_type: payload_type.name.to_string(),
            payload_mass_kg: mass,
            reward,
        }
    }

    /// Generate multiple random contracts
    pub fn generate_batch(count: usize, starting_id: u32) -> Vec<Contract> {
        (0..count)
            .map(|i| Contract::generate(starting_id + i as u32))
            .collect()
    }

    /// Generate contracts with at least one from each difficulty tier
    pub fn generate_diverse_batch(count: usize, starting_id: u32) -> Vec<Contract> {
        let mut rng = rand::thread_rng();
        let mut contracts = Vec::with_capacity(count);

        // Get payload types grouped by destination
        let destinations = Destination::all();

        // Try to include at least one contract per destination tier (if count allows)
        let tiers_to_include = count.min(destinations.len());
        let mut used_destinations = Vec::new();

        // First, add one contract per tier (up to count)
        for (i, dest) in destinations.iter().take(tiers_to_include).enumerate() {
            // Find payload types for this destination
            let matching_types: Vec<&PayloadType> = PAYLOAD_TYPES
                .iter()
                .filter(|pt| &pt.destination == dest)
                .collect();

            if !matching_types.is_empty() {
                let payload_type = matching_types[rng.gen_range(0..matching_types.len())];
                let mass = rng.gen_range(payload_type.mass_range.0..=payload_type.mass_range.1);
                let mass = (mass / 10.0).round() * 10.0;
                let reward = payload_type.base_reward + (mass * payload_type.reward_per_kg);
                let company = COMPANY_NAMES[rng.gen_range(0..COMPANY_NAMES.len())];

                contracts.push(Contract {
                    id: starting_id + i as u32,
                    name: format!("{} - {}", company, payload_type.name),
                    description: format!(
                        "Launch {:.0} kg {} to {}",
                        mass,
                        payload_type.name.to_lowercase(),
                        payload_type.destination.display_name()
                    ),
                    destination: payload_type.destination.clone(),
                    payload_type: payload_type.name.to_string(),
                    payload_mass_kg: mass,
                    reward,
                });
                used_destinations.push(dest.clone());
            }
        }

        // Fill remaining slots with random contracts
        let remaining = count.saturating_sub(contracts.len());
        for i in 0..remaining {
            let contract = Contract::generate(starting_id + contracts.len() as u32 + i as u32);
            contracts.push(contract);
        }

        contracts
    }
}

/// Format a money value for display (e.g., "$150M")
pub fn format_money(amount: f64) -> String {
    if amount >= 1_000_000_000.0 {
        format!("${:.1}B", amount / 1_000_000_000.0)
    } else if amount >= 1_000_000.0 {
        format!("${:.0}M", amount / 1_000_000.0)
    } else if amount >= 1_000.0 {
        format!("${:.0}K", amount / 1_000.0)
    } else {
        format!("${:.0}", amount)
    }
}

/// Format a money value with more precision for display
pub fn format_money_precise(amount: f64) -> String {
    if amount >= 1_000_000_000.0 {
        format!("${:.2}B", amount / 1_000_000_000.0)
    } else if amount >= 1_000_000.0 {
        format!("${:.1}M", amount / 1_000_000.0)
    } else if amount >= 1_000.0 {
        format!("${:.1}K", amount / 1_000.0)
    } else {
        format!("${:.0}", amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_destination_delta_v() {
        // Values from location graph shortest paths (no baked-in gravity losses)
        assert_eq!(Destination::Suborbital.required_delta_v(), 3500.0);
        assert_eq!(Destination::LEO.required_delta_v(), 8100.0);
        assert_eq!(Destination::SSO.required_delta_v(), 8600.0);
        assert_eq!(Destination::MEO.required_delta_v(), 10200.0);
        assert_eq!(Destination::GTO.required_delta_v(), 10540.0);
        assert_eq!(Destination::GEO.required_delta_v(), 12040.0);
    }

    #[test]
    fn test_destination_names_from_graph() {
        assert_eq!(Destination::LEO.display_name(), "Low Earth Orbit");
        assert_eq!(Destination::LEO.short_name(), "LEO");
        assert_eq!(Destination::GEO.display_name(), "Geostationary Orbit");
        assert_eq!(Destination::GEO.short_name(), "GEO");
    }

    #[test]
    fn test_contract_generation() {
        let contract = Contract::generate(1);
        assert_eq!(contract.id, 1);
        assert!(contract.payload_mass_kg > 0.0);
        assert!(contract.reward > 0.0);
        assert!(!contract.name.is_empty());
    }

    #[test]
    fn test_batch_generation() {
        let contracts = Contract::generate_batch(5, 100);
        assert_eq!(contracts.len(), 5);
        for (i, c) in contracts.iter().enumerate() {
            assert_eq!(c.id, 100 + i as u32);
        }
    }

    #[test]
    fn test_diverse_batch() {
        let contracts = Contract::generate_diverse_batch(6, 1);
        assert_eq!(contracts.len(), 6);

        // Should have contracts for multiple destinations
        let destinations: Vec<_> = contracts.iter().map(|c| c.destination.clone()).collect();
        let unique: std::collections::HashSet<_> = destinations.into_iter().collect();
        assert!(unique.len() >= 3, "Should have diverse destinations");
    }

    #[test]
    fn test_format_money() {
        assert_eq!(format_money(500.0), "$500");
        assert_eq!(format_money(5_000.0), "$5K");
        assert_eq!(format_money(50_000_000.0), "$50M");
        assert_eq!(format_money(1_500_000_000.0), "$1.5B");
    }

    #[test]
    fn test_payload_types_cover_all_destinations() {
        for dest in Destination::all() {
            let has_payload = PAYLOAD_TYPES.iter().any(|pt| &pt.destination == dest);
            assert!(has_payload, "No payload type for {:?}", dest);
        }
    }

    #[test]
    fn test_reward_calculation() {
        // Pick the first LEO payload type (CubeSat rideshare)
        let pt = &PAYLOAD_TYPES[3]; // CubeSat rideshare
        assert_eq!(pt.destination, Destination::LEO);

        // At min mass (50 kg): base $3M + 50 * $50K = $3M + $2.5M = $5.5M
        let min_reward = pt.base_reward + (pt.mass_range.0 * pt.reward_per_kg);
        assert_eq!(min_reward, 5_500_000.0);

        // At max mass (200 kg): base $3M + 200 * $50K = $3M + $10M = $13M
        let max_reward = pt.base_reward + (pt.mass_range.1 * pt.reward_per_kg);
        assert_eq!(max_reward, 13_000_000.0);
    }
}
