//! The manufacturing floor: which orders the teams work, in what order
//! the queue is shown, rush jobs, and moving teams between orders.

use super::*;
use super::production::stage_engine_need;

/// One row of the manufacturing queue as drawn: which order, how deep it
/// sits in the integration → stage → engine tree, and whether it is part
/// of a rush job.
///
/// The Manufacturing tab's selection indexes *this* list, so the draw side
/// and the key handlers have to agree on it — hence one function
/// ([`Company::manufacturing_display_order`]) rather than a sort in each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MfgRow {
    pub order_index: usize,
    pub depth: u8,
    pub rushed: bool,
}

impl Company {
    /// Add a manufacturing team to a manufacturing order. Returns true if successful.
    pub fn add_team_to_manufacturing_order(&mut self, order_index: usize) -> bool {
        let available = self.unassigned_manufacturing_team_count();
        self.manufacturing.add_team_to_order(order_index, available)
    }

    /// Remove a manufacturing team from a manufacturing order. Returns true if successful.
    pub fn remove_team_from_manufacturing_order(&mut self, order_index: usize) -> bool {
        self.manufacturing.remove_team_from_order(order_index)
    }

    /// Whether any manufacturing order is actionable (not waiting for prerequisites).
    pub fn has_actionable_manufacturing_orders(&self) -> bool {
        self.manufacturing.orders.iter().any(|o| !o.waiting_for_prerequisites)
    }

