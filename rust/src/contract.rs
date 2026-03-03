use rand::Rng;
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};

use crate::calendar::GameDate;

/// Unique identifier for a contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContractId(pub u64);

/// Status of a contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractStatus {
    /// On the market, available for the player to accept.
    Available,
    /// Accepted by the player, awaiting launch.
    Accepted,
    /// Successfully delivered.
    Completed,
    /// Launch failed.
    Failed { reason: String },
    /// Deadline passed without successful launch.
    Expired,
}

/// A contract to deliver a payload to a destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub id: ContractId,
    pub name: String,
    pub destination: String,
    pub payload_kg: f64,
    pub payment: f64,
    pub deadline: GameDate,
    pub status: ContractStatus,
}

/// A destination that contracts can target, with its reputation threshold
/// and payload/payment parameters.
struct ContractDestination {
    location_id: &'static str,
    display_name: &'static str,
    min_reputation: f64,
    min_payload_kg: f64,
    max_payload_kg: f64,
    /// Payment per kg (base rate, before variance).
    rate_per_kg: f64,
}

/// All possible contract destinations with their parameters.
const CONTRACT_DESTINATIONS: &[ContractDestination] = &[
    ContractDestination {
        location_id: "leo",
        display_name: "LEO",
        min_reputation: 0.0,
        min_payload_kg: 500.0,
        max_payload_kg: 20_000.0,
        rate_per_kg: 20_000.0,
    },
    ContractDestination {
        location_id: "sso",
        display_name: "SSO",
        min_reputation: 20.0,
        min_payload_kg: 300.0,
        max_payload_kg: 8_000.0,
        rate_per_kg: 30_000.0,
    },
    ContractDestination {
        location_id: "meo",
        display_name: "MEO",
        min_reputation: 20.0,
        min_payload_kg: 500.0,
        max_payload_kg: 5_000.0,
        rate_per_kg: 35_000.0,
    },
    ContractDestination {
        location_id: "gto",
        display_name: "GTO",
        min_reputation: 20.0,
        min_payload_kg: 500.0,
        max_payload_kg: 6_000.0,
        rate_per_kg: 40_000.0,
    },
    ContractDestination {
        location_id: "geo",
        display_name: "GEO",
        min_reputation: 50.0,
        min_payload_kg: 500.0,
        max_payload_kg: 5_000.0,
        rate_per_kg: 60_000.0,
    },
    ContractDestination {
        location_id: "l1",
        display_name: "L1",
        min_reputation: 80.0,
        min_payload_kg: 200.0,
        max_payload_kg: 3_000.0,
        rate_per_kg: 80_000.0,
    },
    ContractDestination {
        location_id: "l2",
        display_name: "L2",
        min_reputation: 80.0,
        min_payload_kg: 200.0,
        max_payload_kg: 3_000.0,
        rate_per_kg: 80_000.0,
    },
    ContractDestination {
        location_id: "lunar_orbit",
        display_name: "Lunar Orbit",
        min_reputation: 100.0,
        min_payload_kg: 200.0,
        max_payload_kg: 2_000.0,
        rate_per_kg: 120_000.0,
    },
    ContractDestination {
        location_id: "lunar_surface",
        display_name: "Lunar Surface",
        min_reputation: 150.0,
        min_payload_kg: 200.0,
        max_payload_kg: 2_000.0,
        rate_per_kg: 200_000.0,
    },
];

/// Contract name prefixes for flavor.
const CONTRACT_PREFIXES: &[&str] = &[
    "ComSat", "NavSat", "WeatherSat", "RelaySat", "SciSat",
    "Probe", "Cargo", "Supply", "Observatory", "Surveyor",
];

