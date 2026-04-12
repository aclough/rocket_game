use serde::{Serialize, Deserialize};

use crate::balance;
use crate::engine::EngineId;
use crate::engine_project::EngineSource;
use crate::resources;
use crate::rocket::RocketDesignId;
use crate::rocket_project::RocketProjectId;
use crate::team;

/// Unique identifier for a manufacturing order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ManufacturingOrderId(pub u64);

/// Unique identifier for an inventory item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InventoryItemId(pub u64);

// ── Floor space ──

/// Cost per floor space unit in dollars.
pub const FLOOR_SPACE_COST: f64 = 5_000_000.0;

/// Days to construct one floor space unit.
pub const FLOOR_SPACE_BUILD_DAYS: u32 = 30;

/// Starting floor space units.
pub const STARTING_FLOOR_SPACE: u32 = 12;

/// A floor space expansion order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorSpaceOrder {
    pub units: u32,
    pub days_remaining: u32,
}

/// Floor space management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorSpace {
    pub total_units: u32,
    pub under_construction: Vec<FloorSpaceOrder>,
}

impl FloorSpace {
    pub fn new() -> Self {
        FloorSpace {
            total_units: STARTING_FLOOR_SPACE,
            under_construction: Vec::new(),
        }
    }

    /// Start building more floor space. Returns cost.
    pub fn order_expansion(&mut self, units: u32) -> f64 {
        let cost = units as f64 * FLOOR_SPACE_COST;
        self.under_construction.push(FloorSpaceOrder {
            units,
            days_remaining: FLOOR_SPACE_BUILD_DAYS,
        });
        cost
    }

    /// Advance one day. Returns number of units completed.
    pub fn advance_day(&mut self) -> u32 {
        let mut completed = 0;
        self.under_construction.retain_mut(|order| {
            order.days_remaining = order.days_remaining.saturating_sub(1);
            if order.days_remaining == 0 {
                completed += order.units;
                false
            } else {
                true
            }
        });
        self.total_units += completed;
        completed
    }
}

// ── Manufacturing orders ──

/// What type of item is being manufactured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManufacturingOrderType {
    /// Build a single engine instance.
    Engine {
        source: EngineSource,
        engine_id: EngineId,
        engine_name: String,
        engine_mass_kg: f64,
        complexity: u32,
        /// Revision at time of order placement.
        revision: u32,
        /// Flaw snapshot at time of order placement.
        flaws: Vec<crate::flaw::Flaw>,
        /// Actualized improvements at time of order placement.
        improvements: Vec<crate::engine_project::Improvement>,
    },
    /// Build a single stage (tank + structure).
    Stage {
        rocket_project_id: RocketProjectId,
        group_index: usize,
        stage_index: usize,
        stage_name: String,
        structural_mass_kg: f64,
    },
    /// Final integration of a rocket.
    RocketIntegration {
        rocket_project_id: RocketProjectId,
        design_id: RocketDesignId,
        rocket_name: String,
        total_stages: u32,
        /// Rocket project revision at integration time.
        revision: u32,
        /// Rocket project flaw snapshot at integration time.
        rocket_flaws: Vec<crate::flaw::Flaw>,
    },
}

impl ManufacturingOrderType {
    /// Human-readable name for this order.
    pub fn display_name(&self) -> String {
        match self {
            ManufacturingOrderType::Engine { engine_name, .. } => engine_name.clone(),
            ManufacturingOrderType::Stage { stage_name, .. } => stage_name.clone(),
            ManufacturingOrderType::RocketIntegration { rocket_name, .. } => rocket_name.clone(),
        }
    }
}

/// A manufacturing order in progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManufacturingOrder {
    pub id: ManufacturingOrderId,
    pub order_type: ManufacturingOrderType,
    pub work_completed: f64,
    pub work_required: f64,
    pub material_cost: f64,
    pub teams_assigned: u32,
    pub floor_space_used: u32,
    /// If true, this order is waiting for prerequisite items in inventory.
    pub waiting_for_prerequisites: bool,
    /// How many of this design have been built before (for learning curve).
    pub prior_builds: u32,
}