    /// The manufacturing queue arranged as a tree: each integration order,
    /// then the stages feeding it, then the engine builds feeding those.
    /// A rush job's subtree leads, as one contiguous block.
    ///
    /// **The nesting is an attribution, not a fact.** Orders record which
    /// rocket they were queued for, never which stage an engine is for —
    /// components pool by source and go to whichever stage unblocks first,
    /// which is what makes rush-job stealing work at all. So engines are
    /// matched to stages greedily, and two identical engines shown under
    /// different stages could swap without anything noticing. Read a row as
    /// "this is what the queue owes that rocket", not "this engine is
    /// bolted to that stage".
    ///
    /// The attribution is load-bearing rather than cosmetic, because it is
    /// also what bounds a rush:
    ///
    /// - **A rush is one build, not a project.** Orders name a rocket
    ///   *project*, so a second build of the same rocket is
    ///   indistinguishable from the one you rushed. Taking the rush
    ///   target's subtree — the first matching integration order and the
    ///   stages it claims — pins the rush to a single build and leaves a
    ///   later order of the same rocket in the ordinary queue.
    /// - **A rush takes the engines it needs and no more.** Every engine of
    ///   a needed source could satisfy a rushed stage, but rushing all of
    ///   them spreads the floor thinner and finishes the rush *later*.
    ///   Claiming exactly `engine_count` per blocked stage keeps the teams
    ///   concentrated on the ones that actually unblock it.
    /// - **A rush claims the most-advanced engines**, so it inherits
    ///   whatever is nearest done and the untouched builds fall through to
    ///   whoever was going to get them.
    ///
    /// Only *blocked* stages claim engines: an unblocked stage has already
    /// taken its engines out of inventory. Anything left over — standalone
    /// builds, or an order whose project has gone — sits at the top level
    /// in queue order.
    pub fn manufacturing_display_order(&self) -> Vec<MfgRow> {
        let orders = &self.manufacturing.orders;
        let mut placed = vec![false; orders.len()];
        let mut rows: Vec<MfgRow> = Vec::with_capacity(orders.len());

        // One integration order per rushed project — the earliest, which is
        // the build that has been going longest and so is nearest done.
        let rush_targets: Vec<usize> = {
            let mut seen: Vec<RocketProjectId> = Vec::new();
            orders.iter().enumerate()
                .filter(|(_, o)| matches!(
                    o.order_type, ManufacturingOrderType::RocketIntegration { .. }))
                .filter(|(_, o)| o.parent_rocket()
                    .is_some_and(|id| self.rush_projects.contains(&id)))
                .filter(|(_, o)| {
                    let id = o.parent_rocket().expect("filtered above");
                    if seen.contains(&id) { false } else { seen.push(id); true }
                })
                .map(|(i, _)| i)
                .collect()
        };

        // Parts already on the shelf, drawn down as orders claim them.
        // A slot filled from inventory wants no build order at all.
        let mut stock: HashMap<EngineSource, usize> = HashMap::new();
        let mut stage_stock: HashMap<(RocketProjectId, usize, usize), usize> = HashMap::new();
        let emit_subtree = |root: usize, rushed: bool,
                            placed: &mut Vec<bool>, rows: &mut Vec<MfgRow>,
                            stock: &mut HashMap<EngineSource, usize>,
                            stage_stock: &mut HashMap<(RocketProjectId, usize, usize), usize>| {
            let ManufacturingOrderType::RocketIntegration { rocket_project_id, .. } =
                &orders[root].order_type else { return };
            placed[root] = true;
            rows.push(MfgRow { order_index: root, depth: 0, rushed });

            // An integration that is no longer waiting has already taken its
            // stages out of inventory, so it needs nothing more and claims
            // nothing — otherwise it reaches across and adopts the *next*
            // build's stages, which are not feeding it and, under a rush,
            // pull teams off the integration that is the only thing left to
            // do. Same rule as blocked-stages-only for engines, one level up.
            if !orders[root].waiting_for_prerequisites {
                return;
            }

            // One build's worth: a build queues exactly one stage order per
            // (group, index), so a slot already filled means the next
            // matching order belongs to a *different* build of the same
            // rocket and must be left for its own integration.
            let mut slots: Vec<(usize, usize)> = Vec::new();
            for (j, stage) in orders.iter().enumerate() {
                if placed[j] {
                    continue;
                }
                let ManufacturingOrderType::Stage {
                    rocket_project_id: rp, group_index, stage_index, ..
                } = &stage.order_type else { continue };
                if rp != rocket_project_id {
                    continue;
                }
                if slots.contains(&(*group_index, *stage_index)) {
                    continue;
                }
                // A slot already filled from inventory needs no order. Left
                // unchecked, an integration still waiting on (say) S1 goes
                // on adopting every fresh S2 that appears, and under a rush
                // builds them one after another for a slot that was
                // satisfied long ago.
                let slot = (*rocket_project_id, *group_index, *stage_index);
                let on_shelf = stage_stock.entry(slot).or_insert_with(
                    || self.manufacturing.inventory
                        .stage_count(*rocket_project_id, *group_index, *stage_index));
                if *on_shelf > 0 {
                    *on_shelf -= 1;
                    slots.push((*group_index, *stage_index));
                    continue;
                }
                slots.push((*group_index, *stage_index));
                placed[j] = true;
                rows.push(MfgRow { order_index: j, depth: 1, rushed });

                if !stage.waiting_for_prerequisites {
                    continue; // already holds its engines
                }
                let Some((source, needs)) = stage_engine_need(
                    &stage.order_type, &self.rocket_projects,
                    &self.engine_projects, &self.contracted_engines,
                ) else { continue };

                // Only the *outstanding* engines belong under this stage.
                // A stage wanting six with four already on the shelf is two
                // builds away, not six, and claiming six would park the
                // floor on engines nobody is waiting for — and, under a
                // rush, spread the teams three times thinner than the two
                // that actually unblock it.
                //
                // Stock is contested between stages needing the same
                // engine, so it is drawn down in the order they are
                // walked. `try_unblock` won't part-consume — a stage takes
                // all its engines at once or none — so this is only ever an
                // estimate of who gets there first.
                let on_hand = stock.entry(source).or_insert_with(
                    || self.manufacturing.inventory.engine_count(source));
                let from_stock = (*on_hand).min(needs as usize);
                *on_hand -= from_stock;
                let wanted = needs as usize - from_stock;

                // Nearest-done first for a rush, so it inherits the work
                // already sunk; queue order otherwise, which keeps the
                // ordinary display stable.
                let mut candidates: Vec<usize> = orders.iter().enumerate()
                    .filter(|(k, _)| !placed[*k])
                    .filter(|(_, e)| matches!(
                        &e.order_type,
                        ManufacturingOrderType::Engine { source: es, .. } if *es == source))
                    .map(|(k, _)| k)
                    .collect();
                if rushed {
                    candidates.sort_by(|a, b| orders[*b].progress()
                        .partial_cmp(&orders[*a].progress())
                        .unwrap_or(std::cmp::Ordering::Equal));
                }
                for k in candidates.into_iter().take(wanted) {
                    placed[k] = true;
                    rows.push(MfgRow { order_index: k, depth: 2, rushed });
                }
            }
        };

        for root in rush_targets {
            emit_subtree(root, true, &mut placed, &mut rows, &mut stock, &mut stage_stock);
        }
        for i in 0..orders.len() {
            if !placed[i] {
                emit_subtree(i, false, &mut placed, &mut rows, &mut stock, &mut stage_stock);
            }
        }
        for (i, done) in placed.iter_mut().enumerate() {
            if !*done {
                *done = true;
                rows.push(MfgRow { order_index: i, depth: 0, rushed: false });
            }
        }
        rows
    }

    /// Which orders the rush is actually pulling teams onto — the rush
    /// subtrees from [`Self::manufacturing_display_order`], so what the
    /// floor does and what the pane draws can't drift apart.
    pub fn rushed_order_flags(&self) -> Vec<bool> {
        let mut flags = vec![false; self.manufacturing.orders.len()];
        for row in self.manufacturing_display_order() {
            flags[row.order_index] = row.rushed;
        }
        flags
    }

