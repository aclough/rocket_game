//! Scripted company policies — bots that play the game headlessly.
//!
//! Used by the `simulate` binary for tuning runs, and later as the
//! brains of AI competitors (DinoSoar). A policy is called once per
//! day, before `GameState::advance_day`, and acts through the same
//! public `GameState`/`Company` methods the UI uses.
//!
//! Policies must be deterministic: index-ordered choices only, no
//! wall-clock, no HashMap-iteration-order dependence. A fixed seed +
//! a fixed policy must always produce an identical run.

use std::collections::BTreeMap;

use crate::contract::ContractStatus;
use crate::engine::EngineCycle;
use crate::engine_project::{EngineDesignStatus, EngineProjectId, PropellantPreset};
use crate::flight::Payload;
use crate::game_state::GameState;
use crate::rocket::{RocketDesign, RocketDesignId};
use crate::rocket_project::{RocketDesignStatus, RocketProjectId};
use crate::stage::{Stage, StageId};

pub trait CompanyPolicy {
    /// Take today's actions. Called once per day before `advance_day`.
    fn act(&mut self, game: &mut GameState);

    /// Name for CLI selection and reporting.
    fn name(&self) -> &'static str;
}

/// Takes no actions at all — the "do nothing" baseline. Useful for
/// measuring pure salary burn and for exercising the sim harness
/// before real policies exist.
pub struct NullPolicy;

impl CompanyPolicy for NullPolicy {
    fn act(&mut self, _game: &mut GameState) {}

    fn name(&self) -> &'static str {
        "none"
    }
}

/// Honest-but-naive baseline player. Runs the game's actual core
/// loop, dumbly:
///
/// 1. Hire up to 2 engineering + 3 manufacturing teams.
/// 2. Design a kerolox gas-generator booster engine and a hydrolox
///    upper-stage engine (the hydrolox path exercises the hydrogen
///    problems factor every run).
/// 3. When both engines finish design, start a fixed-template
///    two-stage rocket (kerolox first stage, hydrolox upper).
/// 4. When the rocket design reaches Testing, set an auto-build
///    target of 1 so the game keeps one rocket in inventory.
/// 5. Keep idle engineers testing; whenever a project in Testing has
///    discovered flaws, revise them. (Skipping revision entirely is
///    not viable: unrevised first flights nearly always fail, and a
///    company with negative reputation and no successes is locked out
///    of every market permanently.)
/// 6. Fly test-mass launches until the first success, then switch to
///    accepting the best-paying available contract the template can
///    lift, launching it the same day it is accepted.
///
/// Deliberately skipped: reactors, electric propulsion, third-party
/// engines, design iteration, pricing.
pub struct BasicPolicy {
    booster: Option<EngineProjectId>,
    upper: Option<EngineProjectId>,
    rocket: Option<RocketProjectId>,
    auto_build_set: bool,
    /// Standing bid rules installed yet? (Done once; the game's rule
    /// engine does the actual bidding from then on.)
    bid_rules_set: bool,
    /// Markup the policy's standing rules use: bid = cost × (1 + margin).
    bid_margin: f64,
    /// Max payload (kg) to a destination for the fixed template.
    /// BTreeMap for deterministic iteration.
    capability: BTreeMap<String, f64>,
}

impl BasicPolicy {
    /// Has any launch ever fully succeeded? The success reputation
    /// factor only becomes positive via `on_launch_success`.
    fn had_first_success(game: &GameState) -> bool {
        game.player_company.reputation.success_factor > 0.0
    }

    /// A flight of ours currently in transit?
    fn flight_in_transit(game: &GameState) -> bool {
        !game.active_flights.is_empty()
    }