/// Events emitted by manufacturing processing.
#[derive(Debug, Clone)]
pub enum ManufacturingEvent {
    EngineBuilt {
        order_id: ManufacturingOrderId,
        engine_id: EngineId,
        engine_name: String,
    },
    StageBuilt {
        order_id: ManufacturingOrderId,
        rocket_project_id: RocketProjectId,
        stage_name: String,
    },
    RocketIntegrated {
        order_id: ManufacturingOrderId,
        rocket_project_id: RocketProjectId,
        design_id: RocketDesignId,
        rocket_name: String,
        build_cost: f64,
    },
    FloorSpaceComplete {
        units: u32,
    },
}

impl ManufacturingOrder {
    /// Create an engine build order.
    pub fn new_engine(
        id: ManufacturingOrderId,
        source: EngineSource,
        engine_id: EngineId,
        engine_name: String,
        engine_mass_kg: f64,
        complexity: u32,
        preset: crate::engine_project::PropellantPreset,
        prior_builds: u32,
        revision: u32,
        flaws: Vec<crate::flaw::Flaw>,
        improvements: Vec<crate::engine_project::Improvement>,
    ) -> Self {
        let base_work = balance::engine_build_work(complexity);
        let learning = balance::learning_curve_multiplier(prior_builds);
        let material_cost = resources::engine_material_cost(preset, engine_mass_kg) * learning;

        ManufacturingOrder {
            id,
            order_type: ManufacturingOrderType::Engine {
                source,
                engine_id,
                engine_name,
                engine_mass_kg,
                complexity,
                revision,
                flaws,
                improvements,
            },
            work_completed: 0.0,
            work_required: base_work * learning,
            material_cost,
            teams_assigned: 0,
            floor_space_used: 1,
            waiting_for_prerequisites: false,
            prior_builds,
        }
    }

    /// Create a stage build order.
    pub fn new_stage(
        id: ManufacturingOrderId,
        rocket_project_id: RocketProjectId,
        group_index: usize,
        stage_index: usize,
        stage_name: String,
        structural_mass_kg: f64,
        prior_builds: u32,
    ) -> Self {
        let stage_total_mass = structural_mass_kg; // structural mass drives build work
        let base_work = balance::stage_build_work(stage_total_mass);
        let learning = balance::learning_curve_multiplier(prior_builds);
        let material_cost = (resources::tank_material_cost(structural_mass_kg)
            + resources::stage_assembly_cost()) * learning;

        ManufacturingOrder {
            id,
            order_type: ManufacturingOrderType::Stage {
                rocket_project_id,
                group_index,
                stage_index,
                stage_name,
                structural_mass_kg,
            },
            work_completed: 0.0,
            work_required: base_work * learning,
            material_cost,
            teams_assigned: 0,
            floor_space_used: 1,
            waiting_for_prerequisites: true, // wait for engines
            prior_builds,
        }
    }

    /// Create a rocket integration order.
    pub fn new_integration(
        id: ManufacturingOrderId,
        rocket_project_id: RocketProjectId,
        design_id: RocketDesignId,
        rocket_name: String,
        total_stages: u32,
        prior_builds: u32,
        revision: u32,
        rocket_flaws: Vec<crate::flaw::Flaw>,
    ) -> Self {
        let base_work = balance::rocket_integration_work(total_stages);
        let learning = balance::learning_curve_multiplier(prior_builds);
        let material_cost = resources::rocket_integration_cost() * learning;

        ManufacturingOrder {
            id,
            order_type: ManufacturingOrderType::RocketIntegration {
                rocket_project_id,
                design_id,
                rocket_name,
                total_stages,
                revision,
                rocket_flaws,
            },
            work_completed: 0.0,
            work_required: base_work * learning,
            material_cost,
            teams_assigned: 0,
            floor_space_used: total_stages, // scales with rocket size
            waiting_for_prerequisites: true, // wait for all stages
            prior_builds,
        }
    }

