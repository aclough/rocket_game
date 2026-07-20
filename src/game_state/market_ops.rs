//! Markets and the award pipeline: contract issue, the standing
//! bid-rule engine, sealed-bid resolution, competitor ticks and
//! abstract launches, expiry, and seed-driven market/tech events.


use crate::contract::{self};
use crate::event::GameEvent;
use crate::rocket_project::RocketProjectId;

use super::*;

impl GameState {
    /// Issue mission contracts for won campaigns whose next issue date
    /// has arrived, and retire campaigns that have issued their last
    /// mission. Soliciting campaigns issue nothing — they're waiting on
    /// bid resolution. The winner's missions arrive **pre-accepted** at
    /// the won block price: the block bid was the acceptance.
    pub(super) fn issue_campaign_contracts(&mut self, events: &mut Vec<GameEvent>) {
        let global_window = (
            self.balance.markets.deadline_min_days,
            self.balance.markets.deadline_max_days,
        );
        for i in 0..self.active_campaigns.len() {
            // by_player, or the winning competitor's index for a
            // competitor-won program.
            let winner_ci = match &self.active_campaigns[i].status {
                contract::CampaignStatus::Won { by_player: true, .. } => None,
                contract::CampaignStatus::Won { by_player: false, company } => {
                    match self.competitors.iter()
                        .position(|k| k.company.name == *company)
                    {
                        Some(ci) => Some(ci),
                        // Winner vanished (competitor disabled after a
                        // save): missions still tick away, unserved.
                        None => continue,
                    }
                }
                contract::CampaignStatus::Soliciting { .. } => continue,
            };
            while self.active_campaigns[i].missions_issued
                < self.active_campaigns[i].missions_total
                && self.date >= self.active_campaigns[i].next_issue_date
            {
                let campaign = &self.active_campaigns[i];
                let window = self.markets.iter()
                    .find(|m| m.id == campaign.market_id)
                    .and_then(|m| m.deadline_days)
                    .unwrap_or(global_window);
                // Per-mission query: order-independent and stable
                // across save/load.
                let query = format!(
                    "campaign_issue_{}_{}", campaign.id.0, campaign.missions_issued + 1,
                );
                let mut rng = self.seed.world_query(&query);
                let mut c = contract::campaign_contract(
                    campaign, window, &mut rng, &mut self.next_contract_id, self.date,
                );
                c.status = contract::ContractStatus::Accepted;
                match winner_ci {
                    None => {
                        let evt = GameEvent::CampaignMissionIssued {
                            contract_name: c.name.clone(),
                            amount: c.payment,
                        };
                        self.player_company.active_contracts.push(c);
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    }
                    Some(ci) => {
                        // The winner's missions schedule abstract
                        // launches like any competitor award; the
                        // block award was the news, the launches will
                        // be the rest.
                        let launch_date = {
                            let d = self.date.add_days(self.balance.competitor.launch_lead_days);
                            if d > c.deadline { c.deadline } else { d }
                        };
                        let comp = &mut self.competitors[ci];
                        comp.scheduled_launches.push(crate::competitor::ScheduledLaunch {
                            contract_id: c.id,
                            launch_date,
                        });
                        comp.company.active_contracts.push(c);
                    }
                }

                let campaign = &mut self.active_campaigns[i];
                campaign.missions_issued += 1;
                campaign.next_issue_date = self.date.add_days(campaign.interval_days);
            }
        }
        // A fully-issued campaign stays tracked until its outstanding
        // missions resolve (fly or expire), so the program clause can
        // still catch a late miss; it retires once nothing references
        // it.
        let outstanding: std::collections::HashSet<contract::CampaignId> =
            self.player_company.active_contracts.iter()
                .chain(self.competitors.iter()
                    .flat_map(|k| k.company.active_contracts.iter()))
                .filter_map(|k| k.campaign_id)
                .collect();
        self.active_campaigns.retain(|c|
            c.missions_issued < c.missions_total || outstanding.contains(&c.id)
        );
    }