    /// Start revisions on any Testing project with discovered flaws
    /// (or pending improvements / tech deficiencies for engines).
    fn revise_discovered_flaws(game: &mut GameState) {
        let company = &mut game.player_company;
        for i in 0..company.engine_projects.len() {
            let p = &company.engine_projects[i];
            if matches!(p.status, EngineDesignStatus::Testing { .. })
                && p.discovered_flaw_count() > 0
            {
                company.start_engine_revision(i);
            }
        }
        for i in 0..company.rocket_projects.len() {
            let p = &company.rocket_projects[i];
            if matches!(p.status, RocketDesignStatus::Testing { .. })
                && p.discovered_flaw_count() > 0
            {
                company.start_rocket_revision(i);
            }
        }
    }
}

/// Don't hire or start projects when cash falls below this floor.
const MONEY_FLOOR: f64 = 5_000_000.0;
/// Fraction of computed max payload the bot is willing to book —
/// shared with the game's rule engine.
const PAYLOAD_MARGIN: f64 = crate::game_state::BID_PAYLOAD_MARGIN;
/// Default markup for the policy's standing bid rules. Looks high as
/// a "margin", but the game's payment scales sit several multiples
/// above marginal build cost (see the cost-realism TODO), so cost ×
/// 5 is simply market-rate pricing — the 2026-07 sweep measured
/// profitability rising monotonically through this range (0.25 →
/// 0/200 seeds profitable, 4.0 → 193/200) because the bot's
/// small-payload market is uncontested and only budget ceilings
/// push back.
const DEFAULT_BID_MARGIN: f64 = 4.0;

impl BasicPolicy {
    pub fn new() -> Self {
        Self::with_margin(DEFAULT_BID_MARGIN)
    }

    /// A BasicPolicy whose standing rules use the given markup —
    /// the knob the margin sweep exercises (`--policy basic:0.15`).
    pub fn with_margin(bid_margin: f64) -> Self {
        BasicPolicy {
            booster: None,
            upper: None,
            rocket: None,
            auto_build_set: false,
            bid_rules_set: false,
            bid_margin,
            capability: BTreeMap::new(),
        }
    }

    /// Install standing bid rules for every market, once. The game's
    /// rule engine handles capability, cost basis, and the readiness
    /// gate from here on.
    fn ensure_bid_rules(&mut self, game: &mut GameState) {
        if self.bid_rules_set {
            return;
        }
        let market_ids: Vec<_> = game.markets.iter().map(|m| m.id).collect();
        for id in market_ids {
            game.player_company.bid_rules.insert(id, crate::game_state::BidRule {
                enabled: true,
                margin: self.bid_margin,
            });
        }
        self.bid_rules_set = true;
    }

    fn ensure_teams(&self, game: &mut GameState) {
        if game.player_company.money < MONEY_FLOOR {
            return;
        }
        if game.player_company.teams.len() < 3 {
            let name = format!("Team {}", game.player_company.teams.len() + 1);
            if let Some(evt) = game.player_company.hire_team(name, &game.balance) {
                game.event_log.push(game.date, evt);
            }
        }
        if game.player_company.manufacturing_teams.len() < 3 {
            let name = format!("Mfg Team {}", game.player_company.manufacturing_teams.len() + 1);
            if let Some(evt) = game.player_company.hire_manufacturing_team(name, &game.balance) {
                game.event_log.push(game.date, evt);
            }
        }
    }

    fn ensure_engine_projects(&mut self, game: &mut GameState) {
        if game.player_company.money < MONEY_FLOOR {
            return;
        }
        if self.booster.is_none() {
            if let Some(evt) = game.player_company.start_engine_project(
                "BLV Booster".into(),
                EngineCycle::GasGenerator,
                PropellantPreset::Kerolox,
                1.0,
                false, // sea-level optimized
                None,
                &game.balance,
            ) {
                game.event_log.push(game.date, evt);
                self.booster = game.player_company.engine_projects.last()
                    .map(|p| p.project_id);
            }
        }
        if self.upper.is_none() {
            if let Some(evt) = game.player_company.start_engine_project(
                "BLV Upper".into(),
                EngineCycle::GasGenerator,
                PropellantPreset::Hydrolox,
                1.0,
                true, // vacuum optimized
                None,
                &game.balance,
            ) {
                game.event_log.push(game.date, evt);
                self.upper = game.player_company.engine_projects.last()
                    .map(|p| p.project_id);
            }
        }
    }

