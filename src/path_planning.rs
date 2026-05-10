//! Stage-aware shortest-path planning through the delta-v graph.
//!
//! Uses A* search with a Dijkstra-precomputed admissible heuristic. Search
//! state is `(location, active_stage_index, dv_remaining_in_active_stage)`.
//! The third field is continuous; the search keeps a Pareto frontier per
//! `(location, active_stage_index)` (more dv-remaining at lower g-score
//! dominates).
//!
//! Edge transition rules:
//! - High-thrust attempt: every spanning stage must be high-thrust; cost is
//!   the edge's `delta_v` (plus atmospheric drag if applicable).
//! - Low-thrust attempt: only if the edge has `low_thrust_ok`; cost is
//!   `low_thrust_delta_v` (or the high-thrust dv if not specified). Any
//!   stage class may participate — high-thrust stages can fire during a
//!   low-thrust burn (just at the higher spiral cost).
//! - When an edge can't be covered by the active stage alone, the burn
//!   spills into the next stage(s), which must satisfy the class rule above.
//! - Both attempts can succeed on the same edge with different end-states;
//!   A* explores them in parallel.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::location::{aero_drag_loss, DeltaVMap, Transfer};
use crate::rocket::{Rocket, RocketDesign};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThrustClass {
    HighThrust,
    LowThrust,
}

fn group_thrust_class(design: &RocketDesign, gi: usize) -> ThrustClass {
    let group = match design.stage_groups.get(gi) {
        Some(g) => g,
        None => return ThrustClass::HighThrust,
    };
    if group.iter().any(|s| s.engine.is_low_thrust()) {
        ThrustClass::LowThrust
    } else {
        ThrustClass::HighThrust
    }
}

/// Mass above stage group `gi`: wet mass of all upper groups + payload.
fn payload_above_group(design: &RocketDesign, gi: usize, payload_mass_kg: f64) -> f64 {
    if gi + 1 >= design.stage_groups.len() {
        return payload_mass_kg;
    }
    design.stage_groups[gi + 1..].iter()
        .flat_map(|g| g.iter())
        .map(|s| s.wet_mass_kg())
        .sum::<f64>()
        + payload_mass_kg
}

/// Full delta-v for stage group `gi` assuming upper groups are full and a
/// payload of `payload_mass_kg` sits at the top.
fn full_group_dv(design: &RocketDesign, gi: usize, payload_mass_kg: f64) -> f64 {
    design.group_delta_v(gi, payload_above_group(design, gi, payload_mass_kg))
}

/// Edge dv cost for a given thrust class. None if the class can't use the
/// edge (low-thrust attempt against a non-low-thrust-ok edge).
fn edge_cost_for_class(
    transfer: &Transfer,
    rocket_mass_kg: f64,
    class: ThrustClass,
) -> Option<f64> {
    let base = match class {
        ThrustClass::HighThrust => transfer.delta_v,
        ThrustClass::LowThrust => {
            if !transfer.low_thrust_ok {
                return None;
            }
            transfer.low_thrust_delta_v.unwrap_or(transfer.delta_v)
        }
    };
    let drag = if transfer.through_atmosphere {
        aero_drag_loss(rocket_mass_kg)
    } else {
        0.0
    };
    Some(base + drag)
}

#[derive(Debug, Clone)]
struct EdgeOutcome {
    cost: f64,
    new_active_stage: usize,
    new_dv_in_active: f64,
}

