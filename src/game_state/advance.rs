//! The daily tick: `GameState::advance_day` — R&D work, monthly
//! economy/market/contract generation, bidding, manufacturing,
//! competitors, flights, and endurance rolls, in a fixed order
//! (determinism depends on it).


use crate::contract::{self};
use crate::engine_project::EngineSource;
use crate::event::GameEvent;
use crate::rocket_project::RocketProjectId;

use super::*;

impl GameState {
    /// Advance the game by one day. Returns events generated this tick.
    pub fn advance_day(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();

        self.date = self.date.next_day();

        // Daily R&D across the player's project lists. The tick is a
        // Company method so competitors can eventually run the same
        // loop; tech-deficiency resolution stays here (it needs the
        // world's technology table).
        let research = self.player_company.tick_daily_research(
            &mut self.seed.contingent_rng, &self.balance,
        );
        for evt in &research.events {
            self.event_log.push(self.date, evt.clone());
        }
        events.extend(research.events);
        let tech_def_attempts = research.tech_def_attempts;
        let newly_designed_engines = research.newly_designed_engines;
        let newly_designed_reactors = research.newly_designed_reactors;
        let reactor_tech_def_attempts = research.reactor_tech_def_attempts;

        // Process tech deficiency revision attempts
        for (pi, def_id) in tech_def_attempts {
            let project = &mut self.player_company.engine_projects[pi];
            let tech_id = match project.technology_id {
                Some(id) => id,
                None => continue,
            };
            if let Some(tech) = self.technologies.iter_mut().find(|t| t.id == tech_id) {
                if let Some(def) = tech.deficiencies.iter_mut().find(|d| d.id == def_id) {
                    let already_solved = def.solved;
                    let engine_name = project.design.name.clone();
                    let def_desc = format!("{}: {}", def.description, def.kind);

                    if crate::technology::attempt_solve(def, already_solved, &mut self.seed.contingent_rng) {
                        // Success — remove from engine and restore stats
                        project.tech_deficiency_ids.retain(|id| *id != def_id);
                        match &def.kind {
                            crate::technology::TechDeficiencyKind::IspPenalty(frac) => {
                                project.design.isp_s /= 1.0 - frac;
                            }
                            crate::technology::TechDeficiencyKind::MassPenalty(frac) => {
                                project.design.mass_kg /= 1.0 + frac;
                            }
                            crate::technology::TechDeficiencyKind::ThrustPenalty(frac) => {
                                project.design.thrust_n /= 1.0 - frac;
                            }
                            crate::technology::TechDeficiencyKind::ComplexityPenalty(n) => {
                                project.complexity = project.complexity.saturating_sub(*n);
                            }
                            // Engine techs never generate PowerPenalty
                            // (that's reactor-domain, handled separately).
                            crate::technology::TechDeficiencyKind::PowerPenalty(_) => {}
                        }
                        let evt = GameEvent::RevisionComplete { engine_name: engine_name.clone() };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    } else {
                        // Failed — report attempt count
                        let hint = crate::technology::failure_hint(def.total_attempts);
                        let msg = if let Some(h) = hint {
                            format!("Failed to resolve {}: {}. {}", engine_name, def_desc, h)
                        } else {
                            format!("Failed to resolve {} deficiency: {}", engine_name, def_desc)
                        };
                        let evt = GameEvent::FlawDiscovered {
                            engine_name,
                            flaw_description: msg,
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    }
                }
            }
        }

        // Apply tech deficiencies to newly completed engine designs
        for pi in newly_designed_engines {
            let project = &mut self.player_company.engine_projects[pi];
            if let Some(tech_id) = project.technology_id {
                if let Some(tech) = self.technologies.iter().find(|t| t.id == tech_id) {
                    let deficiency_ids: Vec<crate::technology::TechDeficiencyId> =
                        tech.deficiencies.iter().map(|d| d.id).collect();
                    // Apply stat penalties from unsolved deficiencies
                    for def in &tech.deficiencies {
                        match &def.kind {
                            crate::technology::TechDeficiencyKind::IspPenalty(frac) => {
                                project.design.isp_s *= 1.0 - frac;
                            }
                            crate::technology::TechDeficiencyKind::MassPenalty(frac) => {
                                project.design.mass_kg *= 1.0 + frac;
                            }
                            crate::technology::TechDeficiencyKind::ThrustPenalty(frac) => {
                                project.design.thrust_n *= 1.0 - frac;
                            }
                            crate::technology::TechDeficiencyKind::ComplexityPenalty(n) => {
                                project.complexity += n;
                            }
                            // Engine techs never generate PowerPenalty
                            // (that's reactor-domain, handled separately).
                            crate::technology::TechDeficiencyKind::PowerPenalty(_) => {}
                        }
                    }
                    project.tech_deficiency_ids = deficiency_ids;
                    let engine_name = project.design.name.clone();
                    let tech_name = tech.name.clone();
                    let desc: Vec<String> = tech.deficiencies.iter()
                        .map(|d| format!("{}: {}", d.description, d.kind))
                        .collect();
                    if !desc.is_empty() {
                        let evt = GameEvent::TechDeficienciesFound {
                            engine_name: engine_name.clone(),
                            tech_name: tech_name.clone(),
                            deficiencies: desc.join(", "),
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    }
                }
            }
        }

        // Process reactor tech-deficiency revision attempts (mirrors the
        // engine flow above, applied to reactor stats).
        for (pi, def_id) in reactor_tech_def_attempts {
            let project = &mut self.player_company.reactor_projects[pi];
            let tech_id = match project.technology_id {
                Some(id) => id,
                None => continue,
            };
            if let Some(tech) = self.technologies.iter_mut().find(|t| t.id == tech_id) {
                if let Some(def) = tech.deficiencies.iter_mut().find(|d| d.id == def_id) {
                    let already_solved = def.solved;
                    let reactor_name = project.design.name.clone();
                    let def_desc = format!("{}: {}", def.description, def.kind);

                    if crate::technology::attempt_solve(def, already_solved, &mut self.seed.contingent_rng) {
                        // Success — remove from reactor and restore stats.
                        project.tech_deficiency_ids.retain(|id| *id != def_id);
                        match &def.kind {
                            crate::technology::TechDeficiencyKind::PowerPenalty(frac) => {
                                project.design.steady_w /= 1.0 - frac;
                            }
                            crate::technology::TechDeficiencyKind::MassPenalty(frac) => {
                                project.design.reactor_mass_kg /= 1.0 + frac;
                                project.design.mass_kg =
                                    project.design.reactor_mass_kg + project.design.radiator.mass_kg;
                            }
                            crate::technology::TechDeficiencyKind::ComplexityPenalty(n) => {
                                project.complexity = project.complexity.saturating_sub(*n);
                            }
                            // Reactor techs never generate Isp/Thrust penalties.
                            crate::technology::TechDeficiencyKind::IspPenalty(_)
                            | crate::technology::TechDeficiencyKind::ThrustPenalty(_) => {}
                        }
                        let evt = GameEvent::ReactorRevisionComplete { reactor_name };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    } else {
                        // Failed — report attempt count.
                        let hint = crate::technology::failure_hint(def.total_attempts);
                        let msg = if let Some(h) = hint {
                            format!("Failed to resolve {}: {}. {}", reactor_name, def_desc, h)
                        } else {
                            format!("Failed to resolve {} deficiency: {}", reactor_name, def_desc)
                        };
                        let evt = GameEvent::ReactorFlawDiscovered {
                            reactor_name,
                            flaw_description: msg,
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    }
                }
            }
        }

        // Apply tech deficiencies to newly completed reactor designs
        // (Option 2 gating — deficiencies roll on every reactor project,
        // no create-time tech gate).
        for pi in newly_designed_reactors {
            let project = &mut self.player_company.reactor_projects[pi];
            if let Some(tech_id) = project.technology_id {
                if let Some(tech) = self.technologies.iter().find(|t| t.id == tech_id) {
                    let deficiency_ids: Vec<crate::technology::TechDeficiencyId> =
                        tech.deficiencies.iter().map(|d| d.id).collect();
                    for def in &tech.deficiencies {
                        match &def.kind {
                            crate::technology::TechDeficiencyKind::PowerPenalty(frac) => {
                                project.design.steady_w *= 1.0 - frac;
                            }
                            crate::technology::TechDeficiencyKind::MassPenalty(frac) => {
                                project.design.reactor_mass_kg *= 1.0 + frac;
                                project.design.mass_kg =
                                    project.design.reactor_mass_kg + project.design.radiator.mass_kg;
                            }
                            crate::technology::TechDeficiencyKind::ComplexityPenalty(n) => {
                                project.complexity += n;
                            }
                            // Reactor techs never generate Isp/Thrust penalties.
                            crate::technology::TechDeficiencyKind::IspPenalty(_)
                            | crate::technology::TechDeficiencyKind::ThrustPenalty(_) => {}
                        }
                    }
                    project.tech_deficiency_ids = deficiency_ids;
                    let reactor_name = project.design.name.clone();
                    let tech_name = tech.name.clone();
                    let desc: Vec<String> = tech.deficiencies.iter()
                        .map(|d| format!("{}: {}", d.description, d.kind))
                        .collect();
                    if !desc.is_empty() {
                        let evt = GameEvent::ReactorTechDeficienciesFound {
                            reactor_name,
                            tech_name,
                            deficiencies: desc.join(", "),
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    }
                }
            }
        }

        if self.date.is_first_of_month() {
            let evt = GameEvent::MonthStart;
            self.event_log.push(self.date, evt.clone());
            events.push(evt);

            // Deduct salaries
            let salary = self.player_company.monthly_salary_cost();
            if salary > 0.0 {
                self.player_company.money -= salary;
                // Track expense
                self.record_expense(salary);
                let evt = GameEvent::SalariesPaid { amount: salary };
                self.event_log.push(self.date, evt.clone());
                events.push(evt);

                if self.player_company.money < 0.0 {
                    let evt = GameEvent::InsufficientFunds {
                        shortfall: -self.player_company.money,
                    };
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                }
            }

            // Competitors pay the same salaries, silently.
            for comp in &mut self.competitors {
                let salary = comp.company.monthly_salary_cost();
                comp.company.money -= salary;
            }

            // Advance economy — check if current state has expired
            let prev_condition = self.economy.condition;
            if let Some(new_condition) = crate::economy::advance_economy(
                &mut self.economy, &self.seed, self.date,
            ) {
                // Only fire event if the condition actually changed
                if new_condition != prev_condition {
                    let evt = GameEvent::EconomicShift {
                        condition: new_condition.display_name().to_string(),
                        description: new_condition.flavor_text().to_string(),
                    };
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                    self.speed = GameSpeed::Paused;
                }
            }

            // Expire market modifiers
            for market in &mut self.markets {
                market.expire_modifiers(self.date);
            }

            // Check seed-driven market events
            let market_events = self.check_market_events();
            for evt in market_events {
                self.event_log.push(self.date, evt.clone());
                events.push(evt);
                self.speed = GameSpeed::Paused;
            }

            // Check yearly tech unlock rolls (on January)
            if self.date.month == 1 {
                self.check_tech_unlocks(&mut events);
            }

            // Generate monthly solicitations from all active markets.
            // No reputation gate (M3): visibility is universal, the
            // reputation question lives in award scoring.
            //
            // Each market draws from its own monthly stream, so one
            // market's volume can never shift another's draws — the
            // year-1 floor can't be starved by stream reshuffling,
            // and the additive-only property holds exactly.
            let econ_mod = self.economy.modifier;
            let mut generated = 0u32;
            for market in self.markets.iter_mut() {
                let query = format!(
                    "contracts_{}_{}_{}", self.date.year, self.date.month, market.id.0,
                );
                let mut rng = self.seed.world_query(&query);
                let cs = contract::generate_market_contracts(
                    market, &mut rng, &mut self.next_contract_id,
                    self.date, econ_mod, &self.balance.markets,
                );
                generated += cs.len() as u32;
                self.available_contracts.extend(cs);
            }
            if generated > 0 {
                // Sort by market ID so display order matches selection order
                self.available_contracts.sort_by_key(|c| c.market_id.0);
                let evt = GameEvent::ContractsRefreshed { count: generated };
                self.event_log.push(self.date, evt.clone());
                events.push(evt);
            }

            // Roll anchor-customer campaign announcements. Seeded per
            // month like contract generation, so identical runs get
            // identical programs.
            let campaign_query = format!("campaigns_{}_{}", self.date.year, self.date.month);
            let mut campaign_rng = self.seed.world_query(&campaign_query);
            let mut announced: Vec<contract::Campaign> = Vec::new();
            for arch in &self.balance.markets.archetypes {
                let Some(spec) = &arch.campaign else { continue };
                let Some(market) = self.markets.iter().find(|m| m.id == arch.template.id)
                else { continue };
                if !market.active {
                    continue;
                }
                if let Some(campaign) = contract::spawn_campaign(
                    market, spec, &mut campaign_rng,
                    &mut self.next_campaign_id, self.date, econ_mod,
                ) {
                    announced.push(campaign);
                }
            }
            for campaign in announced {
                let market_name = self.markets.iter()
                    .find(|m| m.id == campaign.market_id)
                    .map(|m| m.name.clone())
                    .unwrap_or_default();
                let evt = GameEvent::EconomicShift {
                    condition: format!("New Program: {}", campaign.name),
                    description: format!(
                        "{market_name}: {} flights of {:.0} kg to {}, block-buy pricing",
                        campaign.missions_total, campaign.payload_kg,
                        campaign.destination_display,
                    ),
                };
                self.event_log.push(self.date, evt.clone());
                events.push(evt);
                self.active_campaigns.push(campaign);
            }

            // Start new month in financials
            self.ensure_current_month_financials();
        }

        // Issue due campaign mission contracts (daily; intervals are
        // day-grained, not month-grained).
        self.issue_campaign_contracts(&mut events);

        // Standing bid rules place the player's automatic bids before
        // today's resolutions.
        self.run_bid_rules(&mut events);

        // Resolve sealed bids on solicitations whose window closed
        // (before delivery-deadline expiry: bid windows are shorter
        // than any delivery deadline, so awards happen first).
        self.resolve_bids(&mut events);

        // Expire contracts past deadline
        self.expire_contracts(&mut events);

        // Fly competitors' awarded contracts that reached their
        // scheduled launch day (abstract launches — real inventory,
        // real reputation, no flight sim).
        self.process_competitor_launches(&mut events);

        // Track launch drought (yearly check)
        if self.date.is_first_of_month() && self.date.month == 1 && self.date.day == 1 {
            if let Some(last) = self.player_company.last_launch_date {
                let days_since = last.days_until(&self.date);
                if days_since >= 365 {
                    self.player_company.reputation.on_year_without_launch(&self.balance.reputation);
                }
            } else if self.date != self.start_date {
                // Never launched and at least a year has passed
                let days_since_start = self.start_date.days_until(&self.date);
                if days_since_start >= 365 {
                    self.player_company.reputation.on_year_without_launch(&self.balance.reputation);
                }
            }
        }

        // Process manufacturing
        let mfg_events = self.player_company.manufacturing.advance_day(&self.balance.costs);
        for me in mfg_events {
            let evt = match me {
                crate::manufacturing::ManufacturingEvent::EngineBuilt {
                    engine_name, source, build_cost, ..
                } => {
                    // Only player-designed engines have a per-project history.
                    if let EngineSource::PlayerDesign(ep_id) = source {
                        self.player_company.engine_cost_history
                            .entry(ep_id)
                            .or_default()
                            .push(build_cost);
                    }
                    GameEvent::EngineBuilt { engine_name }
                }
                crate::manufacturing::ManufacturingEvent::StageBuilt { stage_name, .. } =>
                    GameEvent::StageBuilt { stage_name },
                crate::manufacturing::ManufacturingEvent::RocketIntegrated {
                    rocket_name, design_id, build_cost, ..
                } => {
                    self.player_company.rocket_cost_history
                        .entry(design_id)
                        .or_default()
                        .push(build_cost);
                    GameEvent::RocketIntegrated { rocket_name }
                }
                crate::manufacturing::ManufacturingEvent::FloorSpaceComplete { units } =>
                    GameEvent::FloorSpaceComplete { units },
            };
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }

        // Try to unblock manufacturing orders that now have prerequisites
        self.player_company.try_unblock_manufacturing_orders();

        // Auto-reorder rockets to maintain inventory targets
        let auto_events = self.player_company.auto_reorder_rockets(&self.balance);
        for evt in auto_events {
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }

        // Auto-assign idle manufacturing teams to least-staffed orders
        self.player_company.auto_assign_idle_manufacturing_teams();

        // Competitors run the same manufacturing machinery daily.
        self.tick_competitors(&mut events);

        // Advance flights in transit
        let flight_events = self.advance_flights();
        for evt in flight_events {
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }

        // Run the daily power balance on parked spacecraft too. Brownout
        // kills the spacecraft (loss of attitude/comms/etc — same lethal
        // outcome as a flight stranding). Anything aboard is lost with it.
        // No "had charge before" guard: a fuel-cell-only craft never has
        // battery charge yet still dies on the day its propellant runs
        // out, and removing-on-brownout is self-debouncing (the
        // spacecraft is gone after one event).
        let mut browned_out: Vec<usize> = Vec::new();
        for (i, sc) in self.spacecraft.iter_mut().enumerate() {
            if !sc.rocket.has_explicit_power(&sc.design) {
                continue;
            }
            let sun_au = crate::location::DELTA_V_MAP
                .location(&sc.location)
                .map_or(1.0, |l| l.sun_distance_au());
            let brownout = sc.rocket.run_daily_power_tick(&sc.design, sun_au);
            if brownout {
                browned_out.push(i);
            }
        }
        for &i in browned_out.iter().rev() {
            let sc = self.spacecraft.remove(i);
            let evt = GameEvent::PowerLost {
                rocket_name: sc.name,
                location: crate::contract::destination_display_name(&sc.location)
                    .to_string(),
            };
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }

        // Roll endurance flaws for parked spacecraft
        {
            use rand::Rng;
            use crate::flaw::FlawTrigger;
            // Snapshot PerDay flaws from rocket projects
            struct ScFlawRef {
                project_id: RocketProjectId,
                flaw_index: usize,
                daily_rate: f64,
                consequence: crate::flaw::FlawConsequence,
                description: String,
            }
            let mut sc_flaw_table: Vec<ScFlawRef> = Vec::new();
            for rp in &self.player_company.rocket_projects {
                for (fi, flaw) in rp.flaws.iter().enumerate() {
                    if flaw.trigger == FlawTrigger::PerDay {
                        sc_flaw_table.push(ScFlawRef {
                            project_id: rp.project_id,
                            flaw_index: fi,
                            daily_rate: flaw.daily_rate(),
                            consequence: flaw.consequence.clone(),
                            description: flaw.description.clone(),
                        });
                    }
                }
            }
            let mut sc_flaw_discoveries: Vec<(RocketProjectId, usize)> = Vec::new();
            for sc in &mut self.spacecraft {
                for rf in &sc_flaw_table {
                    if rf.project_id != sc.rocket_project_id {
                        continue;
                    }
                    if self.seed.contingent_rng.gen::<f64>() < rf.daily_rate {
                        // Pick a random attached stage
                        let attached: Vec<(usize, usize)> = sc.design.stage_groups.iter()
                            .enumerate()
                            .flat_map(|(gi, group)| {
                                let stage_states = &sc.rocket.stage_states;
                                group.iter().enumerate()
                                    .filter(move |(si, _)| {
                                        stage_states.get(gi)
                                            .and_then(|g| g.get(*si))
                                            .is_some_and(|ss| ss.attached)
                                    })
                                    .map(move |(si, _)| (gi, si))
                            })
                            .collect();
                        if attached.is_empty() { continue; }
                        let (gi, si) = attached[self.seed.contingent_rng.gen_range(0..attached.len())];
                        crate::launch::apply_consequence_to_stage(
                            &mut sc.design, &rf.consequence, gi, si,
                        );
                        let evt = GameEvent::MidFlightFlawActivated {
                            rocket_name: sc.name.clone(),
                            flaw_description: rf.description.clone(),
                            consequence: rf.consequence.to_string(),
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                        sc_flaw_discoveries.push((rf.project_id, rf.flaw_index));
                    }
                }
            }
            // Discover activated flaws on rocket projects
            for (project_id, flaw_index) in &sc_flaw_discoveries {
                if let Some(rp) = self.player_company.rocket_projects.iter_mut()
                    .find(|rp| rp.project_id == *project_id)
                {
                    if *flaw_index < rp.flaws.len() && !rp.flaws[*flaw_index].discovered {
                        rp.flaws[*flaw_index].discovered = true;
                    }
                }
            }
        }

        // Pause on transition to idle manufacturing
        if !self.player_company.manufacturing_teams.is_empty()
            && !self.player_company.has_actionable_manufacturing_orders()
            && !self.player_company.notified_manufacturing_idle
        {
            self.speed = GameSpeed::Paused;
            self.player_company.notified_manufacturing_idle = true;
            let evt = GameEvent::ManufacturingIdle;
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }
        if self.player_company.has_actionable_manufacturing_orders() {
            self.player_company.notified_manufacturing_idle = false;
        }

        events
    }
}