    /// Put idle engineering teams to work: the rocket project first
    /// (it gates the pipeline — 2 teams while designing/revising, 1
    /// while testing), then one team per engine project so testing
    /// keeps discovering flaws and revisions actually progress.
    fn assign_idle_engineers(&self, game: &mut GameState) {
        let company = &mut game.player_company;
        if let Some(ri) = self.rocket.and_then(|rid|
            company.rocket_projects.iter().position(|p| p.project_id == rid))
        {
            let want = match company.rocket_projects[ri].status {
                RocketDesignStatus::InDesign { .. }
                | RocketDesignStatus::Revising { .. } => 2,
                RocketDesignStatus::Testing { .. } => 1,
            };
            while company.rocket_projects[ri].teams_assigned < want
                && company.add_team_to_rocket_project(ri) {}
            // Pull a team off an engine if the rocket is starved.
            if company.rocket_projects[ri].teams_assigned == 0 {
                company.steal_engineering_team_to_rocket_project(ri);
            }
        }
        for i in 0..company.engine_projects.len() {
            if company.engine_projects[i].teams_assigned == 0 {
                company.add_team_to_project(i);
            }
        }
    }

    /// Fixed two-stage template: one kerolox booster engine under a
    /// 42 t first stage, one hydrolox engine under an 8 t upper stage.
    /// Sized to put a small-sat class payload into LEO with margin.
    fn build_template(&self, game: &GameState) -> Option<RocketDesign> {
        let company = &game.player_company;
        let booster = company.engine_projects.iter()
            .find(|p| Some(p.project_id) == self.booster)?;
        let upper = company.engine_projects.iter()
            .find(|p| Some(p.project_id) == self.upper)?;
        // The engine design exists once it's out of the design phase —
        // Testing *or* Revising both qualify (the revision loop keeps
        // engines cycling between the two, so requiring simultaneous
        // Testing would deadlock the template).
        let design_done = |status: &EngineDesignStatus| !matches!(
            status,
            EngineDesignStatus::Proposed { .. } | EngineDesignStatus::InDesign { .. },
        );
        if !design_done(&booster.status) || !design_done(&upper.status) {
            return None;
        }

        let mut s1 = Stage {
            id: StageId(1),
            name: "BLV S1".into(),
            engine: booster.design.clone(),
            engine_count: 1,
            propellant_mass_kg: 42_000.0,
            structural_mass_kg: 3_000.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        let mut s2 = Stage {
            id: StageId(2),
            name: "BLV S2".into(),
            engine: upper.design.clone(),
            engine_count: 1,
            propellant_mass_kg: 8_000.0,
            structural_mass_kg: 800.0,
            fairing: None,
            power_sources: Vec::new(),
        };
        // Cover housekeeping power like the designer's default panels.
        s1.power_sources.push(crate::power::solar_panel_for_stage_demand(&s1));
        s2.power_sources.push(crate::power::solar_panel_for_stage_demand(&s2));

        Some(RocketDesign {
            id: RocketDesignId(company.next_rocket_project_id),
            name: "BLV-1".into(),
            stage_groups: vec![vec![s1], vec![s2]],
        })
    }

    fn maybe_design_rocket(&mut self, game: &mut GameState) {
        if self.rocket.is_some() || game.player_company.money < MONEY_FLOOR {
            return;
        }
        let Some(design) = self.build_template(game) else {
            return;
        };
        if let Some(evt) = game.player_company.start_rocket_project(design, &game.balance) {
            game.event_log.push(game.date, evt);
            self.rocket = game.player_company.rocket_projects.last()
                .map(|p| p.project_id);
        }
    }

    fn maybe_enable_auto_build(&mut self, game: &mut GameState) {
        if self.auto_build_set {
            return;
        }
        let Some(rid) = self.rocket else { return };
        // Two on the shelf: the readiness gate allows one outstanding
        // bid per free rocket, so a single rejected bid no longer
        // stalls the whole pipeline for a bid window.
        if game.player_company.set_auto_build_target(rid, 2) {
            self.auto_build_set = true;
        }
    }

    /// Max payload the template lifts from Earth to `dest`, cached.
    /// The template is fixed, so the answer never changes.
    fn capability_to(&mut self, game: &GameState, dest: &str) -> f64 {
        if let Some(&kg) = self.capability.get(dest) {
            return kg;
        }
        let kg = self.rocket
            .and_then(|rid| game.player_company.rocket_projects.iter()
                .find(|p| p.project_id == rid))
            .map(|p| crate::rocket_project::max_payload_to(
                &p.design, "earth_surface", dest))
            .unwrap_or(0.0);
        self.capability.insert(dest.to_string(), kg);
        kg
    }

    /// Contract ids currently being carried by a flight in transit.
    fn contracts_in_flight(game: &GameState) -> Vec<crate::contract::ContractId> {
        let mut ids = Vec::new();
        for flight in &game.active_flights {
            for p in &flight.payloads {
                if let Payload::ContractDelivery { contract_id, .. } = p {
                    ids.push(*contract_id);
                }
            }
        }
        ids
    }

    fn accept_and_launch(&mut self, game: &mut GameState) {
        // Need a rocket in inventory to do anything.
        let Some(rocket_item_id) = game.player_company.manufacturing
            .inventory.rockets.first().map(|r| r.item_id)
        else {
            return;
        };

        // Until the first fully successful flight, fly test masses —
        // one vehicle in the air at a time. Failures discover flaws
        // (which the revision loop then fixes) without burning
        // contracts or wrecking reputation beyond recovery.
        if !Self::had_first_success(game) {
            if Self::flight_in_transit(game) {
                return;
            }
            let Ok((dest, payloads)) = game.build_launch_payloads(&[], &[]) else {
                return;
            };
            game.launch_rocket(rocket_item_id, &dest, payloads, false);
            return;
        }

        // 1) An already-accepted contract not currently flying (e.g. a
        //    previous attempt blew up) takes priority.
        let in_flight = Self::contracts_in_flight(game);
        let pending = game.player_company.active_contracts.iter()
            .position(|c| matches!(c.status, ContractStatus::Accepted)
                && !in_flight.contains(&c.id));

        let active_index = match pending {
            Some(i) => i,
            None => {
                // 2) Solicitations are handled by the standing bid
                //    rules (installed in ensure_bid_rules; the game's
                //    rule engine bids and awards arrive by deadline).
                //    Here we only accept the best-paying *pre-priced*
                //    contract the template can lift (campaign missions
                //    and pre-M3 saves).
                let mut best: Option<(usize, f64)> = None;
                let candidates: Vec<(usize, String, f64, f64)> = game.available_contracts
                    .iter().enumerate()
                    .filter(|(_, c)| !c.is_solicitation())
                    .map(|(i, c)| (i, c.destination.clone(), c.payload_kg, c.payment))
                    .collect();
                for (i, dest, payload_kg, payment) in candidates {
                    if payload_kg > self.capability_to(game, &dest) * PAYLOAD_MARGIN {
                        continue;
                    }
                    if best.is_none_or(|(_, p)| payment > p) {
                        best = Some((i, payment));
                    }
                }
                let Some((avail_index, _)) = best else {
                    // No liftable contract. If reputation has gone
                    // negative (a failure after our first success),
                    // every market is gated shut — fly test missions
                    // to rebuild reputation rather than waiting forever.
                    if game.player_company.reputation.total() < 0.0
                        && !Self::flight_in_transit(game)
                    {
                        if let Ok((dest, payloads)) =
                            game.build_launch_payloads(&[], &[])
                        {
                            game.launch_rocket(rocket_item_id, &dest, payloads, false);
                        }
                    }
                    return;
                };
                if game.accept_contract(avail_index).is_none() {
                    return;
                }
                game.player_company.active_contracts.len() - 1
            }
        };

        let destination = game.player_company.active_contracts[active_index]
            .destination.clone();
        let Ok((dest, payloads)) = game.build_launch_payloads(&[active_index], &[])
        else {
            return;
        };
        debug_assert_eq!(dest, destination);
        game.launch_rocket(rocket_item_id, &dest, payloads, false);
    }
}

impl Default for BasicPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl CompanyPolicy for BasicPolicy {
    fn act(&mut self, game: &mut GameState) {
        self.ensure_teams(game);
        self.ensure_engine_projects(game);
        Self::revise_discovered_flaws(game);
        self.assign_idle_engineers(game);
        self.maybe_design_rocket(game);
        self.maybe_enable_auto_build(game);
        self.ensure_bid_rules(game);
        self.accept_and_launch(game);
    }

