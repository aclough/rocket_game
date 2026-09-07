//! The daily tick: `GameState::advance_day` — R&D work, monthly
//! economy/market/contract generation, bidding, manufacturing,
//! competitors, flights, and endurance rolls, in a fixed order
//! (determinism depends on it).
//!
//! `advance_day` is the order; each phase is its own method below, so
//! the order can be read at a glance and a phase tested on its own.

use crate::contract::{self};
use crate::engine_project::EngineSource;
use crate::event::GameEvent;

use super::*;

impl GameState {
    /// Advance the game by one day. Returns events generated this tick.
    ///
    /// The date is *not* bumped first — the tick does today's work under
    /// today's date and only rolls over at the very end. That ordering is
    /// what makes "launch to LEO on the 1st arrives on the 1st" true: the
    /// flight created while the clock read the 1st is resolved by a tick
    /// still stamped the 1st. Bumping first would stamp it the 2nd.
    ///
    /// One consequence: the very first tick of a new game runs its body on
    /// the start date (2001-01-01), so January gets its contracts on day
    /// one instead of the game opening with an empty first month.
    ///
    /// The phases draw from the contingent RNG in this order; reordering
    /// them changes every seed's history.
    pub fn advance_day(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();

        self.tick_research(&mut events);
        if self.date.is_first_of_month() {
            self.tick_month_start(&mut events);
        }
        self.tick_market_pipeline(&mut events);
        if self.date.is_first_of_year() {
            self.tick_launch_drought();
        }
        self.tick_manufacturing(&mut events);
        self.auto_revise_projects(&mut events);
        self.tick_competitors(&mut events);
        let flight_events = self.advance_flights();
        self.emit_all(&mut events, flight_events);
        self.tick_parked_spacecraft(&mut events);
        self.tick_idle_manufacturing_pause(&mut events);

        // Today is done — roll over. Everything above ran under today's date.
        self.date = self.date.next_day();

        events
    }

    /// Daily R&D across the player's project lists, then the
    /// tech-deficiency outcomes it reported. The work tick is a
    /// `Company` method so competitors can eventually run the same
    /// loop; deficiency resolution needs the world's technology table,
    /// so it lives here (`tech_ops`). Engine attempts, engine
    /// completions, reactor attempts, reactor completions: the order
    /// the RNG has always been drawn in.
    fn tick_research(&mut self, events: &mut Vec<GameEvent>) {
        let crate::company::ResearchTick {
            events: research_events,
            newly_designed_engines,
            tech_def_attempts,
            newly_designed_reactors,
            reactor_tech_def_attempts,
        } = self.player_company.tick_daily_research(
            &mut self.seed.contingent_rng, &self.balance,
        );
        self.emit_all(events, research_events);

        self.resolve_tech_attempts::<crate::engine_project::EngineProject>(
            tech_def_attempts, events,
        );
        self.apply_new_design_deficiencies::<crate::engine_project::EngineProject>(
            newly_designed_engines, events,
        );
        self.resolve_tech_attempts::<crate::reactor_project::ReactorProject>(
            reactor_tech_def_attempts, events,
        );
        self.apply_new_design_deficiencies::<crate::reactor_project::ReactorProject>(
            newly_designed_reactors, events,
        );
    }

    /// Everything that happens on the 1st: salaries, the economy and
    /// (in January) the geopolitical arc, market modifiers and events,
    /// (in January) tech unlock rolls, this month's contracts, campaign
    /// announcements, and the new financial month.
    fn tick_month_start(&mut self, events: &mut Vec<GameEvent>) {
        self.emit(events, GameEvent::MonthStart);
        self.pay_salaries(events);
        self.tick_economy(events);
        if self.date.month == 1 {
            self.tick_geopolitics(events);
        }
        self.tick_markets(events);
        if self.date.month == 1 {
            self.check_tech_unlocks(events);
        }
        self.generate_monthly_contracts(events);
        self.announce_campaigns(events);
        self.ensure_current_month_financials();
    }

