use std::collections::BinaryHeap;
use std::cmp::Ordering;
use std::sync::LazyLock;

/// Physical properties of a surface (planet or moon)
#[derive(Debug, Clone)]
pub struct SurfaceProperties {
    pub gravity_m_s2: f64,
    pub radius_m: f64,
    pub has_atmosphere: bool,
    pub atmosphere_density: f64,
}

impl SurfaceProperties {
    /// Calculate orbital velocity at the surface: sqrt(g * r)
    pub fn orbital_velocity(&self) -> f64 {
        (self.gravity_m_s2 * self.radius_m).sqrt()
    }
}

/// Type of location in the delta-v graph
#[derive(Debug, Clone)]
pub enum LocationType {
    Surface(SurfaceProperties),
    Orbit,
    LagrangePoint,
}

/// A location in the delta-v graph (orbit, surface, or Lagrange point)
#[derive(Debug, Clone)]
pub struct Location {
    pub id: &'static str,
    pub display_name: &'static str,
    pub short_name: &'static str,
    pub location_type: LocationType,
    pub parent_body: &'static str,
}

/// Animation type for a transfer between locations
#[derive(Debug, Clone)]
pub enum TransferAnimation {
    Launch,
    Landing,
}

/// A transfer edge in the delta-v graph
#[derive(Debug, Clone)]
pub struct Transfer {
    pub from: &'static str,
    pub to: &'static str,
    pub delta_v: f64,
    pub aero_drag_loss: f64,
    pub animation: Option<TransferAnimation>,
    pub can_aerobrake: bool,
    /// Transit time in game-days for this transfer leg
    pub transit_days: u32,
}

impl Transfer {
    /// Total delta-v cost including aerodynamic drag losses
    pub fn total_delta_v(&self) -> f64 {
        self.delta_v + self.aero_drag_loss
    }
}

/// The delta-v map: a directed graph of locations connected by transfers
pub struct DeltaVMap {
    locations: Vec<Location>,
    transfers: Vec<Transfer>,
}

/// Helper for Dijkstra's algorithm
#[derive(Debug)]
struct DijkstraState {
    cost: f64,
    node_index: usize,
}

impl PartialEq for DijkstraState {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.node_index == other.node_index
    }
}

impl Eq for DijkstraState {}

impl PartialOrd for DijkstraState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DijkstraState {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap
        other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
    }
}

