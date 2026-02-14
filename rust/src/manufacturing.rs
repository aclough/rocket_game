/// Manufacturing system: floor space, orders, and inventory for building engines and rockets.
/// Manufacturing is a phase between "design complete" and "ready to launch" where teams
/// spend time and materials to produce physical hardware.

use crate::engine_design::EngineDesignSnapshot;
use crate::rocket_design::RocketDesign;
use crate::stage::RocketStage;

// ==========================================
// Constants
// ==========================================

/// Exponent for scaling engine build work with scale factor
pub const ENGINE_BUILD_SCALE_EXPONENT: f64 = 0.75;

/// Base assembly days per stage (tank fabrication, structural work, stage integration)
pub const STAGE_BASE_ASSEMBLY_DAYS: f64 = 60.0;

/// Extra assembly days per additional engine beyond the first (mounting, plumbing)
pub const ASSEMBLY_DAYS_PER_EXTRA_ENGINE: f64 = 5.0;

/// Days for final rocket integration (stacking, harness, checkout)
pub const ROCKET_INTEGRATION_DAYS: f64 = 30.0;

/// Team efficiency exponent for manufacturing (more parallelizable than design work)
pub const MANUFACTURING_TEAM_EXPONENT: f64 = 0.85;

/// Cost per unit of floor space
pub const FLOOR_SPACE_COST_PER_UNIT: f64 = 5_000_000.0;

/// Days for new floor space to be constructed
pub const FLOOR_SPACE_CONSTRUCTION_DAYS: u32 = 30;

/// Starting floor space units (enough for default rocket: 2 stages * 2 + 6 engines = 10, + 2 spare)
pub const STARTING_FLOOR_SPACE: usize = 12;

// ==========================================
// Floor Space
// ==========================================

/// A floor space construction order (space being built)
#[derive(Debug, Clone)]
pub struct FloorSpaceOrder {
    /// Number of units being built
    pub units: usize,
    /// Days remaining until construction completes
    pub days_remaining: u32,
}

// ==========================================
// Manufacturing Orders
// ==========================================

/// Unique ID for a manufacturing order
pub type ManufacturingOrderId = u32;

/// What kind of thing is being manufactured
#[derive(Debug, Clone)]
pub enum ManufacturingOrderType {
    /// Building engines from a frozen engine design revision
    Engine {
        engine_design_id: usize,
        revision_number: u32,
        /// Snapshot of engine stats at time of order
        snapshot: EngineDesignSnapshot,
        /// Total quantity to build
        quantity: u32,
        /// How many have been completed so far
        completed: u32,
    },
    /// Assembling a rocket from a frozen rocket design revision
    Rocket {
        rocket_design_id: usize,
        revision_number: u32,
        /// Snapshot of the rocket design at time of order
        design_snapshot: RocketDesign,
    },
}

/// A manufacturing order that teams work on
#[derive(Debug, Clone)]
pub struct ManufacturingOrder {
    /// Unique order ID
    pub id: ManufacturingOrderId,
    /// What is being manufactured
    pub order_type: ManufacturingOrderType,
    /// Current work progress on the current unit
    pub progress: f64,
    /// Total work required for the current unit
    pub total_work: f64,
    /// Base total work (stored separately for future learning curve)
    pub base_total_work: f64,
    /// Material cost per unit (paid up front when order starts)
    pub material_cost_per_unit: f64,
    /// Floor space units consumed by this order
    pub floor_space_used: usize,
    /// Whether this rocket order is waiting for engines to be manufactured
    /// When true, no teams can be assigned and no work progresses
    pub waiting_for_engines: bool,
}

impl ManufacturingOrder {
    /// Get display name for this order
    pub fn display_name(&self) -> String {
        match &self.order_type {
            ManufacturingOrderType::Engine { snapshot, quantity, completed, .. } => {
                format!("{} ({}/{})", snapshot.name, completed, quantity)
            }
            ManufacturingOrderType::Rocket { design_snapshot, .. } => {
                format!("Assemble {}", design_snapshot.name)
            }
        }
    }

    /// Get progress as a fraction (0.0 to 1.0) for the current unit
    pub fn progress_fraction(&self) -> f64 {
        if self.total_work > 0.0 {
            (self.progress / self.total_work).min(1.0)
        } else {
            0.0
        }
    }

    /// Check if the current unit is complete
    pub fn is_unit_complete(&self) -> bool {
        self.progress >= self.total_work
    }

    /// Check if the entire order is complete (all units built)
    pub fn is_order_complete(&self) -> bool {
        match &self.order_type {
            ManufacturingOrderType::Engine { quantity, completed, .. } => {
                *completed >= *quantity
            }
            ManufacturingOrderType::Rocket { .. } => {
                self.is_unit_complete()
            }
        }
    }

    /// Whether this is an engine order
    pub fn is_engine_order(&self) -> bool {
        matches!(self.order_type, ManufacturingOrderType::Engine { .. })
    }

    /// Whether this is a rocket order
    pub fn is_rocket_order(&self) -> bool {
        matches!(self.order_type, ManufacturingOrderType::Rocket { .. })
    }

    /// Calculate total remaining work across all units (current + future)
    pub fn remaining_work(&self) -> f64 {
        let current_unit_remaining = (self.total_work - self.progress).max(0.0);
        match &self.order_type {
            ManufacturingOrderType::Engine { quantity, completed, .. } => {
                let remaining_units = (*quantity as f64) - (*completed as f64) - 1.0;
                current_unit_remaining + remaining_units.max(0.0) * self.total_work
            }
            ManufacturingOrderType::Rocket { .. } => current_unit_remaining,
        }
    }
}

// ==========================================
// Inventory
// ==========================================

/// An entry in the engine inventory
#[derive(Debug, Clone)]
pub struct EngineInventoryEntry {
    /// Which engine design lineage this came from
    pub engine_design_id: usize,
    /// Which revision was used to build it
    pub revision_number: u32,
    /// Snapshot of stats at time of manufacture
    pub snapshot: EngineDesignSnapshot,
    /// Number of this engine type available
    pub quantity: u32,
}