    /// Display name for this order.
    pub fn display_name(&self) -> &str {
        match &self.order_type {
            ManufacturingOrderType::Engine { engine_name, .. } => engine_name,
            ManufacturingOrderType::Stage { stage_name, .. } => stage_name,
            ManufacturingOrderType::RocketIntegration { rocket_name, .. } => rocket_name,
        }
    }

    /// Type label for display.
    pub fn type_label(&self) -> &'static str {
        match &self.order_type {
            ManufacturingOrderType::Engine { .. } => "Engine",
            ManufacturingOrderType::Stage { .. } => "Stage",
            ManufacturingOrderType::RocketIntegration { .. } => "Integration",
        }
    }

    /// Apply one day of manufacturing work. Returns true if completed.
    pub fn apply_daily_work(&mut self) -> bool {
        if self.waiting_for_prerequisites || self.teams_assigned == 0 {
            return false;
        }
        let work = team::manufacturing_work_rate(self.teams_assigned);
        self.work_completed += work;
        self.work_completed >= self.work_required
    }

    /// Progress as a fraction 0.0-1.0.
    pub fn progress(&self) -> f64 {
        if self.work_required <= 0.0 {
            return 1.0;
        }
        (self.work_completed / self.work_required).min(1.0)
    }
}

// ── Inventory ──

/// A built engine in inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryEngine {
    pub item_id: InventoryItemId,
    pub source: EngineSource,
    pub engine_id: EngineId,
    pub engine_name: String,
    /// Manufacturing cost of this engine.
    #[serde(default)]
    pub build_cost: f64,
    /// Revision of the engine project when this was built.
    #[serde(default)]
    pub revision: u32,
    /// Snapshot of flaws at build time.
    #[serde(default)]
    pub flaws: Vec<crate::flaw::Flaw>,
    /// Snapshot of actualized improvements at build time.
    #[serde(default)]
    pub improvements: Vec<crate::engine_project::Improvement>,
}

/// A built stage in inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryStage {
    pub item_id: InventoryItemId,
    pub rocket_project_id: RocketProjectId,
    pub group_index: usize,
    pub stage_index: usize,
    pub stage_name: String,
    /// Manufacturing cost of this stage (including consumed engine costs).
    #[serde(default)]
    pub build_cost: f64,
}

/// An integrated rocket ready for launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryRocket {
    pub item_id: InventoryItemId,
    pub rocket_project_id: RocketProjectId,
    pub design_id: RocketDesignId,
    pub rocket_name: String,
    /// Total build cost (sum of all stage costs + integration cost).
    #[serde(default)]
    pub build_cost: f64,
    /// Revision of the rocket project when this was integrated.
    #[serde(default)]
    pub revision: u32,
    /// Snapshot of rocket project flaws at build time.
    #[serde(default)]
    pub rocket_flaws: Vec<crate::flaw::Flaw>,
}

/// Inventory of manufactured items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub engines: Vec<InventoryEngine>,
    pub stages: Vec<InventoryStage>,
    pub rockets: Vec<InventoryRocket>,
}

impl Inventory {
    pub fn new() -> Self {
        Inventory {
            engines: Vec::new(),
            stages: Vec::new(),
            rockets: Vec::new(),
        }
    }

    /// Count engines matching a given engine source.
    pub fn engine_count(&self, source: EngineSource) -> usize {
        self.engines.iter()
            .filter(|e| e.source == source)
            .count()
    }

    /// Count stages matching a rocket project, group, and stage index.
    pub fn stage_count(&self, rocket_project_id: RocketProjectId, group_index: usize, stage_index: usize) -> usize {
        self.stages.iter()
            .filter(|s| s.rocket_project_id == rocket_project_id
                && s.group_index == group_index
                && s.stage_index == stage_index)
            .count()
    }

    /// Count integrated rockets for a given rocket project.
    pub fn rocket_count(&self, rocket_project_id: RocketProjectId) -> usize {
        self.rockets.iter()
            .filter(|r| r.rocket_project_id == rocket_project_id)
            .count()
    }

