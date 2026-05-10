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
    /// Ambient pressure at the surface in Pascals (e.g. 101_325 for Earth).
    pub ambient_pressure_pa: f64,
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
        "hygiea" => 3.14,
        "eros" => 1.46,
        "bennu" => 1.13,
        _ => 1.0,
    }
}

impl Location {
    /// Distance from the Sun in AU. Heliocentric "X_transfer" and
    /// "X_escape" nodes (parent_body = "sun") look up X's heliocentric
    /// distance so the burn at that node sees the right solar flux —
    /// e.g. `mars_transfer` reports 1.52 AU even though it's filed
    /// under the Sun. Everything else uses its parent body.
    pub fn sun_distance_au(&self) -> f64 {
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
    pub through_atmosphere: bool,
    pub animation: Option<TransferAnimation>,
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
    /// Total delta-v cost including aerodynamic drag losses if applicable.
    pub fn total_delta_v(&self, rocket_mass_kg: f64) -> f64 {
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
            Some(self.total_delta_v(rocket_mass_kg))
        }
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

// ─── Builder helpers for the inner-solar-system graph ────────────────
// These are private to the module and exist so the long graph-construction
// function below stays readable.

fn loc_orbit(
    id: &'static str, display: &'static str, short: &'static str, parent: &'static str,
) -> Location {
    Location {
        id, display_name: display, short_name: short,
        location_type: LocationType::Orbit, parent_body: parent,
    }
}

fn loc_lagrange(
    id: &'static str, display: &'static str, short: &'static str, parent: &'static str,
) -> Location {
    Location {
        id, display_name: display, short_name: short,
        location_type: LocationType::LagrangePoint, parent_body: parent,
    }
}

fn loc_surface(
    id: &'static str, display: &'static str, short: &'static str, parent: &'static str,
    gravity: f64, radius: f64, has_atm: bool, atm_density: f64, ambient: f64,
) -> Location {
    Location {
        id, display_name: display, short_name: short,
        location_type: LocationType::Surface(SurfaceProperties {
            gravity_m_s2: gravity, radius_m: radius,
            has_atmosphere: has_atm, atmosphere_density: atm_density,
            ambient_pressure_pa: ambient,
        }),
        parent_body: parent,
    }
}

/// Push a high-thrust-only symmetric edge pair (a ↔ b).
fn add_impulsive_pair(
    transfers: &mut Vec<Transfer>,
    a: &'static str, b: &'static str, dv: f64, days: u32,
) {
    let make = |from, to| Transfer {
        from, to, delta_v: dv,
        through_atmosphere: false, animation: None,
        can_aerobrake: false, transit_days: days,
        low_thrust_ok: false, low_thrust_delta_v: None,
    };
    transfers.push(make(a, b));
    transfers.push(make(b, a));
}

/// Push a low-thrust-friendly symmetric edge pair (a ↔ b).
/// `lt_dv = None` means low-thrust uses the same dv as high-thrust (small
/// burns where spiral inefficiency is negligible).
fn add_spiral_pair(
    transfers: &mut Vec<Transfer>,
    a: &'static str, b: &'static str, dv: f64, lt_dv: Option<f64>, days: u32,
) {
    let make = |from, to| Transfer {
        from, to, delta_v: dv,
        through_atmosphere: false, animation: None,
        can_aerobrake: false, transit_days: days,
        low_thrust_ok: true, low_thrust_delta_v: lt_dv,
    };
    transfers.push(make(a, b));
    transfers.push(make(b, a));
}

/// Push a surface ↔ orbit pair with the same nominal dv both ways.
/// Ascent edge sets `through_atmosphere` (drag is added on top of dv);
/// descent edge sets `can_aerobrake` (flag for future aerobrake savings).
/// `lt_dv = Some(...)` allows low-thrust to use this pair (e.g. Bennu).
fn add_ground_pair(
    transfers: &mut Vec<Transfer>,
    surface: &'static str, orbit: &'static str, dv: f64, days: u32,
    has_atm: bool, lt_dv: Option<f64>,
) {
    let lt_ok = lt_dv.is_some();
    transfers.push(Transfer {
        from: surface, to: orbit, delta_v: dv,
        through_atmosphere: has_atm,
        animation: Some(TransferAnimation::Launch),
        can_aerobrake: false, transit_days: days,
        low_thrust_ok: lt_ok, low_thrust_delta_v: lt_dv,
    });
    transfers.push(Transfer {
        from: orbit, to: surface, delta_v: dv,
        through_atmosphere: false,
        animation: Some(TransferAnimation::Landing),
        can_aerobrake: has_atm, transit_days: days,
        low_thrust_ok: lt_ok, low_thrust_delta_v: lt_dv,
    });
}

/// Add a body's full side-branch off the heliocentric ladder:
/// `transfer ↔ capture ↔ orbit ↔ surface`.
/// All edges symmetric. The capture and orbit links are spiral-friendly;
/// the surface link is created by `add_ground_pair`.
fn add_body_branch(
    transfers: &mut Vec<Transfer>,
    transfer_node: &'static str, capture: &'static str,
    orbit: &'static str, surface: &'static str,
    capture_dv: f64, capture_days: u32,
    capture_to_orbit_dv: f64,
    orbit_to_surface_dv: f64,
    surface_has_atm: bool,
    surface_low_thrust: Option<f64>,
) {
    // transfer ↔ capture: heliocentric injection burn (large; spiral penalty 1.5x)
    add_spiral_pair(
        transfers, transfer_node, capture, capture_dv,
        Some(capture_dv * 1.5), capture_days,
    );
    // capture ↔ orbit: orbital insertion (small; same dv both classes)
    add_spiral_pair(transfers, capture, orbit, capture_to_orbit_dv, None, 0);
    // orbit ↔ surface: launch/landing
    add_ground_pair(
        transfers, surface, orbit, orbit_to_surface_dv, 0,
        surface_has_atm, surface_low_thrust,
    );
}

impl DeltaVMap {
    /// Build the inner-solar-system delta-v graph (Mercury through the
    /// asteroid belt, plus Earth/Moon and a couple of NEAs).
    pub fn earth_moon() -> Self {
        let locations = vec![
            // ─── Earth system ───
            loc_surface("earth_surface", "Earth Surface", "EARTH", "earth",
                9.81, 6_371_000.0, true, 1.225, 101_325.0),
            loc_orbit("suborbital", "Suborbital", "SUB", "earth"),
            loc_orbit("leo", "Low Earth Orbit", "LEO", "earth"),
            loc_orbit("sso", "Sun-Synchronous Orbit", "SSO", "earth"),
            loc_orbit("meo", "Medium Earth Orbit", "MEO", "earth"),
            loc_orbit("gto", "Geostationary Transfer", "GTO", "earth"),
            loc_orbit("geo", "Geostationary Orbit", "GEO", "earth"),
            loc_orbit("earth_escape", "Earth Escape", "ESC", "sun"),
            loc_lagrange("l1", "Earth-Moon L1", "L1", "earth"),
            loc_lagrange("l2", "Earth-Moon L2", "L2", "earth"),
            loc_orbit("lunar_orbit", "Lunar Orbit", "LLO", "moon"),
            loc_surface("lunar_surface", "Lunar Surface", "MOON", "moon",
                1.62, 1_737_000.0, false, 0.0, 0.0),
            // ─── Mercury ───
            loc_orbit("mercury_transfer", "Mercury Transfer", "MTRF", "sun"),
            loc_orbit("mercury_capture", "Mercury Capture", "MCAP", "mercury"),
            loc_orbit("mercury_orbit_100km", "Mercury 100km Orbit", "MORB", "mercury"),
            loc_surface("mercury_surface", "Mercury Surface", "MERC", "mercury",
                3.7, 2_440_000.0, false, 0.0, 0.0),
            // ─── Venus (balloons at 1 bar instead of surface) ───
            loc_orbit("venus_transfer", "Venus Transfer", "VTRF", "sun"),
            loc_orbit("venus_capture", "Venus Capture", "VCAP", "venus"),
            loc_orbit("venus_orbit_400km", "Venus 400km Orbit", "VORB", "venus"),
            loc_surface("venus_balloons", "Venus 1bar Balloons", "VBAL", "venus",
                8.69, 6_101_800.0, true, 1.2, 100_000.0),
            // ─── Mars + moons ───
            loc_orbit("mars_transfer", "Mars Transfer", "MARTR", "sun"),
            loc_orbit("mars_capture", "Mars Capture", "MARC", "mars"),
            loc_orbit("mars_orbit_200km", "Mars 200km Orbit", "MARO", "mars"),
            loc_surface("mars_surface", "Mars Surface", "MARS", "mars",
                3.71, 3_389_500.0, true, 0.020, 600.0),
            loc_orbit("phobos_transfer", "Phobos Transfer", "PHTR", "mars"),
            loc_orbit("phobos_orbit", "Phobos Orbit", "PHOR", "phobos"),
            loc_surface("phobos_surface", "Phobos Surface", "PHOB", "phobos",
                0.0057, 11_000.0, false, 0.0, 0.0),
            loc_orbit("deimos_transfer", "Deimos Transfer", "DETR", "mars"),
            loc_orbit("deimos_orbit", "Deimos Orbit", "DEOR", "deimos"),
            loc_surface("deimos_surface", "Deimos Surface", "DEIM", "deimos",
                0.003, 6_200.0, false, 0.0, 0.0),
            // ─── Asteroid belt (Vesta, Ceres, Hygiea — Pallas skipped) ───
            loc_orbit("vesta_transfer", "Vesta Transfer", "VETR", "sun"),
            loc_orbit("vesta_capture", "Vesta Capture", "VECP", "vesta"),
            loc_orbit("vesta_orbit_20km", "Vesta 20km Orbit", "VEOR", "vesta"),
            loc_surface("vesta_surface", "Vesta Surface", "VEST", "vesta",
                0.25, 262_700.0, false, 0.0, 0.0),
            loc_orbit("ceres_transfer", "Ceres Transfer", "CETR", "sun"),
            loc_orbit("ceres_capture", "Ceres Capture", "CECP", "ceres"),
            loc_orbit("ceres_orbit_20km", "Ceres 20km Orbit", "CEOR", "ceres"),
            loc_surface("ceres_surface", "Ceres Surface", "CERE", "ceres",
                0.27, 473_000.0, false, 0.0, 0.0),
            loc_orbit("hygiea_transfer", "Hygiea Transfer", "HYTR", "sun"),
            loc_orbit("hygiea_capture", "Hygiea Capture", "HYCP", "hygiea"),
            loc_orbit("hygiea_orbit_20km", "Hygiea 20km Orbit", "HYOR", "hygiea"),
            loc_surface("hygiea_surface", "Hygiea Surface", "HYGI", "hygiea",
                0.13, 200_000.0, false, 0.0, 0.0),
            // ─── NEAs (Eros, Bennu) ───
            loc_orbit("eros_transfer", "Eros Transfer", "ERTR", "sun"),
            loc_orbit("eros_capture", "Eros Capture", "ERCP", "eros"),
            loc_orbit("eros_orbit", "Eros Orbit", "EROR", "eros"),
            loc_surface("eros_surface", "Eros Surface", "EROS", "eros",
                0.0059, 16_840.0, false, 0.0, 0.0),
            loc_orbit("bennu_transfer", "Bennu Transfer", "BNTR", "sun"),
            loc_orbit("bennu_capture", "Bennu Capture", "BNCP", "bennu"),
            loc_orbit("bennu_orbit", "Bennu Orbit", "BNOR", "bennu"),
            loc_surface("bennu_surface", "Bennu Surface", "BENN", "bennu",
                0.000060, 245.0, false, 0.0, 0.0),
        ];

        let mut transfers: Vec<Transfer> = Vec::new();

        // ─── Earth surface launches ───
        // Suborbital is genuinely one-way (you fall back, no thrust needed),
        // so it stays asymmetric.
        transfers.push(Transfer {
            from: "earth_surface", to: "suborbital", delta_v: 3500.0,
            through_atmosphere: true, animation: Some(TransferAnimation::Launch),
            can_aerobrake: false, transit_days: 0,
            low_thrust_ok: false, low_thrust_delta_v: None,
        });
        // Earth surface ↔ LEO: same nominal dv both ways, drag on ascent only.
        add_ground_pair(&mut transfers, "earth_surface", "leo", 7800.0, 0, true, None);

        // ─── Earth orbital climb (low-thrust climbs the ladder) ───
        add_spiral_pair(&mut transfers, "leo", "sso", 500.0, None, 0);
        add_spiral_pair(&mut transfers, "leo", "meo", 2100.0, Some(3500.0), 0);
        add_spiral_pair(&mut transfers, "meo", "geo", 2000.0, Some(2500.0), 0);
        add_spiral_pair(&mut transfers, "geo", "earth_escape", 700.0, Some(1500.0), 0);

        // ─── Earth high-thrust shortcuts (no low-thrust direct shortcuts to escape) ───
        add_impulsive_pair(&mut transfers, "leo", "gto", 2440.0, 1);
        add_impulsive_pair(&mut transfers, "gto", "geo", 1500.0, 0);
        add_impulsive_pair(&mut transfers, "leo", "lunar_orbit", 3850.0, 4);
        add_impulsive_pair(&mut transfers, "lunar_orbit", "earth_escape", 93.0, 4);

        // ─── Lagrange points and lunar surface ───
        add_spiral_pair(&mut transfers, "leo", "l1", 3150.0, None, 5);
        add_spiral_pair(&mut transfers, "l1", "lunar_orbit", 700.0, None, 2);
        add_spiral_pair(&mut transfers, "leo", "l2", 3200.0, None, 5);
        add_spiral_pair(&mut transfers, "l2", "lunar_orbit", 800.0, None, 2);
        add_ground_pair(&mut transfers, "lunar_surface", "lunar_orbit", 1700.0, 0, false, None);

        // ─── Heliocentric backbone (Hohmann ladder) ───
        add_spiral_pair(&mut transfers, "mercury_transfer", "venus_transfer",
            2085.0, Some(2085.0 * 1.5), 50);
        add_spiral_pair(&mut transfers, "venus_transfer", "earth_escape",
            280.0, Some(280.0 * 1.5), 100);
        add_spiral_pair(&mut transfers, "earth_escape", "mars_transfer",
            388.0, Some(388.0 * 1.5), 200);
        add_spiral_pair(&mut transfers, "mars_transfer", "vesta_transfer",
            923.0, Some(923.0 * 1.5), 100);
        add_spiral_pair(&mut transfers, "vesta_transfer", "ceres_transfer",
            379.0, Some(379.0 * 1.5), 100);
        add_spiral_pair(&mut transfers, "ceres_transfer", "hygiea_transfer",
            570.0, Some(570.0 * 1.5), 100);

        // ─── NEA branches off Earth escape (not on the planetary ladder) ───
        add_spiral_pair(&mut transfers, "earth_escape", "eros_transfer",
            600.0, Some(900.0), 200);
        add_spiral_pair(&mut transfers, "earth_escape", "bennu_transfer",
            400.0, Some(600.0), 150);

        // ─── Body branches: transfer → capture → orbit → surface ───
        // Mercury (no atmosphere)
        add_body_branch(&mut transfers, "mercury_transfer", "mercury_capture",
            "mercury_orbit_100km", "mercury_surface",
            3062.0, 30, 1220.0, 6310.0, false, None);
        // Venus (1bar balloons instead of surface; descent through atmosphere)
        add_body_branch(&mut transfers, "venus_transfer", "venus_capture",
            "venus_orbit_400km", "venus_balloons",
            359.0, 30, 2939.0, 1500.0, true, None);
        // Mars (atmosphere)
        add_body_branch(&mut transfers, "mars_transfer", "mars_capture",
            "mars_orbit_200km", "mars_surface",
            673.0, 30, 3578.0, 4100.0, true, None);
        // Vesta / Ceres / Hygiea (no atmosphere, large injection burns)
        add_body_branch(&mut transfers, "vesta_transfer", "vesta_capture",
            "vesta_orbit_20km", "vesta_surface",
            4096.0, 30, 102.0, 173.0, false, None);
        add_body_branch(&mut transfers, "ceres_transfer", "ceres_capture",
            "ceres_orbit_20km", "ceres_surface",
            4381.0, 30, 148.0, 280.0, false, None);
        add_body_branch(&mut transfers, "hygiea_transfer", "hygiea_capture",
            "hygiea_orbit_20km", "hygiea_surface",
            4915.0, 30, 63.0, 139.0, false, None);
        // Eros (NEA, tiny gravity but high-thrust only)
        add_body_branch(&mut transfers, "eros_transfer", "eros_capture",
            "eros_orbit", "eros_surface",
            30.0, 30, 5.0, 10.0, false, None);
        // Bennu (NEA, gravity so low ion drives can land)
        add_body_branch(&mut transfers, "bennu_transfer", "bennu_capture",
            "bennu_orbit", "bennu_surface",
            20.0, 30, 5.0, 5.0, false, Some(5.0));

        // ─── Mars moons (Phobos and Deimos branch off mars_capture) ───
        // Phobos: tiny gravity but ion-thrust margin too small per the
        // planning rule — keep landing high-thrust only.
        add_spiral_pair(&mut transfers, "mars_capture", "phobos_transfer", 535.0, None, 1);
        add_spiral_pair(&mut transfers, "phobos_transfer", "phobos_orbit", 3.0, None, 0);
        add_ground_pair(&mut transfers, "phobos_surface", "phobos_orbit",
            6.0, 0, false, None);
        add_spiral_pair(&mut transfers, "mars_capture", "deimos_transfer", 649.0, None, 1);
        add_spiral_pair(&mut transfers, "deimos_transfer", "deimos_orbit", 2.0, None, 0);
        add_ground_pair(&mut transfers, "deimos_surface", "deimos_orbit",
            4.0, 0, false, None);

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

    /// Find shortest path between two locations using Dijkstra's algorithm.
    /// `rocket_mass_kg` is used to compute atmospheric drag losses.
    /// Returns (path_of_location_ids, total_delta_v) or None if no path exists.
    pub fn shortest_path(&self, from: &str, to: &str, rocket_mass_kg: f64) -> Option<(Vec<&'static str>, f64)> {
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
                    let next_cost = cost + transfer.total_delta_v(rocket_mass_kg);
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

    /// Find shortest path with engine capability constraint.
    /// If `low_thrust` is true, only edges with `low_thrust_ok` are used,
    /// and `low_thrust_delta_v` is preferred when available.
    pub fn shortest_path_constrained(
        &self, from: &str, to: &str, rocket_mass_kg: f64, low_thrust: bool,
    ) -> Option<(Vec<&'static str>, f64)> {
        let from_idx = self.locations.iter().position(|l| l.id == from)?;
        let to_idx = self.locations.iter().position(|l| l.id == to)?;

        let n = self.locations.len();
        let mut dist = vec![f64::INFINITY; n];
        let mut prev = vec![None; n];
        let mut heap = BinaryHeap::new();

        dist[from_idx] = 0.0;
        heap.push(DijkstraState { cost: 0.0, node_index: from_idx });

        while let Some(DijkstraState { cost, node_index }) = heap.pop() {
            if node_index == to_idx { break; }
            if cost > dist[node_index] { continue; }

            let loc_id = self.locations[node_index].id;
            for transfer in self.transfers_from(loc_id) {
                if let Some(dv) = transfer.delta_v_for(low_thrust, rocket_mass_kg) {
                    if let Some(next_idx) = self.locations.iter().position(|l| l.id == transfer.to) {
                        let next_cost = cost + dv;
                        if next_cost < dist[next_idx] {
                            dist[next_idx] = next_cost;
                            prev[next_idx] = Some(node_index);
                            heap.push(DijkstraState { cost: next_cost, node_index: next_idx });
                        }
                    }
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

/// Velocity at which the rocket begins pitching from vertical (gravity turn initiation).
pub const KICK_OVER_VELOCITY: f64 = 50.0;

/// Simulate gravity turn ascent to estimate gravity losses.
///
/// Numerically integrates the gravity turn equations with a coarse 1-second timestep:
///   d(pitch)/dt = -g * cos(pitch) / velocity + velocity * cos(pitch) / R
///   gravity_loss accumulates g * sin(pitch) * dt each step
///
/// The second term is the Earth-curvature correction: at orbital velocity
/// (v = sqrt(g*R)) the two terms cancel and pitch rate goes to zero (orbit
/// achieved). Without this term vehicles pitch horizontal too early and
/// upper stages show unrealistically low gravity losses.
///
/// Parameters come from the stage group (thrust, mass flow, propellant) and
/// the launch location (surface gravity, body radius). The only free parameter
/// is KICK_OVER_VELOCITY (~50 m/s), the velocity at which the rocket begins
/// pitching from vertical.
///
/// For multi-stage rockets, each stage group is simulated sequentially:
/// the first group starts at velocity=0, pitch=90°. Subsequent groups
/// inherit the velocity and pitch from the end of the previous group's burn.
///
/// # Arguments
/// * `surface_gravity` - Surface gravity in m/s² (e.g. 9.81 for Earth)
/// * `body_radius` - Radius of the body in meters (e.g. 6_371_000.0 for Earth)
/// * `stage_params` - Per group: (thrust_n, mass_flow_kg_s, propellant_kg)
/// * `initial_mass_kg` - Total rocket mass including payload
///
/// # Returns
/// Gravity loss in m/s per stage group.
pub fn simulate_gravity_losses(
    surface_gravity: f64,
    body_radius: f64,
    stage_params: &[(f64, f64, f64)],
    initial_mass_kg: f64,
) -> Vec<f64> {
    let g = surface_gravity;
    let mut velocity = 0.0_f64;
    let mut pitch = std::f64::consts::FRAC_PI_2; // 90° = vertical
    let mut mass = initial_mass_kg;
    let mut results = Vec::with_capacity(stage_params.len());

    let mut kicked_over = false;

    for &(thrust, mass_flow, propellant) in stage_params {
        let mut gravity_loss = 0.0;
        let mut remaining_prop = propellant;

        // Skip stages with no propellant/mass flow (solar sails)
        if mass_flow <= 0.0 || propellant <= 0.0 {
            results.push(0.0);
            continue;
        }

        while remaining_prop > 1e-6 {
            let dt = (1.0_f64).min(remaining_prop / mass_flow);
            gravity_loss += g * pitch.sin() * dt;

            let net_accel = thrust / mass - g * pitch.sin();
            velocity += net_accel * dt;
            velocity = velocity.max(0.0); // can't go backwards

            if velocity > KICK_OVER_VELOCITY {
                // Initiate gravity turn with a small kick if we haven't already
                if !kicked_over {
                    kicked_over = true;
                    // Small initial pitch-over: ~1 degree
                    pitch -= 0.02;
                }
                let pitch_rate = g * pitch.cos() / velocity
                    - velocity * pitch.cos() / body_radius;
                pitch -= pitch_rate * dt;
                pitch = pitch.clamp(0.0, std::f64::consts::FRAC_PI_2);
            }

            let dm = mass_flow * dt;
            mass -= dm;
            remaining_prop -= dm;
        }

        results.push(gravity_loss);
        // Next group inherits velocity and pitch
    }

    results
}

/// Return the IDs of locations that are surfaces (where launches can originate).
pub fn surface_location_ids() -> &'static [&'static str] {
    &["earth_surface", "lunar_surface"]
}

/// Global delta-v map instance
pub static DELTA_V_MAP: LazyLock<DeltaVMap> = LazyLock::new(DeltaVMap::earth_moon);

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference mass for tests — produces exactly 300 m/s drag loss
    const REF_MASS: f64 = 500_000.0;

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
            through_atmosphere: false,
            animation: None, can_aerobrake: false, transit_days: 1, low_thrust_ok: true, low_thrust_delta_v: None,
        };
        assert_eq!(t.total_delta_v(REF_MASS), 2440.0);
    }

    #[test]
    fn test_transfer_through_atmosphere() {
        let t = Transfer {
            from: "earth_surface", to: "leo", delta_v: 7800.0,
            through_atmosphere: true,
            animation: None, can_aerobrake: false, transit_days: 0, low_thrust_ok: true, low_thrust_delta_v: None,
        };
        let total = t.total_delta_v(REF_MASS);
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
        let total = t.total_delta_v(REF_MASS);
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
        assert_eq!(path, vec!["earth_surface", "leo", "gto", "geo"]);
        // 8100 + 2440 + 1500 = 12040
        assert!((dv - 12040.0).abs() < 1.0);
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
        assert_eq!(path, vec!["sso", "leo"]);
        assert_eq!(dv, 500.0);
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
        assert_eq!(map.transfer("leo", "sso").unwrap().transit_days, 0);
        assert_eq!(map.transfer("leo", "meo").unwrap().transit_days, 0);
        assert_eq!(map.transfer("gto", "geo").unwrap().transit_days, 0);
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
        assert_eq!(map.transfer("geo", "gto").unwrap().transit_days, 0);
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

    // ==========================================
    // Gravity loss simulation tests
    // ==========================================

    const EARTH_RADIUS: f64 = 6_371_000.0;
    const MOON_RADIUS: f64 = 1_737_000.0;

    #[test]
    fn test_gravity_loss_single_stage_positive() {
        // A single stage launching from Earth: should have significant gravity loss
        let thrust = 2_000_000.0; // 2 MN
        let isp = 300.0;
        let ve = isp * 9.80665;
        let mass_flow = thrust / ve;
        let propellant = 100_000.0;
        let dry_mass = 10_000.0;
        let total_mass = dry_mass + propellant;

        let losses = simulate_gravity_losses(9.81, EARTH_RADIUS, &[(thrust, mass_flow, propellant)], total_mass);
        assert_eq!(losses.len(), 1);
        assert!(losses[0] > 500.0, "Earth launch should have >500 m/s gravity loss, got {}", losses[0]);
        assert!(losses[0] < 3000.0, "Gravity loss should be <3000 m/s, got {}", losses[0]);
    }

    #[test]
    fn test_gravity_loss_higher_twr_means_less_loss() {
        // More engines = higher TWR = rocket gets through vertical phase faster = less gravity loss
        let isp = 300.0;
        let ve = isp * 9.80665;
        let single_thrust = 500_000.0;
        let mass_flow_per_engine = single_thrust / ve;
        let propellant = 50_000.0;
        let dry_mass = 5_000.0;
        let total_mass = dry_mass + propellant;

        // 1 engine
        let loss_1 = simulate_gravity_losses(
            9.81, EARTH_RADIUS,
            &[(single_thrust, mass_flow_per_engine, propellant)],
            total_mass,
        )[0];

        // 3 engines (3x thrust, 3x flow, same propellant = 1/3 burn time)
        let loss_3 = simulate_gravity_losses(
            9.81, EARTH_RADIUS,
            &[(single_thrust * 3.0, mass_flow_per_engine * 3.0, propellant)],
            total_mass,
        )[0];

        assert!(loss_3 < loss_1,
            "3 engines (loss={:.0}) should have less gravity loss than 1 engine (loss={:.0})",
            loss_3, loss_1);
    }

    #[test]
    fn test_gravity_loss_lunar_less_than_earth() {
        let thrust = 1_000_000.0;
        let isp = 300.0;
        let ve = isp * 9.80665;
        let mass_flow = thrust / ve;
        let propellant = 50_000.0;
        let total_mass = 60_000.0;

        let loss_earth = simulate_gravity_losses(
            9.81, EARTH_RADIUS, &[(thrust, mass_flow, propellant)], total_mass,
        )[0];
        let loss_moon = simulate_gravity_losses(
            1.62, MOON_RADIUS, &[(thrust, mass_flow, propellant)], total_mass,
        )[0];

        assert!(loss_moon < loss_earth,
            "Moon (loss={:.0}) should have less gravity loss than Earth (loss={:.0})",
            loss_moon, loss_earth);
    }

    #[test]
    fn test_gravity_loss_upper_stages_less_than_first() {
        // Falcon 9-like: first stage has high TWR and long burn.
        // With curvature correction, S1 won't pitch fully horizontal at suborbital
        // speeds, so S2 still shows meaningful gravity loss — but less than S1.
        // S1: 9 engines, ~7MN, burns ~160s
        let thrust_s1 = 7_000_000.0;
        let isp = 300.0;
        let ve = isp * 9.80665;
        let mass_flow_s1 = thrust_s1 / ve;
        let prop_s1 = mass_flow_s1 * 160.0; // 160 second burn

        // S2: 1 engine at higher Isp
        let thrust_s2 = 800_000.0;
        let isp_s2 = 340.0;
        let ve_s2 = isp_s2 * 9.80665;
        let mass_flow_s2 = thrust_s2 / ve_s2;
        let prop_s2 = mass_flow_s2 * 60.0; // 60 second burn

        // Total mass: S1 dry (5000) + S1 prop + S2 dry (1000) + S2 prop + payload (5000)
        let total_mass = 5_000.0 + prop_s1 + 1_000.0 + prop_s2 + 5_000.0;

        let losses = simulate_gravity_losses(
            9.81, EARTH_RADIUS,
            &[
                (thrust_s1, mass_flow_s1, prop_s1),
                (thrust_s2, mass_flow_s2, prop_s2),
            ],
            total_mass,
        );

        assert_eq!(losses.len(), 2);
        assert!(losses[0] > losses[1],
            "First stage loss ({:.0}) should exceed upper stage loss ({:.0})",
            losses[0], losses[1]);
    }

    #[test]
    fn test_gravity_loss_ssto_moderate() {
        // SSTO: long burn but most is horizontal after pitch-over
        let thrust = 3_000_000.0;
        let isp = 350.0;
        let ve = isp * 9.80665;
        let mass_flow = thrust / ve;
        let propellant = 200_000.0;
        let total_mass = 220_000.0;

        let losses = simulate_gravity_losses(
            9.81, EARTH_RADIUS, &[(thrust, mass_flow, propellant)], total_mass,
        );
        assert_eq!(losses.len(), 1);
        // Should be moderate — not as bad as a weak first stage, but still significant
        assert!(losses[0] > 300.0 && losses[0] < 2500.0,
            "SSTO gravity loss should be moderate, got {:.0}", losses[0]);
    }

    #[test]
    fn test_surface_location_ids() {
        let ids = surface_location_ids();
        assert!(ids.contains(&"earth_surface"));
        assert!(ids.contains(&"lunar_surface"));
        assert!(!ids.contains(&"leo"));
    }
}
