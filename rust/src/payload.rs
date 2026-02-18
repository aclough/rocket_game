/// Unified payload system.
/// Payloads are first-class objects with physical properties, environmental
/// hazard susceptibility, and type-specific data via PayloadKind.

pub type PayloadId = u32;

/// Type-specific payload data.
#[derive(Debug, Clone)]
pub enum PayloadKind {
    /// Contract mission: delivering a customer's satellite
    ContractSatellite {
        contract_id: u32,
        payload_type: String,
        reward: f64,
    },
    /// Company mission: deploying a fuel depot
    Depot {
        depot_design_index: usize,
        capacity_kg: f64,
        serial_number: u32,
        insulated: bool,
    },
}

/// Environmental hazard susceptibility (0.0 = immune, 1.0 = fragile).
#[derive(Debug, Clone)]
pub struct HazardSusceptibility {
    pub radiation_sensitivity: f64,
    pub debris_sensitivity: f64,
    pub thermal_sensitivity: f64,
}

impl HazardSusceptibility {
    /// Neutral susceptibility — moderate sensitivity to all hazards.
    pub fn neutral() -> Self {
        Self {
            radiation_sensitivity: 0.5,
            debris_sensitivity: 0.5,
            thermal_sensitivity: 0.5,
        }
    }
}

/// A payload carried by a rocket or transferable between vehicles.
#[derive(Debug, Clone)]
pub struct Payload {
    pub id: PayloadId,
    pub name: String,
    pub kind: PayloadKind,
    pub mass_kg: f64,
    /// Power generation (solar, RTG) in watts
    pub power_watts: f64,
    /// Power consumption in watts
    pub power_draw_watts: f64,
    pub insulated: bool,
    /// Acceptable temperature range in Kelvin
    pub thermal_range_k: (f64, f64),
    pub hazard_susceptibility: HazardSusceptibility,
}

impl Payload {
    /// Create a contract satellite payload with sensible defaults.
    pub fn contract_satellite(
        id: PayloadId,
        contract_id: u32,
        payload_type: String,
        mass_kg: f64,
        reward: f64,
    ) -> Self {
        Self {
            id,
            name: format!("{} Satellite", payload_type),
            kind: PayloadKind::ContractSatellite {
                contract_id,
                payload_type,
                reward,
            },
            mass_kg,
            power_watts: 0.0,
            power_draw_watts: 0.0,
            insulated: false,
            thermal_range_k: (200.0, 400.0),
            hazard_susceptibility: HazardSusceptibility::neutral(),
        }
    }

    /// Create a fuel depot payload.
    pub fn depot(
        id: PayloadId,
        depot_design_index: usize,
        serial_number: u32,
        name: String,
        capacity_kg: f64,
        dry_mass_kg: f64,
        insulated: bool,
    ) -> Self {
        Self {
            id,
            name,
            kind: PayloadKind::Depot {
                depot_design_index,
                capacity_kg,
                serial_number,
                insulated,
            },
            mass_kg: dry_mass_kg,
            power_watts: 0.0,
            power_draw_watts: 50.0,
            insulated,
            thermal_range_k: if insulated { (60.0, 300.0) } else { (150.0, 400.0) },
            hazard_susceptibility: HazardSusceptibility {
                radiation_sensitivity: 0.2,
                debris_sensitivity: 0.7,
                thermal_sensitivity: 0.5,
            },
        }
    }

    /// Whether this payload is a contract satellite.
    pub fn is_contract(&self) -> bool {
        matches!(self.kind, PayloadKind::ContractSatellite { .. })
    }

    /// Whether this payload is a fuel depot.
    pub fn is_depot(&self) -> bool {
        matches!(self.kind, PayloadKind::Depot { .. })
    }

    /// Contract reward, or 0.0 if not a contract payload.
    pub fn reward(&self) -> f64 {
        match &self.kind {
            PayloadKind::ContractSatellite { reward, .. } => *reward,
            _ => 0.0,
        }
    }

    /// Contract ID, if this is a contract payload.
    pub fn contract_id(&self) -> Option<u32> {
        match &self.kind {
            PayloadKind::ContractSatellite { contract_id, .. } => Some(*contract_id),
            _ => None,
        }
    }
}

/// Total mass of a slice of payloads.
pub fn total_mass(payloads: &[Payload]) -> f64 {
    payloads.iter().map(|p| p.mass_kg).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_satellite_creation() {
        let p = Payload::contract_satellite(1, 42, "Communications".to_string(), 500.0, 5_000_000.0);
        assert_eq!(p.id, 1);
        assert_eq!(p.mass_kg, 500.0);
        assert!(p.is_contract());
        assert!(!p.is_depot());
        assert_eq!(p.reward(), 5_000_000.0);
        assert_eq!(p.contract_id(), Some(42));
        assert_eq!(p.name, "Communications Satellite");
    }

    #[test]
    fn test_depot_creation() {
        let p = Payload::depot(2, 0, 1, "Depot Alpha".to_string(), 5000.0, 300.0, true);
        assert_eq!(p.id, 2);
        assert_eq!(p.mass_kg, 300.0);
        assert!(!p.is_contract());
        assert!(p.is_depot());
        assert_eq!(p.reward(), 0.0);
        assert_eq!(p.contract_id(), None);
        assert!(p.insulated);
        assert_eq!(p.power_draw_watts, 50.0);
        assert_eq!(p.hazard_susceptibility.radiation_sensitivity, 0.2);
        assert_eq!(p.hazard_susceptibility.debris_sensitivity, 0.7);
    }

    #[test]
    fn test_total_mass() {
        let payloads = vec![
            Payload::contract_satellite(1, 1, "Comms".to_string(), 500.0, 1_000_000.0),
            Payload::depot(2, 0, 1, "Depot".to_string(), 5000.0, 300.0, false),
        ];
        assert!((total_mass(&payloads) - 800.0).abs() < 0.01);
    }

    #[test]
    fn test_empty_payloads() {
        let payloads: Vec<Payload> = vec![];
        assert_eq!(total_mass(&payloads), 0.0);
    }

    #[test]
    fn test_hazard_neutral() {
        let h = HazardSusceptibility::neutral();
        assert_eq!(h.radiation_sensitivity, 0.5);
        assert_eq!(h.debris_sensitivity, 0.5);
        assert_eq!(h.thermal_sensitivity, 0.5);
    }
}
