//! The location graph: bodies, orbits and the transfers between them,
//! priced in delta-v, with the shortest-path searches over it. The
//! graph's contents live in `graph_data`; the ascent integrator that
//! prices a climb off a surface lives in `ascent` (17_REFACTOR.md E8).

mod ascent;
mod graph_data;

pub use ascent::{
    simulate_ascent, simulate_ascent_with, simulate_gravity_losses, AscentGroupResult, AscentNozzle,
    AscentPhase, AscentProfile, ASCENT_TIMESTEP_S, DEFAULT_ASCENT_PROFILE, KICK_OVER_VELOCITY,
    LEGACY_GRAVITY_TURN, PITCH_KICK_RAD, PITCH_SCHEDULE_PRESSURE_PA,
};

use std::collections::{BinaryHeap, HashMap};
use std::cmp::Ordering;
use std::sync::{LazyLock, OnceLock};

/// Physical properties of a surface (planet or moon)
#[derive(Debug, Clone)]
pub struct SurfaceProperties {
    pub gravity_m_s2: f64,
    pub radius_m: f64,
    pub has_atmosphere: bool,
    pub atmosphere_density: f64,
    /// Ambient pressure at the surface in Pascals (e.g. 101_325 for Earth).
    pub ambient_pressure_pa: f64,
    /// Atmospheric scale height in metres: pressure falls by e every
    /// `scale_height_m` of altitude (Earth ~8.5 km). 0 for an airless
    /// body, or to hold the ambient value all the way up.
    pub scale_height_m: f64,
}

impl SurfaceProperties {
    /// Ambient pressure `altitude_m` above the surface: an exponential
    /// atmosphere with the body's scale height. Without an atmosphere,
    /// or without a scale height on record, the surface value holds
    /// everywhere (which is 0 for an airless body).
    pub fn pressure_at(&self, altitude_m: f64) -> f64 {
        if !self.has_atmosphere || self.scale_height_m <= 0.0 {
            return self.ambient_pressure_pa;
        }
        self.ambient_pressure_pa * (-altitude_m.max(0.0) / self.scale_height_m).exp()
    }

    /// The altitude above which pressure has fallen below `pressure_pa`:
    /// 0 when it already has at the surface, ∞ when the ambient value
    /// holds all the way up.
    pub fn altitude_where_pressure_falls_to(&self, pressure_pa: f64) -> f64 {
        if !self.has_atmosphere || self.ambient_pressure_pa <= pressure_pa {
            return 0.0;
        }
        if self.scale_height_m <= 0.0 {
            return f64::INFINITY;
        }
        self.scale_height_m * (self.ambient_pressure_pa / pressure_pa).ln()
    }

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
    /// Mean heliocentric distance in AU, set once by
    /// [`DeltaVMap::from_parts`] from the parent-body table (see
    /// [`Location::sun_distance_au`]). Builders leave it 0.
    pub sun_distance_au: f64,
}

/// Mean heliocentric distance in AU for a parent body. Used for
/// solar-panel power scaling. NEAs and Lagrange points inherit Earth's
/// 1.0 AU since they orbit near it.
pub fn parent_body_sun_distance_au(parent: &str) -> f64 {
    match parent {
        "sun" => 0.0,    // sentinel — heliocentric "transfer" nodes
        "mercury" => 0.39,
        "venus" => 0.72,
        "earth" => 1.0,
        "moon" => 1.0,
        "mars" => 1.52,
        "phobos" | "deimos" => 1.52,
        "vesta" => 2.36,
        "ceres" => 2.77,
        // 3.1417 AU semi-major axis — genuinely within a whisker of π,
        // which is also why clippy::approx_constant needs the extra digits.
        "hygiea" => 3.1417,
        "eros" => 1.46,
        "bennu" => 1.13,
        _ => 1.0,
    }
}

impl Location {
    /// Distance from the Sun in AU, for solar flux at this node.
    pub fn sun_distance_au(&self) -> f64 {
        self.sun_distance_au
    }

    /// The rule the field is filled from. Heliocentric "X_transfer" and
    /// "X_escape" nodes (parent_body = "sun") take X's heliocentric
    /// distance so the burn at that node sees the right solar flux —
    /// e.g. `mars_transfer` reports 1.52 AU even though it's filed
    /// under the Sun. Everything else uses its parent body.
    fn derive_sun_distance_au(&self) -> f64 {
        if self.parent_body == "sun" {
            if let Some(prefix) = self.id.strip_suffix("_transfer") {
                let d = parent_body_sun_distance_au(prefix);
                if d > 0.0 { return d; }
            }
            if let Some(prefix) = self.id.strip_suffix("_escape") {
                let d = parent_body_sun_distance_au(prefix);
                if d > 0.0 { return d; }
            }
        }
        let d = parent_body_sun_distance_au(self.parent_body);
        if d == 0.0 { 1.0 } else { d }
    }
}