impl DeltaVMap {
    /// Build the initial Earth-Moon delta-v graph
    pub fn earth_moon() -> Self {
        let locations = vec![
            Location {
                id: "earth_surface",
                display_name: "Earth Surface",
                short_name: "EARTH",
                location_type: LocationType::Surface(SurfaceProperties {
                    gravity_m_s2: 9.81,
                    radius_m: 6_371_000.0,
                    has_atmosphere: true,
                    atmosphere_density: 1.225,
                }),
                parent_body: "earth",
            },
            Location {
                id: "suborbital",
                display_name: "Suborbital",
                short_name: "SUB",
                location_type: LocationType::Orbit,
                parent_body: "earth",
            },
            Location {
                id: "leo",
                display_name: "Low Earth Orbit",
                short_name: "LEO",
                location_type: LocationType::Orbit,
                parent_body: "earth",
            },
            Location {
                id: "sso",
                display_name: "Sun-Synchronous Orbit",
                short_name: "SSO",
                location_type: LocationType::Orbit,
                parent_body: "earth",
            },
            Location {
                id: "meo",
                display_name: "Medium Earth Orbit",
                short_name: "MEO",
                location_type: LocationType::Orbit,
                parent_body: "earth",
            },
            Location {
                id: "gto",
                display_name: "Geostationary Transfer",
                short_name: "GTO",
                location_type: LocationType::Orbit,
                parent_body: "earth",
            },
            Location {
                id: "geo",
                display_name: "Geostationary Orbit",
                short_name: "GEO",
                location_type: LocationType::Orbit,
                parent_body: "earth",
            },
            Location {
                id: "l1",
                display_name: "Earth-Moon L1",
                short_name: "L1",
                location_type: LocationType::LagrangePoint,
                parent_body: "earth",
            },
            Location {
                id: "l2",
                display_name: "Earth-Moon L2",
                short_name: "L2",
                location_type: LocationType::LagrangePoint,
                parent_body: "earth",
            },
            Location {
                id: "lunar_orbit",
                display_name: "Lunar Orbit",
                short_name: "LLO",
                location_type: LocationType::Orbit,
                parent_body: "moon",
            },
            Location {
                id: "lunar_surface",
                display_name: "Lunar Surface",
                short_name: "MOON",
                location_type: LocationType::Surface(SurfaceProperties {
                    gravity_m_s2: 1.62,
                    radius_m: 1_737_000.0,
                    has_atmosphere: false,
                    atmosphere_density: 0.0,
                }),
                parent_body: "moon",
            },
        ];

        let transfers = vec![
            Transfer {
                from: "earth_surface",
                to: "suborbital",
                delta_v: 3500.0,
                aero_drag_loss: 0.0,
                animation: Some(TransferAnimation::Launch),
                can_aerobrake: false,
                transit_days: 0, // ballistic arc
            },
            Transfer {
                from: "earth_surface",
                to: "leo",
                delta_v: 7800.0,
                aero_drag_loss: 300.0,
                animation: Some(TransferAnimation::Launch),
                can_aerobrake: false,
                transit_days: 0, // same-day insertion
            },
            Transfer {
                from: "leo",
                to: "sso",
                delta_v: 500.0,
                aero_drag_loss: 0.0,
                animation: None,
                can_aerobrake: false,
                transit_days: 0, // direct from LEO
            },
            Transfer {
                from: "leo",
                to: "meo",
                delta_v: 2100.0,
                aero_drag_loss: 0.0,
                animation: None,
                can_aerobrake: false,
                transit_days: 0, // direct from LEO
            },
            Transfer {
                from: "leo",
                to: "gto",
                delta_v: 2440.0,
                aero_drag_loss: 0.0,
                animation: None,
                can_aerobrake: false,
                transit_days: 1, // direct burn
            },
            Transfer {
                from: "gto",
                to: "geo",
                delta_v: 1500.0,
                aero_drag_loss: 0.0,
                animation: None,
                can_aerobrake: false,
                transit_days: 0, // circularization burn
            },
            Transfer {
                from: "leo",
                to: "l1",
                delta_v: 3150.0,
                aero_drag_loss: 0.0,
                animation: None,
                can_aerobrake: false,
                transit_days: 5, // 3-body trajectory
            },
            Transfer {
                from: "l1",
                to: "lunar_orbit",
                delta_v: 700.0,
                aero_drag_loss: 0.0,
                animation: None,
                can_aerobrake: false,
                transit_days: 2,
            },
            Transfer {
                from: "leo",
                to: "lunar_orbit",
                delta_v: 3850.0,
                aero_drag_loss: 0.0,
                animation: None,
                can_aerobrake: false,
                transit_days: 4, // direct
            },
            Transfer {
                from: "lunar_orbit",
                to: "lunar_surface",
                delta_v: 1700.0,
                aero_drag_loss: 0.0,
                animation: Some(TransferAnimation::Landing),
                can_aerobrake: false,
                transit_days: 0, // powered descent
            },
            Transfer {
                from: "lunar_surface",
                to: "lunar_orbit",
                delta_v: 1700.0,
                aero_drag_loss: 0.0,
                animation: Some(TransferAnimation::Launch),
                can_aerobrake: false,
                transit_days: 0, // ascent
            },
        ];

        DeltaVMap {
            locations,
            transfers,
        }
    }

    /// Look up a location by ID
    pub fn location(&self, id: &str) -> Option<&Location> {
        self.locations.iter().find(|l| l.id == id)
    }

    /// Get all locations
    pub fn locations(&self) -> &[Location] {
        &self.locations
    }

    /// Get all transfers originating from a location
    pub fn transfers_from(&self, id: &str) -> Vec<&Transfer> {
        self.transfers.iter().filter(|t| t.from == id).collect()
    }

    /// Get a direct transfer between two locations (if one exists)
    pub fn transfer(&self, from: &str, to: &str) -> Option<&Transfer> {
        self.transfers.iter().find(|t| t.from == from && t.to == to)
    }

    /// Get surface properties for a location (None if not a surface)
    pub fn surface_properties(&self, id: &str) -> Option<&SurfaceProperties> {
        self.location(id).and_then(|l| match &l.location_type {
            LocationType::Surface(props) => Some(props),
            _ => None,
        })
    }

