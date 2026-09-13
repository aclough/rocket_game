//! Production: ordering rocket and engine builds, keeping the auto-build
//! targets stocked, unblocking queued stages as their engines arrive,
//! and contracting third-party engines.

use super::*;

/// What a stage build order is waiting on: which engine it needs, and how
/// many. `None` for anything that isn't a stage order, or whose rocket
/// project or engine has since gone away.
///
/// A free function taking slices rather than a `&self` method because both
/// callers iterate `manufacturing.orders` and need the lookup to borrow only
/// the other fields of `Company`.
pub(super) fn stage_engine_need(
    order_type: &crate::manufacturing::ManufacturingOrderType,
    rocket_projects: &[RocketProject],
    engine_projects: &[EngineProject],
    contracted_engines: &[ContractedEngine],
) -> Option<(EngineSource, u32)> {
    let crate::manufacturing::ManufacturingOrderType::Stage {
        rocket_project_id, group_index, stage_index, ..
    } = order_type else {
        return None;
    };
    let rp = rocket_projects.iter().find(|rp| rp.project_id == *rocket_project_id)?;
    let stage = rp.design.stage_groups.get(*group_index)?.get(*stage_index)?;
    let source = stage_engine_source(stage, engine_projects, contracted_engines)?;
    Some((source, stage.engine_count))
}

/// Which engine a stage's cloned `EngineDesign` came from.
///
/// A stage carries a *copy* of the design rather than a reference to its
/// project, so the only way back is to match on `EngineDesign::id`.
/// Shared by `stage_engine_need` (what a queued stage is waiting for) and
/// by retirement (whether anything the player still flies depends on an
/// engine they're about to retire) so the two can't disagree about what
/// "this stage uses that engine" means.
pub(super) fn stage_engine_source(
    stage: &crate::stage::Stage,
    engine_projects: &[EngineProject],
    contracted_engines: &[ContractedEngine],
) -> Option<EngineSource> {
    if let Some(ep) = engine_projects.iter().find(|ep| ep.design.id == stage.engine.id) {
        Some(EngineSource::PlayerDesign(ep.project_id))
    } else {
        let ce = contracted_engines.iter().find(|ce| ce.design.id == stage.engine.id)?;
        Some(EngineSource::Contracted(ce.id))
    }
}

impl Company {
    /// Set the auto-build inventory target for a rocket project
    /// (0 removes the target). The project must be in Testing.
    /// Returns false if the project doesn't exist or isn't Testing.
    pub fn set_auto_build_target(&mut self, project_id: RocketProjectId, target: u32) -> bool {
        let Some(project) = self.rocket_projects.iter().find(|p| p.project_id == project_id)
        else {
            return false;
        };
        if !matches!(project.status, crate::rocket_project::RocketDesignStatus::Testing { .. }) {
            return false;
        }
        if target == 0 {
            self.auto_build_targets.remove(&project_id);
        } else {
            self.auto_build_targets.insert(project_id, target);
        }
        true
    }

    /// Cycle the auto-build target for the rocket project at `index`:
    /// 0 → 1 → 2 → 3 → 0. Returns the new target, or None if the
    /// project doesn't exist or isn't in Testing.
    pub fn cycle_auto_build_target(&mut self, index: usize) -> Option<u32> {
        let project_id = self.rocket_projects.get(index)?.project_id;
        let current = self.auto_build_targets.get(&project_id).copied().unwrap_or(0);
        let next = if current >= 3 { 0 } else { current + 1 };
        if self.set_auto_build_target(project_id, next) {
            Some(next)
        } else {
            None
        }
    }

    /// Engines of `source` that are already paid for and still unspoken for:
    /// everything sitting in inventory, plus everything on the manufacturing
    /// line, less what blocked stage orders will claim the moment their
    /// prerequisites are met.
    ///
    /// A stage order that is *not* blocked has already taken its engines out
    /// of inventory (see `try_unblock_manufacturing_orders`), so only the
    /// blocked ones represent a future claim.
    pub fn uncommitted_engines(&self, source: EngineSource) -> usize {
        let in_stock = self.manufacturing.inventory.engine_count(source);
        let on_the_line = self.manufacturing.orders.iter()
            .filter(|o| matches!(
                &o.order_type,
                crate::manufacturing::ManufacturingOrderType::Engine { source: s, .. }
                    if *s == source
            ))
            .count();

        let claimed: usize = self.manufacturing.orders.iter()
            .filter(|o| o.waiting_for_prerequisites)
            .filter_map(|o| match &o.order_type {
                crate::manufacturing::ManufacturingOrderType::Stage {
                    rocket_project_id, group_index, stage_index, ..
                } => Some((rocket_project_id, group_index, stage_index)),
                _ => None,
            })
            .filter_map(|(rp_id, gi, si)| {
                let rp = self.rocket_projects.iter()
                    .find(|rp| rp.project_id == *rp_id)?;
                let stage = rp.design.stage_groups.get(*gi)?.get(*si)?;
                (self.engine_source_for_id(stage.engine.id)? == source)
                    .then_some(stage.engine_count as usize)
            })
            .sum();

        (in_stock + on_the_line).saturating_sub(claimed)
    }