/// A transfer edge in the delta-v graph
#[derive(Debug, Clone)]
pub struct Transfer {
    pub from: &'static str,
    pub to: &'static str,
    pub delta_v: f64,
    pub through_atmosphere: bool,
    pub can_aerobrake: bool,
    /// Transit time in game-days for this transfer leg
    pub transit_days: u32,
    /// Whether low-thrust vehicles can use this edge.
    pub low_thrust_ok: bool,
    /// Delta-v cost for low-thrust vehicles (spiral transfers cost more).
    /// When None, uses standard delta_v.
    pub low_thrust_delta_v: Option<f64>,
}

/// Estimate aerodynamic drag loss for a launch through atmosphere.
/// Larger rockets have proportionally less drag loss (better ballistic coefficient).
pub fn aero_drag_loss(rocket_mass_kg: f64) -> f64 {
    // Drag loss model: base 300 m/s scaled by (reference_mass / actual_mass)^0.2
    // Heavier rockets push through atmosphere more efficiently.
    let reference_mass = 500_000.0; // ~Falcon 9 class
    let base_loss = 300.0;
    base_loss * (reference_mass / rocket_mass_kg.max(1.0)).powf(0.2)
}

impl Transfer {
    /// What this edge costs a vehicle of `rocket_mass_kg`: the edge's
    /// delta-v, plus drag if it climbs through an atmosphere.
    pub fn cost_for_mass(&self, rocket_mass_kg: f64) -> f64 {
        if self.through_atmosphere {
            self.delta_v + aero_drag_loss(rocket_mass_kg)
        } else {
            self.delta_v
        }
    }

    /// Delta-v cost for a given engine capability.
    /// Low-thrust engines use low_thrust_delta_v if available, else standard.
    pub fn delta_v_for(&self, low_thrust: bool, rocket_mass_kg: f64) -> Option<f64> {
        if low_thrust {
            if !self.low_thrust_ok {
                return None; // edge not usable by low-thrust
            }
            let dv = self.low_thrust_delta_v.unwrap_or(self.delta_v);
            if self.through_atmosphere {
                Some(dv + aero_drag_loss(rocket_mass_kg))
            } else {
                Some(dv)
            }
        } else {
            Some(self.cost_for_mass(rocket_mass_kg))
        }
    }
}

/// The delta-v map: a directed graph of locations connected by transfers.
///
/// Built once by [`DeltaVMap::from_parts`], which derives everything the
/// searches need from the two lists (17_3_PHYSICS.md D3): an id → index
/// map, adjacency lists in transfer insertion order (so equal-cost
/// paths tie-break as they always did), and a slot per goal for the A*
/// heuristic, filled on first use.
pub struct DeltaVMap {
    locations: Vec<Location>,
    transfers: Vec<Transfer>,
    index: HashMap<&'static str, usize>,
    /// Indices into `transfers` leaving each location, insertion order.
    adjacency: Vec<Vec<usize>>,
    /// `heuristic_to(goal)` results, computed once per goal.
    heuristics: Vec<OnceLock<Vec<f64>>>,
}

/// A node on a search heap, cheapest first.
#[derive(Debug)]
pub(crate) struct SearchState {
    pub cost: f64,
    pub node: usize,
}

impl PartialEq for SearchState {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.node == other.node
    }
}

impl Eq for SearchState {}

impl PartialOrd for SearchState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SearchState {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap
        other.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
    }
}

impl DeltaVMap {
    /// Assemble the map from its data, deriving the index, the adjacency
    /// lists, the heuristic slots and each location's solar distance.
    pub(crate) fn from_parts(mut locations: Vec<Location>, transfers: Vec<Transfer>) -> Self {
        for loc in &mut locations {
            loc.sun_distance_au = loc.derive_sun_distance_au();
        }
        let index: HashMap<&'static str, usize> =
            locations.iter().enumerate().map(|(i, l)| (l.id, i)).collect();
        assert_eq!(index.len(), locations.len(), "location ids must be unique");
        let mut adjacency = vec![Vec::new(); locations.len()];
        for (ti, t) in transfers.iter().enumerate() {
            let from = *index.get(t.from)
                .unwrap_or_else(|| panic!("transfer from unknown location {}", t.from));
            assert!(index.contains_key(t.to), "transfer to unknown location {}", t.to);
            adjacency[from].push(ti);
        }
        let heuristics = (0..locations.len()).map(|_| OnceLock::new()).collect();
        DeltaVMap { locations, transfers, index, adjacency, heuristics }
    }

