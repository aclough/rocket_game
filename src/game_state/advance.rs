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

        // The date is *not* bumped here — the tick does today's work under
        // today's date and only rolls over at the very end. That ordering is
        // what makes "launch to LEO on the 1st arrives on the 1st" true: the
        // flight created while the clock read the 1st is resolved by a tick
        // still stamped the 1st. Bumping first would stamp it the 2nd.
        //
        // One consequence: the very first tick of a new game runs its body on
        // the start date (2001-01-01), so January gets its contracts on day
        // one instead of the game opening with an empty first month.

        // Daily R&D across the player's project lists. The tick is a
        // Company method so competitors can eventually run the same
        // loop; tech-deficiency resolution stays here (it needs the
        // world's technology table).
        let crate::company::ResearchTick {
            events: research_events,
            newly_designed_engines,
            tech_def_attempts,
            newly_designed_reactors,
            reactor_tech_def_attempts,
        } = self.player_company.tick_daily_research(
            &mut self.seed.contingent_rng, &self.balance,
        );
        self.emit_all(&mut events, research_events);

        // Tech-deficiency resolution needs the world's technology table,
        // so it lives on the game state (`tech_ops`) rather than in the
        // company's research tick. Engine attempts, engine completions,
        // reactor attempts, reactor completions: the order the RNG has
        // always been drawn in.
        self.resolve_tech_attempts::<crate::engine_project::EngineProject>(
            tech_def_attempts, &mut events,
        );
        self.apply_new_design_deficiencies::<crate::engine_project::EngineProject>(
            newly_designed_engines, &mut events,
        );
        self.resolve_tech_attempts::<crate::reactor_project::ReactorProject>(
            reactor_tech_def_attempts, &mut events,
        );
        self.apply_new_design_deficiencies::<crate::reactor_project::ReactorProject>(
            newly_designed_reactors, &mut events,
        );

        if self.date.is_first_of_month() {
            let evt = GameEvent::MonthStart;
            self.emit(&mut events, evt);

            // Deduct salaries
            let salary = self.player_company.monthly_salary_cost();
            if salary > 0.0 {
                self.player_company.money -= salary;
                // Track expense
                self.record_expense(salary);
                let evt = GameEvent::SalariesPaid { amount: salary };
                self.emit(&mut events, evt);

                if self.player_company.money < 0.0 {
                    let evt = GameEvent::InsufficientFunds {
                        shortfall: -self.player_company.money,
                    };
                    self.emit(&mut events, evt);
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
                    self.emit(&mut events, evt);
                    self.speed = GameSpeed::Paused;
                }
            }

            // Roll the geopolitical arc once a year. Runs before modifiers
            // expire so a state entered this January is applied before
            // anything gets a chance to look at the markets.
            if self.date.month == 1 {
                let year = self.date.year;
                if let Some(shift) = crate::geopolitics::advance_geopolitics(
                    &mut self.geopolitics, &self.seed, year,
                ) {
                    let geo_events = self.apply_geopolitical_shift(shift);
                    self.emit_all(&mut events, geo_events);
                    // A war is at least as worth stopping for as a recession.
                    self.speed = GameSpeed::Paused;
                }
            }

            // Expire market modifiers
            for market in &mut self.markets {
                market.expire_modifiers(self.date);
            }

            // Check seed-driven market events
            let market_events = self.check_market_events();
            if !market_events.is_empty() {
                self.speed = GameSpeed::Paused;
            }
            self.emit_all(&mut events, market_events);

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
                self.emit(&mut events, evt);
            }

            // Roll anchor-customer campaign announcements. Seeded per
            // month like contract generation, so identical runs get
            // identical programs.
            let campaign_query = format!("campaigns_{}_{}", self.date.year, self.date.month);
            let mut campaign_rng = self.seed.world_query(&campaign_query);

            // A customer with a program still putting missions out isn't
            // in the market for another one. Note this is "still issuing",
            // not "still flying": once the last mission has been handed
            // over, the market is free to announce again even though those
            // contracts are outstanding. Waiting for them to land instead
            // would hold the market shut through the whole tail of a
            // program — and would do it to whoever *lost* the block bid
            // just as hard, locking them out for the years a rival spends
            // flying it.
            let still_issuing: std::collections::HashSet<crate::contract::MarketId> =
                self.active_campaigns.iter()
                    .filter(|c| matches!(c.status, contract::CampaignStatus::Soliciting { .. })
                        || c.missions_issued < c.missions_total)
                    .map(|c| c.market_id)
                    .collect();
            // Program names are drawn from a small pool, so without this a
            // market can run two "Commercial Resupply Cycle A" programs at
            // once and issue two contracts of the same name and number.
            let mut taken_names: Vec<String> =
                self.active_campaigns.iter().map(|c| c.name.clone()).collect();

            let mut announced: Vec<contract::Campaign> = Vec::new();
            for arch in &self.balance.markets.archetypes {
                let Some(spec) = &arch.campaign else { continue };
                let Some(market) = self.markets.iter().find(|m| m.id == arch.template.id)
                else { continue };
                if !market.active {
                    continue;
                }
                // The roll happens either way and its result is dropped if
                // the market is busy, so blocking a program doesn't shift
                // the draws every other archetype sees this month.
                if let Some(campaign) = contract::spawn_campaign(
                    market, spec, &mut campaign_rng,
                    &mut self.next_campaign_id, self.date, econ_mod,
                    &taken_names,
                ) {
                    if still_issuing.contains(&campaign.market_id) {
                        continue;
                    }
                    taken_names.push(campaign.name.clone());
                    announced.push(campaign);
                }
            }
            for campaign in announced {
                let market_name = self.markets.iter()
                    .find(|m| m.id == campaign.market_id)
                    .map(|m| m.name.clone())
                    .unwrap_or_default();
                let bid_deadline = match campaign.status {
                    contract::CampaignStatus::Soliciting { bid_deadline, .. } => bid_deadline,
                    _ => self.date,
                };
                // Same capability rule as the bid-rule engine: a
                // Testing design that clears the payload with the
                // shared safety margin. Liftable programs stop the
                // clock — a block commitment is a decision, not a
                // ticker item.
                let dest = campaign.destination.clone();
                // Same capability rule the bid engine and the contract
                // colours use — a program worth pausing for is one you
                // could actually bid on.
                let liftable = self.player_company.rocket_projects.iter()
                    .any(|rp| self.project_can_serve(rp, &dest, campaign.payload_kg));
                if liftable {
                    self.speed = GameSpeed::Paused;
                }
                let evt = GameEvent::CampaignAnnounced {
                    program: campaign.name.clone(),
                    market_name,
                    missions: campaign.missions_total,
                    payload_kg: campaign.payload_kg,
                    destination: campaign.destination_display.clone(),
                    bid_deadline,
                    liftable,
                };
                self.emit(&mut events, evt);
                self.active_campaigns.push(campaign);
            }

            // Start new month in financials
            self.ensure_current_month_financials();
        }

        // Resolve campaign block bids whose window closed, then issue
        // due mission contracts (daily; intervals are day-grained, not
        // month-grained). Resolution runs first so a just-won program
        // issues its first mission the same day.
        self.resolve_campaign_bids(&mut events);
        self.issue_campaign_contracts(&mut events);

        // Standing bid rules place the player's automatic bids before
        // today's resolutions.
        self.run_bid_rules(&mut events);

        // Resolve sealed bids on solicitations whose window closed
        // (before delivery-deadline expiry: bid windows are shorter
        // than any delivery deadline, so awards happen first).
        self.resolve_bids(&mut events);

        // Expire contracts past deadline (player, then competitors'
        // overdue campaign missions — both feed the program clause).
        self.expire_contracts(&mut events);
        self.expire_competitor_campaign_missions(&mut events);

        // Fly competitors' awarded contracts that reached their
        // scheduled launch day (abstract launches — real inventory,
        // real reputation, no flight sim).
        self.process_competitor_launches(&mut events);

        // Track launch drought (yearly check)
        if self.date.is_first_of_year() {
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
                    rocket_name, design_id, build_cost, rocket_project_id, ..
                } => {
                    self.player_company.rocket_cost_history
                        .entry(design_id)
                        .or_default()
                        .push(build_cost);
                    // A rush job is over the moment its rocket exists — not
                    // when the project's whole queue drains, or a second
                    // build behind the urgent one would keep the teams
                    // hostage after the deadline was already met.
                    self.player_company.rush_projects.remove(&rocket_project_id);
                    GameEvent::RocketIntegrated { rocket_name }
                }
            };
            self.emit(&mut events, evt);
        }

        // Try to unblock manufacturing orders that now have prerequisites
        self.player_company.try_unblock_manufacturing_orders();

        // Auto-reorder rockets to maintain inventory targets
        let auto_events = self.player_company.auto_reorder_rockets(&self.balance);
        for evt in auto_events {
            self.emit(&mut events, evt);
        }

        // A rush job is over once its last order has left the queue —
        // clear before assigning, so the floor goes back to normal on the
        // same tick the rocket lands in inventory.
        self.player_company.clear_finished_rush_jobs();
        // Place manufacturing teams: round-robin normally, everything onto
        // the rush job when there is one.
        self.player_company.assign_manufacturing_teams();
        // Same for engineering teams — an idle engineer is pure burn.
        self.player_company.auto_assign_idle_engineering_teams();

        // Auto-revise projects whose testing turned up a flaw. Runs
        // after the day's research tick, so a flaw discovered today
        // starts its revision today. Opt-out per project: a revision
        // partially resets the production learning curve, which a
        // player mid-run may not want.
        self.auto_revise_projects(&mut events);

        // Competitors run the same manufacturing machinery daily.
        self.tick_competitors(&mut events);

        // Advance flights in transit
        let flight_events = self.advance_flights();
        self.emit_all(&mut events, flight_events);

        // Run the daily power balance on parked spacecraft too. Brownout
        // kills the spacecraft (loss of attitude/comms/etc — same lethal
        // outcome as a flight stranding). Anything aboard is lost with it.
        // No "had charge before" guard: a fuel-cell-only craft never has
        // battery charge yet still dies on the day its propellant runs
        // out, and removing-on-brownout is self-debouncing (the
        // spacecraft is gone after one event).
        let mut browned_out: Vec<usize> = Vec::new();
        for (i, sc) in self.spacecraft.iter_mut().enumerate() {
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
            self.emit(&mut events, evt);
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
            let mut sc_news = Vec::new();
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
                        sc_news.push(GameEvent::MidFlightFlawActivated {
                            rocket_name: sc.name.clone(),
                            flaw_description: rf.description.clone(),
                            consequence: rf.consequence.to_string(),
                        });
                        sc_flaw_discoveries.push((rf.project_id, rf.flaw_index));
                    }
                }
            }
            self.emit_all(&mut events, sc_news);
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
            self.emit(&mut events, evt);
        }
        if self.player_company.has_actionable_manufacturing_orders() {
            self.player_company.notified_manufacturing_idle = false;
        }

        // Today is done — roll over. Everything above ran under today's date.
        self.date = self.date.next_day();

        events
    }

    /// Start revisions on projects that have discovered flaws and are
    /// set to auto-revise. Only acts on projects in `Testing` — the
    /// `start_*_revision` calls enforce that themselves and return
    /// `None` otherwise, so this is a filter for clarity, not
    /// correctness.
    ///
    /// Deliberately opt-out rather than unconditional: a revision bumps
    /// the project's `revision`, which is stamped onto build orders and
    /// inventory items, so it partially resets the production learning
    /// curve. A player mid-production run may prefer to keep flying a
    /// known-flawed design.
    fn auto_revise_projects(&mut self, events: &mut Vec<GameEvent>) {
        let mut started: Vec<(String, usize)> = Vec::new();

        for i in 0..self.player_company.engine_projects.len() {
            let p = &self.player_company.engine_projects[i];
            if !p.auto_revise || p.discovered_flaw_count() == 0 { continue; }
            let name = p.design.name.clone();
            if let Some((fc, _)) = self.player_company.start_engine_revision(i) {
                started.push((name, fc));
            }
        }
        for i in 0..self.player_company.rocket_projects.len() {
            let p = &self.player_company.rocket_projects[i];
            if !p.auto_revise || p.discovered_flaw_count() == 0 { continue; }
            let name = p.design.name.clone();
            if let Some(fc) = self.player_company.start_rocket_revision(i) {
                started.push((name, fc));
            }
        }
        for i in 0..self.player_company.reactor_projects.len() {
            let p = &self.player_company.reactor_projects[i];
            if !p.auto_revise || p.discovered_flaw_count() == 0 { continue; }
            let name = p.design.name.clone();
            if let Some((fc, _, _)) = self.player_company.start_reactor_revision(i) {
                started.push((name, fc));
            }
        }

        for (project_name, flaw_count) in started {
            let evt = GameEvent::AutoRevisionStarted { project_name, flaw_count };
            self.emit(events, evt);
        }
    }
}