/// Try to traverse `transfer` in `class` starting from
/// `(active_stage, dv_left_in_active)`. Returns Some(outcome) if feasible,
/// None otherwise.
fn try_class(
    transfer: &Transfer,
    design: &RocketDesign,
    payload_mass_kg: f64,
    rocket_mass_kg: f64,
    active_stage: usize,
    dv_left_in_active: f64,
    class: ThrustClass,
) -> Option<EdgeOutcome> {
    let cost = edge_cost_for_class(transfer, rocket_mass_kg, class)?;

    // High-thrust attempt requires the active stage to be high-thrust.
    if class == ThrustClass::HighThrust
        && group_thrust_class(design, active_stage) == ThrustClass::LowThrust
    {
        return None;
    }

    // Drain active stage first.
    if dv_left_in_active >= cost {
        return Some(EdgeOutcome {
            cost,
            new_active_stage: active_stage,
            new_dv_in_active: dv_left_in_active - cost,
        });
    }
    let mut remaining = cost - dv_left_in_active;
    let mut new_active = active_stage + 1;

    while new_active < design.stage_groups.len() {
        if class == ThrustClass::HighThrust
            && group_thrust_class(design, new_active) == ThrustClass::LowThrust
        {
            return None;
        }
        let stage_dv = full_group_dv(design, new_active, payload_mass_kg);
        if stage_dv >= remaining {
            return Some(EdgeOutcome {
                cost,
                new_active_stage: new_active,
                new_dv_in_active: stage_dv - remaining,
            });
        }
        remaining -= stage_dv;
        new_active += 1;
    }
    None
}

// ─── Heuristic precomputation: reverse Dijkstra from goal ─────────────

#[derive(Debug)]
struct DijkState {
    cost: f64,
    node: usize,
}
impl PartialEq for DijkState {
    fn eq(&self, o: &Self) -> bool { self.cost == o.cost && self.node == o.node }
}
impl Eq for DijkState {}
impl PartialOrd for DijkState {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> { Some(self.cmp(o)) }
}
impl Ord for DijkState {
    fn cmp(&self, o: &Self) -> Ordering {
        o.cost.partial_cmp(&self.cost).unwrap_or(Ordering::Equal)
    }
}

/// Lower-bound dv from each node to `goal_idx`. Uses a "best-case" graph
/// where each transfer's cost is `min(delta_v, low_thrust_delta_v)` with
/// atmospheric drag stripped (drag only adds cost; atmospheric edges are
/// surface-leaf edges anyway).
fn compute_heuristic(map: &DeltaVMap, goal_idx: usize) -> Vec<f64> {
    let n = map.location_count();
    let mut h = vec![f64::INFINITY; n];
    h[goal_idx] = 0.0;

    // Reverse adjacency: for each node `to`, list incoming `(from, cheapest_dv)`.
    let mut incoming: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for to_idx in 0..n {
        let to_id = map.location_at(to_idx).unwrap().id;
        for from_idx in 0..n {
            let from_id = map.location_at(from_idx).unwrap().id;
            if let Some(t) = map.transfer(from_id, to_id) {
                let cheap = t.low_thrust_delta_v
                    .map(|lt| lt.min(t.delta_v))
                    .unwrap_or(t.delta_v);
                incoming[to_idx].push((from_idx, cheap));
            }
        }
    }

    let mut heap = BinaryHeap::new();
    heap.push(DijkState { cost: 0.0, node: goal_idx });
    while let Some(DijkState { cost, node }) = heap.pop() {
        if cost > h[node] { continue; }
        for &(from_idx, edge) in &incoming[node] {
            let next = cost + edge;
            if next < h[from_idx] {
                h[from_idx] = next;
                heap.push(DijkState { cost: next, node: from_idx });
            }
        }
    }
    h
}

// ─── A* search ───────────────────────────────────────────────────────

#[derive(Debug)]
struct AStarState {
    f_score: f64,
    g_score: f64,
    loc_idx: usize,
    active_stage: usize,
    dv_left_in_active: f64,
    /// Index into the history table (set when this state's parent is popped
    /// and finalized). None for the initial state.
    parent: Option<usize>,
}
impl PartialEq for AStarState {
    fn eq(&self, o: &Self) -> bool { self.f_score == o.f_score }
}
impl Eq for AStarState {}
impl PartialOrd for AStarState {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> { Some(self.cmp(o)) }
}
impl Ord for AStarState {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f_score.partial_cmp(&self.f_score).unwrap_or(Ordering::Equal)
    }
}

#[derive(Debug, Clone)]
struct HistoryEntry {
    loc_idx: usize,
    parent: Option<usize>,
}