    /// The index of a location id, as `location_at` counts.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    /// Look up a location by ID
    pub fn location(&self, id: &str) -> Option<&Location> {
        self.index_of(id).map(|i| &self.locations[i])
    }

    /// Get all locations
    pub fn locations(&self) -> &[Location] {
        &self.locations
    }

    /// Get all transfers originating from a location, in the order they
    /// were added.
    pub fn transfers_from(&self, id: &str) -> Vec<&Transfer> {
        self.index_of(id)
            .map(|i| self.adjacency[i].iter().map(|&ti| &self.transfers[ti]).collect())
            .unwrap_or_default()
    }

    /// Get a direct transfer between two locations (if one exists)
    pub fn transfer(&self, from: &str, to: &str) -> Option<&Transfer> {
        let i = self.index_of(from)?;
        self.adjacency[i].iter().map(|&ti| &self.transfers[ti]).find(|t| t.to == to)
    }

    /// Lower-bound delta-v from every node to `goal_idx`, computed once
    /// per goal: Dijkstra backwards over a best-case graph where each
    /// transfer costs `min(delta_v, low_thrust_delta_v)` with drag
    /// stripped (drag only adds cost). The A* heuristic.
    pub(crate) fn heuristic_to(&self, goal_idx: usize) -> &[f64] {
        self.heuristics[goal_idx].get_or_init(|| self.compute_heuristic(goal_idx))
    }

    fn compute_heuristic(&self, goal_idx: usize) -> Vec<f64> {
        let n = self.locations.len();
        let mut h = vec![f64::INFINITY; n];
        h[goal_idx] = 0.0;

        // Reverse adjacency: for each node `to`, incoming `(from, cheapest_dv)`,
        // one entry per (from, to) pair — the first transfer between them,
        // as `transfer()` answers.
        let mut incoming: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for (to_idx, to) in self.locations.iter().enumerate() {
            for (from_idx, from) in self.locations.iter().enumerate() {
                if let Some(t) = self.transfer(from.id, to.id) {
                    let cheap = t.low_thrust_delta_v
                        .map(|lt| lt.min(t.delta_v))
                        .unwrap_or(t.delta_v);
                    incoming[to_idx].push((from_idx, cheap));
                }
            }
        }

        let mut heap = BinaryHeap::new();
        heap.push(SearchState { cost: 0.0, node: goal_idx });
        while let Some(SearchState { cost, node }) = heap.pop() {
            if cost > h[node] { continue; }
            for &(from_idx, edge) in &incoming[node] {
                let next = cost + edge;
                if next < h[from_idx] {
                    h[from_idx] = next;
                    heap.push(SearchState { cost: next, node: from_idx });
                }
            }
        }
        h
    }

    /// Get surface properties for a location (None if not a surface)
    pub fn surface_properties(&self, id: &str) -> Option<&SurfaceProperties> {
        self.location(id).and_then(|l| match &l.location_type {
            LocationType::Surface(props) => Some(props),
            _ => None,
        })
    }

    /// Find shortest path between two locations using Dijkstra's algorithm.
    /// `rocket_mass_kg` is used to compute atmospheric drag losses.
    /// Returns (path_of_location_ids, total_cost) or None if no path exists.
    pub fn shortest_path(&self, from: &str, to: &str, rocket_mass_kg: f64) -> Option<(Vec<&'static str>, f64)> {
        self.dijkstra(from, to, |t| Some(t.cost_for_mass(rocket_mass_kg)))
    }

    /// Find shortest path with engine capability constraint.
    /// If `low_thrust` is true, only edges with `low_thrust_ok` are used,
    /// and `low_thrust_delta_v` is preferred when available.
    pub fn shortest_path_constrained(
        &self, from: &str, to: &str, rocket_mass_kg: f64, low_thrust: bool,
    ) -> Option<(Vec<&'static str>, f64)> {
        self.dijkstra(from, to, |t| t.delta_v_for(low_thrust, rocket_mass_kg))
    }