    /// Order construction of a rocket. Auto-queues engine, stage, and integration orders.
    /// Returns the total material cost and event, or None if the rocket project isn't complete.
    pub fn order_rocket_build(&mut self, rocket_project_index: usize, balance_cfg: &BalanceConfig) -> Option<(f64, GameEvent)> {
        if rocket_project_index >= self.rocket_projects.len() {
            return None;
        }
        let rp = &self.rocket_projects[rocket_project_index];
        if !matches!(rp.status, crate::rocket_project::RocketDesignStatus::Testing { .. }) {
            return None;
        }
        // The single gate every build goes through — the [O] key, the
        // auto-build sweep, and the sim policy alike — so a retired
        // design can't be built by any route.
        if rp.retired {
            return None;
        }

        let rocket_name = rp.design.name.clone();
        let rocket_project_id = rp.project_id;
        let design_id = rp.design.id;
        let mut total_cost = 0.0;

        // Get current build count for this rocket design (for learning curve)
        let rocket_prior = *self.rocket_build_counts.get(&design_id).unwrap_or(&0);

        // Engines the design needs that are already paid for — in stock or on
        // the line — and not promised to some other blocked stage order. An
        // engine the player ordered by hand from the Engines pane is the same
        // engine this rocket needs, so a build tops the stock up rather than
        // duplicating it. Snapshotted before the loop because the loop pushes
        // orders and (for contracted engines) inventory as it goes.
        let mut spare: HashMap<EngineSource, usize> = HashMap::new();
        for group in &rp.design.stage_groups {
            for stage in group {
                if let Some(source) = self.engine_source_for_id(stage.engine.id) {
                    spare.entry(source)
                        .or_insert_with(|| self.uncommitted_engines(source));
                }
            }
        }

        // Queue engine build orders for each engine needed
        for (gi, group) in rp.design.stage_groups.iter().enumerate() {
            for (si, stage) in group.iter().enumerate() {
                let source = self.engine_source_for_id(stage.engine.id);
                // Draw on the existing pool first; only build the shortfall.
                let from_stock = match source {
                    Some(s) => {
                        let pool = spare.get_mut(&s).expect("prefilled above");
                        let take = (*pool).min(stage.engine_count as usize);
                        *pool -= take;
                        take
                    }
                    None => 0,
                };
                for _e in from_stock..stage.engine_count as usize {
                    match source {
                        Some(EngineSource::PlayerDesign(ep_id)) => {
                            // Find the engine project for manufacturing details
                            if let Some(ep) = self.engine_projects.iter()
                                .find(|ep| ep.project_id == ep_id)
                            {
                                let engine_prior = *self.engine_build_counts.get(&ep_id).unwrap_or(&0);
                                let order_id = self.manufacturing.next_order_id.mint();
                                let order = ManufacturingOrder::new_engine(
                                    order_id,
                                    EngineSource::PlayerDesign(ep_id),
                                    stage.engine.id,
                                    stage.engine.name.clone(),
                                    stage.engine.mass_kg,
                                    ep.complexity,
                                    ep.spec.preset,
                                    ep.design.cycle,
                                    engine_prior,
                                    ep.revision,
                                    ep.flaws.clone(),
                                    ep.improvements.iter().filter(|i| i.actualized).cloned().collect(),
                                    balance_cfg,
                                ).for_rocket(rocket_project_id);
                                total_cost += order.material_cost;
                                self.manufacturing.orders.push(order);
                                *self.engine_build_counts.entry(ep_id).or_insert(0) += 1;
                            }
                        }
                        Some(EngineSource::Contracted(ce_id)) => {
                            // Contracted engine: charge per-unit cost, instant delivery
                            if let Some(ce) = self.contracted_engines.iter()
                                .find(|ce| ce.id == ce_id)
                            {
                                total_cost += ce.purchase_cost_per_unit;
                                let item_id = self.manufacturing.next_inventory_id.mint();
                                self.manufacturing.inventory.engines.push(InventoryEngine {
                                    item_id,
                                    source: EngineSource::Contracted(ce_id),
                                    engine_id: stage.engine.id,
                                    engine_name: stage.engine.name.clone(),
                                    build_cost: ce.purchase_cost_per_unit,
                                    revision: 0,
                                    flaws: ce.flaws.clone(),
                                    improvements: Vec::new(),
                                });
                                *self.contracted_engine_build_counts.entry(ce_id).or_insert(0) += 1;
                            }
                        }
                        None => {}
                    }
                }

                // Queue stage build order
                let order_id = self.manufacturing.next_order_id.mint();
                let stage_label = if group.len() == 1 {
                    format!("{}", gi + 1)
                } else {
                    let suffix = (b'a' + si as u8) as char;
                    format!("{}{}", gi + 1, suffix)
                };
                let stage_name = format!("{} S{}", rocket_name, stage_label);
                let order = ManufacturingOrder::new_stage(
                    order_id,
                    rocket_project_id,
                    gi, si,
                    stage_name,
                    stage.structural_mass_kg,
                    rocket_prior,
                    balance_cfg,
                );
                total_cost += order.material_cost;
                self.manufacturing.orders.push(order);
            }
        }

        // Queue integration order
        let total_stages: u32 = rp.design.stage_groups.iter()
            .map(|g| g.len() as u32)
            .sum();
        let order_id = self.manufacturing.next_order_id.mint();
        let integration_order = ManufacturingOrder::new_integration(
            order_id,
            rocket_project_id,
            design_id,
            rocket_name.clone(),
            total_stages,
            rocket_prior,
            rp.revision,
            rp.flaws.clone(),
            balance_cfg,
        );
        total_cost += integration_order.material_cost;
        self.manufacturing.orders.push(integration_order);

        // Increment rocket build count
        *self.rocket_build_counts.entry(design_id).or_insert(0) += 1;

        // Note: rocket_cost_history is populated at integration completion
        // (see advance_day) so the recorded marginal cost includes labor
        // accrued during manufacturing, not just material cost.

        self.debit(total_cost);

        // Reset idle notification since new orders were placed
        self.notified_manufacturing_idle = false;

        Some((total_cost, GameEvent::RocketBuildOrdered {
            rocket_name,
            total_cost,
        }))
    }