    /// Remove one engine matching the given source. Returns the removed item.
    pub fn take_engine(&mut self, source: EngineSource) -> Option<InventoryEngine> {
        let idx = self.engines.iter()
            .position(|e| e.source == source)?;
        Some(self.engines.remove(idx))
    }

    /// Remove one stage matching the given criteria. Returns the removed item.
    pub fn take_stage(&mut self, rocket_project_id: RocketProjectId, group_index: usize, stage_index: usize) -> Option<InventoryStage> {
        let idx = self.stages.iter()
            .position(|s| s.rocket_project_id == rocket_project_id
                && s.group_index == group_index
                && s.stage_index == stage_index)?;
        Some(self.stages.remove(idx))
    }

    /// Remove one rocket by item_id. Returns the removed item.
    pub fn take_rocket(&mut self, item_id: InventoryItemId) -> Option<InventoryRocket> {
        let idx = self.rockets.iter().position(|r| r.item_id == item_id)?;
        Some(self.rockets.remove(idx))
    }
}

// ── Manufacturing state ──

/// Top-level manufacturing state for a company.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manufacturing {
    pub floor_space: FloorSpace,
    pub orders: Vec<ManufacturingOrder>,
    pub inventory: Inventory,
    pub next_order_id: u64,
    pub next_inventory_id: u64,
}

impl Manufacturing {
    pub fn new() -> Self {
        Manufacturing {
            floor_space: FloorSpace::new(),
            orders: Vec::new(),
            inventory: Inventory::new(),
            next_order_id: 1,
            next_inventory_id: 1,
        }
    }

    /// Generate a new order ID.
    pub fn next_order_id(&mut self) -> ManufacturingOrderId {
        let id = ManufacturingOrderId(self.next_order_id);
        self.next_order_id += 1;
        id
    }

    /// Generate a new inventory item ID.
    pub fn next_inventory_id(&mut self) -> InventoryItemId {
        let id = InventoryItemId(self.next_inventory_id);
        self.next_inventory_id += 1;
        id
    }

    /// Floor space currently in use by active (non-waiting) orders.
    pub fn floor_space_in_use(&self) -> u32 {
        self.orders.iter()
            .filter(|o| !o.waiting_for_prerequisites)
            .map(|o| o.floor_space_used)
            .sum()
    }

    /// Floor space available.
    pub fn floor_space_available(&self) -> u32 {
        self.floor_space.total_units.saturating_sub(self.floor_space_in_use())
    }

    /// Total manufacturing teams assigned across all orders.
    pub fn total_teams_assigned(&self) -> u32 {
        self.orders.iter().map(|o| o.teams_assigned).sum()
    }

    /// Add a team to an order. Returns true if successful.
    pub fn add_team_to_order(&mut self, order_index: usize, available_teams: u32) -> bool {
        if available_teams == 0 || order_index >= self.orders.len() {
            return false;
        }
        let order = &mut self.orders[order_index];
        if order.waiting_for_prerequisites {
            return false;
        }
        order.teams_assigned += 1;
        true
    }

    /// Remove a team from an order. Returns true if successful.
    pub fn remove_team_from_order(&mut self, order_index: usize) -> bool {
        if order_index >= self.orders.len() {
            return false;
        }
        let order = &mut self.orders[order_index];
        if order.teams_assigned == 0 {
            return false;
        }
        order.teams_assigned -= 1;
        true
    }