    /// Find shortest path between two locations using Dijkstra's algorithm
    /// Returns (path_of_location_ids, total_delta_v) or None if no path exists
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<(Vec<&'static str>, f64)> {
        let from_idx = self.locations.iter().position(|l| l.id == from)?;
        let to_idx = self.locations.iter().position(|l| l.id == to)?;

        let n = self.locations.len();
        let mut dist = vec![f64::INFINITY; n];
        let mut prev = vec![None; n];
        let mut heap = BinaryHeap::new();

        dist[from_idx] = 0.0;
        heap.push(DijkstraState {
            cost: 0.0,
            node_index: from_idx,
        });

        while let Some(DijkstraState { cost, node_index }) = heap.pop() {
            if node_index == to_idx {
                break;
            }

            if cost > dist[node_index] {
                continue;
            }

            let loc_id = self.locations[node_index].id;
            for transfer in self.transfers_from(loc_id) {
                if let Some(next_idx) = self.locations.iter().position(|l| l.id == transfer.to) {
                    let next_cost = cost + transfer.total_delta_v();
                    if next_cost < dist[next_idx] {
                        dist[next_idx] = next_cost;
                        prev[next_idx] = Some(node_index);
                        heap.push(DijkstraState {
                            cost: next_cost,
                            node_index: next_idx,
                        });
                    }
                }
            }
        }

        if dist[to_idx].is_infinite() {
            return None;
        }

        // Reconstruct path
        let mut path = Vec::new();
        let mut current = to_idx;
        while let Some(p) = prev[current] {
            path.push(self.locations[current].id);
            current = p;
        }
        path.push(self.locations[from_idx].id);
        path.reverse();

        Some((path, dist[to_idx]))
    }

    /// Number of locations in the map
    pub fn location_count(&self) -> usize {
        self.locations.len()
    }

    /// Get a location by index (for GDScript iteration)
    pub fn location_at(&self, index: usize) -> Option<&Location> {
        self.locations.get(index)
    }
}