    /// Charge the month's salaries — the player's with an event and a
    /// debt warning, the competitors' silently.
    fn pay_salaries(&mut self, events: &mut Vec<GameEvent>) {
        let salary = self.player_company.monthly_salary_cost();
        if salary > 0.0 {
            self.player_company.money -= salary;
            self.record_expense(salary);
            self.emit(events, GameEvent::SalariesPaid { amount: salary });

            if self.player_company.money < 0.0 {
                let evt = GameEvent::InsufficientFunds {
                    shortfall: -self.player_company.money,
                };
                self.emit(events, evt);
            }
        }
        for comp in &mut self.competitors {
            let salary = comp.company.monthly_salary_cost();
            comp.company.money -= salary;
        }
    }

    /// Advance the economic condition; a change is news and stops the
    /// clock.
    fn tick_economy(&mut self, events: &mut Vec<GameEvent>) {
        let prev_condition = self.economy.condition;
        if let Some(new_condition) = crate::economy::advance_economy(
            &mut self.economy, &self.seed, self.date,
        ) {
            if new_condition != prev_condition {
                let evt = GameEvent::EconomicShift {
                    condition: new_condition.display_name().to_string(),
                    description: new_condition.flavor_text().to_string(),
                };
                self.emit(events, evt);
                self.speed = GameSpeed::Paused;
            }
        }
    }

    /// Roll the geopolitical arc for the year. Runs before market
    /// modifiers expire so a state entered this January is applied
    /// before anything gets a chance to look at the markets.
    fn tick_geopolitics(&mut self, events: &mut Vec<GameEvent>) {
        let year = self.date.year;
        if let Some(shift) = crate::geopolitics::advance_geopolitics(
            &mut self.geopolitics, &self.seed, year,
        ) {
            let geo_events = self.apply_geopolitical_shift(shift);
            self.emit_all(events, geo_events);
            // A war is at least as worth stopping for as a recession.
            self.speed = GameSpeed::Paused;
        }
    }

    /// Expire market modifiers, then fire any seed-driven market
    /// emergence due this month (which stops the clock).
    fn tick_markets(&mut self, events: &mut Vec<GameEvent>) {
        for market in &mut self.markets {
            market.expire_modifiers(self.date);
        }
        let market_events = self.check_market_events();
        if !market_events.is_empty() {
            self.speed = GameSpeed::Paused;
        }
        self.emit_all(events, market_events);
    }