/// An entry in the rocket inventory (each rocket is unique)
#[derive(Debug, Clone)]
pub struct RocketInventoryEntry {
    /// Which rocket design lineage this came from
    pub rocket_design_id: usize,
    /// Which revision was used to build it
    pub revision_number: u32,
    /// Snapshot of the full design at time of manufacture
    pub design_snapshot: RocketDesign,
    /// Unique serial number for this vehicle
    pub serial_number: u32,
}

// ==========================================
// Floor Space Calculation Functions
// ==========================================

/// Calculate floor space needed for an engine order (based on scale)
pub fn floor_space_for_engine(scale: f64) -> usize {
    scale.ceil() as usize
}

/// Calculate floor space needed for a rocket assembly order
pub fn floor_space_for_rocket(design: &RocketDesign) -> usize {
    let stage_space = design.stages.len() * 2;
    let engine_space: u32 = design.stages.iter().map(|s| s.engine_count).sum();
    stage_space + engine_space as usize
}

// ==========================================
// Manufacturing State
// ==========================================

/// All manufacturing state for a company
#[derive(Debug, Clone)]
pub struct Manufacturing {
    /// Total floor space units available
    pub floor_space_total: usize,
    /// Floor space currently under construction
    pub floor_space_under_construction: Vec<FloorSpaceOrder>,
    /// Active manufacturing orders
    pub active_orders: Vec<ManufacturingOrder>,
    /// Engine inventory (grouped by design+revision)
    pub engine_inventory: Vec<EngineInventoryEntry>,
    /// Assembled rockets ready for launch
    pub rocket_inventory: Vec<RocketInventoryEntry>,
    /// Next order ID
    next_order_id: ManufacturingOrderId,
    /// Next rocket serial number
    next_serial_number: u32,
    /// Number of engines produced per design (for future learning curve)
    pub engine_production_history: Vec<(usize, u32)>,
    /// Number of rockets produced per design (for future learning curve)
    pub rocket_production_history: Vec<(usize, u32)>,
}

impl Manufacturing {
    pub fn new() -> Self {
        Self {
            floor_space_total: STARTING_FLOOR_SPACE,
            floor_space_under_construction: Vec::new(),
            active_orders: Vec::new(),
            engine_inventory: Vec::new(),
            rocket_inventory: Vec::new(),
            next_order_id: 1,
            next_serial_number: 1,
            engine_production_history: Vec::new(),
            rocket_production_history: Vec::new(),
        }
    }

    /// Floor space currently in use by active orders
    pub fn floor_space_in_use(&self) -> usize {
        self.active_orders.iter().map(|o| o.floor_space_used).sum()
    }

    /// Floor space available for new orders
    pub fn floor_space_available(&self) -> usize {
        self.floor_space_total.saturating_sub(self.floor_space_in_use())
    }

    /// Total floor space currently under construction
    pub fn floor_space_constructing(&self) -> usize {
        self.floor_space_under_construction.iter().map(|o| o.units).sum()
    }

    /// Buy new floor space (creates a construction order)
    pub fn buy_floor_space(&mut self, units: usize) {
        self.floor_space_under_construction.push(FloorSpaceOrder {
            units,
            days_remaining: FLOOR_SPACE_CONSTRUCTION_DAYS,
        });
    }

    /// Process one day of floor space construction.
    /// Returns the total number of units that completed construction this day.
    pub fn process_construction(&mut self) -> usize {
        let mut completed_units = 0;
        for order in &mut self.floor_space_under_construction {
            if order.days_remaining > 0 {
                order.days_remaining -= 1;
                if order.days_remaining == 0 {
                    completed_units += order.units;
                }
            }
        }
        // Move completed units to total
        self.floor_space_total += completed_units;
        // Remove completed construction orders
        self.floor_space_under_construction.retain(|o| o.days_remaining > 0);
        completed_units
    }

    /// Check if we can start an engine order (enough floor space)
    pub fn can_start_engine_order_with_space(&self, space_needed: usize) -> bool {
        space_needed <= self.floor_space_available()
    }

    /// Check if we can start a rocket order (enough floor space)
    pub fn can_start_rocket_order_with_space(&self, space_needed: usize) -> bool {
        space_needed <= self.floor_space_available()
    }

    /// Start a new engine manufacturing order.
    /// Returns the order ID and total material cost, or None if insufficient floor space.
    pub fn start_engine_order(
        &mut self,
        engine_design_id: usize,
        revision_number: u32,
        snapshot: EngineDesignSnapshot,
        quantity: u32,
    ) -> Option<(ManufacturingOrderId, f64)> {
        let space_needed = floor_space_for_engine(snapshot.scale);
        if !self.can_start_engine_order_with_space(space_needed) {
            return None;
        }

        let material_cost = engine_material_cost(&snapshot);
        let total_material = material_cost * quantity as f64;
        let build_work = engine_build_work(&snapshot);

        let order_id = self.next_order_id;
        self.next_order_id += 1;

        self.active_orders.push(ManufacturingOrder {
            id: order_id,
            order_type: ManufacturingOrderType::Engine {
                engine_design_id,
                revision_number,
                snapshot,
                quantity,
                completed: 0,
            },
            progress: 0.0,
            total_work: build_work,
            base_total_work: build_work,
            material_cost_per_unit: material_cost,
            floor_space_used: space_needed,
            waiting_for_engines: false,
        });

        Some((order_id, total_material))
    }

    /// Start a new rocket assembly order.
    /// Returns the order ID and material cost, or None if insufficient floor space.
    /// If `waiting_for_engines` is true, the order is blocked until engines arrive.
    pub fn start_rocket_order(
        &mut self,
        rocket_design_id: usize,
        revision_number: u32,
        design_snapshot: RocketDesign,
        waiting_for_engines: bool,
    ) -> Option<(ManufacturingOrderId, f64)> {
        let space_needed = floor_space_for_rocket(&design_snapshot);
        if !self.can_start_rocket_order_with_space(space_needed) {
            return None;
        }

        let material_cost = rocket_material_cost(&design_snapshot);
        let assembly_work = rocket_assembly_work(&design_snapshot);

        let order_id = self.next_order_id;
        self.next_order_id += 1;

        self.active_orders.push(ManufacturingOrder {
            id: order_id,
            order_type: ManufacturingOrderType::Rocket {
                rocket_design_id,
                revision_number,
                design_snapshot,
            },
            progress: 0.0,
            total_work: assembly_work,
            base_total_work: assembly_work,
            material_cost_per_unit: material_cost,
            floor_space_used: space_needed,
            waiting_for_engines,
        });

        Some((order_id, material_cost))
    }