    /// Program-clause strike: the winner of `campaign_id` let a
    /// mission expire. Applies the extra reputation hit (on top of the
    /// normal expiry hit the caller already applied), and after
    /// `campaign_max_misses` strikes the customer cancels the
    /// remainder — the campaign is dropped with its unissued missions
    /// and the winner takes the one-time cancellation hit.
    fn campaign_mission_missed(
        &mut self,
        campaign_id: contract::CampaignId,
        contract_name: &str,
        severity: f64,
        events: &mut Vec<GameEvent>,
    ) {
        let Some(idx) = self.active_campaigns.iter()
            .position(|c| c.id == campaign_id)
        else {
            return;
        };
        let (by_player, company_name) = match &self.active_campaigns[idx].status {
            contract::CampaignStatus::Won { by_player, company } =>
                (*by_player, company.clone()),
            _ => return,
        };

        let miss_penalty = self.balance.markets.campaign_miss_rep_penalty;
        let cancel_penalty = self.balance.markets.campaign_cancel_rep_penalty;
        let max_misses = self.balance.markets.campaign_max_misses;
        let rep_cfg = self.balance.reputation.clone();
        let apply_hit = |gs: &mut Self, mult: f64| {
            if mult <= 0.0 {
                return;
            }
            if by_player {
                gs.player_company.reputation.on_contract_expired(&rep_cfg, severity * mult);
            } else if let Some(comp) = gs.competitors.iter_mut()
                .find(|k| k.company.name == company_name)
            {
                comp.company.reputation.on_contract_expired(&rep_cfg, severity * mult);
            }
        };
        apply_hit(self, miss_penalty);

        let campaign = &mut self.active_campaigns[idx];
        campaign.missions_missed += 1;
        let misses = campaign.missions_missed;
        let evt = GameEvent::CampaignMissionMissed {
            program: campaign.name.clone(),
            company: company_name.clone(),
            contract_name: contract_name.to_string(),
            misses,
            max_misses,
        };
        self.event_log.push(self.date, evt.clone());
        events.push(evt);

        let missions_remaining =
            self.active_campaigns[idx].missions_total - self.active_campaigns[idx].missions_issued;
        if misses >= max_misses && missions_remaining > 0 {
            let campaign = self.active_campaigns.remove(idx);
            apply_hit(self, cancel_penalty);
            let evt = GameEvent::CampaignCancelled {
                program: campaign.name.clone(),
                company: company_name,
                by_player,
                missions_remaining,
            };
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
        }
    }

    /// Expire competitors' overdue campaign missions. Single awards
    /// reserve a vehicle up front and always fly by their deadline,
    /// but a won program's missions issue on cadence and can outrun
    /// the line — when one expires it costs the normal expiry hit,
    /// leaves the books, drops its scheduled launch, and strikes the
    /// program clause.
    pub(super) fn expire_competitor_campaign_missions(&mut self, events: &mut Vec<GameEvent>) {
        for ci in 0..self.competitors.len() {
            let expired: Vec<(contract::ContractId, contract::CampaignId, String, contract::MarketId)> =
                self.competitors[ci].company.active_contracts.iter()
                    .filter(|c| c.campaign_id.is_some() && self.date > c.deadline)
                    .map(|c| (c.id, c.campaign_id.unwrap(), c.name.clone(), c.market_id))
                    .collect();
            for (cid, campaign_id, name, market_id) in expired {
                let severity = self.market_failure_severity(market_id);
                let comp = &mut self.competitors[ci];
                comp.company.active_contracts.retain(|c| c.id != cid);
                comp.scheduled_launches.retain(|sl| sl.contract_id != cid);
                comp.company.reputation.on_contract_expired(&self.balance.reputation, severity);
                self.campaign_mission_missed(campaign_id, &name, severity, events);
            }
        }
    }

    /// Place (or revise) the player's sealed block bid — one price per
    /// mission, applied to the whole block — on a soliciting campaign.
    /// Returns None if the campaign is unknown, already resolved, or
    /// the bid is not positive.
    pub fn place_campaign_bid(
        &mut self, campaign_id: contract::CampaignId, bid: f64,
    ) -> Option<GameEvent> {
        if bid <= 0.0 {
            return None;
        }
        let campaign = self.active_campaigns.iter_mut()
            .find(|c| c.id == campaign_id)?;
        let contract::CampaignStatus::Soliciting { player_bid, .. } = &mut campaign.status
        else {
            return None;
        };
        *player_bid = Some(bid);
        let evt = GameEvent::CampaignBidPlaced {
            program: campaign.name.clone(),
            amount: bid,
            missions: campaign.missions_total,
        };
        self.event_log.push(self.date, evt.clone());
        Some(evt)
    }

