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
use crate::rocket::{DesignPerformance, Rocket, RocketDesign};

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
    rocket_mass_kg: f64,
    active_stage: usize,
    dv_left_in_active: f64,
    class: ThrustClass,
    perf: &DesignPerformance,
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
        // Charging gravity to the group rather than the edge is what keeps
        // the planner honest about low-TWR designs: the loss depends on the
        // ascent profile, not on which transfer is being flown. Edges stay
        // responsible for drag — see `edge_cost_for_class`.
        let stage_dv = perf.planner_dv(new_active);
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

/// Result of `plan_mission` — either a feasible route, or a classified
/// reason the rocket can't make it. The fallback diagnosis runs the
/// class-restricted Dijkstra (low-thrust subgraph for ion designs, full
/// graph for chemical) so the Δv shortfall is measured against the
/// cheapest route the rocket *could* actually fly.
#[derive(Debug, Clone)]
pub enum MissionPlan {
    /// A feasible stage-aware route was found.
    Reachable { path: Vec<&'static str>, dv: f64 },
    /// No path exists in the Δv graph between origin and destination
    /// at all (disconnected nodes).
    NoGraphPath,
    /// A class-compatible path exists but its Δv cost exceeds what the
    /// rocket can deliver. The rocket needs more fuel / better Isp.
    /// `available_dv` is the planner's own figure (vacuum less ascent
    /// losses), so "need X, have Y" is the comparison it actually made.
    DvShortfall { min_required_dv: f64, available_dv: f64 },
    /// The rocket has the Δv for the cheapest class-compatible route in
    /// total, but no stage sequence can spend it along that route (a
    /// stage runs dry mid-edge and the next can't take over). A staging
    /// problem, not a propellant one.
    StagingInfeasible { min_required_dv: f64, available_dv: f64 },
    /// No class-compatible path exists: the rocket's engine type
    /// (low-thrust vs high-thrust) can't fly any route to the
    /// destination. The player needs to change engine type.
    ClassMismatch { available_dv: f64 },
}

impl DeltaVMap {
    /// Plan a mission and classify the failure if any. Wraps
    /// `shortest_path_for_rocket` with a fall-back diagnosis so the
    /// caller can tell *why* a destination is unreachable.
    pub fn plan_mission(
        &self,
        from: &str,
        to: &str,
        design: &RocketDesign,
        payload_mass_kg: f64,
    ) -> MissionPlan {
        if let Some((path, dv)) = self.shortest_path_for_rocket(from, to, design, payload_mass_kg) {
            return MissionPlan::Reachable { path, dv };
        }
        let rocket_mass = design.total_mass_kg() + payload_mass_kg;
        // Disconnected in the underlying graph?
        if self.shortest_path(from, to, rocket_mass).is_none() {
            return MissionPlan::NoGraphPath;
        }
        let available_dv = DesignPerformance::compute(design, payload_mass_kg, from).total_planner_dv();
        // Cheapest route restricted to the rocket's thrust class. For
        // low-thrust designs (always single-stage by designer rule) this
        // is the low-thrust subgraph. For chemical-only designs every
        // edge is high-thrust-feasible, so the unconstrained mass-only
        // path is the right answer.
        let class_route = if design.is_low_thrust() {
            self.shortest_path_constrained(from, to, rocket_mass, true)
        } else {
            self.shortest_path(from, to, rocket_mass)
        };
        match class_route {
            None => MissionPlan::ClassMismatch { available_dv },
            Some((_, min_dv)) if available_dv < min_dv =>
                MissionPlan::DvShortfall { min_required_dv: min_dv, available_dv },
            Some((_, min_dv)) =>
                // Class-compatible path exists and the rocket has enough
                // total Δv, but the stage-aware planner still failed:
                // the Δv is in the wrong stages for that route. Saying
                // "shortfall" here produced a negative shortfall figure.
                MissionPlan::StagingInfeasible { min_required_dv: min_dv, available_dv },
        }
    }

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
        let perf = DesignPerformance::compute(design, payload_mass_kg, from);
        let initial_dv = perf.planner_dv(0);
        self.astar_search(from, to, design, payload_mass_kg, 0, initial_dv, &perf)
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
        // The lowest still-attached stage with propellant remaining.
        let active_stage = rocket.active_group()?;
        // Zero for anything already off a surface, which is the usual case
        // here — a parked spacecraft flying on from orbit owes no ascent.
        // A craft sitting on the lunar surface does, and pays it.
        let perf = DesignPerformance::compute(design, rocket.payload_mass_kg, from);
        let initial_dv = (rocket.group_remaining_delta_v(design, active_stage)
            - perf.ascent.gravity_by_group.get(active_stage).copied().unwrap_or(0.0))
            .max(0.0);
        self.astar_search(
            from, to, design, rocket.payload_mass_kg, active_stage, initial_dv, &perf,
        )
    }