/// Global delta-v map instance
pub static DELTA_V_MAP: LazyLock<DeltaVMap> = LazyLock::new(DeltaVMap::earth_moon);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_earth_surface_properties() {
        let map = DeltaVMap::earth_moon();
        let props = map.surface_properties("earth_surface").unwrap();
        assert_eq!(props.gravity_m_s2, 9.81);
        assert_eq!(props.radius_m, 6_371_000.0);
        assert!(props.has_atmosphere);
        assert_eq!(props.atmosphere_density, 1.225);
    }

    #[test]
    fn test_lunar_surface_properties() {
        let map = DeltaVMap::earth_moon();
        let props = map.surface_properties("lunar_surface").unwrap();
        assert_eq!(props.gravity_m_s2, 1.62);
        assert_eq!(props.radius_m, 1_737_000.0);
        assert!(!props.has_atmosphere);
        assert_eq!(props.atmosphere_density, 0.0);
    }

    #[test]
    fn test_orbital_velocity() {
        let map = DeltaVMap::earth_moon();
        let earth = map.surface_properties("earth_surface").unwrap();
        let v_earth = earth.orbital_velocity();
        // sqrt(9.81 * 6_371_000) ≈ 7905 m/s
        assert!((v_earth - 7905.0).abs() < 10.0,
            "Earth orbital velocity should be ~7905 m/s, got {}", v_earth);

        let moon = map.surface_properties("lunar_surface").unwrap();
        let v_moon = moon.orbital_velocity();
        // sqrt(1.62 * 1_737_000) ≈ 1677 m/s
        assert!((v_moon - 1677.0).abs() < 10.0,
            "Moon orbital velocity should be ~1677 m/s, got {}", v_moon);
    }

    #[test]
    fn test_location_count() {
        let map = DeltaVMap::earth_moon();
        assert_eq!(map.location_count(), 11);
    }

    #[test]
    fn test_location_lookup() {
        let map = DeltaVMap::earth_moon();
        let leo = map.location("leo").unwrap();
        assert_eq!(leo.display_name, "Low Earth Orbit");
        assert_eq!(leo.short_name, "LEO");
        assert_eq!(leo.parent_body, "earth");
        assert!(matches!(leo.location_type, LocationType::Orbit));
    }

    #[test]
    fn test_location_not_found() {
        let map = DeltaVMap::earth_moon();
        assert!(map.location("mars").is_none());
    }

    #[test]
    fn test_direct_transfer() {
        let map = DeltaVMap::earth_moon();
        let t = map.transfer("earth_surface", "leo").unwrap();
        assert_eq!(t.delta_v, 7800.0);
        assert_eq!(t.aero_drag_loss, 300.0);
        assert_eq!(t.total_delta_v(), 8100.0);
        assert!(matches!(t.animation, Some(TransferAnimation::Launch)));
    }

    #[test]
    fn test_no_direct_transfer() {
        let map = DeltaVMap::earth_moon();
        assert!(map.transfer("earth_surface", "geo").is_none());
    }

    #[test]
    fn test_transfers_from_leo() {
        let map = DeltaVMap::earth_moon();
        let transfers = map.transfers_from("leo");
        assert_eq!(transfers.len(), 5); // sso, meo, gto, l1, lunar_orbit
    }

    #[test]
    fn test_shortest_path_direct() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("earth_surface", "leo").unwrap();
        assert_eq!(path, vec!["earth_surface", "leo"]);
        assert_eq!(dv, 8100.0); // 7800 + 300 drag
    }

    #[test]
    fn test_shortest_path_multi_hop() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("earth_surface", "geo").unwrap();
        // earth_surface -> leo -> gto -> geo
        assert_eq!(path, vec!["earth_surface", "leo", "gto", "geo"]);
        // 8100 + 2440 + 1500 = 12040
        assert_eq!(dv, 12040.0);
    }

    #[test]
    fn test_shortest_path_to_lunar_surface() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("earth_surface", "lunar_surface").unwrap();
        // earth_surface -> leo -> lunar_orbit -> lunar_surface
        assert_eq!(path, vec!["earth_surface", "leo", "lunar_orbit", "lunar_surface"]);
        // 8100 + 3850 + 1700 = 13650
        assert_eq!(dv, 13650.0);
    }

    #[test]
    fn test_shortest_path_via_l1() {
        let map = DeltaVMap::earth_moon();
        // leo -> l1 -> lunar_orbit = 3150 + 700 = 3850
        // leo -> lunar_orbit (direct) = 3850
        // Both are equal; the direct path should be chosen (fewer hops, same cost)
        let (path, dv) = map.shortest_path("leo", "lunar_orbit").unwrap();
        assert_eq!(dv, 3850.0);
        // Either path is valid since they have the same cost
        assert!(path.len() <= 3);
    }

    #[test]
    fn test_shortest_path_no_route() {
        let map = DeltaVMap::earth_moon();
        // No path from LEO back to Earth surface (no re-entry transfer)
        assert!(map.shortest_path("leo", "earth_surface").is_none());
    }

    #[test]
    fn test_lunar_round_trip() {
        let map = DeltaVMap::earth_moon();
        // lunar_surface -> lunar_orbit exists
        let t = map.transfer("lunar_surface", "lunar_orbit").unwrap();
        assert_eq!(t.delta_v, 1700.0);
        assert!(matches!(t.animation, Some(TransferAnimation::Launch)));
    }

    #[test]
    fn test_location_at() {
        let map = DeltaVMap::earth_moon();
        let first = map.location_at(0).unwrap();
        assert_eq!(first.id, "earth_surface");
        assert!(map.location_at(100).is_none());
    }

    #[test]
    fn test_lagrange_points() {
        let map = DeltaVMap::earth_moon();
        let l1 = map.location("l1").unwrap();
        assert!(matches!(l1.location_type, LocationType::LagrangePoint));
        let l2 = map.location("l2").unwrap();
        assert!(matches!(l2.location_type, LocationType::LagrangePoint));
    }

    #[test]
    fn test_static_delta_v_map() {
        // Verify the lazy static works
        assert_eq!(DELTA_V_MAP.location_count(), 11);
        assert!(DELTA_V_MAP.location("leo").is_some());
    }

    #[test]
    fn test_surface_properties_for_orbit_returns_none() {
        let map = DeltaVMap::earth_moon();
        assert!(map.surface_properties("leo").is_none());
        assert!(map.surface_properties("l1").is_none());
    }

    #[test]
    fn test_transfer_transit_days() {
        let map = DeltaVMap::earth_moon();

        // Instant transfers
        assert_eq!(map.transfer("earth_surface", "suborbital").unwrap().transit_days, 0);
        assert_eq!(map.transfer("earth_surface", "leo").unwrap().transit_days, 0);
        assert_eq!(map.transfer("leo", "sso").unwrap().transit_days, 0);
        assert_eq!(map.transfer("leo", "meo").unwrap().transit_days, 0);
        assert_eq!(map.transfer("gto", "geo").unwrap().transit_days, 0);
        assert_eq!(map.transfer("lunar_orbit", "lunar_surface").unwrap().transit_days, 0);
        assert_eq!(map.transfer("lunar_surface", "lunar_orbit").unwrap().transit_days, 0);

        // Multi-day transfers
        assert_eq!(map.transfer("leo", "gto").unwrap().transit_days, 1);
        assert_eq!(map.transfer("leo", "l1").unwrap().transit_days, 5);
        assert_eq!(map.transfer("l1", "lunar_orbit").unwrap().transit_days, 2);
        assert_eq!(map.transfer("leo", "lunar_orbit").unwrap().transit_days, 4);
    }
}
