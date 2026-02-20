/// Launch manifest: a collection of payloads (contracts and/or depots)
/// to be delivered on a single rocket launch, potentially to multiple destinations.

use crate::location::DELTA_V_MAP;

/// Type-specific manifest entry data.
#[derive(Debug, Clone)]
pub enum ManifestEntryKind {
    Contract {
        contract_id: u32,
        name: String,
        payload_type: String,
        reward: f64,
    },
    Depot {
        depot_design_index: usize,
        depot_serial: u32,
        depot_name: String,
        capacity_kg: f64,
        insulated: bool,
    },
}

/// A single entry in the launch manifest.
#[derive(Debug, Clone)]
pub struct ManifestEntry {
    pub entry_id: u32,
    pub kind: ManifestEntryKind,
    pub destination: String,         // location_id for drop-off
    pub destination_display: String,  // display name for UI
    pub mass_kg: f64,
}

impl ManifestEntry {
    /// Whether this entry is a contract.
    pub fn is_contract(&self) -> bool {
        matches!(self.kind, ManifestEntryKind::Contract { .. })
    }

    /// Whether this entry is a depot.
    pub fn is_depot(&self) -> bool {
        matches!(self.kind, ManifestEntryKind::Depot { .. })
    }

    /// Contract ID, if this is a contract entry.
    pub fn contract_id(&self) -> Option<u32> {
        match &self.kind {
            ManifestEntryKind::Contract { contract_id, .. } => Some(*contract_id),
            _ => None,
        }
    }

    /// Reward amount, or 0.0 if not a contract entry.
    pub fn reward(&self) -> f64 {
        match &self.kind {
            ManifestEntryKind::Contract { reward, .. } => *reward,
            _ => 0.0,
        }
    }

    /// Display name for this entry.
    pub fn display_name(&self) -> &str {
        match &self.kind {
            ManifestEntryKind::Contract { name, .. } => name,
            ManifestEntryKind::Depot { depot_name, .. } => depot_name,
        }
    }

    /// Entry type as a string for UI.
    pub fn entry_type(&self) -> &'static str {
        match &self.kind {
            ManifestEntryKind::Contract { .. } => "Contract",
            ManifestEntryKind::Depot { .. } => "Depot",
        }
    }
}

/// A collection of payloads to be launched together.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub entries: Vec<ManifestEntry>,
    next_entry_id: u32,
}