    #[allow(clippy::too_many_arguments)] // ditto — the search's starting state
    fn astar_search(
        &self,
        from: &str,
        to: &str,
        design: &RocketDesign,
        payload_mass_kg: f64,
        initial_active_stage: usize,
        initial_dv_left: f64,
        perf: &DesignPerformance,
    ) -> Option<(Vec<&'static str>, f64)> {
        let from_idx = self.index_of(from)?;
        let to_idx = self.index_of(to)?;

        let h = self.heuristic_to(to_idx);
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
                .is_some_and(|f| f.iter().any(|&(g, dv)| {
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
                let Some(next_idx) = self.index_of(transfer.to) else { continue };

                for class in [ThrustClass::HighThrust, ThrustClass::LowThrust] {
                    let outcome = match try_class(
                        transfer,
                        design,
                        rocket_mass_kg,
                        state.active_stage,
                        state.dv_left_in_active,
                        class,
                        perf,
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
    use crate::test_util::{kerolox_engine};
    use crate::engine::{EngineCycle, EngineDesign, EngineId, PropellantFraction};
    use crate::location::DELTA_V_MAP;
    use crate::propellant::Propellant;
    use crate::rocket::{RocketDesign, RocketDesignId};
    use crate::stage::{Stage, StageId};


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
            chamber_pressure_pa: 0.0,
            expansion_ratio: 0.0,
            gamma: 0.0,
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

    /// 2-stage chemical: big booster + high-energy upper (the 420 s Isp
    /// stands in for a hydrolox second stage).
    ///
    /// Sized to reach GTO with margin now that the planner charges gravity
    /// losses. The earlier fixture had ~11 km/s of vacuum delta-v and a
    /// liftoff TWR of 1.5, which buys LEO and nothing beyond it once the
    /// ~2.6 km/s it spends fighting gravity is taken off the top.
    fn two_stage_chemical() -> RocketDesign {
        let s1 = stage(1, "S1", kerolox_engine(1, 18_000_000.0, 1500.0, 280.0), 1, 500_000.0, 25_000.0);
        let s2 = stage(2, "S2", kerolox_engine(2, 1_500_000.0, 800.0, 420.0), 1, 300_000.0, 15_000.0);
        RocketDesign {
            id: RocketDesignId(1), name: "TwoChem".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        }
    }

    /// 2-stage hybrid: chemical booster + ion upper. Sized so S1 alone can
    /// reach LEO — gravity losses and the 70 kPa nozzle's sea-level Isp
    /// penalty (~6% of S1's Δv) included — and the ion S2 carries enough
    /// xenon to spiral to NEA. At 280 s the booster had ~8.4 km/s for a
    /// ~8.3 km/s ascent before the planner charged overexpansion; more
    /// propellant barely helps at a mass ratio of 30, so the fixture's
    /// Isp is 310 s (vacuum kerolox territory) to keep the margin.
    fn chemical_then_ion() -> RocketDesign {
        let s1 = stage(1, "S1", kerolox_engine(1, 50_000_000.0, 5_000.0, 310.0), 1, 2_400_000.0, 40_000.0);
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
        let goal_idx = DELTA_V_MAP.index_of("lunar_surface").unwrap();
        let h = DELTA_V_MAP.heuristic_to(goal_idx);
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
        // S1 thrust is sized for a liftoff TWR above 1 — at the original
        // 7 MN this stack couldn't leave the pad, and "has enough delta-v"
        // no longer implies "flies" now that gravity is charged. S2's
        // thrust gives it a TWR near 1 at ignition: the pitch program
        // (D7) stages at 30° above the horizon, where a 0.24-TWR upper
        // stage would spend its burn falling, not climbing.
        let s1 = stage(1, "S1", kerolox_engine(1, 12_000_000.0, 1_500.0, 280.0), 1, 200_000.0, 15_000.0);
        let s2 = stage(2, "S2", kerolox_engine(2, 6_000_000.0, 3_000.0, 340.0), 1, 600_000.0, 30_000.0);
        let design = RocketDesign {
            id: RocketDesignId(10), name: "SmallS1+BigS2".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        };

        // Sanity: stage 1 alone shouldn't reach LEO.
        let s1_dv = DesignPerformance::compute(&design, 1_000.0, "earth_surface").planner_dv(0);
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
            expected_dv += t.cost_for_mass(rocket_mass);
        }
        assert!((dv - expected_dv).abs() < 1.0,
            "computed dv {} != expected high-thrust dv {} along path {:?}",
            dv, expected_dv, path);
    }
}