    /// Resolve campaigns whose block-bid window has closed. Bids are
    /// gathered like single solicitations — the player's sealed price
    /// first, then each competitor's scripted block bid — and scored
    /// once for the whole block with the same logistic formula; ties
    /// break toward the player. The winner's `payment_per_mission`
    /// becomes the won bid and missions start issuing. Unbid programs
    /// lapse quietly — the customer found no launcher.
    pub(super) fn resolve_campaign_bids(&mut self, events: &mut Vec<GameEvent>) {
        let mut i = 0;
        while i < self.active_campaigns.len() {
            let (deadline, ceiling, player_bid) = match self.active_campaigns[i].status {
                contract::CampaignStatus::Soliciting {
                    bid_deadline, budget_ceiling_per_mission, player_bid,
                } => (bid_deadline, budget_ceiling_per_mission, player_bid),
                _ => { i += 1; continue; }
            };
            if self.date <= deadline {
                i += 1;
                continue;
            }

            let market = self.markets.iter()
                .find(|m| m.id == self.active_campaigns[i].market_id)
                .cloned();
            let rep_scale = self.balance.markets.rep_scale;
            let score = |bid: f64, rep: f64| {
                market.as_ref().map_or(0.0, |m| {
                    contract::bid_score(bid, ceiling, rep, m, rep_scale)
                })
            };

            // (bidder, bid): bidder None = player, Some(ci) = competitor.
            // Player first + strict `>` replacement = ties break toward
            // the player, exactly like resolve_bids.
            let mut winner: Option<(Option<usize>, f64)> = None;
            let mut best_score = f64::NEG_INFINITY;
            let mut consider = |who: Option<usize>, bid: f64, s: f64| {
                if s > best_score {
                    best_score = s;
                    winner = Some((who, bid));
                }
            };
            if let Some(bid) = player_bid {
                if bid <= ceiling {
                    consider(None, bid, score(bid, self.player_company.reputation.total()));
                }
            }
            {
                let campaign = &self.active_campaigns[i];
                for (ci, comp) in self.competitors.iter().enumerate() {
                    if let Some(bid) = comp.compute_block_bid(campaign, &self.balance, &self.seed) {
                        if bid <= ceiling {
                            consider(Some(ci), bid, score(bid, comp.company.reputation.total()));
                        }
                    }
                }
            }

            // Block awards land in the same price-discovery history
            // as single solicitations, tagged with the mission count
            // (the amount is per mission).
            let record_date = self.date;
            let record_outcome =
                move |outcome: contract::AwardOutcome, c: &contract::Campaign| {
                    contract::AwardRecord {
                        date: record_date,
                        market_id: c.market_id,
                        contract_name: c.name.clone(),
                        destination: c.destination.clone(),
                        payload_kg: c.payload_kg,
                        missions: Some(c.missions_total),
                        outcome,
                    }
                };
            match winner {
                Some((None, bid)) => {
                    let name = self.player_company.name.clone();
                    let campaign = &mut self.active_campaigns[i];
                    campaign.payment_per_mission = bid;
                    campaign.status = contract::CampaignStatus::Won {
                        by_player: true,
                        company: name,
                    };
                    // First mission issues immediately; the rest follow
                    // on the program's cadence.
                    campaign.next_issue_date = self.date;
                    let evt = GameEvent::CampaignAwarded {
                        program: campaign.name.clone(),
                        amount: bid,
                        missions: campaign.missions_total,
                    };
                    let record = record_outcome(
                        contract::AwardOutcome::PlayerWon { amount: bid },
                        campaign,
                    );
                    self.push_award_record(record);
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                    // Winning a program is a decision point (schedule
                    // builds for the whole block) — stop the clock.
                    self.speed = GameSpeed::Paused;
                    i += 1;
                }
                Some((Some(ci), bid)) => {
                    let company_name = self.competitors[ci].company.name.clone();
                    let campaign = &mut self.active_campaigns[i];
                    campaign.payment_per_mission = bid;
                    campaign.status = contract::CampaignStatus::Won {
                        by_player: false,
                        company: company_name.clone(),
                    };
                    campaign.next_issue_date = self.date;
                    let evt = GameEvent::CampaignAwardedToCompetitor {
                        program: campaign.name.clone(),
                        company: company_name.clone(),
                        amount: bid,
                        missions: campaign.missions_total,
                        player_bid,
                    };
                    let record = record_outcome(
                        contract::AwardOutcome::CompetitorWon {
                            company: company_name,
                            amount: bid,
                            player_bid,
                        },
                        campaign,
                    );
                    self.push_award_record(record);
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                    i += 1;
                }
                None => {
                    let campaign = self.active_campaigns.remove(i);
                    if let Some(bid) = player_bid {
                        // Over budget: no award, and the customer
                        // doesn't say what the budget was.
                        let record = record_outcome(
                            contract::AwardOutcome::PlayerRejected { bid },
                            &campaign,
                        );
                        self.push_award_record(record);
                        let evt = GameEvent::CampaignBidRejected {
                            program: campaign.name.clone(),
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    }
                    // No bid at all: lapses without ceremony.
                }
            }
        }
    }

    /// Resolve solicitations whose bid window has closed. With the
    /// player as sole bidder (M3 Task 1), a bid wins iff it fits the
    /// customer's hidden budget; `contract::bid_score` ranks bidders
    /// once DinoSoar enters (Task 2). Unbid solicitations lapse
    /// quietly.
    /// The standing-rule auto-bidder (M3 Task 3). For each unbid
    /// solicitation whose market has an enabled rule: find the
    /// cheapest capable Testing design with real build-cost history,
    /// gate on free stock, and place marginal cost × (1 + margin).
    ///
    /// Free stock = capable rockets in inventory − rockets already
    /// promised (accepted contracts not currently flying, plus
    /// outstanding sealed bids — counting bids goes beyond the bare
    /// awarded-unflown reservation so the engine can never promise
    /// five launches against one rocket). Same gate shape as
    /// DinoSoar's script and, later, other competitors.
    pub(super) fn run_bid_rules(&mut self, events: &mut Vec<GameEvent>) {
        if self.player_company.bid_rules.is_empty() {
            return;
        }
        let total_stock = self.player_company.manufacturing.inventory.rockets.len();
        if total_stock == 0 {
            return;
        }
        let in_flight: Vec<contract::ContractId> = self.active_flights.iter()
            .flat_map(|f| f.payloads.iter())
            .filter_map(|p| match p {
                crate::flight::Payload::ContractDelivery { contract_id, .. } =>
                    Some(*contract_id),
                _ => None,
            })
            .collect();
        let accepted_unflown = self.player_company.active_contracts.iter()
            .filter(|c| matches!(c.status, contract::ContractStatus::Accepted)
                && !in_flight.contains(&c.id))
            .count();

        for i in 0..self.available_contracts.len() {
            let (market_id, dest, payload_kg) = {
                let c = &self.available_contracts[i];
                if !c.is_solicitation() || c.player_bid.is_some() {
                    continue;
                }
                (c.market_id, c.destination.clone(), c.payload_kg)
            };
            let Some(rule) = self.player_company.bid_rules.get(&market_id) else {
                continue;
            };
            if !rule.enabled {
                continue;
            }
            let margin = rule.margin;

            // Capable designs (Testing only), and the cheapest real
            // marginal cost among those that have been built before.
            // No cost history → no cost basis → no bid.
            let mut capable_projects: Vec<RocketProjectId> = Vec::new();
            let mut best_cost: Option<f64> = None;
            for rp in &self.player_company.rocket_projects {
                if !matches!(rp.status, crate::rocket_project::RocketDesignStatus::Testing { .. }) {
                    continue;
                }
                let cap = *self.payload_capability_cache
                    .entry((rp.project_id, rp.revision, dest.clone()))
                    .or_insert_with(|| crate::rocket_project::max_payload_to(
                        &rp.design, "earth_surface", &dest,
                    ));
                if payload_kg > cap * BID_PAYLOAD_MARGIN {
                    continue;
                }
                capable_projects.push(rp.project_id);
                if let Some(h) = self.player_company.rocket_cost_history.get(&rp.design.id) {
                    if !h.is_empty() {
                        let recent = &h[h.len().saturating_sub(5)..];
                        let cost = recent.iter().sum::<f64>() / recent.len() as f64;
                        if best_cost.is_none_or(|b| cost < b) {
                            best_cost = Some(cost);
                        }
                    }
                }
            }
            let Some(cost) = best_cost else { continue };

            // Readiness gate: free stock must cover this new bid.
            let capable_stock = self.player_company.manufacturing.inventory.rockets.iter()
                .filter(|r| capable_projects.contains(&r.rocket_project_id))
                .count();
            let pending_bids = self.available_contracts.iter()
                .filter(|c| c.player_bid.is_some())
                .count();
            if capable_stock <= accepted_unflown + pending_bids {
                continue;
            }

            let bid = ((cost * (1.0 + margin)) / 10_000.0).round() * 10_000.0;
            if bid <= 0.0 {
                continue;
            }
            if let Some(evt) = self.place_bid(i, bid) {
                events.push(evt);
            }
        }
    }

    pub(super) fn resolve_bids(&mut self, events: &mut Vec<GameEvent>) {
        let mut i = 0;
        while i < self.available_contracts.len() {
            let due = {
                let c = &self.available_contracts[i];
                c.bid_deadline.is_some_and(|bd| self.date > bd)
            };
            if !due {
                i += 1;
                continue;
            }
            let mut c = self.available_contracts.remove(i);

            let market = self.markets.iter().find(|m| m.id == c.market_id).cloned();
            let rep_scale = self.balance.markets.rep_scale;
            let score = |bid: f64, rep: f64| {
                market.as_ref().map_or(0.0, |m| {
                    contract::bid_score(bid, c.budget_ceiling, rep, m, rep_scale)
                })
            };

            // Gather sealed bids: the player first, then each
            // competitor's scripted price. Over-ceiling bids never
            // score. Ties break toward the earlier entry, so an
            // exactly-matched player never loses to a coin flip.
            // (bidder, bid): bidder None = player, Some(ci) = competitor.
            let mut winner: Option<(Option<usize>, f64)> = None;
            let mut best_score = f64::NEG_INFINITY;
            let mut consider = |who: Option<usize>, bid: f64, s: f64| {
                if s > best_score {
                    best_score = s;
                    winner = Some((who, bid));
                }
            };
            let mut player_over_ceiling = false;
            if let Some(bid) = c.player_bid {
                if bid <= c.budget_ceiling {
                    consider(None, bid, score(bid, self.player_company.reputation.total()));
                } else {
                    player_over_ceiling = true;
                }
            }
            for (ci, comp) in self.competitors.iter().enumerate() {
                if let Some(bid) = comp.compute_bid(&c, &self.balance, &self.seed) {
                    if bid <= c.budget_ceiling {
                        consider(Some(ci), bid, score(bid, comp.company.reputation.total()));
                    }
                }
            }

            let record_date = self.date;
            let record_outcome = move |outcome: contract::AwardOutcome, c: &contract::Contract| {
                contract::AwardRecord {
                    date: record_date,
                    market_id: c.market_id,
                    contract_name: c.name.clone(),
                    destination: c.destination.clone(),
                    payload_kg: c.payload_kg,
                    missions: None,
                    outcome,
                }
            };
            match winner {
                Some((None, bid)) => {
                    let record = record_outcome(
                        contract::AwardOutcome::PlayerWon { amount: bid }, &c,
                    );
                    self.push_award_record(record);
                    c.payment = bid;
                    c.status = contract::ContractStatus::Accepted;
                    let evt = GameEvent::ContractAwarded {
                        contract_name: c.name.clone(),
                        amount: bid,
                    };
                    self.player_company.active_contracts.push(c);
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                    // Winning a contract is a decision point (schedule
                    // the launch, adjust rules) — stop the clock.
                    self.speed = GameSpeed::Paused;
                }
                Some((Some(ci), bid)) => {
                    let losing_player_bid = c.player_bid;
                    let record = record_outcome(
                        contract::AwardOutcome::CompetitorWon {
                            company: self.competitors[ci].company.name.clone(),
                            amount: bid,
                            player_bid: c.player_bid,
                        },
                        &c,
                    );
                    self.push_award_record(record);
                    c.payment = bid;
                    c.status = contract::ContractStatus::Accepted;
                    let launch_date = {
                        let d = self.date.add_days(self.balance.competitor.launch_lead_days);
                        if d > c.deadline { c.deadline } else { d }
                    };
                    let comp = &mut self.competitors[ci];
                    comp.scheduled_launches.push(crate::competitor::ScheduledLaunch {
                        contract_id: c.id,
                        launch_date,
                    });
                    let evt = GameEvent::ContractAwardedToCompetitor {
                        contract_name: c.name.clone(),
                        company: comp.company.name.clone(),
                        amount: bid,
                        player_bid: losing_player_bid,
                    };
                    comp.company.active_contracts.push(c);
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                }
                None if player_over_ceiling => {
                    // Over budget: no award, and the customer doesn't
                    // say what the budget was.
                    let record = record_outcome(
                        contract::AwardOutcome::PlayerRejected {
                            bid: c.player_bid.unwrap_or(0.0),
                        },
                        &c,
                    );
                    self.push_award_record(record);
                    let evt = GameEvent::BidRejected {
                        contract_name: c.name.clone(),
                    };
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                }
                None => {} // No valid bids: lapses without ceremony.
            }
        }
    }

    /// Append to the award-history record, dropping the oldest entries
    /// past the cap (bounds save size; ~15 awards/year game-time).
    pub(super) fn push_award_record(&mut self, record: contract::AwardRecord) {
        const AWARD_HISTORY_CAP: usize = 400;
        self.award_history.push(record);
        if self.award_history.len() > AWARD_HISTORY_CAP {
            let excess = self.award_history.len() - AWARD_HISTORY_CAP;
            self.award_history.drain(..excess);
        }
    }

    /// Daily manufacturing tick for scripted competitors: the same
    /// machinery the player runs, with only finished vehicles making
    /// the news.
    pub(super) fn tick_competitors(&mut self, events: &mut Vec<GameEvent>) {
        for ci in 0..self.competitors.len() {
            let comp = &mut self.competitors[ci];
            let mfg_events = comp.company.manufacturing.advance_day(&self.balance.costs);
            for me in mfg_events {
                if let crate::manufacturing::ManufacturingEvent::RocketIntegrated {
                    design_id, rocket_name, build_cost, ..
                } = me
                {
                    comp.company.rocket_cost_history
                        .entry(design_id)
                        .or_default()
                        .push(build_cost);
                    let evt = GameEvent::CompetitorRocketBuilt {
                        company: comp.company.name.clone(),
                        rocket_name,
                    };
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                }
            }
            comp.company.try_unblock_manufacturing_orders();
            // Auto-build events are the competitor's internal
            // bookkeeping, not news.
            let _ = comp.company.auto_reorder_rockets(&self.balance);
            comp.company.auto_assign_idle_manufacturing_teams();
        }
    }

    /// Fly competitors' awarded contracts whose scheduled day arrived:
    /// consume a real inventory rocket, roll its snapshot flaws once
    /// (per-flight), settle payment and reputation, make the news.
    pub(super) fn process_competitor_launches(&mut self, events: &mut Vec<GameEvent>) {
        use rand::Rng;

        for ci in 0..self.competitors.len() {
            let due: Vec<crate::competitor::ScheduledLaunch> = {
                let comp = &mut self.competitors[ci];
                let mut d = Vec::new();
                let mut j = 0;
                while j < comp.scheduled_launches.len() {
                    if self.date >= comp.scheduled_launches[j].launch_date {
                        d.push(comp.scheduled_launches.remove(j));
                    } else {
                        j += 1;
                    }
                }
                d
            };
            for launch in due {
                let taken = {
                    let comp = &mut self.competitors[ci];
                    let pos = comp.company.active_contracts.iter()
                        .position(|c| c.id == launch.contract_id);
                    let ridx = comp.company.manufacturing.inventory.rockets.iter()
                        .position(|r| r.rocket_project_id == comp.rocket_project_id);
                    match (pos, ridx) {
                        (Some(pos), Some(ridx)) => {
                            let contract = comp.company.active_contracts.remove(pos);
                            let rocket = comp.company.manufacturing.inventory.rockets.remove(ridx);
                            Some((contract, rocket))
                        }
                        (Some(_), None) => {
                            // Out of stock: single awards reserve a
                            // rocket up front, but a won campaign's
                            // missions issue on cadence and can outpace
                            // the line. Retry weekly until a vehicle
                            // integrates (the miss/cancellation clause
                            // in campaign plan Task 3 puts a price on
                            // running this late).
                            comp.scheduled_launches.push(crate::competitor::ScheduledLaunch {
                                contract_id: launch.contract_id,
                                launch_date: self.date.add_days(7),
                            });
                            None
                        }
                        (None, _) => None,
                    }
                };
                let Some((contract, rocket)) = taken else { continue };

                let severity = self.market_failure_severity(contract.market_id);
                let mut rng = self.seed.world_query(&format!("dino_launch_{}", contract.id.0));
                let failed = rocket.rocket_flaws.iter()
                    .any(|fl| rng.gen::<f64>() < fl.activation_chance);

                let comp = &mut self.competitors[ci];
                if failed {
                    comp.company.reputation.on_launch_failure(&self.balance.reputation, severity);
                } else {
                    comp.company.money += contract.payment;
                    comp.company.reputation.on_launch_success(&self.balance.reputation);
                    comp.company.reputation.on_contract_launch(&self.balance.reputation);
                }
                comp.company.last_launch_date = Some(self.date);
                let evt = GameEvent::CompetitorLaunch {
                    company: comp.company.name.clone(),
                    contract_name: contract.name.clone(),
                    success: !failed,
                };
                self.event_log.push(self.date, evt.clone());
                events.push(evt);
            }
        }
    }

    /// Place (or revise) a sealed bid on an available solicitation.
    /// Returns None if the index is invalid, the contract is
    /// pre-priced (campaign missions, legacy saves), or the bid is
    /// not positive.
    pub fn place_bid(&mut self, index: usize, bid: f64) -> Option<GameEvent> {
        let c = self.available_contracts.get_mut(index)?;
        if !c.is_solicitation() || bid <= 0.0 {
            return None;
        }
        c.player_bid = Some(bid);
        let evt = GameEvent::BidPlaced {
            contract_name: c.name.clone(),
            amount: bid,
        };
        self.event_log.push(self.date, evt.clone());
        Some(evt)
    }

    /// Expire contracts past their deadline and update reputation.
    pub(super) fn expire_contracts(&mut self, events: &mut Vec<GameEvent>) {
        // Check available contracts
        let mut expired_available = Vec::new();
        for (i, c) in self.available_contracts.iter().enumerate() {
            if self.date > c.deadline {
                expired_available.push(i);
            }
        }
        for i in expired_available.into_iter().rev() {
            self.available_contracts.remove(i);
        }

        // Check accepted contracts on the company
        let mut expired_accepted = Vec::new();
        for (i, c) in self.player_company.active_contracts.iter().enumerate() {
            if self.date > c.deadline {
                expired_accepted.push((i, c.name.clone(), c.market_id, c.campaign_id));
            }
        }
        for (i, name, market_id, campaign_id) in expired_accepted.into_iter().rev() {
            self.player_company.active_contracts.remove(i);
            let severity = self.market_failure_severity(market_id);
            self.player_company.reputation.on_contract_expired(&self.balance.reputation, severity);
            let evt = GameEvent::ContractExpired { contract_name: name.clone() };
            self.event_log.push(self.date, evt.clone());
            events.push(evt);
            // A missed program mission also strikes the campaign
            // clause on top of the normal expiry hit.
            if let Some(campaign_id) = campaign_id {
                self.campaign_mission_missed(campaign_id, &name, severity, events);
            }
        }
    }

    /// Reputation-penalty severity for a market (1.0 if unknown).
    pub(super) fn market_failure_severity(&self, market_id: contract::MarketId) -> f64 {
        self.markets.iter()
            .find(|m| m.id == market_id)
            .map_or(1.0, |m| m.failure_severity)
    }

    /// Harshest failure severity across the contracts on a manifest
    /// (1.0 when no contracts are aboard, e.g. test-mass flights).
    pub(super) fn manifest_failure_severity(&self, contract_ids: &[contract::ContractId]) -> f64 {
        contract_ids.iter()
            .filter_map(|cid| {
                self.player_company.active_contracts.iter().find(|c| c.id == *cid)
            })
            .map(|c| self.market_failure_severity(c.market_id))
            .fold(1.0, f64::max)
    }

    /// Accept a pre-priced contract from the available market
    /// (campaign missions and pre-M3 saves). Solicitations must be
    /// bid on instead — see [`GameState::place_bid`].
    pub fn accept_contract(&mut self, index: usize) -> Option<GameEvent> {
        if index >= self.available_contracts.len()
            || self.available_contracts[index].is_solicitation()
        {
            return None;
        }
        let mut c = self.available_contracts.remove(index);
        let name = c.name.clone();
        c.status = contract::ContractStatus::Accepted;
        self.player_company.active_contracts.push(c);
        let evt = GameEvent::ContractAccepted { contract_name: name };
        self.event_log.push(self.date, evt.clone());
        Some(evt)
    }

    /// Check yearly tech unlock rolls.
    pub(super) fn check_tech_unlocks(&mut self, events: &mut Vec<GameEvent>) {
        use rand::Rng;
        for tech in &mut self.technologies {
            if tech.unlocked {
                continue;
            }
            let query = format!("tech_unlock_{}_{}", tech.id.0, self.date.year);
            let mut rng = self.seed.world_query(&query);
            let chance = match tech.difficulty {
                0 => 0.0,
                1 => 0.10,
                _ => 0.08,
            };
            if rng.gen::<f64>() < chance {
                tech.unlocked = true;
                let evt = GameEvent::EconomicShift {
                    condition: format!("Technology Available: {}", tech.name),
                    description: tech.description.clone(),
                };
                self.event_log.push(self.date, evt.clone());
                events.push(evt);
                self.speed = GameSpeed::Paused;
            }
        }
    }

    /// Check seed-driven market emergence and activate markets whose
    /// trigger year has arrived. Presence and timing are recomputed
    /// from the archetype table each call (`world_query` is
    /// order-independent), so the only persistent state is
    /// `fired_market_events` recording actual activations.
    pub(super) fn check_market_events(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();

        let realized = contract::realize_markets(&self.seed, &self.balance.markets.archetypes);
        let mut to_fire: Vec<(String, contract::MarketId, String, Vec<contract::CrossEffect>)> =
            Vec::new();
        for (arch, r) in self.balance.markets.archetypes.iter().zip(&realized) {
            let Some(emergence) = &arch.emergence else { continue };
            if !r.present {
                continue;
            }
            let Some(trigger_year) = r.trigger_year else { continue };
            if self.date.year < trigger_year || self.fired_market_events.contains(&arch.key) {
                continue;
            }
            to_fire.push((
                arch.key.clone(),
                arch.template.id,
                emergence.flavor.clone(),
                emergence.cross_effects.clone(),
            ));
        }

        for (key, market_id, flavor, cross_effects) in to_fire {
            if let Some(market) = self.markets.iter_mut().find(|m| m.id == market_id) {
                market.active = true;
                market.activation_date = Some(self.date);
            }
            for ce in &cross_effects {
                if let Some(target) = self.markets.iter_mut().find(|m| m.id == ce.target) {
                    target.add_modifier(ce.modifier.clone());
                }
            }
            let market_name = self.markets.iter()
                .find(|m| m.id == market_id)
                .map(|m| m.name.clone())
                .unwrap_or_default();
            self.fired_market_events.push(key);

            events.push(GameEvent::EconomicShift {
                condition: format!("New Market: {}", market_name),
                description: flavor,
            });
        }

        events
    }
}