    fn name(&self) -> &'static str {
        "basic"
    }
}

/// Look up a policy by CLI name. `basic:<margin>` selects BasicPolicy
/// with a specific standing-rule markup (e.g. `basic:0.15`) — the
/// margin-sweep entry point.
pub fn policy_by_name(name: &str) -> Option<Box<dyn CompanyPolicy>> {
    if let Some(margin) = name.strip_prefix("basic:") {
        let margin: f64 = margin.parse().ok()?;
        if !(0.0..=10.0).contains(&margin) {
            return None;
        }
        return Some(Box::new(BasicPolicy::with_margin(margin)));
    }
    match name {
        "none" => Some(Box::new(NullPolicy)),
        "basic" => Some(Box::new(BasicPolicy::new())),
        _ => None,
    }
}

/// Names accepted by `policy_by_name`, for CLI help text.
pub const POLICY_NAMES: &[&str] = &["none", "basic", "basic:<margin>"];

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seed: u64, days: u32) -> (GameState, BasicPolicy) {
        let mut policy = BasicPolicy::new();
        let mut gs = GameState::new("BotCorp".into(), 200_000_000.0, seed);
        for _ in 0..days {
            policy.act(&mut gs);
            gs.advance_day();
        }
        (gs, policy)
    }

    #[test]
    fn test_basic_policy_reaches_rocket_testing() {
        // Within ~2 years the bot should have both engines designed and
        // the rocket template through design into Testing.
        let (gs, policy) = run(42, 730);
        assert!(policy.rocket.is_some(), "rocket project should exist");
        let rp = gs.player_company.rocket_projects.iter()
            .find(|p| Some(p.project_id) == policy.rocket)
            .expect("rocket project present");
        assert!(
            matches!(rp.status, RocketDesignStatus::Testing { .. }),
            "rocket should reach Testing within 2 years, got {:?}", rp.status,
        );
        assert!(policy.auto_build_set, "auto-build should be enabled");
    }

    #[test]
    fn test_basic_policy_template_lifts_smallsats_to_leo() {
        let (gs, mut policy) = run(42, 730);
        let cap = policy.capability_to(&gs, "leo");
        assert!(cap >= 500.0,
            "template should lift at least 500 kg to LEO, got {cap:.0}");
    }

    #[test]
    fn test_basic_policy_launches_within_four_years() {
        let (gs, _) = run(42, 1460);
        assert!(!gs.player_company.launch_history.is_empty(),
            "bot should have launched at least once in 4 years");
    }

    #[test]
    fn test_basic_policy_is_deterministic() {
        let (a, _) = run(7, 1095);
        let (b, _) = run(7, 1095);
        // Compare parsed values: HashMap key order in the JSON text is
        // not canonical, but the underlying state must be identical.
        let va = serde_json::to_value(&a).unwrap();
        let vb = serde_json::to_value(&b).unwrap();
        assert_eq!(va, vb, "same seed + policy must give identical state");
    }
}