    /// Process one day of manufacturing work. Returns events.
    pub fn advance_day(&mut self) -> Vec<ManufacturingEvent> {
        let mut events = Vec::new();

        // Process floor space construction
        let floor_completed = self.floor_space.advance_day();
        if floor_completed > 0 {
            events.push(ManufacturingEvent::FloorSpaceComplete { units: floor_completed });
        }

        // Process manufacturing orders
        let mut completed_indices = Vec::new();
        for (i, order) in self.orders.iter_mut().enumerate() {
            if order.apply_daily_work() {
                completed_indices.push(i);
            }
        }

        // Handle completed orders (in reverse to preserve indices)
        for &i in completed_indices.iter().rev() {
            let order = self.orders.remove(i);
            let item_id = self.next_inventory_id();

            match &order.order_type {
                ManufacturingOrderType::Engine { source, engine_id, engine_name, revision, flaws, improvements, .. } => {
                    self.inventory.engines.push(InventoryEngine {
                        item_id,
                        source: *source,
                        engine_id: *engine_id,
                        engine_name: engine_name.clone(),
                        build_cost: order.material_cost,
                        revision: *revision,
                        flaws: flaws.clone(),
                        improvements: improvements.clone(),
                    });
                    events.push(ManufacturingEvent::EngineBuilt {
                        order_id: order.id,
                        engine_id: *engine_id,
                        engine_name: engine_name.clone(),
                    });
                }
                ManufacturingOrderType::Stage { rocket_project_id, group_index, stage_index, stage_name, .. } => {
                    self.inventory.stages.push(InventoryStage {
                        item_id,
                        rocket_project_id: *rocket_project_id,
                        group_index: *group_index,
                        stage_index: *stage_index,
                        stage_name: stage_name.clone(),
                        build_cost: order.material_cost,
                    });
                    events.push(ManufacturingEvent::StageBuilt {
                        order_id: order.id,
                        rocket_project_id: *rocket_project_id,
                        stage_name: stage_name.clone(),
                    });
                }
                ManufacturingOrderType::RocketIntegration { rocket_project_id, design_id, rocket_name, revision, rocket_flaws, .. } => {
                    let total_build_cost = order.material_cost;
                    self.inventory.rockets.push(InventoryRocket {
                        item_id,
                        rocket_project_id: *rocket_project_id,
                        design_id: *design_id,
                        rocket_name: rocket_name.clone(),
                        build_cost: total_build_cost,
                        revision: *revision,
                        rocket_flaws: rocket_flaws.clone(),
                    });
                    events.push(ManufacturingEvent::RocketIntegrated {
                        order_id: order.id,
                        rocket_project_id: *rocket_project_id,
                        design_id: *design_id,
                        rocket_name: rocket_name.clone(),
                        build_cost: total_build_cost,
                    });
                }
            }
        }

        // Try to unblock waiting orders
        self.try_unblock_orders();

        events
    }

    /// Check if waiting orders can now proceed (prerequisites in inventory).
    fn try_unblock_orders(&mut self) {
        for order in &mut self.orders {
            if !order.waiting_for_prerequisites {
                continue;
            }

            let can_unblock = match &order.order_type {
                ManufacturingOrderType::Engine { .. } => {
                    // Engines have no prerequisites
                    true
                }
                ManufacturingOrderType::Stage { .. } => {
                    // Stages need engines — but we check this at the Company level
                    // since we need to know which engine project each stage uses.
                    // For now, stages are unblocked by the Company layer.
                    false // leave blocked, Company will unblock
                }
                ManufacturingOrderType::RocketIntegration { .. } => {
                    // Integration needs all stages — checked by Company layer
                    false // leave blocked, Company will unblock
                }
            };

            if can_unblock {
                order.waiting_for_prerequisites = false;
            }
        }
    }

    /// Count pending engine orders for a given engine source.
    pub fn pending_engine_orders(&self, source: EngineSource) -> u32 {
        self.orders.iter()
            .filter(|o| matches!(&o.order_type, ManufacturingOrderType::Engine { source: s, .. } if *s == source))
            .count() as u32
    }

    /// Count pending stage orders for a given rocket project.
    pub fn pending_stage_orders(&self, rocket_project_id: RocketProjectId) -> u32 {
        self.orders.iter()
            .filter(|o| matches!(&o.order_type, ManufacturingOrderType::Stage { rocket_project_id: id, .. } if *id == rocket_project_id))
            .count() as u32
    }