    /// This month's solicitations from every active market. No
    /// reputation gate (M3): visibility is universal, the reputation
    /// question lives in award scoring.
    ///
    /// Each market draws from its own monthly stream, so one market's
    /// volume can never shift another's draws — the year-1 floor can't
    /// be starved by stream reshuffling, and the additive-only property
    /// holds exactly.
    fn generate_monthly_contracts(&mut self, events: &mut Vec<GameEvent>) {
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
            self.emit(events, GameEvent::ContractsRefreshed { count: generated });
        }
    }

    /// Roll anchor-customer campaign announcements. Seeded per month
    /// like contract generation, so identical runs get identical
    /// programs.
    ///
    /// A customer with a program still putting missions out isn't in the
    /// market for another one. Note this is "still issuing", not "still
    /// flying": once the last mission has been handed over, the market
    /// is free to announce again even though those contracts are
    /// outstanding. Waiting for them to land instead would hold the
    /// market shut through the whole tail of a program — and would do it
    /// to whoever *lost* the block bid just as hard, locking them out
    /// for the years a rival spends flying it.
    fn announce_campaigns(&mut self, events: &mut Vec<GameEvent>) {
        let econ_mod = self.economy.modifier;
        let campaign_query = format!("campaigns_{}_{}", self.date.year, self.date.month);
        let mut campaign_rng = self.seed.world_query(&campaign_query);

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
            // Same capability rule the bid engine and the contract
            // colours use — a program worth pausing for is one you
            // could actually bid on. A block commitment is a decision,
            // not a ticker item.
            let dest = campaign.destination.clone();
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
            self.emit(events, evt);
            self.active_campaigns.push(campaign);
        }
    }

    /// The daily contract market: resolve campaign block bids whose
    /// window closed, then issue due mission contracts (resolution first
    /// so a just-won program issues its first mission the same day);
    /// standing bid rules place the player's automatic bids; sealed bids
    /// whose window closed are awarded (before delivery-deadline expiry —
    /// bid windows are shorter than any delivery deadline); contracts
    /// past deadline expire (player, then competitors' overdue campaign
    /// missions — both feed the program clause); and competitors fly
    /// the awarded contracts that reached their launch day.
    fn tick_market_pipeline(&mut self, events: &mut Vec<GameEvent>) {
        self.resolve_campaign_bids(events);
        self.issue_campaign_contracts(events);
        self.run_bid_rules(events);
        self.resolve_bids(events);
        self.expire_contracts(events);
        self.expire_competitor_campaign_missions(events);
        self.process_competitor_launches(events);
    }

    /// A year without a launch costs reputation, checked each January 1st.
    fn tick_launch_drought(&mut self) {
        let days_since = match self.player_company.last_launch_date {
            Some(last) => last.days_until(&self.date),
            // Never launched: the clock started at the founding date.
            None if self.date != self.start_date => self.start_date.days_until(&self.date),
            None => return,
        };
        if days_since >= 365 {
            self.player_company.reputation.on_year_without_launch(&self.balance.reputation);
        }
    }

    /// The factory floor: a day of work on every order, cost history
    /// and rush bookkeeping for what finished, blocked orders that can
    /// now proceed, auto-build reorders, and team placement — the floor
    /// round-robin (or everything onto a rush job), and every idle
    /// engineer onto a project.
    fn tick_manufacturing(&mut self, events: &mut Vec<GameEvent>) {
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
            self.emit(events, evt);
        }

        self.player_company.try_unblock_manufacturing_orders();

        let auto_events = self.player_company.auto_reorder_rockets(&self.balance);
        self.emit_all(events, auto_events);

        // A rush job is over once its last order has left the queue —
        // clear before assigning, so the floor goes back to normal on the
        // same tick the rocket lands in inventory.
        self.player_company.clear_finished_rush_jobs();
        self.player_company.assign_manufacturing_teams();
        // An idle engineer is pure burn.
        self.player_company.auto_assign_idle_engineering_teams();
    }

    /// Stop the clock once when the floor has teams but nothing they
    /// can work on; re-arm the warning as soon as there is work again.
    fn tick_idle_manufacturing_pause(&mut self, events: &mut Vec<GameEvent>) {
        if !self.player_company.manufacturing_teams.is_empty()
            && !self.player_company.has_actionable_manufacturing_orders()
            && !self.player_company.notified_manufacturing_idle
        {
            self.speed = GameSpeed::Paused;
            self.player_company.notified_manufacturing_idle = true;
            self.emit(events, GameEvent::ManufacturingIdle);
        }
        if self.player_company.has_actionable_manufacturing_orders() {
            self.player_company.notified_manufacturing_idle = false;
        }
    }

    /// Start revisions on projects that have discovered flaws and are
    /// set to auto-revise. Runs after the day's research tick, so a flaw
    /// discovered today starts its revision today. `start_revision`
    /// itself refuses anything not in Testing.
    ///
    /// Deliberately opt-out rather than unconditional: a revision bumps
    /// the project's `revision`, which is stamped onto build orders and
    /// inventory items, so it partially resets the production learning
    /// curve. A player mid-production run may prefer to keep flying a
    /// known-flawed design.
    fn auto_revise_projects(&mut self, events: &mut Vec<GameEvent>) {
        let due: Vec<(crate::project::ProjectRef, String)> = self.player_company.projects()
            .filter(|p| p.auto_revise() && p.discovered_flaw_count() > 0)
            .map(|p| (p.project_ref(), p.name().to_string()))
            .collect();
        let mut started: Vec<(String, usize)> = Vec::new();
        for (r, name) in due {
            if let Some(plan) = self.player_company.start_revision(r) {
                started.push((name, plan.flaws));
            }
        }

        for (project_name, flaw_count) in started {
            let evt = GameEvent::AutoRevisionStarted { project_name, flaw_count };
            self.emit(events, evt);
        }
    }
}