    /// Dijkstra over the graph with `edge_cost` pricing each transfer
    /// (`None` = not usable). Returns the path as location ids and its
    /// cost, or `None` when the goal is unreachable.
    fn dijkstra(
        &self, from: &str, to: &str, edge_cost: impl Fn(&Transfer) -> Option<f64>,
    ) -> Option<(Vec<&'static str>, f64)> {
        let from_idx = self.index_of(from)?;
        let to_idx = self.index_of(to)?;

        let n = self.locations.len();
        let mut dist = vec![f64::INFINITY; n];
        let mut prev = vec![None; n];
        let mut heap = BinaryHeap::new();

        dist[from_idx] = 0.0;
        heap.push(SearchState { cost: 0.0, node: from_idx });

        while let Some(SearchState { cost, node }) = heap.pop() {
            if node == to_idx { break; }
            if cost > dist[node] { continue; }

            for &ti in &self.adjacency[node] {
                let transfer = &self.transfers[ti];
                let Some(dv) = edge_cost(transfer) else { continue };
                let Some(next_idx) = self.index_of(transfer.to) else { continue };
                let next_cost = cost + dv;
                if next_cost < dist[next_idx] {
                    dist[next_idx] = next_cost;
                    prev[next_idx] = Some(node);
                    heap.push(SearchState { cost: next_cost, node: next_idx });
                }
            }
        }

        if dist[to_idx].is_infinite() { return None; }

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

    /// Get a location by index (for iteration)
    pub fn location_at(&self, index: usize) -> Option<&Location> {
        self.locations.get(index)
    }
}

/// Global delta-v map instance
pub static DELTA_V_MAP: LazyLock<DeltaVMap> = LazyLock::new(DeltaVMap::earth_moon);

/// A location in [`DELTA_V_MAP`], by index (17_3_PHYSICS.md D3b). `Copy`
/// and cheap to compare; the vehicle-side structs (flights, legs,
/// spacecraft, the planner) hold one of these where they held the id
/// string. Serialises as the id string, so saves stay readable and do
/// not depend on table order; an id the map does not know is a load
/// error, never a silent default. Authored content — contract and
/// market destinations, balance TOML — keeps the string form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocationId(u16);

impl LocationId {
    /// The location with this id, if the map has one.
    pub fn parse(name: &str) -> Option<Self> {
        DELTA_V_MAP.index_of(name).map(|i| LocationId(i as u16))
    }

    /// For ids the code itself names (`LocationId::of("earth_surface")`)
    /// and for tests: panics on an unknown id, which is a bug, not data.
    pub fn of(name: &str) -> Self {
        Self::parse(name).unwrap_or_else(|| panic!("unknown location id {name:?}"))
    }

    /// Index into `DELTA_V_MAP.locations()`.
    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn location(self) -> &'static Location {
        DELTA_V_MAP.location_at(self.index()).expect("LocationId indexes the map")
    }

    /// The id string, as saves and the content data spell it.
    pub fn name(self) -> &'static str {
        self.location().id
    }

    pub fn surface(self) -> Option<&'static SurfaceProperties> {
        match &self.location().location_type {
            LocationType::Surface(props) => Some(props),
            _ => None,
        }
    }
}