    /// Cancel a manufacturing order by ID.
    /// Returns true if found and removed.
    /// Note: material cost is NOT refunded (sunk cost).
    pub fn cancel_order(&mut self, order_id: ManufacturingOrderId) -> bool {
        if let Some(pos) = self.active_orders.iter().position(|o| o.id == order_id) {
            self.active_orders.remove(pos);
            true
        } else {
            false
        }
    }

    /// Increase quantity of an existing engine order.
    /// Returns the additional material cost, or None if the order doesn't exist
    /// or is not an engine order.
    pub fn increase_engine_order_quantity(
        &mut self,
        order_id: ManufacturingOrderId,
        quantity_to_add: u32,
    ) -> Option<f64> {
        let order = self.active_orders.iter_mut().find(|o| o.id == order_id)?;
        match &mut order.order_type {
            ManufacturingOrderType::Engine { quantity, .. } => {
                *quantity += quantity_to_add;
                Some(order.material_cost_per_unit * quantity_to_add as f64)
            }
            ManufacturingOrderType::Rocket { .. } => None,
        }
    }

    /// Get a reference to an order by ID
    pub fn get_order(&self, order_id: ManufacturingOrderId) -> Option<&ManufacturingOrder> {
        self.active_orders.iter().find(|o| o.id == order_id)
    }

    /// Get a mutable reference to an order by ID
    pub fn get_order_mut(&mut self, order_id: ManufacturingOrderId) -> Option<&mut ManufacturingOrder> {
        self.active_orders.iter_mut().find(|o| o.id == order_id)
    }

    /// Add a completed engine to inventory
    pub fn add_engine_to_inventory(
        &mut self,
        engine_design_id: usize,
        revision_number: u32,
        snapshot: EngineDesignSnapshot,
    ) {
        // Try to find an existing entry with the same design+revision
        if let Some(entry) = self.engine_inventory.iter_mut().find(|e| {
            e.engine_design_id == engine_design_id && e.revision_number == revision_number
        }) {
            entry.quantity += 1;
        } else {
            self.engine_inventory.push(EngineInventoryEntry {
                engine_design_id,
                revision_number,
                snapshot,
                quantity: 1,
            });
        }

        // Update production history
        if let Some(entry) = self.engine_production_history.iter_mut().find(|(id, _)| *id == engine_design_id) {
            entry.1 += 1;
        } else {
            self.engine_production_history.push((engine_design_id, 1));
        }
    }

    /// Add a completed rocket to inventory
    pub fn add_rocket_to_inventory(
        &mut self,
        rocket_design_id: usize,
        revision_number: u32,
        design_snapshot: RocketDesign,
    ) {
        let serial = self.next_serial_number;
        self.next_serial_number += 1;

        self.rocket_inventory.push(RocketInventoryEntry {
            rocket_design_id,
            revision_number,
            design_snapshot,
            serial_number: serial,
        });

        // Update production history
        if let Some(entry) = self.rocket_production_history.iter_mut().find(|(id, _)| *id == rocket_design_id) {
            entry.1 += 1;
        } else {
            self.rocket_production_history.push((rocket_design_id, 1));
        }
    }

    /// Consume engines from inventory for rocket assembly.
    /// Returns true if all required engines were available and consumed.
    pub fn consume_engines_for_rocket(&mut self, design: &RocketDesign) -> bool {
        // First, check that all required engines are available
        let required = engines_required(design);
        for (engine_design_id, count) in &required {
            let available = self.engine_inventory.iter()
                .filter(|e| e.engine_design_id == *engine_design_id)
                .map(|e| e.quantity)
                .sum::<u32>();
            if available < *count {
                return false;
            }
        }

        // Consume engines (prefer oldest revision first)
        for (engine_design_id, mut remaining) in required {
            for entry in self.engine_inventory.iter_mut() {
                if entry.engine_design_id == engine_design_id && remaining > 0 {
                    let consume = remaining.min(entry.quantity);
                    entry.quantity -= consume;
                    remaining -= consume;
                }
            }
        }

        // Clean up empty entries
        self.engine_inventory.retain(|e| e.quantity > 0);

        true
    }

    /// Consume a rocket from inventory by serial number.
    /// Returns the consumed rocket if found.
    pub fn consume_rocket(&mut self, serial_number: u32) -> Option<RocketInventoryEntry> {
        if let Some(pos) = self.rocket_inventory.iter().position(|r| r.serial_number == serial_number) {
            Some(self.rocket_inventory.remove(pos))
        } else {
            None
        }
    }

    /// Get available engines for a specific design ID
    pub fn get_engines_available(&self, engine_design_id: usize) -> u32 {
        self.engine_inventory.iter()
            .filter(|e| e.engine_design_id == engine_design_id)
            .map(|e| e.quantity)
            .sum()
    }

    /// Check if all engines required by a rocket design are in inventory
    pub fn has_engines_for_rocket(&self, design: &RocketDesign) -> bool {
        let required = engines_required(design);
        for (engine_design_id, count) in &required {
            if self.get_engines_available(*engine_design_id) < *count {
                return false;
            }
        }
        true
    }

    /// Sum remaining engine units (quantity - completed) from active engine orders
    /// for a given design ID. Used to prevent double-ordering when multiple rockets
    /// are queued.
    pub fn engines_pending_for_design(&self, engine_design_id: usize) -> u32 {
        self.active_orders.iter()
            .filter_map(|o| match &o.order_type {
                ManufacturingOrderType::Engine {
                    engine_design_id: eid,
                    quantity,
                    completed,
                    ..
                } if *eid == engine_design_id => Some(quantity - completed),
                _ => None,
            })
            .sum()
    }

