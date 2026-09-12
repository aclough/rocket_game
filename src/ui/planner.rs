//! The delta-v planner's state and the reachability query behind it.


/// An action in the delta-v planner.
#[derive(Debug, Clone)]
pub enum PlanAction {
    Leg { from: crate::location::LocationId, to: crate::location::LocationId, to_display: String, dv_cost: f64 },
    DropPayload { mass_dropped: f64 },
}

/// Source for the delta-v planner.
#[derive(Debug, Clone)]
pub enum PlannerSource {
    Design { project_index: usize },
    Spacecraft { spacecraft_index: usize },
}

/// Which field is active in the planner setup.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlannerSetupField {
    Design,
    Payload,
    Location,
}

/// State for the planner setup modal.
#[derive(Debug, Clone)]
pub struct PlannerSetupState {
    /// Indices of rocket projects that are in Testing status.
    pub eligible_projects: Vec<usize>,
    pub selected_project: usize,
    /// All locations from the delta-v map.
    pub locations: Vec<(&'static str, &'static str)>, // (id, display_name)
    pub selected_location: usize,
    pub payload_buffer: String,
    pub active_field: PlannerSetupField,
}

/// State for the delta-v planner modal.
#[derive(Debug, Clone)]
pub struct DvPlannerState {
    pub source: PlannerSource,
    pub rocket: crate::rocket::Rocket,
    pub design: crate::rocket::RocketDesign,
    pub current_location: crate::location::LocationId,
    pub actions: Vec<PlanAction>,
    /// Snapshots before each action (for undo): rocket state, payload at
    /// that point, and where the vehicle was.
    pub snapshots: Vec<(crate::rocket::Rocket, f64, crate::location::LocationId)>,
    /// Reachable destinations from current location.
    pub destinations: Vec<(String, String, f64)>, // (id, display, dv_cost)
    pub selected: usize,
    pub payload_kg: f64,
}

pub(super) fn reachable_destinations_multistage(
    from: &str, remaining_dv: f64, rocket_mass: f64,
    rocket: Option<&crate::rocket::Rocket>,
    design: Option<&crate::rocket::RocketDesign>,
) -> Vec<(String, String, f64)> {
    let map = &crate::location::DELTA_V_MAP;
    let mut dests = Vec::new();

    for loc in map.locations() {
        if loc.id == from {
            continue;
        }

        let path = if let (Some(rocket), Some(design)) = (rocket, design) {
            map.shortest_path_for_rocket_state(from, loc.id, design, rocket)
        } else {
            // No rocket state — fall back to the abstract Dijkstra so the
            // UI can still surface destinations for empty/imaginary rockets.
            map.shortest_path(from, loc.id, rocket_mass)
        };

        if let Some((_, dv)) = path {
            if dv <= remaining_dv {
                dests.push((loc.id.to_string(), loc.display_name.to_string(), dv));
            }
        }
    }
    dests.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());
    dests
}
