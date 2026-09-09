//! Retiring a design.
//!
//! Retiring hides a design and stops any further work going into it.
//! It never deletes: manufacturing orders, inventory, spacecraft in
//! flight, launch history and the stages of other rocket designs all
//! hold project ids, and a `Vec::remove` would dangle every one of
//! them. See `retire_designs_plan.md`.

use super::*;
use super::production::stage_engine_source;

/// Why a design can't be retired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetireRefusal {
    /// No such project, or it is already retired.
    NotFound,
    /// An engine that non-retired rocket designs still fly. Retiring it
    /// would strand their stage orders waiting on an engine nothing will
    /// ever build again, so the rockets have to go first.
    EngineInUse { rockets: Vec<String> },
}

/// What retiring a design does, worked out before anything is mutated so
/// the confirmation prompt can describe the consequences and the same
/// decisions can then be applied verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RetirementEffects {
    pub design_name: String,
    /// Engineering teams that go back in the pool.
    pub teams_released: u32,
    /// Orders to drop from the queue. No refund — the materials were
    /// paid for when the order was placed.
    pub cancelled: Vec<crate::manufacturing::ManufacturingOrderId>,
    /// Engine orders tagged to a retiring rocket that survive because a
    /// design the player still flies needs that engine. Components are
    /// pooled, so cancelling these would destroy work a live build is
    /// about to eat; they lose their rocket tag and become ordinary
    /// pooled builds instead.
    pub reassigned_engine_orders: Vec<crate::manufacturing::ManufacturingOrderId>,
    /// Whether a standing auto-build target was switched off.
    pub auto_build_cleared: bool,
}

impl Company {
    /// Names of the non-retired rocket designs whose stages use `source`.
    ///
    /// `except` skips one project — the rocket currently being retired,
    /// which mustn't count as a reason to keep its own engines going.
    fn rockets_using_engine(
        &self, source: EngineSource, except: Option<RocketProjectId>,
    ) -> Vec<String> {
        self.rocket_projects.iter()
            .filter(|rp| !rp.retired && Some(rp.project_id) != except)
            .filter(|rp| rp.design.stage_groups.iter().flatten().any(|stage|
                stage_engine_source(stage, &self.engine_projects, &self.contracted_engines)
                    == Some(source)))
            .map(|rp| rp.design.name.clone())
            .collect()
    }

    /// Work out what retiring `target` would do, without doing it.
    ///
    /// The confirmation prompt shows this; `retire` then applies exactly
    /// these decisions, so what the player is told is what happens.
    pub fn retirement_plan(
        &self, target: ProjectRef,
    ) -> Result<RetirementEffects, RetireRefusal> {
        match target {
            ProjectRef::Reactor(id) => {
                let rp = self.reactor_projects.iter()
                    .find(|rp| rp.project_id == id && !rp.retired)
                    .ok_or(RetireRefusal::NotFound)?;
                // Nothing to cancel: reactors have no order type, and a
                // stage carries a cloned `ReactorDesign` rather than a
                // reference to the project.
                Ok(RetirementEffects {
                    design_name: rp.design.name.clone(),
                    teams_released: rp.teams_assigned,
                    ..Default::default()
                })
            }
            ProjectRef::Engine(id) => {
                let ep = self.engine_projects.iter()
                    .find(|ep| ep.project_id == id && !ep.retired)
                    .ok_or(RetireRefusal::NotFound)?;
                let source = EngineSource::PlayerDesign(id);
                let rockets = self.rockets_using_engine(source, None);
                if !rockets.is_empty() {
                    return Err(RetireRefusal::EngineInUse { rockets });
                }
                // Nothing live needs this engine any more, so every
                // outstanding build of it is work with no destination.
                let cancelled = self.manufacturing.orders.iter()
                    .filter(|o| matches!(&o.order_type,
                        ManufacturingOrderType::Engine { source: s, .. } if *s == source))
                    .map(|o| o.id)
                    .collect();
                Ok(RetirementEffects {
                    design_name: ep.design.name.clone(),
                    teams_released: ep.teams_assigned,
                    cancelled,
                    ..Default::default()
                })
            }
            ProjectRef::Rocket(id) => {
                let rp = self.rocket_projects.iter()
                    .find(|rp| rp.project_id == id && !rp.retired)
                    .ok_or(RetireRefusal::NotFound)?;
                let mut cancelled = Vec::new();
                let mut reassigned_engine_orders = Vec::new();
                for order in &self.manufacturing.orders {
                    match &order.order_type {
                        // Integration and stages are specific to this
                        // design and worthless without it.
                        ManufacturingOrderType::RocketIntegration {
                            rocket_project_id, ..
                        } if *rocket_project_id == id => cancelled.push(order.id),
                        ManufacturingOrderType::Stage {
                            rocket_project_id, ..
                        } if *rocket_project_id == id => cancelled.push(order.id),
                        // Engines are pooled. One ordered for this rocket
                        // is still worth finishing if any design the
                        // player still flies uses it.
                        ManufacturingOrderType::Engine {
                            source, rocket_project_id: Some(owner), ..
                        } if *owner == id => {
                            if self.rockets_using_engine(*source, Some(id)).is_empty() {
                                cancelled.push(order.id);
                            } else {
                                reassigned_engine_orders.push(order.id);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(RetirementEffects {
                    design_name: rp.design.name.clone(),
                    teams_released: rp.teams_assigned,
                    cancelled,
                    reassigned_engine_orders,
                    auto_build_cleared: self.auto_build_targets.contains_key(&id),
                })
            }
        }
    }

    /// Retire a design: hide it, release its teams, and stop further work
    /// going into it. Returns what it did, or why it wouldn't.
    pub fn retire(
        &mut self, target: ProjectRef,
    ) -> Result<RetirementEffects, RetireRefusal> {
        let effects = self.retirement_plan(target)?;

        match target {
            ProjectRef::Engine(id) => {
                if let Some(ep) = self.engine_projects.iter_mut()
                    .find(|ep| ep.project_id == id)
                {
                    ep.retired = true;
                    ep.teams_assigned = 0;
                }
            }
            ProjectRef::Reactor(id) => {
                if let Some(rp) = self.reactor_projects.iter_mut()
                    .find(|rp| rp.project_id == id)
                {
                    rp.retired = true;
                    rp.teams_assigned = 0;
                }
            }
            ProjectRef::Rocket(id) => {
                if let Some(rp) = self.rocket_projects.iter_mut()
                    .find(|rp| rp.project_id == id)
                {
                    rp.retired = true;
                    rp.teams_assigned = 0;
                }
                // Both would otherwise keep feeding the queue orders for
                // a design the player can no longer see.
                self.auto_build_targets.remove(&id);
                self.rush_projects.remove(&id);
            }
        }

        // Surviving engine orders lose their rocket tag: the rocket they
        // were placed for is gone, and the tag only drives rush-priority
        // inheritance, which would now point at nothing.
        for order in &mut self.manufacturing.orders {
            if effects.reassigned_engine_orders.contains(&order.id) {
                if let ManufacturingOrderType::Engine { rocket_project_id, .. } =
                    &mut order.order_type
                {
                    *rocket_project_id = None;
                }
            }
        }
        self.manufacturing.orders.retain(|o| !effects.cancelled.contains(&o.id));

        Ok(effects)
    }
}