impl DeltaVMap {
    /// Stage-aware shortest-path planner.
    ///
    /// Walks the delta-v graph using A* with a Dijkstra-precomputed
    /// admissible heuristic. Computes the minimum-dv route for the given
    /// `design` carrying `payload_mass_kg` of payload. Returns
    /// `(path_of_location_ids, total_dv)`, or `None` if unreachable with the
    /// rocket's stage stack.
    ///
    /// Atmospheric drag is computed against the full rocket+payload mass.
    pub fn shortest_path_for_rocket(
        &self,
        from: &str,
        to: &str,
        design: &RocketDesign,
        payload_mass_kg: f64,
    ) -> Option<(Vec<&'static str>, f64)> {
        if design.stage_groups.is_empty() {
            return None;
        }
        let initial_dv = full_group_dv(design, 0, payload_mass_kg);
        self.astar_search(from, to, design, payload_mass_kg, 0, initial_dv)
    }

    /// Stage-aware shortest-path planner starting from a partial rocket
    /// state (e.g. a spacecraft mid-mission with some stages already
    /// jettisoned and propellant burned). Initial active stage and remaining
    /// dv are derived from `rocket.stage_states`.
    pub fn shortest_path_for_rocket_state(
        &self,
        from: &str,
        to: &str,
        design: &RocketDesign,
        rocket: &Rocket,
    ) -> Option<(Vec<&'static str>, f64)> {
        if design.stage_groups.is_empty() {
            return None;
        }
        // Find the lowest still-attached stage with propellant remaining.
        let n = design.stage_groups.len();
        let active_stage = (0..n).find(|&gi| {
            rocket.stage_states.get(gi)
                .map_or(false, |g| g.iter().any(|s| s.attached && s.propellant_remaining_kg > 0.0))
        })?;
        let initial_dv = rocket.group_remaining_delta_v(design, active_stage);
        self.astar_search(
            from, to, design, rocket.payload_mass_kg, active_stage, initial_dv,
        )
    }