impl Manifest {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_entry_id: 1,
        }
    }

    /// Add a contract to the manifest. Returns the entry_id.
    pub fn add_contract(
        &mut self,
        contract_id: u32,
        name: String,
        payload_type: String,
        reward: f64,
        destination: String,
        destination_display: String,
        mass_kg: f64,
    ) -> u32 {
        let entry_id = self.next_entry_id;
        self.next_entry_id += 1;
        self.entries.push(ManifestEntry {
            entry_id,
            kind: ManifestEntryKind::Contract {
                contract_id,
                name,
                payload_type,
                reward,
            },
            destination,
            destination_display,
            mass_kg,
        });
        entry_id
    }

    /// Add a depot to the manifest. Returns the entry_id.
    pub fn add_depot(
        &mut self,
        depot_design_index: usize,
        depot_serial: u32,
        depot_name: String,
        capacity_kg: f64,
        insulated: bool,
        destination: String,
        destination_display: String,
        mass_kg: f64,
    ) -> u32 {
        let entry_id = self.next_entry_id;
        self.next_entry_id += 1;
        self.entries.push(ManifestEntry {
            entry_id,
            kind: ManifestEntryKind::Depot {
                depot_design_index,
                depot_serial,
                depot_name,
                capacity_kg,
                insulated,
            },
            destination,
            destination_display,
            mass_kg,
        });
        entry_id
    }

    /// Remove an entry by entry_id. Returns the removed entry if found.
    pub fn remove_entry(&mut self, entry_id: u32) -> Option<ManifestEntry> {
        if let Some(idx) = self.entries.iter().position(|e| e.entry_id == entry_id) {
            Some(self.entries.remove(idx))
        } else {
            None
        }
    }

    /// Total payload mass across all entries.
    pub fn total_mass_kg(&self) -> f64 {
        self.entries.iter().map(|e| e.mass_kg).sum()
    }

    /// Total reward across all contract entries.
    pub fn total_reward(&self) -> f64 {
        self.entries.iter().map(|e| e.reward()).sum()
    }

    /// Unique destination location_ids, sorted by delta-v from earth_surface (ascending).
    /// This gives the outward ordering: LEO before GEO before lunar.
    pub fn unique_destinations_sorted_by_delta_v(&self) -> Vec<String> {
        let mut dests: Vec<String> = Vec::new();
        for entry in &self.entries {
            if !dests.contains(&entry.destination) {
                dests.push(entry.destination.clone());
            }
        }
        dests.sort_by(|a, b| {
            let dv_a = DELTA_V_MAP.shortest_path("earth_surface", a)
                .map(|(_, dv)| dv)
                .unwrap_or(f64::INFINITY);
            let dv_b = DELTA_V_MAP.shortest_path("earth_surface", b)
                .map(|(_, dv)| dv)
                .unwrap_or(f64::INFINITY);
            dv_a.partial_cmp(&dv_b).unwrap_or(std::cmp::Ordering::Equal)
        });
        dests
    }

    /// Get entries for a specific destination.
    pub fn entries_for_destination(&self, destination: &str) -> Vec<&ManifestEntry> {
        self.entries.iter().filter(|e| e.destination == destination).collect()
    }

    /// Whether the manifest is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries from the manifest.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Get entry by index.
    pub fn get(&self, index: usize) -> Option<&ManifestEntry> {
        self.entries.get(index)
    }

    /// Get entry by entry_id.
    pub fn get_by_id(&self, entry_id: u32) -> Option<&ManifestEntry> {
        self.entries.iter().find(|e| e.entry_id == entry_id)
    }

    /// The farthest destination's delta-v from earth_surface.
    /// Returns 0.0 if empty.
    pub fn max_delta_v(&self) -> f64 {
        self.unique_destinations_sorted_by_delta_v()
            .last()
            .and_then(|dest| DELTA_V_MAP.shortest_path("earth_surface", dest))
            .map(|(_, dv)| dv)
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_manifest() {
        let m = Manifest::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
        assert_eq!(m.total_mass_kg(), 0.0);
        assert_eq!(m.total_reward(), 0.0);
        assert_eq!(m.max_delta_v(), 0.0);
        assert!(m.unique_destinations_sorted_by_delta_v().is_empty());
    }

    #[test]
    fn test_add_contract() {
        let mut m = Manifest::new();
        let id = m.add_contract(
            42, "SatCom - Comms".to_string(), "Communications".to_string(),
            5_000_000.0, "leo".to_string(), "Low Earth Orbit".to_string(), 500.0,
        );
        assert_eq!(id, 1);
        assert_eq!(m.len(), 1);
        assert!(!m.is_empty());

        let entry = m.get(0).unwrap();
        assert!(entry.is_contract());
        assert!(!entry.is_depot());
        assert_eq!(entry.contract_id(), Some(42));
        assert_eq!(entry.reward(), 5_000_000.0);
        assert_eq!(entry.mass_kg, 500.0);
        assert_eq!(entry.destination, "leo");
        assert_eq!(entry.display_name(), "SatCom - Comms");
        assert_eq!(entry.entry_type(), "Contract");
    }

    #[test]
    fn test_add_depot() {
        let mut m = Manifest::new();
        let id = m.add_depot(
            0, 1, "Depot Alpha".to_string(), 5000.0, false,
            "lunar_orbit".to_string(), "Lunar Orbit".to_string(), 300.0,
        );
        assert_eq!(id, 1);
        let entry = m.get(0).unwrap();
        assert!(entry.is_depot());
        assert!(!entry.is_contract());
        assert_eq!(entry.contract_id(), None);
        assert_eq!(entry.reward(), 0.0);
        assert_eq!(entry.mass_kg, 300.0);
        assert_eq!(entry.display_name(), "Depot Alpha");
        assert_eq!(entry.entry_type(), "Depot");
    }

    #[test]
    fn test_total_mass() {
        let mut m = Manifest::new();
        m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);
        m.add_contract(2, "C2".into(), "T2".into(), 2e6, "geo".into(), "GEO".into(), 1000.0);
        m.add_depot(0, 1, "D1".into(), 5000.0, false, "leo".into(), "LEO".into(), 300.0);
        assert!((m.total_mass_kg() - 1800.0).abs() < 0.01);
    }

    #[test]
    fn test_total_reward() {
        let mut m = Manifest::new();
        m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);
        m.add_contract(2, "C2".into(), "T2".into(), 2e6, "geo".into(), "GEO".into(), 1000.0);
        m.add_depot(0, 1, "D1".into(), 5000.0, false, "leo".into(), "LEO".into(), 300.0);
        assert!((m.total_reward() - 3e6).abs() < 0.01);
    }

    #[test]
    fn test_remove_entry() {
        let mut m = Manifest::new();
        let id1 = m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);
        let id2 = m.add_contract(2, "C2".into(), "T2".into(), 2e6, "geo".into(), "GEO".into(), 1000.0);

        let removed = m.remove_entry(id1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().contract_id(), Some(1));
        assert_eq!(m.len(), 1);
        assert_eq!(m.get(0).unwrap().entry_id, id2);

        // Removing non-existent returns None
        assert!(m.remove_entry(999).is_none());
    }

    #[test]
    fn test_unique_destinations_sorted() {
        let mut m = Manifest::new();
        // Add GEO first, then LEO — should be sorted by delta-v (LEO < GEO)
        m.add_contract(1, "C1".into(), "T1".into(), 1e6, "geo".into(), "GEO".into(), 500.0);
        m.add_contract(2, "C2".into(), "T2".into(), 2e6, "leo".into(), "LEO".into(), 500.0);
        m.add_contract(3, "C3".into(), "T3".into(), 1e6, "leo".into(), "LEO".into(), 200.0);

        let dests = m.unique_destinations_sorted_by_delta_v();
        assert_eq!(dests.len(), 2);
        assert_eq!(dests[0], "leo"); // LEO = 8100 m/s
        assert_eq!(dests[1], "geo"); // GEO = 12040 m/s
    }

    #[test]
    fn test_entries_for_destination() {
        let mut m = Manifest::new();
        m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);
        m.add_contract(2, "C2".into(), "T2".into(), 2e6, "geo".into(), "GEO".into(), 1000.0);
        m.add_contract(3, "C3".into(), "T3".into(), 1e6, "leo".into(), "LEO".into(), 200.0);

        let leo_entries = m.entries_for_destination("leo");
        assert_eq!(leo_entries.len(), 2);
        let geo_entries = m.entries_for_destination("geo");
        assert_eq!(geo_entries.len(), 1);
        let none_entries = m.entries_for_destination("lunar_surface");
        assert_eq!(none_entries.len(), 0);
    }

    #[test]
    fn test_max_delta_v() {
        let mut m = Manifest::new();
        m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);
        assert_eq!(m.max_delta_v(), 8100.0);

        m.add_contract(2, "C2".into(), "T2".into(), 2e6, "geo".into(), "GEO".into(), 1000.0);
        assert_eq!(m.max_delta_v(), 12040.0);
    }

    #[test]
    fn test_clear() {
        let mut m = Manifest::new();
        m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);
        m.add_contract(2, "C2".into(), "T2".into(), 2e6, "geo".into(), "GEO".into(), 1000.0);
        assert_eq!(m.len(), 2);

        m.clear();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn test_get_by_id() {
        let mut m = Manifest::new();
        let id = m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);
        assert!(m.get_by_id(id).is_some());
        assert!(m.get_by_id(999).is_none());
    }

    #[test]
    fn test_entry_ids_increment() {
        let mut m = Manifest::new();
        let id1 = m.add_contract(1, "C1".into(), "T1".into(), 1e6, "leo".into(), "LEO".into(), 500.0);
        let id2 = m.add_depot(0, 1, "D1".into(), 5000.0, false, "leo".into(), "LEO".into(), 300.0);
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }
}