    /// Try to unblock rocket orders that are waiting for engines.
    /// Iterates in FIFO order. For each blocked order, checks if engines are
    /// available; if so, consumes them and sets `waiting_for_engines = false`.
    /// Returns list of unblocked order IDs.
    pub fn try_unblock_rocket_orders(&mut self) -> Vec<ManufacturingOrderId> {
        let mut unblocked = Vec::new();

        // Collect indices of blocked rocket orders (FIFO = order in vec)
        let blocked_indices: Vec<usize> = self.active_orders.iter()
            .enumerate()
            .filter(|(_, o)| o.waiting_for_engines)
            .map(|(i, _)| i)
            .collect();

        for idx in blocked_indices {
            let design = match &self.active_orders[idx].order_type {
                ManufacturingOrderType::Rocket { design_snapshot, .. } => design_snapshot.clone(),
                _ => continue,
            };

            if self.has_engines_for_rocket(&design) {
                self.consume_engines_for_rocket(&design);
                self.active_orders[idx].waiting_for_engines = false;
                unblocked.push(self.active_orders[idx].id);
            }
        }

        unblocked
    }
}

impl Default for Manufacturing {
    fn default() -> Self {
        Self::new()
    }
}

// ==========================================
// Cost and Work Calculation Functions
// ==========================================

/// Calculate engine material cost from a snapshot (delegates to resource BOMs)
pub fn engine_material_cost(snapshot: &EngineDesignSnapshot) -> f64 {
    crate::resources::engine_resource_cost(snapshot.fuel_type, snapshot.mass_kg)
}

/// Calculate engine build work (team-days) from a snapshot.
/// Build time varies by engine type (kerolox 120, hydrolox 180, solid 45 base days).
/// Complexity scales build time linearly (complexity / 6).
pub fn engine_build_work(snapshot: &EngineDesignSnapshot) -> f64 {
    crate::resources::engine_base_build_days(snapshot.fuel_type)
        * snapshot.scale.powf(ENGINE_BUILD_SCALE_EXPONENT)
        * crate::balance::complexity_build_multiplier(snapshot.complexity)
}

/// Calculate material cost for a stage (tanks + assembly hardware, no engines)
pub fn stage_material_cost(stage: &RocketStage) -> f64 {
    let fuel_type = stage.engine_snapshot().fuel_type;
    if stage.is_solid() {
        crate::resources::stage_assembly_cost()
    } else {
        let tank_cost =
            crate::resources::tank_resource_cost(fuel_type, stage.tank_material, stage.tank_mass_kg());
        tank_cost + crate::resources::stage_assembly_cost()
    }
}

/// Calculate total material cost for a rocket design (stages + integration, no engines)
pub fn rocket_material_cost(design: &RocketDesign) -> f64 {
    let stage_costs: f64 = design.stages.iter().map(|s| stage_material_cost(s)).sum();
    stage_costs + crate::resources::rocket_integration_cost()
}

/// Calculate assembly work for a single stage (team-days)
/// Base assembly days are scaled by tank material build time multiplier.
/// Extra-engine days are unaffected by material choice.
pub fn stage_assembly_work(stage: &RocketStage) -> f64 {
    let extra_engines = stage.engine_count.saturating_sub(1) as f64;
    STAGE_BASE_ASSEMBLY_DAYS * stage.tank_material.build_time_multiplier()
        + (extra_engines * ASSEMBLY_DAYS_PER_EXTRA_ENGINE)
}

/// Calculate total assembly work for a rocket design (team-days)
pub fn rocket_assembly_work(design: &RocketDesign) -> f64 {
    let stage_work: f64 = design.stages.iter().map(|s| stage_assembly_work(s)).sum();
    stage_work + ROCKET_INTEGRATION_DAYS
}

/// Get the engines required by a rocket design as (engine_design_id, count) pairs
pub fn engines_required(design: &RocketDesign) -> Vec<(usize, u32)> {
    use std::collections::HashMap;
    let mut counts: HashMap<usize, u32> = HashMap::new();
    for stage in &design.stages {
        *counts.entry(stage.engine_design_id).or_insert(0) += stage.engine_count;
    }
    let mut result: Vec<_> = counts.into_iter().collect();
    result.sort_by_key(|(id, _)| *id);
    result
}