impl std::fmt::Display for LocationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl serde::Serialize for LocationId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> serde::Deserialize<'de> for LocationId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        LocationId::parse(&name)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown location id {name:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `LocationId` is an index at runtime and the id string in a save.
    #[test]
    fn location_id_round_trips_as_its_name_and_rejects_strangers() {
        let leo = LocationId::of("leo");
        assert_eq!(leo.name(), "leo");
        assert_eq!(leo.location().display_name, "Low Earth Orbit");
        assert!(leo.surface().is_none());
        assert!(LocationId::of("earth_surface").surface().is_some());
        assert_eq!(LocationId::parse("leo"), Some(leo));
        assert_eq!(LocationId::parse("narnia"), None);

        let json = serde_json::to_string(&leo).unwrap();
        assert_eq!(json, "\"leo\"", "saves spell a location by its id");
        let back: LocationId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, leo);
        let err = serde_json::from_str::<LocationId>("\"narnia\"").unwrap_err();
        assert!(err.to_string().contains("unknown location id"), "{err}");
    }

    /// Reference mass for tests — produces exactly 300 m/s drag loss
    const REF_MASS: f64 = 500_000.0;

    /// A journey pays for the atmosphere at most once, on the way up:
    /// the only edges flagged `through_atmosphere` leave the surface of
    /// a body that has one. Descent is `can_aerobrake` (a future
    /// saving, not a cost) until EDL is modelled. The planner's split —
    /// gravity and nozzle losses charged to the groups that burn during
    /// the ascent, drag charged on this one edge — rests on this.
    #[test]
    fn only_surface_ascents_are_atmospheric() {
        let map = &*DELTA_V_MAP;
        let atmospheric: Vec<&Transfer> = map.transfers.iter()
            .filter(|t| t.through_atmosphere)
            .collect();
        assert!(!atmospheric.is_empty(), "Earth's ascent edge should be flagged");
        for t in atmospheric {
            let from = map.surface_properties(t.from);
            assert!(
                from.is_some_and(|p| p.has_atmosphere),
                "{} → {}: an atmospheric edge must leave a surface with an atmosphere", t.from, t.to,
            );
            assert!(
                map.surface_properties(t.to).is_none(),
                "{} → {}: an atmospheric edge must climb to orbit, not land", t.from, t.to,
            );
        }
        for t in map.transfers.iter().filter(|t| t.can_aerobrake) {
            assert!(!t.through_atmosphere, "{} → {}: descent is never charged drag", t.from, t.to);
        }
    }

    #[test]
    fn test_aero_drag_loss_reference() {
        let loss = aero_drag_loss(REF_MASS);
        assert!((loss - 300.0).abs() < 0.1, "Reference mass should give ~300 m/s drag, got {}", loss);
    }

    #[test]
    fn test_aero_drag_heavier_less_loss() {
        let light = aero_drag_loss(100_000.0);
        let heavy = aero_drag_loss(1_000_000.0);
        assert!(heavy < light, "Heavier rocket should have less drag loss");
    }

    #[test]
    fn test_transfer_no_atmosphere() {
        let t = Transfer {
            from: "leo", to: "gto", delta_v: 2440.0,
            through_atmosphere: false, can_aerobrake: false, transit_days: 1, low_thrust_ok: true, low_thrust_delta_v: None,
        };
        assert_eq!(t.cost_for_mass(REF_MASS), 2440.0);
    }

    #[test]
    fn test_transfer_through_atmosphere() {
        let t = Transfer {
            from: "earth_surface", to: "leo", delta_v: 7800.0,
            through_atmosphere: true, can_aerobrake: false, transit_days: 0, low_thrust_ok: true, low_thrust_delta_v: None,
        };
        let total = t.cost_for_mass(REF_MASS);
        assert!((total - 8100.0).abs() < 1.0, "Should be ~8100, got {}", total);
    }

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
    }

    #[test]
    fn test_orbital_velocity() {
        let map = DeltaVMap::earth_moon();
        let earth = map.surface_properties("earth_surface").unwrap();
        let v_earth = earth.orbital_velocity();
        assert!((v_earth - 7905.0).abs() < 10.0, "got {}", v_earth);

        let moon = map.surface_properties("lunar_surface").unwrap();
        let v_moon = moon.orbital_velocity();
        assert!((v_moon - 1677.0).abs() < 10.0, "got {}", v_moon);
    }

    #[test]
    fn test_location_count() {
        let map = DeltaVMap::earth_moon();
        assert_eq!(map.location_count(), 50);
    }

    #[test]
    fn test_location_lookup() {
        let map = DeltaVMap::earth_moon();
        let leo = map.location("leo").unwrap();
        assert_eq!(leo.display_name, "Low Earth Orbit");
        assert_eq!(leo.short_name, "LEO");
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
        assert!(t.through_atmosphere);
        let total = t.cost_for_mass(REF_MASS);
        assert!((total - 8100.0).abs() < 1.0);
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
        assert_eq!(transfers.len(), 7); // sso, meo, gto, l1, lunar_orbit, l2, nea
    }

    #[test]
    fn test_shortest_path_direct() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("earth_surface", "leo", REF_MASS).unwrap();
        assert_eq!(path, vec!["earth_surface", "leo"]);
        assert!((dv - 8100.0).abs() < 1.0);
    }

    #[test]
    fn test_shortest_path_multi_hop() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("earth_surface", "geo", REF_MASS).unwrap();
        // Direct-to-GTO beats stopping in LEO on the way: 10100 + drag + 1500
        // against 7800 + drag + 2440 + 1500.
        assert_eq!(path, vec!["earth_surface", "gto", "geo"]);
        // 10100 + 300 drag + 1500 = 11900
        assert!((dv - 11900.0).abs() < 1.0, "got {dv}");
    }

    #[test]
    fn test_shortest_path_to_lunar_surface() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("earth_surface", "lunar_surface", REF_MASS).unwrap();
        assert_eq!(path, vec!["earth_surface", "leo", "lunar_orbit", "lunar_surface"]);
        // 8100 + 3850 + 1700 = 13650
        assert!((dv - 13650.0).abs() < 1.0);
    }

    #[test]
    fn test_shortest_path_via_l1() {
        let map = DeltaVMap::earth_moon();
        let (_path, dv) = map.shortest_path("leo", "lunar_orbit", REF_MASS).unwrap();
        assert_eq!(dv, 3850.0);
    }

    #[test]
    fn test_shortest_path_descent_to_earth_surface() {
        // With symmetric ascent/descent edges, leo → earth_surface is now
        // reachable (descent at the same nominal dv as ascent; aerobrake
        // savings will be a future feature).
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("leo", "earth_surface", REF_MASS).unwrap();
        assert_eq!(path, vec!["leo", "earth_surface"]);
        // Same nominal dv as ascent (without the ascent's drag penalty).
        assert!((dv - 7800.0).abs() < 1.0);
    }

    #[test]
    fn test_lunar_round_trip() {
        let map = DeltaVMap::earth_moon();
        let t = map.transfer("lunar_surface", "lunar_orbit").unwrap();
        assert_eq!(t.delta_v, 1700.0);
        assert!(!t.through_atmosphere);
    }

    #[test]
    fn test_location_at() {
        let map = DeltaVMap::earth_moon();
        assert_eq!(map.location_at(0).unwrap().id, "earth_surface");
        assert!(map.location_at(100).is_none());
    }

    #[test]
    fn test_lagrange_points() {
        let map = DeltaVMap::earth_moon();
        assert!(matches!(map.location("l1").unwrap().location_type, LocationType::LagrangePoint));
        assert!(matches!(map.location("l2").unwrap().location_type, LocationType::LagrangePoint));
    }

    #[test]
    fn test_static_delta_v_map() {
        assert_eq!(DELTA_V_MAP.location_count(), 50);
        assert!(DELTA_V_MAP.location("leo").is_some());
    }

    #[test]
    fn test_surface_properties_for_orbit_returns_none() {
        let map = DeltaVMap::earth_moon();
        assert!(map.surface_properties("leo").is_none());
        assert!(map.surface_properties("l1").is_none());
    }

    #[test]
    fn test_reverse_transfer_geo_to_leo() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("geo", "leo", REF_MASS).unwrap();
        assert_eq!(path, vec!["geo", "gto", "leo"]);
        assert_eq!(dv, 3940.0);
    }

    #[test]
    fn test_reverse_transfer_meo_to_leo() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("meo", "leo", REF_MASS).unwrap();
        assert_eq!(path, vec!["meo", "leo"]);
        assert_eq!(dv, 2100.0);
    }

    #[test]
    fn test_reverse_transfer_lunar_orbit_to_leo() {
        let map = DeltaVMap::earth_moon();
        let (_path, dv) = map.shortest_path("lunar_orbit", "leo", REF_MASS).unwrap();
        assert_eq!(dv, 3850.0);
    }

    #[test]
    fn test_reverse_transfer_sso_to_leo() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("sso", "leo", REF_MASS).unwrap();
        // Still the cheapest way down for something already in SSO — the
        // plane change is dreadful, but landing and relaunching (8500 + 7800
        // + drag) is worse.
        assert_eq!(path, vec!["sso", "leo"]);
        assert_eq!(dv, 8700.0);
    }

    #[test]
    fn launches_to_sso_and_gto_go_direct_not_through_leo() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("earth_surface", "sso", REF_MASS).unwrap();
        assert_eq!(path, vec!["earth_surface", "sso"]);
        assert!((dv - 8800.0).abs() < 1.0, "8500 + 300 drag, got {dv}");

        let (path, dv) = map.shortest_path("earth_surface", "gto", REF_MASS).unwrap();
        assert_eq!(path, vec!["earth_surface", "gto"]);
        assert!((dv - 10_400.0).abs() < 1.0, "10100 + 300 drag, got {dv}");
    }

    #[test]
    fn low_thrust_pays_more_than_impulsive_for_the_sso_plane_change() {
        // The one edge in the map where electric propulsion is the *worse*
        // option: Edelbaum's combined transfer beats a single impulse only for
        // small plane changes, and 70° is not small.
        let map = DeltaVMap::earth_moon();
        let (_, dv_high) = map
            .shortest_path_constrained("leo", "sso", REF_MASS, false).unwrap();
        let (_, dv_low) = map
            .shortest_path_constrained("leo", "sso", REF_MASS, true).unwrap();
        assert!(dv_low > dv_high, "low {dv_low} should exceed high {dv_high}");
    }

    #[test]
    fn test_l2_to_lunar_orbit() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("l2", "lunar_orbit", REF_MASS).unwrap();
        assert_eq!(path, vec!["l2", "lunar_orbit"]);
        assert_eq!(dv, 800.0);
    }

    #[test]
    fn test_leo_to_l2() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("leo", "l2", REF_MASS).unwrap();
        assert_eq!(path, vec!["leo", "l2"]);
        assert_eq!(dv, 3200.0);
    }

    #[test]
    fn test_cross_orbit_geo_to_meo() {
        let map = DeltaVMap::earth_moon();
        let (path, dv) = map.shortest_path("geo", "meo", REF_MASS).unwrap();
        // Direct geo→meo edge exists now (2000 m/s)
        assert_eq!(path, vec!["geo", "meo"]);
        assert!((dv - 2000.0).abs() < 1.0, "Expected ~2000, got {}", dv);
    }

    #[test]
    fn test_return_to_earth_surface_now_reachable() {
        // Symmetric ascent/descent means orbits can now plot return paths to
        // Earth surface. (Previously the graph was launch-only.)
        let map = DeltaVMap::earth_moon();
        assert!(map.shortest_path("leo", "earth_surface", REF_MASS).is_some());
        assert!(map.shortest_path("geo", "earth_surface", REF_MASS).is_some());
        assert!(map.shortest_path("lunar_orbit", "earth_surface", REF_MASS).is_some());
    }

    #[test]
    fn test_transfer_transit_days() {
        let map = DeltaVMap::earth_moon();
        assert_eq!(map.transfer("earth_surface", "suborbital").unwrap().transit_days, 0);
        assert_eq!(map.transfer("earth_surface", "leo").unwrap().transit_days, 0);
        // A launch to SSO or GTO separates the same day it lifts off.
        assert_eq!(map.transfer("earth_surface", "sso").unwrap().transit_days, 0);
        assert_eq!(map.transfer("earth_surface", "gto").unwrap().transit_days, 0);
        assert_eq!(map.transfer("leo", "sso").unwrap().transit_days, 0);
        assert_eq!(map.transfer("leo", "meo").unwrap().transit_days, 0);
        // Circularising at GEO is an apogee-raising campaign, and the coast
        // between burns is what a GEO mission needs endurance for.
        assert_eq!(map.transfer("gto", "geo").unwrap().transit_days, 3);
        assert_eq!(map.transfer("lunar_orbit", "lunar_surface").unwrap().transit_days, 0);
        assert_eq!(map.transfer("lunar_surface", "lunar_orbit").unwrap().transit_days, 0);
        assert_eq!(map.transfer("leo", "gto").unwrap().transit_days, 1);
        assert_eq!(map.transfer("leo", "l1").unwrap().transit_days, 5);
        assert_eq!(map.transfer("l1", "lunar_orbit").unwrap().transit_days, 2);
        assert_eq!(map.transfer("leo", "lunar_orbit").unwrap().transit_days, 4);
    }

    #[test]
    fn test_reverse_transfer_transit_days() {
        let map = DeltaVMap::earth_moon();
        assert_eq!(map.transfer("sso", "leo").unwrap().transit_days, 0);
        assert_eq!(map.transfer("meo", "leo").unwrap().transit_days, 0);
        assert_eq!(map.transfer("gto", "leo").unwrap().transit_days, 1);
        assert_eq!(map.transfer("geo", "gto").unwrap().transit_days, 3);
        assert_eq!(map.transfer("sso", "earth_surface").unwrap().transit_days, 0);
        assert_eq!(map.transfer("gto", "earth_surface").unwrap().transit_days, 0);
        assert_eq!(map.transfer("lunar_orbit", "leo").unwrap().transit_days, 4);
        assert_eq!(map.transfer("lunar_orbit", "l1").unwrap().transit_days, 2);
        assert_eq!(map.transfer("l1", "leo").unwrap().transit_days, 5);
        assert_eq!(map.transfer("l2", "lunar_orbit").unwrap().transit_days, 2);
        assert_eq!(map.transfer("leo", "l2").unwrap().transit_days, 5);
    }

    #[test]
    fn test_mass_dependent_drag() {
        let map = DeltaVMap::earth_moon();
        let (_, dv_light) = map.shortest_path("earth_surface", "leo", 100_000.0).unwrap();
        let (_, dv_heavy) = map.shortest_path("earth_surface", "leo", 2_000_000.0).unwrap();
        // Lighter rocket has more drag loss
        assert!(dv_light > dv_heavy,
            "Light rocket ({}) should need more dv than heavy ({})", dv_light, dv_heavy);
        // Both should be in the ballpark of 7800 + some drag
        assert!(dv_light > 7800.0 && dv_light < 9000.0);
        assert!(dv_heavy > 7800.0 && dv_heavy < 9000.0);
    }

    // ==========================================
    // Low-thrust pathfinding tests
    // ==========================================

    #[test]
    fn test_low_thrust_cannot_reach_surface() {
        let map = DeltaVMap::earth_moon();
        // Low-thrust can't launch from surface
        assert!(map.shortest_path_constrained("earth_surface", "leo", REF_MASS, true).is_none());
        // Low-thrust can't land on lunar surface
        assert!(map.shortest_path_constrained("lunar_orbit", "lunar_surface", REF_MASS, true).is_none());
    }

    #[test]
    fn test_low_thrust_cannot_use_gto() {
        let map = DeltaVMap::earth_moon();
        // Low-thrust should go LEO→MEO→GEO, not through GTO
        let result = map.shortest_path_constrained("leo", "geo", REF_MASS, true);
        assert!(result.is_some());
        let (path, _dv) = result.unwrap();
        assert!(!path.contains(&"gto"), "Low-thrust should not use GTO, path: {:?}", path);
    }

    #[test]
    fn test_low_thrust_leo_to_geo_costs_more() {
        let map = DeltaVMap::earth_moon();
        let (_, dv_high) = map.shortest_path_constrained("leo", "geo", REF_MASS, false).unwrap();
        let (_, dv_low) = map.shortest_path_constrained("leo", "geo", REF_MASS, true).unwrap();
        assert!(dv_low > dv_high,
            "Low-thrust LEO→GEO ({}) should cost more than high-thrust ({})", dv_low, dv_high);
    }

    #[test]
    fn test_low_thrust_can_reach_eros_orbit() {
        let map = DeltaVMap::earth_moon();
        // Low-thrust must spiral up the Earth ladder to reach Eros.
        let result = map.shortest_path_constrained("leo", "eros_orbit", REF_MASS, true);
        assert!(result.is_some());
        let (path, _dv) = result.unwrap();
        // Path must climb LEO → MEO → GEO → escape rather than shortcut to GTO.
        assert!(!path.contains(&"gto"), "low-thrust path should not use GTO: {:?}", path);
        assert!(path.contains(&"earth_escape"), "should pass through earth_escape: {:?}", path);
    }

    #[test]
    fn test_low_thrust_cannot_reach_eros_surface() {
        // Eros gravity is ~5e-3 m/s² — too high for a typical ion drive to
        // land safely. The orbit→surface edge is high-thrust only.
        let map = DeltaVMap::earth_moon();
        assert!(map.shortest_path_constrained("leo", "eros_surface", REF_MASS, true).is_none());
    }

    #[test]
    fn test_low_thrust_can_reach_bennu_surface() {
        // Bennu gravity is ~6e-5 m/s² — well below ion-drive acceleration,
        // so the surface edge is flagged low_thrust_ok and a low-thrust path
        // exists end-to-end.
        let map = DeltaVMap::earth_moon();
        let result = map.shortest_path_constrained("leo", "bennu_surface", REF_MASS, true);
        assert!(result.is_some(), "low-thrust should reach bennu surface");
    }

    #[test]
    fn test_high_thrust_can_reach_eros_surface() {
        let map = DeltaVMap::earth_moon();
        let result = map.shortest_path_constrained("leo", "eros_surface", REF_MASS, false);
        assert!(result.is_some());
        let (path, _) = result.unwrap();
        // Path must traverse Earth escape and the Eros side branch.
        assert_eq!(path.first(), Some(&"leo"));
        assert_eq!(path.last(), Some(&"eros_surface"));
        assert!(path.contains(&"earth_escape"));
        assert!(path.contains(&"eros_transfer"));
    }
}