    /// Order a standalone engine build for a player-designed engine project.
    pub fn order_engine_build(&mut self, engine_project_index: usize, balance_cfg: &BalanceConfig) -> Option<(f64, GameEvent)> {
        if engine_project_index >= self.engine_projects.len() {
            return None;
        }
        let ep = &self.engine_projects[engine_project_index];
        if !matches!(ep.status, crate::engine_project::EngineDesignStatus::Testing { .. }) {
            return None;
        }
        if ep.retired {
            return None;
        }

        let engine_name = ep.design.name.clone();
        let ep_id = ep.project_id;
        let engine_id = ep.design.id;
        let mass_kg = ep.design.mass_kg;
        let complexity = ep.complexity;
        let preset = ep.spec.preset;
        let cycle = ep.design.cycle;
        let revision = ep.revision;
        let flaws = ep.flaws.clone();
        let improvements: Vec<_> = ep.improvements.iter().filter(|i| i.actualized).cloned().collect();
        let engine_prior = *self.engine_build_counts.get(&ep_id).unwrap_or(&0);

        let order_id = self.manufacturing.next_order_id.mint();
        let order = ManufacturingOrder::new_engine(
            order_id,
            EngineSource::PlayerDesign(ep_id),
            engine_id,
            engine_name.clone(),
            mass_kg,
            complexity,
            preset,
            cycle,
            engine_prior,
            revision,
            flaws,
            improvements,
            balance_cfg,
        );
        let cost = order.material_cost;
        self.manufacturing.orders.push(order);
        *self.engine_build_counts.entry(ep_id).or_insert(0) += 1;
        // engine_cost_history is populated at engine-build completion so the
        // recorded cost includes labor in addition to materials.
        self.debit(cost);
        self.notified_manufacturing_idle = false;

        Some((cost, GameEvent::EngineBuildOrdered { engine_name }))
    }

    /// Automatically order rocket builds to maintain auto_build_targets inventory levels.
    pub(crate) fn auto_reorder_rockets(&mut self, balance_cfg: &BalanceConfig) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let targets: Vec<(RocketProjectId, u32)> = self.auto_build_targets.iter()
            .map(|(&pid, &count)| (pid, count))
            .collect();