/// Calculate manufacturing team efficiency using the manufacturing exponent.
/// More parallelizable than design work (n^0.85 vs n^0.75).
pub fn manufacturing_team_efficiency(team_count: usize) -> f64 {
    if team_count == 0 {
        0.0
    } else {
        (team_count as f64).powf(MANUFACTURING_TEAM_EXPONENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_design::default_snapshot;

    fn kerolox_snapshot() -> EngineDesignSnapshot {
        default_snapshot(1) // Kerolox: $10M, 450kg, 500kN, scale=1.0
    }

    fn hydrolox_snapshot() -> EngineDesignSnapshot {
        default_snapshot(0) // Hydrolox: $15M, 300kg, 100kN, scale=1.0
    }

    // ==========================================
    // Floor Space Calculation Tests
    // ==========================================

    #[test]
    fn test_floor_space_for_engine() {
        assert_eq!(floor_space_for_engine(1.0), 1);
        assert_eq!(floor_space_for_engine(1.5), 2);
        assert_eq!(floor_space_for_engine(2.0), 2);
        assert_eq!(floor_space_for_engine(3.7), 4);
        assert_eq!(floor_space_for_engine(4.0), 4);
    }

    #[test]
    fn test_floor_space_for_rocket() {
        let design = RocketDesign::default_design();
        let space = floor_space_for_rocket(&design);
        // Default: 2 stages * 2 = 4, + 5 kerolox + 1 hydrolox = 6
        // Total = 10
        assert_eq!(space, 10);
    }

    // ==========================================
    // Floor Space Tracking Tests
    // ==========================================

    #[test]
    fn test_floor_space_tracking() {
        let mfg = Manufacturing::new();
        assert_eq!(mfg.floor_space_total, STARTING_FLOOR_SPACE);
        assert_eq!(mfg.floor_space_in_use(), 0);
        assert_eq!(mfg.floor_space_available(), STARTING_FLOOR_SPACE);
    }

    #[test]
    fn test_floor_space_construction() {
        let mut mfg = Manufacturing::new();
        let initial = mfg.floor_space_total;

        mfg.buy_floor_space(5);
        assert_eq!(mfg.floor_space_constructing(), 5);
        assert_eq!(mfg.floor_space_total, initial); // Not yet added

        // Tick 29 days — still under construction
        for _ in 0..29 {
            let completed = mfg.process_construction();
            assert_eq!(completed, 0);
        }
        assert_eq!(mfg.floor_space_total, initial);

        // Day 30 — construction completes
        let completed = mfg.process_construction();
        assert_eq!(completed, 5);
        assert_eq!(mfg.floor_space_total, initial + 5);
        assert_eq!(mfg.floor_space_constructing(), 0);
    }

    // ==========================================
    // Cost Calculation Tests
    // ==========================================

    #[test]
    fn test_engine_material_cost() {
        let snap = kerolox_snapshot();
        // Kerolox 450 kg → ~$158K (resource BOMs)
        let cost = engine_material_cost(&snap);
        assert!(cost > 150_000.0 && cost < 165_000.0,
            "Kerolox material cost should be ~$158K, got ${:.0}", cost);

        let snap = hydrolox_snapshot();
        // Hydrolox Expander 270 kg (300 * 0.9 cycle mass) → ~$112K
        let cost = engine_material_cost(&snap);
        assert!(cost > 108_000.0 && cost < 116_000.0,
            "Hydrolox material cost should be ~$112K, got ${:.0}", cost);
    }

    #[test]
    fn test_engine_build_work() {
        let snap = kerolox_snapshot();
        // Kerolox GasGenerator at scale 1.0, complexity=6: 120 * 1.0^0.75 * 6/6 = 120 days
        let work = engine_build_work(&snap);
        assert!((work - 120.0).abs() < 0.1,
            "Kerolox GasGenerator build work should be 120 days, got {}", work);

        let snap = hydrolox_snapshot();
        // Hydrolox Expander at scale 1.0, complexity=7: 180 * 1.0^0.75 * 7/6 = 210 days
        let work = engine_build_work(&snap);
        assert!((work - 210.0).abs() < 0.1,
            "Hydrolox Expander build work should be 210 days, got {}", work);
    }

    #[test]
    fn test_rocket_material_cost() {
        let design = RocketDesign::default_design();
        let cost = rocket_material_cost(&design);
        // 2 stages (tanks + assembly) + integration ≈ $2.65M
        assert!(cost > 2_000_000.0 && cost < 3_500_000.0,
            "Rocket material cost should be ~$2.65M, got ${:.2}M", cost / 1_000_000.0);
    }

    #[test]
    fn test_rocket_assembly_work() {
        let design = RocketDesign::default_design();
        let work = rocket_assembly_work(&design);
        // Stage 1: 5 engines -> 60 + 4*5 = 80 days
        // Stage 2: 1 engine -> 60 + 0*5 = 60 days
        // Integration: 30 days
        // Total: 80 + 60 + 30 = 170 days
        assert!((work - 170.0).abs() < 0.1,
            "Assembly work should be 170 days, got {}", work);
    }

    #[test]
    fn test_engines_required() {
        let design = RocketDesign::default_design();
        let required = engines_required(&design);
        // Default: 5 Kerolox (id=1) + 1 Hydrolox (id=0)
        assert_eq!(required.len(), 2);
        // Sorted by ID
        assert_eq!(required[0], (0, 1)); // 1 Hydrolox
        assert_eq!(required[1], (1, 5)); // 5 Kerolox
    }

    // ==========================================
    // Manufacturing Team Efficiency Tests
    // ==========================================

    #[test]
    fn test_manufacturing_team_efficiency() {
        assert_eq!(manufacturing_team_efficiency(0), 0.0);
        assert_eq!(manufacturing_team_efficiency(1), 1.0);
        // 2^0.85 ~ 1.8025
        assert!((manufacturing_team_efficiency(2) - 1.8025).abs() < 0.01,
            "2 teams: expected ~1.80, got {}", manufacturing_team_efficiency(2));
        // 5^0.85 ~ 3.928
        assert!((manufacturing_team_efficiency(5) - 3.928).abs() < 0.01,
            "5 teams: expected ~3.93, got {}", manufacturing_team_efficiency(5));
    }

    #[test]
    fn test_manufacturing_more_efficient_than_design() {
        use crate::engineering_team::team_efficiency;
        for n in 2..=10 {
            let mfg = manufacturing_team_efficiency(n);
            let design = team_efficiency(n);
            assert!(mfg > design,
                "Manufacturing efficiency ({}) should exceed design efficiency ({}) for {} teams",
                mfg, design, n);
        }
    }

    // ==========================================
    // Manufacturing State Tests
    // ==========================================

    #[test]
    fn test_new_manufacturing() {
        let mfg = Manufacturing::new();
        assert_eq!(mfg.floor_space_total, STARTING_FLOOR_SPACE);
        assert!(mfg.active_orders.is_empty());
        assert!(mfg.engine_inventory.is_empty());
        assert!(mfg.rocket_inventory.is_empty());
    }

    #[test]
    fn test_start_engine_order() {
        let mut mfg = Manufacturing::new();

        let snap = kerolox_snapshot();
        let result = mfg.start_engine_order(1, 1, snap, 3);
        assert!(result.is_some());
        let (order_id, total_cost) = result.unwrap();
        assert_eq!(order_id, 1);
        // 3 × ~$158K = ~$474K (resource-based cost)
        let expected = crate::resources::engine_resource_cost(
            crate::engine_design::FuelType::Kerolox, 450.0) * 3.0;
        assert!((total_cost - expected).abs() < 100.0,
            "Total cost should be ~${:.0}, got ${:.0}", expected, total_cost);
        assert_eq!(mfg.active_orders.len(), 1);
        // Engine at scale 1.0 uses 1 floor space unit
        assert_eq!(mfg.floor_space_in_use(), 1);
    }

    #[test]
    fn test_cannot_start_order_insufficient_floor_space() {
        let mut mfg = Manufacturing::new();
        mfg.floor_space_total = 0; // No floor space

        let snap = kerolox_snapshot();
        let result = mfg.start_engine_order(1, 1, snap, 1);
        assert!(result.is_none());
    }

    #[test]
    fn test_cancel_order() {
        let mut mfg = Manufacturing::new();

        let snap = kerolox_snapshot();
        let (order_id, _) = mfg.start_engine_order(1, 1, snap, 1).unwrap();
        assert_eq!(mfg.active_orders.len(), 1);
        assert_eq!(mfg.floor_space_in_use(), 1);

        assert!(mfg.cancel_order(order_id));
        assert_eq!(mfg.active_orders.len(), 0);
        assert_eq!(mfg.floor_space_in_use(), 0); // Space freed
    }

    #[test]
    fn test_order_progress_tracking() {
        let mut mfg = Manufacturing::new();

        let snap = kerolox_snapshot();
        let (order_id, _) = mfg.start_engine_order(1, 1, snap, 3).unwrap();

        let order = mfg.get_order(order_id).unwrap();
        assert_eq!(order.progress_fraction(), 0.0);
        assert!(!order.is_unit_complete());
        assert!(!order.is_order_complete());

        // Simulate some work
        let order = mfg.get_order_mut(order_id).unwrap();
        order.progress = order.total_work;
        assert!(order.is_unit_complete());
        assert!(!order.is_order_complete()); // Still need 2 more units
    }

    #[test]
    fn test_engine_inventory() {
        let mut mfg = Manufacturing::new();
        let snap = kerolox_snapshot();

        // Add 3 engines of same type
        mfg.add_engine_to_inventory(1, 1, snap.clone());
        mfg.add_engine_to_inventory(1, 1, snap.clone());
        mfg.add_engine_to_inventory(1, 1, snap.clone());

        assert_eq!(mfg.engine_inventory.len(), 1); // Grouped
        assert_eq!(mfg.engine_inventory[0].quantity, 3);
        assert_eq!(mfg.get_engines_available(1), 3);

        // Different revision = different entry
        mfg.add_engine_to_inventory(1, 2, snap);
        assert_eq!(mfg.engine_inventory.len(), 2);
        assert_eq!(mfg.get_engines_available(1), 4); // 3 + 1
    }

    #[test]
    fn test_consume_engines_for_rocket() {
        let mut mfg = Manufacturing::new();
        let kerolox = kerolox_snapshot();
        let hydrolox = hydrolox_snapshot();

        // Stock inventory: 6 Kerolox, 2 Hydrolox
        for _ in 0..6 {
            mfg.add_engine_to_inventory(1, 1, kerolox.clone());
        }
        for _ in 0..2 {
            mfg.add_engine_to_inventory(0, 1, hydrolox.clone());
        }

        let design = RocketDesign::default_design();
        // Default needs: 5 Kerolox (id=1) + 1 Hydrolox (id=0)
        assert!(mfg.has_engines_for_rocket(&design));
        assert!(mfg.consume_engines_for_rocket(&design));

        // Should have 1 Kerolox and 1 Hydrolox left
        assert_eq!(mfg.get_engines_available(1), 1);
        assert_eq!(mfg.get_engines_available(0), 1);
    }

    #[test]
    fn test_cannot_consume_insufficient_engines() {
        let mut mfg = Manufacturing::new();
        let kerolox = kerolox_snapshot();

        // Only 2 Kerolox
        mfg.add_engine_to_inventory(1, 1, kerolox.clone());
        mfg.add_engine_to_inventory(1, 1, kerolox);

        let design = RocketDesign::default_design();
        // Needs 5 Kerolox
        assert!(!mfg.has_engines_for_rocket(&design));
        assert!(!mfg.consume_engines_for_rocket(&design));

        // Inventory unchanged
        assert_eq!(mfg.get_engines_available(1), 2);
    }

    #[test]
    fn test_rocket_inventory() {
        let mut mfg = Manufacturing::new();
        let design = RocketDesign::default_design();

        mfg.add_rocket_to_inventory(0, 1, design.clone());
        mfg.add_rocket_to_inventory(0, 1, design);

        assert_eq!(mfg.rocket_inventory.len(), 2);
        assert_eq!(mfg.rocket_inventory[0].serial_number, 1);
        assert_eq!(mfg.rocket_inventory[1].serial_number, 2);
    }

    #[test]
    fn test_consume_rocket() {
        let mut mfg = Manufacturing::new();
        let design = RocketDesign::default_design();

        mfg.add_rocket_to_inventory(0, 1, design);
        let serial = mfg.rocket_inventory[0].serial_number;

        let consumed = mfg.consume_rocket(serial);
        assert!(consumed.is_some());
        assert_eq!(consumed.unwrap().serial_number, serial);
        assert!(mfg.rocket_inventory.is_empty());
    }

    #[test]
    fn test_production_history_tracking() {
        let mut mfg = Manufacturing::new();
        let snap = kerolox_snapshot();

        mfg.add_engine_to_inventory(1, 1, snap.clone());
        mfg.add_engine_to_inventory(1, 1, snap.clone());
        mfg.add_engine_to_inventory(0, 1, hydrolox_snapshot());

        assert_eq!(mfg.engine_production_history.len(), 2);
        let kerolox_count = mfg.engine_production_history.iter()
            .find(|(id, _)| *id == 1)
            .map(|(_, c)| *c)
            .unwrap_or(0);
        assert_eq!(kerolox_count, 2);
    }

    #[test]
    fn test_stage_material_cost() {
        use crate::stage::RocketStage;

        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.propellant_mass_kg = 10200.0;
        // Tank mass: 10200 * 0.06 = 612 kg, tank BOM cost ~$32K
        // Stage assembly: ~$565K
        // Total: ~$597K
        let cost = stage_material_cost(&stage);
        assert!(cost > 590_000.0 && cost < 610_000.0,
            "Stage material cost should be ~$597K, got ${:.0}", cost);
    }

    #[test]
    fn test_stage_assembly_work() {
        use crate::stage::RocketStage;

        let mut stage = RocketStage::new(kerolox_snapshot());
        stage.engine_count = 5;

        let work = stage_assembly_work(&stage);
        // 60 + 4*5 = 80 days
        assert!((work - 80.0).abs() < 0.1,
            "Stage assembly work should be 80 days, got {}", work);
    }

    // ==========================================
    // Remaining Work Tests
    // ==========================================

    #[test]
    fn test_remaining_work_engine_partial_progress() {
        let mut mfg = Manufacturing::new();
        let snap = kerolox_snapshot();
        let (order_id, _) = mfg.start_engine_order(1, 1, snap, 5).unwrap();

        let order = mfg.get_order_mut(order_id).unwrap();
        let total_work = order.total_work;
        // 2 completed, 50% through current unit
        if let ManufacturingOrderType::Engine { completed, .. } = &mut order.order_type {
            *completed = 2;
        }
        order.progress = total_work * 0.5;

        let remaining = order.remaining_work();
        // Current unit: 0.5 * total_work remaining
        // Future units: 5 - 2 - 1 = 2 full units
        let expected = total_work * 0.5 + 2.0 * total_work;
        assert!((remaining - expected).abs() < 0.1,
            "Expected {:.1}, got {:.1}", expected, remaining);
    }

    #[test]
    fn test_remaining_work_rocket() {
        let mut mfg = Manufacturing::new();
        let design = RocketDesign::default_design();
        let (order_id, _) = mfg.start_rocket_order(0, 1, design, false).unwrap();

        let order = mfg.get_order_mut(order_id).unwrap();
        let total_work = order.total_work;
        order.progress = total_work * 0.3;

        let remaining = order.remaining_work();
        let expected = total_work * 0.7;
        assert!((remaining - expected).abs() < 0.1,
            "Expected {:.1}, got {:.1}", expected, remaining);
    }

    #[test]
    fn test_remaining_work_completed_order() {
        let mut mfg = Manufacturing::new();
        let snap = kerolox_snapshot();
        let (order_id, _) = mfg.start_engine_order(1, 1, snap, 3).unwrap();

        let order = mfg.get_order_mut(order_id).unwrap();
        let total_work = order.total_work;
        // All 3 completed
        if let ManufacturingOrderType::Engine { completed, .. } = &mut order.order_type {
            *completed = 3;
        }
        order.progress = total_work;

        let remaining = order.remaining_work();
        assert!((remaining - 0.0).abs() < 0.1,
            "Completed order remaining should be 0, got {:.1}", remaining);
    }

    // ==========================================
    // Waiting-for-Engines Tests
    // ==========================================

    #[test]
    fn test_engine_order_waiting_for_engines_defaults_false() {
        let mut mfg = Manufacturing::new();
        let snap = kerolox_snapshot();
        let (order_id, _) = mfg.start_engine_order(1, 1, snap, 1).unwrap();
        let order = mfg.get_order(order_id).unwrap();
        assert!(!order.waiting_for_engines);
    }

    #[test]
    fn test_rocket_order_waiting_for_engines() {
        let mut mfg = Manufacturing::new();
        mfg.floor_space_total = 30; // Enough for two rocket orders
        let design = RocketDesign::default_design();

        // Create with waiting = true
        let (order_id, _) = mfg.start_rocket_order(0, 1, design.clone(), true).unwrap();
        let order = mfg.get_order(order_id).unwrap();
        assert!(order.waiting_for_engines);

        // Create with waiting = false
        let (order_id2, _) = mfg.start_rocket_order(0, 1, design, false).unwrap();
        let order2 = mfg.get_order(order_id2).unwrap();
        assert!(!order2.waiting_for_engines);
    }

    #[test]
    fn test_engines_pending_for_design() {
        let mut mfg = Manufacturing::new();
        let snap = kerolox_snapshot();

        // Order 5 kerolox engines
        mfg.start_engine_order(1, 1, snap.clone(), 5).unwrap();
        assert_eq!(mfg.engines_pending_for_design(1), 5);
        assert_eq!(mfg.engines_pending_for_design(0), 0); // Different design

        // Order 3 more
        mfg.start_engine_order(1, 1, snap, 3).unwrap();
        assert_eq!(mfg.engines_pending_for_design(1), 8);

        // Simulate completing 2 in the first order
        if let ManufacturingOrderType::Engine { completed, .. } = &mut mfg.active_orders[0].order_type {
            *completed = 2;
        }
        assert_eq!(mfg.engines_pending_for_design(1), 6); // 3 + 3
    }

    #[test]
    fn test_try_unblock_rocket_orders_unblocks_when_engines_arrive() {
        let mut mfg = Manufacturing::new();
        let kerolox = kerolox_snapshot();
        let hydrolox = hydrolox_snapshot();
        let design = RocketDesign::default_design();

        // Create a blocked rocket order
        let (order_id, _) = mfg.start_rocket_order(0, 1, design.clone(), true).unwrap();
        assert!(mfg.get_order(order_id).unwrap().waiting_for_engines);

        // No engines yet — should not unblock
        let unblocked = mfg.try_unblock_rocket_orders();
        assert!(unblocked.is_empty());
        assert!(mfg.get_order(order_id).unwrap().waiting_for_engines);

        // Stock the required engines (5 kerolox + 1 hydrolox)
        for _ in 0..5 {
            mfg.add_engine_to_inventory(1, 1, kerolox.clone());
        }
        mfg.add_engine_to_inventory(0, 1, hydrolox.clone());

        // Now it should unblock and consume engines
        let unblocked = mfg.try_unblock_rocket_orders();
        assert_eq!(unblocked, vec![order_id]);
        assert!(!mfg.get_order(order_id).unwrap().waiting_for_engines);

        // Engines should be consumed
        assert_eq!(mfg.get_engines_available(1), 0);
        assert_eq!(mfg.get_engines_available(0), 0);
    }

    #[test]
    fn test_try_unblock_fifo_priority() {
        let mut mfg = Manufacturing::new();
        mfg.floor_space_total = 30; // Enough for two rocket orders
        let kerolox = kerolox_snapshot();
        let hydrolox = hydrolox_snapshot();
        let design = RocketDesign::default_design();

        // Create two blocked rocket orders
        let (order1, _) = mfg.start_rocket_order(0, 1, design.clone(), true).unwrap();
        let (order2, _) = mfg.start_rocket_order(0, 1, design.clone(), true).unwrap();

        // Stock enough engines for only one rocket (5 kerolox + 1 hydrolox)
        for _ in 0..5 {
            mfg.add_engine_to_inventory(1, 1, kerolox.clone());
        }
        mfg.add_engine_to_inventory(0, 1, hydrolox.clone());

        // First order should unblock (FIFO), second remains blocked
        let unblocked = mfg.try_unblock_rocket_orders();
        assert_eq!(unblocked, vec![order1]);
        assert!(!mfg.get_order(order1).unwrap().waiting_for_engines);
        assert!(mfg.get_order(order2).unwrap().waiting_for_engines);
    }

    // ==========================================
    // Cycle Effect on Build Time Tests
    // ==========================================

    #[test]
    fn test_engine_build_work_gas_generator_baseline() {
        // Kerolox GasGenerator complexity=6: 120 * 6/6 = 120
        let snap = kerolox_snapshot();
        assert_eq!(snap.cycle, crate::engine_design::EngineCycle::GasGenerator);
        assert_eq!(snap.complexity, 6);
        let work = engine_build_work(&snap);
        assert!((work - 120.0).abs() < 0.1,
            "GasGenerator build work should be 120 days, got {}", work);
    }

    #[test]
    fn test_engine_build_work_staged_combustion() {
        use crate::engine_design::{create_engine, FuelType};
        let mut design = create_engine(FuelType::Kerolox, 1.0);
        design.set_cycle(crate::engine_design::EngineCycle::StagedCombustion);
        let snap = design.snapshot(1, "Kerolox");
        assert_eq!(snap.complexity, 7);
        let work = engine_build_work(&snap);
        // 120 * 1.0^0.75 * 7/6 = 140
        assert!((work - 140.0).abs() < 0.1,
            "StagedCombustion build work should be 140 days, got {:.1}", work);
    }

    #[test]
    fn test_engine_build_work_pressure_fed() {
        use crate::engine_design::{create_engine, FuelType};
        let mut design = create_engine(FuelType::Kerolox, 1.0);
        design.set_cycle(crate::engine_design::EngineCycle::PressureFed);
        let snap = design.snapshot(1, "Kerolox");
        assert_eq!(snap.complexity, 4);
        let work = engine_build_work(&snap);
        // 120 * 1.0^0.75 * 4/6 = 80
        assert!((work - 80.0).abs() < 0.1,
            "PressureFed build work should be 80 days, got {:.1}", work);
    }

    // ==========================================
    // Tank Material Effect Tests
    // ==========================================

    #[test]
    fn test_composite_stage_assembly_work() {
        use crate::stage::RocketStage;
        use crate::resources::TankMaterial;

        let mut alu_stage = RocketStage::new(kerolox_snapshot());
        alu_stage.engine_count = 1;
        let alu_work = stage_assembly_work(&alu_stage);
        // Aluminium: 60 base + 0 extra = 60 days
        assert!((alu_work - 60.0).abs() < 0.1);

        let mut comp_stage = RocketStage::new(kerolox_snapshot());
        comp_stage.engine_count = 1;
        comp_stage.tank_material = TankMaterial::CarbonComposite;
        let comp_work = stage_assembly_work(&comp_stage);
        // Composite: 60 * 1.4 + 0 extra = 84 days
        assert!((comp_work - 84.0).abs() < 0.1,
            "Composite stage assembly should be 84 days, got {}", comp_work);
    }

    #[test]
    fn test_composite_tank_lighter_in_stage() {
        use crate::stage::RocketStage;
        use crate::resources::TankMaterial;

        let mut alu_stage = RocketStage::new(kerolox_snapshot());
        alu_stage.propellant_mass_kg = 10000.0;
        let alu_tank = alu_stage.tank_mass_kg();

        let mut comp_stage = RocketStage::new(kerolox_snapshot());
        comp_stage.propellant_mass_kg = 10000.0;
        comp_stage.tank_material = TankMaterial::CarbonComposite;
        let comp_tank = comp_stage.tank_mass_kg();

        assert!((comp_tank / alu_tank - 0.70).abs() < 0.01,
            "Composite tank should be 70% of aluminium: alu={:.1}, comp={:.1}", alu_tank, comp_tank);
    }

    // ==========================================
    // Increase Engine Order Quantity Tests
    // ==========================================

    #[test]
    fn test_increase_engine_order_quantity() {
        let mut mfg = Manufacturing::new();
        let snap = kerolox_snapshot();
        let (order_id, _) = mfg.start_engine_order(1, 1, snap, 3).unwrap();

        let cost_per_unit = mfg.get_order(order_id).unwrap().material_cost_per_unit;
        let additional_cost = mfg.increase_engine_order_quantity(order_id, 5).unwrap();
        assert!((additional_cost - cost_per_unit * 5.0).abs() < 0.01);

        // Verify quantity increased
        match &mfg.get_order(order_id).unwrap().order_type {
            ManufacturingOrderType::Engine { quantity, .. } => assert_eq!(*quantity, 8),
            _ => panic!("Expected engine order"),
        }
    }

    #[test]
    fn test_increase_rocket_order_returns_none() {
        let mut mfg = Manufacturing::new();
        let design = RocketDesign::default_design();
        let (order_id, _) = mfg.start_rocket_order(0, 1, design, false).unwrap();

        assert!(mfg.increase_engine_order_quantity(order_id, 2).is_none());
    }

    #[test]
    fn test_increase_nonexistent_order_returns_none() {
        let mut mfg = Manufacturing::new();
        assert!(mfg.increase_engine_order_quantity(999, 2).is_none());
    }
}