    fn astar_search(
        &self,
        from: &str,
        to: &str,
        design: &RocketDesign,
        payload_mass_kg: f64,
        initial_active_stage: usize,
        initial_dv_left: f64,
    ) -> Option<(Vec<&'static str>, f64)> {
        let from_idx = self.locations().iter().position(|l| l.id == from)?;
        let to_idx = self.locations().iter().position(|l| l.id == to)?;

        let h = compute_heuristic(self, to_idx);
        if h[from_idx].is_infinite() {
            return None;
        }

        let rocket_mass_kg = design.total_mass_kg() + payload_mass_kg;

        let mut heap: BinaryHeap<AStarState> = BinaryHeap::new();
        // Pareto frontier per (loc_idx, active_stage): list of (g, dv_left).
        let mut frontiers: HashMap<(usize, usize), Vec<(f64, f64)>> = HashMap::new();
        let mut history: Vec<HistoryEntry> = Vec::new();

        heap.push(AStarState {
            f_score: h[from_idx],
            g_score: 0.0,
            loc_idx: from_idx,
            active_stage: initial_active_stage,
            dv_left_in_active: initial_dv_left,
            parent: None,
        });
        frontiers.insert((from_idx, initial_active_stage), vec![(0.0, initial_dv_left)]);

        while let Some(state) = heap.pop() {
            // Skip if this exact (g, dv_left) has been evicted from the
            // frontier (something better dominated it after we pushed).
            let still_on_frontier = frontiers
                .get(&(state.loc_idx, state.active_stage))
                .map_or(false, |f| f.iter().any(|&(g, dv)| {
                    g == state.g_score && dv == state.dv_left_in_active
                }));
            if !still_on_frontier {
                continue;
            }

            // Finalize this state in the history table.
            let my_idx = history.len();
            history.push(HistoryEntry {
                loc_idx: state.loc_idx,
                parent: state.parent,
            });

            if state.loc_idx == to_idx {
                let mut path = Vec::new();
                let mut cur = Some(my_idx);
                while let Some(i) = cur {
                    path.push(self.location_at(history[i].loc_idx).unwrap().id);
                    cur = history[i].parent;
                }
                path.reverse();
                return Some((path, state.g_score));
            }

            let loc_id = self.location_at(state.loc_idx).unwrap().id;
            for transfer in self.transfers_from(loc_id) {
                let next_idx = match self.locations().iter().position(|l| l.id == transfer.to) {
                    Some(i) => i,
                    None => continue,
                };

                for class in [ThrustClass::HighThrust, ThrustClass::LowThrust] {
                    let outcome = match try_class(
                        transfer,
                        design,
                        payload_mass_kg,
                        rocket_mass_kg,
                        state.active_stage,
                        state.dv_left_in_active,
                        class,
                    ) {
                        Some(o) => o,
                        None => continue,
                    };

                    let g = state.g_score + outcome.cost;
                    let f = g + h[next_idx];
                    let key = (next_idx, outcome.new_active_stage);
                    let frontier = frontiers.entry(key).or_default();

                    let dv = outcome.new_dv_in_active;
                    let dominated = frontier.iter().any(|&(ge, dve)| {
                        ge <= g && dve >= dv && (ge < g || dve > dv)
                    });
                    if dominated {
                        continue;
                    }
                    frontier.retain(|&(ge, dve)| {
                        !(g <= ge && dv >= dve && (g < ge || dv > dve))
                    });
                    frontier.push((g, dv));

                    heap.push(AStarState {
                        f_score: f,
                        g_score: g,
                        loc_idx: next_idx,
                        active_stage: outcome.new_active_stage,
                        dv_left_in_active: dv,
                        parent: Some(my_idx),
                    });
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
    use crate::location::DELTA_V_MAP;
    use crate::propellant::Propellant;
    use crate::rocket::{RocketDesign, RocketDesignId};
    use crate::stage::{Stage, StageId};

    fn kerolox_engine(id: u64, thrust: f64, mass: f64, isp: f64) -> EngineDesign {
        EngineDesign {
            id: EngineId(id), name: format!("KE-{}", id),
            cycle: EngineCycle::GasGenerator,
            thrust_n: thrust, mass_kg: mass, isp_s: isp,
            exit_pressure_pa: 70_000.0, needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::LOX, mass_fraction: 0.725 },
                PropellantFraction { propellant: Propellant::RP1, mass_fraction: 0.275 },
            ],
            power_draw_w: 0.0,
        }
    }

    fn ion_engine(id: u64, thrust: f64, mass: f64, isp: f64) -> EngineDesign {
        EngineDesign {
            id: EngineId(id), name: format!("IE-{}", id),
            cycle: EngineCycle::ElectricPropulsion,
            thrust_n: thrust, mass_kg: mass, isp_s: isp,
            exit_pressure_pa: 0.0, needs_atmosphere: false,
            propellant_mix: vec![
                PropellantFraction { propellant: Propellant::Xenon, mass_fraction: 1.0 },
            ],
            power_draw_w: 0.0,
        }
    }

    fn stage(id: u64, name: &str, engine: EngineDesign, count: u32, prop: f64, dry: f64) -> Stage {
        Stage {
            id: StageId(id), name: name.into(),
            engine, engine_count: count,
            propellant_mass_kg: prop, structural_mass_kg: dry,
            fairing: None,
            power_sources: Vec::new(),
        }
    }

    /// 2-stage chemical: big booster + smaller upper.
    fn two_stage_chemical() -> RocketDesign {
        let s1 = stage(1, "S1", kerolox_engine(1, 7_000_000.0, 1500.0, 280.0), 1, 350_000.0, 25_000.0);
        let s2 = stage(2, "S2", kerolox_engine(2, 1_000_000.0, 800.0, 340.0), 1, 90_000.0, 5_000.0);
        RocketDesign {
            id: RocketDesignId(1), name: "TwoChem".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        }
    }

    /// 2-stage hybrid: chemical booster + ion upper. Sized so S1 alone can
    /// reach LEO and the ion S2 carries enough xenon to spiral to NEA.
    fn chemical_then_ion() -> RocketDesign {
        let s1 = stage(1, "S1", kerolox_engine(1, 35_000_000.0, 5_000.0, 280.0), 1, 2_000_000.0, 50_000.0);
        let s2 = stage(2, "S2-Ion", ion_engine(2, 500.0, 200.0, 3500.0), 1, 30_000.0, 5_000.0);
        RocketDesign {
            id: RocketDesignId(2), name: "ChemIon".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        }
    }

    #[test]
    fn pure_chemical_to_leo_matches_simple_dijkstra() {
        // For a single high-thrust rocket, the new planner and the existing
        // straight Dijkstra should pick the same Earth → LEO route.
        let design = two_stage_chemical();
        let payload = 5_000.0;
        let new_path = DELTA_V_MAP.shortest_path_for_rocket(
            "earth_surface", "leo", &design, payload,
        );
        let old_path = DELTA_V_MAP.shortest_path(
            "earth_surface", "leo", design.total_mass_kg() + payload,
        );
        assert!(new_path.is_some(), "should find a path");
        let (np, ndv) = new_path.unwrap();
        let (op, odv) = old_path.unwrap();
        assert_eq!(np, op, "path nodes should match");
        // Cost should match within rounding (both compute the same drag).
        assert!((ndv - odv).abs() < 1.0,
            "new={} old={}", ndv, odv);
    }

    #[test]
    fn unreachable_goal_returns_none() {
        // Tiny 2-stage chemical rocket: insufficient dv to reach Eros even
        // optimistically. The path Earth → ... → Eros surface needs ~10+ km/s.
        let s1 = stage(1, "S1", kerolox_engine(1, 100_000.0, 200.0, 280.0), 1, 1_000.0, 200.0);
        let s2 = stage(2, "S2", kerolox_engine(2, 50_000.0, 100.0, 340.0), 1, 500.0, 100.0);
        let design = RocketDesign {
            id: RocketDesignId(99), name: "Tiny".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };
        let result = DELTA_V_MAP.shortest_path_for_rocket(
            "earth_surface", "eros_surface", &design, 100.0,
        );
        assert!(result.is_none(), "tiny rocket can't reach Eros surface");
    }

    #[test]
    fn chemical_then_ion_uses_ion_for_long_transfer() {
        let design = chemical_then_ion();
        // Eros orbit is reachable from Earth surface for a chem booster +
        // ion upper: chem lifts to LEO, ion spirals through MEO/GEO/escape
        // out to Eros.
        let result = DELTA_V_MAP.shortest_path_for_rocket(
            "earth_surface", "eros_orbit", &design, 200.0,
        );
        assert!(result.is_some(), "chem+ion stack should reach Eros orbit");
        let (path, _dv) = result.unwrap();
        assert_eq!(path.first(), Some(&"earth_surface"));
        assert_eq!(path.last(), Some(&"eros_orbit"));
        // Must traverse LEO (chem can't get further alone) and earth_escape
        // (ion has to spiral up the ladder to leave Earth's neighborhood).
        assert!(path.contains(&"leo"), "path={:?}", path);
        assert!(path.contains(&"earth_escape"), "path={:?}", path);
    }

    #[test]
    fn heuristic_admissibility() {
        // For every node, the heuristic to a fixed goal must be ≤ the true
        // (best-case-graph) Dijkstra distance from that node to goal. Since
        // the heuristic IS a Dijkstra on the best-case graph, it's exactly
        // the true distance there — but we still want to verify the
        // ≤ relation against `shortest_path` (which uses real edge dvs and
        // includes drag), ensuring admissibility against the planner's
        // actual cost function.
        let goal_idx = DELTA_V_MAP.locations().iter()
            .position(|l| l.id == "lunar_surface").unwrap();
        let h = compute_heuristic(&DELTA_V_MAP, goal_idx);
        for (i, loc) in DELTA_V_MAP.locations().iter().enumerate() {
            if let Some((_, true_dv)) = DELTA_V_MAP.shortest_path(
                loc.id, "lunar_surface", 500_000.0,
            ) {
                assert!(h[i] <= true_dv + 1.0,
                    "h[{}]={} > true_dv={}", loc.id, h[i], true_dv);
            }
        }
    }

    #[test]
    fn high_thrust_edge_spans_two_compatible_stages() {
        // Two high-thrust stages: S1 too small to reach LEO alone, but
        // S1 + S2 together cover the ascent. The high-thrust attempt should
        // succeed by spilling from S1 into S2; the next leg should see S2
        // partially drained.
        let s1 = stage(1, "S1", kerolox_engine(1, 7_000_000.0, 1_500.0, 280.0), 1, 200_000.0, 15_000.0);
        let s2 = stage(2, "S2", kerolox_engine(2, 1_500_000.0, 800.0, 340.0), 1, 600_000.0, 30_000.0);
        let design = RocketDesign {
            id: RocketDesignId(10), name: "SmallS1+BigS2".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };

        // Sanity: stage 1 alone shouldn't reach LEO.
        let s1_dv = full_group_dv(&design, 0, 1_000.0);
        let drag = aero_drag_loss(design.total_mass_kg() + 1_000.0);
        assert!(s1_dv < 7_800.0 + drag,
            "test setup wrong: S1 alone has {} dv > 8000 m/s ascent need", s1_dv);

        let result = DELTA_V_MAP.shortest_path_for_rocket(
            "earth_surface", "leo", &design, 1_000.0,
        );
        assert!(result.is_some(),
            "S1+S2 high-thrust spanning should reach LEO; got {:?}", result);
        let (path, _dv) = result.unwrap();
        assert_eq!(path, vec!["earth_surface", "leo"]);
    }

    #[test]
    fn span_into_low_thrust_blocks_high_thrust_only_edges() {
        // S1 high-thrust under-sized for Earth → LEO. S2 is ion (low-thrust).
        // The Earth → LEO transfer has low_thrust_ok = false.
        // - High-thrust attempt: S1 insufficient → would need to spill into S2,
        //   but S2 is low-thrust → reject.
        // - Low-thrust attempt: edge rejects low-thrust → reject.
        // No alternate ascent route exists → planner returns None.
        let s1 = stage(1, "S1-tiny", kerolox_engine(1, 1_000_000.0, 500.0, 280.0), 1, 50_000.0, 5_000.0);
        let s2 = stage(2, "S2-Ion", ion_engine(2, 500.0, 200.0, 3500.0), 1, 50_000.0, 5_000.0);
        let design = RocketDesign {
            id: RocketDesignId(11), name: "TinyChem+Ion".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };
        let result = DELTA_V_MAP.shortest_path_for_rocket(
            "earth_surface", "leo", &design, 100.0,
        );
        assert!(result.is_none(),
            "high-thrust ascent edge cannot spill into low-thrust ion S2; got {:?}",
            result);
    }

    #[test]
    fn high_thrust_path_uses_only_high_thrust_stages() {
        // A pure-chemical 2-stage rocket should never report a low-thrust
        // class for its edges (verified indirectly: the dv it reports must
        // equal the high-thrust dv on each edge).
        let design = two_stage_chemical();
        let payload = 5_000.0;
        let (path, dv) = DELTA_V_MAP.shortest_path_for_rocket(
            "earth_surface", "gto", &design, payload,
        ).unwrap();

        // Recompute the high-thrust dv along the same path using transfers
        // and aero drag, and make sure they match.
        let rocket_mass = design.total_mass_kg() + payload;
        let mut expected_dv = 0.0;
        for w in path.windows(2) {
            let t = DELTA_V_MAP.transfer(w[0], w[1]).unwrap();
            expected_dv += t.total_delta_v(rocket_mass);
        }
        assert!((dv - expected_dv).abs() < 1.0,
            "computed dv {} != expected high-thrust dv {} along path {:?}",
            dv, expected_dv, path);
    }
}