    /// Count pending integration orders for a given rocket project.
    pub fn pending_integration_orders(&self, rocket_project_id: RocketProjectId) -> u32 {
        self.orders.iter()
            .filter(|o| matches!(&o.order_type, ManufacturingOrderType::RocketIntegration { rocket_project_id: id, .. } if *id == rocket_project_id))
            .count() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_project::EngineProjectId;

    fn test_source() -> EngineSource {
        EngineSource::PlayerDesign(EngineProjectId(1))
    }

    #[test]
    fn test_floor_space_new() {
        let fs = FloorSpace::new();
        assert_eq!(fs.total_units, STARTING_FLOOR_SPACE);
        assert!(fs.under_construction.is_empty());
    }

    #[test]
    fn test_floor_space_expansion() {
        let mut fs = FloorSpace::new();
        let cost = fs.order_expansion(2);
        assert_eq!(cost, 2.0 * FLOOR_SPACE_COST);

        // Advance 29 days — not done yet
        for _ in 0..29 {
            assert_eq!(fs.advance_day(), 0);
        }
        assert_eq!(fs.total_units, STARTING_FLOOR_SPACE);

        // Day 30 — complete
        assert_eq!(fs.advance_day(), 2);
        assert_eq!(fs.total_units, STARTING_FLOOR_SPACE + 2);
    }

    #[test]
    fn test_manufacturing_order_engine() {
        let order = ManufacturingOrder::new_engine(
            ManufacturingOrderId(1),
            test_source(),
            EngineId(1),
            "Merlin".into(),
            500.0,
            6,
            crate::engine_project::PropellantPreset::Kerolox,
            0,
            0, Vec::new(), Vec::new(),
        );
        assert!(order.work_required > 0.0);
        assert!(order.material_cost > 0.0);
        assert_eq!(order.floor_space_used, 1);
        assert!(!order.waiting_for_prerequisites);
    }

    #[test]
    fn test_manufacturing_order_stage() {
        let order = ManufacturingOrder::new_stage(
            ManufacturingOrderId(2),
            RocketProjectId(1),
            0, 0,
            "S1".into(),
            3000.0,
            0,
        );
        assert!(order.work_required > 0.0);
        assert!(order.material_cost > 0.0);
        assert!(order.waiting_for_prerequisites);
    }

    #[test]
    fn test_manufacturing_order_integration() {
        let order = ManufacturingOrder::new_integration(
            ManufacturingOrderId(3),
            RocketProjectId(1),
            RocketDesignId(1),
            "Falcon".into(),
            2,
            0,
            0, Vec::new(),
        );
        assert!(order.work_required > 0.0);
        assert!(order.material_cost > 0.0);
        assert!(order.waiting_for_prerequisites);
        assert_eq!(order.floor_space_used, 2);
    }

    #[test]
    fn test_learning_curve_reduces_cost() {
        let first = ManufacturingOrder::new_engine(
            ManufacturingOrderId(1), test_source(), EngineId(1),
            "Merlin".into(), 500.0, 6,
            crate::engine_project::PropellantPreset::Kerolox, 0,
            0, Vec::new(), Vec::new(),
        );
        let tenth = ManufacturingOrder::new_engine(
            ManufacturingOrderId(2), test_source(), EngineId(2),
            "Merlin".into(), 500.0, 6,
            crate::engine_project::PropellantPreset::Kerolox, 10,
            0, Vec::new(), Vec::new(),
        );
        assert!(tenth.work_required < first.work_required,
            "10th build work {} should be less than first {}", tenth.work_required, first.work_required);
        assert!(tenth.material_cost < first.material_cost,
            "10th build cost {} should be less than first {}", tenth.material_cost, first.material_cost);
    }

    #[test]
    fn test_engine_build_completes() {
        let mut mfg = Manufacturing::new();
        let id = mfg.next_order_id();
        let mut order = ManufacturingOrder::new_engine(
            id, test_source(), EngineId(1),
            "Merlin".into(), 500.0, 6,
            crate::engine_project::PropellantPreset::Kerolox, 0,
            0, Vec::new(), Vec::new(),
        );
        order.teams_assigned = 2;
        mfg.orders.push(order);

        let mut engine_built = false;
        for _ in 0..500 {
            let events = mfg.advance_day();
            for evt in &events {
                if matches!(evt, ManufacturingEvent::EngineBuilt { .. }) {
                    engine_built = true;
                }
            }
            if engine_built { break; }
        }

        assert!(engine_built, "Engine should have been built within 500 days");
        assert_eq!(mfg.inventory.engines.len(), 1);
        assert_eq!(mfg.inventory.engine_count(test_source()), 1);
    }

    #[test]
    fn test_inventory_take_engine() {
        let mut inv = Inventory::new();
        inv.engines.push(InventoryEngine {
            item_id: InventoryItemId(1),
            source: test_source(),
            engine_id: EngineId(1),
            engine_name: "Merlin".into(),
            build_cost: 0.0, revision: 0, flaws: Vec::new(), improvements: Vec::new(),
        });
        inv.engines.push(InventoryEngine {
            item_id: InventoryItemId(2),
            source: test_source(),
            engine_id: EngineId(2),
            engine_name: "Merlin".into(),
            build_cost: 0.0, revision: 0, flaws: Vec::new(), improvements: Vec::new(),
        });

        assert_eq!(inv.engine_count(test_source()), 2);
        let taken = inv.take_engine(test_source());
        assert!(taken.is_some());
        assert_eq!(inv.engine_count(test_source()), 1);
    }

    #[test]
    fn test_floor_space_tracking() {
        let mut mfg = Manufacturing::new();
        let id = mfg.next_order_id();
        let mut order = ManufacturingOrder::new_engine(
            id, test_source(), EngineId(1),
            "Merlin".into(), 500.0, 6,
            crate::engine_project::PropellantPreset::Kerolox, 0,
            0, Vec::new(), Vec::new(),
        );
        order.teams_assigned = 1;
        mfg.orders.push(order);

        assert_eq!(mfg.floor_space_in_use(), 1);
        assert_eq!(mfg.floor_space_available(), STARTING_FLOOR_SPACE - 1);
    }

    #[test]
    fn test_waiting_orders_dont_use_floor_space() {
        let mut mfg = Manufacturing::new();
        let id = mfg.next_order_id();
        let order = ManufacturingOrder::new_stage(
            id, RocketProjectId(1), 0, 0, "S1".into(), 3000.0, 0,
        );
        mfg.orders.push(order);

        // Waiting orders don't use floor space
        assert_eq!(mfg.floor_space_in_use(), 0);
        assert_eq!(mfg.floor_space_available(), STARTING_FLOOR_SPACE);
    }

    #[test]
    fn test_waiting_orders_dont_progress() {
        let mut mfg = Manufacturing::new();
        let id = mfg.next_order_id();
        let mut order = ManufacturingOrder::new_stage(
            id, RocketProjectId(1), 0, 0, "S1".into(), 3000.0, 0,
        );
        order.teams_assigned = 2;
        mfg.orders.push(order);

        // Advance some days
        for _ in 0..10 {
            mfg.advance_day();
        }

        // Should have made no progress (waiting for prerequisites)
        assert_eq!(mfg.orders[0].work_completed, 0.0);
    }

    #[test]
    fn test_unblocked_orders_progress() {
        let mut mfg = Manufacturing::new();
        let id = mfg.next_order_id();
        let mut order = ManufacturingOrder::new_stage(
            id, RocketProjectId(1), 0, 0, "S1".into(), 3000.0, 0,
        );
        order.teams_assigned = 2;
        order.waiting_for_prerequisites = false; // manually unblock
        mfg.orders.push(order);

        for _ in 0..10 {
            mfg.advance_day();
        }

        assert!(mfg.orders[0].work_completed > 0.0, "Should have made progress");
    }

    #[test]
    fn test_order_progress() {
        let mut order = ManufacturingOrder::new_engine(
            ManufacturingOrderId(1), test_source(), EngineId(1),
            "Merlin".into(), 500.0, 6,
            crate::engine_project::PropellantPreset::Kerolox, 0,
            0, Vec::new(), Vec::new(),
        );
        assert!((order.progress() - 0.0).abs() < 0.001);

        order.work_completed = order.work_required / 2.0;
        assert!((order.progress() - 0.5).abs() < 0.001);

        order.work_completed = order.work_required;
        assert!((order.progress() - 1.0).abs() < 0.001);
    }
}