/// Generate one contract for a given month using a deterministic RNG.
/// Returns None if no destinations are available at the given reputation.
pub fn generate_monthly_contract(
    rng: &mut StdRng,
    contract_id: ContractId,
    current_date: GameDate,
    reputation: f64,
) -> Option<Contract> {
    // Filter destinations by reputation
    let eligible: Vec<&ContractDestination> = CONTRACT_DESTINATIONS.iter()
        .filter(|d| reputation >= d.min_reputation)
        .collect();

    if eligible.is_empty() {
        return None;
    }

    // Pick a random destination
    let dest = eligible[rng.gen_range(0..eligible.len())];

    // Generate payload mass
    let payload_kg = rng.gen_range(dest.min_payload_kg..=dest.max_payload_kg);
    // Round to nearest 100 kg
    let payload_kg = (payload_kg / 100.0).round() * 100.0;

    // Generate payment with +/- 20% variance
    let base_payment = payload_kg * dest.rate_per_kg;
    let variance = rng.gen_range(0.8..=1.2);
    let payment = (base_payment * variance / 10_000.0).round() * 10_000.0;

    // Deadline: 60-180 days from now
    let deadline_days = rng.gen_range(60..=180);
    let deadline = current_date.add_days(deadline_days);

    // Generate name
    let prefix = CONTRACT_PREFIXES[rng.gen_range(0..CONTRACT_PREFIXES.len())];
    let name = format!("{} to {}", prefix, dest.display_name);

    Some(Contract {
        id: contract_id,
        name,
        destination: dest.location_id.to_string(),
        payload_kg,
        payment,
        deadline,
        status: ContractStatus::Available,
    })
}

/// Get the display name for a contract destination location ID.
pub fn destination_display_name(location_id: &str) -> &str {
    CONTRACT_DESTINATIONS.iter()
        .find(|d| d.location_id == location_id)
        .map(|d| d.display_name)
        .unwrap_or(location_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn make_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    #[test]
    fn test_generate_contract_zero_reputation() {
        let mut rng = make_rng();
        let date = GameDate::new(2001, 1, 1);
        let contract = generate_monthly_contract(
            &mut rng, ContractId(1), date, 0.0,
        );
        // With 0 rep, only LEO is available
        let c = contract.expect("should generate a contract");
        assert_eq!(c.destination, "leo");
        assert!(c.payload_kg >= 500.0 && c.payload_kg <= 20_000.0);
        assert!(c.payment > 0.0);
        assert!(c.deadline > date);
    }

    #[test]
    fn test_generate_contract_high_reputation() {
        let mut rng = make_rng();
        let date = GameDate::new(2001, 1, 1);
        // Generate many contracts at high reputation — should see variety
        let mut destinations = std::collections::HashSet::new();
        for i in 0..50 {
            let c = generate_monthly_contract(
                &mut rng, ContractId(i), date, 200.0,
            ).unwrap();
            destinations.insert(c.destination);
        }
        // Should have more than just LEO
        assert!(destinations.len() > 1);
    }

    #[test]
    fn test_generate_contract_deterministic() {
        let date = GameDate::new(2001, 1, 1);
        let c1 = generate_monthly_contract(
            &mut make_rng(), ContractId(1), date, 0.0,
        ).unwrap();
        let c2 = generate_monthly_contract(
            &mut make_rng(), ContractId(1), date, 0.0,
        ).unwrap();
        assert_eq!(c1.destination, c2.destination);
        assert!((c1.payload_kg - c2.payload_kg).abs() < 0.01);
        assert!((c1.payment - c2.payment).abs() < 0.01);
    }

    #[test]
    fn test_payload_rounded() {
        let mut rng = make_rng();
        let date = GameDate::new(2001, 1, 1);
        for i in 0..20 {
            let c = generate_monthly_contract(
                &mut rng, ContractId(i), date, 200.0,
            ).unwrap();
            assert_eq!(c.payload_kg % 100.0, 0.0, "Payload should be rounded to 100 kg");
        }
    }

    #[test]
    fn test_deadline_in_future() {
        let mut rng = make_rng();
        let date = GameDate::new(2001, 6, 15);
        let c = generate_monthly_contract(
            &mut rng, ContractId(1), date, 0.0,
        ).unwrap();
        assert!(c.deadline > date);
    }
}