        for (project_id, min_count) in targets {
            // Find the project index
            let index = match self.rocket_projects.iter().position(|rp| rp.project_id == project_id) {
                Some(i) => i,
                None => continue,
            };
            // Only auto-build for projects in Testing status
            if !matches!(self.rocket_projects[index].status, crate::rocket_project::RocketDesignStatus::Testing { .. }) {
                continue;
            }
            let current = self.manufacturing.inventory.rocket_count(project_id) as u32
                + self.manufacturing.pending_integration_orders(project_id);
            for _ in current..min_count {
                if let Some((_cost, evt)) = self.order_rocket_build(index, balance_cfg) {
                    events.push(evt);
                }
            }
        }
        events
    }

    /// Try to unblock stage and integration orders that have their prerequisites ready.
    pub fn try_unblock_manufacturing_orders(&mut self) {
        for order in &mut self.manufacturing.orders {
            if !order.waiting_for_prerequisites {
                continue;
            }
            match &order.order_type {
                crate::manufacturing::ManufacturingOrderType::Stage { .. } => {
                    // Stage needs engines for this stage
                    if let Some((source, needed)) = stage_engine_need(
                        &order.order_type, &self.rocket_projects,
                        &self.engine_projects, &self.contracted_engines,
                    ) {
                        let available = self.manufacturing.inventory.engine_count(source);
                        if available >= needed as usize {
                            order.waiting_for_prerequisites = false;
                            // Consume engines from inventory, rolling
                            // their full build_cost (material + labor)
                            // into this stage order's material_cost.
                            for _ in 0..needed {
                                if let Some(eng) = self.manufacturing.inventory.take_engine(source) {
                                    order.material_cost += eng.build_cost;
                                }
                            }
                        }
                    }
                }
                crate::manufacturing::ManufacturingOrderType::RocketIntegration {
                    rocket_project_id, ..
                } => {
                    // Integration needs all stages
                    if let Some(rp) = self.rocket_projects.iter()
                        .find(|rp| rp.project_id == *rocket_project_id)
                    {
                        let all_stages_ready = rp.design.stage_groups.iter().enumerate().all(|(gi, group)| {
                            group.iter().enumerate().all(|(si, _stage)| {
                                self.manufacturing.inventory.stage_count(*rocket_project_id, gi, si) >= 1
                            })
                        });
                        if all_stages_ready {
                            order.waiting_for_prerequisites = false;
                            // Consume stages from inventory, accumulating their build cost
                            for (gi, group) in rp.design.stage_groups.iter().enumerate() {
                                for (si, _stage) in group.iter().enumerate() {
                                    if let Some(stg) = self.manufacturing.inventory.take_stage(*rocket_project_id, gi, si) {
                                        order.material_cost += stg.build_cost;
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Contract a third-party engine from the catalog.
    /// No upfront cost — per-unit cost is charged when building rockets.
    pub fn contract_third_party(&mut self, catalog_index: usize, current_date: GameDate, seed: &GameSeed, balance_cfg: &BalanceConfig) -> Option<GameEvent> {
        if catalog_index >= self.third_party_catalog.len() {
            return None;
        }
        let entry = &self.third_party_catalog[catalog_index];
        if current_date < entry.available_from {
            return None;
        }

        let id = self.next_contracted_engine_id.mint();
        let name = entry.design.name.clone();

        let flaws = third_party::generate_third_party_flaws(
            entry.complexity,
            seed,
            &name,
            &mut self.next_flaw_id,
            &balance_cfg.flaws,
        );

        let contracted = ContractedEngine {
            id,
            design: entry.design.clone(),
            preset: entry.preset,
            purchase_cost_per_unit: entry.purchase_cost_per_unit,
            flaws,
            complexity: entry.complexity,
        };
        self.contracted_engines.push(contracted);
        Some(GameEvent::EngineContracted { engine_name: name })
    }

    /// Look up the EngineSource for an engine by its EngineId.
    pub fn engine_source_for_id(&self, engine_id: EngineId) -> Option<EngineSource> {
        // Check player engine projects first
        if let Some(ep) = self.engine_projects.iter()
            .find(|ep| ep.design.id == engine_id)
        {
            return Some(EngineSource::PlayerDesign(ep.project_id));
        }
        // Check contracted engines
        if let Some(ce) = self.contracted_engines.iter()
            .find(|ce| ce.design.id == engine_id)
        {
            return Some(EngineSource::Contracted(ce.id));
        }
        None
    }
}