    /// Assign manufacturing teams across the actionable orders.
    ///
    /// With no rush job this is the plain "fewest teams wins" round-robin
    /// over every unblocked order, and only idle teams move.
    ///
    /// A rush job preempts: every team is pulled off ordinary work and
    /// split across the rush orders by the same round-robin, because a
    /// deadline means now and waiting for teams to come free could take
    /// weeks. Preempted teams are not remembered — when the rush ends they
    /// fall idle and this function places them again on the next tick.
    pub fn assign_manufacturing_teams(&mut self) {
        let flags = self.rushed_order_flags();
        let rushing: Vec<usize> = self.manufacturing.orders.iter().enumerate()
            .filter(|(_, o)| !o.waiting_for_prerequisites)
            .filter(|(i, _)| flags[*i])
            .map(|(i, _)| i)
            .collect();

        if !rushing.is_empty() {
            // Preempt: strip the floor, then hand it all to the rush.
            for order in &mut self.manufacturing.orders {
                order.teams_assigned = 0;
            }
        }

        loop {
            if self.unassigned_manufacturing_team_count() == 0 {
                break;
            }
            // Ties keep the earliest order, matching the long-standing
            // `min_by_key` behaviour.
            let best = self.manufacturing.orders.iter().enumerate()
                .filter(|(i, o)| {
                    !o.waiting_for_prerequisites
                        && (rushing.is_empty() || rushing.contains(i))
                })
                .min_by_key(|(_, o)| o.teams_assigned)
                .map(|(i, _)| i);
            match best {
                Some(idx) => {
                    let available = self.unassigned_manufacturing_team_count();
                    if !self.manufacturing.add_team_to_order(idx, available) {
                        break;
                    }
                }
                None => break,
            }
        }
    }

    /// Nominal days to build one of these from nothing, with one team on
    /// every part at once.
    ///
    /// This is the critical path, not the total work: engines for a stage
    /// build alongside each other, all stages build alongside each other,
    /// and integration waits for the last of them. One team means a work
    /// rate of exactly 1.0 (`n^0.85` at n=1), so work-days are days.
    ///
    /// Current learning-curve multipliers are folded in, so the number is
    /// what the *next* one would cost rather than what the first did.
    /// Contracted engines contribute nothing: they arrive on order.
    ///
    /// Deliberately ignores how many teams you actually have and anything
    /// already in stock — it is a property of the design, for
    /// comparing one against another.
    pub fn nominal_build_days(
        &self, project: &RocketProject, balance: &BalanceConfig,
    ) -> f64 {
        let rocket_prior = *self.rocket_build_counts
            .get(&project.design.id).unwrap_or(&0);
        let rocket_learning = balance.work.learning_curve_multiplier(rocket_prior);

        let mut critical_path = 0.0_f64;
        for group in &project.design.stage_groups {
            for stage in group {
                let engine_days = match self.engine_source_for_id(stage.engine.id) {
                    Some(EngineSource::PlayerDesign(ep_id)) => self.engine_projects.iter()
                        .find(|ep| ep.project_id == ep_id)
                        .map_or(0.0, |ep| {
                            let prior = *self.engine_build_counts
                                .get(&ep_id).unwrap_or(&0);
                            balance.work.engine_build_work(ep.complexity, stage.engine.mass_kg)
                                * balance.work.learning_curve_multiplier(prior)
                        }),
                    // Contracted engines are delivered when ordered.
                    Some(EngineSource::Contracted(_)) | None => 0.0,
                };
                let stage_days = balance.work.stage_build_work(stage.structural_mass_kg)
                    * rocket_learning;
                critical_path = critical_path.max(engine_days + stage_days);
            }
        }

        let total_stages: u32 = project.design.stage_groups.iter()
            .map(|g| g.len() as u32)
            .sum();
        critical_path
            + balance.work.rocket_integration_work(total_stages) * rocket_learning
    }

    /// Drop rush jobs with nothing left to rush.
    ///
    /// The usual retirement is on the `RocketIntegrated` event — the rush
    /// ends the moment its rocket exists. This is the backstop for a rush
    /// declared on a project whose queue empties some other way, so a
    /// stale entry can't sit there waiting to hijack the next build.
    pub fn clear_finished_rush_jobs(&mut self) {
        let inventory = &self.manufacturing;
        self.rush_projects.retain(|id| {
            inventory.orders.iter().any(|o| o.parent_rocket() == Some(*id))
        });
    }

    /// Steal a manufacturing team from the busiest order and assign to the target order.
    pub fn steal_manufacturing_team_to_order(&mut self, target: usize) -> Option<String> {
        if target >= self.manufacturing.orders.len() {
            return None;
        }
        if self.manufacturing.orders[target].waiting_for_prerequisites {
            return None;
        }
        // Find non-waiting order with most teams (>0, not target)
        let best = self.manufacturing.orders.iter().enumerate()
            .filter(|(i, o)| *i != target && !o.waiting_for_prerequisites && o.teams_assigned > 0)
            .max_by_key(|(_, o)| o.teams_assigned)
            .map(|(i, o)| (i, o.order_type.display_name()));

        let (idx, name) = best?;
        self.manufacturing.orders[idx].teams_assigned -= 1;
        self.manufacturing.orders[target].teams_assigned += 1;
        Some(name)
    }
}
